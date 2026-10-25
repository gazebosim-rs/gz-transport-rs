# Retained multiple-call service assertions

Bounded test-only slice: NodeTest.MultipleServiceCallAsync,
MultipleServiceCallWithoutInputAsync and MultipleServiceWithoutOutputCallAsync,
pinned Node_TEST.cc1736–1872 and helpers146–185. Owned changes are the existing
service assertion test, mandatory original filter, ledger and this contract.
No production code, coverage denominator or numeric tolerance changes.

Retain /foo, Int32 data5 or canonical Empty input, invalid advertise/request,
actual execution and response result/data, counter1 then independently reset
counter3, invalid withdrawal and successful valid withdrawal. Rust polling and
scoped request threads replace callback syntax. Response-bearing calls launch
all requests before draining and count actual completed responses separately
from actual dispatched requests. One-way calls count actual queued events and
assert that no response is expected. No fabricated callback flags or readiness
sleeps. Each test uses an isolated partition.

The three unchanged native originals execute in the mandatory gate. Independent
assertion review and strict local/staged/hosted gates are required. This does not
claim C++ callback syntax, global Node sharing or full NET-01/NET-02 completion.

Full local gate passes76 Rust tests and21 tooling tests,1278/1300 production
lines and1920/1985 regions. Production is unchanged, with0 changed executable
lines. The three newly ported Rust cases pass under default parallel execution,
and the three unchanged native originals pass with exact XML3run/0fail. Independent /root/msgs review compared every original/helper assertion, verified
pinned source/native binary hashes, and independently reran all3 Rust ports under
default parallel execution and all3 exact native originals; all pass. It approved
the actual dispatch/completion counters and polling mapping. Exact receipt:
[service batch review](reviews/service-batches.json). The matching strict staged
hook remains required before publication.
