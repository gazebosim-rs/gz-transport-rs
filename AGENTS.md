# Implementation guidance

Read README.md and the linked canonical plan, this repository's contract, task
packets, and test policy. Confirmed owner decisions override proposed defaults.
This is a planning shell; production quality gates must be implemented before
accepting the first production feature.

Preserve the Gazebo wire protocol and real C++ peer interoperability. Do not replace it with a different transport protocol. Use bounded cancellable APIs with explicit runtime/thread ownership. Depend on gz-msgs-rs as needed, never on Bevy or the simulator.

- Pin the upstream main SHA, toolchain, dependencies, and fixtures. Reconcile the
  central source inventory with actual expanded runtime tests for this repository.
- Preserve behavior, formats, protocols, and all retained upstream test semantics.
  Faithful Rust ports are allowed. Keep assertions, edge cases, tolerances, and
  fixture hashes; verify against C++ reference runs and record the mapping.
- Reuse maintained Rust libraries before writing general-purpose algorithms.
  Record exact missing semantics before adding custom code.
- Require strictly greater than 95% line and region coverage for each production
  crate and the repository. Generated code exclusions must be explicit and reviewed.
  Missing tests, unavailable runners, and empty denominators are not passes.
- Install pre-commit checks and required CI gates using the canonical policy.
  Never bypass hooks, weaken tests, or remove hard code from coverage to pass.
- Keep unsafe code narrow and audited. Document ownership, threading, and error
  behavior. Do not let panics or native exceptions cross FFI boundaries.
- Write concise idiomatic code and human comments. Explain invariants and reasons,
  not obvious statements. Avoid repetitive wrappers and speculative abstractions.
- Measure performance against the pinned C++ behavior at equal fidelity. Report
  evidence and limitations; a Rust rewrite is not inherently a speedup.
- Work in bounded reviewable tasks. Commit only owned changes. A different reviewer
  checks semantics, assertions, exclusions, and performance evidence.
- Record exact check commands/results and unrun work. Keep this repo's test ledger
  and integration contract current; do not silently break downstream consumers.

Do not apply the monorepo's internal crate layout here. Do not edit the active Fuel
rewrite or another repository without a coordinated assignment.
