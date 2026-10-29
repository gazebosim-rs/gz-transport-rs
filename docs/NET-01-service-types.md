# Bounded service signature inspection and resolution

This bounded library slice exposes known service type signatures and resolves
missing request/response types using the existing owned IO worker. Owned files
are runtime/API exports, focused unit and integration tests, mandatory native
original filters, ledger and this contract. No global Node runtime, full provider
address query, relay, CLI or packet completion is claimed.

Pinned Node.cc 1169–1278 and Node.hh 818–866 define the reference. Both explicit
types return unchanged without topic validation or discovery. Otherwise validate
the topic, use each explicit type to select compatible providers, distinguish
no providers from incompatible providers at the final observation, and reject
ambiguity in the missing types. Errors leave caller inputs unchanged by using
borrowed selections and a typed result. Retain canonical descriptor spellings.

The Rust inspection API returns sorted unique type pairs, not native
provider objects or addresses. A bounded command/reply channel snapshots active
local services and unexpired service-channel peers; message-channel/wrong-kind
records do not become service signatures. An existing discovery SUBSCRIBE may
refresh metadata. No service handler executes during inspection.

Resolution polls snapshots at the native 100ms interval until a compatible
signature or its provider-wait deadline. A zero wait still takes one snapshot.
Worker coordination is separately bounded by Config.reply_timeout; this replaces
native ServiceInfo's initialization/global-lock coordination and can add bounded
queue overhead. Queue saturation, coordination timeout and shutdown remain typed
transport errors. Polling is caller work outside simulation stepping; the IO
worker never blocks waiting for providers and stores no unbounded query list.

The ports retain NodeTest.ResolveServiceTypes, ResolveServiceTypesWaitsForProvider
and the observed ambiguity/selection assertions. The original absent-provider
bound remains at least 300ms; the delayed provider uses the original 300ms delay
and must resolve before 5000ms. The later [local resolver slice](NET-01-local-service-resolution.md)
also observes admitted same-domain service declarations immediately. Public
worker-only snapshots remain asynchronous, and the prior barrier variant stays
as a separate regression. Full native process-global identity remains open.

Supplemental cases cover invalid names, both-explicit bypass, cancelled and
saturated commands, coordination timeout/invalid duration, expired or withdrawn
providers, wrong channel/kind, independent selected fields and concurrent calls.
Retain all existing local/staged/hosted gates and real C++ provider metadata
inspection. No assertion tolerance, production exclusion or performance claim.

The reviewed slice implements `Node::service_type_signatures` and
`Node::resolve_service_types`. `ServiceTypes` holds owned canonical request and
response names. `ServiceTypeResolution::Resolved` returns the selected pair;
`NoProviders`, `IncompatibleTypes` and `AmbiguousTypes` preserve native outcome
distinctions. Invalid service names map to `Error::InvalidName`; queue, worker
coordination and cancellation failures remain outer transport errors. Borrowed
input names cannot be modified on failure. Both explicit names bypass even
invalid topics, impossible wait durations and cancellation, as native does.

Each snapshot sends an existing service-channel SUBSCRIBE refresh and returns
sorted, deduplicated type pairs from live local services and unexpired admitted
service peers. Duplicate providers with the same pair do not create ambiguity.
The snapshot can contain at most twice the configured capacity before deduplication;
there is one bounded command queue and no pending-query collection. A timed-out
caller drops its reply receiver; the worker's nonblocking reply cannot stall.
Cancellation while polling is observed at the next snapshot (at most one 100ms
poll interval plus worker scheduling). Discovery lifetime remains 3000ms.

The original inspection slice preserves the local and delayed-provider assertions.
Its historical ambiguity mapping is described below; the new direct resolver
case and its independent review are tracked in the linked local resolver contract.
For that earlier ambiguity case, the Rust test additionally waits for two actual signature
advertisements, then retains every original ambiguity and selection assertion.
This intentionally exposes asynchronous per-Node discovery rather than claiming
native process-global immediate visibility. Independent review accepts this as a
partial port: the immediate multi-Node visibility precondition remains pending,
while every ambiguity/selection value assertion is retained. The local and
delayed-provider originals are complete faithful ports. All three untouched
native originals pass (XML: three runs, zero failures). The metadata
interop supplement uses the real C++ provider and native resolution/ServiceInfo
against a Rust provider; no service execution is needed in either direction.

The full local gate passes 89 Rust tests and 21 tooling tests, with 1362/1383
covered production lines, 2050/2115 regions and 71/71 changed executable lines.
Independent /root/sdformat review compared source, lifecycle/API behavior and
original assertions, then replayed all eight focused Rust checks, freshly rebuilt
real C++ metadata inspection in both directions and the three untouched native
originals (three runs, zero failures); all pass. Its loader setup failure was
retained and corrected by restoring the native dependency search path.

No protocol, dependency, threshold, exclusion, fixture or tolerance change occurs
in this slice. The two complete and one partial mappings leave 22 faithful ports,
one partial and 395 pending cases in the 418-case ledger. The matching immutable
staged gate remains required before publication, followed by hosted CI on the
published revision. No complete NET-01/NET-02 acceptance is claimed.
