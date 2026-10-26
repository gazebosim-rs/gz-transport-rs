# Owned member callback observations

The pinned `NodeTest.ClassMemberCallbackMessage` and
`NodeTest.ClassMemberCallbackMessageInfo` originals create a class that owns a
Node. Each rejects `Bad Topic`, subscribes to `/foo`, waits 100 ms, rejects two
advertisements of `invalid topic`, constructs a valid Int32 Publisher, checks
its connection state, and publishes data 5. The native Publisher is local to
`SendSomeData()` and is destroyed before the test waits for its member callback.
The Info callback also checks `/foo`, the fully qualified partition,
`gz.msgs.Int32`, and `IntraProcess=true` on the same message.

The Rust tests use an owned client with a bounded subscription and a joined
receive thread that calls `MemberClient::cb` or `MemberClient::cb_info` with
the decoded Int32 and borrowed `MessageInfo`. A completion channel and atomic
call count observe the actual method invocation. Invalid names return
`InvalidName`; successful registration and Publisher construction represent
native true and `Valid`, and `has_connections()` checks the live matching
subscriber. The original-path Publisher drops at method return, before the
callback wait. The Info method checks every metadata field from its callback
argument, including the native leading slash on the partition. A separate
supplement keeps a Publisher after owner retirement, joins the dispatch
thread, publishes again and observes no later method call. Exactly one call
and a short quiet interval are supplemental assertions.

Both unchanged pinned native originals pass and are required by the strict
gate. The [independent source review](reviews/class-member-callback.json)
replayed all three Rust tests in a private raw export and showed that routing
the Info case to the plain method fails its complete metadata observation.
The two ledger rows are faithful ports of these originals' observable
assertions. This test-only adaptation adds no public callback registration
API and does not claim native callback scheduling or C++ object-pointer
identity. Other NET-02 behavior and the earlier lost-output status 101 remain
separate; that historical failure is unclassified.
