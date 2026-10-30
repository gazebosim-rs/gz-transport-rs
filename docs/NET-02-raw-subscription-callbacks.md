<!--
Copyright (C) 2026 Open Source Robotics Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Asynchronous opaque callbacks

Independently reviewed candidate, not accepted. `subscribe_raw_callback(topic, type_name, callback)`
and its `_with_options` variant borrow the admitted payload bytes and MessageInfo
for one invocation. An empty type hint is the existing raw receiver wildcard;
nonempty hints and SubscribeOptions use the same admission/filter path. Bytes
are not decoded, reencoded or normalized. Empty, malformed application bytes,
all byte values and unknown protobuf fields remain opaque data. Existing frame
size/protocol validation still applies before delivery. No new type-hint policy
or change to raw receivers is introduced.

Raw and typed callbacks share the same demand-created per-Node dispatcher,
retained-entry and callback-thread budgets, coalesced wake queue, serial user
execution and cancellation/Drop contract. There is no worker per registration,
new queue kind, pool, dependency or wire format. A private registration helper
factors the already qualified typed path; its decode still precedes typed user
entry, while the raw adapter directly borrows bytes and actual metadata.
Returned errors, unwind panics and finite capture/payload disposal panics use
the existing diagnostic/real-exit handling outside runtime locks. Arbitrarily
nonreturning user code and Rust abort/double-panic-abort remain uncontained.

The API is asynchronous. The prior native scheduling control establishes native
local PublishRaw enters local callbacks inline, whereas typed Publish uses the
native process-global pubThread for typed and raw handlers. This extension does
not assert raw-inline or global ordering/identity equivalence. It does not touch
Node runtime statistics, metrics or its pending enable/disable policy.

A freshly compiled canonical SDK probe holds an entered typed callback, calls
Node.Unsubscribe concurrently, observes success before release, then finishes
that callback; the same raw-handler case produces the same actual observations.
This safely confirms that removing queued handlers does not revoke a selected
snapshot. It does not reset dangling userdata or inject malformed native data.
The normal native helper audit checks its actual canonical Gazebo linkage.

The unchanged full CallbackScope.CleanupCorrectly original passes with both its
typed and raw phases. The Rust candidate preserves default Int32, both topic
names, two immediate publications, Node-wide unsubscribe and THEN logical pointer
reset in each phase, with the first Node/publisher retained through the second
phase. No readiness or completion wait is added before reset. Safe shared Option
storage represents the logically nullable unique_ptr captured by reference;
reset still makes the pointer absent. A failure marker is set before a null
assertion, then checked outside panic isolation. Owned handle Drop comes only
after logical reset: it keeps storage alive to observe a late failure rather
than making the null predicate impossible or waiting before reset. This setup
translation and whole-body fidelity have independent assertion approval; normal
staged and exact hosted qualification still precede case credit. No callback count or new native teardown guarantee is inferred from the
original's weak positive observations.

Separate positive controls prove real entry, payload/metadata fidelity, opaque
byte forwarding and typed/raw sharing. An actual C++ peer sends opaque bytes
00ff0100, empty data and canonical Int32 wire to the new raw callback, checking
all received bytes and remote metadata. Type-hint/ignore-local controls check
quiet observations and an independent entry counter before a later positive
malformed-application-byte delivery. Pure tests check borrowed pointer identity,
FnMut state, options admission, shared worker identity, spawn/failure rollback,
and error/panic/capture-drop progress. All 18 pure controls and six public/native
controls pass in debug and release. The affected serial debug run also passed
all 174 library tests, eleven unchanged typed callback tests and five unchanged
owned-subscriber controls, with freshly rebuilt canonical peers. Formatting and
all-target Clippy pass. A pure instrumented run covers 379/380 executable LCOV
lines in the existing callback module; its sole uncovered early-Shutdown branch
is outside the raw changes and is exercised by the public lifecycle regression.
This is diagnostic rather than full coverage acceptance.

Independent SDF terra review used fresh source/target, passed all 18 pure and six
serial public/native controls plus all-target Clippy, rebuilt eight canonical
peers and verified raw-peer linkage. The unchanged full CallbackScope
original passed one case with no failures. Source and original-body comparison
approved both exact phases, nullable state/reset order, external failure marker
and the owned Drop translation; no source finding remains. Only that one whole
original is proposed for credit, conditionally moving 116/7/295 to 117/7/294.
Normal full staged coverage and exact hosted qualification remain required.
All global/raw-inline, in-flight and performance limits above remain explicit.
