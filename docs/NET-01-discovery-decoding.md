# Native-compatible discovery decoding

Bounded NET-01 consumer task: pin reviewed, hosted-green gz-msgs revision
e2001a3bc5b54cb15ea1cc16deaefdf14719b2df and opt the existing discovery decoder
into its sealed native-compatible envelope. Owned changes are the manifest/lock,
protocol decode call, focused native differential cases, C++ parser helper and
this contract/runtime integration record. No service-type inspection API or
full retained-packet acceptance is claimed by this increment.

The pinned C++ protobuf parser accepts a known field with an unexpected wire type
as an unknown occurrence. Direct generated prost decoding rejects it. Retain
native accepted known values, message/oneof presence, duplicates and nested
publisher metadata while rejecting malformed framing/known data. Existing UDP
native-endian length, maximum datagram and ordinary discovery version-10 checks
remain unchanged, including rejection of version 0/9/110 in this profile.

Use the producer envelope only to obtain the typed discovery value. Cloning its
value is explicit and lossy for unknown fields; no lossless discovery relay is
claimed. Transport application message/request/reply payload bytes remain opaque
and unchanged. Caller datagram limits continue to bound decoder input.

Native differential tests must execute the actual installed generated C++
Discovery parser, compare its known-only value against transport decoding, and
cover wrong-wire top-level and nested fields, known values before/after unknowns,
oneof precedence, duplicate-message merge, invalid UTF-8, malformed lengths and
version/framing rejection. Preserve all existing native/Rust gates. Independent
review and exact local/staged/hosted qualification are required before acceptance.

The final local gate passes 81 Rust tests and 21 tooling tests, with 1286/1307
production lines, 1929/1993 regions and all 4/4 changed executable lines covered.
All existing native peers and mandatory original assertions remain in the gate.
The lock preserves prior unrelated Windows package resolutions; its additions
are the producer's pinned bytemuck/quick-xml dependencies and reviewed Git paths.

Independent /root/msgs review rebuilt the strict C++ peer and passed all 31
discovery parser cases plus the runtime admission/unknown-scope regression in a
separate reviewer-owned target. It verified the exact source/test hashes and
unchanged prior Windows package blocks. Full coverage is author evidence, not
an independent coverage rerun. Exact receipt:
[discovery decoding review](reviews/discovery-decoding.json). The matching
immutable staged gate remains required before publication, followed by hosted
CI on the published revision. Twenty original faithful ports remain; this
differential increment retires no additional original case or broad NET packet.
