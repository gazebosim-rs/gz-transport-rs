# Message publication rate options

Exact source and independent source/control/full-evidence reviews passed.
Final metadata binding, the mandatory normal protected hook and exact new hosted
success remain required for acceptance.

`AdvertiseOptions::set_msgs_per_sec` sets an unsigned 64-bit publication rate.
The existing default is `u64::MAX`, the native unlimited sentinel. Zero is a
throttled rate that suppresses every eligible payload; successful suppressed
`publish` and `publish_typed` calls return `Ok(())`. Other positive rates share
one limiter for typed and raw calls through the same Publisher. An ordinary
borrow shares this state; no new Publisher clone/copy API is introduced.

`throttled_update_ready()` queries eligibility as `Result<bool, Error>` without
advancing the timestamp. It does not inspect cancellation or acknowledge queue
admission or delivery. Publication retains descriptor/size validation and
terminal cancellation errors. An admitted interval advances before bounded
queue submission, so a later queue error does not roll back the timestamp.

The qualified Linux implementation uses the existing pinned nix dependency's
safe `CLOCK_MONOTONIC` binding. Its timestamp begins at the clock epoch, matching
the pinned native Linux steady clock rather than the time of advertisement. The
native period is a double `1e9 / rate`, compared with integer elapsed nanoseconds
converted to double. Fractional periods and positive periods shorter than one
nanosecond stay distinct from zero and the unlimited sentinel. Clock errors are
explicit; other platforms need separate clock qualification.

The six fixed originals are NodeTest.PubThrottled and the message-options default,
copy, assignment, equality and accessor cases. The rate original retains first
readiness true, exactly three successful 1 Hz publications, false readiness after
each call, each original 100 ms sleep, and the live callback count of one at the
original checkpoint. The owned observer verifies Int32 data 5 and stops/joins
only after that checkpoint or during failure unwinding. The native synchronous
callback facade is adapted to an owned Rust observer; no immediate callback
completion or scheduler equivalence is promised.

Six native originals and sixteen typed/raw edge runs passed before production
edits and received independent native-only review. Rust supplements exercise
zero-rate suppression, shared typed/raw intervals, descriptor/size/cancellation
errors, clock failure, fractional and sub-nanosecond boundaries, and pure queries.
The installed C++ steady-clock epoch is bracketed by direct monotonic samples
and the Rust clock. Native TopicInfo independently checks the actual Rust
advertisement's rate/throttle fields and receives correct payloads for eight
rates in typed and raw modes. Historical native META output came from a separate
FillDiscovery serialization and remains qualified only as such.

The missing mandatory native peer from the first selected run is retained with
its exact source and failure log. Building that unchanged peer allowed the same
selected assertions to pass. Metadata assembly errors are also retained; none
waives a candidate test, assertion or coverage requirement.

Native readiness and update use separate locks and clock samples; the Rust
limiter serializes its check/update with one lock/sample. Exact concurrent races,
clock precision, physical early-boot execution, Publisher copy construction,
stream formatting, subscriber/service rates, other backends, the remote
PubThrottled integration original, and performance remain unqualified. High-rate
observations do not establish zero-duration or unlimited behavior.

The exact source gate passes 247 Rust tests and 24 tooling tests, native Node 61,
message options 5 and all other required groups. All five production sources
exceed the unchanged strict coverage thresholds: 2,770/2,848 lines and
4,065/4,256 regions; all 73 added executable lines are covered. All 210 source
reports are retained. Independent [review](reviews/publisher-throttle.json)
rebuilds fresh raw sources, repeats native six / Rust 98 selected cases with
sixteen native scenarios, checks five production mutations at their intended
assertions, restores all tests and hashes, and recomputes unchanged full JSON/LCOV
and authoritative changed-line policies. No filtering or failure waiver applies.
