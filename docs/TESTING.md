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

Before compiling, the runner uses Cargo metadata to verify the current one-library,
no-feature, Linux x86_64 layout. Implicit build scripts, bins/examples/benches,
additional local packages/features and owned Rust outside src/tests require a
reviewed coverage-profile extension. A freshly compiled normal `cargo rustc --lib`
dependency inventory must match every declared production file before tests run; production
imports from tests or other data paths therefore cannot borrow the test-code
exemption. Only this package's dev outputs are cleaned for the audit, so restored
source timestamps cannot reuse a stale proof; dependencies stay cached. Existing
cfg(test) support remains separate. This profile accepts
plain compiler input paths and fails closed on ambiguous Make escaping. A hashed
compiler-input proof is retained without raw environment dependency values. The
LLVM report also rejects unexpected owned inputs regardless of extension, and
duplicate production records cannot mask coverage.

The runner checks format, Clippy with warnings denied, quality-tool regressions,
builds a genuine C++ peer linked to libgz-transport, executes the original
TopicUtilsTest.testFullyQualifiedName, NodeTest.ServiceWithoutOutputCallAsync and
NodeOptionsTest.accessors assertions, and runs Rust unit/integration
cases with LLVM instrumentation of owned workspace packages only. LLVM warnings
are fatal; unmodified dependencies do not enter the production denominator. Each of five handwritten production files must
appear in the report with a nonempty denominator. Integer comparisons enforce
strictly >95% lines and regions for the sole crate and repository independently;
no production exclusions exist. Changed executable lines also require >95%.
Generated messages, unmodified ZeroMQ and nix system bindings are dependencies, not added to this
repository's denominator. Reports, source hashes, runner details and test output
are written under artifacts/. No cached coverage evidence is accepted.

The staged hook freezes the Git index as an immutable tree, exports it into a
fresh temporary directory, verifies every exported blob against its Git object
hash, and computes changed lines from that same tree. Export attributes cannot
omit source or substitute transformed content. It
runs the same runner there and rejects concurrent index or HEAD changes before
copying evidence into artifacts/staged. Unstaged source cannot validate a
different staged source. Parent snapshot/export and final HEAD/index guards retain
the caller's actual Git context, including an explicit custom index. Only the
exported verifier/build/test child drops the official repository-local variables
reported by `git rev-parse --local-env-vars`. This prevents reference commands and
temporary Git fixtures from inheriting an absolute worktree Git directory or
index. A real temporary worktree commit exercises bare-fixture initialization,
independent reference lookup, an explicit selected index and untouched working
edits, while checking caller HEAD/tree/index/config preservation. CI invokes the same runner on a standard Ubuntu24.04 hosted worker after
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


The Git-context isolation increment passes the full author gate with 116 Rust
and 22 tooling tests, 1528/1550 production lines and 2323/2389 regions; no
production Rust changed. Independent /root/msgs review raw-verified the private
snapshot and passed all 22 tooling checks, then swapped only the private driver
back to the parent version and confirmed the worktree regression fails with the
wrong reference HEAD. Restored reviewed hashes pass again. The independent
[receipt](reviews/git-env-isolation.json) is copied unchanged; it does not claim
an independent full Rust/coverage rerun. The original live-hook failure evidence
and quarantined isolated branch remain preserved. Exact staged and hosted gates
must qualify the published revision.

The selected two-process ServiceInfo assertion ports require the exact ordinary,
inputless and outputless native originals in every gate. They use real C++
providers and a bounded provider-record type snapshot; the existing signature
view still deduplicates type pairs. See [service inspection assertions](NET-01-service-info-ports.md).

Explicit unsubscribe adds the unchanged native two-node original as a required
baseline control without retiring its Rust mapping. Native/Rust metadata and
actual XPUB filter-lifetime checks cover the narrower API; see the
[receiver ownership boundary](NET-01-unsubscribe.md).

Canonical typed publication requires the unchanged native NodeTest.TypeMismatch
original plus its exact Int32/Vector3d assertion port and real C++ typed-value
delivery. Descriptor/size checks and existing queue/shutdown errors are tested
without changing raw byte forwarding.


Received metadata additionally requires the unchanged native
NodeTest.PubSubSameThreadMessageInfo original. Its two single publications and
cbInfo observations have an independently approved owned-receive adaptation;
see the [review receipt](reviews/message-info.json). A separate native/Rust two-Node test asserts the known origin mismatch;
actual C++ peers cover remote origin in both directions and retirement/recreation.
Existing raw relay and receiver lifecycle assertions stay in the full gate.
