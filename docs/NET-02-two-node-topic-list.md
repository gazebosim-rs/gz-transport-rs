# Two-Node process-local topic listing

Pinned `NodeTest.TopicList` in `src/Node_TEST.cc:2605–2632` creates two Nodes in
one process and advertises Int32 publishers on `topic1` and `topic2`. It
requires the first TopicList result to have two entries, clears that result,
requires the second to have two entries and bounds only the second call below
two integer-truncated milliseconds. The unchanged native original passes
one run with no failures or skips. It does not assert the entry names.

Before this change, a private raw-HEAD Rust probe saw only `/topic1` on the
first `known_topics()` call and both topics on the second. Separate Rust
workers exchange announcements asynchronously; native Nodes use a shared
in-process discovery view. The assertion port now uses two Rust Nodes with
the same explicit configuration and retains the original two advertisements,
both size checks, clear and second-call bound without a discovery poll.

The process-local weak publication index records validated, remapped topics
after an advertisement is accepted for enqueue. A live entry requires both
its publisher lease and owning Node to remain active. Its key uses canonical
partition plus the explicit message discovery interface, multicast address
and port; namespace changes the registered topic path and is not a key.
`known_topics()` and `known_publisher_topics()` merge these entries with a
successful worker snapshot, then sort and deduplicate. No registry lock is
held across a worker command or reply wait. `advertised_topics()` remains
per Node. Wire process UUIDs, socket ownership, subscriber interests and
service discovery are unchanged.

Supplementary controls cover matching canonical partitions with different
namespaces, remapped registration, duplicate names, different partitions
and message ports, publisher drop/recreation, a shut-down Node with a live
Publisher handle and concurrent admission/query. Native custom probes confirm
same-partition namespace visibility, partition isolation, remap and duplicate
dedup. Native `PublisherPrivate` unadvertises when its last handle drops,
while the native Node destructor does not withdraw live publisher handles.
The probe listed a publisher after destroying its Node and removed it after
dropping that handle. Rust's explicit Node shutdown contract invalidates its
worker and handle; native Publisher-after-Node-drop lifetime remains an open
compatibility gap outside this listing slice.

The later [Publisher ownership slice](NET-02-publisher-after-node-drop.md)
covers ordinary Node destruction while preserving explicit shutdown. The
two-Node listing assertion and its partial ledger scope above are unchanged.

This port covers the original first and second list counts and second-call
latency for two live same-process publishers. Process-local subscriber-only
interest, immediate per-topic descriptor snapshots, cross-process discovery
timing, global completeness, origin metadata and delivery readiness remain
separate. A list entry does not prove `has_connections()` or successful
delivery. The independent [review](reviews/two-node-topic-list.json) approved
the exact two-Node assertion port and bounded publication index after private
native/Rust replay and an old-runtime negative control. The source-frozen
strict staged gate passed 163 Rust tests and 22 tooling checks with 1875/1901
production lines, 2838/2930 regions and all 58 changed executable lines
covered. The ledger remains partial for the stated compatibility limits.
Final staged verification, normal hook and hosted qualification are pending.
