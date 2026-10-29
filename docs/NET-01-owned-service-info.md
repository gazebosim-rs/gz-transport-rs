# Owned service-provider metadata snapshots

`Node::known_service_info(topic)` returns an owned `ServiceProviderInfo` for each
known provider of the original qualified topic. Each record preserves the eight
admitted native `ServicePublisher` fields: fully qualified wire topic, address,
process UUID, node UUID, scope, socket ID, request descriptor and response
descriptor. The query qualifies the original name without reapplying remapping.
Registration already applied its remap and namespace.

The result is sorted by those fields, without collapsing equal type signatures.
Native storage order is not promised. The same filtered iterator serves the
existing type-only `service_provider_types` projection: active local services and
unexpired service-channel peers, with matching qualified topic and service record
kind. Type-only queries still clone only type names. Unique signature resolution
is unchanged. Existing peer admission, scope filtering and three-second expiry
remain authoritative; this API does not parse or admit discovery records.

This is an immediate worker snapshot. It does not wait for startup, send a
service request, invoke a handler, or add a discovery refresh. The command queue
and `Config::reply_timeout` bound coordination; invalid names, queue saturation,
timeout and shutdown retain explicit transport errors. Queries consume no
registration permit. Results contain at most the configured local registration
and peer capacities combined.

Call `service_list(timeout)` when service discovery startup must precede the
snapshot. Message `topic_info` preparation has a separate message-query contract;
it sends message subscriber discovery controls and is not a service enumeration
API. Both Rust channels progress within one worker, whereas native discovery
initialization is shared across Nodes and separately maintained by channel.
Neither startup nor an owned record establishes remote convergence, a live
provider, handler readiness or successful delivery.
Rust-owned services advertise worker-generated process/node identities; this API
does not unify them into native process-global identities.

Owning the record keeps its strings readable after withdrawal, expiry or Node
shutdown. It keeps no registration or transport alive. A later query may return
an empty list while an earlier result remains readable. Invalid names return an
error with no result; absent providers return an empty successful snapshot,
adapting native ServiceInfo's false/empty output boundary. General process-global
visibility, native ordering, topology and timing remain outside this slice.

A fresh native ServiceInfo query reports every field for held Int32/Int32,
Empty/Int32 and Int32/Empty providers, a Host-scoped provider, two equal-signature
providers and absence. Rust queries prepare service startup and compare the actual
owned vector with every native field, then observe retirement independently.
Public controls cover original-name remapping, namespaces, foreign partitions,
a separate service discovery port, message-only endpoints, absence, shutdown and
retained values. Worker controls check all fields, multiplicity, inactive local
handles, wrong topics/channels/kinds, stale records, BYE, dropped query receivers,
queue saturation, reply timeout and shutdown.

Existing tests, native originals, dependencies, wire behavior, coverage policy
and upstream dispositions are unchanged. These are correctness checks; no latency,
throughput or allocation equivalence is claimed.
