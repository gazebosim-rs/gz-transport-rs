# Owned subscriber metadata beside existing typed views

`Node::known_topic_subscribers(topic)` returns sorted owned subscriber records
from this worker's live subscriptions and admitted remote replies. The
`TopicSubscriberInfo` alias uses the same nine native MessagePublisher fields and
effective option normalization as [publication metadata](NET-01-owned-publication-info.md).
The query qualifies the original topic, with no second remap, discovery refresh,
registration permit or process-local name-registry merge.

Native remote storage identity is topic/process/address/node. `TopicStorage`
keeps its first record for that identity even when later control, scope,
descriptor or rate fields differ. This differs from MessagePublisher value
comparison and from the existing Rust type-aware interest view. The new canonical
cache retains one complete admitted wire record per native identity and updates
only its timestamp on duplicates. It adds no synthetic remote fields.

The existing interest cache and descriptor/name APIs retain all their assertions
and type-aware identity. For example, first-node Int32 and StringMsg replies plus
a second-node Int32 reply still produce three legacy descriptor entries; the
canonical metadata cache holds two records with the first descriptor for each
wire identity. Neither view silently changes the other's contract. Only the
canonical cache owns complete metadata payloads; the existing view keeps its
keys and timestamps.

Each cache independently admits at most `Config::capacity` remote entries. A
packet can refresh or enter one view while the other rejects a new identity.
`queue_drops` increases once for a packet rejected by either or both cache limits.
Expired entries release admission capacity. Records expire after the existing
three-second reply freshness boundary, and queries exclude stale records even
before cleanup. Generic END removes all addresses for its topic/process/node;
BYE removes the process. Native process-wide heartbeat-based expiry timing is
outside this per-record freshness profile.

Local subscriptions use the same metadata builder as emitted replies. Native
local-handler conversion also uses default advertisement options independently
of subscription rate: scope All, empty control and effective unlimited rate.
The unchanged Rust wire reply's false flag and zero raw rate normalize to that
native unlimited value. An empty subscription descriptor becomes
`google.protobuf.Message`. Local handler merging follows native MessagePublisher
comparison, including control/scope/type but ignoring rate options. Repeated
same-type local handlers collapse; distinct local types remain distinct.

This is a bounded worker snapshot. `Config::reply_timeout` bounds coordination,
with explicit invalid-name, queue-full, timeout and shutdown errors. Call
`topic_info(topic, timeout)` when message startup and subscriber-list refresh must
precede inspection. Initialization and cached records establish neither remote
convergence nor provider liveness. Another Node's process-local registration is
visible here only after wire discovery. Rust advertised identities remain per
worker rather than native process-global identities.

Owned strings remain readable after withdrawal, expiry and Node shutdown. They
keep no subscription or worker alive. The snapshot does not invoke handlers or
change payload filtering, raw wire data, the existing interest view or publisher
connection membership. Native storage ordering, broad global discovery and
performance equivalence remain unqualified; no upstream case gains credit.

The durable C++ storage probe checks control/scope/type/rate distinctions,
first-record retention, distinct addresses/nodes/processes and node/process
retirement. Private tests check the separate views, bounded admission and drop
observability, duplicate refresh, stale records, END/BYE, native local comparison,
normalization and query errors. Public controls cover names, remaps, namespaces,
owned retirement, other record roles and discovery domains. A fresh native
TopicInfo query compares every field for typed and generic subscribers, zero and
seven subscription rates, two same-type processes and absence. Existing typed
identity assertions and the accepted publication metadata checks remain intact.
