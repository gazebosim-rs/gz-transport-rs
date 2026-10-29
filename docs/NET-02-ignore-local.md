# Ignore local messages per subscription

`SubscribeOptions` defaults to local delivery. A caller can set
`ignore_local_messages` and pass the option to `Node::subscribe_with_options`.
The existing `subscribe` API uses the default. The filter applies only to a
matching registration's direct delivery from its owning Node; other
registrations on that Node retain their own policy. The ignored registration
still participates in subscriber discovery and publisher interest.

The pinned `NodeTest.IgnoreLocalMessages` original uses `/foo`, `gz.msgs.Int32`
with data 5, three publications separated by 100 ms, and a zero-count local
callback after opt-in. The Rust port keeps those observations and checks the
empty receiver. It is a faithful mapping of this one Node case.

The pinned `twoProcPubSub.IgnoreLocalMessagesWithRemoteSubscriber` original
uses a separate subscriber process, a publishing and ignored Node, and a
second local witness Node. The new
`original_ignore_local_messages_with_remote_subscriber_uuid` retains ten
default Vector3d publications, ten witness deliveries followed by a 300 ms
quiet interval, and zero ignored deliveries. A real C++ subscriber participates.
Readiness now reads actual subscriber process UUIDs: the native at-least-two
predicate must hold, and an observed identity must be outside both captured
local worker identities. Two local Rust identities alone cannot masquerade as
the remote child. The old descriptor-based
`original_ignore_local_messages_with_remote_subscriber` remains unchanged.

The new setup captures each Node's actual subscriber metadata once before
starting the child, with no polling or startup wait. Each call has the unchanged
five-second `Config::reply_timeout` bound. Native starts its child first; both
fixtures require all subscriptions before publication. This explicit setup
translation establishes the full local identity set. Readiness retains the
original 10 second limit and 50 ms interval; each iteration keeps the existing
`known_topic_info` subscriber-request side effect, then queries owned subscriber
records. No UUID is synthesized or normalized, and no metadata view changes.

The C++ child callback is empty and its process wrapper only joins, so neither
case asserts remote receipt. The accepted
[direct-delivery slice](NET-02-process-local-direct.md) supplies same-process
message origin. Actual UUID readiness does not establish native process-global
identity equality: Rust workers still advertise separate UUIDs. Full global
ownership, general discovery latency and arbitrary delivery guarantees remain
open. Independent `/root/integration_reviewer` compared the complete helper
and bodies, passed all four old/new Rust variants from fresh source/target
with a freshly built C++ peer, and independently replayed the native pair 2/0.
Only the two selected remote-subscriber cases gain faithful-port credit after
normal strict staged qualification and exact hosted success. The author
wrong-topic-child control fails both new readiness checks; it remains separate
from that independent evidence.

Both pinned originals pass separately and the exact Rust assertions run in
the strict staged gate. The source review and private filter-removal control
are recorded in the [independent receipt](reviews/ignore-local.json). The
remaining NET-02 assertions and protocol cases remain open.
