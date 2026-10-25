# Bounded service response admission

`Request::respond` enqueues one response command into a queue with
`min(Config.capacity, 64)` slots. Ordinary commands retain the existing
`Config.capacity` queue. A full publication command queue therefore does not
consume response admission slots. A full response queue returns `QueueFull`,
and shutdown returns `Shutdown`; queue drops remain observable in the existing
counter. The worker owns both receivers and processes one response before each
ordinary command while preserving the existing bounded network work.

When ordinary commands are absent, a response-only burst drains about one
response per worker poll, with up to the existing 2 ms idle wait between
polls. Admission does not guarantee a delivery deadline: downstream pending
replies, the reply queue, and ROUTER sends retain their separate bounds and
failure modes. The ordinary queue capacity and wire protocol are unchanged.

The tracked flood test uses a service Node that publishes clock and pose
messages, a separate subscriber Node, and a fresh client Node. It requires
publication queue pressure, exactly one received service request, successful
`respond`, and the exact cross-Node reply. On the prior public runtime, the
same traffic produced one received request followed by `QueueFull` at
`respond` and a client timeout. A capacity-one worker test separately checks
independent admission, typed `QueueFull` and `Shutdown`, and drop counters.
The shared bounded command queue for responses existed before public
`2d810f8`; this observed failure does not establish when the pressure began
or whether publication-cache work contributed to it.
The [independent review](reviews/service-response-lane.json) replayed the
negative and positive cases. The first frozen Sim gate's timeout remains
unclassified at the precise failure-event level; a later diagnostic replay
identified response admission failure in the same test case.
Publication-cache cost is a separate performance question, with no speedup
claim from this slice.
