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
see the [review receipt](reviews/message-info.json). The separate native/Rust two-Node
test originally exposed an origin mismatch; the bounded
[direct-delivery successor](NET-02-process-local-direct.md) requires true from both;
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

The remapped service-list assertion ports require the unchanged native
NodeTest.ServiceListRemap original in every gate. The discovered query retains
its exact one-entry, index-zero `/bar` result; the older local-inspection test
remains unchanged. A separate supplement covers withdrawal and recreation; see
the [service-list contract](NET-02-discovered-service-remap.md). The selected
one-Node original is a faithful port; global convergence and general latency
remain unqualified.

The remapped topic-list ports require the unchanged native NodeTest.TopicListRemap
original in every gate. The discovered query retains its exact one-entry,
index-zero `/bar` result, while the older local-inspection test remains unchanged.
A separate supplement preserves subscriber-only visibility after publisher drop;
see the [topic-list contract](NET-02-topic-list-remap.md). The selected one-Node
original is a faithful port; remote convergence and general latency remain open.

The bounded known-list APIs additionally require the unchanged native
NodeTest.TopicList and NodeTest.ServiceList originals as contextual controls.
Rust native-peer tests exercise local and discovered remote publisher/service
records, remapping, partition isolation and eventual disappearance; see
[known-list scope](NET-02-known-lists.md) and independent
[review receipt](reviews/known-lists.json). Full original list equivalence is
pending.

The subscriber-only known-topic snapshot requires the unchanged native
NodeTest.TopicListSubscriberOnly original. Its faithful Int32 port retains
subscription success, size one, indexed `/foo`, unsubscribe success, explicit
clear and the empty second list. The older StringMsg variant remains unchanged;
real C++ peer tests cover remote interest separately. See the
[subscriber-list contract](NET-02-subscriber-lists.md). Broader registry,
initialization, convergence and latency behavior remain open.

The subscriber-only topic information ports require unchanged native
NodeTest.TopicInfoSubscriberOnly in every gate. The faithful ready-query port
retains Int32 subscription success, info-query success, zero publishers, one
subscriber and exact `gz.msgs.Int32` type. The older immediate snapshot test
and existing remap, endpoint identity, remote descriptor and withdrawal controls
remain unchanged. See [topic information scope](NET-02-topic-info.md). Full
MessagePublisher metadata, process-global ownership and general latency remain
open.

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

Three pinned `twoProcSrvCall*.ServiceList` originals each wait for a separate
Int32/Empty service, require two single-entry `/foo` lists separated by a
clear, and bound the second call below two milliseconds. Their exact C++
originals now run in the mandatory gate. The proposed Rust assertion ports
use the real C++ service peer and the existing discovery and list APIs; see
the [two-process service-list scope](NET-02-two-process-service-lists.md).
The independent [review](reviews/two-process-service-lists.json) approved
the three bounded mappings; the ledger remains partial. Final staged
verification, normal hook and hosted CI remain pending.

The pinned one-process, two-Node `NodeTest.TopicList` original requires both
advertisements in the first list and bounds only the second list call below
two integer-truncated milliseconds. A private Rust baseline missed the other
Node's topic on the first call. The process-local publication index and
assertion port retain both size checks, clear and timing bound; see the
[two-Node topic-list scope](NET-02-two-node-topic-list.md). The independent
[review](reviews/two-node-topic-list.json) approved the scoped assertion
mapping after private native/Rust and old-runtime negative replays; the
ledger remains partial. The exact staged gate passed 163 Rust tests,
22 tooling checks and strict 1875/1901 line, 2838/2930 region coverage.
Final staged verification, normal hook and hosted CI remain pending.

## Four Node topic information original

The faithful four-node `NodeTest.AllTopicInfo` mapping retains the old snapshot variant
and adds a startup-ready composition using actual enumerated topics. Every
per-topic query must succeed, and the three original names and endpoint counts
remain unchanged. Native initialization precedes enumeration, followed by
separate helper snapshots; it has no atomic whole-vector guarantee. See the
[four-node contract](NET-02-four-node-topic-info.md). Full endpoint metadata,
process-global initialization, dynamic-snapshot behavior and ownership remain
separate. Existing Publisher-after-Node-drop tests and explicit shutdown rules
are unchanged.

## Failed staged test output

When the staged child fails, `scripts/verify.py` retains its complete
`test-build.jsonl`, if one was created, under
`artifacts/staged-failures/<tree>/failure-*/`. The adjacent `failure.json`
records the staged tree, child exit, output hash and caller tree. Failures
before test execution have a null output hash. Artifact retention is best
effort: an I/O error is diagnosed, and the original child failure remains the
reported exit. `staged_tree` uses a private index copy so its Git write-tree
operation does not modify either the selected alternate index or normal index.

The exact two-file tooling change passed 24 tooling tests, including a forced
child exit with complete retained output and an injected artifact I/O failure.
The strict staged gate passed with 2085/2124 production lines and 3142/3271
regions; no production executable lines or coverage thresholds changed. An
independent [review](reviews/staged-failure-evidence.json) reproduced the prior
status-masking defect and approved the corrected behavior. The normal hook and
hosted qualification remain pending at this review point.

## Nonblocking Publisher interest snapshot

`Publisher::connection_snapshot()` reads an atomic membership hint without a
worker round trip. The pinned native local-interest probe observed 0, 0, 1, 0
before subscription, for the wrong type, for a matching subscription, and
after unsubscribe. A real C++ subscriber peer and Rust controls cover local
wildcards, duplicate handlers, remaps, delayed worker commands, remote
registration and withdrawal, partition and type isolation, ordinary Node
destruction, explicit shutdown, and queue saturation. Removing the
subscribe-side cache refresh fails the admitted-local-interest assertion.

The corrected five-file staged gate recorded 2261/2307 production lines,
3397/3544 regions, and 105/107 changed lines. The working gate's observed
floor was 2260/2307 lines and 3396/3544 regions; both uncovered changed lines
remain counted. The independent [review](reviews/publisher-interest-snapshot.json)
approves the bounded source and assertions. This is a membership hint, not a
delivery or readiness guarantee; remote discovery remains eventual, and BYE
does not clear remembered registrations. No original test is retired by this
slice. The final protected hook and hosted qualification remain pending.

