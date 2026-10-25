# Retained synchronous service assertions

Bounded test-contract slice following local service lifecycle: faithful ports of
NodeTest.ServiceCallSync, ServiceCallWithoutInputSync, ServiceCallSyncTimeout and
ServiceCallWithoutInputSyncTimeout from pinned Node_TEST.cc1877–1990, including
their srvEcho/srvWithoutInput helper payload assertions. Owned changes are an
integration test, mandatory native filter, ledger and this contract. No production
code, protocol, coverage denominator or tolerance changes are needed.

Success ports retain /foo, Int32 data5, invalid-service rejection,1000ms deadline,
actual service execution, Boolean result true and exact response data. Empty input
is the canonical gz.msgs.Empty request descriptor and empty protobuf bytes. A
scoped request thread permits the Rust receiver owner to process the request;
completion is observed from the actual returned response, without fabricated
callback flags. This is the explicit Rust polling API mapping of the C++ helper.

Absent-service ports retain1000ms and the exact <=200ms absolute elapsed-time
difference assertion, then require Timeout. Partitions are independently unique
so parallel tests cannot satisfy another test's absent-service request. The
unchanged four native originals execute in the mandatory gate. This does not
claim callback-syntax parity or complete NET-01/NET-02 acceptance. Independent
source/assertion review and strict local/staged/hosted checks remain required.

Full local gate passes73 Rust tests and21 tooling tests,1278/1300 production
lines and1920/1985 regions. Production files are byte-identical to dff6ef5; this
slice changes no executable production lines. Native source/fixtures remain at
the pinned revision. Independent /root/msgs review compared every original and
helper assertion and independently reran all4 Rust ports under default parallel
execution and all4 exact native originals; all pass. It verified the source and
native binary hashes and approved the polling API mapping and unchanged timing
bound. Exact receipt: [synchronous service review](reviews/service-assertions.json).
The matching strict staged hook remains required before publication.
