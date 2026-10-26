# Mixed wrong and good response types for local services

The pinned `NodeTest.SrvTwoRequestsOneWrong` and
`NodeTest.SrvWithoutInputTwoRequestsOneWrong` originals register an Int32
response service on `/foo`. The first supplies Int32 data 5; the other has no
input. Each first fails a synchronous Vector3d-response request with the
original 1000 ms bound, admits the asynchronous wrong-response request, waits
300 ms and observes a false wrong-response callback flag. It then succeeds
with synchronous and asynchronous Int32 responses. The native response callback
checks data 5 and true result, then sets its flag; the original waits up to ten
seconds for that flag and asserts it.

The Rust port uses a live owned provider and a separate wrong-response observer.
A readiness barrier precedes the original sequence. Any reply sets the wrong
observer flag, matching native wrongResponse, which ignores both arguments. The
observer remains alive through both good-request observations. The synchronous
success also checks data 5 and true result as a supplement; the asynchronous
actual reply executes the original callback predicates and flag within a fresh
ten-second observation window. Only afterward does explicit terminal cancellation
begin; unexpected errors and early timeouts fail, both threads join, and a
supplemental final false flag catches late wrong replies.

Both owned asynchronous request deadlines are thirty seconds. The good callback
observation has its own ten-second wait after admission. Native unbounded pending
lifetime, C++ callback API identity and exact thread scheduling remain unclaimed.
No production or wire code changes. Source, independent review, final protected
hook and exact hosted qualification remain separate requirements.


The exact source gate passed 218 Rust tests, the mandatory native suite including
56 Node originals, and strict 2645/2714 production lines and 3893/4071 regions.
No production line changed. Independent [review](reviews/service-mixed-response.json)
replayed native 2/0 and raw-source Rust 2/0. Valid wrong-async Int32-response
controls received data 5 and true result, then failed both retained 300 ms false
predicates. Wrong good-async Vector3d-response controls failed both required
ten-second completions with Timeout. Their unchanged wrong observers also hit
the thirty-second ownership bounds during unwinding; those secondary failures
are disclosed and are not the sensitivity evidence. All 163 raw source hashes
were restored and Rust 2/0 passed again. The fixed observable originals are
faithful ports within the limits above. Final metadata binding, normal protected
hook and exact hosted qualification remain separate.
