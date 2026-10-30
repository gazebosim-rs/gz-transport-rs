// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::Condvar;

type Callback = dyn FnMut(&ReceivedMessage) -> Result<(), Error> + Send;

#[derive(Clone)]
pub(super) struct MessageDestination {
    tx: SyncSender<ReceivedMessage>,
    wake: Option<Weak<SyncSender<()>>>,
}
impl MessageDestination {
    pub(super) fn new(tx: SyncSender<ReceivedMessage>, wake: Option<Weak<SyncSender<()>>>) -> Self {
        Self { tx, wake }
    }
    pub(super) fn try_send(
        &self,
        value: ReceivedMessage,
    ) -> Result<(), TrySendError<ReceivedMessage>> {
        self.tx.try_send(value)?;
        if let Some(wake) = self.wake.as_ref().and_then(Weak::upgrade) {
            let _ = wake.try_send(());
        }
        Ok(())
    }
}
impl From<SyncSender<ReceivedMessage>> for MessageDestination {
    fn from(tx: SyncSender<ReceivedMessage>) -> Self {
        Self::new(tx, None)
    }
}

struct Admission {
    retained: usize,
    closed: bool,
}
pub(super) struct SubscriptionDispatcher {
    tx: SyncSender<Entry>,
    wake: Arc<SyncSender<()>>,
    admission: Mutex<Admission>,
    worker: Mutex<Option<JoinHandle<()>>>,
    thread: OnceLock<thread::ThreadId>,
    sender: Sender,
    owner_dead: Arc<AtomicBool>,
}
impl SubscriptionDispatcher {
    fn new(
        sender: Sender,
        owner_dead: Arc<AtomicBool>,
        spawn: CallbackSpawner,
    ) -> Result<(Arc<Self>, Retained), Error> {
        let thread_permit = sender.callback_permit(&sender.callback_threads)?;
        let (tx, rx) = mpsc::sync_channel(sender.capacity);
        let (wake, events) = mpsc::sync_channel(1);
        let control = Arc::new(Self {
            tx,
            wake: Arc::new(wake),
            admission: Mutex::new(Admission {
                retained: 1,
                closed: false,
            }),
            worker: Mutex::new(None),
            thread: OnceLock::new(),
            sender,
            owner_dead,
        });
        let retained = Retained(control.clone());
        let dispatch = control.clone();
        let worker = spawn(Box::new(move || {
            let _thread_permit = thread_permit;
            dispatch.run(rx, events);
        }))?;
        let _ = control.thread.set(worker.thread().id());
        *control.worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(worker);
        Ok((control, retained))
    }
    fn reserve(self: &Arc<Self>) -> Result<Option<Retained>, Error> {
        let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        if admission.closed {
            return Ok(None);
        }
        if admission.retained >= self.sender.capacity {
            self.sender
                .counters
                .queue_drops
                .fetch_add(1, Ordering::Relaxed);
            return Err(Error::QueueFull);
        }
        admission.retained += 1;
        Ok(Some(Retained(self.clone())))
    }
    pub(super) fn notify(&self) {
        let _ = self.wake.try_send(());
    }
    fn terminal(&self) -> bool {
        let admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        admission.closed && admission.retained == 0
    }
    fn on_dispatcher(&self) -> bool {
        self.thread
            .get()
            .is_some_and(|id| *id == thread::current().id())
    }
    fn join_if_terminal(&self) {
        if self.terminal() && !self.on_dispatcher() {
            let worker = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take();
            join_callback(worker);
        }
    }
    fn run(&self, commands: Receiver<Entry>, events: Receiver<()>) {
        let mut entries = Vec::<Entry>::new();
        let mut cursor = 0;
        loop {
            // Retained-entry admission also bounds queued and in-flight Adds.
            for _ in 0..self.sender.capacity {
                match commands.try_recv() {
                    Ok(entry) => entries.push(entry),
                    Err(_) => break,
                }
            }
            if self.owner_dead.load(Ordering::Acquire) || self.sender.stop.load(Ordering::Acquire) {
                self.admission
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .closed = true;
            }
            let mut index = 0;
            while index < entries.len() {
                if !entries[index].live() {
                    // User capture destruction is outside every registry lock.
                    drop(entries.remove(index));
                } else {
                    index += 1;
                }
            }
            if self.terminal() {
                break;
            }
            let mut progress = false;
            for offset in 0..entries.len() {
                let index = (cursor + offset) % entries.len();
                match entries[index].subscription.as_ref().unwrap().rx.try_recv() {
                    Ok(message) => {
                        if entries[index].live() {
                            entries[index].capture.invoke(&message);
                        }
                        cursor = (index + 1) % entries.len();
                        progress = true;
                        break;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        drop(entries.remove(index));
                        progress = true;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            if !progress {
                // Successful data enqueue, cancellation and admission coalesce
                // wakeups. An active but idle callback needs no polling.
                let _ = events.recv();
            }
        }
        // Zero retained entries means commands/entries own no user captures.
    }
}
struct Retained(Arc<SubscriptionDispatcher>);
impl Drop for Retained {
    fn drop(&mut self) {
        {
            let mut state = self.0.admission.lock().unwrap_or_else(|e| e.into_inner());
            state.retained -= 1;
            if state.retained == 0 {
                state.closed = true;
            }
        }
        self.0.notify();
    }
}
#[derive(Default)]
struct Completion {
    done: Mutex<bool>,
    ready: Condvar,
}
impl Completion {
    fn finish(&self) {
        *self.done.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.ready.notify_all();
    }
    fn wait(&self) {
        let mut done = self.done.lock().unwrap_or_else(|e| e.into_inner());
        while !*done {
            done = self.ready.wait(done).unwrap_or_else(|e| e.into_inner());
        }
    }
}
struct Capture {
    callback: Option<Box<Callback>>,
    counters: Arc<Counters>,
}
// A panic payload can itself run user Drop code. Dispose it under the same
// unwind boundary, without retaining a history or releasing admissions early.
fn contain_unwind<T>(task: impl FnOnce() -> T) -> Option<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(task)) {
        Ok(value) => Some(value),
        Err(payload) => {
            let mut pending = Some(payload);
            while let Some(payload) = pending.take() {
                pending =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(payload))).err();
            }
            None
        }
    }
}
impl Capture {
    fn invoke(&mut self, message: &ReceivedMessage) {
        let result = contain_unwind(|| self.callback.as_mut().unwrap()(message));
        if !matches!(result, Some(Ok(()))) {
            self.counters.routing_errors.fetch_add(1, Ordering::Relaxed);
        }
    }
    fn dispose(&mut self) {
        let callback = self.callback.take();
        if contain_unwind(|| drop(callback)).is_none() {
            self.counters.routing_errors.fetch_add(1, Ordering::Relaxed);
        }
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        self.dispose();
    }
}
struct Entry {
    subscription: Option<Subscription>,
    capture: Capture,
    retained: Option<Retained>,
    done: Arc<Completion>,
}
impl Entry {
    fn live(&self) -> bool {
        let subscription = self.subscription.as_ref().unwrap();
        subscription._lease.active.load(Ordering::Acquire)
            && !subscription.owner_dead.load(Ordering::Acquire)
            && !self
                .retained
                .as_ref()
                .unwrap()
                .0
                .sender
                .stop
                .load(Ordering::Acquire)
    }
}
impl Drop for Entry {
    fn drop(&mut self) {
        // Capture Drop catches unwind. Its completion precedes both admissions
        // being released, including when the destructor reenters this Node.
        self.capture.dispose();
        drop(self.subscription.take());
        drop(self.retained.take());
        self.done.finish();
    }
}
/// Owned asynchronous typed callback registration. Cancel suppresses queued
/// work. Drop waits for this callback and its captures, except on the dispatcher;
/// an already selected invocation may finish. User code cannot be preempted.
pub struct CallbackSubscription {
    active: Arc<AtomicBool>,
    slots: Arc<AtomicUsize>,
    interest: Arc<Mutex<LocalInterestCache>>,
    control: Arc<SubscriptionDispatcher>,
    done: Arc<Completion>,
}
impl CallbackSubscription {
    /// Snapshot whether this lease is active and its Node has not stopped.
    /// This is live Rust registration state, not native stored-ID validity or
    /// a guarantee that a callback will run or has finished.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
            && !self.control.owner_dead.load(Ordering::Acquire)
            && !self.control.sender.stop.load(Ordering::Acquire)
    }
    /// Retire this lease without waiting for selected user code or capture Drop.
    /// Exactly one concurrent retirement of an active lease returns true.
    /// Owner stop can make is_active false before lease retirement, so this
    /// may still return true. It is not the inverse of the liveness query.
    /// Queued unselected callbacks are suppressed; Drop waits for real exit.
    pub fn unsubscribe(&self) -> bool {
        let retired = release(&self.active, &self.slots);
        self.interest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .refresh();
        self.control.notify();
        retired
    }
    pub fn cancel(&self) {
        let _ = self.unsubscribe();
    }
}
impl Drop for CallbackSubscription {
    fn drop(&mut self) {
        self.cancel();
        if !self.control.on_dispatcher() {
            self.done.wait();
        }
        self.control.join_if_terminal();
    }
}
impl Node {
    fn subscription_callback_dispatcher(
        &self,
        spawn: CallbackSpawner,
    ) -> Result<(Arc<SubscriptionDispatcher>, Retained), Error> {
        if self.sender.stop.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        let mut slot = self
            .subscription_dispatcher
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(control) = slot.upgrade()
            && let Some(retained) = control.reserve()?
        {
            return Ok((control, retained));
        }
        // The slot is weak. Worker creation transfers only control and permits;
        // user captures are queued after this guard has been released.
        let (control, retained) =
            SubscriptionDispatcher::new(self.sender.clone(), self.owner_dead.clone(), spawn)?;
        *slot = Arc::downgrade(&control);
        Ok((control, retained))
    }
    pub(super) fn wake_subscription_callbacks(&self) {
        let slot = self
            .subscription_dispatcher
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(control) = slot.upgrade() {
            control.notify();
        }
    }
    /// Decode canonical messages before invoking serialized asynchronous callbacks.
    /// One dispatcher is created on demand per Node and retired after its last
    /// capture exits. Invocation/capture-drop panics are counted and isolated.
    pub fn subscribe_typed_callback<M, F>(
        &self,
        topic: &str,
        callback: F,
    ) -> Result<CallbackSubscription, Error>
    where
        M: gz_msgs::CanonicalMessage,
        F: for<'a> FnMut(&M, MessageInfo<'a>) -> Result<(), Error> + Send + 'static,
    {
        self.subscribe_typed_callback_with_options::<M, F>(
            topic,
            SubscribeOptions::default(),
            callback,
        )
    }
    pub fn subscribe_typed_callback_with_options<M, F>(
        &self,
        topic: &str,
        options: SubscribeOptions,
        callback: F,
    ) -> Result<CallbackSubscription, Error>
    where
        M: gz_msgs::CanonicalMessage,
        F: for<'a> FnMut(&M, MessageInfo<'a>) -> Result<(), Error> + Send + 'static,
    {
        self.subscribe_typed_callback_with_spawner::<M, F>(topic, options, callback, spawn_callback)
    }
    fn subscribe_typed_callback_with_spawner<M, F>(
        &self,
        topic: &str,
        options: SubscribeOptions,
        mut callback: F,
        spawn: CallbackSpawner,
    ) -> Result<CallbackSubscription, Error>
    where
        M: gz_msgs::CanonicalMessage,
        F: for<'a> FnMut(&M, MessageInfo<'a>) -> Result<(), Error> + Send + 'static,
    {
        let capture = Capture {
            callback: Some(Box::new(move |message| {
                let value = M::decode(message.data.as_slice()).map_err(|_| Error::InvalidFrame)?;
                callback(&value, message.info())
            })),
            counters: self.sender.counters.clone(),
        };
        self.register_subscription_callback(topic, M::PROTOBUF_NAME, options, capture, spawn)
    }
    /// Invoke an asynchronous callback with the admitted opaque bytes unchanged.
    /// An empty type hint matches every publication, as with subscribe().
    /// This shares the typed dispatcher; local PublishRaw is not inline here.
    pub fn subscribe_raw_callback<F>(
        &self,
        topic: &str,
        type_name: &str,
        callback: F,
    ) -> Result<CallbackSubscription, Error>
    where
        F: for<'a> FnMut(&'a [u8], MessageInfo<'a>) -> Result<(), Error> + Send + 'static,
    {
        self.subscribe_raw_callback_with_options(
            topic,
            type_name,
            SubscribeOptions::default(),
            callback,
        )
    }
    pub fn subscribe_raw_callback_with_options<F>(
        &self,
        topic: &str,
        type_name: &str,
        options: SubscribeOptions,
        callback: F,
    ) -> Result<CallbackSubscription, Error>
    where
        F: for<'a> FnMut(&'a [u8], MessageInfo<'a>) -> Result<(), Error> + Send + 'static,
    {
        self.subscribe_raw_callback_with_spawner(
            topic,
            type_name,
            options,
            callback,
            spawn_callback,
        )
    }
    fn subscribe_raw_callback_with_spawner<F>(
        &self,
        topic: &str,
        type_name: &str,
        options: SubscribeOptions,
        mut callback: F,
        spawn: CallbackSpawner,
    ) -> Result<CallbackSubscription, Error>
    where
        F: for<'a> FnMut(&'a [u8], MessageInfo<'a>) -> Result<(), Error> + Send + 'static,
    {
        let capture = Capture {
            callback: Some(Box::new(move |message| {
                callback(message.data.as_slice(), message.info())
            })),
            counters: self.sender.counters.clone(),
        };
        self.register_subscription_callback(topic, type_name, options, capture, spawn)
    }
    fn register_subscription_callback(
        &self,
        topic: &str,
        type_name: &str,
        options: SubscribeOptions,
        capture: Capture,
        spawn: CallbackSpawner,
    ) -> Result<CallbackSubscription, Error> {
        self.remapped_topic(topic)?;
        let (control, retained) = self.subscription_callback_dispatcher(spawn)?;
        let subscription = match self.subscribe_destination(
            topic,
            type_name,
            options,
            Some(Arc::downgrade(&control.wake)),
        ) {
            Ok(subscription) => subscription,
            Err(error) => {
                drop(capture);
                drop(retained);
                control.join_if_terminal();
                return Err(error);
            }
        };
        let handle = CallbackSubscription {
            active: subscription._lease.active.clone(),
            slots: subscription._lease.slots.clone(),
            interest: subscription.local_interest.clone(),
            control: control.clone(),
            done: Arc::new(Completion::default()),
        };
        let entry = Entry {
            subscription: Some(subscription),
            capture,
            retained: Some(retained),
            done: handle.done.clone(),
        };
        if let Err(error) = control.tx.try_send(entry) {
            let error = match error {
                TrySendError::Full(entry) => {
                    self.sender
                        .counters
                        .queue_drops
                        .fetch_add(1, Ordering::Relaxed);
                    drop(entry);
                    Error::QueueFull
                }
                TrySendError::Disconnected(entry) => {
                    drop(entry);
                    Error::Shutdown
                }
            };
            control.join_if_terminal();
            return Err(error);
        }
        control.notify();
        Ok(handle)
    }
}

#[cfg(test)]
#[path = "../tests/support/subscription_callback.rs"]
mod tests;
