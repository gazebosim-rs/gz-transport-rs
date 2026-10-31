<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->
# Bounded discovery forwarding

This private candidate adds real forwarding on both existing discovery sockets.
`Node::add_discovery_relay(Ipv4Addr)` admits an explicit peer to both the message
and service channel. Each destination uses that channel's configured port.
`Node::discovery_relays()` returns the sorted union of this worker's explicit and
learned IPv4 addresses. These are worker-owned APIs: native `AddGlobalRelay` and
`GlobalRelays` use process-global discovery objects. Neither API establishes a
remote connection or discovery convergence.

Each channel retains at most `Config::capacity` addresses, independently of the
existing endpoint and request bounds. Duplicate admission is idempotent. Explicit
admission checks both channels before changing either; an exhausted channel
returns `QueueFull` and increments `queue_drops` once. Unspecified, multicast and
broadcast explicit destinations return `InvalidConfig`. Learned addresses come
from the actual IPv4 UDP source; repeated sources do not consume capacity. A full
learned set increments `queue_drops`, continues multicast reinjection and retains
its existing addresses. Native has an unbounded address vector; Rust retains its
bounded vectors until this worker exits, with no invented silence expiry.

Both APIs use the existing nonblocking bounded command queue, configured reply
timeout and shutdown errors. A caller timeout does not revoke a command already
admitted to the worker. No additional thread, socket, timer or persistent queue
is created. Empty address vectors allocate no payload storage. This first API
slice did not implement GZ_RELAY; the later [environment contract](NET-02-legacy-relay.md)
adds it separately without relaxing explicit address admission.

The flow matches pinned `Discovery.hh::DispatchDiscoveryMsg`:

- Reject malformed/version-mismatched packets and this worker's own process UUID
  before forwarding or source learning.
- With `relay=true`, multicast a copy with `relay=false,no_relay=true`, learn the
  source address on that channel, and defer local dispatch to the multicast copy.
- Otherwise, forward a copy with `relay=true` to each channel relay unless
  `no_relay=true`, then perform the existing local discovery handling.
- Locally generated discovery messages retain their ordinary multicast copy and
  also send a `relay=true` copy to the same channel's peers.

The relay branch takes precedence when both flags are true, as in native code.
Existing scope filtering happens during local dispatch, after the relay decision;
no new scope policy or process-global name merge is introduced. Statistics-wire
mode keeps version 110; the default remains version 10 on both channels.
Unicast destinations retain insertion order and a failed send ends that packet's
unicast loop, matching native. Send errors and final-size rejection increment
`send_failures`; receive framing/version errors retain `malformed_frames`.

Forwarding copies every non-Flags field byte and merges only legitimate Flags
message occurrences. Existing `prost::encoding` decodes/skips fields and encodes
the replacement: the edited bool occurrences are removed, all other flag bytes
retain their order and representation, and the desired bool is appended to one
merged Flags message. Wrong-wire known tags remain unknown occurrences. Clearing
relay then uses its native default false. This narrow protocol transformation
does not implement a varint codec or general-purpose protobuf parser. Pinned
`prost-reflect` dynamic mutation was considered, but its known-tag decoder rejects
wrong wire types. The producer's compatible envelope provides a read-only typed
view, not mutation of retained unknown data.

An initial append-only helper overflowed a valid maximum-size relayed packet;
native reinjection of that same 65507-byte input fits without growth. A durable
native-backed red regression demonstrated the loss. Merging Flags fixes that
case while retaining unknown fields and noncanonical data outside the edited
flags. Final datagram bounds still apply; broader native normalization of other
fields to make space is not claimed. The typed projection serves only local
handling and never serializes forwarded endpoint metadata.

Four revised pure author controls cover 64 comparisons with the actual canonical
C++ mutator over duplicate Flags, explicit false, unknown groups, noncanonical
varints and wrong-wire fields, plus merged length boundaries, malformed fields
and the exact maximum-size regression. These and four private/four public checks
pass in debug, release and instrumented builds. Both channels and version
profiles are exercised. A separate check feeds actual C++ relay packets into
Rust and observes both multicast reinjections. This same-host Linux fixture
relies on known shared-port bind ordering; it does not simulate separate
multicast networks or prove multi-host connectivity.

The corrected serial affected run passes 182 library, 111 unchanged interop,
11 typed callback, five owned, six raw and four relay tests with nine fresh
canonical peers. The selected unchanged native original passes one/no failures;
11 unchanged configuration and 35 Python controls and Clippy pass. Root identified
an ephemeral-port reservation race in the new public fixture. Reusable wildcard
socket guards now survive each whole test, with multicast-all disabled, retaining
Node/native/observer bind order and all predicates/deadlines. The final four public
checks re-pass in all three profiles after this test-only correction. The first
synthetic topic spelling was also replaced with the existing qualification API.
Relay-peer linkage resolves all three Gazebo libraries inside the reference prefix.

Focused final evidence reports 150/155 changed executable lines; its five misses
are the existing moved announcement-encoding error block, not exclusions. This
is diagnostic and does not establish full crate/repository coverage. Earlier
append-only evidence is superseded. Independent Root review used a fresh source
export and target, rebuilt all canonical peers, and passed eight pure/private
and four guarded public/native controls. Native linkage resolves all three Gazebo
libraries inside the pinned prefix. The reviewer compared the native forwarding
source, flag precedence, own-process rejection, scope timing, learning and failed
send order, including the maximum-frame and unknown-field controls. This bounded
slice is approved for normal staged/exact hosted qualification, which remains
pending; no original row is promoted.

The selected retained `NodeTest.relay` case checks absence of `127.0.0.123`, adds
it, then checks a one-element size increase and presence. The candidate preserves
that complete one-Node assertion body through the owned API, with separate bounds
and packet controls. Broader multi-host topology, shared native singleton
identity, CLI relay and whole NET-02 completion remain open.


A later [legacy environment slice](NET-02-legacy-relay.md) qualifies bounded
GZ_RELAY parsing separately without changing forwarding or strict typed admission.
Native global identity, multi-host topology and CLI scope remain open.
