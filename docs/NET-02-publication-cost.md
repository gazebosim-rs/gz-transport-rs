# Process-local publication cost

The same-process direct-delivery contract still records a fingerprint for
every publication from a live advertisement lease before its wire send. A
queued command after lease retirement cannot populate a replacement lease's
history. This history suppresses a
matching local wire echo even when no registration matched the earlier direct
snapshot. A later subscription can otherwise receive an already queued local
wire frame with the wrong origin. Echo matching still requires the message
domain, publisher address, qualified topic, descriptor, payload size and
fingerprint; the three-second retention, `Config.capacity` bound and
address-reuse controls remain in force.

An admitted Publisher now owns the history for its exact advertisement lease.
The worker records into that handle without scanning the process-wide
publication registry on each normal `Publish`. The registry and per-lease
history retain their established lock order, retirement, poison accounting
and conservative suppression on ambiguous poisoned history. Ordered prefix
expiry removes only entries past the same three-second limit. A generation
cache avoids scanning process-local interests while no other live Node can
receive; successful subscription admission invalidates that cached absence.
The cache does not skip fingerprint recording.

The worker borrows the owned publication for same-Node delivery, then uses
independent clones for earlier matching process-local receivers and moves the
owned message into the final receiver. Matching order, wildcard/type/domain
checks, `ignore_local_messages`, bounded `try_send`, queue-drop counters,
wire format and public API remain unchanged. A capacity-two fanout control
checks two receivers, a full queue, type mismatch, partition isolation and
an ignored same-Node registration. Its initial `has_connections()` query
could encounter `QueueFull` during worker startup; the tracked test retries
only that setup query for at most five seconds. It neither retries a
publication nor changes payload, drop-count or receive assertions.

The exact staged source gate passed with 2486/2543 production lines,
3690/3846 regions and 104/108 changed executable lines. The independent
[source review](reviews/publication-cost-source.json) privately replayed
eight process-local controls and showed that disabling only final-recipient
delivery fails the new drop-count assertion.

An independent [performance review](reviews/publication-cost-performance.json)
audited 22 matched Release samples of the unchanged default Server
clock/pose/IMU stream: 128 steps, 20 ms minimum pacing, shared CPU 31,
ten alternating measured pairs and one warmup pair. All payload, count,
identity and error checks passed. Median work time was 37.3309 microseconds
per step for the parent and 38.5756 for this candidate. The paired geometric
ratio was 1.03598 with bootstrap 95% interval [0.99243, 1.07658]. The
3.598% point overhead meets the proposed 5% point budget, but the interval
extends above 5%. This shared-host result does not establish a 5% upper
bound, a C++ speedup or a whole-engine latency claim. Simulator adoption
requires its own exact consumer gate and comparison.