## Local delivery with a remote subscriber

The pinned `twoProcPubSub.LocalSubscriberReceivesOnceWithRemoteSubscriber`
original maps partially through
`original_local_subscriber_receives_once_with_remote_subscriber`. The Rust
fixture retains ten ordered local `Vector3d` copies and rejects extras after
the original bounded wait and quiet period while a real C++ subscriber is
present. Two topic-info descriptors are only a fixture-specific surrogate
for two distinct native subscriber process UUIDs. Remote receipt is not
asserted by either test. The exact one-file source passed the strict staged
gate with 2260/2307 lines and 3396/3544 regions, zero changed production
lines; an extra-publish mutant fails. The independent
[review](reviews/local-remote-once.json) approved this partial scope. See the
[case contract](NET-02-local-remote-once.md). Its normal protected hook and
exact hosted run 40230542175 passed on public commit 1422aff0a0a89b1616f7a936ed71606b5affc27f.

## Per-registration ignore-local delivery

The pinned `NodeTest.IgnoreLocalMessages` original maps faithfully through
`original_ignore_local_messages`: default false, set true and getter, one
`/foo` Int32 publisher and ignored subscriber, three data5 publications 100 ms
apart, and no local delivery. The pinned
`twoProcPubSub.IgnoreLocalMessagesWithRemoteSubscriber` maps partially through
`original_ignore_local_messages_with_remote_subscriber`: a real C++ subscriber
is registered, a second Rust Node witnesses ten Vector3d deliveries with no
extra, and the publishing Node's opted-in receiver sees none. Three endpoint
descriptors are a fixture-specific surrogate for distinct native subscriber
process UUIDs. Neither original nor port checks receipt by the C++ child, and
same-process origin sharing was unported at that review checkpoint. The later
[direct-delivery successor](NET-02-process-local-direct.md) covers same-process
message origin with explicit limits. The additive option leaves
ordinary subscriptions and discovery registration intact. The exact staged
source gate passed with 2277/2324 production lines, 3415/3563 regions and
18/18 changed executable lines. The independent
[review](reviews/ignore-local.json) reproduced both native originals and the
selected Rust cases; removing the direct-delivery filter fails the Int32
assertion. See the [case contract](NET-02-ignore-local.md). Final protected
hook and hosted qualification remain separate publication gates.

The additive `original_local_subscriber_receives_once_with_remote_subscriber_uuid`
and `original_ignore_local_messages_with_remote_subscriber_uuid` retain the
old variants above while replacing their readiness surrogate with actual
subscriber `process_uuid` values. Both preserve the native at-least-two UUID
predicate and require an identity outside all local workers, so the native
child must be represented. Local metadata is captured once per Node before
child startup, with the unchanged five-second reply bound and no added
readiness loop. The original 10 s/50 ms discovery poll, 5 s/10 ms delivery
checks, ten Vector3d values/counts, ignore-local option and 300 ms quiet period
remain. Setup order is the explicit translation described in the two case
contracts. Raw identities remain per worker; full process-global equality and
remote receipt remain outside these selected assertions. The pinned unchanged
C++ originals passed 2/0 during author characterization; all four retained/new
Rust variants and all-target Clippy passed. Independent `/root/integration_reviewer`
compared helper/body semantics, repeated the four Rust tests using fresh source,
target and C++ peer, and replayed the native pair 2/0. A separate author wrong-topic
child control fails both new readiness checks after 10 seconds, including the
two-local-worker case. Only these two rows move to faithful_port, conditional on
the normal strict staged gate and exact hosted success (97/9 becomes 99/7;
312 pending remains). No production, dependency, gate or raw identity change.

## Same-process direct message delivery

The retained `NodeTest.PubSubOneThreadTwoSubs` first publication now reaches
both Nodes after only its original publisher membership check. The scoped
[contract](NET-02-process-local-direct.md) records process-local routing,
per-registration ignore-local behavior, domain isolation, bounded wire-echo
suppression and its address/payload reuse limit. A tracked supplemental test
establishes a connected second-Node wire path and rejects a duplicate copy;
disabling only the echo-suppression callsite makes that test fail. Removing
only cross-Node direct delivery makes the original first receipt fail at the
unchanged 10-second bound. The exact staged source gate passed 185 Rust tests
with 2395/2448 production lines, 3566/3717 regions and 132/132 changed
executable lines. [Independent review](reviews/process-local-direct.json)
approved the bounded port. The original lost-output status101 from the prior
Publisher-lifetime gate remains unclassified; it is not explained by this
slice. The reviewed direct-delivery commit `2d810f8` passed its normal hook
and exact hosted run `40237034528`.

## Service responses under publication flood

The [response-lane contract](NET-02-service-response-lane.md) records a
publication-flood regression: on public `2d810f8`, the server received one
request but `Request::respond` returned `QueueFull` and the client timed out.
The reviewed source gives responses a separate bounded queue and interleaves
them with ordinary commands. The same tracked cross-Node test now observes
publication queue pressure, one admitted response, and the exact client reply.
The exact staged gate passed with 2421/2475 production lines, 3594/3746
regions and 28/29 changed executable lines.
[Independent review](reviews/service-response-lane.json)
replayed the old failure, the fixed service test, and all 45 frozen Sim unit
tests using only a private Transport dependency redirect. Root separately
replayed all 15 real C++ Sim client scenarios against the frozen source. The
response-only worker poll and downstream queue limits remain in the contract;
normal hook, public commit and hosted CI are separate acceptance steps.

## Process-local publication cost

The [publication cost contract](NET-02-publication-cost.md) records the
lease-owned, unconditional wire-echo history, ordered expiry, cached absence
generation and owned final-recipient delivery. The frozen three-file source
passed its exact staged gate with 2486/2543 production lines, 3690/3846
regions and 104/108 changed executable lines. Independent private controls
passed eight process-local cases; skipping the final recipient fails the
capacity-two fanout drop count. The test's bounded five-second setup retry
applies only to `has_connections()` when its worker queue is initially full;
publication, payload, queue-drop and receive assertions remain unchanged.

