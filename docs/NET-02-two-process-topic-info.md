# Two-process topic information assertion port

Pinned `gz-transport` source `41913b9dfc3dbce33ac5464ddcb427cc82450f26`
retains `twoProcPubSub.TopicInfo` in
`test/integration/twoProcsPubSub.cc:456–482`. Its unchanged native executable
passes one test with no failures or skips. The original starts an auxiliary
`gz.msgs.Vector3d` publisher on `/foo`, waits up to ten seconds for discovery
with 100ms polling, rejects invalid `@` while leaving its publisher vector
empty, accepts absent `/bogus` with zero publishers, then finds exactly one
`/foo` publisher with descriptor `gz.msgs.Vector3d`.

The Rust port starts a real C++ publisher peer using the same partition and
native descriptor, waits for `/foo` in `known_publisher_topics()` with the
original deadline and polling interval, and checks invalid, absent and present
queries through `known_topic_info()`. The C++ peer remains alive through every
query and exits after an explicit `QUIT`. The pinned native original is added
to the mandatory gate. No production API, runtime, protocol or dependency
changes are made.

The Rust `Result` API does not expose a mutable output vector after an invalid
query, so that one original C++ state assertion has no direct equivalent.
The retained ledger row stays partial. Full native `MessagePublisher` metadata,
process-global Node sharing and query latency also remain unqualified.

The [independent review](reviews/two-process-topic-info.json) approved this
bounded assertion port. A private C++ peer with the wrong descriptor fails the
exact `gz.msgs.Vector3d` assertion. The reviewed six-file staged gate passed
154 Rust tests, 22 tooling checks, and strict coverage at 1814/1838 lines and
2750/2833 regions. Final staged verification, the normal commit hook, and
hosted CI remain required for the metadata-bound tree.
