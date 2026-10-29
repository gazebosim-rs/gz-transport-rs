<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Four-node all-topic information mapping

The pinned `NodeTest.AllTopicInfo` creates four Nodes in one process. Two
advertise Int32 on `/topic1` and `/topic2`; node2 subscribes to `/topic1` and
node3 to `/topic3`. Node4 requires a successful query with exactly three named
records and publisher/subscriber counts `(1,1)`, `(1,0)` and `(0,1)` respectively.
No callback payload or timing is asserted.

Native `Node::AllTopicInfo` first calls `TopicList`, whose discovery path sends
`SUBSCRIBERS_REQ` and waits for initialization before enumerating names. It then
calls `TopicInfoHelper` separately for each name. Each helper takes its own
shared-state lock. The returned vector is not an atomic global snapshot; the
older review's atomic-vector rationale was incorrect.

The existing `original_four_node_all_topic_info` snapshot test remains unchanged.
The new `original_four_node_all_topic_info_ready` keeps the same four-node
fixture and typed Int32 registrations. A `topic_info` query on node4 for the
original `/topic1` models native's internal startup wait. It then enumerates
actual `known_topics()` names and collects each actual `topic_info` result.
Every query must succeed; failed queries are not omitted. The vector's count,
name-presence and endpoint-count assertions match the original. No sleeps,
expected-name polling or hardcoded result records are introduced.

This is a caller-side composition of accepted bounded APIs, not a new public
AllTopicInfo facade. Each ready query has a five-second bound; the native
original is untimed. The composition can send additional discovery requests
and does not claim equal traffic, latency, dynamic-snapshot behavior or complete
process-global initialization. Endpoint vectors still expose descriptor names,
not full native MessagePublisher address/control/options metadata.

All fixture Nodes and registration handles stay alive through inspection.
Existing endpoint identity, duplicate collapse, retirement, domain, remap and
shutdown tests remain unchanged. Ordinary Publisher survival after Node drop
is covered by the separate [ownership contract](NET-02-publisher-after-node-drop.md);
this mapping does not establish every native process-global ownership or
callback-lifetime relationship. Remote convergence and delivery readiness remain
separate contracts.

The earlier [runtime review](reviews/four-node-topic-info.json) remains historical
evidence for the process-local indexes. Its original snapshot test and both
supplements are retained; this slice changes tests and documentation only.
Independent source comparison and replay approved the selected four-node
mapping as a faithful port. The broader limits above remain unchanged.

Run the focused variants with:

```sh
cargo test --locked --test interop original_four_node_all_topic_info
```

Every mandatory gate also runs the unchanged native `NodeTest.AllTopicInfo`
original from the pinned reference.