The matched Release characterization used the unchanged 128-step complete
clock/pose/IMU harness on a shared CPU 31, 20 ms minimum pacing and ten
alternating measured pairs plus a warmup pair. All 22 sample work checks
passed. Parent and candidate medians were 37.3309 and 38.5756 microseconds
per step; paired geometric overhead was 3.598%, bootstrap 95% interval
[-0.757%, 7.658%]. The point is within the proposed 5% budget, while the
interval does not establish a 5% upper bound. The [semantic review](reviews/publication-cost-source.json)
and [performance review](reviews/publication-cost-performance.json) are
separate. The normal protected hook, hosted qualification and simulator
adoption were checked independently; public `c21ffcc7` passed hosted run
`40248292544`. This does not strengthen the stated performance interval.

## Owned member callback observations

The pinned `NodeTest.ClassMemberCallbackMessage` and
`NodeTest.ClassMemberCallbackMessageInfo` originals now run in every strict
gate. The owned Rust client tests preserve invalid and valid registration,
publisher construction and connection predicates, the original 100 ms
settle, Int32 data 5, callback completion, and the Info case's exact topic,
partition, descriptor and intra-process values. The original-path Publisher
drops at its method return before callback completion. A separate test checks
joined owner retirement with a surviving Publisher. The first candidate
lacked the required SPDX line; the second retained the Publisher too long.
Both were superseded before [independent approval](reviews/class-member-callback.json).
The final test and mandatory-native verifier change no production Rust;
the exact staged source gate passed 195 Rust tests and both native originals
with 2486/2543 production lines and 3690/3846 regions. Native callback
registration and scheduling remain outside this test-only adaptation.

## Owned member service observations

The pinned `NodeTest.ClassMemberCallbackService` and
`NodeTest.ClassMemberCallbackServiceWithoutInput` originals now run in every
strict gate. The Rust owned Client tests cover invalid and valid service
admission, synchronous and raw Int32 data 5/result, admitted asynchronous
requests dispatched to an owned response method, wrong synchronous type
Timeout, and wrong asynchronous type admission followed by the original
300 ms service-method quiet interval. The service and response dispatch
threads join before owner destruction; a supplemental dropped pending handle
cannot take a later request's reply. `Node::request()` retains its existing
completion and error path through `begin_request().wait()`.

The [independent review](reviews/class-member-service.json) approved the
source and assertions after native 2/0, private Rust 3/0, library 82/0 and a
response-type-guard mutant that fails the wrong-type assertion. The exact
staged source gate passed with 17/17 changed production lines covered. Those older waiter variants remain unchanged.
New direct variants in
`service_callbacks.rs` use a Client-owned Node and the bounded callback APIs,
checking the response flag immediately after request return. They preserve all
original predicates and the native wrong-type assertion order. Private controls
cover admission and spawn rollback, bounded request/thread permit lifetime,
nonpreemptible user code, panic/error outcomes, cancellation, deadlines,
reentrancy, self-drop and payload bounds. Public supplements keep local domain
selection and exactly-once behavior explicit, and exchange real C++ requests and
replies with a fresh native peer. See the [callback contract](NET-02-response-callbacks.md).
Only these two selected original cases gain faithful-port credit; C++ object
identity, global discovery and broader scheduling remain open. The historical
lost-output status 101 remains unclassified.

The pinned `NodeTest.ClassMemberRequestServiceBeforeAdvertise` original is
also mandatory. It passes natively 1/0. The owned Rust test submits Int32
data 5 before service registration, then checks the provider and response
methods, true result and completion within the original 10-second bound.
The worker retains the request ID and deadline and retries a later exact typed
local service once. Independent [source review](reviews/class-member-before-advertise.json)
passed raw-export native/Rust controls and a retry-removal negative; the
unchanged parent likewise times out. The staged source gate passed with
2533/2594 production lines, 3739/3902 regions and 60/60 changed lines.
The ledger credits this one observable original; C++ callback API identity,
scheduling and remote provider races remain outside the claim.

## Local service lambda observations

`NodeTest.ServiceCallAsyncLambda` and
`NodeTest.ServiceCallWithoutInputAsyncLambda` now run in the mandatory pinned
native suite. Each advertises `/foo`, sends Int32 data 5 or Empty, and checks
the provider and response closures, reply data 5, true result and an executed
flag immediately after the request call. The owned Rust adaptation dispatches
the actual request through a scoped service thread and invokes the response
closure from the actual reply before its adapter returns. An exactly-once
response count is supplemental.

The [source review](reviews/service-lambda.json) replayed native 2/0 and Rust
2/0 from a raw export; removing the response-closure call makes both adapter
flag assertions fail. The exact staged source gate passed with 2533/2594
production lines and 3739/3902 regions; no production line changed. A
[superseding disposition review](reviews/service-lambda-disposition-correction.json)
found that native invokes the response callback before `Node::Request` returns,
while Rust invokes it after `Node::request` returns and before the test helper
returns. That correction remains valid for the retained adapter variants.

New direct variants in `service_lambda_original.rs` use `serve_callback` and
`request_callback` on one Node. They preserve successful registration and
request admission, Int32 data 5 and true result, and check `executed` immediately
after the actual request call. No adapter, readiness query or completion wait
precedes that assertion. Both callbacks count entries before parsing/assertions
and must run exactly once. Completion is checked afterward, so a caught panic
cannot disappear. The original bodies are untimed; the Rust API's one-second
bound is an explicit capability translation. Independent `/root` review passed
fresh four old/new Rust tests and native originals 2/0, with all old bodies
unchanged. Only these two cases gain faithful credit after strict local and hosted gates;
global identity, remote immediate completion, indefinite-timeout equivalence
and broader scheduling remain unclaimed.

## One-way local callback return boundary

The pinned `NodeTest.ServiceCallWithoutOutputAsyncLambda` original checks
Int32 data 5 and an `executed` flag immediately after its one-way request
returns. The owned Rust callback registration now checks the flag immediately
after raw `request_oneway` returns. A held-callback control requires that
request to remain pending until invocation completes; queued other-Node
requests retain their asynchronous admission boundary. The exact staged
source gate passed pinned native originals, the Rust suite, 2645/2714
production lines, 3893/4071 regions and 106/108 changed executable lines.
An earlier test-only adapter was excluded because it set the flag only after
raw request return, and the first production gate's 98/112 changed-line
coverage failure is retained. See [the bounded callback contract](NET-02-oneway-callback.md).

