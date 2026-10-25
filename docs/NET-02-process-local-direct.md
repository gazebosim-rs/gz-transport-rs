# Same-process direct message delivery

The pinned `NodeTest.PubSubOneThreadTwoSubs` original creates two Nodes, one
publisher and two Int32 subscriptions on `/foo`. Its only readiness check is
`HasConnections()`; the first data-5 publication reaches both callbacks. The
Rust assertion port keeps that first publication, both receipts, the first
Node's one-item subscription inventory, invalid and valid unsubscribe results,
the original 500 ms withdrawal interval, the second Node's second receipt,
the first Node's retirement, and empty subscription and publisher inventories.
It adds no warmup publication, second-Node discovery barrier or delivery retry.
The earlier test-only attempt timed out on the first second-Node receipt even
after a two-descriptor metadata barrier; its failure is retained in ignored
review artifacts.

Each successfully admitted subscription has a process-local routing record.
The record identifies its canonical message domain, qualified topic, Node and
descriptor, and holds a bounded queue sender. Active, worker-stop and Node-owner
flags are weakly referenced. Admission acquires fallible registry locks before
enqueueing the worker command, so a poisoned registry returns a typed error
without leaving an admitted subscription. After a successful publish command,
the publisher worker attempts the ordinary ZeroMQ send and directly routes one
copy to matching registrations on other live Nodes in the same domain. Its own
Node retains its existing direct path, including each registration's
`ignore_local_messages` option. The other Node receives even when the
publisher's own registration opts out. Matching includes wildcard subscribers;
partition, multicast address, interface and message port isolate explicit
domains. Each Node keeps its existing discovery UUID and service worker.

The publisher worker records a bounded, three-second fingerprint of its own
payloads. On receiving a matching wire echo, a subscriber worker suppresses
the copy only when domain, publisher address, topic, descriptor, payload size
and payload fingerprint match a recent local publication. A reused address
with a different payload remains deliverable, and expired records do not mask
later traffic. Because the four-frame wire format has no origin UUID, an
external process reusing the same address and sending identical bytes during
that short window cannot be distinguished. This bounded limit does not change
the wire protocol. Ordinary external C++/Rust publications continue to report
`intra_process=false`; same-process direct copies report true. The previously
reviewed two-Node metadata differential keeps its inputs and now requires the
native true value from Rust.

The direct queue remains bounded by `Config::capacity`; `queue_drops` records
full or disconnected receiver sends. If the process-local registry is poisoned,
caller-side subscription admission fails before enqueue. A publisher worker
increments `Statistics::routing_errors`, still attempts the ordinary wire
publication, and does not count a direct delivery. The resulting degraded
path does not guarantee one copy. Worker cleanup releases retired routing
senders at the existing subscription-withdrawal boundary, preserving already
queued receiver draining and shutdown behavior. Explicit Node shutdown still
cancels all its handles; an ordinary Node drop retires its subscriptions even
when a Publisher handle keeps its worker alive.

The source packet also tests first delivery without an extra readiness step,
per-Node ignore-local behavior, wildcard and typed registrations, domain
isolation, Node-owner retirement, address reuse, and an isolated
poisoned-registry child process. A separate test lets the second Node's wire
subscription connect, publishes once, requires the direct-origin receipt, and
rejects a duplicate wire echo over 500 ms. Disabling only echo suppression
fails that assertion. These are assertion and error boundaries, not throughput
or delivery guarantees. Native process-wide discovery UUIDs, callback
scheduling, adversarial address/identical-payload reuse, and other retained
NET-02 cases remain open. The exact staged source gate passed 185 Rust tests
with 2395/2448 production lines, 3566/3717 regions and 132/132 changed
executable lines. [Independent review](reviews/process-local-direct.json)
approved this bounded mapping. Final metadata binding, normal hook and hosted
CI remain required before publication.
