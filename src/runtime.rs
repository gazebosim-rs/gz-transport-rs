// Copyright (C) 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use crate::{Config, Error, decompose_fully_qualified_topic, protocol, qualified_name};
use gz_msgs::discovery::{
    Discovery,
    discovery::{
        DiscContents, Publisher as WirePublisher, Subscriber, Type,
        publisher::{MessagePublisher, PubType, Scope, ServicePublisher},
    },
};
use socket2::{Domain, Protocol, Socket, Type as SocketType};
use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use uuid::Uuid;

/// Local application loss/error counters. Native PUB high-water-mark loss is not observable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Statistics {
    pub queue_drops: u64,
    pub malformed_frames: u64,
    pub send_failures: u64,
    pub reply_timeouts: u64,
}
#[derive(Default)]
struct Counters {
    queue_drops: AtomicU64,
    malformed_frames: AtomicU64,
    send_failures: AtomicU64,
    reply_timeouts: AtomicU64,
}
impl Counters {
    fn snapshot(&self) -> Statistics {
        Statistics {
            queue_drops: self.queue_drops.load(Ordering::Relaxed),
            malformed_frames: self.malformed_frames.load(Ordering::Relaxed),
            send_failures: self.send_failures.load(Ordering::Relaxed),
            reply_timeouts: self.reply_timeouts.load(Ordering::Relaxed),
        }
    }
    fn sent<E>(&self, result: Result<(), E>) -> bool {
        if result.is_err() {
            self.send_failures.fetch_add(1, Ordering::Relaxed);
            false
        } else {
            true
        }
    }
    fn dropped<T>(&self, result: Result<(), TrySendError<T>>) {
        if result.is_err() {
            self.queue_drops.fetch_add(1, Ordering::Relaxed);
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub topic: String,
    pub type_name: String,
    pub data: Vec<u8>,
}
impl Message {
    /// Topic path without its partition, as exposed by native MessageInfo::Topic.
    pub fn topic_path(&self) -> &str {
        self.topic.rsplit('@').next().unwrap_or(&self.topic)
    }
}
/// Borrowed metadata for an owned received message.
/// `intra_process` records direct delivery within this Node's worker. Independent
/// Rust Nodes still use the wire, even in one OS process; native process-global
/// sharing and its broader origin semantics remain an explicit compatibility gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageInfo<'a> {
    pub topic: &'a str,
    pub partition: &'a str,
    pub type_name: &'a str,
    pub intra_process: bool,
}
/// An unchanged raw message plus its captured delivery path. Dereferences to the
/// raw value; moving it out preserves every payload byte without reencoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedMessage {
    message: Message,
    intra_process: bool,
}
impl ReceivedMessage {
    pub fn into_message(self) -> Message {
        self.message
    }
    pub fn info(&self) -> MessageInfo<'_> {
        // Delivered topics match a validated subscription's qualified name.
        let partition = self
            .message
            .topic
            .strip_prefix('@')
            .and_then(|topic| topic.split_once('@'))
            .map_or("", |(partition, _)| partition);
        MessageInfo {
            topic: self.message.topic_path(),
            partition,
            type_name: &self.message.type_name,
            intra_process: self.intra_process,
        }
    }
}
impl std::ops::Deref for ReceivedMessage {
    type Target = Message;
    fn deref(&self) -> &Message {
        &self.message
    }
}
/// Canonical protobuf names advertised for one service signature.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServiceTypes {
    pub request_type: String,
    pub response_type: String,
}
/// Descriptor names from one bounded topic endpoint snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicEndpointTypes {
    pub publishers: Vec<String>,
    pub subscribers: Vec<String>,
}
/// Provider selection outcomes, separate from queue/runtime failures in [`Error`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceTypeResolution {
    Resolved(ServiceTypes),
    NoProviders,
    IncompatibleTypes,
    AmbiguousTypes,
}
#[derive(Clone)]
struct Sender {
    tx: SyncSender<Command>,
    stop: Arc<AtomicBool>,
    slots: Arc<AtomicUsize>,
    capacity: usize,
    counters: Arc<Counters>,
}
impl Sender {
    fn send(&self, cmd: Command) -> Result<(), Error> {
        if self.stop.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        self.tx.try_send(cmd).map_err(|e| match e {
            TrySendError::Full(_) => {
                self.counters.queue_drops.fetch_add(1, Ordering::Relaxed);
                Error::QueueFull
            }
            TrySendError::Disconnected(_) => Error::Shutdown,
        })
    }
}
struct Lease {
    active: Arc<AtomicBool>,
    slots: Arc<AtomicUsize>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        release(&self.active, &self.slots);
    }
}
fn release(active: &AtomicBool, slots: &AtomicUsize) {
    if active.swap(false, Ordering::AcqRel) {
        slots.fetch_sub(1, Ordering::AcqRel);
    }
}
pub struct Publisher {
    _lease: Lease,
    sender: Sender,
    topic: String,
    type_name: String,
    max: usize,
    reply_timeout: Duration,
}
impl Publisher {
    /// Snapshot known matching subscriber registrations. As in native Gazebo,
    /// membership does not acknowledge authentication or successful delivery,
    /// and process-only disconnects do not erase remembered registrations.
    /// Worker coordination is bounded by `Config::reply_timeout`.
    pub fn has_connections(&self) -> Result<bool, Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender.send(Command::HasConnections(
            self.topic.clone(),
            self.type_name.clone(),
            tx,
        ))?;
        rx.recv_timeout(self.reply_timeout).map_err(receive_error)
    }
    /// Encode a canonical value after checking its descriptor and size.
    /// Typed reencoding does not preserve unknown fields from original wire bytes.
    pub fn publish_typed<M: gz_msgs::CanonicalMessage>(&self, value: &M) -> Result<(), Error> {
        if self.type_name != M::PROTOBUF_NAME || value.encoded_len() > self.max {
            return Err(Error::InvalidFrame);
        }
        self.publish(value.encode_to_vec())
    }
    pub fn publish(&self, data: impl Into<Vec<u8>>) -> Result<(), Error> {
        let data = data.into();
        if data.len() > self.max {
            return Err(Error::InvalidFrame);
        }
        self.sender.send(Command::Publish(
            self.topic.clone(),
            self.type_name.clone(),
            data,
        ))
    }
}
pub struct Subscription {
    rx: Receiver<ReceivedMessage>,
    _lease: Lease,
}
impl Subscription {
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Message, Error> {
        self.recv_with_info_timeout(timeout)
            .map(ReceivedMessage::into_message)
    }
    /// Receive owned bytes with a borrowed metadata view, using the same bounded
    /// queue, timeout and shutdown behavior as the raw receive method.
    pub fn recv_with_info_timeout(&self, timeout: Duration) -> Result<ReceivedMessage, Error> {
        self.rx.recv_timeout(timeout).map_err(receive_error)
    }
}
pub struct Service {
    rx: Receiver<Request>,
    _lease: Lease,
}
impl Service {
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Request, Error> {
        self.rx.recv_timeout(timeout).map_err(receive_error)
    }
}
pub struct Request {
    pub data: Vec<u8>,
    sender: Sender,
    route: Vec<Vec<u8>>,
    max: usize,
}
impl Request {
    /// Gazebo services with response type `gz.msgs.Empty` never send a reply.
    pub fn expects_response(&self) -> bool {
        !self.route.is_empty()
    }
    /// Complete a normal request, or consume a one-way request without sending.
    pub fn respond(self, data: impl Into<Vec<u8>>, success: bool) -> Result<(), Error> {
        if !self.expects_response() {
            return Ok(());
        }
        let data = data.into();
        if data.len() > self.max {
            return Err(Error::InvalidFrame);
        }
        self.sender
            .send(Command::Respond(self.route, data, success))
    }
}
fn receive_error(e: mpsc::RecvTimeoutError) -> Error {
    match e {
        mpsc::RecvTimeoutError::Timeout => Error::Timeout,
        mpsc::RecvTimeoutError::Disconnected => Error::Shutdown,
    }
}
fn active_topics<'a>(
    registrations: impl Iterator<Item = (&'a String, &'a Weak<AtomicBool>)>,
) -> Vec<String> {
    let mut topics: Vec<_> = registrations
        .filter(|(_, active)| active.upgrade().is_some_and(|a| a.load(Ordering::Acquire)))
        .map(|(topic, _)| topic.rsplit('@').next().unwrap_or(topic).to_owned())
        .collect();
    topics.sort();
    topics.dedup();
    topics
}

