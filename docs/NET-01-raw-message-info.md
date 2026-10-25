# Same-Node raw message metadata assertion ports

The selected original cases are `NodeTest.RawPubSubSameThreadMessageInfo`,
`NodeTest.RawPubRawSubSameThreadMessageInfo` and
`NodeTest.PubRawSubSameThreadMessageInfo`. Each retains a valid typed Int32
publisher on `/foo`, data 5, successful subscription, the original
`HasConnections` readiness predicate with the 10000ms/10ms bound, and two
independently reset single publications. There is no repeated-send substitute or
extra discovery barrier.

The first case retains an exact Int32 subscription and raw encoded publication.
The second uses raw encoded publication and a wildcard receiver. The third uses
`publish_typed` and a wildcard receiver. Native `SubscribeRaw` defaults to the
generic descriptor; an empty Rust type filter preserves that selection.
`PublishRaw`'s explicit canonical type comes from the typed publisher binding.
No arbitrary raw descriptor override is claimed.

The shared receive helper preserves `cbInfo` and `rawCbInfo` topic `/foo`,
normalized partition, successful whole-buffer protobuf parse, data 5 and
intra-process true observations. Exact canonical type and receipt counters are
additional observations in the raw callback cases. The entire received byte
slice maps to native pointer plus size. Only the test adaptation decodes it;
transport still forwards opaque bytes without decoding or reencoding.

Fresh owned state and bounded receive completion replace global callback state
and polling, following the preceding reviewed same-Node metadata mapping. The
original full 2014 Apache/OSRF notice remains with the ports. These cases do not
claim callback executor/mutex scheduling, mutable standalone MessageInfo,
process-global two-Node origin, broad PublishRaw type-override behavior or whole
NET-01/NET-02 acceptance. The lambda metadata case remains pending separately.

The unchanged native originals are required by every full gate. Source/header,
helper and native binary hashes plus XML are retained in
`artifacts/raw-message-info-next/proof.json` and `original.xml`. All three
proposed ports pass private prequalification with the matching exact Msgs pin,
including all 138 default-parallel checks and a freshly built native peer. Three
private harness setup failures (native library path, executable mode and the
environment-clearing child peer path) are retained separately from that result.
The author strict gate passes 138 Rust and 22 tooling checks with unchanged
1641/1645 production lines and 2460/2509 regions; no executable production lines
change in this assertion-only slice.

Independent [assertion review](reviews/raw-message-info.json) approves the three
selected original mappings. Its raw 83-blob baseline export and five frozen
hashes were verified; a fresh private source/target and native peer passed all
138 checks in both default-parallel and required serial modes. The three exact
native originals passed XML three runs, zero failures, with source/header/helper
and binary hashes matching the producer proof. The lambda and broader raw
callback/process-global/type-override gaps remain explicit. Exact final staged
and hosted qualification remain required.
