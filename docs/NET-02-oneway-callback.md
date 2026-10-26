# Local one-way service callback

The pinned `NodeTest.ServiceCallWithoutOutputAsyncLambda` original advertises
`/foo` with an Int32 callback, requests Int32 data 5, and checks that the
callback has set `executed` immediately after `Node::Request` returns. The
native original passes 1/0. The Rust test calls raw `Node::request_oneway`,
then checks the same data and flag at that return boundary. A held-callback
control proves the request cannot return before callback completion.

`Node::serve_oneway_callback` owns the service lease and callback dispatcher.
A matching request on the same Node invokes the callback on the requesting
thread after releasing the callback registry lock. It returns only after that
invocation completes. Other-Node requests use the existing queued service
route and a dedicated callback thread; their admission does not acknowledge
callback completion. The direct branch requires the remapped qualified topic,
request descriptor, canonical Empty response descriptor, active lease and live
Node. Unmatched requests keep the existing bounded queue path.

The caller's timeout bounds queued discovery and admission. A synchronous
same-Node callback may run longer than that timeout; the native one-way API
has no timeout argument. Zero and overflowing timeouts retain their typed
errors. Callback errors increment routing-error accounting; panics are caught
and returned as a typed native error on the direct path. Dropping the lease
retires matching for subsequent requests and joins its queued dispatcher. A
direct request already in flight may still invoke or finish its callback after
Drop. User callbacks run without a registry lock;
they must not recursively wait on their own service.

The fixed original checks only the local callback value and post-request
flag. Supplemental controls cover one invocation, held callback completion,
wrong descriptors, remapping and reentrancy, other-Node dispatch/error,
Drop, shutdown and panic. C++ callback registration, exact thread identity,
process-global remote behavior, and a general scheduling guarantee remain
outside this slice. The historical lost-output status 101 remains
unclassified.

The [independent source review](reviews/service-oneway-callback.json)
replayed the pinned native original and the Rust original from an exact raw
export. All seven focused Rust controls passed. Removing only the direct
callback invocation makes the original Rust test fail at its unchanged
post-request `executed` assertion. The reviewer approved `faithful_port` for
that fixed observable original; the broader limits above remain explicit.