/// Dropping the node cancels its worker and joins it. Handles never own sockets.
pub struct Node {
    sender: Sender,
    worker: Option<JoinHandle<()>>,
    config: Config,
    topics: Mutex<HashMap<String, Weak<AtomicBool>>>,
    subscriptions: Mutex<Vec<(String, Weak<AtomicBool>)>>,
    services: Mutex<Vec<(String, Weak<AtomicBool>)>>,
}
impl Node {
    pub fn new(config: Config) -> Result<Self, Error> {
        config.validate()?;
        let (tx, rx) = mpsc::sync_channel(config.capacity);
        let sender = Sender {
            tx,
            stop: Arc::new(AtomicBool::new(false)),
            slots: Arc::new(AtomicUsize::new(0)),
            capacity: config.capacity,
            counters: Arc::new(Counters::default()),
        };
        let state = Worker::new(config.clone(), sender.clone())?;
        let stop = sender.stop.clone();
        let worker = thread::Builder::new()
            .name("gz-transport-io".into())
            .spawn(move || state.run(rx, stop))?;
        Ok(Self {
            sender,
            worker: Some(worker),
            config,
            topics: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(Vec::new()),
            services: Mutex::new(Vec::new()),
        })
    }
    fn topic(&self, topic: &str) -> Result<String, Error> {
        qualified_name(&self.config.partition, &self.config.namespace, topic)
    }
    // Native inspection and explicit service withdrawal use the original name;
    // only registration, subscription and requests apply the configured map.
    fn remapped_topic(&self, topic: &str) -> Result<String, Error> {
        self.topic(self.config.topic_remap(topic).unwrap_or(topic))
    }
    fn reserve(&self) -> Result<Lease, Error> {
        self.sender
            .slots
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < self.sender.capacity).then_some(n + 1)
            })
            .map_err(|_| Error::QueueFull)?;
        Ok(Lease {
            active: Arc::new(AtomicBool::new(true)),
            slots: self.sender.slots.clone(),
        })
    }
    /// Reserve one publisher per qualified topic on this Node, as native Gazebo does.
    /// Dropping it immediately releases ownership; worker withdrawal precedes recreation.
    /// Advertise the pinned canonical descriptor for a generated message type.
    pub fn advertise_typed<M: gz_msgs::CanonicalMessage>(
        &self,
        topic: &str,
    ) -> Result<Publisher, Error> {
        self.advertise(topic, M::PROTOBUF_NAME)
    }
    pub fn advertise(&self, topic: &str, type_name: &str) -> Result<Publisher, Error> {
        let topic = self.remapped_topic(topic)?;
        let mut topics = self
            .topics
            .lock()
            .map_err(|_| Error::Native("publisher registry poisoned".into()))?;
        topics.retain(|_, active| active.upgrade().is_some_and(|a| a.load(Ordering::Acquire)));
        if topics.contains_key(&topic) {
            return Err(Error::AlreadyAdvertised);
        }
        let lease = self.reserve()?;
        self.sender.send(Command::Advertise(
            topic.clone(),
            type_name.into(),
            lease.active.clone(),
        ))?;
        topics.insert(topic.clone(), Arc::downgrade(&lease.active));
        Ok(Publisher {
            _lease: lease,
            sender: self.sender.clone(),
            topic,
            type_name: type_name.into(),
            max: self.config.max_message_bytes,
            reply_timeout: self.config.reply_timeout,
        })
    }
    pub fn subscribe(&self, topic: &str, type_name: &str) -> Result<Subscription, Error> {
        let (tx, rx) = mpsc::sync_channel(self.config.capacity);
        let lease = self.reserve()?;
        let topic = self.remapped_topic(topic)?;
        let mut subscriptions = self
            .subscriptions
            .lock()
            .map_err(|_| Error::Native("subscription registry poisoned".into()))?;
        subscriptions.retain(|(_, a)| a.upgrade().is_some_and(|a| a.load(Ordering::Acquire)));
        self.sender.send(Command::Subscribe(
            topic.clone(),
            type_name.into(),
            tx,
            lease.active.clone(),
        ))?;
        subscriptions.push((topic, Arc::downgrade(&lease.active)));
        Ok(Subscription { rx, _lease: lease })
    }
    /// This Node's live publisher topics, sorted without partition prefixes.
    /// Local inspection performs no worker command or network operation.
    pub fn advertised_topics(&self) -> Result<Vec<String>, Error> {
        if self.sender.stop.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        let topics = self
            .topics
            .lock()
            .map_err(|_| Error::Native("publisher registry poisoned".into()))?;
        Ok(active_topics(topics.iter()))
    }
    /// Sorted topic paths advertised locally or by discovered message peers.
    /// This worker snapshot does not include subscriber-only topics or wait for
    /// remote discovery. `advertised_topics` remains local handle inspection.
    pub fn known_publisher_topics(&self) -> Result<Vec<String>, Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender.send(Command::KnownPublisherTopics(tx))?;
        rx.recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)
    }
    /// Sorted paths with a known publisher or subscription interest.
    /// A nonblocking subscriber query refreshes remote interests eventually;
    /// this snapshot does not wait for replies or alter publisher-only lists.
    pub fn known_topics(&self) -> Result<Vec<String>, Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender.send(Command::KnownTopics(tx))?;
        rx.recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)
    }
    /// Snapshot message descriptor names for known endpoints on an original topic.
    /// Equal names from distinct endpoints remain separate. The worker requests
    /// subscriber interests without waiting for remote replies.
    pub fn known_topic_info(&self, topic: &str) -> Result<TopicEndpointTypes, Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .send(Command::KnownTopicInfo(self.topic(topic)?, tx))?;
        rx.recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)
    }
    /// This Node's live subscription interests, including topics with no publisher.
    /// Duplicate receivers share one sorted entry; dropping the last removes it.
    pub fn subscribed_topics(&self) -> Result<Vec<String>, Error> {
        if self.sender.stop.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        let subscriptions = self
            .subscriptions
            .lock()
            .map_err(|_| Error::Native("subscription registry poisoned".into()))?;
        Ok(active_topics(subscriptions.iter().map(|(t, a)| (t, a))))
    }
    /// Retire this Node's subscriptions to an original qualified topic.
    /// Valid absent topics also succeed. Already admitted receiver messages remain
    /// owned by their queues; already-selected worker deliveries may complete. Filter
    /// cleanup and remote withdrawal run asynchronously on the existing worker.
    /// A saturated command queue leaves local leases unchanged.
    pub fn unsubscribe(&self, topic: &str) -> Result<(), Error> {
        let topic = self.topic(topic)?;
        if self.sender.stop.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        let mut subscriptions = self
            .subscriptions
            .lock()
            .map_err(|_| Error::Native("subscription registry poisoned".into()))?;
        self.sender.send(Command::Unsubscribe(topic.clone()))?;
        subscriptions.retain(|(name, active)| {
            if name == &topic {
                if let Some(active) = active.upgrade() {
                    release(&active, &self.sender.slots);
                }
                false
            } else {
                true
            }
        });
        Ok(())
    }
    /// Register one service per qualified topic on this Node. A rejected duplicate
    /// leaves the original receiver authoritative, including for different types.
    pub fn serve(
        &self,
        topic: &str,
        request_type: &str,
        response_type: &str,
    ) -> Result<Service, Error> {
        let topic = self.remapped_topic(topic)?;
        let mut services = self
            .services
            .lock()
            .map_err(|_| Error::Native("service registry poisoned".into()))?;
        services.retain(|(_, active)| active.upgrade().is_some_and(|a| a.load(Ordering::Acquire)));
        if services.iter().any(|(name, _)| name == &topic) {
            return Err(Error::AlreadyAdvertised);
        }
        let (tx, rx) = mpsc::sync_channel(self.config.capacity);
        let lease = self.reserve()?;
        self.sender.send(Command::Serve(
            topic.clone(),
            request_type.into(),
            response_type.into(),
            tx,
            lease.active.clone(),
        ))?;
        services.push((topic, Arc::downgrade(&lease.active)));
        Ok(Service { rx, _lease: lease })
    }
    /// This Node's live service topics, without partition prefixes, sorted uniquely.
    pub fn advertised_services(&self) -> Result<Vec<String>, Error> {
        if self.sender.stop.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        let services = self
            .services
            .lock()
            .map_err(|_| Error::Native("service registry poisoned".into()))?;
        Ok(active_topics(services.iter().map(|(t, a)| (t, a))))
    }
    /// Sorted service paths advertised locally or by discovered service peers.
    /// This worker snapshot does not wait for remote discovery.
    /// `advertised_services` remains local handle inspection.
    pub fn known_services(&self) -> Result<Vec<String>, Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender.send(Command::KnownServices(tx))?;
        rx.recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)
    }
    /// Withdraw all handlers for a local service. Valid absent topics also succeed.
    /// Already admitted requests may still be drained and completed.
    pub fn unadvertise_service(&self, topic: &str) -> Result<(), Error> {
        let topic = self.topic(topic)?;
        if self.sender.stop.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        let mut services = self
            .services
            .lock()
            .map_err(|_| Error::Native("service registry poisoned".into()))?;
        services.retain(|(name, active)| {
            if name == &topic {
                if let Some(active) = active.upgrade() {
                    release(&active, &self.sender.slots);
                }
                false
            } else {
                true
            }
        });
        Ok(())
    }
    /// Hand a one-way request to a matching local queue or connected native peer.
    /// The timeout bounds discovery and send retries; success does not acknowledge
    /// execution by the service. No response is sent or awaited on the wire.
    pub fn request_oneway(
        &self,
        topic: &str,
        request_type: &str,
        data: impl Into<Vec<u8>>,
        timeout: Duration,
    ) -> Result<(), Error> {
        self.request(topic, request_type, EMPTY_TYPE, data, timeout)
            .map(|_| ())
    }
    /// Request a response. `gz.msgs.Empty` selects Gazebo's one-way semantics.
    pub fn request(
        &self,
        topic: &str,
        request_type: &str,
        response_type: &str,
        data: impl Into<Vec<u8>>,
        timeout: Duration,
    ) -> Result<(Vec<u8>, bool), Error> {
        let data = data.into();
        if data.len() > self.config.max_message_bytes {
            return Err(Error::InvalidFrame);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender.send(Command::Request(Pending {
            topic: self.remapped_topic(topic)?,
            request_type: request_type.into(),
            response_type: response_type.into(),
            data,
            deadline: Instant::now()
                .checked_add(timeout)
                .ok_or(Error::InvalidConfig)?,
            reply: tx,
            sent: false,
        }))?;
        rx.recv_timeout(timeout).map_err(receive_error)?
    }
    /// Snapshot sorted, unique signatures known to this Node's worker.
    /// Refreshes discovery without invoking a handler. Newly advertised remote
    /// providers can arrive after this snapshot; this is not a global registry.
    pub fn service_type_signatures(&self, topic: &str) -> Result<Vec<ServiceTypes>, Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .send(Command::ServiceSignatures(self.topic(topic)?, tx))?;
        rx.recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)
    }
    /// One type entry per known provider record, sorted but not deduplicated.
    /// Equal signatures retain provider multiplicity. This bounded worker snapshot
    /// uses original-name qualification and the same discovery/coordination limits
    /// as `service_type_signatures`; it exposes no addresses or process-global view.
    pub fn service_provider_types(&self, topic: &str) -> Result<Vec<ServiceTypes>, Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .send(Command::ServiceProviderTypes(self.topic(topic)?, tx))?;
        rx.recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)
    }
    /// Resolve empty descriptor names from matching providers. Explicit names
    /// select providers; both explicit names return immediately without lookup.
    /// The provider wait is polled every 100ms on the caller, never the IO worker.
    /// Each snapshot additionally has the bounded `Config::reply_timeout` for
    /// worker coordination. Invalid names and runtime failures return `Error`.
    pub fn resolve_service_types(
        &self,
        topic: &str,
        request_type: &str,
        response_type: &str,
        timeout: Duration,
    ) -> Result<ServiceTypeResolution, Error> {
        use ServiceTypeResolution::*;
        if !request_type.is_empty() && !response_type.is_empty() {
            return Ok(Resolved(ServiceTypes {
                request_type: request_type.into(),
                response_type: response_type.into(),
            }));
        }
        // Validate before creating a deadline, preserving the native bypass.
        self.topic(topic)?;
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(Error::InvalidConfig)?;
        loop {
            let signatures = self.service_type_signatures(topic)?;
            let mut compatible = signatures.iter().filter(|types| {
                (request_type.is_empty() || types.request_type == request_type)
                    && (response_type.is_empty() || types.response_type == response_type)
            });
            if let Some(first) = compatible.next() {
                return Ok(if compatible.next().is_some() {
                    AmbiguousTypes
                } else {
                    Resolved(first.clone())
                });
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(if signatures.is_empty() {
                    NoProviders
                } else {
                    IncompatibleTypes
                });
            }
            thread::sleep((deadline - now).min(Duration::from_millis(100)));
        }
    }
    pub fn statistics(&self) -> Statistics {
        self.sender.counters.snapshot()
    }
    pub fn shutdown(&mut self) {
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
    /// Cancel all pending operations; the owner joins the worker on shutdown/drop.
    pub fn cancel(&self) {
        self.sender.stop.store(true, Ordering::Release);
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        self.shutdown();
    }
}

