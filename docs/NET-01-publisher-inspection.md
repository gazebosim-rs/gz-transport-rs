# Bounded publisher subscriber membership

`Publisher::has_connections` snapshots matching known subscriber registrations
through a bounded worker command/reply. It returns `Result<bool, Error>` using
Config.reply_timeout for worker coordination; full queues, cancellation, timeout
and disconnected worker remain explicit errors. No pending-query collection,
registration permit or hidden runtime is introduced. This control-plane query
can wait on its caller and belongs outside simulation stepping.

Pinned reference: Node.cc 380–396, NodeShared.cc 1071–1290, HandlerStorage.hh and
TopicStorage.hh. Native HasConnections checks publisher validity plus matching
local handlers or remembered remote registrations. Its own authentication TODO
means membership is not an authentication, socket handshake or delivery receipt.
Rust empty-type local subscriptions map to native wildcard membership. Remote
canonical `google.protobuf.Message` also matches every descriptor.

Message-channel NEW_CONNECTION registers only when message metadata ctrl names
this publisher worker's process UUID. Native identity is topic/process/node/TCP
address; duplicate metadata keeps the first value. A separate registration table
is bounded by capacity; rejected new identities increment queue_drops. Native
unrestricted table growth is not claimed. Matching local active subscriptions
are read directly; dead local leases do not count. Service-channel records and
SUBSCRIBERS_REP inspection replies do not become publisher registrations.

Native process-only disconnects, BYE and publication-peer expiry deliberately
retain NodeShared's registration memory; explicit node/topic withdrawal removes
it. END_CONNECTION accepts empty or matching ctrl and removes every address for
that topic/process/node. Native dispatch constructs a fresh MessagePublisher and
projects absent/wrong message metadata to empty ctrl; subscriber-shaped and empty
END frames therefore use native default identities. These projection cases are
retained, rather than inventing a stricter message-oneof guard.

Graceful worker shutdown now sends one generic END per known subscription topic
before its process BYE, including a dropped lease whose ordinary cleanup has not
run yet. This follows native Node destruction's explicit unsubscription and
avoids leaving a known live publisher with only a process-disconnect event.
Lease permit accounting is unchanged. Actual process-only BYE retains membership;
this distinction does not introduce a delivery or network reliability guarantee.

The real native callback reference covers local membership, destination checks,
keep-first duplicates, process-only retention, wildcard and default END projection.
Real C++ subscription tests require actual known membership, one publication,
received data and explicit withdrawal while the process remains alive. Rust
worker tests cover table bounds, wrong channel/type/topic/node/process, inactive
leases, nonblocking abandoned replies, shutdown/queue/coordination failures and
ordered deduplicated END-before-BYE. Graceful Rust shutdown is also observed
through another real worker while the old subscription handle remains alive.

Four original helper ports are independently reviewed: PubSubTwoThreadsSameTopic,
PubSubTwoThreadsSameTopicRemap, ScopeAll and ScopeAllRemap. Every helper assertion
retains valid publisher/subscriber creation, actual HasConnections polling,
exact Int32 data 5, a single successful publish, joined subscriber thread and
actual execution flag. Native waitUntil defaults (10000ms, 10ms) are retained.
The explicit native ALL option binds to the Rust publisher's existing ALL default;
PROCESS/HOST controls and immediate process-global multi-Node sharing remain
pending. The original readiness wait makes asynchronous discovery observable;
no new zero-wait multi-Node visibility claim is made. Independent review approves
these four complete mappings within that explicit helper scope.

The first full unit replay exposed an older owned service race test's assumption
that a capacity-one queue was empty after registration retirement. Its original
zero-permit, empty-list and successful-recreation assertions remain. An actual
worker-admission barrier now establishes that precondition. A separate deterministic
pre-admission race additionally retains the full old queue, expects QueueFull,
checks rollback to zero permits, drains the retired command without advertising,
and recreates successfully. The initial failure is retained; no production
service algorithm, original assertion, tolerance or exclusion was changed.

No complete transport, scope/global-sharing, authentication, CLI or performance
acceptance is claimed by this bounded membership API.

The full local gate passes 108 Rust tests and 21 tooling tests, with 1480/1501
covered production lines, 2246/2310 regions and 84/84 changed executable lines.
Independent /root/msgs review used a fresh hash-verified private source/target,
then replayed all 53 library units, both live C++ membership checks, the four Rust
ports and all four untouched native originals (XML: four runs, zero failures).
It checked native callbacks/storage/default projection and the ALL binding, plus
the old race assertion tail and added deterministic queued-race coverage. Its
initial sandbox socket abort was retained; the permitted socket replay passes.
The exact [review receipt](reviews/publisher-inspection.json) is copied unchanged.

The ledger now contains 28 complete faithful ports, one partial and 389 pending
cases of 418. The matching immutable staged hook remains required before
publication, followed by hosted CI on the published revision. No whole packet,
process-global shared runtime, authentication, guaranteed delivery or performance
acceptance is claimed.
