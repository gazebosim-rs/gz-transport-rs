<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Base advertisement option values

Five fixed native `AdvertiseOptionsTest` originals map to the existing Rust
`AdvertiseOptions`: default construction, copy construction, assignment, equality
and accessors. Their nine predicates inspect All, Host and Process scope values.
A separate Rust supplement reads the assignment destination's initial All value
before overwriting it. This slice changes tests and evidence metadata; the
production API and behavior remain unchanged.

Every original input keeps the combined Rust message rate at its unlimited
default. These fixed tests do not establish general equality between native base
options and combined message options when rates differ. Base/service streams,
service options, copy/destruction/lifetime, network scope and performance remain
unqualified. No additional case or broader NET packet is retired.

Before the test edits, five unchanged native originals passed. Independent fresh
native replay reproduced the same five cases and qualified all source, executable
and 25 linked-library identities. The author selection passes five Rust tests
and targeted Clippy. Independent [source review](reviews/base-advertise-option-values.json)
checks a fresh 188-file export, all old parent blobs, the exact native filter
additions and the nine original predicates plus separate supplement. Production
controls change default scope, scope assignment and equality; each fails its
intended original predicate. Restored tests and source/compiler hashes pass.

The exact canonical source gate passes 264 Rust and 24 tooling tests, 2829/2907 lines and 4146/4337 regions.
Every production denominator remains present; all complete line and region
thresholds pass unchanged. There is no production source delta or changed-line
coverage credit. All 213 source reports are retained;
full unfiltered JSON/LCOV and authoritative delta policies are independently
checked. All earlier tests, assertions, pins and the 19 partial originals remain.

The initial linked-library commentary count, stale auxiliary limits sentence,
receipt schema/readiness guards and control/helper or reviewer path assembly
errors remain recorded with their corrections. Those corrections changed only
metadata or auxiliary helpers; no runtime failure, assertion relaxation or
coverage waiver is concealed. Final metadata binding, the mandatory normal
protected hook and exact new hosted success remain required before acceptance.
