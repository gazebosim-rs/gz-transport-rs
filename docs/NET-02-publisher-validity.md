<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Publisher construction validity

Pinned `NodePubTest.BoolOperatorTest` at `src/Node_TEST.cc527–541` creates two
Nodes, mutable and const default Publishers, checks both false, then assigns a
Vector3d `/foo` advertisement to the mutable Publisher and constructs a second
const Publisher from the other Node. Both advertised Publishers must be true.
The literal topic is `/foo` (`g_topic`, line 48). Those are the complete four
predicates; the original publishes no message. Both native bool overloads call
`PublisherPrivate::Valid`, which checks only a nonempty topic.

Rust Publisher handles come from fallible Node factories and have no public
default constructor or boolean conversion. The intentional API translation
keeps two Nodes and the false/false/true/true construction order through real
factory Results. Empty-topic Vector3d factories return `InvalidName`; the mutable
Result is then replaced by the successful `/foo` factory, and an immutable second
Result comes from the other Node. These failures express empty-topic admission,
not actual C++ default-object states or literal predicate parity. A public
`Publisher::default()` compile-fail control records the unrepresentable state,
and a compiling typed factory explicitly returns a real Publisher.

The separate supplemental test checks both Nodes' actual advertised topics and
worker publication records, including the qualified name and Vector3d descriptor.
It uses both immutable handle borrows to publish distinct Vector3d values and
checks bounded local receipts. These additional observations earn no upstream
case credit and do not turn construction validity into readiness, remote
delivery or liveness. No production API, invalid object, default, dependency,
wire format or prior test changes are included.

Author checks pass both public tests and the Publisher compile-fail Rustdoc in
debug and release, the unchanged native original, all 42 tooling checks and
workspace all-target Clippy. An actual compiler check using the same built
library accepts the public typed factory and rejects `Publisher::default()`
solely with E0599. Independent fresh-source/own-target review passes those same
debug/release public/Rustdoc controls, the unchanged native original, 42 tools,
Clippy and same-library compiler controls. The complete native constructors,
Valid and both bool overloads have been reviewed with no findings. Initial
sandbox socket and compiler search-path failures were excluded setup; the proper
fresh-source host runs qualify. Only the normal full staged/exact hosted checks
remain pending. Exactly this original row is conditional: accepted 124 faithful,
7 partial and 287 pending becomes 125/7/286 only after those checks pass.
