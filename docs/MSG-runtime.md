# Maintained message runtime integration

Transport pins reviewed gz-msgs commit
e1800c3e8d058ce7358b393e5f7aba6dd912920f, using prost0.14.4 and
prost-reflect0.16.5 with producer-owned, tested signed-zero corrections.
The former rust-protobuf dependency is removed from the resolved graph.

Discovery codecs now use generated typed oneofs, integer enum fields,
encode_to_vec/decode, and encoded_len validation before allocation. Unknown scope
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

Validation:30 Rust tests and seven quality-tool tests pass, including all real
C++ pub/sub/service/configuration peers, unknown-field payloads and discovery enum
edge cases. Rustfmt/Clippy pass. Production coverage:987/1006 lines (98.11%),
1486/1545 regions (96.18%); changed executable lines40/40. Strict LLVM exports
pass with --fatal-warnings. The commit hook repeats the exact staged-tree matrix.