Independent [source review](reviews/service-oneway-callback.json) replayed
the pinned native original 1/0 and exact raw-export Rust original 1/0, plus
all seven focused Rust controls. Removing only the direct callback invocation
fails the original at the post-request `executed` assertion. The reviewer
approved the fixed same-Node observable original as `faithful_port`. C++
callback API identity, exact general scheduling and native no-timeout API
identity remain unclaimed. A separate bounded repair now exercises same-domain
cross-Node one-way completion with a freshly compiled native two-Node observer:
Int32 data 5, caller thread, invalid name, wrong-type and withdrawal quiet
intervals. Real native/Rust wire directions remain tested separately.

The shared weak callback registry retains own callback/plain-queue priority and
keeps worker metadata unchanged. Dedicated controls require thread permits
through blocked execution after lease retirement, registration spawn/queue/commit
rollback, request admission through user destruction, ordinary Drop unwind,
self-drop, domain and payload validation, and no new inline execution deadline.
The earlier queued-error supplement uses explicit `begin_request` with Empty
and the unchanged wait/map fallback so its error/count/progress predicates still
exercise queue handoff rather than inline completion. No native-original body
or assertion changes, and this repair earns no new original-case credit.

## Wrong-type publication with local delivery

The pinned `NodeTest.PubSubWrongTypesOnPublish` original passes 1/0. The
test-only Rust port preserves its Int32 data 5, Vector3d components 1/2/3,
valid typed advertisement and local subscription, connection settling,
rejected wrong publication with an immediate false callback flag, then
successful right publication and callback data/flag within ten seconds.
`InvalidFrame` maps the native false return before queue admission.

The exact staged source gate includes this original among 49 native Node
tests and passes the Rust suite with 2645/2714 production lines and
3893/4071 regions. No production line changed. An initial selected Rust
compilation failed because the test imported `prost` directly; it was fixed
to use `gz_msgs::prost::Message`, and the corrected selected test and exact
staged gate pass. C++ callback registration and thread identity remain
outside this fixed observable port.

Independent [source review](reviews/pubsub-wrong-type-publish.json) replayed
the pinned native original and selected Rust original 1/0 each from exact
source. Removing the typed publication guard makes the wrong-type
`InvalidFrame` assertion fail with `Ok(())`; the reviewer approved the fixed
observable original as `faithful_port` with the API limits above.


## Wrong-type subscription on the same Node

The pinned `NodeTest.PubSubWrongTypesOnSubscription` original now runs in every
native gate. Its test-only Rust port retains successful Vector3d advertisement,
Int32 subscription, the original 100 ms settle, successful Vector3d(1,2,3)
publication and immediately false callback flag. A supplemental joined receive
thread requires exact Timeout and no delivered message. C++ callback registration
and scheduling identities remain unclaimed.

The exact frozen source passed strict staged verification with 2645/2714
production lines and 3893/4071 regions; no production source changed. Independent
[review](reviews/pubsub-wrong-subscription.json) replayed native and raw-export
Rust originals 1/0 each. Changing the subscriber type to Vector3d failed the
post-join false-flag control. See [the scoped contract](NET-01-wrong-type-subscription.md).
The final protected hook and exact hosted run `40284202185` passed on public
`a0e48c985958580e243f894592896eed028ee7a3` (reviewer-bound tree `7776318b`).


## Two subscribers with different message types

The pinned `NodeTest.PubSubWrongTypesTwoSubscribers` original runs in every
native gate. The test-only Rust port retains two Nodes, Int32(5), matching Int32
and mismatching Vector3d subscriptions, a matching connection wait, successful
publication, good callback data/flag and bad callback false. The connection and
matching receive waits have separate ten-second bounds. The bad receiver stays
live through the good callback and a supplemental 300 ms quiet interval; exact
Timeout is its only accepted no-delivery result.

Independent static review rejected a superseded source whose good deadline
began before connection readiness and whose bad subscription could drop before
good delivery. The corrected source passed its own exact staged gate: 213 Rust
tests, pinned native originals including Node51/0, production 2645/2714 lines and
3893/4071 regions, no production source edit. Independent [source review](reviews/pubsub-wrong-two-subscribers.json)
replayed native and raw-export Rust originals 1/0 each; changing only the bad
subscriber type to Int32 failed the bad=false predicate. The fixed observable
original is a faithful port; C++ callback registration and exact scheduling
remain unclaimed. The final normal protected hook and exact hosted run
`40287897483` passed on public `02cbb6836b74d4c5ffd9d212952bdf8b5ef65cd8`,
reviewer-bound tree `faeae393`. See [the bounded contract](NET-01-wrong-two-subscribers.md).


## Wrong-type publication receive ordering

The existing wrong-type publication port now waits for connection readiness
before starting its ten-second good-message receive. All payload values,
wrong/right publication results, immediate wrong callback flag and decoded good
callback data/flag assertions remain unchanged. Independent source review found
the previous early receive wait could expire while connection readiness was
still being awaited. The correction preserves the two separate bounded waits.

The exact one-file source gate passed 213 Rust tests, mandatory pinned native
originals including Node51/0, strict 2645/2714 production lines and 3893/4071
regions; no production line changed. Independent [review](reviews/pubsub-wrong-type-publish-deadline.json)
replayed the exact raw-source Rust original and pinned native original 1/0 each.
A private scaled old-order control failed with Timeout while the same fixed-order
control passed; these controls establish relative ordering only, without a
scheduler or full timeout-window claim. The final normal protected hook and exact
hosted run `40292391143` passed on public `315cf3b2d9411acd94cc68c019198b972dcc82ec`,
reviewer-bound tree `16e80704`.


## Wrong request type for a local service

The pinned `NodeTest.SrvRequestWrongReq` original runs in every native gate.
The test-only port retains a same-Node Int32 echo service, Vector3d components
1/2/3, actual asynchronous request admission, the original 300 ms false callback
flag and exact Timeout for the synchronous 1000 ms request. Active provider and
response observers remain alive through both checkpoints; shutdown begins only
afterward, and unexpected errors fail. Both joins plus a supplemental final false
flag prevent late queued replies from being silently hidden.

