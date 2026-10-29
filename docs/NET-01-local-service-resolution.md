<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Immediate admitted local service resolution

`resolve_service_types` now includes live services admitted by other Rust workers
in the same explicit service domain. The selected native
`NodeTest.ResolveServiceTypesAmbiguous` registers Int32/Int32 and Int32/StringMsg
providers and immediately checks ambiguity, explicit StringMsg response selection,
and incompatible Vector3d selection. Its default provider timeout is zero. The
new direct Rust case keeps those inputs and every result predicate without a
readiness barrier; the prior discovered-signature variant remains intact.

Native `detail/Node.hh` 325–353 adds the handler and synchronously calls shared
service discovery advertisement before returning. `Discovery.hh` 510–536 inserts
the local provider before sending multicast. `Node.cc` 1169–1278 reads that shared
table and selects compatible types. Native ServiceInfo still has its internal
startup wait. A separate warm native control verifies all selected outcomes and
immediate withdrawal with zero provider timeout; it makes no latency claim.

## Ownership and bounds

The existing admitted Node service vector now also keeps canonical types beside
its weak active leases. Node and its worker identity share that one vector.
Global lookup indexes only weak worker identities, pruned on registration and
lookup. No global handler, request queue, provider wire payload or runtime is
introduced. Service declarations retain the existing per-worker registration-slot
bound; inactive entries are pruned before admission and during resolution.

An observer retains an owner reference only for admitted service-channel peers
whose identity and domain match a known local worker. At most `Config::capacity`
such references can exist because every retained owner has an admitted peer.
UNADVERTISE, BYE and expiry release an owner with its last peer. A query's bounded
snapshot retains those references until its caller masks stale wire records;
otherwise owner destruction between snapshot and merge could resurrect a copy.
There is no per-service tombstone table. The weak index scales with existing live
workers and owner identities held by bounded peer/query state, rather than service
churn. Retained identity does not override a stopped owner or expired/inactive lease.

The resolver first receives an identity-bearing worker snapshot through the same
bounded command queue. It then releases the global index lock before inspecting
shared admitted vectors; no registry lock is held during worker/network waits.
Known local owners mask their copied wire providers, including inactive ones.
Active declarations and remaining remote providers feed a fixed-size selection
accumulator, avoiding a copy of every local provider. Equal signatures do not
create ambiguity. No handler executes during inspection.

Admission failure, duplicate rejection, lease drop, explicit service withdrawal,
Node destruction and callback retirement retain their existing flag/slot rollback.
A failed callback-thread spawn drops its captured Service lease as before; weak
registry state cannot keep it active. Private Node fixtures now share the same
admitted vector as their worker; their assertions remain unchanged.

## Boundaries

Service domains match normalized partition, interface, multicast address and
service port. Message-port differences do not split a service domain. Topic
qualification preserves namespace and the original-name query rule; registration
remapping does not remap inspection queries. Worker-only `service_type_signatures`,
`service_provider_types`, `known_services` and owned service metadata keep their
existing views and freshness. The resolver can therefore see an admitted local
provider before those asynchronous worker snapshots do.

Both explicit names still bypass validation and runtime lookup. Other validation,
queue saturation, coordination timeout, shutdown, incompatible/absent outcomes and
100 ms caller polling retain their existing contracts. No pending-query list,
wire change, dependency, service delivery path or simulator step wait is added.

This is resolver-only local type visibility, not full process-global discovery or
identity. A late first wire packet after every owner and admitted-peer identity
reference is gone cannot be recognized as an old local copy. That existing identity
gap remains open, as do global metadata snapshots, atomic observations across
concurrent owner changes, service scopes and broader callback/delivery parity.

Four focused public/native tests and four private controls cover the exact direct
original, warm native outcomes, live/drop/callback/stop behavior, stale-copy masking,
retained-owner expiry and capacity, admission rollback, weak-index pruning/poison
errors, namespace/remap and service-domain isolation. Prior barrier, worker-view,
queue and discovery assertions remain. Independent /root review compared the
exact original and checked ownership, domain, admission and retirement semantics;
a fresh source/target/native peer replay passed 114 library tests, 18 prior
service assertions and four public/native tests. Only the selected direct
ambiguity original is approved for faithful-port credit, conditional on the normal
strict staged gate and exact hosted CI. Broader identity and metadata gaps remain.

```sh
cargo test --locked --test service_resolution_local
cargo test --locked --lib local_service_
cargo test --locked --test service_assertions
```
