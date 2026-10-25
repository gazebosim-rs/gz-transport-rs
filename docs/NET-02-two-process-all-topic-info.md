# Two-process all-topic information assertion port

Pinned `gz-transport` source `41913b9dfc3dbce33ac5464ddcb427cc82450f26`
retains `twoProcPubSub.AllTopicInfo` at
`test/integration/twoProcsPubSub.cc:487–526`. Its unchanged native executable
passes one test with no failures or skips. The original starts a Vector3d
publisher on `/foo` and a Vector3d subscriber on `/subscriber_only` in separate
processes, then subscribes locally to `/foo` with Int32. It waits up to five
seconds for each topic name. The result vector is searched by name: `/foo`
must have one publisher and one subscriber, and `/subscriber_only` must have
zero publishers and one subscriber. The original does not assert complete list
length, order or endpoint type names.

The Rust port retains two live C++ Vector3d peers, a local Int32 subscription,
the two five-second waits with 100ms polling, both name searches and all four
endpoint-count assertions. It calls existing `known_topics()` and then
`known_topic_info()` for each returned path, matching native `AllTopicInfo`'s
TopicList then TopicInfoHelper sequence. Each C++ peer exits only after an
explicit `QUIT` following the assertions. The pinned original joins the
mandatory native gate. Production API, runtime, protocol, dependencies and
coverage policy remain unchanged.

The ledger stays partial. Rust descriptor vectors omit native endpoint
addresses and options; process-global Node sharing, complete native result
vector contents and timing outside the original waits remain unqualified.
No exact descriptor-name assertion is attributed to this original.
The [independent review](reviews/two-process-all-topic-info.json) approved this
partial assertion port after replaying the pinned native original, both C++
peers and the selected and full Rust suites. The reviewed six-file exact staged
gate passed 155 Rust tests, 22 tooling checks, and strict coverage at
1814/1838 lines and 2751/2833 regions. Final staged verification, the normal
hook and hosted CI remain required before publication.
