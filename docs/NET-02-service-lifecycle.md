# Local service enumeration and explicit withdrawal

Next bounded NET-02 task: complete the local advertised-service enumeration and
withdrawal assertions retained in NodeTest.ServiceWithoutOutputCallAsync. Owned
files: runtime, existing test support/interop, this contract and ledger; no
remote service-query API, relay, shared-process Node runtime or full packet claim.

Pinned Node.cc799–846 enumerates this node's advertised qualified service topics
without partition prefixes, and UnadvertiseSrv validates the name, removes all
node handlers for that topic, then withdraws its discovery identity. Discovery.hh
658–687 treats withdrawal of a valid absent topic as success. Preserve those
semantics using explicit Rust errors for invalid names/shutdown and a bounded
local weak-lease registry. Drop remains atomic-only. Explicit withdrawal must
release registration slots exactly once, including when a still-held Service is
dropped later. Retired queued registrations cannot advertise after withdrawal;
old withdrawal must precede immediate replacement. Already admitted requests may
still be drained/responded; withdrawal stops future dispatch, not accepted work.

Required evidence: retain original /foo, Int32(5), invalid advertise/request and
counter1 checks; add original enumeration count/topic, invalid/valid withdrawal
and final empty enumeration. Cover valid absent withdrawal, namespace/partition
normalization, queue failure, dropped leases, capacity reuse without double
release, registration-command race and actual C++ requester after recreation.
Service duplicate-advertisement semantics remain pending independently; this
slice does not claim native failure-sideeffect parity for duplicate handlers.
Use strict full local/staged/hosted gates and independent assertion review.

Full local gate passes69 Rust tests and21 tooling tests,1277/1300 production
lines,1919/1985 regions and54/54 changed executable lines. Four lifecycle tests
check failed queue admission, held retired receivers, concurrent Drop/withdrawal,
exactly-once slot release, already admitted requests, poison errors and real UDP
withdraw-before-replacement ordering. Integration tests observe local
namespace/partition-aware unique listing and an actual C++ request/reply after
immediate service recreation. A native C++ probe verifies valid absent/repeated
withdrawal and relative-name list normalization. The unchanged native original
one-way case passes in the mandatory gate. The matching strict staged hook remains
required before publication.

Independent /root/sdformat review compared the native lifecycle source and the
complete original one-way assertion sequence. It approved bounded registry
ownership, exactly-once release, rollback, retirement order and admitted-request
draining, and independently replayed4 lifecycle units,4 integration cases
(including the actual C++ recreated-service requester) and the exact native
original1/0; all pass. The entire original one-way case is now a faithful port.
Duplicate service-advertisement/failure-sideeffect parity remains pending.
The matching strict staged hook is required before publication. Exact receipt:
[service lifecycle review](reviews/service-lifecycle.json).
