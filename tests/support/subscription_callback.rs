// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
// Pure dispatcher state tests: std channels/threads, no Node sockets or native IO.
use super::*;
use std::sync::mpsc::RecvTimeoutError;

fn sender(capacity: usize) -> Sender {
    Sender {
        tx: mpsc::sync_channel(capacity).0,
        response_tx: mpsc::sync_channel(capacity).0,
        stop: Arc::new(AtomicBool::new(false)),
        slots: Arc::new(AtomicUsize::new(0)),
        callback_threads: Arc::new(AtomicUsize::new(0)),
        callback_requests: Arc::new(AtomicUsize::new(0)),
        capacity,
        counters: Arc::new(Counters::default()),
        identity: Arc::new(OnceLock::new()),
        publication_sequences: None,
    }
}
fn admit<F>(
    control: &Arc<SubscriptionDispatcher>,
    retained: Retained,
    callback: F,
) -> (CallbackSubscription, MessageDestination)
where
    F: FnMut(&ReceivedMessage) -> Result<(), Error> + Send + 'static,
{
    let (tx, rx) = mpsc::sync_channel(2);
    let active = Arc::new(AtomicBool::new(true));
    let slots = Arc::new(AtomicUsize::new(1));
    let interest = Arc::new(Mutex::new(LocalInterestCache::default()));
    let done = Arc::new(Completion::default());
    let entry = Entry {
        subscription: Some(Subscription {
            rx,
            _lease: Lease {
                active: active.clone(),
                slots: slots.clone(),
            },
            owner_dead: control.owner_dead.clone(),
            local_interest: interest.clone(),
        }),
        capture: Capture {
            callback: Some(Box::new(callback)),
            counters: control.sender.counters.clone(),
        },
        retained: Some(retained),
        done: done.clone(),
    };
    assert!(control.tx.try_send(entry).is_ok());
    control.notify();
    (
        CallbackSubscription {
            active,
            slots,
            interest,
            done,
            control: control.clone(),
        },
        MessageDestination::new(tx, Some(Arc::downgrade(&control.wake))),
    )
}
struct ReleaseGate(Arc<(Mutex<bool>, Condvar)>);
impl Drop for ReleaseGate {
    fn drop(&mut self) {
        *self.0.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.0.1.notify_all();
    }
}
fn message() -> ReceivedMessage {
    ReceivedMessage {
        message: Message {
            topic: "@/pure@/foo".into(),
            type_name: "gz.msgs.Int32".into(),
            data: vec![5],
        },
        intra_process: true,
    }
}

#[test]
fn many_callbacks_share_one_dispatcher_and_last_drop_joins_it() {
    let sender = sender(8);
    let (control, first) = SubscriptionDispatcher::new(
        sender.clone(),
        Arc::new(AtomicBool::new(false)),
        spawn_callback,
    )
    .unwrap();
    let mut first = Some(first);
    let (completed, received) = mpsc::channel();
    let mut handles = Vec::new();
    let mut destinations = Vec::new();
    for i in 0..8 {
        let permit = if i == 0 {
            first.take().unwrap()
        } else {
            control.reserve().unwrap().unwrap()
        };
        let completed = completed.clone();
        let (handle, destination) = admit(&control, permit, move |message| {
            assert_eq!(message.data, [5]);
            completed.send(thread::current().id()).unwrap();
            Ok(())
        });
        handles.push(handle);
        destination.try_send(message()).unwrap();
        destinations.push(destination);
    }
    let ids = (0..8)
        .map(|_| received.recv_timeout(Duration::from_secs(1)).unwrap())
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), 1);
    assert!(!ids.contains(&thread::current().id()));
    assert_eq!(sender.callback_threads.load(Ordering::Acquire), 1);
    assert!(matches!(control.reserve(), Err(Error::QueueFull)));
    drop(handles);
    assert!(control.terminal());
    assert_eq!(sender.callback_threads.load(Ordering::Acquire), 0);
    assert!(control.reserve().unwrap().is_none());
}

