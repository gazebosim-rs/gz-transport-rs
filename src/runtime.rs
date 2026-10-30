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
    collections::{HashMap, HashSet, VecDeque},
    hash::{Hash, Hasher},
    net::{IpAddr, Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering},
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
    /// Process-local direct routing failures; the ordinary wire send is still attempted.
    pub routing_errors: u64,
    /// Recipient clock failures while admitting rate-limited subscription deliveries.
    pub subscription_clock_errors: u64,
}
#[derive(Default)]
struct Counters {
    queue_drops: AtomicU64,
    malformed_frames: AtomicU64,
    send_failures: AtomicU64,
    reply_timeouts: AtomicU64,
    routing_errors: AtomicU64,
    subscription_clock_errors: AtomicU64,
}
impl Counters {
    fn snapshot(&self) -> Statistics {
        Statistics {
            queue_drops: self.queue_drops.load(Ordering::Relaxed),
            malformed_frames: self.malformed_frames.load(Ordering::Relaxed),
            send_failures: self.send_failures.load(Ordering::Relaxed),
            reply_timeouts: self.reply_timeouts.load(Ordering::Relaxed),
            routing_errors: self.routing_errors.load(Ordering::Relaxed),
            subscription_clock_errors: self.subscription_clock_errors.load(Ordering::Relaxed),
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
/// `intra_process` records direct delivery within the local process.
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
/// Owned discovery metadata for one known service provider record.
/// Retaining this value does not keep the provider alive or prove reachability.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServiceProviderInfo {
    /// The original fully qualified wire topic, including its partition.
    pub topic: String,
    pub address: String,
    /// Advertised process identity; Rust generates this per worker.
    pub process_uuid: String,
    pub node_uuid: String,
    pub scope: Scope,
    pub socket_id: String,
    pub request_type: String,
    pub response_type: String,
}
/// Owned metadata in the native MessagePublisher shape, with effective options.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TopicPublisherInfo {
    pub topic: String,
    pub address: String,
    /// Advertised control field, which may be empty.
    pub control_address: String,
    /// Advertised process identity; Rust generates this per worker.
    pub process_uuid: String,
    pub node_uuid: String,
    pub scope: Scope,
    pub type_name: String,
    pub throttled: bool,
    pub msgs_per_sec: u64,
}
/// Subscriber metadata uses the same native fields as publication metadata.
pub type TopicSubscriberInfo = TopicPublisherInfo;
impl TopicPublisherInfo {
    fn from_publisher(publisher: &WirePublisher) -> Option<Self> {
        let Some(PubType::MsgPub(info)) = &publisher.pub_type else {
            return None;
        };
        // Native ignores the raw rate when throttled is false, then derives
        // Throttled from the effective rate, even for true/MAX.
        let msgs_per_sec = if info.throttled {
            info.msgs_per_sec
        } else {
            u64::MAX
        };
        Some(Self {
            topic: publisher.topic.clone(),
            address: publisher.address.clone(),
            control_address: info.ctrl.clone(),
            process_uuid: publisher.process_uuid.clone(),
            node_uuid: publisher.node_uuid.clone(),
            scope: publisher.scope(),
            type_name: info.msg_type.clone(),
            throttled: msgs_per_sec != u64::MAX,
            msgs_per_sec,
        })
    }
    fn same_subscriber(&self, other: &Self) -> bool {
        // Native merges local handlers with remote storage by MessagePublisher
        // equality, which deliberately ignores rate options.
        self.topic == other.topic
            && self.address == other.address
            && self.control_address == other.control_address
            && self.process_uuid == other.process_uuid
            && self.node_uuid == other.node_uuid
            && self.scope == other.scope
            && self.type_name == other.type_name
    }
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
    response_tx: SyncSender<Command>,
    stop: Arc<AtomicBool>,
    slots: Arc<AtomicUsize>,
    callback_threads: Arc<AtomicUsize>,
    callback_requests: Arc<AtomicUsize>,
    capacity: usize,
    counters: Arc<Counters>,
    identity: WorkerOwner,
    publication_sequences: Option<PublicationSequences>,
}
type PublicationSequences = Arc<Mutex<HashMap<String, Arc<AtomicU64>>>>;
type WorkerOwner = Arc<OnceLock<WorkerIdentity>>;
type WeakWorkerOwner = Weak<OnceLock<WorkerIdentity>>;
struct LocalService {
    topic: String,
    types: ServiceTypes,
    active: Weak<AtomicBool>,
    callback: Option<LocalCallback>,
    queue: Weak<LocalServiceQueue>,
}
#[derive(Clone)]
enum LocalCallback {
    OneWay(Weak<OneWayCallbackState>),
    Response(Weak<ResponseCallbackState>),
}
enum CallbackState {
    OneWay(Arc<OneWayCallbackState>),
    Response(Arc<ResponseCallbackState>),
}
impl LocalCallback {
    // Call only after releasing registry guards: a failed liveness check can
    // destroy the last strong reference and run captured user Drop code.
    fn upgrade(&self) -> Option<CallbackState> {
        match self {
            Self::OneWay(state) => state
                .upgrade()
                .filter(|state| state.live() && state.ready.load(Ordering::Acquire))
                .map(CallbackState::OneWay),
            Self::Response(state) => state
                .upgrade()
                .filter(|state| state.live() && state.ready.load(Ordering::Acquire))
                .map(CallbackState::Response),
        }
    }
}
struct WorkerIdentity {
    process: String,
    node: String,
    address: String,
    response_address: String,
    response_id: String,
    service_domain: MessageDomain,
    services: Arc<Mutex<Vec<LocalService>>>,
    stopped: Weak<AtomicBool>,
}
static LOCAL_SERVICE_WORKERS: OnceLock<Mutex<Vec<WeakWorkerOwner>>> = OnceLock::new();
fn local_service_workers() -> &'static Mutex<Vec<WeakWorkerOwner>> {
    LOCAL_SERVICE_WORKERS.get_or_init(|| Mutex::new(Vec::new()))
}
fn service_workers(index: &Mutex<Vec<WeakWorkerOwner>>) -> Result<Vec<WorkerOwner>, Error> {
    let mut workers = index
        .lock()
        .map_err(|_| Error::Native("process-local service owners poisoned".into()))?;
    workers.retain(|worker| worker.strong_count() != 0);
    Ok(workers.iter().filter_map(Weak::upgrade).collect())
}
impl Sender {
    fn callback_permit(&self, count: &Arc<AtomicUsize>) -> Result<CallbackPermit, Error> {
        if self.stop.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < self.capacity).then_some(n + 1)
            })
            .map_err(|_| {
                self.counters.queue_drops.fetch_add(1, Ordering::Relaxed);
                Error::QueueFull
            })?;
        Ok(CallbackPermit(count.clone()))
    }
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

    // A publication flood cannot consume the bounded service-response slots.
    fn send_response(&self, cmd: Command) -> Result<(), Error> {
        if self.stop.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        self.response_tx.try_send(cmd).map_err(|error| match error {
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
// Only Node and Publishers own this value. The worker owns Sender, never this
// Arc, so the final publisher can stop and join without an ownership cycle.
struct WorkerLifetime {
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl WorkerLifetime {
    fn stop_and_join(&self) {
        self.stop.store(true, Ordering::Release);
        let worker = self
            .worker
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(worker) = worker {
            // A worker never owns this lifetime. Keep the join guard explicit.
            if worker.thread().id() != thread::current().id() {
                let _ = worker.join();
            }
        }
    }
}
impl Drop for WorkerLifetime {
    fn drop(&mut self) {
        self.stop_and_join();
    }
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
const LOCAL_INTEREST: u8 = 1;
const REMOTE_INTEREST: u8 = 2;
const PROCESS_INTEREST: u8 = 4;

struct LocalPublisherInterest {
    topic: String,
    type_name: String,
    active: Weak<AtomicBool>,
    bits: Weak<AtomicU8>,
}
struct LocalSubscriberInterest {
    topic: String,
    type_name: String,
    active: Weak<AtomicBool>,
}
#[derive(Default)]
struct LocalInterestCache {
    publishers: Vec<LocalPublisherInterest>,
    subscribers: Vec<LocalSubscriberInterest>,
}
impl LocalInterestCache {
    fn refresh(&mut self) {
        self.subscribers.retain(|sub| {
            sub.active
                .upgrade()
                .is_some_and(|a| a.load(Ordering::Acquire))
        });
        self.publishers.retain(|pub_| {
            pub_.active
                .upgrade()
                .is_some_and(|a| a.load(Ordering::Acquire))
                && pub_.bits.upgrade().is_some()
        });
        for pub_ in &self.publishers {
            let matching = self.subscribers.iter().any(|sub| {
                sub.topic == pub_.topic
                    && (sub.type_name.is_empty() || sub.type_name == pub_.type_name)
            });
            if let Some(bits) = pub_.bits.upgrade() {
                if matching {
                    bits.fetch_or(LOCAL_INTEREST, Ordering::AcqRel);
                } else {
                    bits.fetch_and(!LOCAL_INTEREST, Ordering::AcqRel);
                }
            }
        }
    }
}
pub struct Publisher {
    _lease: Lease,
    sender: Sender,
    _lifetime: Arc<WorkerLifetime>,
    interest: Arc<AtomicU8>,
    publication: PublicationHandle,
    topic: String,
    type_name: String,
    max: usize,
    reply_timeout: Duration,
    throttle: Option<PublicationThrottle>,
}
struct PublicationThrottle {
    period_ns: f64,
    last: Mutex<Duration>,
}
#[cfg(unix)]
fn publication_clock(clock: nix::time::ClockId) -> Result<Duration, Error> {
    nix::time::clock_gettime(clock)
        .map(Duration::from)
        .map_err(|error| Error::Native(error.to_string()))
}
fn publication_time() -> Result<Duration, Error> {
    #[cfg(unix)]
    {
        // The qualified native Linux steady_clock uses CLOCK_MONOTONIC.
        publication_clock(nix::time::ClockId::CLOCK_MONOTONIC)
    }
    #[cfg(not(unix))]
    Err(Error::Unsupported("publication rate clock"))
}
impl PublicationThrottle {
    fn new(rate: u64) -> Option<Self> {
        (rate != u64::MAX).then(|| Self {
            period_ns: 1e9 / rate as f64,
            // Native initializes its last-publication timestamp at the clock epoch.
            last: Mutex::new(Duration::ZERO),
        })
    }
    fn ready_at(&self, last: Duration, now: Duration) -> bool {
        // Native compares integer elapsed nanoseconds against a double period.
        // Zero rate has an infinite period; positive sub-ns periods stay positive.
        now.saturating_sub(last).as_nanos() as f64 >= self.period_ns
    }
    fn ready(&self) -> Result<bool, Error> {
        let last = self.last.lock().unwrap_or_else(|error| error.into_inner());
        Ok(self.ready_at(*last, publication_time()?))
    }
    fn admit(&self) -> Result<bool, Error> {
        let mut last = self.last.lock().unwrap_or_else(|error| error.into_inner());
        let now = publication_time()?;
        if !self.ready_at(*last, now) {
            return Ok(false);
        }
        *last = now;
        Ok(true)
    }
}
// Both worker delivery and other-Node direct delivery share this registration's
// admission clock. Suppression happens before its bounded queue is attempted.
struct SubscriptionThrottle {
    limiter: PublicationThrottle,
    counters: Arc<Counters>,
}
impl SubscriptionThrottle {
    fn new(rate: u64, counters: Arc<Counters>) -> Option<Arc<Self>> {
        PublicationThrottle::new(rate).map(|limiter| Arc::new(Self { limiter, counters }))
    }
    fn admission(&self, result: Result<bool, Error>) -> bool {
        match result {
            Ok(admitted) => admitted,
            Err(_) => {
                self.counters
                    .subscription_clock_errors
                    .fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
    fn admit(&self) -> bool {
        self.admission(self.limiter.admit())
    }
}
impl Publisher {
    /// Inspect rate eligibility without consuming a publication interval.
    /// This clock query does not acknowledge queue admission or delivery.
    /// Clock failures are explicit; eligibility does not observe cancellation.
    pub fn throttled_update_ready(&self) -> Result<bool, Error> {
        self.throttle
            .as_ref()
            .map_or(Ok(true), PublicationThrottle::ready)
    }
    /// Read known matching registration interest without worker coordination.
    /// This does not acknowledge authentication or delivery. The remote half
    /// remembers native NEW_CONNECTION records across process-only BYE.
    pub fn connection_snapshot(&self) -> Result<bool, Error> {
        if self.sender.stop.load(Ordering::Acquire) || !self._lease.active.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        Ok(self.interest.load(Ordering::Acquire) != 0)
    }
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
    /// Refresh the cached membership outside a latency-sensitive step.
    /// The explicit timeout bounds this exact publisher's worker reply wait.
    /// Zero returns `Timeout`; an unrepresentable deadline is `InvalidConfig`.
    /// Those validation errors enqueue nothing.
    /// Matching live process-local routes and learned remote registrations count;
    /// this does not acknowledge remote authentication or payload delivery.
    pub fn refresh_connection_snapshot(&self, timeout: Duration) -> Result<bool, Error> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(Error::InvalidConfig)?;
        if timeout.is_zero() {
            return Err(Error::Timeout);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender.send(Command::RefreshConnections(
            self.topic.clone(),
            self.type_name.clone(),
            self._lease.active.clone(),
            self.interest.clone(),
            deadline,
            tx,
        ))?;
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(receive_error)?
    }
    /// Encode a canonical value after checking its descriptor and size.
    /// Typed reencoding does not preserve unknown fields from original wire bytes.
    pub fn publish_typed<M: gz_msgs::CanonicalMessage>(&self, value: &M) -> Result<(), Error> {
        if self.type_name != M::PROTOBUF_NAME || value.encoded_len() > self.max {
            return Err(Error::InvalidFrame);
        }
        if !self.admit_publication()? {
            return Ok(());
        }
        self.send_publication(value.encode_to_vec())
    }
    pub fn publish(&self, data: impl Into<Vec<u8>>) -> Result<(), Error> {
        let data = data.into();
        if data.len() > self.max {
            return Err(Error::InvalidFrame);
        }
        if !self.admit_publication()? {
            return Ok(());
        }
        self.send_publication(data)
    }
    fn admit_publication(&self) -> Result<bool, Error> {
        let Some(throttle) = &self.throttle else {
            return Ok(true);
        };
        // A suppressed publication still observes the existing cancellation
        // boundary even though it never reaches Sender::send.
        if self.sender.stop.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        throttle.admit()
    }
    fn send_publication(&self, data: Vec<u8>) -> Result<(), Error> {
        self.sender.send(Command::Publish(
            self.topic.clone(),
            self.type_name.clone(),
            data,
            Some(self.publication.clone()),
        ))
    }
}
pub struct Subscription {
    rx: Receiver<ReceivedMessage>,
    _lease: Lease,
    owner_dead: Arc<AtomicBool>,
    local_interest: Arc<Mutex<LocalInterestCache>>,
}
/// Discovery scope for a message publisher. All peers are eligible by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvertiseOptions {
    scope: Scope,
    msgs_per_sec: u64,
}
impl Default for AdvertiseOptions {
    fn default() -> Self {
        Self {
            scope: Scope::All,
            msgs_per_sec: u64::MAX,
        }
    }
}
impl AdvertiseOptions {
    pub fn scope(&self) -> Scope {
        self.scope
    }
    pub fn set_scope(&mut self, scope: Scope) {
        self.scope = scope;
    }
    pub fn msgs_per_sec(&self) -> u64 {
        self.msgs_per_sec
    }
    pub fn set_msgs_per_sec(&mut self, rate: u64) {
        self.msgs_per_sec = rate;
    }
    pub fn throttled(&self) -> bool {
        self.msgs_per_sec != u64::MAX
    }
}
impl std::fmt::Display for AdvertiseOptions {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scope = scope_name(self.scope);
        writeln!(output, "Advertise options:\n\tScope: {scope}")?;
        if self.throttled() {
            writeln!(
                output,
                "\tThrottled? Yes\n\tRate: {} msgs/sec",
                self.msgs_per_sec
            )
        } else {
            writeln!(output, "\tThrottled? No")
        }
    }
}
/// Discovery eligibility for an admitted service. This is not access control.
/// Options are copied on admission; changing the source value cannot change a lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceOptions {
    scope: Scope,
}
impl Default for ServiceOptions {
    fn default() -> Self {
        Self { scope: Scope::All }
    }
}
impl ServiceOptions {
    pub fn scope(&self) -> Scope {
        self.scope
    }
    pub fn set_scope(&mut self, scope: Scope) {
        self.scope = scope;
    }
}
impl std::fmt::Display for ServiceOptions {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            output,
            "Advertise options:\n\tScope: {}",
            scope_name(self.scope)
        )
    }
}
fn scope_name(scope: Scope) -> &'static str {
    match scope {
        Scope::Process => "Process",
        Scope::Host => "Host",
        Scope::All => "All",
    }
}
/// Per-subscription local-delivery policy. The default receives publications
/// from the owning Node as well as matching wire publishers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscribeOptions {
    ignore_local_messages: bool,
    msgs_per_sec: u64,
}
impl Default for SubscribeOptions {
    fn default() -> Self {
        Self {
            ignore_local_messages: false,
            msgs_per_sec: u64::MAX,
        }
    }
}
impl SubscribeOptions {
    pub fn msgs_per_sec(&self) -> u64 {
        self.msgs_per_sec
    }
    pub fn set_msgs_per_sec(&mut self, rate: u64) {
        self.msgs_per_sec = rate;
    }
    pub fn throttled(&self) -> bool {
        self.msgs_per_sec != u64::MAX
    }
    pub fn ignore_local_messages(&self) -> bool {
        self.ignore_local_messages
    }
    pub fn set_ignore_local_messages(&mut self, ignore: bool) {
        self.ignore_local_messages = ignore;
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        release(&self._lease.active, &self._lease.slots);
        self.local_interest
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .refresh();
    }
}
impl Subscription {
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Message, Error> {
        self.recv_with_info_timeout(timeout)
            .map(ReceivedMessage::into_message)
    }
    /// Receive owned bytes with a borrowed metadata view, using the same bounded
    /// queue, timeout and shutdown behavior as the raw receive method.
    pub fn recv_with_info_timeout(&self, timeout: Duration) -> Result<ReceivedMessage, Error> {
        if self.owner_dead.load(Ordering::Acquire) {
            return self.rx.try_recv().map_err(|_| Error::Shutdown);
        }
        self.rx.recv_timeout(timeout).map_err(receive_error)
    }
}
pub struct Service {
    rx: Receiver<Request>,
    _lease: Lease,
    owner_dead: Arc<AtomicBool>,
    _queue: Arc<LocalServiceQueue>,
}
struct LocalServiceQueue {
    tx: Weak<SyncSender<Request>>,
    sender: Sender,
    active: Weak<AtomicBool>,
    owner_dead: Arc<AtomicBool>,
    max: usize,
}
impl LocalServiceQueue {
    fn send(
        &self,
        caller: &Sender,
        caller_dead: &AtomicBool,
        id: &str,
        pending: &Pending,
    ) -> Result<bool, Error> {
        let Some(tx) = self.tx.upgrade() else {
            return Ok(false);
        };
        self.send_on(tx, caller, caller_dead, id, pending)
    }
    // The upgraded sender is an in-flight lease. Recheck liveness after that
    // upgrade, outside all registries, before the one nonblocking handoff.
    fn send_on(
        &self,
        tx: Arc<SyncSender<Request>>,
        caller: &Sender,
        caller_dead: &AtomicBool,
        id: &str,
        pending: &Pending,
    ) -> Result<bool, Error> {
        if caller.stop.load(Ordering::Acquire) || caller_dead.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        if Instant::now() >= pending.deadline {
            return Err(Error::Timeout);
        }
        if self.sender.stop.load(Ordering::Acquire)
            || self.owner_dead.load(Ordering::Acquire)
            || !self
                .active
                .upgrade()
                .is_some_and(|active| active.load(Ordering::Acquire))
        {
            return Ok(false);
        }
        if pending.data.len() > self.max {
            return Err(Error::InvalidFrame);
        }
        let identity = caller
            .identity
            .get()
            .expect("worker initialized caller identity");
        tx.try_send(Request {
            data: pending.data.clone(),
            sender: self.sender.clone(),
            route: if pending.response_type == EMPTY_TYPE {
                Vec::new()
            } else {
                // Same route as an actual received request, using the caller
                // worker's bound response endpoint. Replies keep the provider
                // response queue and ordinary mandatory ZeroMQ routing.
                vec![
                    identity.response_address.as_bytes().to_vec(),
                    identity.response_id.as_bytes().to_vec(),
                    pending.topic.as_bytes().to_vec(),
                    identity.node.as_bytes().to_vec(),
                    id.as_bytes().to_vec(),
                ]
            },
            max: self.max,
            owner_dead: self.owner_dead.clone(),
        })
        .map_err(|_| {
            self.sender
                .counters
                .queue_drops
                .fetch_add(1, Ordering::Relaxed);
            Error::QueueFull
        })?;
        Ok(true)
    }
}
type OneWayCallback = dyn Fn(&[u8]) -> Result<(), Error> + Send + Sync;
struct OneWayCallbackState {
    callback: Box<OneWayCallback>,
    active: Arc<AtomicBool>,
    slots: Arc<AtomicUsize>,
    owner_dead: Arc<AtomicBool>,
    sender: Sender,
    max: usize,
    ready: AtomicBool,
}
impl OneWayCallbackState {
    fn live(&self) -> bool {
        self.active.load(Ordering::Acquire)
            && !self.owner_dead.load(Ordering::Acquire)
            && !self.sender.stop.load(Ordering::Acquire)
    }
    fn invoke(&self, data: &[u8]) -> Result<(), Error> {
        if !self.live() {
            return Err(Error::Shutdown);
        }
        if data.len() > self.max {
            return Err(Error::InvalidFrame);
        }
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.callback)(data)))
            .unwrap_or_else(|_| Err(Error::Native("one-way callback panicked".into())))
    }
    fn record_error(&self) {
        self.sender
            .counters
            .routing_errors
            .fetch_add(1, Ordering::Relaxed);
    }
}
/// Owned one-way callback registration. Drop waits for the queued dispatcher;
/// a same-domain caller invocation already in progress may finish after Drop.
pub struct OneWayCallbackService {
    state: Arc<OneWayCallbackState>,
    worker: Option<JoinHandle<()>>,
}
impl Drop for OneWayCallbackService {
    fn drop(&mut self) {
        release(&self.state.active, &self.state.slots);
        if let Some(worker) = self.worker.take()
            && worker.thread().id() != thread::current().id()
        {
            let _ = worker.join();
        }
    }
}
// Callback dispatch has its own bounds: withdrawing a service releases its
// registration slot, but cannot release a thread still executing user code.
struct CallbackPermit(Arc<AtomicUsize>);
impl Drop for CallbackPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
type CallbackTask = Box<dyn FnOnce() + Send>;
type CallbackSpawner = fn(CallbackTask) -> std::io::Result<JoinHandle<()>>;
fn spawn_callback(task: CallbackTask) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("gz-transport-callback".into())
        .spawn(task)
}
type ResponseCallback = dyn Fn(&[u8]) -> Result<(Vec<u8>, bool), Error> + Send + Sync;
struct ResponseCallbackState {
    callback: Box<ResponseCallback>,
    active: Arc<AtomicBool>,
    owner_dead: Arc<AtomicBool>,
    sender: Sender,
    max: usize,
    ready: AtomicBool,
}
impl ResponseCallbackState {
    fn live(&self) -> bool {
        self.active.load(Ordering::Acquire)
            && !self.owner_dead.load(Ordering::Acquire)
            && !self.sender.stop.load(Ordering::Acquire)
    }
    fn invoke(&self, data: &[u8]) -> Result<(Vec<u8>, bool), Error> {
        if !self.live() {
            return Err(Error::Shutdown);
        }
        if data.len() > self.max {
            return Err(Error::InvalidFrame);
        }
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.callback)(data)))
                .unwrap_or_else(|_| Err(Error::Native("service callback panicked".into())))?;
        if result.0.len() > self.max {
            return Err(Error::InvalidFrame);
        }
        Ok(result)
    }
}
/// A response callback registration and its owned queued dispatcher.
/// Cancellation retires the lease. Drop joins the dispatcher unless called on
/// that dispatcher. An inline invocation already selected may finish afterward.
pub struct CallbackService {
    state: Arc<ResponseCallbackState>,
    worker: Option<JoinHandle<()>>,
}
impl CallbackService {
    pub fn cancel(&self) {
        release(&self.state.active, &self.state.sender.slots);
    }
}
impl Drop for CallbackService {
    fn drop(&mut self) {
        self.cancel();
        join_callback(self.worker.take());
    }
}
fn join_callback(worker: Option<JoinHandle<()>>) {
    if let Some(worker) = worker
        && worker.thread().id() != thread::current().id()
    {
        let _ = worker.join();
    }
}
/// One callback request completion. Local exact matches complete inline;
/// queued/remote replies use an owned dispatcher. Cancel suppresses a callback
/// not yet selected. Drop cancels and joins, except on its own dispatcher.
/// User code already running is not preempted.
pub struct CallbackRequest {
    rx: Receiver<Result<(), Error>>,
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl CallbackRequest {
    /// Observe completion once. This bound does not cancel an admitted request.
    pub fn wait(&self, timeout: Duration) -> Result<(), Error> {
        self.rx.recv_timeout(timeout).map_err(receive_error)?
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
impl Drop for CallbackRequest {
    fn drop(&mut self) {
        self.cancel();
        join_callback(self.worker.take());
    }
}
fn callback_types(request: &str, response: &str) -> Result<(), Error> {
    if request.is_empty() || response.is_empty() {
        return Err(Error::InvalidFrame);
    }
    if response == EMPTY_TYPE {
        return Err(Error::Unsupported("response callback for gz.msgs.Empty"));
    }
    Ok(())
}
fn callback_progress(
    sender: &Sender,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Duration, Error> {
    if sender.stop.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
        return Err(Error::Shutdown);
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(Error::Timeout);
    }
    Ok(remaining.min(Duration::from_millis(20)))
}
fn run_response_callback<F>(callback: F, response: (Vec<u8>, bool), max: usize) -> Result<(), Error>
where
    F: FnOnce(Vec<u8>, bool),
{
    if response.0.len() > max {
        return Err(Error::InvalidFrame);
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        callback(response.0, response.1)
    }))
    .map_err(|_| Error::Native("response callback panicked".into()))
}

impl Service {
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Request, Error> {
        if self.owner_dead.load(Ordering::Acquire) {
            return self.rx.try_recv().map_err(|_| Error::Shutdown);
        }
        self.rx.recv_timeout(timeout).map_err(receive_error)
    }
}
/// An admitted request whose response can be awaited by its owner.
/// Dropping the handle abandons the reply; the worker retires its bounded
/// pending entry on reply or at the original deadline.
pub struct PendingResponse {
    rx: Receiver<Result<(Vec<u8>, bool), Error>>,
}
impl PendingResponse {
    /// Wait once for the response, a typed worker error, or this caller's bound.
    pub fn wait(self, timeout: Duration) -> Result<(Vec<u8>, bool), Error> {
        self.rx.recv_timeout(timeout).map_err(receive_error)?
    }
}
pub struct Request {
    pub data: Vec<u8>,
    sender: Sender,
    route: Vec<Vec<u8>>,
    max: usize,
    owner_dead: Arc<AtomicBool>,
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
        if self.owner_dead.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        self.sender
            .send_response(Command::Respond(self.route, data, success))
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

#[derive(PartialEq, Eq)]
struct MessageDomain {
    partition: String,
    interface: Ipv4Addr,
    multicast: Ipv4Addr,
    port: u16,
    statistics_wire: bool,
}
impl MessageDomain {
    fn from_service_config(config: &Config) -> Self {
        Self {
            port: config.service_port,
            ..Self::from_config(config)
        }
    }
    fn from_config(config: &Config) -> Self {
        let partition = config.partition.trim_matches('/');
        Self {
            partition: if partition.is_empty() {
                String::new()
            } else {
                format!("/{partition}")
            },
            interface: config.interface,
            multicast: config.multicast,
            port: config.message_port,
            statistics_wire: config.statistics_wire,
        }
    }
}
#[derive(Clone)]
struct PublicationHandle {
    active: Weak<AtomicBool>,
    sequence: Option<Arc<AtomicU64>>,
    recent_frames: Arc<Mutex<VecDeque<(u64, usize, Instant)>>>,
}
fn statistics_metadata(
    sequence: &AtomicU64,
    now: Result<Duration, Error>,
) -> Result<[u8; 16], Error> {
    let stamp = u64::try_from(now?.as_millis()).map_err(|_| Error::InvalidFrame)?;
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(&stamp.to_ne_bytes());
    bytes[8..].copy_from_slice(&sequence.fetch_add(1, Ordering::Relaxed).to_ne_bytes());
    Ok(bytes)
}
struct LocalPublication {
    domain: MessageDomain,
    topic: String,
    key: PeerKey,
    type_name: String,
    publication: PublicationHandle,
    stopped: Weak<AtomicBool>,
    retired_at: Option<Instant>,
}
fn frame_fingerprint(data: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}
impl LocalPublication {
    fn live(&self) -> bool {
        self.publication
            .active
            .upgrade()
            .is_some_and(|active| active.load(Ordering::Acquire))
            && self
                .stopped
                .upgrade()
                .is_some_and(|stopped| !stopped.load(Ordering::Acquire))
    }
    fn retain_for_peer_expiry(&mut self) -> bool {
        // Keep a retired key through the worker's three-second peer window so
        // an old multicast copy cannot resurrect a dropped local endpoint.
        // Frames are appended by this publication's worker in timestamp order.
        let now = Instant::now();
        let mut recent = self
            .publication
            .recent_frames
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while recent.front().is_some_and(|(_, _, stamp)| {
            now.saturating_duration_since(*stamp) >= Duration::from_secs(3)
        }) {
            recent.pop_front();
        }
        drop(recent);
        if self.live() {
            self.retired_at = None;
            true
        } else {
            self.retired_at.get_or_insert_with(Instant::now).elapsed() < Duration::from_secs(3)
        }
    }
}
static LOCAL_PUBLICATIONS: OnceLock<Mutex<Vec<LocalPublication>>> = OnceLock::new();
fn local_publications() -> &'static Mutex<Vec<LocalPublication>> {
    LOCAL_PUBLICATIONS.get_or_init(|| Mutex::new(Vec::new()))
}
struct LocalInterest {
    domain: MessageDomain,
    topic: String,
    key: SubscriberInterestKey,
    active: Weak<AtomicBool>,
    stopped: Weak<AtomicBool>,
    owner_dead: Weak<AtomicBool>,
    tx: Option<SyncSender<ReceivedMessage>>,
    throttle: Option<Arc<SubscriptionThrottle>>,
    retired_at: Option<Instant>,
}
impl LocalInterest {
    fn live(&self) -> bool {
        self.active
            .upgrade()
            .is_some_and(|active| active.load(Ordering::Acquire))
            && self
                .stopped
                .upgrade()
                .is_some_and(|stopped| !stopped.load(Ordering::Acquire))
            && self
                .owner_dead
                .upgrade()
                .is_some_and(|dead| !dead.load(Ordering::Acquire))
    }
    fn retain_for_peer_expiry(&mut self) -> bool {
        if self.live() {
            self.retired_at = None;
            true
        } else {
            self.retired_at.get_or_insert_with(Instant::now).elapsed() < Duration::from_secs(3)
        }
    }
}
static LOCAL_INTERESTS: OnceLock<Mutex<Vec<LocalInterest>>> = OnceLock::new();
static LOCAL_INTEREST_GENERATION: AtomicU64 = AtomicU64::new(0);
fn local_interests() -> &'static Mutex<Vec<LocalInterest>> {
    LOCAL_INTERESTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Publishers can retain the worker after ordinary Node destruction. Explicit
/// shutdown still cancels and joins it, including when Publisher handles live.
pub struct Node {
    sender: Sender,
    lifetime: Arc<WorkerLifetime>,
    owner_dead: Arc<AtomicBool>,
    local_interest: Arc<Mutex<LocalInterestCache>>,
    config: Config,
    topics: Mutex<HashMap<String, Weak<AtomicBool>>>,
    subscriptions: Mutex<Vec<(String, Weak<AtomicBool>)>>,
    services: Arc<Mutex<Vec<LocalService>>>,
}
impl Node {
    fn identity(&self) -> Result<&WorkerIdentity, Error> {
        self.sender
            .identity
            .get()
            .ok_or_else(|| Error::Native("worker identity unavailable".into()))
    }
    pub fn new(config: Config) -> Result<Self, Error> {
        config.validate()?;
        let (tx, rx) = mpsc::sync_channel(config.capacity);
        let (response_tx, response_rx) = mpsc::sync_channel(config.capacity.min(64));
        let sender = Sender {
            tx,
            response_tx,
            stop: Arc::new(AtomicBool::new(false)),
            slots: Arc::new(AtomicUsize::new(0)),
            callback_threads: Arc::new(AtomicUsize::new(0)),
            callback_requests: Arc::new(AtomicUsize::new(0)),
            capacity: config.capacity,
            counters: Arc::new(Counters::default()),
            identity: Arc::new(OnceLock::new()),
            publication_sequences: config
                .statistics_wire
                .then(|| Arc::new(Mutex::new(HashMap::new()))),
        };
        let owner_dead = Arc::new(AtomicBool::new(false));
        let state = Worker::new(
            config.clone(),
            sender.clone(),
            owner_dead.clone(),
            response_rx,
        )?;
        let services = sender
            .identity
            .get()
            .expect("Worker::new initialized identity")
            .services
            .clone();
        let stop = sender.stop.clone();
        let state_stop = sender.stop.clone();
        let worker = thread::Builder::new()
            .name("gz-transport-io".into())
            .spawn(move || state.run(rx, stop))?;
        Ok(Self {
            sender,
            lifetime: Arc::new(WorkerLifetime {
                stop: state_stop,
                worker: Mutex::new(Some(worker)),
            }),
            owner_dead,
            local_interest: Arc::new(Mutex::new(LocalInterestCache::default())),
            config,
            topics: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(Vec::new()),
            services,
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
        self.advertise_with_options(topic, type_name, AdvertiseOptions::default())
    }
    /// Advertise a canonical message with an explicit discovery scope.
    pub fn advertise_typed_with_options<M: gz_msgs::CanonicalMessage>(
        &self,
        topic: &str,
        options: AdvertiseOptions,
    ) -> Result<Publisher, Error> {
        self.advertise_with_options(topic, M::PROTOBUF_NAME, options)
    }
    /// Advertise message bytes with an explicit discovery scope.
    pub fn advertise_with_options(
        &self,
        topic: &str,
        type_name: &str,
        options: AdvertiseOptions,
    ) -> Result<Publisher, Error> {
        let topic = self.remapped_topic(topic)?;
        let mut topics = self
            .topics
            .lock()
            .map_err(|_| Error::Native("publisher registry poisoned".into()))?;
        topics.retain(|_, active| active.upgrade().is_some_and(|a| a.load(Ordering::Acquire)));
        if topics.contains_key(&topic) {
            return Err(Error::AlreadyAdvertised);
        }
        if self.config.statistics_wire && self.sender.stop.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        // Retain history for this worker's wire-process lifetime. Keeping the
        // guard through registration commits no new history on admission failure.
        let mut sequences = self
            .sender
            .publication_sequences
            .as_ref()
            .map(|history| {
                history
                    .lock()
                    .map_err(|_| Error::Native("statistics sequence registry poisoned".into()))
            })
            .transpose()?;
        let sequence = if let Some(history) = sequences.as_ref() {
            if let Some(sequence) = history.get(&topic) {
                Some(sequence.clone())
            } else if history.len() < self.config.capacity {
                Some(Arc::new(AtomicU64::new(0)))
            } else {
                self.sender
                    .counters
                    .queue_drops
                    .fetch_add(1, Ordering::Relaxed);
                return Err(Error::QueueFull);
            }
        } else {
            None
        };
        let lease = self.reserve()?;
        let interest = Arc::new(AtomicU8::new(0));
        self.sender.send(Command::Advertise(
            topic.clone(),
            type_name.into(),
            options,
            lease.active.clone(),
            interest.clone(),
        ))?;
        topics.insert(topic.clone(), Arc::downgrade(&lease.active));
        drop(topics);
        // Accepted for enqueue, not yet confirmed by the worker or network.
        let (_, path) = decompose_fully_qualified_topic(&topic)?;
        let publication = PublicationHandle {
            active: Arc::downgrade(&lease.active),
            sequence: sequence.clone(),
            recent_frames: Arc::new(Mutex::new(VecDeque::new())),
        };
        let mut local = local_publications()
            .lock()
            .map_err(|_| Error::Native("process-local publisher registry poisoned".into()))?;
        local.retain_mut(LocalPublication::retain_for_peer_expiry);
        let identity = self.identity()?;
        local.push(LocalPublication {
            domain: MessageDomain::from_config(&self.config),
            topic: path.into(),
            key: PeerKey {
                channel: 0,
                process: identity.process.clone(),
                node: identity.node.clone(),
                topic: topic.clone(),
                address: identity.address.clone(),
            },
            type_name: type_name.into(),
            // Echo tombstones retain only payload fingerprints, not this
            // worker's sequence ownership after all of its senders retire.
            publication: PublicationHandle {
                sequence: None,
                ..publication.clone()
            },
            stopped: Arc::downgrade(&self.sender.stop),
            retired_at: None,
        });
        let mut interests = self
            .local_interest
            .lock()
            .map_err(|_| Error::Native("local interest cache poisoned".into()))?;
        interests.publishers.push(LocalPublisherInterest {
            topic: topic.clone(),
            type_name: type_name.into(),
            active: Arc::downgrade(&lease.active),
            bits: Arc::downgrade(&interest),
        });
        interests.refresh();
        if let (Some(history), Some(sequence)) = (sequences.as_mut(), sequence) {
            history.insert(topic.clone(), sequence);
        }
        Ok(Publisher {
            _lease: lease,
            sender: self.sender.clone(),
            _lifetime: self.lifetime.clone(),
            interest,
            publication,
            topic,
            type_name: type_name.into(),
            max: self.config.max_message_bytes,
            reply_timeout: self.config.reply_timeout,
            throttle: PublicationThrottle::new(options.msgs_per_sec),
        })
    }
    pub fn subscribe(&self, topic: &str, type_name: &str) -> Result<Subscription, Error> {
        self.subscribe_with_options(topic, type_name, SubscribeOptions::default())
    }
    /// Subscribe with per-registration rate admission and a same-Node filter.
    /// Suppression keeps discovery visible. Clock errors drop admission and are
    /// counted on this Node; queue saturation still uses the queue-drop counter.
    pub fn subscribe_with_options(
        &self,
        topic: &str,
        type_name: &str,
        options: SubscribeOptions,
    ) -> Result<Subscription, Error> {
        let (tx, rx) = mpsc::sync_channel(self.config.capacity);
        let lease = self.reserve()?;
        let topic = self.remapped_topic(topic)?;
        let (_, path) = decompose_fully_qualified_topic(&topic)?;
        let identity = self.identity()?;
        let mut subscriptions = self
            .subscriptions
            .lock()
            .map_err(|_| Error::Native("subscription registry poisoned".into()))?;
        subscriptions.retain(|(_, a)| a.upgrade().is_some_and(|a| a.load(Ordering::Acquire)));
        let mut local = local_interests()
            .lock()
            .map_err(|_| Error::Native("process-local subscriber registry poisoned".into()))?;
        let mut interests = self
            .local_interest
            .lock()
            .map_err(|_| Error::Native("local interest cache poisoned".into()))?;
        let throttle =
            SubscriptionThrottle::new(options.msgs_per_sec, self.sender.counters.clone());
        self.sender.send(Command::Subscribe(
            topic.clone(),
            type_name.into(),
            tx.clone(),
            lease.active.clone(),
            options,
            throttle.clone(),
        ))?;
        subscriptions.push((topic.clone(), Arc::downgrade(&lease.active)));
        local.retain_mut(LocalInterest::retain_for_peer_expiry);
        local.push(LocalInterest {
            domain: MessageDomain::from_config(&self.config),
            topic: path.into(),
            key: SubscriberInterestKey {
                process: identity.process.clone(),
                node: identity.node.clone(),
                topic: topic.clone(),
                address: identity.address.clone(),
                type_name: if type_name.is_empty() {
                    "google.protobuf.Message".into()
                } else {
                    type_name.into()
                },
            },
            active: Arc::downgrade(&lease.active),
            stopped: Arc::downgrade(&self.sender.stop),
            owner_dead: Arc::downgrade(&self.owner_dead),
            tx: Some(tx),
            throttle,
            retired_at: None,
        });
        let _ = LOCAL_INTEREST_GENERATION.fetch_update(
            Ordering::Release,
            Ordering::Relaxed,
            |generation| generation.checked_add(1),
        );
        interests.subscribers.push(LocalSubscriberInterest {
            topic,
            type_name: type_name.into(),
            active: Arc::downgrade(&lease.active),
        });
        interests.refresh();
        Ok(Subscription {
            rx,
            _lease: lease,
            owner_dead: self.owner_dead.clone(),
            local_interest: self.local_interest.clone(),
        })
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
    /// Same-process live publishers are visible before multicast discovery. The
    /// worker snapshot does not wait for remote discovery. `advertised_topics`
    /// remains local handle inspection.
    pub fn known_publisher_topics(&self) -> Result<Vec<String>, Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender.send(Command::KnownPublisherTopics(tx))?;
        let mut records = rx
            .recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)?;
        self.merge_process_local_publications(&mut records)?;
        Ok(records.into_names())
    }
    /// Sorted paths with a known publisher or subscription interest.
    /// Live same-process publishers and subscriptions are immediate; a
    /// nonblocking subscriber query refreshes remote interests eventually.
    pub fn known_topics(&self) -> Result<Vec<String>, Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender.send(Command::KnownTopics(tx))?;
        let mut records = rx
            .recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)?;
        self.merge_process_local_publications(&mut records)?;
        let domain = MessageDomain::from_config(&self.config);
        let mut interests = local_interests()
            .lock()
            .map_err(|_| Error::Native("process-local subscriber registry poisoned".into()))?;
        interests.retain_mut(LocalInterest::retain_for_peer_expiry);
        for interest in interests
            .iter()
            .filter(|interest| interest.domain == domain)
        {
            records.subscribers.remove(&interest.key);
        }
        records.subscribers.extend(
            interests
                .iter()
                .filter(|interest| interest.domain == domain && interest.live())
                .map(|interest| (interest.key.clone(), interest.topic.clone())),
        );
        Ok(records.into_names())
    }
    fn merge_process_local_publications(
        &self,
        records: &mut TopicNameRecords,
    ) -> Result<(), Error> {
        let domain = MessageDomain::from_config(&self.config);
        let mut local = local_publications()
            .lock()
            .map_err(|_| Error::Native("process-local publisher registry poisoned".into()))?;
        local.retain_mut(LocalPublication::retain_for_peer_expiry);
        for publication in local
            .iter()
            .filter(|publication| publication.domain == domain)
        {
            records.publishers.remove(&publication.key);
        }
        records.publishers.extend(
            local
                .iter()
                .filter(|publication| publication.domain == domain && publication.live())
                .map(|publication| (publication.key.clone(), publication.topic.clone())),
        );
        Ok(())
    }
    /// Snapshot message descriptor names for known endpoints on an original topic.
    /// Same-process endpoints are immediate. Equal names from distinct
    /// endpoints remain separate; remote subscriber replies are eventual.
    pub fn known_topic_info(&self, topic: &str) -> Result<TopicEndpointTypes, Error> {
        let topic = self.topic(topic)?;
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .send(Command::KnownTopicInfo(topic.clone(), tx))?;
        let records = rx
            .recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)?;
        self.merge_topic_info(&topic, records)
    }
    /// Wait for message discovery startup, then snapshot this original topic.
    /// The timeout bounds startup and worker reply waits; zero returns `Timeout`. The IO worker
    /// keeps processing discovery and other commands while the query is pending.
    /// Startup is per Rust worker, not native process-global state. Initialization
    /// is not consensus, delivery readiness or complete remote subscriber replies.
    pub fn topic_info(&self, topic: &str, timeout: Duration) -> Result<TopicEndpointTypes, Error> {
        let topic = self.topic(topic)?;
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(Error::InvalidConfig)?;
        if timeout.is_zero() {
            return Err(Error::Timeout);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender.send(Command::ReadyTopicInfo(TopicInfoQuery {
            topic: topic.clone(),
            deadline,
            reply: tx,
        }))?;
        let records = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(receive_error)??;
        self.merge_topic_info(&topic, records)
    }
    fn merge_topic_info(
        &self,
        topic: &str,
        mut records: TopicEndpointRecords,
    ) -> Result<TopicEndpointTypes, Error> {
        let domain = MessageDomain::from_config(&self.config);
        {
            let mut publications = local_publications()
                .lock()
                .map_err(|_| Error::Native("process-local publisher registry poisoned".into()))?;
            publications.retain_mut(LocalPublication::retain_for_peer_expiry);
            for p in publications.iter().filter(|p| p.domain == domain) {
                records.publishers.remove(&p.key);
            }
            records.publishers.extend(
                publications
                    .iter()
                    .filter(|p| p.domain == domain && p.key.topic == topic && p.live())
                    .map(|p| (p.key.clone(), p.type_name.clone())),
            );
        }
        {
            let mut interests = local_interests()
                .lock()
                .map_err(|_| Error::Native("process-local subscriber registry poisoned".into()))?;
            interests.retain_mut(LocalInterest::retain_for_peer_expiry);
            for s in interests.iter().filter(|s| s.domain == domain) {
                records.subscribers.remove(&s.key);
            }
            records.subscribers.extend(
                interests
                    .iter()
                    .filter(|s| s.domain == domain && s.key.topic == topic && s.live())
                    .map(|s| (s.key.clone(), s.key.type_name.clone())),
            );
        }
        Ok(records.into_types())
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
        self.local_interest
            .lock()
            .map_err(|_| Error::Native("local interest cache poisoned".into()))?
            .refresh();
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
        self.serve_with_options(
            topic,
            request_type,
            response_type,
            ServiceOptions::default(),
        )
    }
    /// Admit a queued service with immutable discovery scope. Process scope is
    /// reachable by admitted local routing; inspection remains worker-only.
    pub fn serve_with_options(
        &self,
        topic: &str,
        request_type: &str,
        response_type: &str,
        options: ServiceOptions,
    ) -> Result<Service, Error> {
        let topic = self.remapped_topic(topic)?;
        let mut services = self
            .services
            .lock()
            .map_err(|_| Error::Native("service registry poisoned".into()))?;
        services.retain(|service| {
            service
                .active
                .upgrade()
                .is_some_and(|a| a.load(Ordering::Acquire))
        });
        if services.iter().any(|service| service.topic == topic) {
            return Err(Error::AlreadyAdvertised);
        }
        let (tx, rx) = mpsc::sync_channel(self.config.capacity);
        let lease = self.reserve()?;
        let tx = Arc::new(tx);
        let queue = Arc::new(LocalServiceQueue {
            tx: Arc::downgrade(&tx),
            sender: self.sender.clone(),
            active: Arc::downgrade(&lease.active),
            owner_dead: self.owner_dead.clone(),
            max: self.config.max_message_bytes,
        });
        self.sender.send(Command::Serve(
            topic.clone(),
            request_type.into(),
            response_type.into(),
            options.scope,
            tx,
            lease.active.clone(),
        ))?;
        services.push(LocalService {
            topic,
            types: ServiceTypes {
                request_type: request_type.into(),
                response_type: response_type.into(),
            },
            active: Arc::downgrade(&lease.active),
            callback: None,
            queue: Arc::downgrade(&queue),
        });
        Ok(Service {
            rx,
            _lease: lease,
            owner_dead: self.owner_dead.clone(),
            _queue: queue,
        })
    }
    /// Register a typed one-way callback. Matching live callbacks in this
    /// process and service domain execute on the requesting thread. Explicit
    /// queued/wire requests use an owned dispatcher. User code runs outside
    /// registry locks; panics become typed invocation errors.
    pub fn serve_oneway_callback<F>(
        &self,
        topic: &str,
        request_type: &str,
        callback: F,
    ) -> Result<OneWayCallbackService, Error>
    where
        F: Fn(&[u8]) -> Result<(), Error> + Send + Sync + 'static,
    {
        self.serve_oneway_with_spawner(topic, request_type, callback, spawn_callback)
    }
    /// Register a one-way callback with immutable service discovery scope.
    pub fn serve_oneway_callback_with_options<F>(
        &self,
        topic: &str,
        request_type: &str,
        options: ServiceOptions,
        callback: F,
    ) -> Result<OneWayCallbackService, Error>
    where
        F: Fn(&[u8]) -> Result<(), Error> + Send + Sync + 'static,
    {
        self.serve_oneway_options_with_spawner(
            topic,
            request_type,
            options,
            callback,
            spawn_callback,
        )
    }
    fn serve_oneway_with_spawner<F>(
        &self,
        topic: &str,
        request_type: &str,
        callback: F,
        spawn: CallbackSpawner,
    ) -> Result<OneWayCallbackService, Error>
    where
        F: Fn(&[u8]) -> Result<(), Error> + Send + Sync + 'static,
    {
        self.serve_oneway_options_with_spawner(
            topic,
            request_type,
            ServiceOptions::default(),
            callback,
            spawn,
        )
    }
    fn serve_oneway_options_with_spawner<F>(
        &self,
        topic: &str,
        request_type: &str,
        options: ServiceOptions,
        callback: F,
        spawn: CallbackSpawner,
    ) -> Result<OneWayCallbackService, Error>
    where
        F: Fn(&[u8]) -> Result<(), Error> + Send + Sync + 'static,
    {
        let qualified = self.remapped_topic(topic)?;
        // Preserve the established duplicate error even at capacity one.
        {
            let services = self
                .services
                .lock()
                .map_err(|_| Error::Native("service registry poisoned".into()))?;
            if services.iter().any(|service| {
                service.topic == qualified
                    && service
                        .active
                        .upgrade()
                        .is_some_and(|active| active.load(Ordering::Acquire))
            }) {
                return Err(Error::AlreadyAdvertised);
            }
        }
        let permit = self.sender.callback_permit(&self.sender.callback_threads)?;
        let service = self.serve_with_options(topic, request_type, EMPTY_TYPE, options)?;
        let state = Arc::new(OneWayCallbackState {
            callback: Box::new(callback),
            active: service._lease.active.clone(),
            slots: service._lease.slots.clone(),
            owner_dead: self.owner_dead.clone(),
            sender: self.sender.clone(),
            max: self.config.max_message_bytes,
            ready: AtomicBool::new(false),
        });
        let dispatch = state.clone();
        let worker = spawn(Box::new(move || {
            let _permit = permit;
            while dispatch.live() && !dispatch.ready.load(Ordering::Acquire) {
                thread::park_timeout(Duration::from_millis(20));
            }
            while dispatch.live() {
                match service.recv_timeout(Duration::from_millis(20)) {
                    Ok(request) if !request.expects_response() => {
                        if dispatch.invoke(&request.data).is_err() {
                            dispatch.record_error();
                        }
                    }
                    Ok(_) => dispatch.record_error(),
                    Err(Error::Timeout) => {}
                    Err(_) => break,
                }
            }
            drop(service);
            drop(dispatch);
        }))?;
        let handle = OneWayCallbackService {
            state,
            worker: Some(worker),
        };
        self.set_local_callback(
            &handle.state.active,
            LocalCallback::OneWay(Arc::downgrade(&handle.state)),
        )?;
        handle.state.ready.store(true, Ordering::Release);
        handle.worker.as_ref().unwrap().thread().unpark();
        Ok(handle)
    }
    /// Register a response service callback. Exact local `request_callback`
    /// calls execute on the requesting thread; queued/wire calls execute on an
    /// owned bounded dispatcher. User code runs without transport registry locks.
    /// Empty responses use the separate one-way API. Panics become errors.
    pub fn serve_callback<F>(
        &self,
        topic: &str,
        request_type: &str,
        response_type: &str,
        callback: F,
    ) -> Result<CallbackService, Error>
    where
        F: Fn(&[u8]) -> Result<(Vec<u8>, bool), Error> + Send + Sync + 'static,
    {
        self.serve_callback_with_spawner(
            topic,
            request_type,
            response_type,
            callback,
            spawn_callback,
        )
    }
    /// Register an owned response callback with immutable discovery scope.
    pub fn serve_callback_with_options<F>(
        &self,
        topic: &str,
        request_type: &str,
        response_type: &str,
        options: ServiceOptions,
        callback: F,
    ) -> Result<CallbackService, Error>
    where
        F: Fn(&[u8]) -> Result<(Vec<u8>, bool), Error> + Send + Sync + 'static,
    {
        self.serve_callback_options_with_spawner(
            topic,
            request_type,
            response_type,
            options,
            callback,
            spawn_callback,
        )
    }
    fn serve_callback_with_spawner<F>(
        &self,
        topic: &str,
        request_type: &str,
        response_type: &str,
        callback: F,
        spawn: CallbackSpawner,
    ) -> Result<CallbackService, Error>
    where
        F: Fn(&[u8]) -> Result<(Vec<u8>, bool), Error> + Send + Sync + 'static,
    {
        self.serve_callback_options_with_spawner(
            topic,
            request_type,
            response_type,
            ServiceOptions::default(),
            callback,
            spawn,
        )
    }
    fn serve_callback_options_with_spawner<F>(
        &self,
        topic: &str,
        request_type: &str,
        response_type: &str,
        options: ServiceOptions,
        callback: F,
        spawn: CallbackSpawner,
    ) -> Result<CallbackService, Error>
    where
        F: Fn(&[u8]) -> Result<(Vec<u8>, bool), Error> + Send + Sync + 'static,
    {
        self.remapped_topic(topic)?;
        callback_types(request_type, response_type)?;
        let permit = self.sender.callback_permit(&self.sender.callback_threads)?;
        let service = self.serve_with_options(topic, request_type, response_type, options)?;
        let state = Arc::new(ResponseCallbackState {
            callback: Box::new(callback),
            active: service._lease.active.clone(),
            owner_dead: self.owner_dead.clone(),
            sender: self.sender.clone(),
            max: self.config.max_message_bytes,
            ready: AtomicBool::new(false),
        });
        let dispatch = state.clone();
        let worker = spawn(Box::new(move || {
            let _permit = permit;
            // Do not invoke user code until local registry admission commits.
            while dispatch.live() && !dispatch.ready.load(Ordering::Acquire) {
                thread::park_timeout(Duration::from_millis(20));
            }
            while dispatch.live() {
                match service.recv_timeout(Duration::from_millis(20)) {
                    Ok(request) => {
                        let result = dispatch.invoke(&request.data);
                        let reply = match result {
                            Ok(reply) => reply,
                            Err(_) => {
                                dispatch
                                    .sender
                                    .counters
                                    .routing_errors
                                    .fetch_add(1, Ordering::Relaxed);
                                (Vec::new(), false)
                            }
                        };
                        if request.respond(reply.0, reply.1).is_err() {
                            dispatch
                                .sender
                                .counters
                                .routing_errors
                                .fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Err(Error::Timeout) => {}
                    Err(_) => break,
                }
            }
            drop(service);
            drop(dispatch);
        }))?;
        let handle = CallbackService {
            state,
            worker: Some(worker),
        };
        self.set_local_callback(
            &handle.state.active,
            LocalCallback::Response(Arc::downgrade(&handle.state)),
        )?;
        handle.state.ready.store(true, Ordering::Release);
        handle.worker.as_ref().unwrap().thread().unpark();
        Ok(handle)
    }
    fn set_local_callback(
        &self,
        active: &Arc<AtomicBool>,
        callback: LocalCallback,
    ) -> Result<(), Error> {
        let mut services = self
            .services
            .lock()
            .map_err(|_| Error::Native("service registry poisoned".into()))?;
        let lease = Arc::downgrade(active);
        let entry = services
            .iter_mut()
            .find(|entry| entry.active.ptr_eq(&lease))
            .ok_or(Error::Shutdown)?;
        if !active.load(Ordering::Acquire)
            || self.sender.stop.load(Ordering::Acquire)
            || self.owner_dead.load(Ordering::Acquire)
        {
            return Err(Error::Shutdown);
        }
        entry.callback = Some(callback);
        Ok(())
    }
    fn local_callback(
        &self,
        topic: &str,
        request_type: &str,
        response_type: &str,
    ) -> Result<Option<CallbackState>, Error> {
        let domain = &self.identity()?.service_domain;
        for owner in service_workers(local_service_workers())? {
            let Some(owner) = owner.get() else { continue };
            if &owner.service_domain != domain {
                continue;
            }
            // Only weak references cross the lock boundary. Upgrading or
            // dropping the last callback may run a user-owned destructor.
            let callback = {
                let services = owner
                    .services
                    .lock()
                    .map_err(|_| Error::Native("service registry poisoned".into()))?;
                services
                    .iter()
                    .find(|entry| {
                        entry.topic == topic
                            && entry.types.request_type == request_type
                            && entry.types.response_type == response_type
                    })
                    .and_then(|entry| entry.callback.clone())
            };
            if let Some(callback) = callback.and_then(|callback| callback.upgrade()) {
                return Ok(Some(callback));
            }
        }
        Ok(None)
    }
    /// Admit a response callback. A live exact local callback runs both user
    /// functions before return, once, without enqueueing a duplicate request.
    /// Other providers use the ordinary bounded queue and an owned dispatcher.
    /// Validation and admission precede user code. Timeouts/cancellation cannot
    /// preempt a user function already selected; completion reports its outcome.
    pub fn request_callback<F>(
        &self,
        topic: &str,
        request_type: &str,
        response_type: &str,
        data: impl Into<Vec<u8>>,
        timeout: Duration,
        callback: F,
    ) -> Result<CallbackRequest, Error>
    where
        F: FnOnce(Vec<u8>, bool) + Send + 'static,
    {
        self.request_callback_with_spawner(
            topic,
            request_type,
            response_type,
            data.into(),
            timeout,
            callback,
            spawn_callback,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn request_callback_with_spawner<F>(
        &self,
        topic: &str,
        request_type: &str,
        response_type: &str,
        data: Vec<u8>,
        timeout: Duration,
        callback: F,
        spawn: CallbackSpawner,
    ) -> Result<CallbackRequest, Error>
    where
        F: FnOnce(Vec<u8>, bool) + Send + 'static,
    {
        let topic_name = self.remapped_topic(topic)?;
        callback_types(request_type, response_type)?;
        if data.len() > self.config.max_message_bytes {
            return Err(Error::InvalidFrame);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(Error::InvalidConfig)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        callback_progress(&self.sender, &cancelled, deadline)?;
        let permit = self
            .sender
            .callback_permit(&self.sender.callback_requests)?;
        let (done, rx) = mpsc::sync_channel(1);
        if let Some(CallbackState::Response(service)) =
            self.local_callback(&topic_name, request_type, response_type)?
        {
            // Selection is final: a retirement or user error after this point
            // completes this request and never retries through the worker.
            callback_progress(&self.sender, &cancelled, deadline)?;
            if data.len() > service.max {
                return Err(Error::InvalidFrame);
            }
            let result = service.invoke(&data).and_then(|response| {
                callback_progress(&self.sender, &cancelled, deadline)?;
                run_response_callback(callback, response, self.config.max_message_bytes)
            });
            // Last captured user values may block or unwind during destruction.
            // Keep admission until that cleanup finishes, outside registry locks.
            drop(service);
            let _ = done.try_send(result);
            drop(permit);
            return Ok(CallbackRequest {
                rx,
                cancelled,
                worker: None,
            });
        }
        let thread_permit = self.sender.callback_permit(&self.sender.callback_threads)?;
        let (work, incoming) = mpsc::sync_channel::<PendingResponse>(1);
        let stop = cancelled.clone();
        let sender = self.sender.clone();
        let max = self.config.max_message_bytes;
        // Spawn before transport admission, so a failed spawn cannot orphan an
        // admitted request. Inline completion never creates this thread.
        let worker = spawn(Box::new(move || {
            let _permit = permit;
            let _thread_permit = thread_permit;
            let result = (|| {
                let pending = loop {
                    let poll = callback_progress(&sender, &stop, deadline)?;
                    match incoming.recv_timeout(poll) {
                        Ok(pending) => break pending,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(error) => return Err(receive_error(error)),
                    }
                };
                let response = loop {
                    let poll = callback_progress(&sender, &stop, deadline)?;
                    match pending.rx.recv_timeout(poll) {
                        Ok(response) => break response?,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(error) => return Err(receive_error(error)),
                    }
                };
                callback_progress(&sender, &stop, deadline)?;
                run_response_callback(callback, response, max)
            })();
            let _ = done.try_send(result);
        }))?;
        let handle = CallbackRequest {
            rx,
            cancelled,
            worker: Some(worker),
        };
        let pending = self.begin_request(
            topic,
            request_type,
            response_type,
            data,
            deadline.saturating_duration_since(Instant::now()),
        )?;
        // A dispatcher can expire before handoff. The request remains admitted
        // and expires normally; the handle reports timeout, without a callback.
        let _ = work.try_send(pending);
        Ok(handle)
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
        Ok(active_topics(
            services
                .iter()
                .map(|service| (&service.topic, &service.active)),
        ))
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
    /// Wait for discovery startup, then snapshot sorted known service paths.
    /// The explicit timeout bounds startup and the worker reply; zero returns
    /// `Timeout`. Startup belongs to this worker and does not prove that all
    /// remote services have been discovered. `known_services` remains immediate.
    pub fn service_list(&self, timeout: Duration) -> Result<Vec<String>, Error> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(Error::InvalidConfig)?;
        if timeout.is_zero() {
            return Err(Error::Timeout);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .send(Command::ReadyServiceList(ServiceListQuery {
                deadline,
                reply: tx,
            }))?;
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(receive_error)?
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
        services.retain(|service| {
            if service.topic == topic {
                if let Some(active) = service.active.upgrade() {
                    release(&active, &self.sender.slots);
                }
                false
            } else {
                true
            }
        });
        Ok(())
    }
    /// Hand a one-way request to a matching local callback, queue or native peer.
    /// Same-domain callbacks in this process finish on this caller before return.
    /// The timeout bounds queued discovery and handoff. Inline calls reject
    /// zero/overflowing timeouts but add no execution deadline to user code.
    /// A queued handoff
    /// does not acknowledge execution, and no response is sent on the wire.
    pub fn request_oneway(
        &self,
        topic: &str,
        request_type: &str,
        data: impl Into<Vec<u8>>,
        timeout: Duration,
    ) -> Result<(), Error> {
        let data: Vec<u8> = data.into();
        if data.len() > self.config.max_message_bytes {
            return Err(Error::InvalidFrame);
        }
        let topic_name = self.remapped_topic(topic)?;
        Instant::now()
            .checked_add(timeout)
            .ok_or(Error::InvalidConfig)?;
        if self.sender.stop.load(Ordering::Acquire) {
            return Err(Error::Shutdown);
        }
        if timeout.is_zero() {
            return Err(Error::Timeout);
        }
        // Lookup can destroy a retired candidate's last user capture as well.
        // Admission must cover that cleanup, not just a selected invocation.
        let permit = self
            .sender
            .callback_permit(&self.sender.callback_requests)?;
        // Preserve this Node's receiver before considering other Nodes. A plain
        // local service keeps its queued path; only weak references cross locks.
        let own = {
            let services = self
                .services
                .lock()
                .map_err(|_| Error::Native("service registry poisoned".into()))?;
            services
                .iter()
                .find(|service| {
                    service.topic == topic_name
                        && service.types.request_type == request_type
                        && service.types.response_type == EMPTY_TYPE
                        && service
                            .active
                            .upgrade()
                            .is_some_and(|active| active.load(Ordering::Acquire))
                })
                .map(|service| service.callback.clone())
        };
        let callback = match own {
            Some(callback) => callback.and_then(|callback| callback.upgrade()),
            None => self.local_callback(&topic_name, request_type, EMPTY_TYPE)?,
        };
        if let Some(CallbackState::OneWay(callback)) = callback {
            if self.sender.stop.load(Ordering::Acquire) {
                return Err(Error::Shutdown);
            }
            if data.len() > callback.max {
                return Err(Error::InvalidFrame);
            }
            let result = callback.invoke(&data);
            if result.is_err() {
                callback.record_error();
            }
            drop(callback);
            drop(permit);
            return result;
        }
        self.request(topic, request_type, EMPTY_TYPE, data, timeout)
            .map(|_| ())
    }
    /// Admit a response request without waiting for its result. The same bounded
    /// worker queue, provider matching, and deadline apply as in `request`.
    /// An admitted request can still complete with `Timeout` or another error.
    pub fn begin_request(
        &self,
        topic: &str,
        request_type: &str,
        response_type: &str,
        data: impl Into<Vec<u8>>,
        timeout: Duration,
    ) -> Result<PendingResponse, Error> {
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
        Ok(PendingResponse { rx })
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
        self.begin_request(topic, request_type, response_type, data, timeout)?
            .wait(timeout)
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
    /// Snapshot owned metadata for each known provider of this original topic.
    /// Records are sorted without collapsing equal signatures. This method does
    /// not wait for startup or convergence; `service_list` can prepare service
    /// discovery first. Addresses and identities are observations, not leases.
    pub fn known_service_info(&self, topic: &str) -> Result<Vec<ServiceProviderInfo>, Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .send(Command::KnownServiceInfo(self.topic(topic)?, tx))?;
        rx.recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)
    }
    /// Snapshot owned publication metadata for an original topic, without waiting
    /// for discovery startup. Other Nodes are visible only after this worker has
    /// discovered their advertisements. Subscriber metadata is not included.
    /// Effective rate options follow native MessagePublisher interpretation.
    pub fn known_topic_publishers(&self, topic: &str) -> Result<Vec<TopicPublisherInfo>, Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .send(Command::KnownTopicPublishers(self.topic(topic)?, tx))?;
        rx.recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)
    }
    /// Snapshot owned metadata from this worker's live subscriptions and remote
    /// wire records. Remote storage keeps the first record per topic/process/
    /// address/node; legacy descriptor views remain type-aware. This query does
    /// not refresh discovery or merge other Nodes' process-local registrations.
    pub fn known_topic_subscribers(&self, topic: &str) -> Result<Vec<TopicSubscriberInfo>, Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .send(Command::KnownTopicSubscribers(self.topic(topic)?, tx))?;
        rx.recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)
    }
    fn service_resolution(
        &self,
        topic: &str,
        request: &str,
        response: &str,
    ) -> Result<ServiceResolution, Error> {
        let qualified = self.topic(topic)?;
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender
            .send(Command::ResolveServiceSignatures(qualified.clone(), tx))?;
        let mut records = rx
            .recv_timeout(self.config.reply_timeout)
            .map_err(receive_error)?;
        let owners = service_workers(local_service_workers())?;
        let domain = MessageDomain::from_service_config(&self.config);
        let mut result = ServiceResolution::default();
        for owner in owners {
            let identity = owner.get().expect("registered worker identity");
            if identity.service_domain != domain {
                continue;
            }
            // Local admitted state is authoritative even after lease retirement;
            // a delayed multicast copy must not resurrect that local provider.
            records
                .providers
                .retain(|(key, _)| key.process != identity.process);
            if identity
                .stopped
                .upgrade()
                .is_none_or(|stopped| stopped.load(Ordering::Acquire))
            {
                continue;
            }
            let mut services = identity
                .services
                .lock()
                .map_err(|_| Error::Native("service registry poisoned".into()))?;
            services.retain(|service| {
                service
                    .active
                    .upgrade()
                    .is_some_and(|active| active.load(Ordering::Acquire))
            });
            for service in services.iter().filter(|service| service.topic == qualified) {
                result.observe(&service.types, request, response);
            }
        }
        for (_, types) in records.providers {
            result.observe(&types, request, response);
        }
        Ok(result)
    }
    /// Resolve empty descriptor names from matching providers. Explicit names
    /// select providers; both explicit names return immediately without lookup.
    /// Live admitted services in the same local service domain participate before
    /// their multicast copies arrive; public worker-only inspection is unchanged.
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
            let observed = self.service_resolution(topic, request_type, response_type)?;
            if let Some(types) = observed.first {
                return Ok(if observed.ambiguous {
                    AmbiguousTypes
                } else {
                    Resolved(types)
                });
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(if observed.any_provider {
                    IncompatibleTypes
                } else {
                    NoProviders
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
        self.lifetime.stop_and_join();
    }
    /// Cancel all pending operations; the owner joins the worker on shutdown/drop.
    pub fn cancel(&self) {
        self.sender.stop.store(true, Ordering::Release);
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        self.owner_dead.store(true, Ordering::Release);
        for (_, active) in self
            .subscriptions
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
            if let Some(active) = active.upgrade() {
                release(&active, &self.sender.slots);
            }
        }
        for service in self
            .services
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
            if let Some(active) = service.active.upgrade() {
                release(&active, &self.sender.slots);
            }
        }
        self.local_interest
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .refresh();
        if self.sender.stop.load(Ordering::Acquire) || Arc::strong_count(&self.lifetime) == 1 {
            self.lifetime.stop_and_join();
        }
    }
}

enum Command {
    Advertise(
        String,
        String,
        AdvertiseOptions,
        Arc<AtomicBool>,
        Arc<AtomicU8>,
    ),
    KnownPublisherTopics(SyncSender<TopicNameRecords>),
    KnownTopics(SyncSender<TopicNameRecords>),
    KnownTopicInfo(String, SyncSender<TopicEndpointRecords>),
    ReadyTopicInfo(TopicInfoQuery),
    Publish(String, String, Vec<u8>, Option<PublicationHandle>),
    Subscribe(
        String,
        String,
        SyncSender<ReceivedMessage>,
        Arc<AtomicBool>,
        SubscribeOptions,
        Option<Arc<SubscriptionThrottle>>,
    ),
    Unsubscribe(String),
    Serve(
        String,
        String,
        String,
        Scope,
        Arc<SyncSender<Request>>,
        Arc<AtomicBool>,
    ),
    KnownServices(SyncSender<Vec<String>>),
    ReadyServiceList(ServiceListQuery),
    ServiceSignatures(String, SyncSender<Vec<ServiceTypes>>),
    ResolveServiceSignatures(String, SyncSender<ServiceResolutionRecords>),
    ServiceProviderTypes(String, SyncSender<Vec<ServiceTypes>>),
    KnownServiceInfo(String, SyncSender<Vec<ServiceProviderInfo>>),
    KnownTopicPublishers(String, SyncSender<Vec<TopicPublisherInfo>>),
    KnownTopicSubscribers(String, SyncSender<Vec<TopicSubscriberInfo>>),
    HasConnections(String, String, SyncSender<bool>),
    RefreshConnections(
        String,
        String,
        Arc<AtomicBool>,
        Arc<AtomicU8>,
        Instant,
        SyncSender<Result<bool, Error>>,
    ),
    Request(Pending),
    Respond(Vec<Vec<u8>>, Vec<u8>, bool),
}
const EMPTY_TYPE: &str = "gz.msgs.Empty";

struct TopicInfoQuery {
    topic: String,
    deadline: Instant,
    reply: SyncSender<Result<TopicEndpointRecords, Error>>,
}
#[derive(Default)]
struct ServiceResolution {
    any_provider: bool,
    first: Option<ServiceTypes>,
    ambiguous: bool,
}
impl ServiceResolution {
    fn observe(&mut self, types: &ServiceTypes, request: &str, response: &str) {
        self.any_provider = true;
        if !(request.is_empty() || request == types.request_type)
            || !(response.is_empty() || response == types.response_type)
        {
            return;
        }
        if let Some(first) = &self.first {
            self.ambiguous |= first != types;
        } else {
            self.first = Some(types.clone());
        }
    }
}
struct ServiceResolutionRecords {
    providers: Vec<(PeerKey, ServiceTypes)>,
    // Keep source identities alive until the caller masks copied wire records.
    _owners: Vec<WorkerOwner>,
}
struct ServiceListQuery {
    deadline: Instant,
    reply: SyncSender<Result<Vec<String>, Error>>,
}
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
    options: SubscribeOptions,
    throttle: Option<Arc<SubscriptionThrottle>>,
}
struct ServiceState {
    publisher: WirePublisher,
    tx: Arc<SyncSender<Request>>,
    active: Arc<AtomicBool>,
}
fn send_local_request(
    services: &[ServiceState],
    sender: &Sender,
    owner_dead: &Arc<AtomicBool>,
    max: usize,
    id: &str,
    pending: &Pending,
) -> Result<bool, Error> {
    let Some(service) = services.iter().find(|service| {
        service.active.load(Ordering::Acquire)
            && service.publisher.topic == pending.topic
            && matches!(&service.publisher.pub_type, Some(PubType::SrvPub(types))
                if types.request_type == pending.request_type
                    && types.response_type == pending.response_type)
    }) else {
        return send_admitted_service_request(
            local_service_workers(),
            sender,
            owner_dead,
            id,
            pending,
        );
    };
    service
        .tx
        .try_send(Request {
            data: pending.data.clone(),
            sender: sender.clone(),
            route: if pending.response_type == EMPTY_TYPE {
                Vec::new()
            } else {
                vec![id.as_bytes().to_vec()]
            },
            max,
            owner_dead: owner_dead.clone(),
        })
        .map_err(|_| {
            sender.counters.queue_drops.fetch_add(1, Ordering::Relaxed);
            Error::QueueFull
        })?;
    Ok(true)
}
fn local_service_queue(
    identity: &WorkerIdentity,
    pending: &Pending,
) -> Result<Option<Weak<LocalServiceQueue>>, Error> {
    if identity
        .stopped
        .upgrade()
        .is_none_or(|stop| stop.load(Ordering::Acquire))
    {
        return Ok(None);
    }
    let services = identity
        .services
        .lock()
        .map_err(|_| Error::Native("service registry poisoned".into()))?;
    Ok(services
        .iter()
        .find(|service| {
            service.topic == pending.topic
                && service.types.request_type == pending.request_type
                && service.types.response_type == pending.response_type
                && service
                    .active
                    .upgrade()
                    .is_some_and(|active| active.load(Ordering::Acquire))
        })
        .map(|service| service.queue.clone()))
}
fn send_admitted_service_request(
    index: &Mutex<Vec<WeakWorkerOwner>>,
    sender: &Sender,
    owner_dead: &AtomicBool,
    id: &str,
    pending: &Pending,
) -> Result<bool, Error> {
    let caller = sender
        .identity
        .get()
        .expect("worker initialized caller identity");
    // An admitted own receiver keeps priority even if its Serve command has not
    // reached the worker yet. Upgrade queue payloads only outside registry locks.
    if let Some(queue) = local_service_queue(caller, pending)?.and_then(|queue| queue.upgrade())
        && queue.send(sender, owner_dead, id, pending)?
    {
        return Ok(true);
    }
    for owner in service_workers(index)? {
        if Arc::ptr_eq(&owner, &sender.identity) {
            continue;
        }
        let identity = owner.get().expect("registered worker identity");
        if identity.service_domain == caller.service_domain
            && let Some(queue) =
                local_service_queue(identity, pending)?.and_then(|queue| queue.upgrade())
            && queue.send(sender, owner_dead, id, pending)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct PeerKey {
    channel: usize,
    process: String,
    node: String,
    topic: String,
    address: String,
}
#[derive(Clone, Hash, PartialEq, Eq)]
struct SubscriberInterestKey {
    process: String,
    node: String,
    topic: String,
    address: String,
    type_name: String,
}
struct SubscriberMetadata {
    publisher: WirePublisher,
    seen: Instant,
}
#[derive(Default)]
struct TopicEndpointRecords {
    publishers: HashMap<PeerKey, String>,
    subscribers: HashMap<SubscriberInterestKey, String>,
}
#[derive(Default)]
struct TopicNameRecords {
    publishers: HashMap<PeerKey, String>,
    subscribers: HashMap<SubscriberInterestKey, String>,
}
impl TopicNameRecords {
    fn into_names(self) -> Vec<String> {
        let mut names: Vec<_> = self
            .publishers
            .into_values()
            .chain(self.subscribers.into_values())
            .collect();
        names.sort();
        names.dedup();
        names
    }
}
impl TopicEndpointRecords {
    fn into_types(self) -> TopicEndpointTypes {
        let mut publishers: Vec<_> = self.publishers.into_values().collect();
        let mut subscribers: Vec<_> = self.subscribers.into_values().collect();
        publishers.sort();
        subscribers.sort();
        TopicEndpointTypes {
            publishers,
            subscribers,
        }
    }
}
struct Worker {
    config: Config,
    host_interfaces: Vec<Ipv4Addr>,
    sender: Sender,
    response_rx: Receiver<Command>,
    owner_dead: Arc<AtomicBool>,
    local_interest_generation: u64,
    has_other_local_interest: bool,
    discovery_cycles: u8,
    topic_info_queries: VecDeque<TopicInfoQuery>,
    service_list_queries: VecDeque<ServiceListQuery>,
    prepared_publications: Vec<(Weak<AtomicBool>, Weak<AtomicU8>)>,
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
    advertisements: Vec<(WirePublisher, Arc<AtomicBool>, Arc<AtomicU8>)>,
    subscriptions: Vec<SubscriberState>,
    services: Vec<ServiceState>,
    peers: HashMap<PeerKey, (WirePublisher, Instant)>,
    local_service_owners: HashMap<String, WorkerOwner>,
    remote_subscribers: HashMap<PeerKey, WirePublisher>,
    subscriber_interests: HashMap<SubscriberInterestKey, Instant>,
    subscriber_metadata: HashMap<PeerKey, SubscriberMetadata>,
    connected: HashSet<String>,
    subscriptions_connected: HashSet<String>,
    publication_routes: HashSet<PeerKey>,
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
    cancelled: impl FnMut() -> bool,
) -> Option<Vec<Vec<u8>>> {
    receive_validated(socket, expected, counters, cancelled, |_, _| true)
}
fn receive_validated(
    socket: &zmq::Socket,
    expected: usize,
    counters: &Counters,
    mut cancelled: impl FnMut() -> bool,
    mut valid: impl FnMut(usize, usize) -> bool,
) -> Option<Vec<Vec<u8>>> {
    let mut frames = Vec::with_capacity(expected);
    let mut count = 0;
    let mut malformed = false;
    loop {
        if cancelled() {
            return None;
        }
        let frame = socket.recv_msg(zmq::DONTWAIT).ok()?;
        count += 1;
        let keep = valid(count - 1, frame.len());
        malformed |= !keep;
        if count <= expected && keep {
            frames.push(frame.to_vec());
        }
        if !frame.get_more() {
            if count != expected || malformed {
                counters.malformed_frames.fetch_add(1, Ordering::Relaxed);
            }
            return (count == expected && !malformed).then_some(frames);
        }
    }
}
fn receive_publication_frames(
    socket: &zmq::Socket,
    statistics_wire: bool,
    counters: &Counters,
    cancelled: impl FnMut() -> bool,
) -> Option<Vec<Vec<u8>>> {
    receive_validated(
        socket,
        if statistics_wire { 5 } else { 4 },
        counters,
        cancelled,
        |index, size| !statistics_wire || index != 4 || size == 16,
    )
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
    fn new(
        config: Config,
        sender: Sender,
        owner_dead: Arc<AtomicBool>,
        response_rx: Receiver<Command>,
    ) -> Result<Self, Error> {
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
        let process = Uuid::new_v4().to_string();
        let node = Uuid::new_v4().to_string();
        sender
            .identity
            .set(WorkerIdentity {
                process: process.clone(),
                node: node.clone(),
                address: address.clone(),
                response_address: response_address.clone(),
                response_id: response_id.clone(),
                service_domain: MessageDomain::from_service_config(&config),
                services: Arc::new(Mutex::new(Vec::new())),
                stopped: Arc::downgrade(&sender.stop),
            })
            .map_err(|_| Error::Native("worker identity already initialized".into()))?;
        {
            let mut workers = local_service_workers()
                .lock()
                .map_err(|_| Error::Native("process-local service owners poisoned".into()))?;
            workers.retain(|worker| worker.strong_count() != 0);
            workers.push(Arc::downgrade(&sender.identity));
        }
        Ok(Self {
            config,
            host_interfaces,
            sender,
            response_rx,
            owner_dead,
            local_interest_generation: u64::MAX,
            has_other_local_interest: false,
            discovery_cycles: 0,
            topic_info_queries: VecDeque::new(),
            service_list_queries: VecDeque::new(),
            prepared_publications: Vec::new(),
            process,
            node,
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
            local_service_owners: HashMap::new(),
            remote_subscribers: HashMap::new(),
            subscriber_interests: HashMap::new(),
            subscriber_metadata: HashMap::new(),
            connected: HashSet::new(),
            subscriptions_connected: HashSet::new(),
            publication_routes: HashSet::new(),
            response_connections: HashMap::new(),
            pending: HashMap::new(),
            replies: Vec::new(),
        })
    }
    fn announce(&self, channel: usize, mut message: Discovery) {
        if self.config.statistics_wire {
            message.version = protocol::VERSION + 100;
        }
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
            for (publisher, active, _) in &self.advertisements {
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
    #[cfg(test)]
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
    fn topic_name_records(&self, include_subscribers: bool) -> TopicNameRecords {
        let mut records = TopicNameRecords::default();
        for (publisher, active, _) in &self.advertisements {
            if active.load(Ordering::Acquire)
                && matches!(publisher.pub_type, Some(PubType::MsgPub(_)))
                && let Some(path) = self.known_topic_path(&publisher.topic)
            {
                records.publishers.insert(
                    PeerKey {
                        channel: 0,
                        process: publisher.process_uuid.clone(),
                        node: publisher.node_uuid.clone(),
                        topic: publisher.topic.clone(),
                        address: publisher.address.clone(),
                    },
                    path.into(),
                );
            }
        }
        for (key, (publisher, stamp)) in &self.peers {
            if key.channel == 0
                && stamp.elapsed() < Duration::from_secs(3)
                && matches!(publisher.pub_type, Some(PubType::MsgPub(_)))
                && let Some(path) = self.known_topic_path(&publisher.topic)
            {
                records.publishers.insert(key.clone(), path.into());
            }
        }
        if include_subscribers {
            for subscriber in &self.subscriptions {
                if subscriber.active.load(Ordering::Acquire)
                    && let Some(path) = self.known_topic_path(&subscriber.topic)
                {
                    let type_name = if subscriber.type_name.is_empty() {
                        "google.protobuf.Message"
                    } else {
                        &subscriber.type_name
                    };
                    records.subscribers.insert(
                        SubscriberInterestKey {
                            process: self.process.clone(),
                            node: self.node.clone(),
                            topic: subscriber.topic.clone(),
                            address: self.address.clone(),
                            type_name: type_name.into(),
                        },
                        path.into(),
                    );
                }
            }
            for (key, stamp) in &self.subscriber_interests {
                if stamp.elapsed() < Duration::from_secs(3)
                    && let Some(path) = self.known_topic_path(&key.topic)
                {
                    records.subscribers.insert(key.clone(), path.into());
                }
            }
        }
        records
    }
    fn topic_endpoint_records(&self, topic: &str) -> TopicEndpointRecords {
        let mut records = TopicEndpointRecords::default();
        for (publisher, active, _) in &self.advertisements {
            if active.load(Ordering::Acquire)
                && publisher.topic == topic
                && let Some(PubType::MsgPub(info)) = &publisher.pub_type
            {
                records.publishers.insert(
                    PeerKey {
                        channel: 0,
                        process: publisher.process_uuid.clone(),
                        node: publisher.node_uuid.clone(),
                        topic: publisher.topic.clone(),
                        address: publisher.address.clone(),
                    },
                    info.msg_type.clone(),
                );
            }
        }
        for (key, (publisher, stamp)) in &self.peers {
            if key.channel == 0
                && stamp.elapsed() < Duration::from_secs(3)
                && publisher.topic == topic
                && let Some(PubType::MsgPub(info)) = &publisher.pub_type
            {
                records
                    .publishers
                    .insert(key.clone(), info.msg_type.clone());
            }
        }
        for subscriber in &self.subscriptions {
            if subscriber.active.load(Ordering::Acquire) && subscriber.topic == topic {
                let type_name = if subscriber.type_name.is_empty() {
                    "google.protobuf.Message"
                } else {
                    &subscriber.type_name
                };
                records.subscribers.insert(
                    SubscriberInterestKey {
                        process: self.process.clone(),
                        node: self.node.clone(),
                        topic: subscriber.topic.clone(),
                        address: self.address.clone(),
                        type_name: type_name.into(),
                    },
                    type_name.into(),
                );
            }
        }
        for (key, stamp) in &self.subscriber_interests {
            if key.topic == topic && stamp.elapsed() < Duration::from_secs(3) {
                records
                    .subscribers
                    .insert(key.clone(), key.type_name.clone());
            }
        }
        records
    }
    #[cfg(test)]
    fn topic_endpoint_types(&self, topic: &str) -> TopicEndpointTypes {
        self.topic_endpoint_records(topic).into_types()
    }
    fn service_provider_records<'a>(
        &'a self,
        topic: &'a str,
    ) -> impl Iterator<Item = (&'a WirePublisher, &'a ServicePublisher)> {
        let local = self.services.iter().filter_map(|service| {
            service
                .active
                .load(Ordering::Acquire)
                .then_some(&service.publisher)
        });
        let remote = self.peers.iter().filter_map(|(key, (publisher, stamp))| {
            (key.channel == 1 && stamp.elapsed() < Duration::from_secs(3)).then_some(publisher)
        });
        local
            .chain(remote)
            .filter(move |publisher| publisher.topic == topic)
            .filter_map(|publisher| match &publisher.pub_type {
                Some(PubType::SrvPub(types)) => Some((publisher, types)),
                _ => None,
            })
    }
    fn service_provider_types(&self, topic: &str) -> Vec<ServiceTypes> {
        let mut signatures: Vec<_> = self
            .service_provider_records(topic)
            .map(|(_, types)| ServiceTypes {
                request_type: types.request_type.clone(),
                response_type: types.response_type.clone(),
            })
            .collect();
        signatures.sort();
        signatures
    }
    fn service_provider_info(&self, topic: &str) -> Vec<ServiceProviderInfo> {
        let mut records: Vec<_> = self
            .service_provider_records(topic)
            .map(|(publisher, types)| ServiceProviderInfo {
                topic: publisher.topic.clone(),
                address: publisher.address.clone(),
                process_uuid: publisher.process_uuid.clone(),
                node_uuid: publisher.node_uuid.clone(),
                scope: publisher.scope(),
                socket_id: types.socket_id.clone(),
                request_type: types.request_type.clone(),
                response_type: types.response_type.clone(),
            })
            .collect();
        records.sort();
        records
    }
    fn topic_publisher_info(&self, topic: &str) -> Vec<TopicPublisherInfo> {
        let local = self
            .advertisements
            .iter()
            .filter_map(|(publisher, active, _)| {
                active.load(Ordering::Acquire).then_some(publisher)
            });
        let remote = self.peers.iter().filter_map(|(key, (publisher, stamp))| {
            (key.channel == 0 && stamp.elapsed() < Duration::from_secs(3)).then_some(publisher)
        });
        let mut records: Vec<_> = local
            .chain(remote)
            .filter(|publisher| publisher.topic == topic)
            .filter_map(TopicPublisherInfo::from_publisher)
            .collect();
        records.sort();
        records
    }
    fn topic_subscriber_info(&self, topic: &str) -> Vec<TopicSubscriberInfo> {
        let mut records: Vec<_> = self
            .subscriber_metadata
            .values()
            .filter(|record| {
                record.publisher.topic == topic && record.seen.elapsed() < Duration::from_secs(3)
            })
            .filter_map(|record| TopicSubscriberInfo::from_publisher(&record.publisher))
            .collect();
        for subscriber in &self.subscriptions {
            if subscriber.active.load(Ordering::Acquire)
                && subscriber.topic == topic
                && let Some(local) = TopicSubscriberInfo::from_publisher(
                    &self.subscriber_publisher(&subscriber.topic, &subscriber.type_name),
                )
                && !records.iter().any(|record| record.same_subscriber(&local))
            {
                records.push(local);
            }
        }
        records.sort();
        records
    }
    fn has_remote_subscribers(&self, topic: &str, type_name: &str) -> bool {
        self.remote_subscribers.values().any(|p| {
            p.topic == topic
                && matches!(&p.pub_type, Some(PubType::MsgPub(m))
                    if m.msg_type == type_name || m.msg_type == "google.protobuf.Message")
        })
    }
    fn refresh_remote_interest(&self) {
        for (publisher, active, bits) in &self.advertisements {
            let type_name = match &publisher.pub_type {
                Some(PubType::MsgPub(info)) => info.msg_type.as_str(),
                _ => continue,
            };
            if active.load(Ordering::Acquire)
                && self.has_remote_subscribers(&publisher.topic, type_name)
            {
                bits.fetch_or(REMOTE_INTEREST, Ordering::AcqRel);
            } else {
                bits.fetch_and(!REMOTE_INTEREST, Ordering::AcqRel);
            }
        }
    }
    fn has_subscribers(&self, topic: &str, type_name: &str) -> bool {
        self.subscriptions.iter().any(|s| {
            s.active.load(Ordering::Acquire)
                && s.topic == topic
                && (s.type_name.is_empty() || s.type_name == type_name)
        }) || self.has_remote_subscribers(topic, type_name)
            || self.advertisements.iter().any(|(p, _, bits)| {
                p.topic == topic
                    && matches!(&p.pub_type, Some(PubType::MsgPub(info)) if info.msg_type == type_name)
                    && bits.load(Ordering::Acquire) & PROCESS_INTEREST != 0
            })
    }
    fn command(&mut self, command: Command) {
        match command {
            Command::Advertise(topic, type_name, options, active, interest) => {
                // A queued replacement must never be followed by the old withdrawal.
                self.cleanup();
                if !active.load(Ordering::Acquire) {
                    return;
                }
                let mut p = self.wire_publisher(
                    topic,
                    PubType::MsgPub(MessagePublisher {
                        msg_type: type_name,
                        throttled: options.throttled(),
                        msgs_per_sec: options.msgs_per_sec,
                        ..Default::default()
                    }),
                );
                p.scope = options.scope.into();
                if options.scope != Scope::Process {
                    self.announce(0, protocol::advertisement(&self.process, p.clone()));
                }
                self.advertisements.push((p, active, interest));
                self.refresh_remote_interest();
            }
            Command::KnownPublisherTopics(reply) => {
                let _ = reply.try_send(self.topic_name_records(false));
            }
            Command::KnownTopics(reply) => {
                self.announce(
                    0,
                    protocol::discovery(&self.process, Type::SubscribersReq, None),
                );
                let _ = reply.try_send(self.topic_name_records(true));
            }
            Command::KnownTopicInfo(topic, reply) => {
                self.announce(
                    0,
                    protocol::discovery(&self.process, Type::SubscribersReq, None),
                );
                let _ = reply.try_send(self.topic_endpoint_records(&topic));
            }
            Command::ReadyTopicInfo(query) => {
                // Native sends SUBSCRIBERS_REQ before its initialization wait.
                self.announce(
                    0,
                    protocol::discovery(&self.process, Type::SubscribersReq, None),
                );
                self.progress_topic_info_queries(Instant::now());
                if self.topic_info_queries.len() >= self.config.capacity {
                    self.sender
                        .counters
                        .queue_drops
                        .fetch_add(1, Ordering::Relaxed);
                    let _ = query.reply.try_send(Err(Error::QueueFull));
                } else {
                    self.topic_info_queries.push_back(query);
                    self.progress_topic_info_queries(Instant::now());
                }
            }
            Command::Publish(topic, type_name, data, publication) => {
                if let Some(publication) = publication.as_ref() {
                    // An admitted Publisher owns this exact history. A queued
                    // command after its lease dies cannot record in a later
                    // advertisement's history at the same address.
                    if publication
                        .active
                        .upgrade()
                        .is_some_and(|active| active.load(Ordering::Acquire))
                        && !self.sender.stop.load(Ordering::Acquire)
                    {
                        self.record_publication(publication, &data);
                    }
                } else {
                    // Private worker tests construct raw Publish commands.
                    self.remember_local_publication(&topic, &type_name, &data);
                }
                self.send_publication_wire(
                    &topic,
                    &type_name,
                    &data,
                    publication.as_ref().and_then(|p| p.sequence.as_deref()),
                );
                let message = Message {
                    topic,
                    type_name,
                    data,
                };
                self.deliver(&message, true);
                self.deliver_process_local(message);
            }
            Command::Subscribe(topic, type_name, tx, active, options, throttle) => {
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
                    options,
                    throttle,
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
            Command::Serve(topic, request_type, response_type, scope, tx, active) => {
                self.cleanup();
                if !active.load(Ordering::Acquire) {
                    return;
                }
                let mut p = self.wire_publisher(
                    topic,
                    PubType::SrvPub(ServicePublisher {
                        socket_id: self.router_id.clone(),
                        request_type,
                        response_type,
                    }),
                );
                p.scope = scope.into();
                if scope != Scope::Process {
                    self.announce(1, protocol::advertisement(&self.process, p.clone()));
                }
                self.services.push(ServiceState {
                    publisher: p,
                    tx,
                    active,
                });
            }
            Command::KnownServices(reply) => {
                let _ = reply.try_send(self.known_names(1));
            }
            Command::ReadyServiceList(query) => {
                // Native service discovery waits for startup without sending
                // the message-only SUBSCRIBERS_REQ control.
                self.progress_service_list_queries(Instant::now());
                if self.service_list_queries.len() >= self.config.capacity {
                    self.sender
                        .counters
                        .queue_drops
                        .fetch_add(1, Ordering::Relaxed);
                    let _ = query.reply.try_send(Err(Error::QueueFull));
                } else {
                    self.service_list_queries.push_back(query);
                    self.progress_service_list_queries(Instant::now());
                }
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
            Command::ResolveServiceSignatures(topic, reply) => {
                self.subscribe_message(1, &topic);
                let providers = self
                    .service_provider_records(&topic)
                    .map(|(publisher, types)| {
                        (
                            PeerKey {
                                channel: 1,
                                process: publisher.process_uuid.clone(),
                                node: publisher.node_uuid.clone(),
                                topic: publisher.topic.clone(),
                                address: publisher.address.clone(),
                            },
                            ServiceTypes {
                                request_type: types.request_type.clone(),
                                response_type: types.response_type.clone(),
                            },
                        )
                    })
                    .collect();
                let _ = reply.try_send(ServiceResolutionRecords {
                    providers,
                    _owners: self.local_service_owners.values().cloned().collect(),
                });
            }
            Command::ServiceSignatures(topic, reply) => {
                self.subscribe_message(1, &topic);
                let _ = reply.try_send(self.service_signatures(&topic));
            }
            Command::ServiceProviderTypes(topic, reply) => {
                self.subscribe_message(1, &topic);
                let _ = reply.try_send(self.service_provider_types(&topic));
            }
            Command::KnownServiceInfo(topic, reply) => {
                let _ = reply.try_send(self.service_provider_info(&topic));
            }
            Command::KnownTopicPublishers(topic, reply) => {
                let _ = reply.try_send(self.topic_publisher_info(&topic));
            }
            Command::KnownTopicSubscribers(topic, reply) => {
                let _ = reply.try_send(self.topic_subscriber_info(&topic));
            }
            Command::HasConnections(topic, type_name, reply) => {
                let _ = reply.try_send(self.has_subscribers(&topic, &type_name));
            }
            Command::RefreshConnections(topic, type_name, active, bits, deadline, reply) => {
                // A request still queued after its caller times out must not
                // enroll a stale lease into prepared-interest tracking.
                if Instant::now() >= deadline {
                    let _ = reply.try_send(Err(Error::Timeout));
                    return;
                }
                let valid = active.load(Ordering::Acquire) && !self.sender.stop.load(Ordering::Acquire)
                    && self.advertisements.iter().any(|(publisher, lease, interest)| {
                        publisher.topic == topic
                            && matches!(&publisher.pub_type, Some(PubType::MsgPub(info)) if info.msg_type == type_name)
                            && Arc::ptr_eq(lease, &active) && Arc::ptr_eq(interest, &bits)
                    });
                let result = if valid {
                    self.prepared_publications.retain(|(lease, _)| {
                        lease
                            .upgrade()
                            .is_some_and(|lease| lease.load(Ordering::Acquire))
                    });
                    if !self
                        .prepared_publications
                        .iter()
                        .any(|(_, interest)| interest.as_ptr() == Arc::as_ptr(&bits))
                    {
                        // Each tracked entry belongs to one existing live advertisement,
                        // so the Node's existing lease capacity bounds this collection.
                        self.prepared_publications
                            .push((Arc::downgrade(&active), Arc::downgrade(&bits)));
                    }
                    self.refresh_process_interest()
                        .map(|()| bits.load(Ordering::Acquire) != 0)
                } else {
                    Err(Error::Shutdown)
                };
                let _ = reply.try_send(result);
            }
            Command::Request(pending) => {
                if self.owner_dead.load(Ordering::Acquire) {
                    let _ = pending.reply.try_send(Err(Error::Shutdown));
                    return;
                }
                if Instant::now() >= pending.deadline {
                    let _ = pending.reply.try_send(Err(Error::Timeout));
                    return;
                }
                if self.pending.len() < self.config.capacity {
                    self.subscribe_message(1, &pending.topic);
                    let id = Uuid::new_v4().to_string();
                    let mut pending = pending;
                    match send_local_request(
                        &self.services,
                        &self.sender,
                        &self.owner_dead,
                        self.config.max_message_bytes,
                        &id,
                        &pending,
                    ) {
                        Ok(sent) => pending.sent = sent,
                        Err(error) => {
                            let _ = pending.reply.try_send(Err(error));
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
    fn send_publication_wire(
        &self,
        topic: &str,
        type_name: &str,
        data: &[u8],
        sequence: Option<&AtomicU64>,
    ) {
        let frames = [
            topic.as_bytes(),
            self.address.as_bytes(),
            data,
            type_name.as_bytes(),
        ];
        if !self.config.statistics_wire {
            self.sender
                .counters
                .sent(self.publisher.send_multipart(frames, zmq::DONTWAIT));
        } else if self.has_remote_subscribers(topic, type_name) {
            // Native advances the per-topic sequence only on a wire attempt;
            // local delivery and no-subscriber publications do not consume it.
            let metadata = sequence
                .ok_or(Error::InvalidFrame)
                .and_then(|sequence| statistics_metadata(sequence, publication_time()));
            match metadata {
                Ok(metadata) => {
                    self.sender.counters.sent(
                        self.publisher.send_multipart(
                            frames
                                .into_iter()
                                .chain(std::iter::once(metadata.as_slice())),
                            zmq::DONTWAIT,
                        ),
                    );
                }
                Err(_) => {
                    self.sender
                        .counters
                        .send_failures
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
    fn deliver(&self, message: &Message, intra_process: bool) {
        for s in &self.subscriptions {
            if s.active.load(Ordering::Acquire)
                && s.topic == message.topic
                && (s.type_name.is_empty() || s.type_name == message.type_name)
                && !(intra_process && s.options.ignore_local_messages())
                && s.throttle.as_ref().is_none_or(|throttle| throttle.admit())
            {
                self.sender.counters.dropped(s.tx.try_send(ReceivedMessage {
                    message: message.clone(),
                    intra_process,
                }));
            }
        }
    }
    fn deliver_process_local(&mut self, message: Message) {
        let domain = MessageDomain::from_config(&self.config);
        let generation = LOCAL_INTEREST_GENERATION.load(Ordering::Acquire);
        if generation != u64::MAX
            && generation == self.local_interest_generation
            && !self.has_other_local_interest
            && !local_interests().is_poisoned()
        {
            return;
        }
        let (targets, has_other) = match local_interests().lock() {
            Ok(mut interests) => {
                interests.retain_mut(LocalInterest::retain_for_peer_expiry);
                let has_other = interests.iter().any(|interest| {
                    interest.live() && interest.domain == domain && interest.key.node != self.node
                });
                let targets = interests
                    .iter()
                    .filter(|interest| {
                        interest.live()
                            && interest.domain == domain
                            && interest.key.node != self.node
                            && interest.key.topic == message.topic
                            && (interest.key.type_name == "google.protobuf.Message"
                                || interest.key.type_name == message.type_name)
                    })
                    .filter_map(|interest| {
                        interest
                            .tx
                            .clone()
                            .map(|tx| (tx, interest.throttle.clone()))
                    })
                    .collect::<Vec<_>>();
                (targets, has_other)
            }
            Err(_) => {
                self.sender
                    .counters
                    .routing_errors
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        self.local_interest_generation = generation;
        self.has_other_local_interest = has_other;
        let last = targets.len().saturating_sub(1);
        let mut message = Some(message);
        for (index, (target, throttle)) in targets.into_iter().enumerate() {
            if throttle.as_ref().is_some_and(|throttle| !throttle.admit()) {
                continue;
            }
            // Every receiver still owns an independent Message. Move the
            // worker's final copy into the last queue after same-Node
            // delivery, avoiding an extra payload clone for one target.
            let delivered = if index == last {
                message.take().unwrap()
            } else {
                message.as_ref().unwrap().clone()
            };
            self.sender
                .counters
                .dropped(target.try_send(ReceivedMessage {
                    message: delivered,
                    intra_process: true,
                }));
        }
    }
    fn remember_local_publication(&self, topic: &str, type_name: &str, data: &[u8]) {
        match local_publications().lock() {
            Ok(mut publications) => {
                publications.retain_mut(LocalPublication::retain_for_peer_expiry);
                if let Some(publication) = publications.iter().find(|publication| {
                    publication.live()
                        && publication.key.node == self.node
                        && publication.key.topic == topic
                        && publication.type_name == type_name
                        && publication.key.address == self.address
                }) {
                    self.record_publication(&publication.publication, data);
                }
            }
            Err(_) => {
                self.sender
                    .counters
                    .routing_errors
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    fn record_publication(&self, publication: &PublicationHandle, data: &[u8]) {
        match publication.recent_frames.lock() {
            Ok(mut frames) => {
                frames.push_back((frame_fingerprint(data), data.len(), Instant::now()));
                while frames.len() > self.config.capacity {
                    frames.pop_front();
                }
            }
            Err(_) => {
                self.sender
                    .counters
                    .routing_errors
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    fn local_wire_echo(&self, topic: &str, address: &str, type_name: &str, data: &[u8]) -> bool {
        let domain = MessageDomain::from_config(&self.config);
        match local_publications().lock() {
            Ok(mut publications) => {
                publications.retain_mut(LocalPublication::retain_for_peer_expiry);
                let mut fingerprint = None;
                publications.iter().any(|publication| {
                    if !(publication.domain == domain
                        && publication.key.topic == topic
                        && publication.key.address == address
                        && publication.type_name == type_name)
                    {
                        return false;
                    }
                    match publication.publication.recent_frames.lock() {
                        Ok(frames) => frames.iter().any(|(hash, len, _)| {
                            *len == data.len()
                                && *hash
                                    == *fingerprint.get_or_insert_with(|| frame_fingerprint(data))
                        }),
                        Err(_) => {
                            // A poisoned local history is observable. Suppress
                            // this exact local endpoint's wire copy so it
                            // cannot become a duplicate with false origin.
                            self.sender
                                .counters
                                .routing_errors
                                .fetch_add(1, Ordering::Relaxed);
                            true
                        }
                    }
                })
            }
            Err(_) => {
                self.sender
                    .counters
                    .routing_errors
                    .fetch_add(1, Ordering::Relaxed);
                false
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
            self.retain_local_service_owners();
            self.publication_routes
                .retain(|key| key.channel != channel || key.process != message.process_uuid);
            if channel == 0 {
                self.subscriber_interests
                    .retain(|key, _| key.process != message.process_uuid);
                self.subscriber_metadata
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
                self.refresh_remote_interest();
            }
            self.subscriber_interests
                .retain(|key, _| key.process != process || key.node != node || key.topic != topic);
            self.subscriber_metadata
                .retain(|key, _| key.process != process || key.node != node || key.topic != topic);
            return;
        }
        match message.disc_contents {
            Some(DiscContents::Pub(p)) if channel == 0 && kind == Type::SubscribersRep => {
                let Some(PubType::MsgPub(info)) = &p.pub_type else {
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
                    process: p.process_uuid.clone(),
                    node: p.node_uuid.clone(),
                    topic: p.topic.clone(),
                    address: p.address.clone(),
                    type_name: info.msg_type.clone(),
                };
                let mut dropped = false;
                if let Some(stamp) = self.subscriber_interests.get_mut(&key) {
                    *stamp = Instant::now();
                } else if self.subscriber_interests.len() < self.config.capacity {
                    self.subscriber_interests.insert(key, Instant::now());
                } else {
                    dropped = true;
                }
                self.subscriber_metadata
                    .retain(|_, record| record.seen.elapsed() < Duration::from_secs(3));
                let key = PeerKey {
                    channel: 0,
                    process: p.process_uuid.clone(),
                    node: p.node_uuid.clone(),
                    topic: p.topic.clone(),
                    address: p.address.clone(),
                };
                if let Some(record) = self.subscriber_metadata.get_mut(&key) {
                    record.seen = Instant::now();
                } else if self.subscriber_metadata.len() < self.config.capacity {
                    self.subscriber_metadata.insert(
                        key,
                        SubscriberMetadata {
                            publisher: p,
                            seen: Instant::now(),
                        },
                    );
                } else {
                    dropped = true;
                }
                if dropped {
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
                            self.refresh_remote_interest();
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
                    let service_process = (channel == 1
                        && matches!(&p.pub_type, Some(PubType::SrvPub(_))))
                    .then(|| p.process_uuid.clone());
                    self.peers.insert(key, (p, Instant::now()));
                    if let Some(process) = service_process {
                        self.cache_local_service_owner(&process);
                    }
                }
            }
            Some(DiscContents::Pub(p)) if kind == Type::Unadvertise => {
                self.peers.retain(|key, _| {
                    key.channel != channel
                        || key.process != p.process_uuid
                        || key.node != p.node_uuid
                        || key.topic != p.topic
                });
                self.retain_local_service_owners();
                self.publication_routes.retain(|key| {
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
                    self.refresh_remote_interest();
                }
            }
            Some(DiscContents::Sub(s)) if kind == Type::Subscribe => {
                let publishers: Vec<_> = if channel == 0 {
                    self.advertisements
                        .iter()
                        .filter(|(p, a, _)| {
                            a.load(Ordering::Acquire)
                                && p.scope != i32::from(Scope::Process)
                                && (p.scope != i32::from(Scope::Host) || local)
                        })
                        .map(|(p, _, _)| p.clone())
                        .collect()
                } else {
                    self.services
                        .iter()
                        .filter(|s| {
                            s.active.load(Ordering::Acquire)
                                && s.publisher.scope != i32::from(Scope::Process)
                                && (s.publisher.scope != i32::from(Scope::Host) || local)
                        })
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
    fn retain_local_service_owners(&mut self) {
        self.local_service_owners.retain(|process, _| {
            self.peers.iter().any(|(key, (publisher, _))| {
                key.channel == 1
                    && key.process == *process
                    && matches!(&publisher.pub_type, Some(PubType::SrvPub(_)))
            })
        });
    }
    fn cache_local_service_owner(&mut self, process: &str) {
        self.retain_local_service_owners();
        match service_workers(local_service_workers()) {
            Ok(owners) => {
                if let Some(owner) = owners.into_iter().find(|owner| {
                    owner.get().is_some_and(|identity| {
                        identity.process == process
                            && identity.service_domain
                                == MessageDomain::from_service_config(&self.config)
                    })
                }) {
                    self.local_service_owners.insert(process.into(), owner);
                }
            }
            Err(_) => {
                self.sender
                    .counters
                    .routing_errors
                    .fetch_add(1, Ordering::Relaxed);
            }
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
            // A withdrawal and replacement can arrive in the same UDP batch.
            self.cleanup_publication_routes();
            let key = PeerKey {
                channel: 0,
                process: publisher.process_uuid.clone(),
                node: publisher.node_uuid.clone(),
                topic: publisher.topic.clone(),
                address: publisher.address.clone(),
            };
            if !self.publication_routes.contains(&key)
                && self.publication_routes.len() >= self.config.capacity
            {
                self.sender
                    .counters
                    .queue_drops
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
            if !self.subscriptions_connected.contains(&publisher.address) {
                if !self
                    .sender
                    .counters
                    .sent(self.subscriber.connect(&publisher.address))
                {
                    return;
                }
                self.subscriptions_connected
                    .insert(publisher.address.clone());
            }
            // The route bound also bounds distinct socket addresses. Failed
            // connects consume neither capacity nor a registration announcement.
            self.publication_routes.insert(key);
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
            self.retire_owner_requests();
            self.cleanup();
            for _ in 0..self.config.capacity {
                if let Ok(response) = self.response_rx.try_recv() {
                    self.command(response);
                }
                match rx.try_recv() {
                    Ok(cmd) => self.command(cmd),
                    Err(_) => break,
                }
            }
            for channel in 0..2 {
                for _ in 0..self.config.capacity {
                    match self.udp[channel].recv_from(&mut buffer) {
                        Ok((len, source)) => {
                            let message = if self.config.statistics_wire {
                                protocol::decode_version(&buffer[..len], protocol::VERSION + 100)
                            } else {
                                protocol::decode(&buffer[..len])
                            };
                            if let Ok(msg) = message {
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
            self.retire_owner_requests();
            if !self.owner_dead.load(Ordering::Acquire) {
                self.send_requests();
                self.send_replies();
            }
            if heartbeat.elapsed() >= Duration::from_secs(1) {
                for channel in 0..2 {
                    self.announce(
                        channel,
                        protocol::discovery(&self.process, Type::Heartbeat, None),
                    );
                }
                for (p, active, _) in &self.advertisements {
                    // Native heartbeats re-advertise even Process publishers;
                    // receiving peers enforce the scope carried in metadata.
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
                self.discovery_cycles = self.discovery_cycles.saturating_add(1).min(2);
            }
            self.progress_topic_info_queries(Instant::now());
            self.progress_service_list_queries(Instant::now());
            let old_peer_count = self.peers.len();
            self.peers
                .retain(|_, (_, stamp)| stamp.elapsed() < Duration::from_secs(3));
            if self.peers.len() != old_peer_count {
                self.retain_local_service_owners();
            }
            self.subscriber_interests
                .retain(|_, stamp| stamp.elapsed() < Duration::from_secs(3));
            self.subscriber_metadata
                .retain(|_, record| record.seen.elapsed() < Duration::from_secs(3));
            // Wake for commands while retaining the bounded socket poll interval.
            if let Ok(cmd) = rx.recv_timeout(Duration::from_millis(2)) {
                self.command(cmd);
            }
        }
        self.progress_topic_info_queries(Instant::now());
        self.progress_service_list_queries(Instant::now());
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
        // The ordered END controls above own final subscription withdrawal.
        // Retire inactive publications and services before process BYE.
        self.subscriptions.clear();
        self.cleanup();
        for channel in 0..2 {
            self.announce(channel, protocol::discovery(&self.process, Type::Bye, None));
        }
    }
    fn progress_topic_info_queries(&mut self, now: Instant) {
        for _ in 0..self.topic_info_queries.len() {
            let query = self.topic_info_queries.pop_front().unwrap();
            let result = if self.sender.stop.load(Ordering::Acquire)
                || self.owner_dead.load(Ordering::Acquire)
            {
                Err(Error::Shutdown)
            } else if now >= query.deadline {
                Err(Error::Timeout)
            } else if self.discovery_cycles >= 2 {
                Ok(self.topic_endpoint_records(&query.topic))
            } else {
                self.topic_info_queries.push_back(query);
                continue;
            };
            let _ = query.reply.try_send(result);
        }
    }
    fn progress_service_list_queries(&mut self, now: Instant) {
        for _ in 0..self.service_list_queries.len() {
            let query = self.service_list_queries.pop_front().unwrap();
            let result = if self.sender.stop.load(Ordering::Acquire)
                || self.owner_dead.load(Ordering::Acquire)
            {
                Err(Error::Shutdown)
            } else if now >= query.deadline {
                Err(Error::Timeout)
            } else if self.discovery_cycles >= 2 {
                Ok(self.known_names(1))
            } else {
                self.service_list_queries.push_back(query);
                continue;
            };
            let _ = query.reply.try_send(result);
        }
    }
    fn retire_owner_requests(&mut self) {
        if self.owner_dead.load(Ordering::Acquire) {
            self.pending.clear();
            self.replies.clear();
        }
    }
    fn receive_publications(&self) {
        for _ in 0..self.config.capacity {
            let Some(frames) = receive_publication_frames(
                &self.subscriber,
                self.config.statistics_wire,
                &self.sender.counters,
                || self.sender.stop.load(Ordering::Acquire),
            ) else {
                break;
            };
            if let (Ok(topic), Ok(type_name)) =
                (protocol::text(&frames[0]), protocol::text(&frames[3]))
            {
                let Ok(address) = protocol::text(&frames[1]) else {
                    self.sender
                        .counters
                        .malformed_frames
                        .fetch_add(1, Ordering::Relaxed);
                    continue;
                };
                if self.local_wire_echo(topic, address, type_name, &frames[2]) {
                    continue;
                }
                let message = Message {
                    topic: topic.into(),
                    type_name: type_name.into(),
                    data: frames[2].clone(),
                };
                self.deliver(&message, false);
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
    fn refresh_process_interest(&mut self) -> Result<(), Error> {
        self.refresh_process_interest_with(local_interests().lock())
    }
    fn refresh_process_interest_with(
        &mut self,
        registry: std::sync::LockResult<std::sync::MutexGuard<'_, Vec<LocalInterest>>>,
    ) -> Result<(), Error> {
        self.prepared_publications.retain(|(lease, _)| {
            lease
                .upgrade()
                .is_some_and(|lease| lease.load(Ordering::Acquire))
        });
        match registry {
            Ok(mut interests) => {
                for interest in interests.iter_mut().filter(|s| s.key.node == self.node) {
                    if !interest.live() {
                        interest.tx = None;
                    }
                }
                // Process scope and explicitly prepared publishers use the same
                // live registrations as direct process-local delivery.
                for (publisher, _, bits) in &self.advertisements {
                    if publisher.scope != i32::from(Scope::Process)
                        && !self
                            .prepared_publications
                            .iter()
                            .any(|(_, prepared)| prepared.as_ptr() == Arc::as_ptr(bits))
                    {
                        continue;
                    }
                    let domain = MessageDomain::from_config(&self.config);
                    let matching = interests.iter().any(|interest| {
                        interest.live()
                            && interest.domain == domain
                            && interest.key.node != self.node
                            && interest.key.topic == publisher.topic
                            && matches!(&publisher.pub_type, Some(PubType::MsgPub(info))
                                if interest.key.type_name == "google.protobuf.Message"
                                    || interest.key.type_name == info.msg_type)
                    });
                    if matching {
                        bits.fetch_or(PROCESS_INTEREST, Ordering::AcqRel);
                    } else {
                        bits.fetch_and(!PROCESS_INTEREST, Ordering::AcqRel);
                    }
                }
            }
            Err(_) => {
                for (_, _, bits) in &self.advertisements {
                    bits.fetch_and(!PROCESS_INTEREST, Ordering::AcqRel);
                }
                self.sender
                    .counters
                    .routing_errors
                    .fetch_add(1, Ordering::Relaxed);
                return Err(Error::Native(
                    "process-local subscriber registry poisoned".into(),
                ));
            }
        }
        Ok(())
    }
    fn cleanup(&mut self) {
        let mut removed = Vec::new();
        self.advertisements.retain(|(p, a, _)| {
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
            if p.scope != i32::from(Scope::Process) {
                self.announce(
                    channel,
                    protocol::discovery(
                        &self.process,
                        Type::Unadvertise,
                        Some(DiscContents::Pub(p)),
                    ),
                );
            }
        }
        let mut topics = HashSet::new();
        self.subscriptions.retain(|s| {
            let keep = s.active.load(Ordering::Acquire);
            if !keep {
                topics.insert(s.topic.clone());
            }
            keep
        });
        // Match the worker's withdrawal boundary: before cleanup an already
        // queued receiver can still time out; afterward no registry clone may
        // keep its channel open once the worker drops its sender.
        let _ = self.refresh_process_interest();
        let mut topics: Vec<_> = topics.into_iter().collect();
        topics.sort();
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
        self.cleanup_publication_routes();
        self.response_connections.retain(|address, stamp| {
            let keep = stamp.elapsed() < self.config.reply_timeout.max(Duration::from_secs(3));
            if !keep {
                let _ = self.router.disconnect(address);
            }
            keep
        });
    }
    fn cleanup_publication_routes(&mut self) {
        // Discovery silence retires metadata, not a still-used physical link.
        // Explicit withdrawals above and local unsubscription release routes.
        self.publication_routes.retain(|key| {
            self.subscriptions.iter().any(|subscription| {
                subscription.active.load(Ordering::Acquire) && subscription.topic == key.topic
            })
        });
        self.subscriptions_connected.retain(|address| {
            let keep = self
                .publication_routes
                .iter()
                .any(|key| &key.address == address);
            if !keep {
                let _ = self.subscriber.disconnect(address);
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
                        owner_dead: self.owner_dead.clone(),
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
        let services = &self.services;
        let sender = &self.sender;
        let owner_dead = &self.owner_dead;
        let max = self.config.max_message_bytes;
        self.pending.retain(|id, p| {
            if Instant::now() >= p.deadline {
                let _ = p.reply.try_send(Err(Error::Timeout));
                return false;
            }
            if p.sent {
                return true;
            }
            // A request can precede a local service registration. Keep its
            // original ID and deadline, and mark it sent only after admission.
            match send_local_request(services, sender, owner_dead, max, id, p) {
                Ok(true) => p.sent = true,
                Ok(false) => {}
                Err(error) => {
                    let _ = p.reply.try_send(Err(error));
                    return false;
                }
            }
            if p.sent && p.response_type == EMPTY_TYPE {
                let _ = p.reply.try_send(Ok((Vec::new(), true)));
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

#[cfg(test)]
#[path = "../tests/support/statistics_wire.rs"]
mod statistics_wire_tests;
