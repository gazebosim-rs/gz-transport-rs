# Bounded provider-record service inspection

`Node::service_provider_types` returns one owned canonical request/response type
pair per known provider record. It sorts without deduplication, so distinct
providers with identical types remain separate entries. The existing
`service_type_signatures` API continues to return sorted unique pairs; type
resolution and ambiguity behavior are unchanged.

The snapshot uses active local service registrations and unexpired admitted
service-channel peers from this Node's worker. Wrong-topic, message-channel and
wrong-kind records are excluded. Original-name qualification matches native
ServiceInfo and the existing remapping contract. A service-channel SUBSCRIBE
refresh may be sent; inspection never runs a service handler. The result has at
most twice the configured registration/peer capacity. No address or provider
identity is exposed, and ordering is by type names rather than native storage
order.

Invalid names return `InvalidName`. Absent providers yield an empty successful
snapshot. The bounded command queue, reply timeout, cancellation and shutdown
use the existing typed transport errors. A dropped query receiver cannot block
the worker, and queries consume no registration permit. This is asynchronous
per-Node discovery, with the existing 3000ms expiry and admission policy; no
process-global immediate visibility or native global-lock timing is promised.

The proposed ports retain the three exact two-process ServiceInfo fixtures:
ordinary Int32/Int32, inputless Empty/Int32 and outputless Int32. Each starts a
real typed C++ provider before the Rust observer, then uses native waitForService's
actual provider predicate, 10000ms timeout and 100ms polling interval. Invalid
`@`, absent `/bogus`, known `/foo`, exactly one observed provider record and every
original canonical descriptor literal remain asserted. Invalid false/empty
out-parameters map to an error with no result; absent false/empty out-parameters
map to an empty owned vector. The outputless original asserts only request type.
Full original 2014 and 2016 Apache/OSRF notices remain attached to the ports.

The previous signature-only prototype did not preserve the native provider-count
observer: unique type pairs can hide extra equal-signature records. That prototype
was not published or retired as a complete port. The new record view closes that
specific observation gap. A separate live test first verifies that native
ServiceInfo observes two identical providers, then checks Rust record counts
2, 1 and 0 across actual graceful withdrawals while the unique signature count
remains 1 until final withdrawal. Unit checks preserve the prior signature
assertions and add record multiplicity, lifetime/channel/kind filtering and
bounded query failure paths.

All three untouched native originals previously passed with XML one run and zero
failures each; source and binary hashes are retained. They are mandatory in every
new full gate. The full author gate and independent assertion/API review below qualify this
bounded production API increment. Provider address/identity inspection,
global query/cache timing, process-global visibility and complete NET-01/NET-02
acceptance remain pending. No wire, schema, dependency or performance change is
claimed.

The integrated author gate passes 121 Rust checks and 22 tooling checks, with
1543/1565 production lines, 2356/2422 regions and 16/16 changed executable
production lines. All three untouched native ServiceInfo originals pass again.
An initial Clippy failure in two new assertion expressions is retained; replacing
cloned expected values with borrowed one-element slices preserves their exact
checks. Independent review accepts all three as faithful ports within the
owned Result/no-address API contract. It independently replayed two focused
unit tests, all four new real-C++ interoperability checks and all three exact
native originals (each XML one run, zero failures), using a private source and
target. Original source/binary hashes match; initial missing-Zenoh loader failures
are retained separately from the corrected successful replay. The prior unique
signature assertions remain intact. This independent subset is distinct from
the author full 121-check coverage gate. The ledger now records 34 complete
ports, one partial and 383 pending cases. Matching final metadata, the normal
strict staged gate and hosted qualification remain required.

The exact [independent review receipt](reviews/service-providers.json) binds the
inspected source and final mapping metadata, with its focused replay limits.
