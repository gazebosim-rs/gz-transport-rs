# Implementation guidance

Read README.md and the linked canonical plan, this repository's contract, task
packets, and test policy. Confirmed owner decisions override proposed defaults.
The bounded ZeroMQ runtime is implemented; retained NET-01/NET-02 scope remains
open. Run the strict staged production gates for every accepted slice.

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

## Public progress reporting

Keep this repository's report current on the
[rewrite progress dashboard](https://gazebosim-rs.github.io/rewrite-progress/).
Follow the shared [status contract](https://github.com/gazebosim-rs/rewrite-progress/blob/main/status-contract.md).
Prepare `artifacts/progress-status.json` (ignored), refresh its UTC timestamp,
and publish with the sibling tracker checkout's `scripts/update-status.py`:

```sh
python3 ../rewrite-progress/scripts/update-status.py gz-transport-rs artifacts/progress-status.json --dry-run
python3 ../rewrite-progress/scripts/update-status.py gz-transport-rs artifacts/progress-status.json
```

Update on accepted slices, task/review changes, new test evidence and real
blockers. Only whole packets meeting all acceptance criteria may be complete;
reviewed slices are separate. Report exact line/region counts and revision-linked
CI state. A local pass or queued native job is not hosted CI success. Publish no
credentials, private paths or raw environment variables. Each owner updates only
`data/gz-transport-rs.json` on the tracker `status` branch through that script.

## Licensing

Owned code is Apache-2.0 and uses `Copyright (C) 2026 Open Source Robotics
Foundation` plus `SPDX-License-Identifier: Apache-2.0` in new Rust, Python,
C++ and shell source (after shebangs). Preserve original copyright and license notices for
imported schemas, fixtures, faithful test ports and vendored dependencies; do not
relabel third-party code as Apache-2.0. Keep Cargo metadata and the root license
consistent with this policy.