enum Command {
    Advertise(String, String, Arc<AtomicBool>),
    KnownPublisherTopics(SyncSender<Vec<String>>),
    KnownTopics(SyncSender<Vec<String>>),
    KnownTopicInfo(String, SyncSender<TopicEndpointTypes>),
    Publish(String, String, Vec<u8>),
    Subscribe(String, String, SyncSender<ReceivedMessage>, Arc<AtomicBool>),
    Unsubscribe(String),
    Serve(String, String, String, SyncSender<Request>, Arc<AtomicBool>),
    KnownServices(SyncSender<Vec<String>>),
    ServiceSignatures(String, SyncSender<Vec<ServiceTypes>>),
    ServiceProviderTypes(String, SyncSender<Vec<ServiceTypes>>),
    HasConnections(String, String, SyncSender<bool>),
    Request(Pending),
    Respond(Vec<Vec<u8>>, Vec<u8>, bool),
}
const EMPTY_TYPE: &str = "gz.msgs.Empty";

struct Pending {
    topic: String,
    request_type: String,
    response_type: String,
    data: Vec<u8>,
    deadline: Instant,
    reply: SyncSender<Result<(Vec<u8>, bool), Error>>,
    sent: bool,
}
struct SubscriberState {
    topic: String,
    type_name: String,
    tx: SyncSender<ReceivedMessage>,
    active: Arc<AtomicBool>,
}
struct ServiceState {
    publisher: WirePublisher,
    tx: SyncSender<Request>,
    active: Arc<AtomicBool>,
}
#[derive(Hash, PartialEq, Eq)]
struct PeerKey {
    channel: usize,
    process: String,
    node: String,
    topic: String,
    address: String,
}
#[derive(Hash, PartialEq, Eq)]
struct SubscriberInterestKey {
    process: String,
    node: String,
    topic: String,
    address: String,
    type_name: String,
}
struct Worker {
    config: Config,
    host_interfaces: Vec<Ipv4Addr>,
    sender: Sender,
    process: String,
    node: String,
    udp: [UdpSocket; 2],
    publisher: zmq::Socket,
    subscriber: zmq::Socket,
    router: zmq::Socket,
    requester: zmq::Socket,
    responses: zmq::Socket,
    address: String,
    service_address: String,
    response_address: String,
    router_id: String,
    response_id: String,
    advertisements: Vec<(WirePublisher, Arc<AtomicBool>)>,
    subscriptions: Vec<SubscriberState>,
    services: Vec<ServiceState>,
    peers: HashMap<PeerKey, (WirePublisher, Instant)>,
    remote_subscribers: HashMap<PeerKey, WirePublisher>,
    subscriber_interests: HashMap<SubscriberInterestKey, Instant>,
    connected: HashSet<String>,
    subscriptions_connected: HashSet<String>,
    response_connections: HashMap<String, Instant>,
    pending: HashMap<String, Pending>,
    replies: Vec<(Instant, Vec<Vec<u8>>)>,
}
fn udp(config: &Config, port: u16) -> Result<UdpSocket, Error> {
    let socket = Socket::new(Domain::IPV4, SocketType::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())?;
    socket.join_multicast_v4(&config.multicast, &config.interface)?;
    socket.set_multicast_if_v4(&config.interface)?;
    socket.set_multicast_loop_v4(true)?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}
