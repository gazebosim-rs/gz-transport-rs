# One-Node service list inspection and latency

Pinned `gz-transport` source `41913b9dfc3dbce33ac5464ddcb427cc82450f26`
retains `NodeTest.ServiceList` at `src/Node_TEST.cc:2739–2763`. The unchanged
native executable passes the selected original 1/0. It creates one Node,
advertises `/foo` with the Int32 echo handler, requires the first `ServiceList`
result length to equal one, clears the vector, then requires a second length
of one and integer-truncated elapsed milliseconds strictly less than two.
The original does not inspect an entry's name or type or invoke the handler.

The Rust assertion port keeps one live `Service` handle and queries
`known_services()` twice. The worker consumes the queued registration before
the first query. The test clears the first result and preserves both length
assertions and the second-call duration bound. `known_services()` is the
discovered-service snapshot API; `advertised_services()` only inspects this
Node's local handles. The unchanged native original remains in the required
gate.

This observation covers one Node's own registration and query latency. It
does not establish immediate sharing among multiple Nodes in one process,
remote convergence, global completeness, name or type contents, handler
delivery, or a general latency guarantee. The existing discovered-list
contract describes partition filtering, remote TTL and shutdown behavior.
No production, API, wire, dependency or coverage-policy change is proposed.

Independent [review](reviews/service-list-latency.json) approved the scoped
assertion mapping after native and private Rust replays. Its old-sleep mutant
failed the exact second-call bound at 2.089304 ms. The semantic-approved staged
gate passed 157 Rust tests, 22 tooling checks and strict coverage at 1816/1840
lines and 2756/2837 regions. Final staged verification, normal hook and hosted
CI remain pending.
