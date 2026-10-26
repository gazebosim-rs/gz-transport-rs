# Wrong response types for local services

The pinned `NodeTest.SrvRequestWrongRep` and
`NodeTest.SrvWithoutInputRequestWrongRep` originals advertise an Int32 response
service on `/foo`. The first sends Int32 data 5; the second has no input. Both
admit an asynchronous request expecting Vector3d, retain a false wrong-response
callback flag after 300 ms, then require the synchronous request with a 1000 ms
timeout to return false.

The Rust tests share the accepted wrong-request harness through three exact case
variants. The ordinary provider retains its decoded Int32 data 5 check and echo;
the inputless provider accepts an empty request and replies with Int32 data 5 and
a true result. Each wrong-response observer sets its flag on any received reply,
matching native `wrongResponse`, which ignores both payload and result. Raw
synchronous requests must return exactly `Error::Timeout`.

Provider and response observer readiness precede the first observation. Both
remain active through both original checkpoints. Explicit terminal cancellation
starts afterward; Shutdown is accepted only behind the stop flag. Both threads
are joined, and a supplemental final false flag detects late queued replies.
The existing wrong-request values, flags, exact Timeout and echo predicates are
preserved in the shared harness.

The asynchronous owned request has a ten-second bound and is canceled after the
fixed observations. Native unbounded asynchronous lifetime, C++ callback API
identity and exact thread scheduling remain outside this mapping. No production
API or wire code changed. The exact source gate passed 216 Rust tests and the mandatory native suite,
including 54 Node originals, with 2645/2714 production lines and 3893/4071
regions. No production line changed. Independent [review](reviews/service-wrong-response.json)
replayed native 2/0 and raw-source Rust 3/0. Each private control changed only
the asynchronous expected response descriptor to Int32 and added control traces:
provider and observer received Int32 data 5 with a true result, then the original
300 ms false-flag assertion failed. All raw source hashes were restored, and
Rust 3/0 passed again. The fixed observable originals are faithful ports with
the lifetime and API limits above. Final normal protected hook and exact hosted
qualification remain separate.
