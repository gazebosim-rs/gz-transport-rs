# Verification contract

Run `scripts/bootstrap.sh` to install Rust 1.94.1, matching LLVM tools and the
checked-in Git pre-commit hook. The equivalent runner interface is:

```sh
python3 scripts/verify.py verify --profile zeromq-linux
python3 scripts/verify.py verify --staged
```

`GZ_REFERENCE_SOURCE` must point to the pinned built C++ checkout;
`GZ_REFERENCE_PREFIX` to its installed prefix. Local defaults match the inspected
reference `/home/nkoenig/work/gazebo/rust/gz-transport` and `/home/nkoenig/local`.
Local networking/multicast must be permitted. Missing C++ dependencies, native
socket failures, absent LLVM tools, or unavailable reference runners are failures.

The runner checks format, Clippy with warnings denied, quality-tool regressions,
builds a genuine C++ peer linked to libgz-transport, executes the original
TopicUtilsTest.testFullyQualifiedName and NodeTest.ServiceWithoutOutputCallAsync assertions, and runs Rust unit/integration
cases with LLVM instrumentation of owned workspace packages only. LLVM warnings
are fatal; unmodified dependencies do not enter the production denominator. Each of five handwritten production files must
appear in the report with a nonempty denominator. Integer comparisons enforce
strictly >95% lines and regions for the sole crate and repository independently;
no production exclusions exist. Changed executable lines also require >95%.
Generated messages and unmodified ZeroMQ are dependencies, not added to this
repository's denominator. Reports, source hashes, runner details and test output
are written under artifacts/. No cached coverage evidence is accepted.

The staged hook freezes the Git index as an immutable tree, exports it into a
fresh temporary directory, verifies every exported blob against its Git object
hash, and computes changed lines from that same tree. Export attributes cannot
omit source or substitute transformed content. It
runs the same runner there and rejects concurrent index or HEAD changes before
copying evidence into artifacts/staged. Unstaged source cannot validate a
different staged source. CI invokes the same runner on a standard Ubuntu24.04 hosted worker after
building the exact native source chain; see [hosted CI](HOSTED-CI.md). No custom
runner is required. Making its native check required in branch protection remains
release administration work. A queued/missing/failed runner is not a passing result.

# Preserved assertions and inventory

`docs/upstream-tests.json` records 418 actually expanded gtest cases from 58 built
test binaries, no unavailable enumeration targets, and SHA256s for 227
source/fixture files; all 418 cases have exact parsed source symbols. `scripts/inventory.py` reproduces enumeration; it does not
execute or mark the suite passing. Ruby/CLI/Python/example workflow expansion is
still pending. No cases have been retired.

`runtime::tests::upstream_fully_qualified_name_cross_product` ports the full
8×5×10 input/expectation table from upstream
src/TopicUtils_TEST.cc:TopicUtilsTest.testFullyQualifiedName, including overlength
names, invalid partitions/namespaces/topics and absolute-topic override. The
mandatory original executable is its differential reference. Additional invalid
name cases preserve upstream byte-length and ASCII-space behavior.

The mixed-process supplement tests canonical StringMsg values, topic/type names,
C++↔Rust publication and services, publisher restart, delayed responder startup,
partition isolation, timeout, false service results, local fast paths and
concurrent shutdown. Private unit tests cover malformed delimiters/protobuf,
wrong version, overlength frames, queue/registration bounds, malformed multipart
messages, native configuration failures, discovery withdrawal/BYE and reply
routes. These supplement upstream assertions; they do not claim full equivalence
of every legacy integration test. Assertion equivalence review is pending.
