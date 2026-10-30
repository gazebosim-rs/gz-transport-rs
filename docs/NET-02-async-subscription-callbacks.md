<!--
Copyright (C) 2026 Open Source Robotics Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Demand-created asynchronous typed callbacks

`Node::subscribe_typed_callback<M, F>` registers a callback for a sealed canonical
`gz-msgs` message. `subscribe_typed_callback_with_options` also accepts the existing
`SubscribeOptions`. Both return an owned `CallbackSubscription`. The callback is
`FnMut(&M, MessageInfo) -> Result<(), Error> + Send + 'static`; borrowed message
and metadata values remain valid for that invocation. The maintained generated
codec decodes before user entry. Existing opaque/raw receivers, their options,
wire bytes, ownership and queued-drain behavior remain unchanged.

The first callback on a Node creates one shared dispatcher. Subsequent callbacks
reuse it, rather than creating one thread per subscription. Ordinary receivers
create no dispatcher or wake channel. Successful delivery notifies the shared
dispatcher through a coalesced capacity-one wake channel; idle callbacks do not
poll their queues. Unsubscribe, cancellation, Node retirement and registration
also notify it. The final callback's real retirement closes admission against
concurrent registration and retires the worker. A completed old worker can be
replaced without losing an admitted callback. Short-lived exiting workers remain
subject to the existing total callback-thread budget.

Endpoint leases keep the existing `Config::capacity` limit. Each dispatcher also
bounds retained callback entries, including queued registrations, selected user
work and cancelled captures whose destructor has not finished, by that capacity.
The control queue is bounded by the same value; each subscription reuses its
existing bounded message queue. The existing per-Node callback-thread budget is
shared with service/request dispatchers. Exhaustion returns `QueueFull`; shutdown
returns `Shutdown`. No timeout can preempt user code. These are bounds on runtime
entries/queues/threads and raw frame admission, not arbitrary heap state a caller
chooses to capture or retain. Each registration decodes its own received bytes;
shared execution does not promise shared decoded protobuf objects.

All user code runs outside IO and registration/admission/cache locks. Returned
errors and unwind panics in invocation or capture disposal increment the existing
operational `routing_errors` counter, leaving other callbacks able to progress.
Panic-payload destruction is included in the unwind boundary. Retained entry and
thread permits remain held through real callback/capture/payload destruction.
An operation that never returns cannot be preempted; ordinary Rust abort and
double-panic-abort behavior is not contained. Node statistics enablement,
TopicStatistics aggregation and Metric publishing are unrelated and unchanged.

`cancel()` retires the registration without waiting. An invocation is selected
when its liveness check succeeds immediately before dispatch; it may finish if
cancellation follows that check. Work still queued when cancellation is observed
is discarded. Drop waits for that registration's callback and captures, then
joins the final worker when appropriate. A dispatcher cannot wait on itself:
self-drop and removal of another callback from that dispatcher cancel without
joining/waiting there. Reentrant registration stays subject to the same retained
entry/thread budgets. Node shutdown/drop suppress callbacks not yet selected,
while the old raw receiver API retains its existing drain contract.

The pinned native sources are
[`Node.cc`](https://github.com/gazebosim/gz-transport/blob/41913b9dfc3dbce33ac5464ddcb427cc82450f26/src/Node.cc)
and
[`NodeShared.cc`](https://github.com/gazebosim/gz-transport/blob/41913b9dfc3dbce33ac5464ddcb427cc82450f26/src/NodeShared.cc).
Native typed publication queues matching handlers to one process-global
PublishThread. Native PublishRaw directly invokes local callbacks on the caller.
The Rust API is explicitly asynchronous and serialized per Node. It does not
claim native process-global ordering, raw inline execution or general callback
latency/throughput equivalence. Independent Rust Node ownership/wire identities
keep their existing limits; no global registry merge is introduced.

A bounded canonical SDK probe observed thirty-two typed registrations sharing
one non-caller callback thread, with eight process tasks before/after registration.
A blocked native callback also blocked another native Node's typed callback;
queued unsubscribe and self-unsubscribe succeeded. Raw publication entered its
callback before returning. These are scheduling/lifetime observations, not a
performance benchmark. Both unchanged native Lambda and LambdaMessageInfo
originals passed. The Rust ports preserve Int32 value 5, `/foo`, successful
registration/publication, HasConnections readiness and every metadata predicate.
The metadata case retains its lock before Publish; the ordinary lambda locks
after Publish. Readiness retains native waitUntil's ten-second limit and ten-ms
polling interval. A one-second explicit Rust completion bound translates the
native untimed condition waits; it establishes no general native deadline
equivalence.

Author controls cover shared execution/last-worker recreation, malformed and
wrong-type callback-entry protection, options, self-drop, Node/unsubscribe
retirement, blocked callback IO/other-Node progress and real C++ publication.
FIFO/backpressure distinguishes counted command rejections from exactly two full
data-queue drops. Cancellation during an already entered callback permits its
completion but never enters queued value 6 after selected value 5. Pure controls
exercise retained capture/thread limits, spawn/disconnected rollback, finite
panic-payload chains and concurrent final retirement/re-admission. No existing
assertion/data/timer is weakened; private old fixtures only supply an empty weak
slot and ordinary destinations without notifiers.

Author debug and release runs each passed nine pure dispatcher controls and
eleven public/native controls. The affected debug regression run passed all 163
then-current library tests, 111 unchanged interoperability tests and the ten
then-current public callback tests; the later additions are two pure controls
and the durable native scheduling probe. All-target Clippy and formatting pass.
The focused instrumented nine-plus-eleven run covers all 314 executable LCOV
lines in the new module. Its raw LLVM summary is 310/333 lines and 413/457
regions, including generic instantiations; this focused run is diagnostic, not
full crate coverage or staged acceptance. The unchanged strict full gate must
account for every production file and all changed executable lines.

Independent integration review approved the complete implementation, ownership
and selected-original mappings. A fresh source/target and six freshly built
canonical peers passed the intended serial profile: 165 library, 111 unchanged
interoperability and 11 public callback tests; release passed nine pure plus
eleven public controls. Both unchanged native originals and all-target Clippy
passed. The final readiness correction independently replayed all eleven public
tests in both profiles. The first peer build lacked the explicit native source
include path and was discarded as setup-only; the corrected canonical build and
linkage are the qualified evidence.

An exploratory default-parallel run failed two unchanged native one-shot query
headers before Rust field comparison. A fresh unchanged-base parallel control
reproduced the publisher-header failure; the service-header failure was not
separately reproduced on that base. Both remain retained outside qualification.
The existing required serial profile passed all 111 without assertion, timeout
or setup changes. The first replay guidance omitted that serial flag; this is
not a waiver or a claim that default-parallel behavior has been repaired.

Only PubSubSameThreadLambda and PubSubSameThreadLambdaMessageInfo are proposed
whole-case ports: candidate ledger totals become 113 faithful, seven partial and
298 pending. Normal full staged coverage and exact hosted qualification still
remain mandatory before acceptance. All broader ordering, identity, raw-inline,
latency and throughput limits above remain open.