The exact frozen source passed 214 Rust tests and pinned native originals
including Node52/0, with strict 2645/2714 production lines and 3893/4071 regions;
no production line changed. Independent [review](reviews/service-wrong-request.json)
replayed native and exact raw-source Rust originals 1/0 each. A valid asynchronous
Int32(5) control exercised provider request data and observer response data/true
result, then failed the retained 300 ms false-flag assertion. Native unbounded
asynchronous lifetime, C++ callback registration and precise scheduling remain
unclaimed. See [the bounded contract](NET-02-service-wrong-request.md). Final
normal protected hook and exact hosted qualification remain separate.


## Wrong response types for local services

The mandatory native suite now includes `NodeTest.SrvRequestWrongRep` and
`NodeTest.SrvWithoutInputRequestWrongRep`. Their shared Rust harness preserves
same-Node Int32(5) or Empty input, actual asynchronous admission, any-response
callback flag false after 300 ms, and exact synchronous Timeout with the native
1000 ms bound. Both observers remain active through the original checkpoints;
explicit cancellation and joins occur afterward, followed by a supplemental
false flag. The previously accepted wrong-request case retains its values,
echo bytes and all observations.

The exact source gate passed 216 Rust tests, native Node54/0, 2645/2714
production lines and 3893/4071 regions, with no production change. Independent
[review](reviews/service-wrong-response.json) replayed native2/0 and raw Rust3/0.
Each valid expected-response Int32 control exercised provider/observer data5 and
true result, then failed the retained 300 ms false predicate. All source hashes
were restored and Rust3/0 passed again. Native unbounded asynchronous lifetime,
C++ callback API and scheduling remain unqualified. See the
[bounded contract](NET-02-service-wrong-response.md); final normal protected hook
and exact hosted qualification remain separate.


## Mixed wrong and good service response types

The mandatory native suite includes `NodeTest.SrvTwoRequestsOneWrong` and
`NodeTest.SrvWithoutInputTwoRequestsOneWrong`. The ports preserve wrong
synchronous Timeout with 1000 ms, admitted wrong asynchronous request and false
callback after 300 ms, successful good synchronous request with 1000 ms, and
admitted good asynchronous request with data5/true callback flag within a fresh
ten-second observation. Both asynchronous ownership bounds are thirty seconds;
the wrong observer remains live through the later good completion. Cancellation
and joins follow the original checks, then a supplemental false flag detects
late wrong replies. Native unbounded lifetime and C++ API/scheduling stay open.

The exact source gate passed 218 Rust tests and Node56/0, with strict unchanged
2645/2714 production lines and 3893/4071 regions. Independent
[review](reviews/service-mixed-response.json) replayed native2/0 and raw Rust2/0.
A valid wrong-async response control exercised data5/true and failed both 300 ms
false flags; wrong good-async response controls failed both ten-second required
completions. Secondary thirty-second wrong-observer errors during failed-control
unwinding are disclosed separately. Exact source restored and Rust2/0 passed.
See the [bounded contract](NET-02-service-mixed-response.md); final normal hook
and exact hosted qualification remain required.


## Wrong service types across processes

The mandatory ordinary service integration filter adds the unchanged native
`twoProcSrvCall.SrvRequestWrongReq` and `twoProcSrvCall.SrvRequestWrongRep` plus
`SrvTwoProcs` as a native-only positive. Existing ServiceInfo/ServiceList filters
remain. The two Rust ports use a real C++ typed echo peer, bounded READY/QUIT
ownership, actual empty-filter discovery and provider-liveness checks. Vector3d
1/2/3 or Int32 data5, actual async admission, original 300 ms false callback and
exact synchronous Timeout with 1000 ms remain; explicit cancellation follows the
checks, then a join and supplemental final false flag retain late replies.

The exact source gate passed 220 Rust tests, Node56/0 and ordinary integration5/0,
with strict unchanged2645/2714 lines3893/4071 regions. Independent
[review](reviews/remote-service-wrong-types.json) replayed native2+positive1 and
raw Rust2 against a fresh real C++ peer. Async-only valid Int32(5) controls
received actual data5/true then failed the retained false flags; all166 hashes
were restored and Rust2/0 passed again. Ten-second owned async, original native
six-second provider lifetime, native unbounded pending and C++callback/scheduling
limits are explicit. Native positive counter/storage predicates receive no Rust
port credit. See the [bounded contract](NET-02-remote-service-wrong-types.md).
Final normal protected hook and exact hosted qualification remain required.


## Inputless wrong-response service across processes

The existing inputless ServiceInfo/ServiceList native filters now also require
`twoProcSrvCallWithoutInput.SrvRequestWrongRep` and a native-only `SrvTwoProcs`
positive. The shared Rust harness adds the fixed Empty/empty-bytes request with
Vector3d expected reply, preserving original asynchronous admission, 300 ms false
callback and exact 1000 ms synchronous Timeout. The real native inputless
provider returns Int32 data 5/true; actual empty-filter discovery and liveness
are supplemental. Existing ordinary assertions and filters remain unchanged.

