<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Nonblocking Publisher interest snapshot

`Publisher::connection_snapshot()` reads a per-publication atomic registration
cache. It sends no worker command and performs no socket operation, lock or
allocation. It returns `Err(Shutdown)` after explicit cancellation or after the
Publisher registration is retired. Ordinary Node destruction leaves a surviving
Publisher usable; it retires that Node's subscriptions, so their local interest
disappears from the snapshot immediately. A true result reports a known matching
subscriber registration, not authentication, delivery readiness or delivery.

Local interest is admitted when `subscribe` or `advertise` succeeds, before the
worker processes the queued command. Matching uses the qualified topic after
one configured remap, the exact message type or an empty type wildcard. Dropping
one of two matching Subscription handles leaves interest true; dropping the last
handle or calling `unsubscribe` with the registered topic clears it immediately.
The cache contains only subscriptions on the Publisher's own Node. Other Nodes
become visible through discovery; this API makes no process-global immediate
visibility claim.

Remote interest follows the worker's existing remembered `NewConnection`
registrations, including its matching control identity and message type.
Matching `EndConnection` and Unadvertise clear it. Process-only BYE leaves it
remembered, as in the existing `has_connections()` contract. No TTL is applied
to that connection map. The worker updates the remote bit without touching the
locally admitted bit, so delayed worker admission cannot erase local interest.

`Publisher::has_connections()` remains the bounded worker-roundtrip query and
its current error semantics remain unchanged. The atomic method is intended for
callers that must decide whether to prepare a publication without waiting for
worker scheduling. The result is only a membership hint; concurrent admission
or retirement can change it immediately after the read.

The pinned `Node.cc` `Publisher::HasConnections` path checks local and remote
subscriber membership under `NodeShared` ownership. A source-bound native
single-Node probe observed false before subscription, false for a wrong type,
true immediately after a matching subscription, and false immediately after
unsubscribe. The Rust interop test uses a real pinned C++ subscriber peer to
exercise eventual remote matching. Rust tests additionally cover wildcard, duplicate local
handles, remap, delayed worker commands, remote identity/type/partition,
withdrawal, process-only BYE, owner destruction, cancellation and queue
saturation. The exact native and Rust evidence for this candidate is retained
under ignored `artifacts/interest-snapshot-scope/`.
