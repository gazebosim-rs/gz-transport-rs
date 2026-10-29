# Subscriber-only discovered topics

`Node::known_topics() -> Result<Vec<String>, Error>` merges its bounded worker
snapshot with live same-process publisher and subscription interests. It returns
sorted, unique topic paths in the configured message domain and canonical
partition. Publisher-only and local handle inspection APIs remain separate.

The query sends `SUBSCRIBERS_REQ` through the existing nonblocking UDP path but
does not wait for remote replies. Remote subscriber-only names become visible
after accepted channel-0 `SUBSCRIBERS_REP` records. Their capacity-bounded registry
is separate from the subscriber connection map. Repeated replies refresh the
same entry; matching `END_CONNECTION`, process `BYE`, or the three-second expiry
removes it. Malformed names, wrong record kinds and foreign partitions remain
excluded. Remapping happens at registration, not at query time.

`original_int32_topic_list_subscriber_only` preserves the complete selected
`NodeTest.TopicListSubscriberOnly` assertion body: successful Int32 subscription
on `/foo`, one listed topic with index zero `/foo`, successful unsubscribe,
explicit list clear and an empty second query. It adds no sleep or polling.
The older StringMsg subscription variant remains unchanged.

Independent source comparison and replay approved this selected one-Node
mapping as a faithful port. This does not establish complete process-global
registry behavior, native initialization waits, general latency or immediate
remote convergence. Existing real C++ peer tests cover eventual remote interest
and withdrawal separately and earn no extra credit for this original.

Run the focused cases with:

```sh
cargo test --locked --test interop topic_list_subscriber_only
```

The mandatory gate runs the unchanged native original. The earlier
[runtime review](reviews/subscriber-lists.json) remains historical evidence for
the discovery implementation. This slice changes tests and documentation only;
it adds no runtime, dependency, protocol or coverage-policy changes.