The exact source gate passed 221 Rust / 24 tooling tests, Node 56/0, ordinary
service 5/0 and inputless service 4/0, with unchanged strict 2645/2714 lines and
3893/4071 regions. No production lines changed. Independent
[review](reviews/remote-service-inputless-wrong-response.json) verified all 208
retained reports and replayed native 1+positive 1 and raw Rust 3/0 with a fresh
C++ peer. Its async-only valid expected-reply control received actual data 5/true
then failed the original false checkpoint; source restored and Rust 3/0 passed.
Setup-only reviewer filename/proof failures are disclosed separately. Native
positive callback/counter assertions receive no Rust port credit. Owned/native
lifetime and C++ API/scheduling limits remain explicit in the
[contract](NET-02-remote-service-wrong-types.md#inputless-wrong-response-original).
Final binding, normal protected hook and exact new hosted qualification remain
required.


## Mixed wrong and good remote service responses

The mandatory ordinary and inputless native filters include their pinned
`SrvTwoRequestsOneWrong` originals. The shared Rust ports retain the initial
500 ms settle before Node creation, wrong synchronous 2000 ms Timeout, admitted
wrong async and false callback after 300 ms, good synchronous 2000 ms success,
and admitted good async with data 5/true and required flag after 300 ms. The wrong
observer stays live through the good checkpoint. Synchronous payload/result,
actual discovery/liveness and late wrong=false are supplemental. Ten-second
owned pending, native unbounded/fixture-six-second lifetime and C++ API/scheduling
limits remain explicit; cleanup occurs at the tail or failed-assertion unwind.

The exact source gate passed 223 Rust / 24 tooling tests, native Node 56/0,
ordinary service 6/0 and inputless service 5/0. Strict coverage remains 2645/2714
lines and 3893/4071 regions, with zero production edits and all 208 source reports
retained. Independent [review](reviews/remote-service-mixed-response.json)
replayed native 2/0 and fresh raw Rust 5/0. Valid wrong-async controls received
actual data 5/true then failed both false checks; wrong good-async descriptors
were admitted then failed both required true checks. Both controls kept the
three earlier cases passing and produced no secondary unwind errors. Source
restored and Rust 5/0 passed again. The [contract](NET-02-remote-service-wrong-types.md#mixed-wrong-and-good-remote-responses)
scopes the two fixed originals. Final binding and the normal protected hook
passed on public `72fdf3a13ae2061cc89afb401ee04d9bfe03d4e4`; exact hosted run
`40344472155` succeeded for that mixed-response slice.


## Remote wrong-type subscriptions

The mandatory native pub/sub filter includes the typed and raw remote
wrong-subscription originals. Their Rust ports retain Int32 subscription
admission, the original 1500 ms wait and the false callback flag, with a live
joined observer. The exact native publisher fixture and its two publications
remain unchanged; actual matching Vector3d values 1, 2 and 3 are supplemental.
The [contract](NET-01-remote-wrong-subscription.md) records callback and scheduling
limits, tail/unwind cleanup and the separate retained native-loader failure.

The exact source gate passed 225 Rust and 24 tooling tests, native Node 56/0,
pub/sub 9/0, ordinary service 6/0 and inputless service 5/0, with unchanged strict
2645/2714 lines and 3893/4071 regions. All 209 source reports were retained.
Independent [review](reviews/remote-pubsub-wrong-subscription.json) replayed
native 2/0 and fresh Rust 2/0. Valid-type controls received actual values in both
wrong observers, then failed both primary 1500 ms false assertions; restored
source passed 2/0. The reviewer rechecked the full coverage and changed-line
evidence without filtering any regions. Final binding and the normal protected
hook passed on public `9737abdee625bcb6ef9d5b59dbcf3f30550eacca`; exact hosted
run `40356975861` succeeded for that two-original slice.


## Matching and generic remote subscribers

The mandatory native pub/sub filter adds both three-subscriber originals. Their
Rust ports retain 500 ms before three-Node construction, three admissions, the
2500 ms false/true/true flags, and the generic callback's topic-without-`@`
predicate. All observers remain live through the original checks. Payload and
child-lifetime checks are supplemental; the existing exact native publisher is
unchanged. See the [contract](NET-01-remote-wrong-subscription.md#matching-and-generic-remote-subscribers).

The exact source gate passed 227 Rust and 24 tooling tests, native Node 56/0,
pub/sub 11/0, ordinary service 6/0 and inputless service 5/0. Strict coverage stays
2645/2714 lines and 3893/4071 regions; zero production changes and all 209 source
reports are retained. Independent [review](reviews/remote-pubsub-mixed-subscribers.json)
replayed native 2/0 and fresh Rust 4/0. Compatible wrong and incompatible generic
descriptor controls fail the original flag predicates; a production topic-view
control fails the original metadata predicate, with expected join propagation
retained. Old tests pass throughout; restored source passes 4/0. Full source
evidence and unchanged coverage policy were independently rechecked. Final
metadata binding and the normal protected hook passed on public
`f963c83a736701b64e2a529cddf068099c73e416`; exact hosted run `40370522967`
attempt 1 succeeded.

## Message publication scopes

The [scope contract](NET-01-publisher-scope.md) adds explicit canonical
Process/Host/All message options while retaining All defaults. Four pinned native
Process/Host and remap originals passed before implementation; their Rust ports
retain publisher/subscriber admission, separate joined thread, original
10-second/10-millisecond readiness, Int32 data 5 and actual execution flag.
Existing All/default threaded assertions remain required. Supplements check
native discovery metadata and payloads, Host query admission, Process initial and
withdrawal suppression, and live process-local readiness and retirement.
Native Process heartbeat advertisements remain present.

The exact source gate passed 234 Rust and 24 tooling tests, native Node 60/0,
pub/sub 11/0, ordinary services 6/0, inputless 5/0 and all other required groups.
Strict owned coverage is 2702/2778 lines and 3971/4157 regions across all five
production files; automatic changed coverage is 73/73 executable lines. All 209
source reports are retained. Independent [review](reviews/publisher-scope.json)
replayed native 6/0 and fresh Rust 95/0, verified three production controls and
restored 95/0, then recomputed unchanged full JSON/LCOV and added-line policies.
Expected primary and secondary control failures, author compile/readiness
failures and reviewer assembly errors remain retained.

The scope slice then passed final metadata binding and the normal protected
hook on public `7cf70b20827fe1d80dc01f4ec3aa5f658b3c632c`, followed by exact
hosted run `40384167730` SUCCESS. Native callback/storage/scheduler facades,
immediate global visibility, physical multi-host qualification, service scope
and performance remain open.


## Message publication rates

The [rate contract](NET-01-publisher-throttle.md) maps six fixed originals:
NodeTest.PubThrottled and five message-option cases. It preserves the original
1 Hz / three successful publications / false readiness / 100 ms sleeps / live
counter-one checkpoint, plus exact default/copy/assignment/equality/accessor
observations. Native six originals and sixteen edge runs passed before edits
and independent native-only review. Rust zero/MAX, shared typed/raw interval,
fractional/sub-nanosecond/epoch and explicit clock-error supplements pass.
Sixteen native typed/raw scenarios check real discovered rate metadata and
callback payload/counts. Native callback and Publisher copy APIs, concurrency,
clock precision, other backends, stream formatting, the remote throttle original
and performance remain unqualified.

The exact source gate passes 247 Rust and 24 tooling tests, Node 61/0, message
options 5/0, pub/sub 11/0, ordinary services 6/0, inputless 5/0, outputless 2/0
and all unchanged required groups. Strict coverage is 2770/2848 lines and
4065/4256 regions across five production files; all 73 added executable lines
are covered. All 210 source reports are retained. Independent
[review](reviews/publisher-throttle.json) verifies fresh raw source, native six,
Rust 98 selected cases including sixteen native scenarios, five production
mutations with six intended failure commands and restored passing tests/hashes.
Full JSON/LCOV and authoritative changed-line policies are recomputed unchanged.

The first selected run's two absent-peer failures are retained with exact source;
canonical peer construction allowed an unchanged retry to pass. Metadata path
assembly failures and a corrected commentary line-total error are also retained.
No assertions or thresholds are waived. That rate slice passed final binding
and the mandatory normal protected hook on public
`2ef23aa61fba7124397be666b6d7205f32cb5d8f`, followed by exact hosted run
`40398288785` attempt 1 SUCCESS.


## Message-option stream bytes

The [formatting contract](NET-01-message-options-format.md) maps the fixed native
`AdvertiseOptionsTest.msgStreamInsertion` original to its unchanged two literal
byte predicates. Before edits, native 1/0 and 24 exact scope/rate output rows
passed; independent native replay reproduced them. Rust adds the same native-byte
matrix and prefix/tail writer-error checks. All earlier tests remain required.

The exact source gate passes 250 Rust and 24 tooling tests, 2781/2859 lines and 4081/4272 regions, with
11/11 changed executable lines covered across
the same five production sources. Native Node61, message-options6, pub/sub11,
ordinary-service6, inputless5, outputless2 and the other unchanged required
groups pass. All 211 source reports are retained.
Independent [review](reviews/message-options-format.json) replays 89 library and
three formatting tests on fresh source, verifies scope/rate/writer-error
production controls and restored tests, and binds the full unfiltered JSON/LCOV
and automatic changed-line policy. No stream-state, locale, base/service
formatting or performance claim follows from this bounded default-byte port.
The native review's stdout hash timing correction is retained; no candidate
runtime failure, assertion relaxation or coverage waiver applies. Final metadata
binding, the mandatory normal hook and exact new hosted success remain required.


## Subscription rate admission

The prior message-option formatting slice passed its normal protected hook on
public `468cdf4783ed35059126059df688427cdcc15aae`, followed by exact hosted run
`40416633542` attempt 1 SUCCESS.

The [subscription rate contract](NET-02-subscription-rate.md) maps four fixed
native originals: NodeTest.SubThrottled and SubscribeOptions copy construction,
accessors and throttled state. Their original publication/readiness/value/count,
100 ms waits and option predicates remain intact. Native four passed before
edits; the independent 64-row probe is bounded characterization, with raw
ignore-local divergences retained and no blanket Rust port credit. All earlier
raw-ignore assertions remain unchanged.

Author and independent selections pass 101 Rust checks. Independent production
controls verify admission, shared local budget, filter order and recipient clock
errors, followed by restored passing source. The exact full gate passes
259 Rust and 24 tooling tests, 2829/2907 lines and 4146/4337 regions; 47/47 changed
executable lines are covered, and all 212 source
reports are retained. Full JSON/LCOV and authoritative changed-line policy are
independently checked without filtering. Fixed original mappings do not imply
native callback lifetime/concurrency, queue, settled/remote count or performance
parity. Final metadata binding, mandatory normal hooks and exact new hosted
success remain required. No assertions or coverage thresholds are waived.


## Fixed base-option scope values

The preceding subscription-rate slice passed its ordinary protected hook on
public `4af4e1872c0f1e38f07dcbb5ee4c49a441fd52a2`, followed by exact hosted
`40447602240` attempt 1 SUCCESS.

The [base-option value contract](NET-01-base-option-values.md) preserves five
fixed native originals and their nine scope predicates. The combined Rust options
retain the default message rate for all these inputs; base/service stream bytes
and broader equality/lifetime/network semantics remain outside this mapping.
Five native originals passed before edits, independent native replay passed five,
and author and fresh reviewer Rust selections each passed five. Three production
controls fail the intended predicates and restored source/tests pass.

The canonical source gate passes 264 Rust and 24 tooling tests, 2829/2907 lines and 4146/4337 regions, with
no production source changes. All 213 source reports
are retained. Independent [review](reviews/base-advertise-option-values.json)
recomputes full unfiltered JSON/LCOV and the empty production delta using unchanged
policies. Every old test and partial mapping remains. Final metadata binding,
mandatory normal hooks and exact new hosted success remain required.

## Bounded service-list startup

The [service startup query](NET-02-ready-service-list.md) adds deadline-bounded
production behavior while retaining immediate snapshots and all old assertions.
Its supplemental Rust tests exercise public and worker error/lifecycle boundaries.
A real native peer compares fresh and warm native/Rust lists for three service
signatures and an absent provider, then checks native withdrawal separately.
These tests run in the ordinary full gate; they confer no additional original
case credit and claim no convergence or latency equality.

## Owned service-provider metadata

The [owned service metadata contract](NET-01-owned-service-info.md) adds actual
provider records while preserving type-only queries and original assertions.
Five supplemental tests cover public/worker boundaries and fresh native
every-field comparisons for six service fixtures. Equal signatures retain
provider multiplicity; retired metadata remains owned without keeping a
registration live. The ordinary full gate runs these tests; no original case
gains additional credit or a general discovery/latency claim.

## Owned message-publication metadata

The [publication metadata snapshot](NET-01-owned-publication-info.md) has six
focused tests covering public lifecycle/domain boundaries, worker filters/errors
and fresh native every-field comparisons for six fixtures. Eight native option
controls preserve effective flag/rate semantics without changing raw records.
The tests join the normal full gate; prior assertions and ledger dispositions
remain. These checks establish no global discovery or performance claim.

## Owned subscriber metadata

The [subscriber metadata contract](NET-01-owned-subscriber-info.md) preserves
existing typed-view assertions beside a bounded native-keyed full-record cache.
Eight new checks cover native storage identity/retirement, public and worker
boundaries, overflow observability, local/remote normalization and six real
native every-field fixtures. Prior typed identity and publication metadata
checks remain unchanged. The normal source/native/coverage gate remains required.

## Publication routes across discovery silence

The [route lifetime control](NET-01-publication-route-lifetime.md) drives an actual
native Node and Rust Node from one raw ZeroMQ publisher. Both must receive after
heartbeat-only traffic and discovery silence with publisher metadata absent.
Explicit native/Rust withdrawal differences remain visible. Four private controls
cover failed-connect capacity recovery, full-capacity withdrawal/replacement in
one batch, independently bounded routes, drop counts, shared addresses, multiple
topics and unsubscribe/shutdown release. Old freshness, channel and withdrawal assertions
remain unchanged; no ledger promotion or broad lifetime completion is implied.

## Immediate admitted local service resolution

The [local resolver contract](NET-01-local-service-resolution.md) retains the old
barrier-based ambiguity fixture and adds the direct original with zero timeout.
Four public/native tests and four private controls check immediate selection,
lifetime/rollback, stale copied peer masking, bounded retained owners, actual
worker expiry and domain isolation. Private Node construction shares the admitted
vector with its worker identity; all prior assertions remain. Normal production
coverage and real native gates remain mandatory before promotion/publication.

The admitted cross-Node queue implementation has separate supplemental
`admitted_service_queue_*` controls and `tests/service_queue_local.rs`; no
ledger row changes. It reuses the original sender through Weak/Arc ownership,
keeps own receiver priority, sends real replies through the provider lane and
mandatory ZeroMQ route, and preserves worker-only inspection. Private race
coverage explicitly pauses an upgraded sender across withdrawal/cleanup;
concurrent disconnect can wait for that single operation. The new native
`service-local-process-scope` peer control checks actual PROCESS metadata and
cross-Node request/withdrawal. No scope-option API, native scheduling guarantee
or process-global identity completion is inferred. See the
[queued service contract](NET-02-local-service-queue.md).

Independent `/root/sdformat_owner` source/contract review passed fresh 134 library
and 31 public/native candidate tests versus accepted cf8f2ec's 127 and 24,
followed by the final seven bounded-receive controls. Proper loopback permission
is required; the initial sandbox EPERM/abort is failed setup, not a candidate
regression. Root approved the unchanged production ordering and final contract.
Author-only handoff and sender-retention negatives remain separate evidence.
Resumed/merged author coverage is diagnostic only; the single normal staged hook
must run from scratch and exact hosted CI must pass before acceptance. No
upstream row or accepted count changes for this queue slice.


The bounded service-scope slice adds six unchanged native srv option originals,
separate Rust value/format controls and actual three-scope data/packet evidence.
The dedicated scope C++ peer uses an actual admitted service record and the
pinned native Discovery template with an explicit controlled heartbeat interval;
its eight fields, initial/Subscribe/heartbeat/withdrawal packets and local data
are asserted. Real native remote requests distinguish All/Host from Process,
while Rust local queue and callback forms preserve Int32(5). Process periodic
packets remain mandatory; worker-only views reject them. Private controls retain
legacy All constructors and test Host locality, rollback and cancellation.
Both native helper linkages are checked against the selected SDK. See the
[service scope contract](NET-01-service-scopes.md). Independent source/assertion
review passed 136 library, 42 public/native and six unchanged native checks,
including the final distinct-port setup. Only those six value originals gain
candidate faithful-port credit; normal staged coverage and exact hosted
qualification remain required before acceptance.


The [statistics transport profile](NET-01-statistics-wire.md) has separate exact
configuration, wire-frame, mode-domain, history and lifecycle controls. The
previous explicit statistics=1 Unsupported input moves into a complete positive
case after prior independent review; all unrelated negative configuration rows
remain byte-equivalent. Native four-pair and sequence lifecycle controls preceded
implementation. Actual C++/Rust publications and RPC pass in both directions
across matching and mismatched modes, including opaque/empty payloads and
same-worker re-registration/orphan continuity. Local callback entry counters are
checked before waiting and after the wrong-mode quiet window. Malformed metadata
sizes, multipart counts, drain/progress and cancellation target the production
receiver through inproc sockets. These also require socket-authorized execution;
the retained default-sandbox signaler abort is setup failure, not a passing run.
Echo tombstones retain no sequence ownership after the wire owner retires.
Independent final review passed 149 library and 46 public/native checks with
fresh source, target and canonical peers, plus the rebuilt native-only sequence
lifecycle. The final test verifies actual failure/drop accounting and retained
local delivery for missing sequence ownership, a controlled nonblocking send
failure and absent interest; it does not claim an injected wire-clock failure.
No ledger rows are promoted. The complete normal staged gate and exact hosted CI
remain required; metric and global-identity work stays open.

The [SampleStatistics value](NET-01-sample-statistics.md) preserves the complete
native MinMax and AvgStdDev scalar cases, including the original 1e-3 tolerance.
The normal gate also runs all four unchanged native TopicsStatistics originals;
Constructor and DroppedMsg remain pending because topic aggregation is absent.
A separate pure native helper emits every scalar getter from exact input bits.
The retained 604-state corpus, all reduced library underflow counterexamples and
43,122 longer adversarial states are compared against that helper. It creates
no Nodes or sockets. Constructor sentinels, signed-zero/NaN comparison order,
nonfinite count propagation and independent value copies are covered. Private
synthetic count controls verify explicit u64 wrap and width; they are source
boundary tests, not a claim of 2^64 actual native updates. No dependency or
operational counter/runtime behavior changes. Independent fresh source/target
and canonical helper review passed two private and six public/native tests in
both debug and release, four unchanged native originals and independently
rebuilt five-version/six-reduction library controls. Only MinMax/AvgStdDev gain
candidate faithful-port credit; normal strict staged coverage and exact hosted
qualification remain required before acceptance.