#[test]
fn invocation_and_capture_panics_are_isolated_from_other_callbacks() {
    struct Fault;
    impl Drop for Fault {
        fn drop(&mut self) {
            panic!("intentional capture fault");
        }
    }
    let sender = sender(2);
    let (control, first) = SubscriptionDispatcher::new(
        sender.clone(),
        Arc::new(AtomicBool::new(false)),
        spawn_callback,
    )
    .unwrap();
    let fault = Fault;
    let (event, observed) = mpsc::channel();
    let mut calls = 0;
    let (bad, input) = admit(&control, first, move |_| {
        std::hint::black_box(&fault);
        calls += 1;
        event.send(calls).unwrap();
        match calls {
            1 => Err(Error::InvalidFrame),
            2 => panic!("intentional invocation fault"),
            _ => Ok(()),
        }
    });
    let (event, observed_good) = mpsc::channel();
    let (good, good_input) = admit(&control, control.reserve().unwrap().unwrap(), move |_| {
        event.send(()).unwrap();
        Ok(())
    });
    for i in 1..=3 {
        input.try_send(message()).unwrap();
        assert_eq!(observed.recv_timeout(Duration::from_secs(1)).unwrap(), i);
    }
    drop(bad);
    assert_eq!(sender.counters.snapshot().routing_errors, 3);
    good_input.try_send(message()).unwrap();
    observed_good.recv_timeout(Duration::from_secs(1)).unwrap();
    drop(good);
    assert_eq!(sender.callback_threads.load(Ordering::Acquire), 0);
}

#[test]
fn blocked_capture_retains_registration_and_thread_bounds_through_real_exit() {
    struct Block {
        entered: SyncSender<()>,
        gate: Arc<(Mutex<bool>, Condvar)>,
        control: Arc<SubscriptionDispatcher>,
    }
    impl Drop for Block {
        fn drop(&mut self) {
            assert!(matches!(self.control.reserve(), Err(Error::QueueFull)));
            self.entered.send(()).unwrap();
            let mut release = self.gate.0.lock().unwrap();
            while !*release {
                release = self.gate.1.wait(release).unwrap();
            }
        }
    }
    let sender = sender(1);
    let (control, first) = SubscriptionDispatcher::new(
        sender.clone(),
        Arc::new(AtomicBool::new(false)),
        spawn_callback,
    )
    .unwrap();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (entered, observed) = mpsc::sync_channel(1);
    let blocked = Block {
        entered,
        gate: gate.clone(),
        control: control.clone(),
    };
    let (handle, _destination) = admit(&control, first, move |_| {
        std::hint::black_box(&blocked);
        Ok(())
    });
    let _release_on_unwind = ReleaseGate(gate.clone());
    handle.cancel();
    observed.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(
        sender.callback_permit(&sender.callback_threads),
        Err(Error::QueueFull)
    ));
    let (done, finished) = mpsc::channel();
    let dropping = thread::spawn(move || {
        drop(handle);
        done.send(()).unwrap();
    });
    assert_eq!(
        finished.recv_timeout(Duration::from_millis(20)),
        Err(RecvTimeoutError::Timeout)
    );
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    finished.recv_timeout(Duration::from_secs(1)).unwrap();
    dropping.join().unwrap();
    assert_eq!(sender.callback_threads.load(Ordering::Acquire), 0);
}

