# Owned message-publication metadata

`Node::known_topic_publishers(topic)` snapshots one owned `TopicPublisherInfo` per
known message publication. Records carry the fully qualified wire topic,
publication address, advertised control field, process/node identities, scope,
descriptor name and native effective rate options. The query qualifies the
original name without applying a second remap. Results are sorted by the record
fields and retain distinct providers with equal descriptors.

Native `MessagePublisher::SetFromDiscovery` interprets rate options before
exposing them. A false wire `throttled` flag means the effective rate is
`u64::MAX`, regardless of the raw rate. A true flag retains the raw rate; an
effective rate of `u64::MAX` still reports unthrottled. The projection follows
these rules. Stored advertisements, emitted discovery and publication delivery
remain unchanged. In particular, an ignored raw rate is not a delivery defect.

The advertised control field is copied exactly. Rust currently advertises an
empty control field; native C++ advertisements generally carry their control
endpoint. Returning this field creates no endpoint or routing guarantee. Rust
advertised process/node identities remain generated per worker, without native
process-global identity sharing.

This immediate worker snapshot reads active local advertisements and unexpired
message-channel peers of the correct topic and publisher kind. Remote records
follow existing scope/admission and three-second expiry rules. It does not merge
the process-local publication-name index used by `known_publisher_topics()`.
Another Node's name can therefore be immediately listed while this worker has
no corresponding metadata, especially for Process scope. Subscriber-only
interests and service records contribute no publication records.

The command queue and `Config::reply_timeout` bound coordination, with explicit
invalid-name, queue-full, timeout and shutdown errors. Queries consume no
registration permit and emit no discovery control. Call `topic_info(topic,
timeout)` when message discovery startup must precede inspection. That startup
boundary belongs to this worker; broader remote convergence, subscriber metadata,
provider liveness and delivery readiness remain separate work.

Owned strings remain readable after withdrawal, expiry or Node shutdown, and
owning a result keeps no Publisher or transport alive. An ordinary Node drop can
leave a publication alive through its existing Publisher lease; observation of
that publication ends after its lease retires and discovery catches up. Native
storage ordering, global cache behavior and timing are outside this snapshot.

Tests compare all nine fields against a fresh native TopicInfo query for a
default Vector3d publication, unlimited and zero rates, Host scope, two
same-descriptor publishers and absence. Eight native SetFromDiscovery controls
check both flags against zero, one, seven and maximum raw rates. These controls
also prove that projecting options leaves stored wire records unchanged.
Public and worker controls exercise original-name remapping, namespaces, all
local scopes, foreign partitions, a separate message port, subscriber-only and
service interests, Publisher survival and retirement, multiplicity, stale peers,
BYE, wrong channels/kinds, dropped receivers and bounded error outcomes.

Existing APIs, assertions, payload behavior, dependencies, gates and upstream
case dispositions are unchanged. The tests qualify correctness within this
scope; latency, throughput and allocation equivalence remain unmeasured.
