# Subscriber-only discovered topic snapshot

`Node::known_topics() -> Result<Vec<String>, Error>` merges a bounded worker
snapshot with accepted live same-process publishers: union active local publishers, already-discovered remote message
publishers, active local subscription interests, and recently received remote
`SUBSCRIBERS_REP` interests. It returns sorted/deduplicated `/topic` paths whose
full wire names pass the same canonical partition comparison as the published
`known_publisher_topics()` method. The existing publisher-only and local
`advertised_topics()`/`subscribed_topics()` APIs keep their exact behavior.

`known_topics()` broadcasts `SUBSCRIBERS_REQ` through the existing nonblocking
UDP announce path before taking its snapshot, as native TopicList does, but
does not wait for replies. Repeated bounded polling
may observe eventual remote subscriber interests. Use a separate
capacity-bounded remote-interest registry; do not put replies into the existing
`remote_subscribers` connection map or change `Publisher::has_connections()`.
Only channel-0 `SUBSCRIBERS_REP` records with `MsgPub` payloads enter it. A repeated reply refreshes its timestamp without spending another capacity
slot; `END_CONNECTION` matches process/node/topic identity, while process `BYE`
or the existing three-second expiry removes it. Snapshot checks stamp age again.
Malformed wire names, wrong kind, foreign partition and missing leading `/`
remain excluded; registration/subscription remapping occurs before the record
is sent, never on a list query.

Local-only subscriber topics appear immediately through ordered worker commands
and disappear after dropping the last subscription or explicit unsubscribe.
Remote subscriber-only topics are eventually visible after a reply, including
across processes. The later
[two-Node publisher listing](NET-02-two-node-topic-list.md) qualifies one
immediate same-process publisher case and its second-call <2ms assertion;
subscriber-only process-global sharing remains unqualified. A native C++
subscriber-only peer verifies eventual remote visibility and withdrawal. The exact
NodeTest.TopicListSubscriberOnly original passes 1/0 and
its one-node size, indexed name, and post-unsubscribe assertions are retained in
Rust. The existing two-process original also passes 1/0. The selected Node
ledger row remains partial after independent review; global initialization
and latency remain open.

Three exact staged replays pass with at least 1759/1777 production lines,
2663/2735 regions and all 71 changed executable lines. A private mutation
disabling only `SUBSCRIBERS_REP` admission fails the real C++ remote-interest
test: the local `/local-only` path remains while `/remote-only` never appears.
Independent source/API/assertion review passed with the
[independent receipt](reviews/subscriber-lists.json). The normal commit hook and
hosted CI remain pending.