fn fake_node(capacity: usize) -> (Node, Receiver<Command>) {
    let config = Config {
        capacity,
        partition: format!("pure-callback-{}", Uuid::new_v4()),
        ..Default::default()
    };
    let (commands, received) = mpsc::sync_channel(capacity);
    let mut sender = sender(capacity);
    sender.tx = commands;
    let services = Arc::new(Mutex::new(Vec::new()));
    assert!(
        sender
            .identity
            .set(WorkerIdentity {
                process: Uuid::new_v4().to_string(),
                node: Uuid::new_v4().to_string(),
                address: "tcp://127.0.0.1:1".into(),
                response_address: "tcp://127.0.0.1:2".into(),
                response_id: "pure".into(),
                service_domain: MessageDomain::from_service_config(&config),
                services: services.clone(),
                stopped: Arc::downgrade(&sender.stop),
            })
            .is_ok()
    );
    (
        Node {
            lifetime: Arc::new(WorkerLifetime {
                stop: sender.stop.clone(),
                worker: Mutex::new(None),
            }),
            sender,
            owner_dead: Arc::new(AtomicBool::new(false)),
            local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
            config,
            topics: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(Vec::new()),
            services,
            subscription_dispatcher: Mutex::new(Weak::new()),
        },
        received,
    )
}
fn fail_spawn(_task: CallbackTask) -> std::io::Result<JoinHandle<()>> {
    Err(std::io::Error::other("controlled spawn failure"))
}
fn disconnected_spawn(task: CallbackTask) -> std::io::Result<JoinHandle<()>> {
    drop(task);
    Ok(thread::spawn(|| {}))
}

#[test]
fn typed_api_validation_spawn_and_queue_failure_roll_back_without_io() {
    use gz_msgs::msgs::Int32;
    let (node, commands) = fake_node(1);
    assert!(matches!(
        node.subscribe_typed_callback::<Int32, _>("invalid topic", |_, _| Ok(())),
        Err(Error::InvalidName)
    ));
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    assert!(matches!(
        node.subscribe_typed_callback_with_spawner::<Int32, _>(
            "/foo",
            SubscribeOptions::default(),
            |_, _| Ok(()),
            fail_spawn
        ),
        Err(Error::Native(_))
    ));
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(matches!(
        node.subscribe_typed_callback_with_spawner::<Int32, _>(
            "/foo",
            SubscribeOptions::default(),
            |_, _| Ok(()),
            disconnected_spawn
        ),
        Err(Error::Shutdown)
    ));
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(node.subscribed_topics().unwrap().is_empty());
    // Discard the simulated worker's already admitted inactive subscription.
    assert!(matches!(commands.try_recv(), Ok(Command::Subscribe(..))));
    let _held = node.reserve().unwrap();
    assert!(matches!(
        node.subscribe_typed_callback::<Int32, _>("/foo", |_, _| Ok(())),
        Err(Error::QueueFull)
    ));
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
}

