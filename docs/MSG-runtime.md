# Maintained message runtime integration

Transport pins reviewed gz-msgs commit
2cea810e929adb4f925750b325265a99f9ae78b4, using prost 0.14.4 and
prost-reflect 0.16.5 with producer-owned, tested signed-zero corrections.
The former rust-protobuf dependency is removed from the resolved graph.

Discovery codecs now use generated typed oneofs, integer enum fields,
encode_to_vec and encoded_len validation before allocation. Incoming discovery
uses the reviewed native-compatible envelope: known-tag/wrong-wire occurrences
are unknowns, preserving known values, optional presence and oneof ordering.
Its typed value is explicitly cloned; unknown discovery data is not retained as
a relay promise. Exact-revision hosted qualification is recorded on the public
progress dashboard. The consumer's bounded native matrix is documented in
[discovery decoding](NET-01-discovery-decoding.md). Unknown scope
values remain fail-closed; a service advertisement cannot become a message
subscription. The public transport Message/Publisher/Subscription/Request bytes
and type-name API is unchanged. Application payloads are never decoded/reencoded
inside transport. New real C++ fixtures prove exact bytes for an unknown varint
application field in both directions. Discovery relay forwarding remains pending;
no lossless forwarding claim is made for edited decoded discovery controls.

LLVM instrumentation now applies only to owned Cargo workspace packages using
RUSTC_WORKSPACE_WRAPPER. Unmodified registry/Git dependencies remain outside the
production denominator and are uninstrumented. This avoids a confirmed zero-hash
memchr2.8.3::cow::Imp::new diagnostic in Rust1.94.1's LLVM tooling. Fatal LLVM
warnings reject any remaining mismatch; no warning allowlist or production
coverage exclusion is used. The wrapper path contains its contents digest so
Cargo cannot reuse differently instrumented owned artifacts. Every declared
production file still requires nonempty instrumentation.

Initial integration evidence: 30 Rust tests and seven quality-tool tests passed,
including all real
C++ pub/sub/service/configuration peers, unknown-field payloads and discovery enum
edge cases. Rustfmt/Clippy pass. Production coverage:987/1006 lines (98.11%),
1486/1545 regions (96.18%); changed executable lines40/40. Strict LLVM exports
pass with --fatal-warnings. The commit hook repeats the exact staged-tree matrix.

The current pin brings the reviewed bounded descriptor byte-default rejection
and definition/CLI updates into the dependency graph. Canonical schemas,
generation, `CanonicalMessage`, wire code and vendored codecs are byte-identical
to the prior e2001a3 pin. Only the exact Git sources and new `clap_lex` 1.1.1
package change in Transport's lockfile; no owned production source changes.
The descriptor preflight rejects octal overflow and UTF-8-splitting hex escapes
with a normal `DecodeError`; it does not claim native default interpretation
parity. Transport does not load external descriptor defaults during delivery.

Consumers passing canonical values to `publish_typed` must pin their direct
`gz-msgs` dependency to the same exact revision as Transport. Cargo treats two
Git revisions as distinct generated types and sealed `CanonicalMessage` traits.
A private matching consumer compiles typed publication successfully. A separate
Linux graph with the unchanged Fuel checkout resolves one `gz-msgs`, one prost
0.14.4 and one socket2 0.6.5. Fuel has no `gz-msgs` dependency; its private
crates.io prost-reflect remains separate from the producer's vendored copy.
No broader wire or performance claim is added.

The independent [pin review](reviews/msgs-pin-2cea810.json) approved the exact
three-file source/documentation packet. A fresh reviewer source/target and
native peer passed all 135 Rust checks in the gate's declared serial mode; the
matching consumer/default guard, expected mixed-pin compilation failure and
read-only Fuel graph were independently confirmed. The author full gate passed
135 Rust and 22 tooling checks with 1641/1645 lines and 2460/2509 regions.
No executable production lines change in this slice.

An exploratory default-parallel reviewer run timed out in the pre-existing
malformed-discovery fixture, which sends one unicast datagram to a shared UDP
port. Controlled probes reproduce delivery to a competing worker on both old
and new pins. This failure is retained separately from the passing declared
serial gate and requires a separate fixture-isolation repair; it is not a pin
regression or a waived test. Final exact staged and hosted checks remain required.
