<!--
Copyright (C) 2026 Open Source Robotics Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Bounded owned metric values

`TopicStatistics::fill_metric(&mut Metric, max_encoded_bytes)` formats the
accepted owned topic/scalar values into the existing pinned `gz-msgs` generated
`Metric` type. It reads no clock, enables no Node statistics, retains no wire
frame and publishes nothing. Runtime enable/disable policy and publisher
lifecycle remain a separate owner decision; this value operation does not
choose that policy.

The source is pinned native
[`TopicStatistics::FillMessage`](https://github.com/gazebosim/gz-transport/blob/41913b9dfc3dbce33ac5464ddcb427cc82450f26/src/TopicStatistics.cc).
Like native, each call overwrites the unit with `milliseconds`, appends one
`SAMPLE_COUNT` statistic named `dropped_message_count`, and appends three groups
in order: `publication_statistics`, `reception_statistics`, `age_statistics`.
Each group appends average/minimum/maximum/population-standard-deviation values
with native enum types and names. Period groups express the average as
`1000 / average_period`; the other values remain milliseconds. Native spellings
`period_standard_devation` and `age_standard_devation` are preserved. Existing
header, statistics and groups remain in their original order, including any
nested headers. Calling again appends another thirteen statistics rather than
replacing the earlier ones.

Default sample extrema retain native `MAX`/positive-minimum sentinels. Empty or
zero-period samples yield positive infinite average frequency. Floating values
are passed to the generated double fields without clamping, filtering or custom
encoding. This boundary also preserves synthetic NaN/infinite scalar bits via
ordinary protobuf encoding; public unsigned timestamp updates do not themselves
accept arbitrary floating samples. Dropped unsigned counts convert to double
in the same manner as native.

The explicit byte budget bounds the final encoded message, including its caller
prefix. Native has no such output limit. The implementation builds only the
fixed thirteen-statistic additions, uses maintained prost `encoded_len` to
subtract the old unit contribution and account for the replacement/additions,
and checks the final sum before changing the message. Overflow or a result
larger than `max_encoded_bytes` returns `Error::QueueFull`, preserving the
caller's complete encoded bytes. An exact budget succeeds; zero rejects even
an empty input. A long old unit may shrink, so the old total alone does not
reject a valid result. The fixed additions require bounded temporary storage;
the existing caller prefix is traversed for its encoded size without cloning
or truncation. This budget bounds encoded output, not allocator overhead or
arbitrary memory the caller already owns. Ordinary Rust allocation-failure
behavior is unchanged.

Four complete native wire results are checked live against Rust in debug and
release: default395 bytes, populated442, prefixed501 and repeated870. A separate
pure helper supplies controlled monotonic milliseconds20/30/38 only to the native
child, asserts each actual clock observation, checks native reception/age
samples and preserves unrelated clock calls. This is controlled-clock value
and encoding parity, not network timing/performance or unchanged-original
evidence. The prior uncontrolled study and original TopicStatistics tests are
unchanged. The generated codec owns field encoding and unknown-field policy;
this operation makes no additional raw unknown-field preservation promise.

Independent fresh source/target and newly compiled canonical SDK helpers passed
sixteen public/native checks and five private checks in both debug and release.
The reviewer verified actual helper linkage, active C++ assertions/interposition,
child-only preload and the four complete SDK wires. A test tightening adds an
explicit retained nested negative-zero bit comparison; its final overlay passes
both profiles. A separate reviewer-only Unicode/varint-prefix budget control
also passes against actual encoded bytes. That extra control is independent
review evidence, not a claim that it is a committed public fixture.

No dependency, wire receive path, enable/disable behavior, Node operational
counter or ledger disposition changes. Review has no remaining findings; the
normal staged coverage gate and exact hosted checks remain required before
acceptance of this formatter.
