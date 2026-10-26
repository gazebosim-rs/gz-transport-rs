# Two subscribers with different message types

The pinned `NodeTest.PubSubWrongTypesTwoSubscribers` original uses two Nodes.
The first advertises Int32 on `/foo` and registers the matching Int32 callback;
the second registers a mismatching Vector3d callback. After a matching connection
is observed, Int32 data 5 publishes successfully. The matching callback receives
data 5 and sets its flag within the original ten-second callback wait; its flag
is true while the mismatching callback flag is false.

The Rust port uses the existing typed publisher and owned subscriptions. Its
connection wait has a separate ten-second bound. The matching receive dispatch
starts after connection readiness and the mismatching receive thread's readiness
barrier, preserving a fresh callback receive bound. The mismatching subscription
stays alive in an exact-Timeout polling loop through the matching callback
observation and a supplemental 300 ms quiet interval. Any unexpected error fails.
The final joined mismatch flag must still be false.

Independent static review rejected a superseded first test: its matching receive
deadline began before the connection wait, and its mismatching subscription could
drop after 300 ms before the matching callback. That frozen source and its passing
gate remain retained as unapproved evidence. The corrected exact source gate passed 213 Rust tests, the mandatory native
suite including 51 Node originals, and strict production coverage: 2645/2714
lines and 3893/4071 regions. No production API or wire code changes.

Independent [review](reviews/pubsub-wrong-two-subscribers.json) replayed the
pinned native original and selected raw-export Rust original 1/0 each. Changing
only the second subscriber descriptor to Int32 delivered the message and failed
the bad callback flag assertion. The fixed original observable predicates are a
`faithful_port`. The normal protected hook passed exact reviewer-bound tree
`faeae393`; public `02cbb6836b74d4c5ffd9d212952bdf8b5ef65cd8` passed exact
hosted run `40287897483`.

C++ callback registration and exact thread scheduling remain outside the owned
receive API mapping. Broader wrong-type cases and the historical lost-output
status 101 remain open and unclassified respectively.
