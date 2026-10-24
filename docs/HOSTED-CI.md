# Hosted native reference execution

The native parity job runs on ordinary Ubuntu24.04 hosted workers. It builds
immutable upstream main snapshots from reference-lock.json: gz-cmake, gz-utils,
gz-math (required by canonical msgs), gz-tools (required to instantiate CLI
reference cases), gz-msgs and gz-transport. No simulator, rendering, GUI or private
runner is needed. Apt supplies native compiler/protobuf/ZeroMQ/SQLite/Eigen and
logging dependencies. Their actual package/compiler/library versions and hashes
are archived in environment.json; this is not yet a release container digest.

`python3 scripts/bootstrap-reference.py --jobs 2` creates an isolated installation
under artifacts/reference. The source checkouts are exact Git SHAs and must remain
clean. No existing system Gazebo library is accepted as a substitute. Local
bootstrap validation can copy immutable Git objects with --local-sources while
still building into fresh owned source/install directories. Every locked commit
has also been verified publicly fetchable for hosted execution.

The verifier prepends the selected installation to runtime library search paths
and checks the real peer's ldd output: every Gazebo library must resolve inside
that prefix. Library hashes and linkage output are archived. This prevents an
old local/ROS library from accidentally satisfying fresh-reference tests.

The complete retained catalog stays418 cases with immutable ID digest and227
source/fixture hashes. ZeroMQ-only reference builds instantiate393 cases; the25
Zenoh-conditioned cases remain in the catalog, pending, and are required when
that reference feature is enabled. Existing full-feature local references still
reconcile all418. Missing applicable cases, omitted optional catalog rows,
changed fixture hashes, unavailable enumeration targets, missing mapped Rust
execution and ignored mapped tests all fail. No upstream scope is retired.

Local isolated build and native gate pass with30 Rust tests,987/1006 production
lines and1486/1545 production regions, no exclusions and fatal LLVM warnings.
This demonstrates the bootstrap locally. Hosted success must be established by
the linked GitHub run after publication; a local pass is not that evidence.

The workflow archives coverage JSON/LCOV/raw profiles, exact source/toolchain
hashes, C++ linkage, reference environment metadata and native build logs even
when a step fails. Branch protection must require native-parity-and-coverage.
The public tracker separates reviewed slices from packet completion and links
actual CI revisions.
