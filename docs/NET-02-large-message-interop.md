<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Two-process large Bytes payload

Pinned `twoProcPubSub.PubSubLargeMessageAboveShmThreshold` at
`test/integration/twoProcsPubSub.cc:615` starts the unchanged publisher auxiliary,
resets its observation state, subscribes to `/large_msg` and sleeps for 2500 ms.
The complete assertions require successful registration, callback execution,
exactly 256 * 1024 received bytes and the complete string of `S` bytes. Its
callback copies `Bytes.data()` before setting its flag. No exact receipt count,
acknowledgement, throughput or connection-lifecycle predicate is present.

The auxiliary advertises Vector3d `/foo` and Bytes `/large_msg`, with `(1,2,3)`
and 256 KiB of `S`. It sleeps 500 ms, publishes both, sleeps another 1500 ms,
publishes both again and sleeps 1000 ms before destruction. The Rust receiver
uses that exact unchanged SDK executable and retains the original 2500 ms wait
without an extra readiness, delivery or flush barrier. Fallible typed
registration translates the native successful Subscribe result. A shared owned
observation replaces reset global state; one mutex reads its flag and complete
payload coherently. The subscriber and its Node retire before joining the
publisher child, retaining the original destruction order.

The reverse test uses a small native receiver retaining every original payload
predicate and its 2500 ms wait. Its `READY` line identifies completed subscription
setup before the Rust publisher starts the unchanged 500/1500/1000 ms schedule;
it does not wait for a connection or payload. Rust constructs both original
message types and topics. Successful publication admission is supplemental:
the original auxiliary does not assert its Publish return values. Both receivers
also retain a persistent corruption flag, so a later valid message cannot hide
an earlier wrong payload. These additional controls earn no original credit.

Every peer uses child-only partitions and two distinct UDP discovery ports held
for the whole test by reusable wildcard sockets with multicast-all disabled.
The original native main replaces its partition but preserves these port
overrides. No host environment is mutated. Child output is bounded to 64 KiB;
the reverse setup marker has a five-second bound and child joins have a
ten-second watchdog that kills and reaps failures. These cleanup/setup bounds
do not replace the original receipt wait. Mutex-protected payload state also
avoids the original global string's unsynchronized callback access.

The native name refers to its 128 KiB SHM threshold. The selected canonical
reference is the qualified ZeroMQ profile, and Rust uses its existing native
wire protocol with a default 16 MiB message bound. This slice qualifies the
actual fixed payload assertions over two processes; it implements no SHM path
and makes no shared-memory, Zenoh, zero-copy, performance or general large-message
delivery claim. Config bounds, protocols, defaults, dependencies, runtime source,
all old tests and the 22 immutable NSS controls remain unchanged.

Author debug and release replays each pass both directions; the unchanged full
native original also passes. The helper resolves all three Gazebo libraries in
the canonical SDK. All 42 tooling controls, formatting and workspace all-target
Clippy pass. Root independently reviewed the complete original/helper bodies,
all six paths, waits, latches and cleanup with no findings. Its fresh accepted-46
archive plus exact six-path overlay and reviewer-owned source/target passes both
directions in debug (6.03 s, zero skips), using a freshly built helper whose three
Gazebo libraries resolve inside the canonical prefix. That focused review does
not claim independent release, tooling, Clippy or unchanged-original execution.

Exactly this original is a conditional mapping: accepted 127 faithful, 7 partial
and 284 pending becomes 128/7/283 only after the normal full staged gate and exact
whole hosted success with downloaded source evidence. Neither the reverse
direction nor the corruption latches earn additional original credit. Those
mandatory staged/hosted checks remain pending.
