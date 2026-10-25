# Two-process topic list and latency

Pinned `gz-transport` source `41913b9dfc3dbce33ac5464ddcb427cc82450f26`
retains `twoProcPubSub.TopicList` at
`test/integration/twoProcsPubSub.cc:348–380`. Its unchanged native executable
passes one test with no failures or skips. The auxiliary advertises a
`gz.msgs.Vector3d` publisher on `/foo` and a `gz.msgs.Bytes` publisher on
`/large_msg`. The original waits up to ten seconds for `/foo` discovery using
100 ms polls. It then requires a nonempty first list containing `/foo`, clears
that vector, and requires a nonempty second list containing `/foo` with its
call taking strictly less than 2 ms.

The Rust port starts a real C++ peer with both native advertisements, uses the
same discovery deadline and poll interval, and checks both list contents and
the original second-call bound through `known_topics()`. An explicit `QUIT`
barrier keeps the peer alive through the observations; the original auxiliary
uses a fixed three-second lifetime. Neither test inspects published payloads.
The pinned original joins the mandatory native gate.

The worker's previous unconditional 2 ms sleep made each sequential list
request take about 2.06–2.10 ms in an isolated 50-call measurement. The exact
ported <2 ms assertion fails against that production source at 2.091207 ms.
Waiting up to the same 2 ms on the command receiver preserves bounded idle
socket polling and lets commands wake the worker. The same exploratory
50-call measurement after the change took 22–35 µs per call. These samples
describe this local machine, not a general latency guarantee. The original
single-call bound remains the gate assertion.

The [independent review](reviews/two-process-topic-list.json) approved the
original assertions, command wake behavior and bounded idle poll. Its private
source and target replay passed the native original and ten exact Rust repeats;
restoring the old sleep failed the <2 ms assertion. The reviewed staged gate
passed 156 Rust tests, 22 tooling checks and strict coverage at 1816/1840
lines and 2756/2837 regions, with all 3 changed lines covered. The final
metadata-bound staged gate, normal hook and hosted CI remain required.
