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


The malformed-discovery fixture reserves an ephemeral IPv4 UDP port with
`SO_REUSEADDR` until its tested worker has bound, then drops the reservation.
A second worker deliberately binds the usual shared discovery port afterward.
The original single `[255]` send, one-second deadline, 2ms poll interval and
exact `malformed_frames == 1` assertion remain unchanged; only the destination
port follows the isolated worker. This prevents another parallel test worker
from receiving the fixture's unicast datagram. No production protocol or runtime
code changes.

The pin review retained the earlier default-parallel timeout and controlled
old/new pin observations of target0/competing1. The fixture repair's private
negative control deliberately shares its reserved port with the later worker,
reproduces target0/competing1, and fails the unchanged deadline predicate.
Restoring the identical isolated source passes. Evidence and source hashes live
in `artifacts/udp-fixture`; default-parallel and strict serial full runs,
independent review, exact staged and hosted qualification are recorded separately.

Independent [fixture review](reviews/discovery-fixture-isolation.json) approved
the exact frozen inputs, bind/drop ownership and unchanged assertion sequence.
A raw reviewer export, separate fresh target and native peer passed all 135
checks in both default-parallel and required serial modes. An independent
negative-source control failed the original deadline with target0/competing1;
the hash-bound positive source remained unchanged. The author strict gate also
passed 135 Rust and 22 tooling checks, with unchanged 1641/1645 production lines
and 2460/2509 regions; no executable production lines changed. Final exact
staged and hosted qualification remain required.


The three selected same-Node raw metadata assertion ports additionally require
the unchanged native RawPubSubSameThreadMessageInfo,
RawPubRawSubSameThreadMessageInfo and PubRawSubSameThreadMessageInfo originals in
every gate. Exact versus wildcard receive filters and raw versus typed
publication remain distinct; see the [raw assertion contract](NET-01-raw-message-info.md).

The reviewed topic/namespace validation assertion ports additionally require
the unchanged native TopicUtilsTest.testTopics and testNamespaces originals in
every gate. Their exact 21 and 18 predicates are retained in Rust; see the
[validation assertion contract](NET-01-name-validation.md) and the
[review receipt](reviews/name-validation.json). Both are faithful ports.

The reviewed partition validation port additionally requires the unchanged
native TopicUtilsTest.testPartitions original in every gate. Its exact 18
predicates run through `Config::set_partition`; see the
[partition assertion contract](NET-01-partition-validation.md) and
[review receipt](reviews/partition-validation.json). It is a faithful port.

The reviewed fully qualified topic decomposition port additionally requires
the unchanged native TopicUtilsTest.decomposeFullyQualifiedTopic original in
every gate. Its three output tuples and five negative inputs are retained in
Rust; see the [decomposition contract](NET-01-decomposition.md) and
[review receipt](reviews/decomposition.json). It is a faithful port.

The reviewed partial remapped service-list assertion port additionally requires the
unchanged native NodeTest.ServiceListRemap original in every gate. The exact
one-entry `/bar` result is retained through local service inspection; see the
[service-list contract](NET-02-service-list-remap.md) and
[review receipt](reviews/service-list-remap.json). Global discovery and timing
remain unqualified.

The reviewed partial remapped topic-list assertion port additionally requires the
unchanged native NodeTest.TopicListRemap original in every gate. The exact
one-entry `/bar` result is retained through local publisher inspection; see the
[topic-list contract](NET-02-topic-list-remap.md) and
[review receipt](reviews/topic-list-remap.json). Global discovery remains
unqualified.

The bounded known-list APIs additionally require the unchanged native
NodeTest.TopicList and NodeTest.ServiceList originals as contextual controls.
Rust native-peer tests exercise local and discovered remote publisher/service
records, remapping, partition isolation and eventual disappearance; see
[known-list scope](NET-02-known-lists.md) and independent
[review receipt](reviews/known-lists.json). Full original list equivalence is
pending.

The subscriber-only known-topic snapshot additionally requires the unchanged
native NodeTest.TopicListSubscriberOnly original. Rust retains its local size,
indexed `/foo`, and post-unsubscribe empty assertions; a real C++ peer exercises
eventual remote subscriber discovery and disappearance. See
[subscriber-list scope](NET-02-subscriber-lists.md) and
[independent receipt](reviews/subscriber-lists.json). Full original TopicList
equivalence remains pending.

The bounded topic information descriptor snapshot requires unchanged native
NodeTest.TopicInfoSubscriberOnly in every gate. Its Rust assertion port checks
query success, zero publishers, one subscriber and exact `gz.msgs.Int32` type.
Additional controls cover original-name remap lookup, duplicate subscriber
endpoint identity, fresh remote C++ subscriber and publisher descriptors,
and subscriber withdrawal. See [topic information scope](NET-02-topic-info.md).
The [independent receipt](reviews/topic-info.json) approved this partial
mapping. Native process-global endpoint ownership and full TopicInfo metadata
remain open.

The retained two-process `twoProcPubSub.TopicInfo` original additionally runs
in the pinned native gate. A real C++ Vector3d advertiser and Rust assertion
port preserve its discovery wait, invalid `@`, absent `/bogus`, and exact one
`gz.msgs.Vector3d` publisher observations. See the
[two-process topic information scope](NET-02-two-process-topic-info.md).
The [independent receipt](reviews/two-process-topic-info.json) approved the
bounded partial mapping. The C++ output-vector state after a failed query
remains unqualified.

The retained `twoProcPubSub.AllTopicInfo` original additionally checks two
discovered topic entries: `/foo` has one publisher and one subscriber, while
`/subscriber_only` has no publisher and one subscriber. A Rust assertion port
uses two real C++ Vector3d peers, a local Int32 subscription, and the existing
`known_topics()` then `known_topic_info()` sequence. See the
[two-process all-topic scope](NET-02-two-process-all-topic-info.md). Native
endpoint metadata, complete vector contents and process-global sharing remain
unqualified. The [independent receipt](reviews/two-process-all-topic-info.json)
approved this partial mapping.

The pinned `twoProcPubSub.TopicList` original checks remote `/foo` discovery,
both list contents and a second list call under 2 ms. Its exact assertion port
uses a real C++ peer with the original `/foo` Vector3d and `/large_msg` Bytes
advertisements. The command-woken worker keeps its 2 ms idle socket poll bound
while answering queued list commands promptly. See the
[two-process topic list contract](NET-02-two-process-topic-list.md).
The [independent receipt](reviews/two-process-topic-list.json) approves the
original assertion sequence and bounded worker wake semantics; the final hook and
hosted run remain pending.

The pinned one-Node `NodeTest.ServiceList` original retains both list-size
checks, the intervening clear and the second-call bound below two milliseconds.
The Rust port uses the existing discovered-service worker snapshot and keeps
the handler live through both queries; see the
[service-list assertion scope](NET-02-service-list-latency.md). Its independent
[review](reviews/service-list-latency.json) approved the one-Node mapping;
the ledger remains partial and the final hook and hosted check remain pending.