#[test]
fn typed_decode_and_owner_retirement_use_coalesced_destination_wakes_without_io() {
    use gz_msgs::{msgs::Int32, prost::Message as _};
    let (node, commands) = fake_node(2);
    let (done, observed) = mpsc::channel();
    let callback = node
        .subscribe_typed_callback::<Int32, _>("/foo", move |value, info| {
            assert_eq!(value.data, 5);
            assert_eq!(info.topic, "/foo");
            done.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    let (topic, kind, destination) = match commands.recv_timeout(Duration::from_secs(1)).unwrap() {
        Command::Subscribe(topic, kind, destination, ..) => (topic, kind, destination),
        _ => panic!("wrong simulated worker command"),
    };
    destination
        .try_send(ReceivedMessage {
            message: Message {
                topic,
                type_name: kind,
                data: Int32 {
                    data: 5,
                    ..Default::default()
                }
                .encode_to_vec(),
            },
            intra_process: true,
        })
        .unwrap();
    observed.recv_timeout(Duration::from_secs(1)).unwrap();
    drop(node);
    assert_eq!(
        observed.recv_timeout(Duration::from_secs(1)),
        Err(RecvTimeoutError::Disconnected)
    );
    drop(callback);
    // A retired weak notifier cannot keep a dispatcher alive.
    assert!(destination.try_send(message()).is_err());
    let (raw, received) = mpsc::sync_channel(1);
    let plain = MessageDestination::from(raw);
    plain.try_send(message()).unwrap();
    assert_eq!(received.recv().unwrap().data, [5]);
}

#[test]
fn panic_payload_destructors_cannot_poison_shared_dispatcher_progress() {
    struct Payload;
    impl Drop for Payload {
        fn drop(&mut self) {
            panic!("secondary panic payload drop");
        }
    }
    struct CaptureFault;
    impl Drop for CaptureFault {
        fn drop(&mut self) {
            std::panic::panic_any(Payload);
        }
    }
    let sender = sender(2);
    let (control, first) = SubscriptionDispatcher::new(
        sender.clone(),
        Arc::new(AtomicBool::new(false)),
        spawn_callback,
    )
    .unwrap();
    let fault = CaptureFault;
    let (entered, observed) = mpsc::channel();
    let (bad, input) = admit(&control, first, move |_| {
        std::hint::black_box(&fault);
        entered.send(()).unwrap();
        std::panic::panic_any(Payload)
    });
    let (done, completed) = mpsc::channel();
    let (good, next) = admit(&control, control.reserve().unwrap().unwrap(), move |_| {
        done.send(()).unwrap();
        Ok(())
    });
    input.try_send(message()).unwrap();
    observed.recv_timeout(Duration::from_secs(1)).unwrap();
    drop(bad);
    assert_eq!(sender.counters.snapshot().routing_errors, 2);
    next.try_send(message()).unwrap();
    completed.recv_timeout(Duration::from_secs(1)).unwrap();
    drop(good);
    assert_eq!(sender.callback_threads.load(Ordering::Acquire), 0);
}

#[test]
fn concurrent_last_retirement_and_registration_never_lose_admitted_callbacks() {
    let (node, _commands) = fake_node(8);
    let node = Arc::new(node);
    let start = Arc::new(std::sync::Barrier::new(8));
    let admitted = Arc::new(AtomicUsize::new(0));
    let completed = Arc::new(AtomicUsize::new(0));
    thread::scope(|scope| {
        let mut threads = Vec::new();
        for _ in 0..8 {
            let node = node.clone();
            let start = start.clone();
            let admitted = admitted.clone();
            let completed = completed.clone();
            threads.push(scope.spawn(move || {
                start.wait();
                for _ in 0..8 {
                    let (control, retained) =
                        match node.subscription_callback_dispatcher(spawn_callback) {
                            Ok(admission) => admission,
                            Err(Error::QueueFull) => continue,
                            Err(error) => panic!("unexpected admission error: {error}"),
                        };
                    admitted.fetch_add(1, Ordering::AcqRel);
                    let completed = completed.clone();
                    let (done, observed) = mpsc::channel();
                    let (handle, destination) = admit(&control, retained, move |_| {
                        completed.fetch_add(1, Ordering::AcqRel);
                        done.send(()).unwrap();
                        Ok(())
                    });
                    destination.try_send(message()).unwrap();
                    observed.recv_timeout(Duration::from_secs(1)).unwrap();
                    drop(handle);
                }
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
    });
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    assert!(admitted.load(Ordering::Acquire) > 0);
    assert_eq!(
        admitted.load(Ordering::Acquire),
        completed.load(Ordering::Acquire)
    );
}

#[test]
fn disconnected_data_producer_retires_capture_without_handle_cancellation() {
    struct End(mpsc::Sender<()>);
    impl Drop for End {
        fn drop(&mut self) {
            self.0.send(()).unwrap();
        }
    }
    let sender = sender(1);
    let (control, retained) = SubscriptionDispatcher::new(
        sender.clone(),
        Arc::new(AtomicBool::new(false)),
        spawn_callback,
    )
    .unwrap();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let callback_gate = gate.clone();
    let (entered, waiting) = mpsc::channel();
    let (disposed, observed) = mpsc::channel();
    let end = End(disposed);
    let (handle, destination) = admit(&control, retained, move |_| {
        std::hint::black_box(&end);
        entered.send(()).unwrap();
        let mut release = callback_gate.0.lock().unwrap();
        while !*release {
            release = callback_gate.1.wait(release).unwrap();
        }
        Ok(())
    });
    let release = ReleaseGate(gate);
    destination.try_send(message()).unwrap();
    waiting.recv_timeout(Duration::from_secs(1)).unwrap();
    drop(destination);
    drop(release);
    observed.recv_timeout(Duration::from_secs(1)).unwrap();
    drop(handle);
    assert_eq!(sender.callback_threads.load(Ordering::Acquire), 0);
}

#[test]
fn unavailable_control_queue_rolls_back_capture_and_endpoint_admission() {
    use gz_msgs::msgs::Int32;
    struct End(Arc<AtomicUsize>);
    impl Drop for End {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }
    let (node, _commands) = fake_node(1);
    // Controlled handoff fault: a zero-capacity, nonreceiving control queue.
    // Normal construction uses capacity-bound geometry and retained admission.
    let (tx, _unavailable) = mpsc::sync_channel(0);
    let (wake, _events) = mpsc::sync_channel(1);
    let control = Arc::new(SubscriptionDispatcher {
        tx,
        wake: Arc::new(wake),
        admission: Mutex::new(Admission {
            retained: 0,
            closed: false,
        }),
        worker: Mutex::new(None),
        thread: OnceLock::new(),
        sender: node.sender.clone(),
        owner_dead: node.owner_dead.clone(),
    });
    *node.subscription_dispatcher.lock().unwrap() = Arc::downgrade(&control);
    let disposed = Arc::new(AtomicUsize::new(0));
    let end = End(disposed.clone());
    assert!(matches!(
        node.subscribe_typed_callback::<Int32, _>("/foo", move |_, _| {
            std::hint::black_box(&end);
            panic!("failed handoff invoked user callback")
        }),
        Err(Error::QueueFull)
    ));
    assert_eq!(disposed.load(Ordering::Acquire), 1);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.counters.snapshot().queue_drops, 1);
    assert!(node.subscribed_topics().unwrap().is_empty());
    assert!(control.terminal());
}

#[test]
fn per_handle_unsubscribe_has_one_concurrent_winner_and_releases_one_lease() {
    let sender = sender(8);
    let (control, retained) = SubscriptionDispatcher::new(
        sender.clone(),
        Arc::new(AtomicBool::new(false)),
        spawn_callback,
    )
    .unwrap();
    let (handle, destination) = admit(&control, retained, |_| Ok(()));
    let handle = Arc::new(handle);
    assert!(handle.is_active());
    let winners = thread::scope(|scope| {
        (0..8)
            .map(|_| scope.spawn(|| handle.unsubscribe()))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|task| task.join().unwrap())
            .filter(|won| *won)
            .count()
    });
    assert_eq!(winners, 1);
    assert!(!handle.is_active());
    assert_eq!(handle.slots.load(Ordering::Acquire), 0);
    assert!(!handle.unsubscribe());
    handle.cancel();
    assert_eq!(handle.slots.load(Ordering::Acquire), 0);
    drop(handle);
    drop(destination);
    assert_eq!(sender.callback_threads.load(Ordering::Acquire), 0);
}

#[test]
fn unsubscribed_selected_callback_retains_entry_budget_until_real_exit() {
    let sender = sender(1);
    let (control, retained) = SubscriptionDispatcher::new(
        sender.clone(),
        Arc::new(AtomicBool::new(false)),
        spawn_callback,
    )
    .unwrap();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let release_gate = ReleaseGate(gate.clone());
    let callback_gate = gate.clone();
    let (entered, entry) = mpsc::channel();
    let (handle, destination) = admit(&control, retained, move |_| {
        entered.send(()).unwrap();
        let (lock, ready) = &*callback_gate;
        let mut released = lock.lock().unwrap();
        while !*released {
            released = ready.wait(released).unwrap();
        }
        Ok(())
    });
    destination.try_send(message()).unwrap();
    entry.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(handle.unsubscribe());
    assert!(!handle.is_active());
    assert_eq!(handle.slots.load(Ordering::Acquire), 0);
    assert!(matches!(control.reserve(), Err(Error::QueueFull)));
    assert_eq!(sender.callback_threads.load(Ordering::Acquire), 1);
    assert!(!handle.unsubscribe());
    drop(release_gate);
    drop(handle);
    assert_eq!(sender.callback_threads.load(Ordering::Acquire), 0);
}

#[test]
fn self_unsubscribe_is_nonblocking_and_suppresses_queued_work() {
    let sender = sender(1);
    let (control, retained) = SubscriptionDispatcher::new(
        sender.clone(),
        Arc::new(AtomicBool::new(false)),
        spawn_callback,
    )
    .unwrap();
    let held = Arc::new(Mutex::new(None::<Weak<CallbackSubscription>>));
    let callback_handle = held.clone();
    let (completed, completion) = mpsc::channel();
    let entries = Arc::new(AtomicUsize::new(0));
    let callback_entries = entries.clone();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let release_gate = ReleaseGate(gate.clone());
    let callback_gate = gate.clone();
    let (entered, entry) = mpsc::channel();
    let (handle, destination) = admit(&control, retained, move |_| {
        callback_entries.fetch_add(1, Ordering::AcqRel);
        entered.send(()).unwrap();
        let (lock, ready) = &*callback_gate;
        let mut released = lock.lock().unwrap();
        while !*released {
            released = ready.wait(released).unwrap();
        }
        drop(released);
        let handle = callback_handle
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .upgrade()
            .unwrap();
        let retired = handle.unsubscribe();
        completed.send(retired).unwrap();
        Ok(())
    });
    let handle = Arc::new(handle);
    *held.lock().unwrap() = Some(Arc::downgrade(&handle));
    destination.try_send(message()).unwrap();
    entry.recv_timeout(Duration::from_secs(1)).unwrap();
    destination.try_send(message()).unwrap();
    drop(release_gate);
    assert!(completion.recv_timeout(Duration::from_secs(1)).unwrap());
    assert!(!handle.is_active());
    assert!(!handle.unsubscribe());
    drop(handle);
    assert_eq!(entries.load(Ordering::Acquire), 1);
    assert_eq!(sender.callback_threads.load(Ordering::Acquire), 0);
}

#[test]
fn active_snapshot_observes_owner_stop_and_existing_cancel_is_idempotent() {
    for owner_retirement in [false, true] {
        let sender = sender(1);
        let owner_dead = Arc::new(AtomicBool::new(false));
        let (control, retained) =
            SubscriptionDispatcher::new(sender.clone(), owner_dead.clone(), spawn_callback)
                .unwrap();
        let (handle, destination) = admit(&control, retained, |_| Ok(()));
        assert!(handle.is_active());
        if owner_retirement {
            owner_dead.store(true, Ordering::Release);
        } else {
            sender.stop.store(true, Ordering::Release);
        }
        assert!(!handle.is_active());
        control.notify();
        handle.cancel();
        handle.cancel();
        assert!(!handle.unsubscribe());
        assert_eq!(handle.slots.load(Ordering::Acquire), 0);
        drop(handle);
        drop(destination);
        assert_eq!(sender.callback_threads.load(Ordering::Acquire), 0);
    }
}

#[test]
fn stopped_owner_can_hide_liveness_before_retained_lease_retirement() {
    let sender = sender(1);
    let owner_dead = Arc::new(AtomicBool::new(false));
    let (control, retained) =
        SubscriptionDispatcher::new(sender.clone(), owner_dead.clone(), spawn_callback).unwrap();
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let release_gate = ReleaseGate(gate.clone());
    let callback_gate = gate.clone();
    let (entered, entry) = mpsc::channel();
    let (handle, destination) = admit(&control, retained, move |_| {
        entered.send(()).unwrap();
        let mut released = callback_gate.0.lock().unwrap();
        while !*released {
            released = callback_gate.1.wait(released).unwrap();
        }
        Ok(())
    });
    destination.try_send(message()).unwrap();
    entry.recv_timeout(Duration::from_secs(1)).unwrap();
    // The blocked invocation prevents dispatcher retirement from racing this
    // observation. Liveness includes owner state; retirement reports the lease.
    owner_dead.store(true, Ordering::Release);
    assert!(!handle.is_active());
    assert!(handle.unsubscribe());
    assert!(!handle.unsubscribe());
    assert_eq!(handle.slots.load(Ordering::Acquire), 0);
    assert!(matches!(control.reserve(), Err(Error::QueueFull)));
    assert_eq!(sender.callback_threads.load(Ordering::Acquire), 1);
    drop(release_gate);
    drop(handle);
    assert_eq!(sender.callback_threads.load(Ordering::Acquire), 0);
}

#[test]
fn raw_callback_preserves_opaque_bytes_pointer_and_actual_metadata_without_io() {
    let (node, commands) = fake_node(2);
    let partition = format!("/{}", node.config.partition);
    let (done, observed) = mpsc::channel();
    let mut count = 0;
    let callback = node
        .subscribe_raw_callback("/foo", "", move |data, info| {
            count += 1;
            done.send((
                count,
                data.to_vec(),
                data.as_ptr() as usize,
                info.topic.to_owned(),
                info.partition.to_owned(),
                info.type_name.to_owned(),
                info.intra_process,
            ))
            .unwrap();
            Ok(())
        })
        .unwrap();
    let (topic, destination) = match commands.recv_timeout(Duration::from_secs(1)).unwrap() {
        Command::Subscribe(topic, hint, destination, ..) => {
            assert!(hint.is_empty());
            (topic, destination)
        }
        _ => panic!("wrong simulated worker command"),
    };
    let payloads = [
        vec![],
        vec![0xff],
        (0..=255).collect(),
        vec![0x10, 5, 0xf8, 7, 1],
    ];
    for (i, data) in payloads.into_iter().enumerate() {
        let expected = data.clone();
        let address = data.as_ptr() as usize;
        let local = i % 2 == 0;
        destination
            .try_send(ReceivedMessage {
                message: Message {
                    topic: topic.clone(),
                    type_name: "opaque.future.Type".into(),
                    data,
                },
                intra_process: local,
            })
            .unwrap();
        let actual = observed.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(
            actual,
            (
                i + 1,
                expected,
                address,
                "/foo".into(),
                partition.clone(),
                "opaque.future.Type".into(),
                local
            )
        );
    }
    drop(callback);
    assert_eq!(node.sender.counters.snapshot().routing_errors, 0);
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
}

#[test]
fn raw_and_typed_callbacks_share_existing_dispatcher_and_option_admission() {
    use gz_msgs::{msgs::Int32, prost::Message as _};
    let (node, commands) = fake_node(2);
    let (done, observed) = mpsc::channel();
    let typed_done = done.clone();
    let typed = node
        .subscribe_typed_callback::<Int32, _>("/foo", move |value, _| {
            assert_eq!(value.data, 5);
            typed_done.send(thread::current().id()).unwrap();
            Ok(())
        })
        .unwrap();
    let options = SubscribeOptions {
        ignore_local_messages: true,
        msgs_per_sec: 7,
    };
    let raw = node
        .subscribe_raw_callback_with_options("/foo", "opaque.Type", options, move |data, _| {
            assert_eq!(data, [0xff]);
            done.send(thread::current().id()).unwrap();
            Ok(())
        })
        .unwrap();
    assert!(Arc::ptr_eq(&typed.control, &raw.control));
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 1);
    for is_raw in [false, true] {
        let (topic, kind, destination) =
            match commands.recv_timeout(Duration::from_secs(1)).unwrap() {
                Command::Subscribe(topic, kind, destination, _, actual_options, _) => {
                    if is_raw {
                        assert_eq!(actual_options, options);
                        assert_eq!(kind, "opaque.Type");
                    } else {
                        assert_eq!(kind, "gz.msgs.Int32");
                    }
                    (topic, kind, destination)
                }
                _ => panic!("wrong simulated worker command"),
            };
        let data = if is_raw {
            vec![0xff]
        } else {
            Int32 {
                data: 5,
                ..Default::default()
            }
            .encode_to_vec()
        };
        destination
            .try_send(ReceivedMessage {
                message: Message {
                    topic,
                    type_name: kind,
                    data,
                },
                intra_process: false,
            })
            .unwrap();
    }
    let first = observed.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(
        observed.recv_timeout(Duration::from_secs(1)).unwrap(),
        first
    );
    drop(typed);
    drop(raw);
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
}

#[test]
fn raw_api_validation_and_spawn_failure_use_existing_rollback_without_io() {
    let (node, commands) = fake_node(1);
    assert!(matches!(
        node.subscribe_raw_callback("invalid topic", "", |_, _| Ok(())),
        Err(Error::InvalidName)
    ));
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    assert!(matches!(
        node.subscribe_raw_callback_with_spawner(
            "/foo",
            "",
            SubscribeOptions::default(),
            |_, _| Ok(()),
            fail_spawn
        ),
        Err(Error::Native(_))
    ));
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(matches!(
        node.subscribe_raw_callback_with_spawner(
            "/foo",
            "",
            SubscribeOptions::default(),
            |_, _| Ok(()),
            disconnected_spawn
        ),
        Err(Error::Shutdown)
    ));
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
    assert!(node.subscribed_topics().unwrap().is_empty());
    assert!(matches!(commands.try_recv(), Ok(Command::Subscribe(..))));
    let _held = node.reserve().unwrap();
    assert!(matches!(
        node.subscribe_raw_callback("/foo", "", |_, _| Ok(())),
        Err(Error::QueueFull)
    ));
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
}

#[test]
fn raw_callback_errors_panics_and_capture_drop_preserve_other_registration_progress() {
    struct DropFault;
    impl Drop for DropFault {
        fn drop(&mut self) {
            panic!("controlled raw capture drop");
        }
    }
    let (node, commands) = fake_node(2);
    let (entered, observed) = mpsc::channel();
    let fault = DropFault;
    let bad = node
        .subscribe_raw_callback("/foo", "", move |data, _| {
            std::hint::black_box(&fault);
            entered.send(()).unwrap();
            if data == [1] {
                Err(Error::InvalidFrame)
            } else {
                panic!("controlled raw callback panic");
            }
        })
        .unwrap();
    let (done, completion) = mpsc::channel();
    let good = node
        .subscribe_raw_callback("/foo", "", move |data, _| {
            assert_eq!(data, [0xff]);
            done.send(()).unwrap();
            Ok(())
        })
        .unwrap();
    let mut inputs = Vec::new();
    for _ in 0..2 {
        match commands.recv_timeout(Duration::from_secs(1)).unwrap() {
            Command::Subscribe(topic, _, destination, ..) => inputs.push((topic, destination)),
            _ => panic!("wrong simulated worker command"),
        }
    }
    for value in [1, 2] {
        inputs[0]
            .1
            .try_send(ReceivedMessage {
                message: Message {
                    topic: inputs[0].0.clone(),
                    type_name: "opaque.Type".into(),
                    data: vec![value],
                },
                intra_process: true,
            })
            .unwrap();
        observed.recv_timeout(Duration::from_secs(1)).unwrap();
    }
    drop(bad);
    assert_eq!(node.sender.counters.snapshot().routing_errors, 3);
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 1);
    inputs[1]
        .1
        .try_send(ReceivedMessage {
            message: Message {
                topic: inputs[1].0.clone(),
                type_name: "opaque.Type".into(),
                data: vec![0xff],
            },
            intra_process: true,
        })
        .unwrap();
    completion.recv_timeout(Duration::from_secs(1)).unwrap();
    drop(good);
    assert_eq!(node.sender.callback_threads.load(Ordering::Acquire), 0);
    assert_eq!(node.sender.slots.load(Ordering::Acquire), 0);
}
