<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Physical publication routes across discovery silence

A remote publisher can keep its ZeroMQ socket open while discovery traffic stops.
Previously Rust removed the expired advertisement and disconnected the subscriber
socket. Pinned native Discovery removes the advertisement, while NodeShared's
existing subscription socket continues receiving. The same-observer native probe
shows an empty publisher metadata list and successful delivery after silence.

Rust now retains each admitted message-channel publication route independently
of discovery metadata. A route contains only process/node/topic/address identity,
with no extra publisher payload. It has no silence TTL. Metadata and all existing
`known_*` freshness, identity, admission and expiry assertions remain unchanged;
a retained physical link does not resurrect an advertisement or subscriber reply.
RPC connection behavior is unchanged.

Routes are admitted only for a matching live local subscription and a newly
admitted message advertisement, or when subscribing to an already known message
publisher. At most `Config::capacity` routes are retained independently of the
peer table. Rejecting a new route increments `queue_drops` once per attempted
registration and emits no NEW_CONNECTION. Ignored duplicate advertisements do not
retry registration or count another rejection. A failed socket connect increments
`send_failures`, keeps no physical route/address admission and emits no
NEW_CONNECTION. A valid TCP connect to an absent peer succeeds asynchronously
and retains its bounded route; a route is not a delivery or liveness guarantee.
Expired metadata frees peer-table capacity without freeing a still-used physical route. A new subscription can
retry registration from admitted metadata after local retirement frees capacity.

Multiple identities/topics can share one address. The worker disconnects that
address when its last retained route is released. Physical address cleanup runs
before new route admission, including withdrawal/replacement in one UDP batch.
Local unsubscribe, final lease retirement and shutdown release their routes; wrong-channel metadata cannot
retain them. Existing Rust explicit UNADVERTISE and BYE policies still release
matching routes, including routes whose discovery records have already expired.
END keeps the existing subscriber-registration interpretation.

This is a silence-only compatibility repair. Actual native wire controls also
continue receiving after END, BYE and UNADVERTISE while the source socket remains
open, and stop callback delivery after local unsubscribe. Rust retains its earlier
explicit withdrawal cleanup assertions and releases links after UNADVERTISE/BYE.
Broader physical-link withdrawal parity remains open; this slice does not claim
complete native lifetime, process-global discovery, reliability or convergence.

The durable `silence-wire-observer` C++ peer uses the pinned native Node directly.
A shared raw ZeroMQ publisher and canonical discovery frames drive both native
and Rust observers. Initial admission is followed by four seconds of heartbeat-only
traffic, then 3.5 seconds without any advertisement, reply or heartbeat refresh.
Both observers must receive the exact StringMsg payload with publisher metadata
absent after silence. The test also preserves the native/Rust explicit withdrawal
difference, readvertisement, END, unsubscribe and shutdown behavior. Four private
controls cover independent route capacity/drop accounting, shared addresses,
multiple topics, process/channel retirement and full-capacity replacement in
one batch, and failed-connect capacity recovery. All prior assertions remain;
no upstream ledger disposition changes.

Focused commands (with the installed reference library directory on
`LD_LIBRARY_PATH` and `GZ_REFERENCE_PREFIX` set):

```sh
bash scripts/build-cpp-peer.sh
cargo test --locked --lib publication_routes
cargo test --locked --test interop physical_publication_routes
```

Independent review and the normal strict staged hook are required before landing.
