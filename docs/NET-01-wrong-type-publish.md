# Wrong-type publication with a local subscriber

The pinned `NodeTest.PubSubWrongTypesOnPublish` original advertises Int32 on
`/foo`, subscribes on the same Node, and waits for a connection. Publishing a
Vector3d with components 1, 2 and 3 returns false, with the callback flag
still false immediately afterward. Publishing Int32 data 5 returns true; the
callback receives data 5 and sets its flag within the original ten-second
wait.

The Rust test uses `advertise_typed::<Int32>` and an owned subscription.
`publish_typed(&Vector3d)` returns `InvalidFrame` before queue admission,
while `publish_typed(&Int32)` succeeds. A user-owned receive thread decodes
the actual Int32 bytes and sets the flag. The test checks that the wrong
publication leaves the flag false and that the right publication completes
the callback within ten seconds. The publisher connection predicate is
observed before either publish.

This port covers the fixed same-Node data, admission and callback assertions.
C++ callback registration and exact thread scheduling are outside its API
mapping. Other wrong-type subscription combinations have separate retained
originals. The historical lost-output status 101 remains unclassified.

The [independent review](reviews/pubsub-wrong-type-publish.json) replayed the
pinned native original 1/0 and exact raw-export Rust original 1/0. Removing
the typed publication guard made the wrong-type `InvalidFrame` assertion fail
with `Ok(())`. The reviewer approved `faithful_port` for the fixed observable
original; the API and scheduling limits above remain explicit.
