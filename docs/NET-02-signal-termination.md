<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Application-owned signal termination

Pinned `src/Node_TEST.cc` 2149–2224 provides the complete
`createInfinitePublisher` helper and `NodeTest.SigIntTermination` /
`NodeTest.SigTermTermination`. Despite the helper's older comment, the actual
tests install their own signal handler and inspect their own termination flag;
they never call a Transport `Interrupted()` API.

The Rust ports preserve both signals, Int32 data 5, `/foo`, successful
advertisement and every successful publication, the 500 ms delay before raising
the signal, each 10 ms loop sleep, the thread join and the final count below 200.
The flag is reset at the beginning and end of the publisher thread, matching
the helper.
No readiness wait or replacement iteration cap is added. Fresh explicit ports
and partition replace native process-global discovery defaults; held reusable
UDP reservations keep setup isolated without changing the asserted sequence.

Each port runs in its own test child. The application registers an atomic flag
using maintained `signal-hook`; Transport installs no handler and its public
API/runtime remain unchanged. The native mutex handler is translated to that
safe async-signal flag, preserving the observable stop request without copying
mutex operations into a signal handler. Ordinary Rust joining propagates
publisher-thread assertion failures. A six-second parent watchdog kills and
reaps a stuck child as a test failure; native `join` itself is untimed. The
watchdog does not substitute for the original less-than-200 predicate.

Only the selected child registers an action. It unregisters it after joining;
signal-hook does not restore the previous/default handler on unregistration,
so process isolation, rather than handler restoration, protects other tests.
This does not define an implicit Node shutdown policy, interrupt pending user
callbacks, or claim general signal-delivery scheduling or performance parity.

## Maintained dependency boundary

`signal-hook` 0.4.5 is pinned as a Unix **dev dependency**, with default features
disabled. Its `flag::register`, `low_level::raise` and `unregister` APIs supply
the signal action; no iterator/channel worker is enabled or started. Its
published license is MIT OR Apache-2.0 and minimum Rust version is 1.66. The
normal registry package retains its upstream notices; no source is vendored or
relicensed. The lock adds only signal-hook 0.4.5 and signal-hook-registry 1.4.8;
their libc/errno dependencies already exist at unchanged versions. Registry
1.4.8 also retains its published MIT OR Apache-2.0 license. Production
dependencies, code, unsafe prohibition and coverage ownership remain unchanged.

The audited package metadata and implementation are the published registry
source. Primary API documentation:
[flag registration](https://docs.rs/signal-hook/0.4.5/signal_hook/flag/fn.register.html),
[safe raise](https://docs.rs/signal-hook/0.4.5/signal_hook/low_level/fn.raise.html),
and [unregistration and its restoration limit](https://docs.rs/signal-hook/0.4.5/signal_hook/low_level/fn.unregister.html).

## Qualification

Author debug and release replay passed both complete Rust ports, with workspace
Clippy, 42 tools and both unchanged native originals. The helper entry point is a
third test function that returns without side effects outside its explicitly
marked child. Independent review by `/root/integration_reviewer` used a fresh
source export and owned target: debug 3 and release 3, both unchanged native
originals and 42 tools passed, with no findings. All processes exited.

Exactly these two original rows are reviewed candidates, 119 faithful, 7 partial
and 292 pending, conditional on the ordinary full staged gate and exact hosted
success. All older tests and native filters remain; the gate adds both original
filters. No production API, general signal scheduling or Node shutdown claim is
made.
