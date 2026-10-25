# Remapped local topic listing assertion

The reviewed partial port retains the complete selected
`NodeTest.TopicListRemap` assertion body. The pinned native test maps `/foo`
to `/bar`, advertises a publisher at `/foo`, then asserts the topic list has
one entry and index zero is `/bar`. Rust uses the existing single-hop
`Config::add_topic_remap`, `Node::advertise`, and `Node::advertised_topics` paths
with those exact names and both list assertions. The live `Publisher` handle is
kept until after inspection.

The unchanged native original passes 1/0 from upstream SHA
`41913b9dfc3dbce33ac5464ddcb427cc82450f26`, and the full gate now runs
it with the previously required Node originals. This selected one-node
observation does not qualify global discovery listing or cross-process
behavior. No production rule, dependency, API, or wire format changes.

Independent [review](reviews/topic-list-remap.json) approved this selected
one-node assertion mapping as partial. The ledger remains partial because the
Rust local list does not establish native global discovery behavior. Broader
NET-02 topic contracts remain open.
