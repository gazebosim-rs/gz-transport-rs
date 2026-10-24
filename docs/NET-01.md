# NET-01 first acceptance slice

Task: minimal interoperable discovery/pub-sub/services and bounded IO ownership.
Upstream: 41913b9dfc3dbce33ac5464ddcb427cc82450f26.
Messages producer: immutable 94e6c618dfbd25b1b43bb32aa28db3f8d9a9a3af.

Implemented: canonical discovery version10 and framing; multicast discovery,
NEW_CONNECTION registration, advertisements/withdrawal, heartbeat/expiry;
qualified partition/namespace/topic rules; raw typed publications and exact
post-filtering; correlated ROUTER services in both directions and local services;
request deadlines/cancellation; explicit worker shutdown; bounded registration,
command/application/native queues and peer/reply state. Reply routes use mandatory
routing and bounded retries instead of dropping during ZMTP establishment.

NET-02 advancement: local operational counters for application queue drops,
malformed frames, native send failures, reply deadline expiration. This does not
implement the version110 topic-statistics wire format or imply log/auth/CLI parity.

Checks (2026-10-24, Linux x86_64):

- `python3 scripts/verify.py verify --profile zeromq-linux`: passed.
- `python3 scripts/verify.py verify --staged`: passed on the exact staged snapshot,
  with the same coverage counts.
- Rustfmt and Clippy all targets with warnings denied: passed.
- 20 Rust tests: passed, including real C++ pub/sub/service peers in both directions,
  restart/delayed startup, cancellation, overload and late-binding reply receiver.
- Original C++ TopicUtilsTest.testFullyQualifiedName: passed, with full400-case
  Rust cross-product port preserving inputs/expectations and length boundaries.
- Handwritten production coverage:886/905 lines (97.90%),1316/1374 regions (95.78%).
  Changed executable production lines:879/879. No production exclusions.
- Five quality-tool tests: strict integer95 boundary, missing/empty instrumentation,
  inventory changes, changed-line failure, stale evidence/profile, exact staged
  snapshot despite unstaged differences: passed.

Machine-readable coverage JSON, LCOV, profiler data, source/dependency/toolchain
hashes, runner identity and full test output are under artifacts/. The native
reference dependency and library versions are in reference.json. Strict staged
verification is required by the installed Git hook; CI uses the identical runner.

Review: parent implementation reviewer identified missing routing retries and
unobservable drops; both were corrected with executable regressions. Full source,
assertion equivalence, coverage exclusion and release-infrastructure review is
still required. Runtime baseline inventory is418 cases; only one original full
case has a verified faithful port. No remaining upstream tests have been retired.

Limits and next bounded slices: maintained message runtime migration; remaining
per-case contracts/baseline runs/non-gtest expansion; relay/host/process scope;
environment/interface policy; statistics110; throttle/auth; logging and complete
CLI; same-fidelity latency/throughput/allocation measurements. CI's native runner
must be provisioned and its status made required before claiming a release gate.
