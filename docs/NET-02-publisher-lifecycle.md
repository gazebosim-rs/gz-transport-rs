# Publisher topic ownership and recreation

Bounded NET-02 task: preserve native per-node publisher topic ownership and
drop/recreate ordering for simulator sensor lifecycle. Owned files are runtime,
error API, test support/interop peer, mandatory native test filter and this
contract/ledger. No service ownership or process-global Node sharing is claimed.

Pinned Node.cc1282–1340 rejects another active publisher on the same qualified
topic in one Node, including different message types. PublisherPrivate destruction
withdraws its topic synchronously. The prior Rust implementation accepted
duplicate leases and could withdraw a surviving or newly recreated publisher
after its advertisement.

Implemented synchronous, bounded per-Node topic reservation at advertise; release
uses the existing atomic lease flag, without locks or socket waits in Publisher
Drop. Serialize concurrent advertise reservations. Reject duplicates explicitly.
Before a queued replacement is advertised, withdraw the retired identity; skip
advertisements whose lease already expired before the worker handled them.

Advertise serializes its registry check with a mutex, outside the simulation
critical step. Publisher Drop still touches only existing lease atomics; it does
not acquire that mutex, enqueue work or access sockets. A poisoned registry fails
explicitly. Registration/queue failure drops the provisional lease and leaves no
topic reservation. The registry prunes dead weak leases and remains bounded by
the existing shared registration capacity. This corrects previously permissive
Rust duplicate admission to the native contract; it adds `Error::AlreadyAdvertised`.

Evidence: original same/different-type, relative and namespaced duplicate
assertions; different Nodes remain independent; failed queue admission releases
ownership; concurrent same-topic attempts admit exactly one; deterministic UDP
withdraw-before-replacement trace including an already-dropped queued lease;
real C++ subscriber receives samples before and after topic recreation. Remapped
topic assertions remain pending because remapping is unsupported. Retain all
418 catalog cases and existing source/fixture hashes. Peer review and unchanged
strict staged/hosted gates remain required; no full packet completion.

Full local verification passes62 Rust tests and21 tooling tests,1216/1241
production lines,1816/1885 regions and14/14 changed executable lines. Four
unchanged native originals pass; the real C++ subscriber test establishes
readiness through observed samples and keeps the same subscriber alive across
immediate publisher recreation. No fixed readiness sleep is used. The matching
staged hook remains required before publication.

Independent /root/msgs review approved the exact source and assertion mapping,
including concurrency, rollback and atomic-only destruction. It independently
ran3 publisher-filtered library tests,4 original Rust ports, the freshly compiled
C++ recreation test and4 exact native originals; all pass. It also verified the
source/native binary/peer and failing-before-correction proof hashes. The four
original duplicate cases are faithful ports; remapping remains pending. The
author's full gate measurements above are distinguished from that independent
targeted replay. Exact receipt: [publisher lifecycle review](reviews/publisher-lifecycle.json).

The later [ordinary Node destruction slice](NET-02-publisher-after-node-drop.md)
adds private Publisher-held worker ownership. The atomic-only Drop description
above records this earlier duplicate-admission slice; last-Publisher Drop now
also stops and joins the retained worker after releasing its lease.
