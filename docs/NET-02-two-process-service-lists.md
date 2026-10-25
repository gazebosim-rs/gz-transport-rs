# Three two-process service list assertion ports

Pinned `gz-transport` source `41913b9dfc3dbce33ac5464ddcb427cc82450f26`
retains `twoProcSrvCall.ServiceList`, `twoProcSrvCallWithoutInput.ServiceList`
and `twoProcSrvCallWithoutOutput.ServiceList`. The unchanged native originals
each pass one test with no failures or skips. Their fixtures advertise `/foo`
from a separate C++ process with Int32→Int32, Empty→Int32 and Int32→Empty
callback signatures respectively. Each waits up to ten seconds for
`ServiceInfo` to find the provider, polling every 100 ms. Then it requires a
first `ServiceList` of length one with index zero `/foo`, clears the vector,
and repeats both list assertions with the second call taking less than two
integer-truncated milliseconds. No original calls a handler or inspects a
service type through `ServiceList`.

The Rust ports use the existing controlled C++ peer in those exact three
signature modes. They poll `service_type_signatures("/foo")` at the same
100 ms interval and ten-second bound, then use `known_services()` for both
snapshots. They retain both length and indexed-name assertions, the intervening
clear and the second-call bound. The peer exits after an explicit `QUIT`
barrier following the assertions. The original fixtures terminate their
repliers after each test. The native originals run in the mandatory gate.

This maps the observed single remote provider and discovery-complete list
behavior. `known_services()` is a bounded worker snapshot, not an atomic
global registry. Process-global same-process sharing, arbitrary provider
counts, handler execution, complete discovery timing and general latency
remain unqualified. No production API, runtime, wire, dependency or coverage
policy changes are proposed.

The independent [review](reviews/two-process-service-lists.json) approved
the scoped assertion mapping after private native and Rust replays. A
separate old-worker mutant failed each exact second-call bound at
2.07–2.08 ms. The source-frozen staged gate passed 160 Rust tests, 22 tooling
checks and strict coverage at 1816/1840 lines and 2755/2837 regions. Final
staged verification, normal hook and hosted CI remain pending.
