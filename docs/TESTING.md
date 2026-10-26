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

`NodeTest.AllTopicInfo` is mapped partially by
`original_four_node_all_topic_info`: all original immediate four Node topic
names and endpoint count assertions are retained, while native one-vector
`MessagePublisher` metadata and atomicity remain unqualified. The frozen
seven-file source passed the exact staged native/Rust/strict coverage gate
(166 Rust; 2085/2124 production lines, 3142/3271 regions, 249/251 changed
lines). Independent [review](reviews/four-node-topic-info.json) replayed the
original and Rust selected cases, both supplemental cases, serial full Rust
and two boundary mutants. Native Publisher lifetime after Node destruction
remains open. Normal final hook and hosted CI are pending at this review point.

The later [Publisher ownership review](NET-02-publisher-after-node-drop.md)
covers ordinary Node destruction with a surviving Publisher and retains the
earlier staged interop failure as unclassified. That later slice does not
change the partial AllTopicInfo disposition above.

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
staged source gate passed with 17/17 changed production lines covered. Both
ledger cases remain partial: native member callback registration and
immediate-after-Request flag visibility are not reproduced by the bounded
Rust waiter. C++ object-pointer identity and scheduling are unclaimed. The
historical lost-output status 101 remains unclassified.

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
returns. Both originals are partial. Native callback registration, the actual
post-request timing predicate, exact scheduling/thread identity, remote
asynchronous behavior and native no-timeout semantics remain unported.

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
callback API identity, process-global other-Node inline behavior, exact
thread scheduling and native no-timeout API identity remain unclaimed.

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
scheduler or full timeout-window claim. Final normal protected hook and exact
hosted qualification remain separate.
