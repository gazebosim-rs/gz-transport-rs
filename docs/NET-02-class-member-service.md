# Owned member service observations

The pinned `NodeTest.ClassMemberCallbackService` and
`NodeTest.ClassMemberCallbackServiceWithoutInput` originals use a class-owned
Node to serve `/foo`. Both reject `Bad Srv`, then check a synchronous request,
Int32 data 5, a successful result and execution of the owned service method.
The first case also sends serialized Int32 bytes through `RequestRaw` and
checks the parsed reply. Their wrong-type synchronous requests return false;
wrong-type asynchronous requests are admitted, then leave the service method
uninvoked through the original 300 ms quiet interval.

The retained earlier Rust variants use owned `Client` methods and a joined service receive thread.
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

The new direct variants use `serve_callback` and `request_callback` with an
owned Client and its Node. They preserve the original immediate response flag
check after asynchronous request return, with no readiness or completion wait
before that assertion. Synchronous and raw results, Int32 data 5, invalid name,
wrong-type synchronous timeouts, asynchronous admission and the original 300 ms
quiet interval remain. The old waiter variants and all their assertions remain.
See the [bounded callback contract](NET-02-response-callbacks.md) for admission,
selection, cancellation, panic, reentrancy and thread lifetime rules.

Only these two selected local original cases gain faithful-port credit after
independent source/native replay and normal strict qualification. The new
callbacks do not establish C++ object-pointer identity, a global metadata merge,
remote immediate completion or general callback scheduling equivalence. Other
NET-02 scope and the earlier unclassified lost-output status 101 remain separate.

## Request before local advertisement

`NodeTest.ClassMemberRequestServiceBeforeAdvertise` submits Int32 data 5 on
`/foo` before its owning Node advertises the matching Int32 service. The
worker keeps the admitted request's ID and deadline while no provider matches.
After local registration, it queues the request once to the exact typed
service. An owned provider method checks data 5 and returns it; a separate
owned waiter invokes the response method with data 5 and a true result. The
test observes that method's flag within the original 10-second bound, then
joins both dispatch threads before owner destruction. The native original
passes 1/0 and runs in every strict gate.

The [independent review](reviews/class-member-before-advertise.json) replayed
the pinned native original and Rust mapping from a raw parent export. The
unchanged parent and a separate late-local-retry removal both time out at the
original bound. Worker controls require typed matching, one admission,
one-way completion, bounded provider queue failure and no second delivery
after a request was already sent. The exact staged source gate passed with
60/60 changed executable lines covered.

This third ledger row is a faithful port of the original's observable
request-before-advertise predicates. The native C++ callback registration API,
object-pointer identity and scheduling are outside this Rust API claim. Remote
provider races and broader NET-02 service behavior remain open. The earlier
lost-output status 101 remains unclassified.
