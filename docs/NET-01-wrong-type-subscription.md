# Wrong-type subscription on the same Node

The pinned `NodeTest.PubSubWrongTypesOnSubscription` original advertises
Vector3d on `/foo` and successfully registers an Int32 subscriber. After the
original 100 ms settling interval, publishing Vector3d components 1, 2 and 3
succeeds and the callback flag remains false immediately after publication.

The Rust port uses the existing typed publisher and an owned subscription.
A user-owned receive thread records any delivered message; the fixed original
checks run immediately after `publish_typed` returns. A supplemental 300 ms
receive window must end with `Error::Timeout`; shutdown and other errors fail.
The joined receive thread must still leave the flag false.

The exact staged source gate passed the pinned native original, full Rust and
tooling suite, and strict production coverage: 2645/2714 lines and 3893/4071
regions. No production source changed. A superseded test accepted any receive
error as quiet; the frozen source requires exact Timeout. Its initial sandbox
socket denial was a setup failure before assertions and is excluded.

The independent [source review](reviews/pubsub-wrong-subscription.json)
replayed the pinned native original 1/0 and exact raw-export Rust original 1/0.
Changing only the subscriber descriptor to Vector3d delivered the publication
and failed the supplemental post-join false-flag assertion. The fixed original
observable predicates are a `faithful_port`; C++ callback registration and exact
thread scheduling remain outside the API mapping. Broader wrong-type cases and
the historical lost-output status 101 remain open and unclassified respectively.
The normal protected hook passed exact reviewer-bound tree `7776318b`. Public
`a0e48c985958580e243f894592896eed028ee7a3` passed exact hosted run
`40284202185`; this remains a fixed original observable port.
