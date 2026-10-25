# NET-02 one-way service slice

The pinned C++ `NodeShared::RecvSrvRequest` and `SendPendingRemoteReqs` use the
response descriptor `gz.msgs.Empty` to suppress replies and remove pending
request handlers. Rust preserves the nine request frames and the same descriptor.
`Node::request_oneway(topic, request_type, bytes, timeout)` waits only for bounded
local queue or ZeroMQ handoff. Success does not acknowledge service execution.
Unknown providers, wrong request types, and failed connections expire at the
caller deadline. Queue saturation remains observable. `Request::expects_response`
is false for these requests; consuming one with `respond` sends nothing and
ignores unused response data. Ordinary response-bearing requests keep their
existing behavior. Empty request descriptors remain ordinary raw-byte inputs.

The original `NodeTest.ServiceWithoutOutputCallAsync` is executed by the mandatory
gate. The original /foo service, Int32 payload5, invalid advertise/request names and
single execution count1 are represented by
`original_oneway_int32_five_dispatches_once_and_rejects_invalid_service`. The Rust
API polls actual queued requests instead of registering a callback; each polled
event executes the original payload assertion and increments the observed count.
`oneway_local_handoff_bounds_and_matching` supplements those exact assertions
with queue saturation, type and lifecycle bounds.
The [local service lifecycle slice](NET-02-service-lifecycle.md) adds the remaining
advertised-service count/topic, invalid/valid withdrawal and final empty-list
assertions to that same port. Independent /root/sdformat review approved this
complete original assertion sequence and independently reran the Rust and native
case; both pass. Native source and fixture hashes remain in the unchanged retained
catalog.

Supplemental real C++ peers exercise both requester/provider directions, checking
exact canonical StringMsg payloads. A direct ZeroMQ request verifies no response
command, delayed reply or connection is created and no reply frame arrives. Local
checks exceed pending capacity across sequential handoffs, fill the service queue,
verify exact overload counters, wrong-type timeout, invalid name, payload limit,
and shutdown. No legacy async callback/CLI API parity is claimed.

Independent /root/msgs review compared the original /foo/Int32(5)/counter1 and
invalid-name assertions, reran the exact native original and supplemental Rust
test, and approved that partial mapping. /root/sdformat subsequently approved the
complete enumeration and explicit withdrawal additions. The first reviewer also approved
the complete400-row TopicUtils mapping after source comparison and exact native
and Rust reruns; no runtime behavior or production coverage policy changed.
