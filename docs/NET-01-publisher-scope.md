# Message publication scopes

`AdvertiseOptions` selects the canonical `Scope::Process`, `Scope::Host` or
`Scope::All` for a message publisher. Its default is `All`; existing `advertise`
and `advertise_typed` calls keep that default. Explicit options use
`advertise_with_options` or `advertise_typed_with_options`:

```rust
let mut options = gz_transport::AdvertiseOptions::default();
options.set_scope(gz_transport::Scope::Process);
let publisher = node.advertise_typed_with_options::<gz_msgs::int32::Int32>(
    "/reading", options,
)?;
```

Qualification, one publisher per qualified topic on each Node, remapping,
handle lifetime and cancellation follow the existing publisher contract.
The scope enum comes from the pinned canonical messages dependency.

## Discovery and local delivery

The worker preserves the pinned native `Discovery.hh` behavior on its configured
message channel:

| Event | Process | Host | All |
| --- | --- | --- | --- |
| Initial advertisement | Suppressed | HOST metadata | ALL metadata |
| Subscribe response | Suppressed | HOST metadata for eligible local senders | ALL metadata |
| Periodic heartbeat advertisement | PROCESS metadata | HOST metadata | ALL metadata |
| Publisher withdrawal | Suppressed | HOST metadata | ALL metadata |

Native heartbeats re-advertise stored Process publishers. Receiving peers discard
Process metadata, and admit Host metadata only from eligible local IPv4 interfaces
or loopback. The Rust receiver keeps that existing admission policy. The Host
query-response check uses the same local interface inventory captured at startup.
Physical multi-host qualification remains open.

Matching owned Nodes in the same explicit message domain retain direct local
delivery for every scope. Process publisher readiness uses the existing live
process-local subscription registry, matching domain, topic and descriptor or
wildcard. The worker refreshes a separate atomic interest bit, including
subscription retirement and owner cancellation. A poisoned registry clears that
bit and increments the routing-error counter. Same-Node and remembered remote
interest retain their previous independent bits.

`has_connections` remains a bounded worker snapshot; `connection_snapshot` reads
the cache. Neither acknowledges delivery or authentication. Publication sending
and remote registration behavior remain unchanged. Scope governs discovery
admission; authentication and broader transport policies remain open.

## Retained originals and evidence

The four fixed `NodeTest.ScopeProcess`, `ScopeProcessRemap`, `ScopeHost` and
`ScopeHostRemap` originals map to the corresponding `original_scope_*` Rust tests.
They preserve the selected scope, valid Int32 publisher, admitted subscriber on
a separate thread, the original 10-second/10-millisecond readiness polling,
successful single publication of Int32 data 5, exact received data, joined
subscriber and execution flag. Remapped cases retain `/foo` to `/bar` on both
Nodes. The four existing default/All threaded originals retain their assertions.

All four pinned native originals passed before implementation. A native-only
capture also retained Process heartbeat advertisements and absence from a normal
remote subscriber, with positive Host/All controls. Supplemental Rust tests check
initial/query/withdrawal datagrams, native payload and scope observations,
process-local payload delivery, and readiness across domain/topic/type mismatches,
wildcards, subscription drop and owner cancellation.

The exact source gate passed 234 Rust and 24 tooling tests, native Node 60/0,
pub/sub 11/0, ordinary services 6/0, inputless services 5/0 and all other required
groups. All five production files clear strictly greater than 95% line and region
coverage: 2702/2778 lines and 3971/4157 regions in total, with 73/73 added executable
lines covered. All 209 source reports are retained.

Independent [review](reviews/publisher-scope.json) rebuilt fresh source and peer
targets, passed six native and 95 selected Rust cases, and exercised three
production controls. Forcing Process wire scope to All failed the UDP/native
metadata predicates; disabling Process readiness failed both original readiness
bounds; accepting a foreign Host query failed the response-admission predicate.
The readiness control's secondary subscriber timeouts remain retained. Restored
source passed all 95 cases. Full JSON/LCOV and added-line policies were separately
recomputed without source or region filtering.

The initial prototype compile failure, first selected Process readiness failures
and reviewer metadata/log assembly errors remain retained. The production
readiness correction preserves every original assertion and bound. C++ callback,
storage and scheduler facades remain owned Rust receiver/worker adaptations;
immediate native process-global visibility is unclaimed. Service scope,
throttling, authentication, physical multi-host behavior and performance remain
outside this packet. Final metadata binding, the mandatory normal protected hook
and exact new hosted success remain required.
