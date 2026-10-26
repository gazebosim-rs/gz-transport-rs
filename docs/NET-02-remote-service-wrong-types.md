# Wrong service types across a process boundary

The pinned `twoProcSrvCall.SrvRequestWrongReq` and
`twoProcSrvCall.SrvRequestWrongRep` originals use a separately spawned native
Int32 echo service on `/foo`. The first sends Vector3d components 1, 2 and 3
while expecting Int32; the second sends Int32 data 5 while expecting Vector3d.
Both admit an asynchronous request, wait the original 300 ms and require the
response callback flag to remain false. A synchronous request with the same
mismatched types must return false with the original 1000 ms timeout.

The Rust ports run the existing real C++ `service-info-reference ordinary` echo
provider. Its fixed Int32 data-copy/true response matches the native replier.
Bounded READY and actual empty-filter service-type discovery precede observation;
explicit type filters are not used because that resolver path bypasses discovery.
The native child is checked alive before and after the two original observations.
No warm-up request changes the original wrong-first sequence.

An owned observer admits the actual asynchronous request and acknowledges
readiness before the 300 ms observation. The ordinary response checks Int32 data
5 and true result if invoked. Native wrongResponse ignores both arguments, so
any reply trips the wrong-response flag in that case. The synchronous raw call
must return exactly Error::Timeout. Unexpected errors fail, and only afterward
can explicit terminal cancellation begin. A join and supplemental final false
flag retain late queued replies. Native QUIT cleanup and failure-path kill/wait
keep child ownership bounded, including setup failures.

The owned asynchronous request has a ten-second bound. Native unbounded pending
lifetime, the original fixture's six-second provider lifetime, C++ callback APIs
and exact thread scheduling are not claimed equivalent. No production Rust or
C++ peer source changes. The native-only positive `twoProcSrvCall.SrvTwoProcs`
checks two data5/true callbacks, counts and handler cleanup in a separate fresh
process; it receives no Rust port credit from this packet.


The exact source gate passed 220 Rust tests, the mandatory native suite including
Node56/0 and ordinary service integration5/0, and strict 2645/2714 production
lines and 3893/4071 regions. No production or C++ peer source changed. Independent
[review](reviews/remote-service-wrong-types.json) replayed native wrong-type2/0,
a separate native-only positive1/0 and raw-source Rust2/0 against a freshly rebuilt
C++ peer. Each async-only valid Int32(5) control received actual data5/true and
failed the retained 300 ms false predicate. All 166 source hashes were restored
and Rust2/0 passed again. The valid-control build's unused-variable warning is
recorded separately; no failed setup or unrun work is passing evidence. The
fixed original predicates are faithful ports within the limits above. Final
metadata binding, normal protected hook and exact hosted qualification remain
separate requirements.
