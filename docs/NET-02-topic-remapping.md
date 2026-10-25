# Bounded single-hop topic remapping

This library slice adds owned `Config::topic_remaps`, `add_topic_remap` and
`topic_remap`. The immutable per-Node configuration applies one substitution by
exact raw key before namespace/partition qualification. `/foo`, `foo`, `foo/`
and `FOO` are separate keys. A mapped destination can itself be another key;
chains, cycles and self-maps are never followed transitively. Canonical message
descriptor names and application payload bytes are unchanged.

Pinned reference: NodeOptions.cc 99–140; Node.cc 929 and 1287;
detail/Node.hh 149, 295, 480 and 606. Native substitution applies to publisher
and service advertisement, subscriptions and requests. Publisher handles retain
the qualified mapped topic, so publishing and dropped-lease withdrawal use the
same identity. Alias collisions participate in existing per-topic admission.

Native ServiceInfo/ResolveServiceTypes use the original topic. UnadvertiseSrv
also qualifies the original argument (Node.cc 820–826), despite first computing
an unused remapped variable. This observable behavior is retained explicitly:
withdraw the actual advertised name or drop its Rust lease. With `/from` → `/to`
and `/to` → `/third`, advertising `/from` creates `/to`, querying `/from` finds no
provider, querying `/to` finds it, withdrawing `/from` leaves it live, and
withdrawing `/to` removes it. An actual native C++ lifecycle probe and equivalent
Rust assertions preserve this boundary; it is not a generalized mapper applied
to every Node method.

Both source and destination use native nonempty topic grammar, including byte
length and ASCII-space rules. Failed setters preserve prior mappings. Duplicate
sources return `AlreadyRemapped`; capacity saturation returns `QueueFull`.
The map is bounded by Config.capacity, including direct public-map construction
validated before any worker or socket exists. Native C++'s unrestricted options
map has no overload result; this is an explicit bounded Rust configuration API.
Full qualified-length failure occurs when an operation qualifies its destination,
matching native separation of options grammar and topic creation.

Tests compare 144 source/destination combinations against the real native
NodeOptions methods, including duplicate and absent lookup behavior. Supplemental
cases cover exact raw keys, cycles/self-maps, rejected mutation, direct-map
validation, qualification overflow, alias ownership, command saturation and
cancellation. Real C++ peers exercise remapped publication, subscription, service
and request in all four directions, plus the original-name inspection/withdrawal
lifecycle. No readiness sleep substitutes for received data or completed replies.

Two complete original mappings are independently reviewed: NodeTest.WrongTopicRemap
and AdvertiseRemappedTopicTwice. Their untouched native originals are mandatory
in the gate. Other
remapping/scope/list/HasConnections cases remain pending. In particular, native
list operations' less-than-2ms assertion is not replaced with an IO-worker timing
claim. No global discovery, full CLI, PROCESS advertisement controls, complete
NET-01/NET-02 or performance acceptance is claimed.

The full local gate passes 97 Rust tests and 21 tooling tests, with 1391/1412
covered production lines, 2104/2169 regions and 33/33 changed executable lines.
Its first run rejected an unqualified unit-test replacement in the ledger after
the Rust checks passed. Correcting only that metadata and rerunning the full gate
passed; the initial failure is retained. No coverage threshold or assertion changed.

Independent /root/msgs review compared all original assertions, API ownership and
native operation-specific behavior, then replayed all eight focused Rust checks
in its own target and both untouched native originals (XML: two runs, zero
failures). This includes all 144 native options pairs, real C++ communication in
four directions and the original-name inspection/withdrawal lifecycle. Source and
binary hashes were verified. The exact [review receipt](reviews/topic-remapping.json)
is copied unchanged. The ledger now has 24 faithful ports, one partial and 393
pending cases of 418. The matching immutable staged gate remains required before
publication, followed by hosted CI on the published revision. No full packet or
performance acceptance is claimed.
