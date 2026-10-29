# Local one-way service callback

The pinned `NodeTest.ServiceCallWithoutOutputAsyncLambda` original advertises
`/foo` with an Int32 callback, requests Int32 data 5, and checks that the
callback has set `executed` immediately after `Node::Request` returns. The
native original passes 1/0. The Rust test calls raw `Node::request_oneway`,
then checks the same data and flag at that return boundary. A held-callback
control proves the request cannot return before callback completion.

`Node::serve_oneway_callback` owns the service lease and callback dispatcher.
A matching callback on this Node keeps its original caller-thread behavior.
The same immediate completion now applies to other Rust Nodes in the explicit
service domain: canonical partition, interface, multicast group and service port,
with normal namespace qualification and one remapping. Message port differences
do not isolate service callbacks. This lookup uses admitted weak declarations;
it does not import names or metadata into any worker snapshot or unify wire
identities. Absolute topic paths remain absolute.

This Node's existing matching receiver remains first, including a plain queued
`Service`. Only if it has no matching local receiver does the caller consider
other live callback declarations. Multiple foreign matching callbacks have no
selection-order guarantee. Exact request and Empty response types, ready state,
active lease and live owner are required. A selected callback error, panic or
retirement completes this call without another provider or a queued/wire retry.
The requesting Node's payload bound and the provider's bound apply before user
invocation. User code and last strong captured-value destruction run outside
all registry and worker locks. Inline callers and the queued dispatcher can
execute concurrently; service functions require `Fn + Send + Sync`.

The existing timeout bounds queued discovery and handoff. Inline calls reject
zero or overflowing timeout values, but add no new execution deadline and do
not preempt selected user code; a successful callback can finish after that
duration and still return success. Invocation errors and panics retain the
same typed errors as the earlier same-Node path, with provider routing-error
accounting. Ordinary captured-value `Drop` code retains Rust unwind behavior;
it is not silently converted into an invocation error.

Calls to `request_oneway` share the existing per-Node request admission counter
with response callback requests. Callback service dispatchers and owned response callback
waiters share the existing separate thread counter. Each is limited by
`Config::capacity`; an exhausted callback counter returns `QueueFull` and counts
one local queue drop. Registration slots and ordinary command/pending queues
retain their own bounds. Request admission includes lookup and captured-value
cleanup. A dispatcher retains its thread permit through actual exit and user
cleanup, even after unadvertise frees its registration slot. Duplicate service
rejection remains `AlreadyAdvertised`, including at capacity one.

A dispatcher parks until its weak declaration commits. Failed thread spawn,
command admission or commit retirement rolls back without invoking user code.
Drop retires the lease and joins its dispatcher, except when called from that
dispatcher itself. An inline call already selected may finish after service
handle destruction. Unmatched and explicit `begin_request` operations preserve
the existing queue route. Empty completion means only queue/send handoff;
it never acknowledges callback execution or delivery. No response is sent.

The earlier supplemental queued-error control now explicitly submits both
operations with `begin_request(..., gz.msgs.Empty).wait(...).map(|_| ())`, the
exact fallback previously used by other-Node `request_oneway`. Its payload,
error/count/progress assertions, 300-by-10 ms polls and every native-original
body are unchanged. This setup translation keeps the control genuinely queued;
separate controls test cross-Node inline error and panic propagation without
asymmetric masking.

A fresh native two-Node observer checks caller-thread completion, Int32 data 5,
invalid-name rejection, and the unchanged 300 ms wrong-type and withdrawal quiet
intervals. The Rust control checks immediate completion and no second delivery.
Private controls cover dispatcher capacity after lease retirement, shared
request admission, spawn/queue/commit rollback before user invocation, domains,
payload bounds, stop, nonpreemptible execution, reentrant user destruction and
ordinary destructor unwind. The earlier response callback lifetime regressions
remain required after the shared weak lookup/commit helper change.

The fixed original checks only the local callback value and post-request
flag. Supplemental controls cover one invocation, held callback completion,
wrong descriptors, remapping and reentrancy, explicit queued dispatch/error,
Drop, shutdown and panic. C++ callback registration, exact thread identity,
process-global remote behavior, and a general scheduling guarantee remain
outside this slice. The historical lost-output status 101 remains
unclassified.

The [independent source review](reviews/service-oneway-callback.json)
replayed the pinned native original and the Rust original from an exact raw
export. All seven focused Rust controls passed. Removing only the direct
callback invocation makes the original Rust test fail at its unchanged
post-request `executed` assertion. The reviewer approved `faithful_port` for
that fixed observable original; the broader limits above remain explicit.

This repair claims no new original-case credit, process-global wire identity,
remote immediate callback timing, metadata convergence or broad NoOutput
completion. The native source `detail/Node.hh` one-way overload delegates to
`Request<RequestT, Empty>`, whose exact local handler is invoked before return.
