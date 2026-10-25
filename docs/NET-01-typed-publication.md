# Canonical typed publication

`Node::advertise_typed<M>` uses the exact descriptor name bound to a generated
canonical message. `Publisher::publish_typed(&M)` rejects a different descriptor
or an encoded size above this publisher's bound with `InvalidFrame`, before
encoding or admitting a command. It then uses maintained protobuf encoding and
the existing publication path. Queue saturation and shutdown retain the existing
typed transport errors. Publisher lease ownership and raw APIs are unchanged.

The wrappers reuse `CanonicalMessage` from the already pinned message producer,
including its sealed canonical descriptor binding and standard prost methods.
They add no dependency, registry, reflection lookup, codec, executor or error
enum variant. They support this producer's generated canonical messages; custom
external protobuf types can use the existing explicit descriptor/raw-byte API.
Typed callers must use generated types from the same pinned producer package as
its sealed trait. Raw forwarding remains independent of the caller's protobuf
message package.

Typed encoding is a value operation. Unknown fields discarded by typed decoding
cannot be reconstructed. Use raw `publish(bytes)` and unchanged envelope bytes
for opaque or lossless relays. No implicit typed decode/reencode is added to
transport forwarding, and raw payload bytes are not reinterpreted by the wrapper.

The reviewed complete NodeTest.TypeMismatch port retains Int32 data 1, Vector3d
components 1/2/3, a valid Int32 advertisement, rejection of the wrong typed value
and admission of the right value. Fresh owned Node state replaces the unused
callback globals reset by this fixture. No subscriber, timeout or extra readiness
barrier is introduced. The full original 2014 Apache/OSRF notice and new adaptation
attribution are retained. The unchanged native case passes XML one run and zero
failures with source/binary hashes recorded, and is required in every gate.

Supplements check descriptor validation before queue admission, encoded size
before allocation, preserved queue/shutdown errors, exact maintained Int32
encoding and delivery of a typed StringMsg to an actual C++ subscriber. The latter
uses bounded repeated publication as a mixed-process supplement, not as a change
to the original mismatch fixture. Existing opaque forwarding assertions remain.

Complete typed/reflection/external-type APIs and whole NET-01/NET-02 acceptance
remain open.

The integrated author gate passes 128 Rust checks and 22 tooling checks, with
1591/1611 production lines, 2420/2485 regions and 12/12 changed executable
production lines. The exact native TypeMismatch original and all prior required
originals pass. Independent /root/msgs review passed the three targeted Rust
checks with a fresh private source/target and C++ peer, plus the exact unchanged
native original (XML one run, zero failures) with verified source/binary hashes.
It accepted all original assertions and the canonical/owned-result API mapping;
that subset is distinct from the author full coverage gate. The exact
[authored receipt](reviews/typed-publication.json) is copied unchanged. The ledger
now has 35 complete ports, one partial and 382 pending cases. Matching final
metadata, the normal strict staged gate and hosted qualification remain required.
