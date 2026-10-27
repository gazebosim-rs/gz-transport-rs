# Bounded startup topic information and prepared publisher interest

This V2 candidate passed 18 targeted Rust checks and Clippy. Its predecessor
passed 108 selected Rust checks and a separate warm Clippy completion on the
predecessor source; the earlier Clippy resource stop and all binaries remain
preserved. It does not retire an upstream test or
change the existing immediate `known_topic_info` contract.

`Node::topic_info(topic, timeout)` waits for the message discovery worker's
startup boundary before taking the same descriptor snapshot and process-local
merge as `known_topic_info`. It qualifies the original topic without remapping.
The timeout bounds the startup and worker reply wait; zero returns `Timeout`,
an unrepresentable deadline returns `InvalidConfig`, and invalid names preserve
`InvalidName`. Queries consume a bounded pending queue, with explicit `QueueFull`
and shutdown outcomes. The worker continues handling sockets and other commands
while queries wait. No caller sleep blocks discovery or an authoritative tick.
The existing process registry merge retains its mutex and error discipline.

The initialization boundary follows two existing worker heartbeat intervals.
Native UDP discovery starts its heartbeat counter immediately and considers
initialization complete after two further cycles. Four independently replayed
native cases use a new query process after a distinct process registers a held
Clock publisher, StringMsg publisher, StringMsg subscriber, or no endpoint.
The first query observes the registered publisher type, subscriber-only interest,
or absence; the warm query retains the same identities within each run. Raw
elapsed times remain observations, with no timing equality or latency guarantee.
Native discovery is shared across Nodes in a process; startup here belongs to
each Rust worker. Process-global initialization latency parity remains unqualified.

The native query emits `SUBSCRIBERS_REQ` before waiting. This candidate preserves
that order and leaves the immediate method unchanged. Initialization does not
mean all remote replies arrived, agree on ownership, or acknowledge delivery.
Late competing publishers can still appear. A valid absent topic returns empty
vectors and is not a network-wide proof of absence.

`Publisher::refresh_connection_snapshot(timeout)` performs bounded worker coordination
outside a step and refreshes this exact live lease's cached membership. Zero
returns `Timeout` and deadline overflow is `InvalidConfig`, without enqueue.
A queued request whose deadline has expired cannot enroll prepared tracking.
A consumer can compute one checked preparation deadline and pass its remaining
budget to the startup query and both world/root refreshes. This does not assume
that `Config::reply_timeout` covers discovery initialization. It opts
that publisher into subsequent worker refresh of matching same-process direct
registrations, alongside already learned remote `NEW_CONNECTION` membership.
Other default-All publishers do not acquire this additional registry lookup.
The cached `connection_snapshot()` remains an atomic read and does not wait for
IO. Later membership changes follow the worker refresh boundary.

Prepared membership uses the same message domain, qualified topic, descriptor
and live registration as direct process delivery. Registration interest includes
ignored same-Node subscriptions; their payload filter remains intact. A full
queue or rate suppression can still prevent delivery. Remote subscriber metadata
from `SUBSCRIBERS_REP` alone is not promoted to a connected physical route, and
remembered remote membership retains the existing withdrawal rules. The method
is not authentication, payload acknowledgment, or a first-wire-delivery guarantee.

Supplemental tests exercise the first actual payload after preparation for All,
Host and Process scopes; three payload deliveries with repeated refreshes after
an actual TCP `NEW_CONNECTION`, plus a bounded negative duplicate observation;
wrong domains and descriptors; All, Host and Process withdrawal and recreation;
ignored local delivery; Publisher survival after ordinary Node drop; cancellation
and explicit shutdown. Additional checks cover pending queue bounds, deadline
and startup progression, immediate query availability, stale lease identities,
zero/overflow requests without enqueue, expired queued refreshes without tracking,
and isolated registry failure. All/Host direct-registration tests check actual
queue delivery while their private worker never drains discovery, so they also
check that preparation does not fabricate remote connection membership. Old test
bodies and stronger raw-ignore assertions remain unchanged. Root simulator clock assertions and its c21 consumer pin are
unchanged. The eventual integration must prepare outside stepping and retain its
full clock bytes and next-frame assertions.

The 18 targeted checks cover four prepared-interest private tests, one startup
private test, six public API tests, one real C++ startup test, and six existing
immediate-query, process-query, snapshot and lifetime regressions. Clippy passed
for the library and the new API and interop targets. The warm target used
1,231,637,208 bytes within its 1.5 GiB cap; the minimum observed free space was
9,184,333,824 bytes. These are correctness resource observations, not benchmarks.

Independent review passed 110 selected Rust checks, five effective production
controls, and 110 restored checks. The strict source gate then passed 276 Rust
tests and 24 tooling tests. An independent unfiltered coverage recomputation
confirmed 2,987/3,047 lines, 4,393/4,565 regions and all 165 changed executable
lines covered. All 214 canonical evidence files and input hashes are retained.
See the [source and full-evidence review](reviews/topic-info-ready.json).

The protected normal commit, publication and exact hosted CI remain required at
this final metadata freeze. No additional upstream original is retired: the
retained inventory remains 87 complete and 19 partial cases. These bounded APIs
do not establish broader NET-01/NET-02 completion, network-wide discovery
completeness, physical delivery acknowledgment, latency equality or performance
parity. The simulator consumer pin and its full-clock assertions are unchanged.
