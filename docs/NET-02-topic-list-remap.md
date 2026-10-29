# Remapped discovered topic listing

`original_topic_list_remap_discovered` preserves the complete assertion body of
upstream `NodeTest.TopicListRemap`: one Node maps `/foo` to `/bar`, advertises an
Int32 publisher at `/foo`, queries the topic list, and requires exactly one
entry with index zero equal to `/bar`.

The port calls `known_topics()`, the bounded discovered-topic query. It keeps
the publisher alive and adds no sleep or discovery polling. The earlier
`original_topic_list_remap` local `advertised_topics()` test remains unchanged;
its [partial review](reviews/topic-list-remap.json) remains historical evidence.

A separate supplement keeps a matching subscription alive after dropping the
publisher. The discovered topic remains `/bar`, while the publisher-only list
is empty. Dropping the subscription removes the topic; recreating and dropping
the remapped publisher updates it again. Service names remain separate. These
lifecycle checks receive no additional original-case credit.

Independent source comparison and replay approved the selected one-Node mapping
as a faithful port. This does not establish remote convergence, general query
latency, complete global-registry semantics or wider NET-02 behavior. No
production, protocol, dependency or gate-policy changes are involved.

Run the focused checks with:

```sh
cargo test --locked --test interop topic_list_remap
```

Every mandatory gate also runs the unchanged native `NodeTest.TopicListRemap`
original from the pinned C++ reference.
