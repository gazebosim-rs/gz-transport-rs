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
metadata binding, the mandatory normal protected hook and public
`747460f8516346746ad935a850c22b4adfb7f76f` passed exact hosted run
`40327340902` SUCCESS for those two ordinary cases.


## Inputless wrong-response original

The pinned 2016 `twoProcSrvCallWithoutInput.SrvRequestWrongRep` original uses
an Empty request and expects Vector3d from a separate inputless Int32 service.
Its retained assertions are asynchronous admission, a false callback flag after
300 ms and synchronous failure with the original 1000 ms timeout. The Rust case
uses Empty descriptor/empty bytes, requires exact Error::Timeout, and sets the
wrong-response flag on any reply because the native callback ignores both
arguments. The existing ordinary cases retain every observation.

The existing real C++ `service-info-reference inputless` provider returns Int32
data 5 and true result, matching the pinned native replier. READY, actual
empty-filter discovery of Empty/Int32, provider liveness, joined observer,
tail-only cancellation, late-false supplement and bounded QUIT cleanup retain
the same owned adaptation. Its ten-second observer and controlled provider
lifetime do not qualify native unbounded pending lifetime, the original
six-second provider lifetime, C++ callback APIs or precise scheduling.

The exact two-file source gate passed 221 Rust tests, 24 tooling tests and the
mandatory native suite, including Node 56/0, ordinary service 5/0 and inputless
service 4/0. Strict coverage stays 2645/2714 production lines and 3893/4071
regions; no production or C++ peer source changed. Complete 208-file source
reports were copied before metadata or normal-hook work. Independent
[review](reviews/remote-service-inputless-wrong-response.json) replayed fresh
native negative 1/0, a separate native positive 1/0, and raw-source Rust 3/0
against a freshly rebuilt C++ peer. Changing only the new asynchronous expected
reply to Int32 received actual data 5/true and failed the retained 300 ms false
checkpoint; the two old cases passed. All 168 source hashes were restored and
Rust 3/0 passed again. Initial reviewer log-filename and dependent missing-proof
setup failures stopped before export/build/native and are retained separately.

The separate native positive checks two data 5/true callbacks and counters 1;
it receives no Rust replacement or port credit. Only the fixed inputless
negative predicates are mapped. Final metadata binding, the mandatory normal
protected hook, publication and exact hosted candidate qualification remain
required; broader NET-01/NET-02 and performance remain open.
