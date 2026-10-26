# Wrong request type for a local service

The pinned `NodeTest.SrvRequestWrongReq` original advertises the Int32 echo
service on `/foo`. Its asynchronous request supplies Vector3d components 1, 2
and 3: admission succeeds, while the Int32 response callback flag remains false
after the original 300 ms interval. Its synchronous request uses the same wrong
request type and a 1000 ms timeout; the call returns false.

The Rust port registers the existing owned Int32 service and keeps an active
provider thread through both checkpoints. If invoked, that provider decodes and
checks Int32 data 5, then echoes it with a true result, preserving the native
provider predicates. A separate owned observer admits the actual `begin_request`
and signals readiness before the 300 ms observation. If it receives a response,
it checks Int32 data 5 and the true result before setting the flag. The raw
synchronous `Node::request` must return exactly `Error::Timeout`.

Both observers remain alive through the original observations. Explicit terminal
cancellation starts afterward; Shutdown is accepted only behind that stop flag.
Both threads are joined, and a supplemental false-flag assertion catches late
queued responses. Unexpected receive errors fail instead of counting as quiet.

The Rust asynchronous request has an explicit ten-second ownership bound and
is canceled after the fixed original checkpoints. Native asynchronous lifetime
without a supplied timeout, C++ callback registration and exact thread scheduling
remain outside this bounded mapping. No production API or wire code changes.


The exact frozen source gate passed 214 Rust tests, the mandatory native suite
including 52 Node originals, and strict 2645/2714 production lines and 3893/4071
regions. No production line changed. Independent [review](reviews/service-wrong-request.json)
replayed the pinned original and exact raw-source Rust original 1/0 each. A
private control changed only the asynchronous descriptor and payload to valid
Int32 data 5: provider and observer checked data/result, then the retained
300 ms false-flag assertion failed. Restoring the exact source passed again.
The fixed original observable predicates are a faithful port with the lifetime
and API limits above. The final normal protected hook and exact hosted run
`40299936911` passed on public `b869bc7c3fb88eac16b3dbb65e60b00d0079e678`,
reviewer-bound tree `84805107`.
