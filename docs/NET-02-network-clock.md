<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Owned selected network clock

`NetworkClock::new(&node, topic, ClockTimeBase)` attaches an owned canonical
`gz.msgs.Clock` subscription and publication to an existing Node. Simulation,
Real and System select `sim`, `real` and `system`, respectively. It creates no
global Node, separate IO worker, clock-reading loop or new thread model. Updates
use the existing demand-created shared callback dispatcher and its bounded
admission. Callers provide the ordinary Node/domain configuration.

`time_ns()` reads one coherent signed 64-bit nanosecond sample, initially zero.
`is_ready()` means that the current sample is nonzero, exactly as native
`NetworkClock::IsReady`; it is not a first-message latch or liveness test. A
selected zero clears readiness, and backward or negative values are accepted.
Separate getter calls can observe separate concurrent updates.

Only the selected optional field changes the sample. Missing fields preserve
the previous sample. Native emits a diagnostic to stderr for a missing selected
field; Rust ignores this sparse-message condition without unconditional logging
or incrementing an error counter. The native diagnostic remains captured in the
reference fixture. This is an explicit diagnostic disposition, not a default
time substitution. Signed or unnormalized `nsec` values retain native integer
addition semantics. Malformed protobuf or checked `sec * 1e9 + nsec` overflow
leaves the sample unchanged and increments the Node's existing `routing_errors`
counter. Native signed arithmetic overflow is outside the qualified value
domain; it is never invoked as a reference oracle.

## Publication and native seconds width

`set_time_ns()` constructs only the selected field and publishes it through the
existing canonical Publisher. It does not write the stored sample directly;
actual receipt updates the sample asynchronously. Success is bounded
publication admission, not receipt acknowledgement, scheduling or remote
convergence. Payload-size, capacity and runtime errors propagate. A retired
clock subscription returns `Shutdown`; an already admitted publication or
selected callback can still finish when retirement races the call.
Native process-local callbacks can complete inline; this API retains the
accepted asynchronous shared dispatcher. The selected originals assert waited
values, not a synchronous setter return-time boundary.

Pinned `Clock.cc::NetworkClock::Implementation::SetTime` divides nanoseconds
toward zero, narrows whole seconds into a C++ `int`, and retains the signed
nanosecond remainder. The qualified Linux SDK uses 32-bit `int`. Rust preserves
that measured narrowing, rather than widening the wire seconds silently. For
example, setting 2147483648000000000 ns publishes seconds -2147483648 and is
observed as -2147483648000000000 ns. Setting `i64::MAX` is observed as
633437444854775807 ns. This historical setter width is distinct from receiving
arbitrary defined-range `int64` seconds in a Clock message.

The canonical codec preserves selected-zero field presence, absent header and
other time fields, negative integer encodings and exact payload bytes. No
floating conversion, wall-clock read, interpolation or clock arbitration is
performed. No additional dependency or unsafe code is introduced.

## Ownership and admission

The caller owns the Node. The clock owns only its callback and publication
leases, following the existing domain, topic remapping, one-publisher-per-topic
and configured capacity rules. Construction subscribes before advertising,
matching native order. If publication admission fails, the partial callback
lease is canceled before the error returns. Capture/permit cleanup follows the
same off-dispatcher wait or same-dispatcher deferral described below; a reentrant
caller cannot release real-exit budgets early. Two ordinary registration slots
are required, plus existing callback/thread permits.

Drop first retires the clock callback. Outside its shared dispatcher it waits
for capture exit before withdrawing publication. From that same dispatcher,
cleanup is deferred and publication can withdraw before the retired capture
exits. It preserves other callbacks and releases permits through existing
real-exit rules. A user callback blocking the dispatcher can delay off-dispatcher
cleanup; user code is not preempted. The clock callback captures only
the shared atomic sample and time-base enum, with no Node/clock ownership cycle.
Explicit Node unsubscribe, cancellation or owner destruction retires updates;
getters retain the last owned value, which can remain nonzero. The setter
rejects a quiescent retired clock even if its Publisher would otherwise outlive
the Node. This explicit caller-owned setup replaces native Clock's private Node.

## Retained cases and current qualification

Only the three complete parameterized `Clock_TEST.cc` Functionality cases are
selected. Each retains initial not-ready, empty Clock then 100 ms and still
not-ready, all three distinct time fields from native `MakeClockMessage`, the
selected 54321 s + 12345 ns value after 100 ms, and SetTime to
54321 s + 24690 ns followed by the final 100 ms exact-value check. A separate
explicit Node supplies the resource owned privately by native Clock; both Rust
Nodes use the same isolated configured domain. No readiness poll replaces any
original sleep or predicate.

Author debug and release each passed two pure controls, seven public controls
and two rebuilt-native/wire controls. All three unchanged native originals also
passed. The freshly compiled SDK helper replays 63 complete value/wire states, including signed, zero, missing-field,
unnormalized and int32/i64 setter edges. Bidirectional C++/Rust tests match actual
remote bytes and selected values across 13 setter inputs for all three bases.
Rust-to-native requires a fresh native observer count, not just a stale matching
value. Its supplemental data-plane warmup is separate from the originals.

