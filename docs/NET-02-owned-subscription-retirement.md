<!--
Copyright (C) 2026 Open Source Robotics Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Per-handle subscription retirement

Reviewed candidate, not accepted. Its preceding asynchronous typed callback
API is accepted at b860b64 after normal staged and exact hosted qualification.
This follow-up still requires its own normal gate and exact hosted success.

`CallbackSubscription::unsubscribe()` retires the admitted lease and returns
whether this call performed its active-to-retired transition. Concurrent calls
have one winner. It refreshes the existing interest cache and wakes the shared
dispatcher, using the same path as `cancel()`. The existing `cancel() -> ()`
contract remains idempotent. Neither operation waits for selected user code or
capture destruction. `Drop` remains the real-exit boundary, with its documented
self-dispatcher exception. Cancelled retained entries still consume their bound
until callback/capture destruction actually finishes; releasing a registration
slot does not prematurely release this separate bound or its thread permit.

`is_active()` observes the admitted lease and Node stop/owner-retirement flags.
It makes no IO query and promises no delivery or callback completion. False
includes handle unsubscribe, Node-wide unsubscribe, shutdown and owner death.
A concurrent retirement can follow a true result. Owner stop can make this
query false while a blocked invocation still retains its active lease;
`unsubscribe()` can then return true when it retires that lease. The retirement
boolean is not the inverse of the live-state query. No new thread, queue, heap
allocation, global registry or wire field is introduced.

This is intentionally a live Rust predicate. Pinned native `Subscriber::Valid`
in Node.cc201-230 checks stored nonempty topic, Node and handler IDs. Native
per-handle Unsubscribe at263-278 invalidates those fields after its operation;
Node-wide unsubscribe does not mutate a still-held Subscriber's ID fields.
Therefore native stored-ID validity and actual registration liveness are not
synonyms. A freshly built canonical native control measured those states and held-handle
owner destruction: own unsubscribe returns1, validity becomes0, repeat returns0;
Node-wide unsubscribe returns1 while the held native validity stays1, and a
later held-handle unsubscribe still returns1. After owner destruction a held
native handle also remains valid and its unsubscribe returns1. These observations
are retained as a normal public/native test and do not redefine the Rust live
predicate.

The selected BoolOperatorTest states translate default-invalid native handles
into Rust `Option<CallbackSubscription>::None`, including the immutable invalid
handle. They preserve successful Int32 `/foo` callback creation, active state,
successful per-handle unsubscribe, inactive state and a second immutable active
handle. This does not claim every native Valid state maps to `is_active()`.

The CreateSubscriber original retains Int32 value 5, `/foo`, successful
registration/publication, its default-invalid slot then assignment, connection
readiness, asynchronous completion, scope
retirement and the final300ms conditional wait. Its executed flag is never reset
before that final wait, and it has no post-drop false assertion. A separate
supplement resets its flag, waits 300 ms and checks an independent entry counter
never changes after scope exit. The original's weak predicate is not rewritten
or presented as that stronger proof.

The mixed-API original preserves three callbacks, all first deliveries, explicit
handle2 unsubscribe, callback1/3 continued delivery and callback2's 300 ms quiet
observation, then Node-wide unsubscribe and three separate 300 ms quiet checks.
Holding the first Rust owned callback to the Node-wide unsubscribe point
translates the lifetime of native legacy Subscribe; this does not introduce a
second callback execution API. All callback entry counters increment before
payload assertions so invocation cannot be hidden by panic isolation. Native
waitUntil readiness retains its ten-second limit and ten-ms polling interval.
Only native completion condition waits are untimed; those use explicit
one-second Rust completion bounds. The original quiet durations remain 300 ms. No global callback ordering or raw-inline claim.

Five new pure controls and the nine existing dispatcher controls pass in debug
and release, including deterministic stopped-owner/retained-lease distinction. They
exercise concurrent exactly-once retirement, selected-work retained capacity,
self-unsubscribe with queued work suppressed, owner stop and old cancel
idempotence. Formatting and all-target Clippy pass. All five final public/native controls pass in debug and release (1.52 seconds
per profile), including the exact-output held-ID probe wrapper. All three
unchanged native originals pass, and the freshly compiled probe resolves all
Gazebo libraries inside the canonical SDK.

Independent SDF terra source/lifecycle/assertion review approved all eight paths.
A fresh reviewer source/target passed all 14 pure and five public/native controls,
including the actual stored-ID peer, plus all three unchanged native originals.
The final native readiness correction independently replayed the five public
controls. The review also requested and verified the deterministic stopped-owner
case above. No production finding remains.

Only BoolOperatorTest, PubSubWithCreateSubscriber and PubSubWithMixedSubscribeAPIs
are proposed whole-case ports. Conditional on preceding callback acceptance,
ledger totals would move from 113 faithful / 7 partial / 298 pending to
116 / 7 / 295. Native stored-ID validity remains different from Rust liveness;
the weak original post-drop predicate remains weak, and stronger evidence stays
separate. Normal full staged/exact hosted qualification is still required; no
publication or acceptance is claimed by this reviewed candidate.