fn socket(
    context: &zmq::Context,
    kind: zmq::SocketType,
    config: &Config,
) -> Result<zmq::Socket, Error> {
    let socket = context.socket(kind)?;
    socket.set_linger(0)?;
    socket.set_sndhwm(config.send_high_watermark as i32)?;
    socket.set_rcvhwm(config.receive_high_watermark as i32)?;
    socket.set_maxmsgsize(config.max_message_bytes as i64)?;
    Ok(socket)
}
fn bind(socket: &zmq::Socket, config: &Config) -> Result<String, Error> {
    socket.bind(&format!("tcp://{}:*", config.interface))?;
    socket.get_last_endpoint()?.map_err(|_| Error::InvalidFrame)
}
// Keep message boundaries by draining malformed multipart input. Cancellation
// abandons the socket's remaining frames only when its owner is tearing it down.
fn receive(
    socket: &zmq::Socket,
    expected: usize,
    counters: &Counters,
    mut cancelled: impl FnMut() -> bool,
) -> Option<Vec<Vec<u8>>> {
    let mut frames = Vec::with_capacity(expected);
    let mut count = 0;
    loop {
        if cancelled() {
            return None;
        }
        let frame = socket.recv_msg(zmq::DONTWAIT).ok()?;
        count += 1;
        if count <= expected {
            frames.push(frame.to_vec());
        }
        if !frame.get_more() {
            if count != expected {
                counters.malformed_frames.fetch_add(1, Ordering::Relaxed);
            }
            return (count == expected).then_some(frames);
        }
    }
}
#[cfg(unix)]
struct LocalInterface {
    index: Option<u32>,
    address: IpAddr,
    multicast: bool,
    up: bool,
}

#[cfg(unix)]
fn eligible_interfaces(records: Vec<LocalInterface>) -> Vec<Ipv4Addr> {
    let mut seen = HashSet::new();
    let mut addresses = Vec::new();
    for record in records {
        let Some(index) = record.index else { continue };
        // Native deduplicates interface indices before family/flag filtering.
        if !seen.insert(index) {
            continue;
        }
        if let IpAddr::V4(ip) = record.address
            && ip != Ipv4Addr::LOCALHOST
            && record.multicast
            && record.up
            && !addresses.contains(&ip)
        {
            addresses.push(ip);
        }
    }
    if addresses.is_empty() {
        addresses.push(Ipv4Addr::LOCALHOST);
    }
    addresses
}

#[cfg(unix)]
fn local_interfaces() -> Result<Vec<Ipv4Addr>, Error> {
    use nix::{
        ifaddrs::getifaddrs,
        net::if_::{InterfaceFlags, if_nametoindex},
    };
    let mut records = Vec::new();
    for interface in getifaddrs().map_err(|error| Error::Native(error.to_string()))? {
        let address = interface.address.and_then(|address| {
            address
                .as_sockaddr_in()
                .map(|v4| IpAddr::V4(v4.ip()))
                .or_else(|| address.as_sockaddr_in6().map(|v6| IpAddr::V6(v6.ip())))
        });
        let Some(address) = address else { continue };
        if interface.interface_name.contains('\u{fffd}') {
            return Err(Error::Unsupported("ambiguous interface name encoding"));
        }
        records.push(LocalInterface {
            index: if_nametoindex(interface.interface_name.as_str()).ok(),
            address,
            multicast: interface.flags.contains(InterfaceFlags::IFF_MULTICAST),
            up: interface.flags.contains(InterfaceFlags::IFF_UP),
        });
    }
    Ok(eligible_interfaces(records))
}

