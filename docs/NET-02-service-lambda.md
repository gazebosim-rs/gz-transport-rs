# Local service lambda observations

The pinned `NodeTest.ServiceCallAsyncLambda` and
`NodeTest.ServiceCallWithoutInputAsyncLambda` originals advertise `/foo` with
provider lambdas. One sends `Int32(5)` and the other sends an Empty request.
Both provider lambdas return `Int32(5)` successfully. Each response lambda
checks data 5 and a true result, then sets `executed`; the test requires that
flag immediately after `Node::Request` returns.

The owned Rust tests register a matching service and dispatch its actual
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

Native local `Node::Request` invokes its provider and response callbacks
inline. Rust `Node::request` returns before the adapter invokes the response
closure. The request payload, response data/result and one invocation remain
covered, but the immediate native post-request callback predicate is unported.
Public callback registration, exact scheduling/thread identity, remote
asynchronous callback behavior and broader service parity are also unclaimed.
The historical lost-output status 101 remains unclassified.
