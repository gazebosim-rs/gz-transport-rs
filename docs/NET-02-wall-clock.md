<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Owned wall clock

`WallClock::new()` reads `SystemTime`, floors its signed UTC value to whole
seconds, then reads `Instant`. `time_ns()` adds checked monotonic elapsed time
to that immutable anchor. It returns signed nanoseconds without floating point,
threads, Nodes, endpoints, dependencies or unsafe code. Later realtime clock
adjustments do not move the anchor. Setup reads are synchronous; there is no
clock-reading worker or implicit process-wide singleton.

This follows pinned `src/Clock.cc` at
`41913b9dfc3dbce33ac5464ddcb427cc82450f26`: native WallClock samples
`std::time(NULL)` before `steady_clock`, then reports their fixed offset plus
current monotonic time. Retaining the whole-second anchor matters: directly
returning `SystemTime::now()` would preserve a fractional second that native
intentionally discards. Negative fractions floor toward negative infinity:
-1 ns anchors at -1 second, and -1.9 seconds anchors at -2 seconds.

Every successfully constructed object is ready, including one reporting zero.
Readiness does not promise that a future OS read succeeds. Rust construction is
fallible and creates no invalid/default-zero object. Each explicit construction
owns its own anchor; multiple Rust clocks can therefore have different phases.
Native `Instance()` shares its first anchor process-wide. Pointer identity or a
shared singleton is not a Rust API claim. Pinned Clock.hh exposes `Time` and
`IsReady`, not separate `TimePoint` or `Now` methods.

## Error domain

`new()` rejects an anchor outside signed 64-bit nanoseconds with
`Unsupported("wall clock anchor range")`. `time_ns()` rejects a reversed
monotonic sample with `Native("monotonic clock moved backwards")`, excessive
elapsed time with `Unsupported("wall clock elapsed range")`, and an overflowing
sum with `Unsupported("wall clock sample range")`. Failed reads never modify
the anchor and do not clamp, wrap or substitute a zero clock.

On the qualified Linux GNU/Rust 1.94.1 implementation, standard clock readers
panic on OS errors. A private `catch_unwind` around only those standard readers
returns `Native("wall clock source unavailable")`. It does not replace the
process panic hook, so the ordinary hook can still print a diagnostic. This
cannot contain `panic=abort`, an aborting hook or a second panic during unwinding.
There are no user callbacks or Rust FFI in this boundary. These explicit Rust
errors are not claimed equivalent to native unchecked OS-error or overflowing
signed-arithmetic behavior; neither is executed as a native oracle.

## Tests and present qualification

`wall_clock_original` retains the complete Linux `ClockTest.WallClock`
predicates: successful construction, readiness, and at least 100,000,000 ns
elapsed across the original 100,000,000 ns sleep. Owned construction replaces
native non-null singleton acquisition; no stronger accuracy tolerance or
Windows scheduling claim is inferred. The unchanged native case passes.

Seven actual SDK observations cover whole-second rounding, zero readiness,
negative timestamps and a later realtime step. Each native child constructs
only WallClock and checks singleton identity; its counters establish one
`time()` call and three monotonic reads. Two independent realtime observations
also prove the supplied step was seen. The Rust children use standard clocks
and a test-thread-scoped interposer: a child-owned marker selects its Linux TID,
while the test harness and unrelated clocks pass through. Rust observes the
same ready/first/last values with one realtime and three monotonic reads. All
observed values, stdout/stderr and source/library provenance remain in
`tests/fixtures/wall-clock.json`; the normal verifier rebuilds the native
helper and the fixture test replays it. The interposer is set only on helper children, never the parent or host.
This is controlled-clock value evidence, not network timing or performance.

Separate child controls exercise realtime and monotonic read errors, a failed
post-construction read, signed anchor range rejection and monotonic reversal.
No native failure case or malformed timestamp is used. Two private tests cover
signed floor, both elapsed/sum bounds and immutable state after rejection.
Author debug and release each pass two private plus five public/native checks;
all-target Clippy and 42 tooling controls pass. Focused instrumentation reports
38/38 module lines and 70/70 regions; this is diagnostic evidence only.
Independent review used a fresh accepted-8cc archive plus all thirteen candidate
paths and a reviewer-owned target. Its debug two private/five public checks,
seven actual SDK cases and unchanged native original pass. The reviewer checked
pinned Clock.cc/library hashes and actual canonical Gazebo linkage with no
findings. Independent release replay is not claimed; author release evidence
remains separate. Only `ClockTest.WallClock` is a reviewed whole-case candidate:
122/7/289 becomes 123/7/288 only after the full mandatory staged gate and exact
hosted CI succeed. Node statistics policy, network-clock behavior, consumer pins
and other retained cases are unchanged.
