# Local service lambda observations

The pinned `NodeTest.ServiceCallAsyncLambda` and
`NodeTest.ServiceCallWithoutInputAsyncLambda` originals advertise `/foo` with
provider lambdas. One sends `Int32(5)` and the other sends an Empty request.
Both provider lambdas return `Int32(5)` successfully. Each response lambda
checks data 5 and a true result, then sets `executed`; the test requires that
flag immediately after `Node::Request` returns.

The retained earlier Rust variants register a matching service and dispatch its actual
request through a scoped provider thread. A small request adapter passes the
actual decoded response and result to a closure before the adapter returns.
The tests check the flag after the adapter returns and an additional
exactly-once count. The one-second Rust wait is a safety bound; the native
originals do not assert a timeout.

The [source review](reviews/service-lambda.json) replayed both pinned native
originals and both Rust tests from an isolated source export. Removing the
response-closure call makes both adapter flag assertions fail. A
[superseding review](reviews/service-lambda-disposition-correction.json)
found that both ledger rows must remain partial because the original checks
the flag at the actual `Node::Request` return boundary.

New direct variants register the provider with `serve_callback` and submit the
request with `request_callback` on the same Node. They check `executed`
immediately after the actual request API returns, without an adapter, readiness
query or completion wait. Int32 input and output retain data 5 and the true
result; the inputless case uses canonical Empty and returns the same Int32.
Both service and response entry counters increment before parsing/assertions,
so caught Rust panics cannot hide invocation. Each counter must equal one.
Completion is checked only after the original immediate flag and counters.

The earlier adapter test bodies and all their assertions remain unchanged.
Their historical disposition correction remains valid for those adapters.
The new variants use the separately reviewed
[owned response callback API](NET-02-response-callbacks.md), whose exact local
selection runs both user functions before returning without a wire duplicate.
The original C++ bodies are untimed; the Rust API retains the one-second
operation bound already used by the earlier tests. This is an explicit bounded
Rust capability, with no latency or native indefinite-timeout claim.

Independent `/root` review compared the exact native bodies and passed fresh
four old/new Rust tests and the two unchanged native originals. Only these two
selected cases gain faithful-port credit after normal strict staged qualification
and exact hosted success. The new cases do not establish global discovery, C++ object identity,
remote immediate completion or general native scheduling equivalence. The
historical lost-output status 101 remains unclassified.
