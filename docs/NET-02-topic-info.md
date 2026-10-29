# Bounded topic information descriptor snapshot

`Node::known_topic_info(topic)` returns sorted vectors of publisher and
subscriber descriptor names from a bounded worker snapshot merged with live
same-process endpoint records. It qualifies
the original query with the Node's partition and namespace. Native
`Node::TopicInfoHelper` does not remap the query; registration and subscription
still apply the configured remap before discovery. A valid absent topic has
two empty vectors. Invalid names and worker shutdown return the existing
errors. A message-channel `SUBSCRIBERS_REQ` is sent without waiting for its
replies, so a remote subscriber can appear on a later query.

`Node::topic_info(topic, timeout)` uses the same endpoint projection after
waiting for discovery startup. Native `Node::TopicInfo` first triggers discovery
`TopicList`, then calls `TopicInfoHelper`; this is the corresponding bounded
query path. Its explicit timeout covers startup and worker reply waits. Startup
is per Rust worker, not native process-global state, and is not remote consensus.

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

## Selected subscriber-only original

`original_topic_info_subscriber_only_ready` keeps the native Int32 subscription
on `/foo`, successful info lookup, zero publishers, one subscriber and exact
`gz.msgs.Int32` type name through `topic_info`. Its five-second deadline bounds
the query; the original has no timing assertion. The older immediate
`known_topic_info` snapshot test remains unchanged.

Independent native-source comparison and replay approved the selected one-Node
mapping as a faithful port. Existing controls for remap lookup, equal endpoint
collapse, remote identities and types, withdrawal, expiry and shutdown remain.
The earlier [runtime review](reviews/topic-info.json) is historical evidence for
the snapshot implementation.

The vectors still omit the full native `MessagePublisher` address/control/options
facade. Complete process-global initialization and ownership, immediate remote
convergence and general latency remain unqualified. No production, dependency,
protocol or gate-policy change is part of this selected test mapping.

Run both subscriber-only variants with:

```sh
cargo test --locked --test interop original_topic_info_subscriber_only
```

Every mandatory gate also runs the unchanged native
`NodeTest.TopicInfoSubscriberOnly` original from the pinned reference.
