# Owned member service observations

The pinned `NodeTest.ClassMemberCallbackService` and
`NodeTest.ClassMemberCallbackServiceWithoutInput` originals use a class-owned
Node to serve `/foo`. Both reject `Bad Srv`, then check a synchronous request,
Int32 data 5, a successful result and execution of the owned service method.
The first case also sends serialized Int32 bytes through `RequestRaw` and
checks the parsed reply. Their wrong-type synchronous requests return false;
wrong-type asynchronous requests are admitted, then leave the service method
uninvoked through the original 300 ms quiet interval.

The Rust tests use owned `Client` methods and a joined service receive thread.
An Int32 or Empty request reaches the matching method, which responds with
Int32 data 5. `Result::Err(InvalidName)` maps invalid registration;
`Error::Timeout` maps the wrong-type synchronous false result. A successful
`begin_request()` maps asynchronous admission. Its one-shot `PendingResponse`
is consumed by a separate scoped waiter, which calls the owned response
method with the decoded Int32 and result. The test thread observes that
method's flag through a bounded completion channel. The service and response
threads join before their owner is destroyed. A supplement drops one pending
handle, responds to two uniquely correlated requests, and checks that the
second handle receives only its own reply. Dropping a handle abandons its
reply; the worker retires the bounded pending entry on reply or at its
original deadline.

The native originals pass 2/0 and run in the strict staged gate. The
[independent review](reviews/class-member-service.json) privately replayed
both Rust original mappings and the abandoned-waiter control, plus the pinned
native cases. Removing the provider response-type check makes the original
wrong-type assertion fail. The exact staged source gate passed with 17/17
changed production lines covered.

**Both ledger rows remain partial.** Native C++ immediately checks its member
response flag after asynchronous `Node::Request` returns; Rust waits up to the
500 ms duration used by the original synchronous calls for the owned waiter
before checking it. This does not
establish native immediate callback visibility, callback registration API,
object-pointer identity or scheduling. Other NET-02 behavior and the earlier
lost-output status 101 remain separate; that historical failure is
unclassified.
