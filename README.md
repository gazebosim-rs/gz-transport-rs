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
`known_topics()` also includes live same-process subscriptions and snapshots
eventually discovered remote subscriber-only interests; see the
[bounded subscriber contract](docs/NET-02-subscriber-lists.md) and
[four-Node topic information mapping](docs/NET-02-four-node-topic-info.md).
`known_topic_info()` returns known publisher and subscriber descriptor names for
one original topic path, with immediate same-process endpoint identity;
see the [bounded topic information contract](docs/NET-02-topic-info.md).
`advertised_services()` reports live local service topics;
`unadvertise_service()` withdraws their handlers and permits immediate recreation.
Already admitted requests remain available to drain and complete.
`service_provider_types()` preserves one type entry per known provider record,
including equal-signature providers; see [service inspection](docs/NET-01-service-info-ports.md).
`service_type_signatures()` snapshots known canonical service type pairs;
`resolve_service_types()` selects providers and waits for missing names with
explicit absence, incompatibility and ambiguity outcomes. These APIs use each
Node's worker and asynchronous discovery; see the [bounded contract](docs/NET-01-service-types.md).
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
`Subscription::recv_with_info_timeout()` returns an owned envelope with borrowed
partition, topic, descriptor and delivery-path metadata; raw receive remains
available. Matching same-process Nodes now receive direct copies with native
intra-process origin; wire peers remain remote. See [direct delivery](docs/NET-02-process-local-direct.md)
and [received metadata](docs/NET-01-message-info.md).
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
