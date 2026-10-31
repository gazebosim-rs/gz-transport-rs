<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Scoped publishers and remembered subscriber interest

Pinned `twoProcPubSub.PubSubTwoProcsScopedPub` retains one Node across two
iterations. Each starts a fresh native subscriber process before advertising a
Vector3d `/foo` publisher. The original waits for HasConnections with the native
10-second/10-millisecond bounds, publishes `(1, 2, 3)` successfully, destroys the
Publisher, sleeps 100 milliseconds and joins the child. The child has both typed
and raw subscriptions; it checks descriptor/parse success and all three values,
unsubscribes and checks both callback flags.

The Rust port runs that unchanged subscriber executable. A successful typed
factory represents the valid Publisher. `connection_snapshot()` reads the same
known matching registration interest without a worker-query wait, preserving the
bounded readiness predicate. Publication success remains queue admission; the
actual native child's assertions establish receipt in this selected fixture.
There is no flush, acknowledgement or extra wait before the Publisher is dropped.
The native auxiliary's ten-second receive loop is unchanged. A separate
twelve-second child watchdog guarantees cleanup on a broken child; Drop kills
and reaps any unfinished process.

Partition and discovery ports are explicit fixture translations. Both processes
share a unique partition, loopback interface and held distinct ephemeral ports.
The unchanged original chooses its own partition but inherits those discovery
port overrides through the native subprocess wrapper. Its independent reference
run has a fifty-second whole-process-group watchdog. No host environment is
modified, and the reference's predicates and values are unchanged.

A separate private control retires the final Publisher before a generic remote
END, checks that the END still clears remembered registration, recreates the
Publisher with no stale cached interest and reuses a capacity-one registration
table. A late END for the old reader must not erase a new reader. Publisher
retirement alone does not invent subscriber withdrawal. Existing BYE retention,
explicit withdrawal and bounded snapshot policies remain unchanged.

A second private control freezes command processing after advertisement, admits
one publication into the capacity-one queue, drops its Publisher and executes
cleanup before dequeuing that exact command. An already-established external
ZeroMQ receiver then checks the single four-frame publication, exact raw bytes
and descriptor, and typed Vector3d decoding. Readiness probes establish only
this supplemental receiver; the original public test gains no flush or barrier.
This checks the worker scheduling boundary deterministically: cleanup removes
the advertisement, while the admitted command owns its payload and the wire
send remains independent of the retired lease. The lease guard applies only to
recording publication history. No duplicate send or failure is observed.

Author public one/private two controls pass in debug and release, the unchanged native
original passes, and all 42 tools plus workspace all-target Clippy pass.
No production defect has been reproduced and no production change is proposed.
Independent fresh-source/owned-target debug review passes the public and private
controls plus the unchanged native original under held unique ports. Source,
full auxiliary assertions and the gate addition are approved with no findings.
The additional deterministic queued/drop/cleanup private control also passes
independently on the reviewer's fresh-source overlay and owned debug target.
Initial sandbox EPERM was excluded setup; the authorized host replay passed.
The reviewer does not claim release or full-gate execution. Only this case is a
conditional candidate: accepted 125 faithful, 7 partial and 286 pending becomes
126/7/285 after the full staged and exact hosted checks. This is
membership and lifecycle evidence, not authentication, a general delivery
guarantee, performance or whole NET-02 completion.
