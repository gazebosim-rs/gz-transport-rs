# Bounded startup service listing

`Node::service_list(timeout)` waits for this worker's discovery startup boundary,
then returns the same sorted, deduplicated service paths as `known_services()`.
The existing immediate method is unchanged. Registration remapping, namespaces,
partition filtering, active local handles, remote record admission and expiry
follow the [known-list contract](NET-02-known-lists.md). The query does not apply
a second remap or ask for an expected name.

The timeout covers startup and the worker reply. Zero returns `Timeout`; deadline
overflow returns `InvalidConfig`. A pending queue holds at most `Config::capacity`
service-list queries, separate from the existing topic-info queue. Command-queue
and pending-queue admission can return `QueueFull`. Expired queries return
`Timeout` and release pending capacity. Cancellation and worker owner retirement
return `Shutdown` to admitted waiters. The worker keeps processing discovery,
requests and other commands while startup queries wait. Callers should prepare
outside a latency-sensitive step.

Pinned native `Node::ServiceList` calls the service discovery `TopicList`, which
waits for initialization before reading names. Unlike message discovery, it sends
no `SUBSCRIBERS_REQ`. Native discovery emits an immediate heartbeat and completes
startup after two further heartbeat cycles. The existing Rust startup counter
advances after both message and service heartbeat work completes each interval;
this API uses its two-interval boundary without sending a subscriber request.
Warm calls snapshot current records without restarting that boundary.

Initialization is local to each Rust worker. Native discovery initialization is
shared by Nodes in a process. Neither initialization nor a successful query proves
remote convergence, provider consensus, handler readiness, delivery, network-wide
absence or future list stability. This API adds no process-global service registry
or native timing guarantee. A valid empty result is the current known snapshot.
Existing partial ServiceList mappings remain partial; no upstream case gains
credit from this addition.

Supplemental tests compare the first and warm lists of fresh native and Rust
query Nodes against held native Int32/Int32, Empty/Int32 and Int32/Empty providers,
and against an absent provider. Every actual query result must match; no name
polling precedes the startup query. A separate bounded withdrawal observation
checks the native provider's retirement. Public tests cover live local handles,
namespace/remap behavior, empty and warm results, early and zero deadlines,
overflow, cancellation and explicit shutdown. Worker tests check pending capacity,
one-cycle non-readiness, two-cycle completion, expiry freeing capacity, immediate
inspection during startup and shutdown of every admitted waiter.

These are correctness checks, not equal-fidelity latency, throughput or allocation
measurements. Existing original assertions, required native runs and production
coverage policy are unchanged.
