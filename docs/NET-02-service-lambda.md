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
The tests check the original post-call flag and an additional exactly-once
count. The one-second Rust wait is a safety bound; the native originals do
not assert a timeout.

The [independent review](reviews/service-lambda.json) replayed both pinned
native originals and both Rust tests from an isolated source export. Removing
the response-closure call makes both post-call flag assertions fail. The two
ledger rows are faithful ports of those fixed observable assertions.

Native local `Node::Request` invokes its provider and response callbacks
inline. The Rust adaptation preserves the observed result before its own call
returns through a scoped service thread and blocking request. It does not
claim a public callback-registration API, exact native scheduling or thread
identity, remote asynchronous callback behavior, or broader service parity.
The historical lost-output status 101 remains unclassified.
