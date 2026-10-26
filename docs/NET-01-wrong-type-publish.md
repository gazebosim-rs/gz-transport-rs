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
observed before the receive thread starts and before either publish.

This port covers the fixed same-Node data, admission and callback assertions.
C++ callback registration and exact thread scheduling are outside its API
mapping. Other wrong-type subscription combinations have separate retained
originals. The historical lost-output status 101 remains unclassified.

The [independent review](reviews/pubsub-wrong-type-publish.json) replayed the
pinned native original 1/0 and exact raw-export Rust original 1/0. Removing
the typed publication guard made the wrong-type `InvalidFrame` assertion fail
with `Ok(())`. The reviewer approved `faithful_port` for the fixed observable
original; the API and scheduling limits above remain explicit.


A later [ordering review](reviews/pubsub-wrong-type-publish-deadline.json)
found that the first port started its receive timeout before connection
readiness. The scoped correction moves only that readiness wait ahead of the
existing receive thread. Both ten-second bounds, payloads, publication outcomes,
callback data/flags and license notices are retained; connection readiness no
longer consumes the receive budget. It changes no production API or wire code.

The exact one-file source gate passed 213 Rust tests, the mandatory native suite
including 51 Node originals, and strict 2645/2714 production lines and 3893/4071
regions. Independent exact raw-source Rust and pinned native replays passed 1/0
each. A private scaled ordering control failed for the old ordering and passed
for the fixed order; it demonstrates wait placement, not scheduler timing or
full timeout-window equivalence. The earlier fixed-original passing history is
retained. Final normal protected hook and exact hosted qualification remain
separate for this correction.
