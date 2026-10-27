<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Subscription rate admission

This candidate adds a message rate to `SubscribeOptions`. The default
`u64::MAX` is unlimited; zero suppresses admission. Copying options copies the
value. Changing an options value after registration does not change that
registration. Existing default and ignore-local policies stay intact.

Each registration owns one shared limiter across same-Node, other-Node direct
and wire delivery. Existing descriptor and origin filters run first. Admission
uses the existing safe monotonic clock and epoch-zero, floating-period limiter;
it occurs before attempting the receiver's bounded queue. Separate registrations
have separate budgets. The unlimited default adds no limiter or clock query.

Suppression does not count as queue loss. An admitted attempt that finds a full
queue consumes its interval and increments the existing queue-drop statistic.
A clock error suppresses admission and increments the recipient Node's
`subscription_clock_errors`. These are explicit owned-queue contracts; the
native callback implementation does not establish queue or concurrency parity.

Before edits, four unchanged native originals passed and an independently
repeated native probe retained 64 typed/raw, local/peer and ignore-local cases.
The probe records bounded checkpoints after three publications 100 ms apart,
not settled counts or throughput. Native typed local callbacks honor
ignore-local while several raw callback/publication paths do not. All existing
Rust raw ignore-local assertions remain unchanged; those native raw divergences
are not mapped as matching Rust behavior.

The fixed ports are `NodeTest.SubThrottled` and the copy-constructor,
accessor and throttled cases in `SubscribeOptionsTest`. The same-Node original
retains three successful unthrottled publications, readiness checks, 100 ms waits
and a live observer count of one at the original checkpoint. Additional tests
cover recipient errors, filtering, independent registrations, queue admission,
local/other-Node sharing and real C++ wire delivery. The author selection passes 101 Rust checks and modified-target Clippy.
Independent [source review](reviews/subscriber-throttle.json) verifies fresh
185-file source, four native originals and 101 selected Rust checks. Four
production controls reject bypassed admission, unshared local budgets,
admission before rejection filters and suppressed recipient clock accounting;
the restored 101 checks and source/native hashes pass. The exact canonical
source gate passes 259 Rust and 24 tooling tests, 2829/2907 lines and 4146/4337 regions;
47/47 changed executable lines are covered.
All 212 source reports are retained. Full unfiltered
JSON/LCOV and authoritative changed-line policy are independently recomputed.

No physical early-boot experiment, fractional precision boundary experiment,
callback alias or lifetime equivalence, mismatch ordering, concurrent callback
behavior, settled count, remote count or performance equivalence is claimed.
Final metadata review, mandatory normal hooks and exact new hosted CI remain
required before acceptance. This candidate maps four fixed originals; all 19
partial originals and the broader NET-01/NET-02 gaps remain open.
