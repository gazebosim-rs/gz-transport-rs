# Local delivery with a remote subscriber

The pinned `twoProcPubSub.LocalSubscriberReceivesOnceWithRemoteSubscriber`
original creates a local and a remote `gz.msgs.Vector3d` subscriber on
`/local_and_remote`. Its helper reads actual subscriber metadata and waits for
at least two distinct process UUIDs, with a 10 second limit and 50 ms polling.
It then publishes ten messages whose x coordinates are 0 through 9. The local
callback must count ten within the original 5 second wait and 10 ms poll,
followed by a 300 ms quiet period without another local copy.

`original_local_subscriber_receives_once_with_remote_subscriber_uuid` uses
actual owned subscriber metadata and a real C++ remote subscriber. Its UUID set
must contain at least two identities and one outside all captured local worker
identities. This extra check proves the native child is represented; local
Rust worker identities cannot stand in for it. The selected fixture retains
the ten payloads, ordered receipts, waits and quiet check. The earlier
`original_local_subscriber_receives_once_with_remote_subscriber` descriptor
variant remains unchanged, with its historical [partial review](reviews/local-remote-once.json).

The Rust setup captures its local subscription's actual metadata before
starting the child. This is one immediate `known_topic_subscribers` call,
bounded by the unchanged five-second `Config::reply_timeout`, with no readiness
loop or startup wait. The native original starts its child first; neither case
publishes until local and remote subscriptions are present. This setup
translation supplies the complete known local identity set without changing
that asserted relationship. Each readiness iteration retains the existing
`known_topic_info` query to send `SUBSCRIBERS_REQ`, then reads actual
`known_topic_subscribers` records. It does not synthesize UUIDs, merge local
metadata or add a readiness API. Polling retains the original 10 seconds and
50 ms interval.

Neither original nor port asserts remote receipt. Rust wire identities remain
per worker; native identities are process-wide. This selected UUID predicate
and local-delivery fixture do not qualify general process identity, discovery
timing, full metadata ownership or delivery guarantees. The unchanged native
original and both Rust variants run in the strict staged gate.

Independent `/root/integration_reviewer` compared the complete native helper
and both bodies, passed all four old/new Rust variants from fresh source and
target with a freshly built C++ peer, and replayed the unchanged native pair
2/0. Only the two named remote-subscriber cases gain faithful-port credit after
the normal strict staged gate and exact hosted success, taking accepted counts
from 97 faithful / 9 partial to 99 / 7. A separate author wrong-topic-child
control makes both new readiness checks fail at the original deadline; that
negative result is author evidence, not an independent replay claim.