Lifecycle controls cover exact-capacity construction rollback, payload
rejection without mutation, Node-wide unsubscribe, cancellation/owner retirement
and blocked shared-dispatch cleanup while preserving other callbacks. Workspace
Clippy and formatting pass. Focused instrumentation of the eleven new controls
reports 63/63 executable module lines and 86/86 regions. This is a diagnostic
module result, not aggregate or staged acceptance.

Independent release replay exposed a test setup assumption: four registration
commands could fill a capacity-four worker queue before the first publication.
The shared-teardown fixture now reserves five command slots for that bounded
setup/publication batch, without retrying publication or changing any lifecycle
predicate or deadline. Its separate capacity-one rollback assertions remain.
The earlier release failure remains review evidence. Corrected author public
controls passed in debug and release, with all predicates unchanged. Independent
review by `/root/integration_reviewer` used a fresh source export, owned target
and freshly compiled SDK helper. Corrected debug and release each passed two
pure, seven public and two native/wire controls; the three unchanged native
originals, all-target Clippy and 42 tools also passed. The helper matched the
fixture's captured binary hash. All peers exited, with no remaining findings.

The three selected Functionality rows were accepted at `8cc6295` after the full
staged and exact hosted checks. The separate owned
[WallClock](NET-02-wall-clock.md) was accepted at `447ad3b`; current accepted
totals are 123 faithful, 7 partial and 288 pending. The invalid-input candidate
below is separate from those accepted value contracts. Node statistics
enable/disable and Metric publication remain separate retained work.

## Invalid clock inputs: deliberate API translation

Pinned `ClockTest.BadNetworkClock` remains unchanged in the mandatory native
Clock filter. It constructs `//bad-clock` using the default SIM base and checks
not-ready. It then casts -1 to the fixed-width native TimeBase and creates a
`/clock` object. Three readiness observations cover initial state, an empty
Clock from a second Node followed by 100 ms, and `SetTime(10 seconds)` followed
by another 100 ms. All four original not-ready predicates and both original
waits stay in the native reference; none is silently removed or replaced.

`Clock.cc` initializes the native sample to zero. Invalid-topic construction
fails both registrations but still returns an allocated clock object. An
invalid-base message callback only logs and leaves the sample unchanged; its
setter logs and returns **before** publication. The original does not assert
stderr or packet counts. These source facts explain the inert-object behavior,
without attributing supplemental predicates to the original.

Rust deliberately exposes earlier rejection instead of that invalid object:

- `NetworkClock::new(&node, "//bad-clock", ClockTimeBase::Simulation)` returns
  `InvalidName`. Explicit Simulation translates the native default; no default
  API or normalization is added. Existing topic validation occurs before shared
  dispatcher and registration admission.
- Passing the exact -1 value to the actual public constructor fails compilation
  with E0308. The `ClockTimeBase` argument cannot represent it in safe Rust.
  There is consequently no invalid Rust clock, readiness method or setter with
  which to reproduce the three invalid-base observations and their waits.

This is an owner-approved new-API translation of invalid-input intent, **not
literal runtime-predicate parity**. A compile-fail Rustdoc checks the real
constructor boundary. The separate public bad-topic control also verifies that
a later valid clock can consume the full two-slot budget and retire cleanly;
its initial zero/not-ready check belongs to that valid supplemental clock.
It does not fabricate a placeholder that always reports false. No numeric enum
conversion, invalid variant, production/default change or new dependency is
introduced. Existing valid clock and rollback tests remain untouched.

Author debug/release each pass the public negative-admission control and the
constructor compile-fail Rustdoc. Separate metadata-only compiler checks first
accept the valid Simulation argument and then reject -1 with E0308, so a missing
dependency cannot masquerade as the intended failure. Neither compiler program
executes. The unchanged native original passes 1/1 with all original predicates
and waits. All-target Clippy and 42 tools pass.

Independent review used a fresh accepted 447ad3b archive plus all five final paths
and an owned target. Public 1 and Rustdoc 1 pass in both debug/release; the actual
same-library compiler controls accept Simulation and reject -1 with sole E0308.
The unchanged native original 1, 42 tools and workspace Clippy pass. The reviewer
checked the complete source/header/default/invalid-setter behavior and this
explicit API disposition, with no findings. An initial pure Cargo command from
the wrong working directory was setup-only and excluded; qualifying commands
used the fresh source. All final source paths match the reviewed copy.

Only BadNetworkClock is a reviewed conditional case mapping: accepted totals
123/7/288 become 124/7/287 only after the unchanged full normal staged gate and
exact hosted success. The native four predicates and both waits remain in the
reference gate; its Rust mapping does not claim literal runtime-predicate
parity. Node statistics lifecycle policy and runtime Metric publication remain
held outside this slice.
