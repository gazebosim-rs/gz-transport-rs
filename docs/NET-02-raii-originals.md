<!--
Copyright (C) 2026 Open Source Robotics Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Two retained RAII lifetime setups

The pinned native `NodePubTest.DestructionOrder` and `NodeTest.MoveSubscriber`
bodies in
[`src/Node_TEST.cc`](https://github.com/gazebosim/gz-transport/blob/41913b9dfc3dbce33ac5464ddcb427cc82450f26/src/Node_TEST.cc)
have no EXPECT or ASSERT predicates. Their complete setup/destruction behavior
is retained, and both unchanged native originals are executed by the normal
verifier. Source and runtime evidence for these two small fixtures does not
establish general callback scheduling, process-global ownership or delivery.

The publisher body declares an initially invalid outer Publisher, constructs an
inner Node, advertises Vector3d on `/foo`, destroys the Node and finally destroys
the Publisher. Rust uses an outer `Option<Publisher>` initialized to None because
an invalid Publisher is not a public Rust value. Successful typed registration
replaces that slot before the inner Node dies. No publication or timing predicate
is added to the original mapping. An added None assertion merely checks the
explicit setup translation.

The subscriber body constructs one Node and moves its newly created Int32 `/foo`
subscriber into a vector. The vector is destroyed before the Node. Native's
callback is never invoked by this no-publication original. Rust's owned receiver
therefore preserves the registration, type, move and destruction relationships
without claiming to implement a subscription-callback API. Result admission must
succeed in both mappings. Unique Rust partitions and simultaneously reserved,
pairwise-distinct ephemeral discovery ports isolate setup; no claimed native
assertion depends on a specific partition or port number.

The separate supplements exercise actual existing runtime behavior. Moving a
receiver out of its vector preserves actual Int32 value5 delivery, then its final
drop removes the interest. A newly admitted receiver retains its already queued
value5 across explicit Node shutdown, then returns Shutdown; later publication
is rejected too. A worker-command barrier fixes the supplemental queued-drain
setup and is absent from both original variants.
A surviving Vector3d publisher sends values1/2/3 to a live receiver after its
owner Node is gone, then releases/joins its worker and removes its registration
after the last handle drops. These tests bound receipt to one second, readiness
and last-worker retirement to two seconds. The native originals are untimed;
these bounds belong to the supplemental controls. Prior stronger owned-lifetime
and real C++ peer tests remain unchanged, including explicit cancel, drained
queued receivers and concurrent inspection.

Independent review using a reviewer-owned fresh target passed all four Rust controls and both
unchanged native originals with proper loopback/interface access. The reviewer
compared both complete native bodies, exact message types/setup and the gate's
additive filter, and found no outstanding issue. All reviewer processes exited.

No production source, public API, dependency, wire format, coverage ownership or
exclusion changes. Exactly DestructionOrder and MoveSubscriber gain candidate
whole-case credit: 109 faithful /7 partial /302 pending becomes 111/7/300 only
after the normal staged and exact hosted checks succeed. Broader native lifetime,
callback and global-identity contracts remain open. This additive packet is based
on the exact hosted-accepted formatter
`fa92d2a96e4e8f600c9bbbfa544a883eaa8ffe8c`; its helper and prose are preserved.
