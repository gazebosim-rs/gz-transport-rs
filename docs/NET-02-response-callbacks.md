# Bounded response service callbacks

`serve_callback(topic, request_type, response_type, callback)` owns one service
registration and a dispatcher. The callback takes borrowed request bytes and
returns owned response bytes plus the service result. `request_callback` takes
owned request bytes and a one-shot response callback. Its `CallbackRequest`
reports completion separately from transport admission. Nonempty opaque type
names are permitted as in the raw service API. Empty type names are invalid;
`gz.msgs.Empty` responses return `Unsupported`. Existing one-way services and
`serve`, `request`, `begin_request` and `PendingResponse` retain their contracts.

An exact live callback in this process and explicit service domain executes the
service callback, then the response callback, on the requesting thread before
`request_callback` returns. It never also enqueues that request. Service-domain
matching uses canonical partition, service discovery port, multicast group and
interface; topic qualification and remapping preserve namespace semantics.
Absolute paths remain absolute. This domain governs the inline registry path;
it does not promise that host UDP socket delivery isolates multicast interfaces
or groups sharing a discovery port. Worker-only metadata views are unchanged.

The pinned native `detail/Node.hh` asynchronous `Request` finds an exact local
replier under the shared mutex, releases that mutex, then invokes the service
and response callbacks before returning. The new direct member-service tests
preserve that immediate flag assertion, Int32 data 5, successful result,
invalid service name, synchronous and raw request checks, wrong-type admission,
and the unchanged 300 ms quiet interval. The older queued adaptations remain.
Neither a discovery readiness barrier nor a response wait precedes the new
immediate assertion. Both pinned C++ originals pass unchanged.

A local selection is final for this call. Retirement before its liveness check
returns a completion error; retirement after invocation starts cannot preempt
user code. A selected callback error, panic, oversized response or expired
deadline completes once without trying a different provider or a wire retry.
Multiple matching providers have no selection-order guarantee. No general
process-global discovery, object-pointer identity, native thread scheduling or
remote immediate-completion claim follows.

Before invoking local user code, the caller validates the remapped name,
nonempty type names, response kind, payload size, checked nonzero timeout,
shutdown state and request admission bound. The provider's input size bound is
also checked. All new request operations, including inline ones, reserve from
a separate per-Node counter limited by `Config::capacity`. New server and reply
dispatchers share a separate per-Node thread counter with the same limit.
Registration slots and existing command/pending queues retain their own bounds.
An exhausted callback counter returns `QueueFull` and increments `queue_drops`.
An inline call creates no dispatcher thread and does not consume thread capacity.

Other requests use the unchanged typed service queue and pending reply path.
The reply dispatcher is spawned before transport admission, so an OS thread
creation failure cannot leave an admitted callback request. Failed queue
admission cancels and joins that dispatcher. A registration dispatcher stays
parked until its weak callback entry commits; failed spawn or commit retires
the lease. Only weak callback references are stored with admitted declarations.
No user function or last strong callback destruction runs under a registry or
IO-worker lock. Reentrant calls use the same capacity bounds. Inline callers
and the queued dispatcher may execute concurrently, so service callbacks require
`Fn + Send + Sync`. An ordinary blocking request back into its own occupied
queued dispatcher can time out; this API adds no general scheduling guarantee.

`CallbackService::cancel` invalidates its registration. `CallbackRequest::cancel`
suppresses a response callback not yet selected and reports `Shutdown`; it
cannot retract a request already delivered to its service. Dropping either
handle cancels and joins its dispatcher, except when dropped on that dispatcher
itself. A service invocation selected inline may finish after service handle
destruction. Thread and request permits remain held until actual dispatch or
inline execution finishes, even if cancellation frees a registration slot.
User functions cannot be interrupted, so Drop can wait for running user code.
`CallbackRequest::wait` observes completion once; its wait bound alone does not
cancel. Deadlines bound selection and response dispatch, not execution time of
user code already selected. Response size is checked against both applicable
provider and caller limits before the response callback runs.

Rust panics are caught at each callback boundary and reported as typed `Native`
completion errors. Local callback errors preserve the registration and never
trigger a second invocation. Queued service errors send an empty false result
and increment `routing_errors`, because the unchanged Gazebo response wire has
no Rust error channel. A response callback panic completes with an error. No
user callback is invoked across an FFI boundary. These rules do not change
panic handling of the existing one-way API.

Controls exercise direct and queued delivery, every applicable admission bound,
spawn and command-queue rollback, cancellation, stopped owners, payload limits,
panic/error handling, deadline expiry, reentrancy, self-drop, and permits held
through blocked user callbacks. Real C++ peers serve a Rust callback request and
request a Rust callback service over the unchanged wire. Broader service
scheduling, global identity, withdrawal and discovery gaps remain open.

The private capacity-one permit fixture drains the registration command and
uses the existing same-Node queued path before blocking user code; it does not
rely on cold UDP discovery. The new original mappings have no such barrier.
Pre-parse service and response-entry counters prevent caught assertion panics
from hiding wrong-type invocations. Separate removals of either type-selection
guard fail the corresponding zero-invocation assertion.
