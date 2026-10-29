# Gazebo Transport in Rust

An independent Gazebo ZeroMQ transport implementation with an owned IO worker,
bounded queues, explicit cancellation, and canonical protobuf discovery.

The first NET-01 slice interoperates with the pinned real C++ Gazebo Transport
main reference in both directions for pub/sub and request/reply. It includes
partition/namespace qualification, delayed startup, reconnect, raw typed bytes,
request correlation/timeouts, local delivery, dropped-handle withdrawal, and
shutdown. One publisher per qualified topic is allowed on each Node; duplicates
return `AlreadyAdvertised`. Dropping the lease permits immediate recreation;
the worker withdraws the old identity before advertising its replacement.
Ordinary Node destruction retains its worker while a Publisher handle survives;
that handle can continue publishing, while Node-owned subscriptions, services
and requests retire. Explicit `cancel()` and `shutdown()` still cancel all
handles. See the [bounded ownership contract](docs/NET-02-publisher-after-node-drop.md).
One service per qualified topic is also allowed on each Node. Duplicate service
registration returns `AlreadyAdvertised` and preserves the original receiver; see
the [explicit native API disposition](docs/NET-02-service-ownership.md).
`known_publisher_topics()` includes accepted live publishers from other Nodes
in this process and already-discovered remote advertisements in the same
explicit message domain and partition. `known_services()` retains its bounded
worker snapshot; see the
[bounded discovered listing contract](docs/NET-02-known-lists.md).
`service_list(timeout)` waits for per-worker discovery startup before taking that
snapshot; see the [bounded service startup contract](docs/NET-02-ready-service-list.md).
`known_topics()` also includes live same-process subscriptions and snapshots
eventually discovered remote subscriber-only interests; see the
[bounded subscriber contract](docs/NET-02-subscriber-lists.md) and
[four-Node topic information mapping](docs/NET-02-four-node-topic-info.md).
`known_topic_info()` returns known publisher and subscriber descriptor names for
one original topic path, with immediate same-process endpoint identity;
see the [bounded topic information contract](docs/NET-02-topic-info.md).
`known_topic_publishers()` snapshots owned publication addresses, identities,
control metadata and effective options; see [owned publication metadata](docs/NET-01-owned-publication-info.md).
`known_topic_subscribers()` preserves native remote keep-first records alongside
the existing type-aware views; see [owned subscriber metadata](docs/NET-01-owned-subscriber-info.md).
Actual subscriber UUID snapshots also support the retained
[local delivery with a remote subscriber](docs/NET-02-local-remote-once.md) and
[ignore-local remote fixture](docs/NET-02-ignore-local.md), alongside their
unchanged earlier descriptor variants. These selected predicates do not unify
Rust worker identities into native process-wide identities.
Admitted physical publication links survive discovery silence within a separate
bounded route table; snapshot freshness stays unchanged. See the
[route lifetime contract](docs/NET-01-publication-route-lifetime.md), including
the retained explicit withdrawal parity gap.
The `topic_info(topic, timeout)` startup query and prepare-phase
`Publisher::refresh_connection_snapshot(timeout)` have a separate
[bounded readiness contract](docs/NET-02-ready-topic-info.md). Callers provide an
explicit deadline; startup is per worker and does not imply remote consensus.
`advertised_services()` reports live local service topics;
`unadvertise_service()` withdraws their handlers and permits immediate recreation.
Already admitted requests remain available to drain and complete.
`service_provider_types()` preserves one type entry per known provider record,
including equal-signature providers; see [service inspection](docs/NET-01-service-info-ports.md).
`known_service_info()` snapshots the owned provider address, identities, scope,
socket and descriptor fields; see [owned service metadata](docs/NET-01-owned-service-info.md).
`service_type_signatures()` snapshots known canonical service type pairs;
`resolve_service_types()` selects providers and waits for missing names with
explicit absence, incompatibility and ambiguity outcomes. Inspection remains a
worker snapshot; the resolver also uses admitted local declarations immediately.
See the [bounded inspection contract](docs/NET-01-service-types.md) and
[local resolver contract](docs/NET-01-local-service-resolution.md).
`begin_request()` admits a response request and returns a one-shot
`PendingResponse`; `request()` retains its existing bounded wait through that
handle. The [wrong-request contract](docs/NET-02-service-wrong-request.md) checks
asynchronous admission without a response and synchronous Timeout for a
Vector3d request sent to a local Int32 service. The
[wrong-response originals](docs/NET-02-service-wrong-response.md) retain the
same observations for Int32 and inputless requests expecting Vector3d replies.
The [mixed-response originals](docs/NET-02-service-mixed-response.md) then require
successful Int32 responses while the wrong-response observer remains active.
The [cross-process wrong-type originals](docs/NET-02-remote-service-wrong-types.md)
retain mismatched request/response observations against a live native echo peer.
The same contract covers an inputless request expecting a mismatched response
from a live native fixed Int32 provider.
The [remote mixed-response cases](docs/NET-02-remote-service-wrong-types.md#mixed-wrong-and-good-remote-responses)
then require good replies while the wrong-response observer remains live.
`serve_callback()` and `request_callback()` add bounded owned response callbacks.
Exact live local matches complete inline; remote and queued replies use owned
dispatchers. See the [callback lifetime and admission contract](docs/NET-02-response-callbacks.md).

An owned dispatch adaptation covers three native member-service cases,
including request before local service registration. New direct callback
variants preserve the two local immediate response checks; the earlier queued
variants remain. See the [selected member-service mappings](docs/NET-02-class-member-service.md).
Two local service lambda originals now have direct variants through the
accepted callback API, preserving their immediate response flags while retaining
the earlier adapters. See the [selected lambda mappings](docs/NET-02-service-lambda.md).
An owned [one-way callback service](docs/NET-02-oneway-callback.md) invokes a
matching callback on the same Node or another Node in the explicit local service
domain before `request_oneway` returns. Existing own receivers retain priority;
explicit queued and wire requests use a bounded owned dispatcher.
`Config::add_topic_remap` installs bounded, exact-key substitutions before Node
creation. Registration, subscriptions and requests apply one substitution;
service inspection and explicit withdrawal use the advertised name, matching
native behavior. See [topic remapping](docs/NET-02-topic-remapping.md).
`Publisher::has_connections()` snapshots matching subscriber registrations with
bounded worker coordination. Native remembered membership can outlive a process
disconnect and does not acknowledge delivery; see [publisher inspection](docs/NET-01-publisher-inspection.md).
`Publisher::connection_snapshot()` reads the same bounded interest boundary from
an atomic cache without waiting for the worker; see the [nonblocking snapshot
contract](docs/NET-01-publisher-interest-snapshot.md).
`AdvertiseOptions` selects Process, Host or the existing All default through
`advertise_with_options` and `advertise_typed_with_options`. Matching local Nodes
retain direct delivery and bounded readiness. Native discovery admission and
Process heartbeat metadata follow the [publication scope contract](docs/NET-01-publisher-scope.md).
The same options select an unsigned message rate; the native unlimited default is
`u64::MAX`, while zero suppresses payloads successfully. Typed and raw calls on
one Publisher share the limiter. `throttled_update_ready()` reports eligibility
without consuming an interval, using the qualified native Linux monotonic epoch.
See the [bounded rate contract](docs/NET-01-publisher-throttle.md) for error,
queue, callback, clock and concurrency limits.
`AdvertiseOptions` also implements `Display` with the native default stream
bytes; see the [formatting contract](docs/NET-01-message-options-format.md).
Graceful Node shutdown withdraws its subscription topics before process BYE.
`advertised_topics()` and `subscribed_topics()` inspect this Node's live lease
ownership without network waits; see [local topic inventory](docs/NET-01-local-topics.md).
`unsubscribe()` retires every matching subscription on this Node using the
original qualified name. Already-admitted owned receiver buffers remain drainable;
see the [retirement contract](docs/NET-01-unsubscribe.md).
`SubscribeOptions::set_ignore_local_messages(true)` suppresses direct delivery
to that registration from its owning Node. Ordinary `subscribe()` retains local
delivery, and ignored registrations remain discoverable; see the
[bounded ignore-local contract](docs/NET-02-ignore-local.md).
`SubscribeOptions` also selects a per-registration rate shared across local and
wire admission; see the [subscription rate contract](docs/NET-02-subscription-rate.md).
`Subscription::recv_with_info_timeout()` returns an owned envelope with borrowed
partition, topic, descriptor and delivery-path metadata; raw receive remains
available. Matching same-process Nodes now receive direct copies with native
intra-process origin; wire peers remain remote. See [direct delivery](docs/NET-02-process-local-direct.md)
and [received metadata](docs/NET-01-message-info.md). Two native owned
member-callback originals map to a [joined Rust dispatch adaptation](docs/NET-02-class-member-callback.md)
without a public callback registration API. The publisher's bounded
echo history and process-local fanout have a reviewed [cost update](docs/NET-02-publication-cost.md).
Local counters expose drops, wire errors and direct-routing errors; service replies retry
mandatory routing until an explicit deadline. This is not complete upstream transport parity: statistics/auth/logs,
relay/all-interface discovery, the full CLI and other retained behaviors remain
listed in [the protocol contract](docs/PROTOCOL.md) and the 418-case
[upstream ledger](docs/upstream-tests.json).
Service responses enter a separate bounded worker queue so a publication
command flood cannot consume their admission slots; see the
[response-lane contract](docs/NET-02-service-response-lane.md).

```rust,no_run
use gz_msgs::stringmsg::StringMsg;
use gz_transport::{Config, Node};
let node = Node::new(Config { partition: "example".into(), ..Config::default() })?;
let publisher = node.advertise_typed::<StringMsg>("/greeting")?;
publisher.publish_typed(&StringMsg { data: "ok".into(), ..Default::default() })?;
# Ok::<(), gz_transport::Error>(())
```

Typed publication rejects a different canonical descriptor before encoding.
Use `publish(bytes)` for opaque forwarding; typed reencoding does not retain
unknown fields. See [typed publication](docs/NET-01-typed-publication.md).
The [wrong-type publication contract](docs/NET-01-wrong-type-publish.md)
checks that a mismatched value is rejected before a local subscriber callback,
then that a correctly typed Int32 value reaches it.
The [wrong-type subscription contract](docs/NET-01-wrong-type-subscription.md)
checks successful Vector3d publication with an Int32 subscriber and an immediate
false callback flag.
The [two-subscriber wrong-type contract](docs/NET-01-wrong-two-subscribers.md)
checks that only the matching Int32 subscriber receives data 5 across two Nodes.
Two [remote wrong-subscription originals](docs/NET-01-remote-wrong-subscription.md)
retain the fixed 1500 ms false callback observations against the unchanged native
Vector3d publisher.
The same contract covers [matching and generic remote subscribers](docs/NET-01-remote-wrong-subscription.md#matching-and-generic-remote-subscribers)
across three Nodes, including the original callback flags and topic metadata.

Package `gz-transport` 0.1.0 depends on the immutable reviewed `gz-msgs` commit,
not simulator/Bevy or developer-only workspace paths. Rust 1.94.1 and dependencies
are pinned. `zmq` uses native libzmq; this is intentional wire compatibility.
Default configuration binds loopback, with an explicit IPv4 interface for remote
use. There is no hidden async runtime or implicit environment capture. Opt in through
`Config::from_env()` or apply an injected lookup with `with_environment`; pending
wire modes return explicit Unsupported errors.

Run `scripts/bootstrap.sh`, then
`python3 scripts/verify.py verify --profile zeromq-linux` on the native reference
runner. The same authoritative runner verifies an isolated staged snapshot in
the pre-commit hook and CI. It requires strictly >95% handwritten production
line **and** region coverage, no missing files, a real C++ peer, and original
mapped assertions. Missing native infrastructure fails the gate.
See [testing and evidence](docs/TESTING.md).

The canonical cross-repository plan, test policy and compatibility contract live
in [gz-sim-rs](https://github.com/gazebosim-rs/gz-sim-rs/blob/main/docs/PLAN.md).
New Rust APIs preserve observable Gazebo wire behavior; no C++ ABI is claimed.

License: Apache-2.0. Ported assertion provenance and fixture hashes are retained.

Copyright (C) 2026 Open Source Robotics Foundation. Newly authored code is
Apache-2.0; see [NOTICE](NOTICE) and [LICENSE](LICENSE). Imported notices, years
and third-party licenses are preserved.
