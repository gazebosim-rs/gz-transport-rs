# Bounded topic information descriptor snapshot

`Node::known_topic_info(topic)` returns sorted vectors of publisher and
subscriber descriptor names from one bounded worker snapshot. It qualifies
the original query with the Node's partition and namespace. Native
`Node::TopicInfoHelper` does not remap the query; registration and subscription
still apply the configured remap before discovery. A valid absent topic has
two empty vectors. Invalid names and worker shutdown return the existing
errors. A message-channel `SUBSCRIBERS_REQ` is sent without waiting for its
replies, so a remote subscriber can appear on a later query.

Active local and fresh discovered channel-0 message publishers supply publisher
types. Active local subscriptions and fresh `SUBSCRIBERS_REP` records supply
subscriber types. Remote records retain process, node, topic, address and type
identity, with a three-second freshness check at snapshot. Repeated identical
replies refresh age within the bounded interest cache. `END_CONNECTION` removes
all type variants of the matching process/node/topic; BYE and expiry also
withdraw remote interests. Wrong channel/kind, malformed or foreign-partition
topics, and scope-rejected records do not enter this snapshot. This interest
cache remains separate from publisher connection membership.

The vectors preserve one entry per distinct known endpoint identity, even
when type names match. Two subscriptions of the same topic and type on one
Rust Node collapse to one entry, as native `TopicInfoHelper` collapses equal
`MessagePublisher` records. Distinct type names remain distinct; different
remote node identities can produce duplicate type names. The vectors omit
native address/control/options and process-global shared-node metadata.

The unchanged pinned `NodeTest.TopicInfoSubscriberOnly` passes natively
one test, zero failures and no skips. Its Rust port retains successful query,
zero publishers, subscriber count one and the exact `gz.msgs.Int32` descriptor.
A separate native three-handler probe observes two native records for two
same-node `Int32` handlers plus one `StringMsg` handler, confirming identical
endpoint collapse. Extra Rust checks cover `/foo` to `/bar` remapping with an
unmapped `/foo` query, local duplicates, two remote node identities and type
variants, age and END removal, a real C++ remote subscriber and publisher, and
post-shutdown errors. Original full `TopicInfo`, `AllTopicInfo`, immediate
remote discovery, process-global visibility and latency remain unqualified.

The exact nine-file staged gate passes 153 Rust checks and 22 tooling checks,
with 1814/1838 production lines, 2749/2833 regions and 61/61 changed
executable lines covered. Independent source/API/assertion review approved
this bounded mapping after a private original replay, real C++ endpoint
checks, a worker identity/withdrawal test and a local-subscriber removal
control that fails the original assertion. The [review receipt](reviews/topic-info.json)
binds the nine source inputs and pinned native original. The ledger row remains
partial. Final metadata verification, normal commit hook and hosted CI remain
pending.
