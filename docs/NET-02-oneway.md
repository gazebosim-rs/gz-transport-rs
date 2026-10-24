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
gate. Its callback execution, invalid request name and successful local dispatch
assertions are partially represented by `oneway_local_handoff_bounds_and_matching`.
Full case equivalence remains pending: advertised-service enumeration and explicit
UnadvertiseSrv API assertions are retained and are not claimed as ported. Native
source and fixture hashes remain in the unchanged retained catalog.

Supplemental real C++ peers exercise both requester/provider directions, checking
exact canonical StringMsg payloads. A direct ZeroMQ request verifies no response
command, delayed reply or connection is created and no reply frame arrives. Local
checks exceed pending capacity across sequential handoffs, fill the service queue,
verify exact overload counters, wrong-type timeout, invalid name, payload limit,
and shutdown. No legacy async callback/CLI API parity is claimed.
