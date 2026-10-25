<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Four same-process topic records

Pinned `NodeTest.AllTopicInfo` in `gz-transport/src/Node_TEST.cc:2695–2736`
creates four Nodes in one process. Two advertise `gz.msgs.Int32` on `/topic1`
and `/topic2`; two subscribe to `/topic1` and `/topic3`. The fourth Node
queries immediately. The original requires exactly three named records and
publisher/subscriber counts `(1,1)`, `(1,0)` and `(0,1)`. The unchanged native
original passes one run without failures or skips. A pre-fix private Rust
replay returned only two names; the first exact count assertion failed.

The Rust port retains those names and counts through `known_topics()` followed
by `known_topic_info()` for each original topic. This is a **partial mapping**:
the native method returns one vector snapshot with full `MessagePublisher`
metadata, while Rust makes separate bounded name and descriptor queries and
returns descriptor names only. The original asserts counts, not metadata
fields or cross-query atomicity. No new public API or general global-registry
claim is introduced.

Each worker keeps its existing wire process UUID, node UUID and publication
address. Private worker replies now carry those endpoint keys until after
the caller merges same-process registrations; only then are descriptor vectors
or topic names sorted and returned. The process index holds weak publisher
and subscriber leases plus a weak Node-stop flag. It is keyed by the canonical
partition and explicit message interface, multicast address and port.
Successful admission/enqueue creates an entry; it does not confirm multicast
discovery. Duplicate subscriptions of the same type on one Node count once;
distinct Nodes with the same descriptor remain separate. Remapping applies at
registration, while inspection uses the original query name. A dropped or
shut-down local lease overrides an old peer copy with the same wire identity;
retired keys remain for the peer's three-second freshness window and then
expire. Worker reply errors and timeouts remain authoritative; no registry
lock is held while waiting for a worker.

Supplemental controls cover duplicate and distinct endpoint identities,
multicast convergence without double counting, last-lease withdrawal,
partition and message-port isolation, remap, and explicit shutdown. Remote
discovery and subscriber-interest replies remain eventual; `known_topics()`
and `known_topic_info()` are inspection snapshots, not delivery guarantees.
Native Publisher ownership after Node destruction is a separate open gap:
the existing Rust explicit shutdown cancels its worker even when a handle
survives. This packet does not change that contract.

The later [Publisher ownership slice](NET-02-publisher-after-node-drop.md)
covers ordinary Node destruction separately; the partial four-Node descriptor
mapping and its metadata limits remain unchanged.

Independent source/API/assertion review passed on the frozen seven-file tree
`0d205871c583dd0796f4d55855f139dd8fb45955`. A separate raw export
passed the pinned native original 1/0, exact Rust port 1/0, both supplemental
checks 2/0, and all 166 Rust tests in the pinned serial environment. Removing
subscriber-only names failed the original count; collapsing equal descriptors
from distinct Nodes failed the supplemental multiplicity assertion. The
[review receipt](reviews/four-node-topic-info.json) records setup-only failures
separately. The author exact staged gate passed 2085/2124 production lines,
3142/3271 regions and 249/251 changed lines. Final metadata binding, the
normal commit hook and hosted qualification remain separate gates.
