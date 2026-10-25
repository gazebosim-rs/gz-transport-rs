# Bounded discovered publisher and service listings

The new `Node::known_publisher_topics() -> Result<Vec<String>, Error>` and
`Node::known_services() -> Result<Vec<String>, Error>` methods each ask this Node's IO
worker for one snapshot of active local advertisements and remote publisher or
service advertisements already accepted by its discovery registry. Results are
sorted and deduplicated topic paths such as `/foo`, without the partition
prefix. The methods preserve `advertised_topics()` and `advertised_services()`
as local handle inspection.

The snapshot decomposes each wire name with `decompose_fully_qualified_topic`,
requires a leading `/` topic path, and compares its partition to the exact
canonical partition spelling that `qualified_name` emits for this Node (empty
or `/<partition>` after trimming outer slashes). Malformed or foreign-partition
records are excluded. Topic snapshots use only active local publishers and
channel-0 `MsgPub` peers; service snapshots use only active local handlers and
channel-1 `SrvPub` peers. Remote stamps must be younger than three seconds at
snapshot time. Existing scope admission still applies: PROCESS is local,
HOST comes from eligible interfaces, and ALL may be remote. Registration applies
topic remapping before advertising; a query does not reapply a remap. The
namespace is already part of the wire name. Multiple providers of one name
yield one list entry.

A local live handle is visible once its registration command is processed in
order. A dropped or explicitly withdrawn local handle is omitted by checking its
active flag before snapshot. A remote record appears only after discovery reaches
this worker; these methods do not wait for global convergence. Remote withdrawal,
BYE and expiry remove entries after the corresponding worker event. A worker
reply timeout returns `Error::Timeout`; a shut-down Node returns `Error::Shutdown`.
The peer registry's capacity limit and 3-second expiry remain part of the
existing discovery behavior.

The topic method includes published topics only. Native TopicList also includes
subscriber-only topics, which remains an explicit gap. Neither method claims a
process-global shared registry, immediate remote visibility, native second-call
latency, global completeness, or complete original TopicList/ServiceList
assertion equivalence. A real C++ publisher and service in separate
processes verify eventual remote visibility and shutdown disappearance. Local remap, local-only API separation,
foreign partition, wrong record kind, stale peer and empty partition controls
pin the subset. Exact native TopicList and ServiceList originals pass 2/0 as
contextual controls; retained ledger cases remain pending until every original
assertion can be mapped. The strict staged gate covers 1698/1708 production
lines, 2562/2622 regions and all 54 changed executable lines. The first gate
stopped on two new-code Clippy findings; both were corrected before this exact
staged pass.

Independent [source/API review](reviews/known-lists.json) approved this
bounded snapshot contract after private C++ cross-process and worker-filter
replays, all 146 Rust tests, native originals and a partition-bypass negative
control. The exact frozen staged gate passed with 1698/1708 lines, 2562/2622
regions and 54/54 changed executable lines. Normal commit and hosted checks
remain.
