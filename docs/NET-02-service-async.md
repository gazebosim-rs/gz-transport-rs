# Ordinary and inputless asynchronous service assertions

Bounded test-only NET-02 slice: NodeTest.ServiceCallAsync and
ServiceCallWithoutInputAsync, pinned Node_TEST.cc 1498–1600 and helpers 146–185.
Preserve original invalid advertise/request, /foo, Int32 data 5 or canonical
Empty input, initial unique advertised-service listing, independently reset one
execution then another one, actual execution and response completion, true result
and exact response data, invalid and valid withdrawal, and final empty listing.

The reviewed batch helper now accepts explicit call-count sequences. Existing
one-then-three batch ports remain unchanged in assertions; these two new ports
use one-then-one. Scoped request threads and receiver polling preserve actual
dispatch/completion evidence instead of C++ callback syntax. Each test has an
isolated partition and bounded polling, without fabricated callback flags or
readiness sleeps. No production source, coverage denominator, tolerance or
protocol change. These two exact native originals join the mandatory gate.

Independent original/helper assertion review and strict local/staged/hosted gates
remain required. No full NET-01/NET-02 acceptance is claimed.

The full local gate passes 79 Rust tests and 21 tooling tests, with 1283/1304
covered production lines and 1927/1991 regions. Production source is byte-identical
to 7268928; this slice adds no executable production lines. The local runner also
rechecks all 3/3 changed predecessor ownership lines through its fallback base.

Independent /root/sdformat review compared both original cases and their helpers,
then replayed the two new Rust ports, all three existing batch ports, and both
exact native originals (two runs, zero failures); all pass. It approved the
scoped polling mapping, actual counters and unchanged batch assertions. Exact
receipt: [asynchronous service review](reviews/service-async.json). The matching
strict staged gate remains required before publication, followed by hosted CI
on the published revision. No complete packet acceptance is claimed.