impl Worker {
    fn new(config: Config, sender: Sender) -> Result<Self, Error> {
        #[cfg(unix)]
        let host_interfaces = local_interfaces()?;
        #[cfg(not(unix))]
        let host_interfaces = vec![config.interface];
        let context = zmq::Context::new();
        let publisher = socket(&context, zmq::PUB, &config)?;
        let subscriber = socket(&context, zmq::SUB, &config)?;
        let router = socket(&context, zmq::ROUTER, &config)?;
        let requester = socket(&context, zmq::ROUTER, &config)?;
        requester.set_router_mandatory(true)?;
        let responses = socket(&context, zmq::ROUTER, &config)?;
        let router_id = Uuid::new_v4().to_string();
        let response_id = Uuid::new_v4().to_string();
        router.set_identity(router_id.as_bytes())?;
        router.set_router_mandatory(true)?;
        responses.set_identity(response_id.as_bytes())?;
        let address = bind(&publisher, &config)?;
        let service_address = bind(&router, &config)?;
        let response_address = bind(&responses, &config)?;
        let udp = [
            udp(&config, config.message_port)?,
            udp(&config, config.service_port)?,
        ];
        Ok(Self {
            config,
            host_interfaces,
            sender,
            process: Uuid::new_v4().to_string(),
            node: Uuid::new_v4().to_string(),
            udp,
            publisher,
            subscriber,
            router,
            requester,
            responses,
            address,
            service_address,
            response_address,
            router_id,
            response_id,
            advertisements: Vec::new(),
            subscriptions: Vec::new(),
            services: Vec::new(),
            peers: HashMap::new(),
            remote_subscribers: HashMap::new(),
            subscriber_interests: HashMap::new(),
            connected: HashSet::new(),
            subscriptions_connected: HashSet::new(),
            response_connections: HashMap::new(),
            pending: HashMap::new(),
            replies: Vec::new(),
        })
    }
    fn announce(&self, channel: usize, message: Discovery) {
        if let Ok(frame) = protocol::encode(&message) {
            self.sender.counters.sent(
                self.udp[channel]
                    .send_to(
                        &frame,
                        SocketAddrV4::new(
                            self.config.multicast,
                            if channel == 0 {
                                self.config.message_port
                            } else {
                                self.config.service_port
                            },
                        ),
                    )
                    .map(|_| ()),
            );
        } else {
            self.sender
                .counters
                .malformed_frames
                .fetch_add(1, Ordering::Relaxed);
        }
    }
    fn wire_publisher(&self, topic: String, kind: PubType) -> WirePublisher {
        WirePublisher {
            topic,
            address: if matches!(kind, PubType::MsgPub(_)) {
                self.address.clone()
            } else {
                self.service_address.clone()
            },
            process_uuid: self.process.clone(),
            node_uuid: self.node.clone(),
            scope: Scope::All.into(),
            pub_type: Some(kind),
        }
    }
    fn subscribe_message(&self, channel: usize, topic: &str) {
        self.announce(
            channel,
            protocol::discovery(
                &self.process,
                Type::Subscribe,
                Some(DiscContents::Sub(Subscriber {
                    topic: topic.into(),
                })),
            ),
        );
    }
    fn subscriber_publisher(&self, topic: &str, type_name: &str) -> WirePublisher {
        self.wire_publisher(
            topic.into(),
            PubType::MsgPub(MessagePublisher {
                msg_type: if type_name.is_empty() {
                    "google.protobuf.Message".into()
                } else {
                    type_name.into()
                },
                ..Default::default()
            }),
        )
    }
    fn service_signatures(&self, topic: &str) -> Vec<ServiceTypes> {
        let mut signatures = self.service_provider_types(topic);
        signatures.dedup();
        signatures
    }
    fn known_topic_path<'a>(&self, wire_topic: &'a str) -> Option<&'a str> {
        // Match the partition spelling produced by qualified_name, including
        // its leading slash for a nonempty partition.
        let partition = self.config.partition.trim_matches('/');
        let partition = if partition.is_empty() {
            String::new()
        } else {
            format!("/{partition}")
        };
        let (wire_partition, path) = decompose_fully_qualified_topic(wire_topic).ok()?;
        (wire_partition == partition && path.starts_with('/')).then_some(path)
    }
    fn known_names(&self, channel: usize) -> Vec<String> {
        let mut names = Vec::new();
        let mut collect = |publisher: &WirePublisher| {
            let right_type = matches!(
                (channel, &publisher.pub_type),
                (0, Some(PubType::MsgPub(_))) | (1, Some(PubType::SrvPub(_)))
            );
            if !right_type {
                return;
            }
            if let Some(path) = self.known_topic_path(&publisher.topic) {
                names.push(path.to_owned());
            }
        };
        if channel == 0 {
            for (publisher, active) in &self.advertisements {
                if active.load(Ordering::Acquire) {
                    collect(publisher);
                }
            }
        } else {
            for service in &self.services {
                if service.active.load(Ordering::Acquire) {
                    collect(&service.publisher);
                }
            }
        }
        for (key, (publisher, stamp)) in &self.peers {
            if key.channel == channel && stamp.elapsed() < Duration::from_secs(3) {
                collect(publisher);
            }
        }
        names.sort();
        names.dedup();
        names
    }
    fn known_topic_names(&self) -> Vec<String> {
        let mut names = self.known_names(0);
        for subscriber in &self.subscriptions {
            if subscriber.active.load(Ordering::Acquire)
                && let Some(path) = self.known_topic_path(&subscriber.topic)
            {
                names.push(path.to_owned());
            }
        }
        for (key, stamp) in &self.subscriber_interests {
            if stamp.elapsed() < Duration::from_secs(3)
                && let Some(path) = self.known_topic_path(&key.topic)
            {
                names.push(path.to_owned());
            }
        }
        names.sort();
        names.dedup();
        names
    }
    fn topic_endpoint_types(&self, topic: &str) -> TopicEndpointTypes {
        let mut publishers = Vec::new();
        for (publisher, active) in &self.advertisements {
            if active.load(Ordering::Acquire)
                && publisher.topic == topic
                && let Some(PubType::MsgPub(info)) = &publisher.pub_type
            {
                publishers.push(info.msg_type.clone());
            }
        }
        for (key, (publisher, stamp)) in &self.peers {
            if key.channel == 0
                && stamp.elapsed() < Duration::from_secs(3)
                && publisher.topic == topic
                && let Some(PubType::MsgPub(info)) = &publisher.pub_type
            {
                publishers.push(info.msg_type.clone());
            }
        }
        let mut subscribers = Vec::new();
        let mut local_types = HashSet::new();
        for subscriber in &self.subscriptions {
            if subscriber.active.load(Ordering::Acquire) && subscriber.topic == topic {
                let type_name = if subscriber.type_name.is_empty() {
                    "google.protobuf.Message"
                } else {
                    &subscriber.type_name
                };
                if local_types.insert(type_name) {
                    subscribers.push(type_name.to_owned());
                }
            }
        }
        subscribers.extend(
            self.subscriber_interests
                .iter()
                .filter(|(key, stamp)| {
                    key.topic == topic && stamp.elapsed() < Duration::from_secs(3)
                })
                .map(|(key, _)| key.type_name.clone()),
        );
        publishers.sort();
        subscribers.sort();
        TopicEndpointTypes {
            publishers,
            subscribers,
        }
    }
    fn service_provider_types(&self, topic: &str) -> Vec<ServiceTypes> {
        let local = self.services.iter().filter_map(|service| {
            service
                .active
                .load(Ordering::Acquire)
                .then_some(&service.publisher)
        });
        let remote = self.peers.iter().filter_map(|(key, (publisher, stamp))| {
            (key.channel == 1 && stamp.elapsed() < Duration::from_secs(3)).then_some(publisher)
        });
        let mut signatures: Vec<_> = local
            .chain(remote)
            .filter(|publisher| publisher.topic == topic)
            .filter_map(|publisher| match &publisher.pub_type {
                Some(PubType::SrvPub(types)) => Some(ServiceTypes {
                    request_type: types.request_type.clone(),
                    response_type: types.response_type.clone(),
                }),
                _ => None,
            })
            .collect();
        signatures.sort();
        signatures
    }
    fn has_subscribers(&self, topic: &str, type_name: &str) -> bool {
        self.subscriptions.iter().any(|s| {
            s.active.load(Ordering::Acquire)
                && s.topic == topic
                && (s.type_name.is_empty() || s.type_name == type_name)
        }) || self.remote_subscribers.values().any(|p| {
            p.topic == topic
                && matches!(&p.pub_type, Some(PubType::MsgPub(m))
                    if m.msg_type == type_name || m.msg_type == "google.protobuf.Message")
        })
    }
    fn command(&mut self, command: Command) {
        match command {
            Command::Advertise(topic, type_name, active) => {
                // A queued replacement must never be followed by the old withdrawal.
                self.cleanup();
                if !active.load(Ordering::Acquire) {
                    return;
                }
                let p = self.wire_publisher(
                    topic,
                    PubType::MsgPub(MessagePublisher {
                        msg_type: type_name,
                        ..Default::default()
                    }),
                );
                self.announce(0, protocol::advertisement(&self.process, p.clone()));
                self.advertisements.push((p, active));
            }
            Command::KnownPublisherTopics(reply) => {
                let _ = reply.try_send(self.known_names(0));
            }
            Command::KnownTopics(reply) => {
                self.announce(
                    0,
                    protocol::discovery(&self.process, Type::SubscribersReq, None),
                );
                let _ = reply.try_send(self.known_topic_names());
            }
            Command::KnownTopicInfo(topic, reply) => {
                self.announce(
                    0,
                    protocol::discovery(&self.process, Type::SubscribersReq, None),
                );
                let _ = reply.try_send(self.topic_endpoint_types(&topic));
            }
            Command::Publish(topic, type_name, data) => {
                self.sender.counters.sent(self.publisher.send_multipart(
                    [
                        topic.as_bytes(),
                        self.address.as_bytes(),
                        &data,
                        type_name.as_bytes(),
                    ],
                    zmq::DONTWAIT,
                ));
                self.deliver(
                    Message {
                        topic,
                        type_name,
                        data,
                    },
                    true,
                );
            }
            Command::Subscribe(topic, type_name, tx, active) => {
                // Retire old filters/registrations before a replacement. This
                // also bounds inactive entries during continuous command traffic.
                self.cleanup();
                if !active.load(Ordering::Acquire) {
                    return;
                }
                // ZeroMQ counts duplicate filters. Own one filter per topic,
                // while retaining each receiver and descriptor registration.
                if !self.subscriptions.iter().any(|s| s.topic == topic) {
                    let _ = self.subscriber.set_subscribe(topic.as_bytes());
                }
                self.subscribe_message(0, &topic);
                self.subscriptions.push(SubscriberState {
                    topic,
                    type_name,
                    tx,
                    active,
                });
                for peer in self
                    .peers
                    .iter()
                    .filter(|(key, _)| key.channel == 0)
                    .map(|(_, (p, _))| p.clone())
                    .collect::<Vec<_>>()
                {
                    self.register_subscriber(&peer);
                }
            }
            Command::Serve(topic, request_type, response_type, tx, active) => {
                self.cleanup();
                if !active.load(Ordering::Acquire) {
                    return;
                }
                let p = self.wire_publisher(
                    topic,
                    PubType::SrvPub(ServicePublisher {
                        socket_id: self.router_id.clone(),
                        request_type,
                        response_type,
                    }),
                );
                self.announce(1, protocol::advertisement(&self.process, p.clone()));
                self.services.push(ServiceState {
                    publisher: p,
                    tx,
                    active,
                });
            }
            Command::KnownServices(reply) => {
                let _ = reply.try_send(self.known_names(1));
            }
            Command::Unsubscribe(topic) => {
                // Even an absent or repeated native retirement refreshes END.
                self.cleanup();
                self.announce(
                    0,
                    protocol::discovery(
                        &self.process,
                        Type::EndConnection,
                        Some(DiscContents::Pub(self.subscriber_publisher(&topic, ""))),
                    ),
                );
            }
            Command::ServiceSignatures(topic, reply) => {
                self.subscribe_message(1, &topic);
                let _ = reply.try_send(self.service_signatures(&topic));
            }
            Command::ServiceProviderTypes(topic, reply) => {
                self.subscribe_message(1, &topic);
                let _ = reply.try_send(self.service_provider_types(&topic));
            }
            Command::HasConnections(topic, type_name, reply) => {
                let _ = reply.try_send(self.has_subscribers(&topic, &type_name));
            }
            Command::Request(pending) => {
                if Instant::now() >= pending.deadline {
                    let _ = pending.reply.try_send(Err(Error::Timeout));
                    return;
                }
                if self.pending.len() < self.config.capacity {
                    self.subscribe_message(1, &pending.topic);
                    let id = Uuid::new_v4().to_string();
                    let local = self.services.iter().find(|s| {
                        s.active.load(Ordering::Acquire)
                            && s.publisher.topic == pending.topic
                            && matches!(&s.publisher.pub_type,Some(PubType::SrvPub(types)) if types.request_type==pending.request_type && types.response_type==pending.response_type)
                    });
                    let mut pending = pending;
                    if let Some(service) = local {
                        pending.sent = true;
                        if service
                            .tx
                            .try_send(Request {
                                data: pending.data.clone(),
                                sender: self.sender.clone(),
                                route: if pending.response_type == EMPTY_TYPE {
                                    Vec::new()
                                } else {
                                    vec![id.as_bytes().to_vec()]
                                },
                                max: self.config.max_message_bytes,
                            })
                            .is_err()
                        {
                            self.sender
                                .counters
                                .queue_drops
                                .fetch_add(1, Ordering::Relaxed);
                            let _ = pending.reply.try_send(Err(Error::QueueFull));
                            return;
                        }
                    }
                    if pending.sent && pending.response_type == EMPTY_TYPE {
                        let _ = pending.reply.try_send(Ok((Vec::new(), true)));
                    } else {
                        self.pending.insert(id, pending);
                    }
                } else {
                    self.sender
                        .counters
                        .queue_drops
                        .fetch_add(1, Ordering::Relaxed);
                    let _ = pending.reply.try_send(Err(Error::QueueFull));
                }
            }
            Command::Respond(route, data, success) => {
                if route.len() == 1 {
                    if let Ok(id) = protocol::text(&route[0])
                        && let Some(p) = self.pending.remove(id)
                    {
                        let _ = p.reply.try_send(Ok((data, success)));
                    }
                    return;
                }
                if route.len() != 5 {
                    self.sender
                        .counters
                        .malformed_frames
                        .fetch_add(1, Ordering::Relaxed);
                    return;
                }
                if self.replies.len() >= self.config.capacity {
                    self.sender
                        .counters
                        .queue_drops
                        .fetch_add(1, Ordering::Relaxed);
                    return;
                }
                let Ok(address) = protocol::text(&route[0]) else {
                    self.sender
                        .counters
                        .malformed_frames
                        .fetch_add(1, Ordering::Relaxed);
                    return;
                };
                if !self.response_connections.contains_key(address) {
                    if self.response_connections.len() >= self.config.capacity {
                        self.sender
                            .counters
                            .queue_drops
                            .fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                    if !self.sender.counters.sent(self.router.connect(address)) {
                        return;
                    }
                }
                self.response_connections
                    .insert(address.into(), Instant::now());
                let mut frames = route[1..].to_vec();
                frames.push(data);
                frames.push(if success {
                    b"1".to_vec()
                } else {
                    b"0".to_vec()
                });
                self.replies
                    .push((Instant::now() + self.config.reply_timeout, frames));
            }
        }
    }
    fn deliver(&self, message: Message, intra_process: bool) {
        for s in &self.subscriptions {
            if s.active.load(Ordering::Acquire)
                && s.topic == message.topic
                && (s.type_name.is_empty() || s.type_name == message.type_name)
            {
                self.sender.counters.dropped(s.tx.try_send(ReceivedMessage {
                    message: message.clone(),
                    intra_process,
                }));
            }
        }
    }
    fn discovery(&mut self, channel: usize, message: Discovery, source: IpAddr) {
        if message.process_uuid == self.process {
            return;
        }
        // Native Discovery tracks process activity per channel for every packet.
        for (key, (p, stamp)) in &mut self.peers {
            if key.channel == channel && p.process_uuid == message.process_uuid {
                *stamp = Instant::now();
            }
        }
        let Ok(kind) = Type::try_from(message.r#type) else {
            return;
        };
        if channel == 0 && kind == Type::SubscribersReq {
            for s in &self.subscriptions {
                if s.active.load(Ordering::Acquire) {
                    self.announce(
                        0,
                        protocol::discovery(
                            &self.process,
                            Type::SubscribersRep,
                            Some(DiscContents::Pub(
                                self.subscriber_publisher(&s.topic, &s.type_name),
                            )),
                        ),
                    );
                }
            }
            return;
        }
        if kind == Type::Bye {
            // Native NodeShared deliberately keeps subscriber registrations
            // across process-only disconnects; only publication peers expire.
            self.peers.retain(|key, (p, _)| {
                key.channel != channel || p.process_uuid != message.process_uuid
            });
            if channel == 0 {
                self.subscriber_interests
                    .retain(|key, _| key.process != message.process_uuid);
            }
        }
        let local = matches!(source, IpAddr::V4(ip) if ip.is_loopback() || self.host_interfaces.contains(&ip));
        if matches!(&message.disc_contents, Some(DiscContents::Pub(p))
        if matches!(kind, Type::Advertise | Type::Unadvertise | Type::SubscribersRep)
            && match Scope::try_from(p.scope).unwrap_or(Scope::Process) {
                Scope::Process => true,
                Scope::Host => !local,
                Scope::All => false,
            })
        {
            return;
        }
        if channel == 0 && kind == Type::EndConnection {
            // MessagePublisher::SetFromDiscovery projects missing message
            // metadata to an empty ctrl, the generic withdrawal form.
            let (topic, process, node, ctrl) = match &message.disc_contents {
                Some(DiscContents::Pub(p)) => (
                    p.topic.as_str(),
                    p.process_uuid.as_str(),
                    p.node_uuid.as_str(),
                    match &p.pub_type {
                        Some(PubType::MsgPub(m)) => m.ctrl.as_str(),
                        _ => "",
                    },
                ),
                Some(DiscContents::Sub(s)) => (s.topic.as_str(), "", "", ""),
                None => ("", "", "", ""),
            };
            if ctrl.is_empty() || ctrl == self.process {
                self.remote_subscribers.retain(|key, _| {
                    key.process != process || key.node != node || key.topic != topic
                });
            }
            self.subscriber_interests
                .retain(|key, _| key.process != process || key.node != node || key.topic != topic);
            return;
        }
        match message.disc_contents {
            Some(DiscContents::Pub(p)) if channel == 0 && kind == Type::SubscribersRep => {
                let Some(PubType::MsgPub(info)) = p.pub_type else {
                    return;
                };
                if p.process_uuid.is_empty()
                    || p.node_uuid.is_empty()
                    || self.known_topic_path(&p.topic).is_none()
                {
                    return;
                }
                self.subscriber_interests
                    .retain(|_, stamp| stamp.elapsed() < Duration::from_secs(3));
                let key = SubscriberInterestKey {
                    process: p.process_uuid,
                    node: p.node_uuid,
                    topic: p.topic,
                    address: p.address,
                    type_name: info.msg_type,
                };
                if let Some(stamp) = self.subscriber_interests.get_mut(&key) {
                    *stamp = Instant::now();
                } else if self.subscriber_interests.len() < self.config.capacity {
                    self.subscriber_interests.insert(key, Instant::now());
                } else {
                    self.sender
                        .counters
                        .queue_drops
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
            Some(DiscContents::Pub(p)) if channel == 0 && kind == Type::NewConnection => {
                if matches!(&p.pub_type, Some(PubType::MsgPub(m)) if m.ctrl == self.process) {
                    let key = PeerKey {
                        channel,
                        process: p.process_uuid.clone(),
                        node: p.node_uuid.clone(),
                        topic: p.topic.clone(),
                        address: p.address.clone(),
                    };
                    if !self.remote_subscribers.contains_key(&key) {
                        if self.remote_subscribers.len() < self.config.capacity {
                            self.remote_subscribers.insert(key, p);
                        } else {
                            self.sender
                                .counters
                                .queue_drops
                                .fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
            Some(DiscContents::Pub(p)) if kind == Type::Advertise => {
                let key = PeerKey {
                    channel,
                    process: p.process_uuid.clone(),
                    node: p.node_uuid.clone(),
                    topic: p.topic.clone(),
                    address: p.address.clone(),
                };
                if !self.peers.contains_key(&key) && self.peers.len() < self.config.capacity {
                    if channel == 0 && matches!(p.pub_type, Some(PubType::MsgPub(_))) {
                        self.register_subscriber(&p);
                    }
                    self.peers.insert(key, (p, Instant::now()));
                }
            }
            Some(DiscContents::Pub(p)) if kind == Type::Unadvertise => {
                self.peers.retain(|key, _| {
                    key.channel != channel
                        || key.process != p.process_uuid
                        || key.node != p.node_uuid
                        || key.topic != p.topic
                });
                if channel == 0 && !p.topic.is_empty() && !p.node_uuid.is_empty() {
                    self.remote_subscribers.retain(|key, _| {
                        key.process != p.process_uuid
                            || key.node != p.node_uuid
                            || key.topic != p.topic
                    });
                }
            }
            Some(DiscContents::Sub(s)) if kind == Type::Subscribe => {
                let publishers: Vec<_> = if channel == 0 {
                    self.advertisements
                        .iter()
                        .filter(|(_, a)| a.load(Ordering::Acquire))
                        .map(|(p, _)| p.clone())
                        .collect()
                } else {
                    self.services
                        .iter()
                        .filter(|s| s.active.load(Ordering::Acquire))
                        .map(|s| s.publisher.clone())
                        .collect()
                };
                for p in publishers {
                    if p.topic == s.topic {
                        self.announce(channel, protocol::advertisement(&self.process, p));
                    }
                }
            }
            _ => {}
        }
    }
    fn register_subscriber(&mut self, publisher: &WirePublisher) {
        let Some(PubType::MsgPub(message)) = &publisher.pub_type else {
            return;
        };
        if self.subscriptions.iter().any(|s| {
            s.active.load(Ordering::Acquire)
                && s.topic == publisher.topic
                && (s.type_name.is_empty() || s.type_name == message.msg_type)
        }) {
            if self.subscriptions_connected.len() < self.config.capacity
                && self
                    .subscriptions_connected
                    .insert(publisher.address.clone())
            {
                self.sender
                    .counters
                    .sent(self.subscriber.connect(&publisher.address));
            }
            let mut registered = publisher.clone();
            registered.process_uuid = self.process.clone();
            registered.node_uuid = self.node.clone();
            registered.pub_type = Some(PubType::MsgPub(MessagePublisher {
                ctrl: publisher.process_uuid.clone(),
                ..message.clone()
            }));
            self.announce(
                0,
                protocol::discovery(
                    &self.process,
                    Type::NewConnection,
                    Some(DiscContents::Pub(registered)),
                ),
            );
        }
    }
    fn run(mut self, rx: Receiver<Command>, stop: Arc<AtomicBool>) {
        let mut heartbeat = Instant::now();
        let mut buffer = vec![0; protocol::MAX_DATAGRAM];
        while !stop.load(Ordering::Acquire) {
            self.cleanup();
            for _ in 0..self.config.capacity {
                match rx.try_recv() {
                    Ok(cmd) => self.command(cmd),
                    Err(_) => break,
                }
            }
            for channel in 0..2 {
                for _ in 0..self.config.capacity {
                    match self.udp[channel].recv_from(&mut buffer) {
                        Ok((len, source)) => {
                            if let Ok(msg) = protocol::decode(&buffer[..len]) {
                                self.discovery(channel, msg, source.ip());
                            } else {
                                self.sender
                                    .counters
                                    .malformed_frames
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
            self.receive_publications();
            self.receive_requests();
            self.receive_responses();
            self.send_requests();
            self.send_replies();
            if heartbeat.elapsed() >= Duration::from_secs(1) {
                for channel in 0..2 {
                    self.announce(
                        channel,
                        protocol::discovery(&self.process, Type::Heartbeat, None),
                    );
                }
                for (p, active) in &self.advertisements {
                    if active.load(Ordering::Acquire) {
                        self.announce(0, protocol::advertisement(&self.process, p.clone()));
                    }
                }
                for p in &self.services {
                    if p.active.load(Ordering::Acquire) {
                        self.announce(
                            1,
                            protocol::advertisement(&self.process, p.publisher.clone()),
                        );
                    }
                }
                for s in &self.subscriptions {
                    if s.active.load(Ordering::Acquire) {
                        self.subscribe_message(0, &s.topic);
                    }
                }
                for p in self.pending.values() {
                    self.subscribe_message(1, &p.topic);
                }
                heartbeat = Instant::now();
            }
            self.peers
                .retain(|_, (_, stamp)| stamp.elapsed() < Duration::from_secs(3));
            self.subscriber_interests
                .retain(|_, stamp| stamp.elapsed() < Duration::from_secs(3));
            // Wake for commands while retaining the bounded socket poll interval.
            if let Ok(cmd) = rx.recv_timeout(Duration::from_millis(2)) {
                self.command(cmd);
            }
        }
        // Graceful Node shutdown withdraws subscriptions before process BYE.
        // Native publishers intentionally retain registration on BYE alone.
        let mut topics = HashSet::new();
        for subscription in &self.subscriptions {
            if topics.insert(subscription.topic.as_str()) {
                self.announce(
                    0,
                    protocol::discovery(
                        &self.process,
                        Type::EndConnection,
                        Some(DiscContents::Pub(
                            self.subscriber_publisher(&subscription.topic, ""),
                        )),
                    ),
                );
            }
        }
        for channel in 0..2 {
            self.announce(channel, protocol::discovery(&self.process, Type::Bye, None));
        }
    }
    fn receive_publications(&self) {
        for _ in 0..self.config.capacity {
            let Some(frames) = receive(&self.subscriber, 4, &self.sender.counters, || {
                self.sender.stop.load(Ordering::Acquire)
            }) else {
                break;
            };
            if let (Ok(topic), Ok(type_name)) =
                (protocol::text(&frames[0]), protocol::text(&frames[3]))
            {
                self.deliver(
                    Message {
                        topic: topic.into(),
                        type_name: type_name.into(),
                        data: frames[2].clone(),
                    },
                    false,
                );
            } else {
                self.sender
                    .counters
                    .malformed_frames
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    fn send_replies(&mut self) {
        self.replies.retain(|(deadline, frames)| {
            if Instant::now() >= *deadline {
                self.sender
                    .counters
                    .reply_timeouts
                    .fetch_add(1, Ordering::Relaxed);
                false
            } else {
                !self
                    .sender
                    .counters
                    .sent(self.router.send_multipart(frames, zmq::DONTWAIT))
            }
        });
    }
    fn cleanup(&mut self) {
        let mut removed = Vec::new();
        self.advertisements.retain(|(p, a)| {
            let keep = a.load(Ordering::Acquire);
            if !keep {
                removed.push((0, p.clone()));
            }
            keep
        });
        self.services.retain(|s| {
            let keep = s.active.load(Ordering::Acquire);
            if !keep {
                removed.push((1, s.publisher.clone()));
            }
            keep
        });
        for (channel, p) in removed {
            self.announce(
                channel,
                protocol::discovery(&self.process, Type::Unadvertise, Some(DiscContents::Pub(p))),
            );
        }
        let mut topics = HashSet::new();
        self.subscriptions.retain(|s| {
            let keep = s.active.load(Ordering::Acquire);
            if !keep {
                topics.insert(s.topic.clone());
            }
            keep
        });
        for topic in topics {
            if !self.subscriptions.iter().any(|s| s.topic == topic) {
                let _ = self.subscriber.set_unsubscribe(topic.as_bytes());
                self.announce(
                    0,
                    protocol::discovery(
                        &self.process,
                        Type::EndConnection,
                        Some(DiscContents::Pub(self.subscriber_publisher(&topic, ""))),
                    ),
                );
            }
        }
        self.connected.retain(|address| {
            let keep = self
                .peers
                .iter()
                .any(|(key, (p, _))| key.channel == 1 && &p.address == address);
            if !keep {
                let _ = self.requester.disconnect(address);
            }
            keep
        });
        self.subscriptions_connected.retain(|address| {
            let keep = self.peers.iter().any(|(key, (p, _))| {
                key.channel == 0
                    && &p.address == address
                    && self.subscriptions.iter().any(|s| s.topic == p.topic)
            });
            if !keep {
                let _ = self.subscriber.disconnect(address);
            }
            keep
        });
        self.response_connections.retain(|address, stamp| {
            let keep = stamp.elapsed() < self.config.reply_timeout.max(Duration::from_secs(3));
            if !keep {
                let _ = self.router.disconnect(address);
            }
            keep
        });
    }
    fn receive_requests(&mut self) {
        for _ in 0..self.config.capacity {
            let Some(f) = receive(&self.router, 9, &self.sender.counters, || {
                self.sender.stop.load(Ordering::Acquire)
            }) else {
                break;
            };
            for service in &self.services {
                let p = &service.publisher;
                if service.active.load(Ordering::Acquire)
                    && f[1] == p.topic.as_bytes()
                    && matches!(&p.pub_type,Some(PubType::SrvPub(types)) if f[7]==types.request_type.as_bytes() && f[8]==types.response_type.as_bytes())
                {
                    let route = if f[8] == EMPTY_TYPE.as_bytes() {
                        Vec::new()
                    } else {
                        vec![
                            f[2].clone(),
                            f[3].clone(),
                            f[1].clone(),
                            f[4].clone(),
                            f[5].clone(),
                        ]
                    };
                    self.sender.counters.dropped(service.tx.try_send(Request {
                        data: f[6].clone(),
                        sender: self.sender.clone(),
                        route,
                        max: self.config.max_message_bytes,
                    }));
                    break;
                }
            }
        }
    }
    fn receive_responses(&mut self) {
        for _ in 0..self.config.capacity {
            let Some(f) = receive(&self.responses, 6, &self.sender.counters, || {
                self.sender.stop.load(Ordering::Acquire)
            }) else {
                break;
            };
            if f[2] != self.node.as_bytes() {
                continue;
            }
            if let Ok(id) = protocol::text(&f[3])
                && self
                    .pending
                    .get(id)
                    .is_some_and(|p| f[1] == p.topic.as_bytes())
                && let Some(p) = self.pending.remove(id)
            {
                let _ = p.reply.try_send(Ok((f[4].clone(), f[5] == b"1")));
            }
        }
    }
    fn send_requests(&mut self) {
        self.pending.retain(|id, p| {
            if Instant::now() >= p.deadline {
                let _ = p.reply.try_send(Err(Error::Timeout));
                return false;
            }
            if p.sent {
                return true;
            }
            for (key, (peer, _)) in &self.peers {
                if key.channel == 1
                    && let Some(PubType::SrvPub(types)) = &peer.pub_type
                    && peer.topic == p.topic
                    && types.request_type == p.request_type
                    && types.response_type == p.response_type
                {
                    if self.connected.insert(peer.address.clone()) {
                        self.sender
                            .counters
                            .sent(self.requester.connect(&peer.address));
                    }
                    // Mandatory routing reports EHOSTUNREACH until ZMTP is ready.
                    p.sent = self.sender.counters.sent(self.requester.send_multipart(
                        [
                            types.socket_id.as_bytes(),
                            p.topic.as_bytes(),
                            self.response_address.as_bytes(),
                            self.response_id.as_bytes(),
                            self.node.as_bytes(),
                            id.as_bytes(),
                            &p.data,
                            p.request_type.as_bytes(),
                            p.response_type.as_bytes(),
                        ],
                        zmq::DONTWAIT,
                    ));
                    break;
                }
            }
            if p.sent && p.response_type == EMPTY_TYPE {
                let _ = p.reply.try_send(Ok((Vec::new(), true)));
                return false;
            }
            true
        });
    }
}

#[cfg(test)]
#[path = "../tests/support/runtime.rs"]
mod tests;
