<!--
Copyright (C) 2026 Open Source Robotics Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Scalar sample statistics

`SampleStatistics` owns the count, mean, population standard deviation and
extrema of scalar `f64` samples in constant space. It is separate from the
operational counters returned by `Node::statistics()`. Copies have independent
value state. No Node, clock, sender map or protobuf message is involved.

The compatibility source is `Statistics` in pinned Transport
`41913b9dfc3dbce33ac5464ddcb427cc82450f26`,
[`src/TopicStatistics.cc`](https://github.com/gazebosim/gz-transport/blob/41913b9dfc3dbce33ac5464ddcb427cc82450f26/src/TopicStatistics.cc)
and
[`include/gz/transport/TopicStatistics.hh`](https://github.com/gazebosim/gz-transport/blob/41913b9dfc3dbce33ac5464ddcb427cc82450f26/include/gz/transport/TopicStatistics.hh).
The small owned port retains its OSRF 2020 attribution and Apache-2.0 license.
It uses standard floating-point operations and `sqrt`; there is no general
statistics framework or new dependency.

Empty count, mean and deviation are zero. The native minimum sentinel is
`f64::MAX`, and the maximum sentinel is **positive** `f64::MIN_POSITIVE`.
Consequently an all-negative, zero or sufficiently small positive stream can
retain that positive maximum. Comparisons retain the existing operand on ties
and NaN input, including signed-zero extrema. These values are preserved rather
than normalized into conventional extrema.

Every sample advances the unsigned 64-bit count, including NaN and infinities.
The update keeps the native order: count increment, mean, extrema, then
unnormalized squared-mean distance (M2). The getter divides M2 by count before
taking its square root. It reports population rather than sample deviation.
The count wraps explicitly, as native's unsigned field does; at wrap the update
has a floating-point zero denominator, while the zero-count deviation getter
returns zero. A private synthetic boundary test checks that policy in both Rust
build profiles. It does not claim to have executed 2^64 native samples or to
have mutated native private memory. There is no new exhaustion error.

## Library reuse boundary

Before this compatibility port, actual native values were compared with stable
builds of average 0.16.0/0.17.0, streaming-stats 0.2.3, welford 0.1.0 and
rolling-stats 0.8.0. The initial 604-state corpus found only one/two-ULP finite
rounding differences for streaming-stats, with no original-tolerance failure;
those differences alone did not justify custom code. Longer adversarial
families exposed irreversible underflow differences instead.

Let A have bits `9e70000000000000` and B have bits `9e50000000000000`
(approximately -4.4455e-162 and -1.1114e-162). The reduced native/library
controls reproduce these zero/nonzero distinctions:

| Library API | Inputs | Native deviation bits | Library deviation bits |
| --- | --- | --- | --- |
| streaming-stats population | A, B, +0 | `1e60000000000000` | `0000000000000000` |
| average 0.16/0.17 population | A, A, B | `1e60000000000000` | `0000000000000000` |
| welford sample-to-population adapter | A, A, A, B | `0000000000000000` | `1e60000000000000` |
| rolling-stats sample-to-population root adapter | A, B | `0000000000000000` | `1e56a09e667f3bcd` |

Streaming-stats stores normalized variance between updates; it cannot recover
the lost M2. Welford and rolling-stats hide M2 and expose rounded sample
variance or its root, so population conversion also loses information.
Average exposes population variance but uses a different M2 recurrence, which
loses the reduced case. Repairing these APIs would require another accumulator
or retained history. This port preserves the native scalar operation order
without either. It does not widen the original tolerances or treat zero as
close enough to the measured nonzero result.

The evaluated streaming-stats package was exactly 0.2.3, crate checksum
`b0d670ce4e348a2081843569e0f79b21c99c91bb9028b3b3ecb0f050306de547`,
upstream commit `aaa17b69c141788ceb62b8f9a40ebbb63cedce07` in
[BurntSushi/rust-stats](https://github.com/BurntSushi/rust-stats/tree/aaa17b69c141788ceb62b8f9a40ebbb63cedce07).
Its license is MIT/Unlicense with Andrew Gallant's 2015 MIT copyright. No crate
source is copied or relabelled here, and no evaluated package is added to this
repository. Its 2020 release remains the newest stable and the repository is
not archived as checked on 2026-10-29; low recent activity was a consideration,
not the reason for the semantic rejection. The dependency graph remains
unchanged.

## Verification and limits

The pure C++ helper constructs the actual pinned native `Statistics` value,
accepts the exact input bits, and emits every getter after every update. It
constructs no Nodes or sockets. The normal helper build and gate audit its
actual Gazebo library resolution inside the selected SDK. Tests retain 604
original investigation states and all reduced underflow controls, then compare
43,122 states from longer repetition, monotonic, near-constant, reciprocal,
subnormal, extreme, nonfinite and order families. The current canonical Linux
replay compares every returned count and floating-point bit. This is evidence
for those inputs and that SDK, not a universal cross-platform bit-identity or
two-ULP guarantee.

The native `TopicsStatistics.MinMax` and `TopicsStatistics.AvgStdDev` bodies
have complete Rust ports, retaining all values, assertions and the original
third-sample 1e-3 tolerance. The scalar constructor checks are supplemental:
`TopicsStatistics.Constructor` also requires TopicStatistics values and remains
pending. `TopicsStatistics.DroppedMsg` remains pending. TopicStatistics sender
maps, clocks, timing aggregation, Metric formatting/publication, runtime
integration and broader NET-01/statistics completion are outside this slice.
Independent review used a fresh source export, target and canonical helper:
two private and six public/native tests pass in both debug and release,
including all 604 retained, 16 reduced and 43,122 longer native getter states.
All four unchanged native originals also pass. The reviewer separately rebuilt
all five evaluated library versions and reproduced all six recorded zero/nonzero
counterexamples in both profiles. No findings remain. Only MinMax and AvgStdDev
gain candidate faithful-port credit (105/7/306 becomes 107/7/304); acceptance
still requires the strict normal staged gate and exact hosted checks.
