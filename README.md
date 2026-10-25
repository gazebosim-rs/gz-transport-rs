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
One service per qualified topic is also allowed on each Node. Duplicate service
registration returns `AlreadyAdvertised` and preserves the original receiver; see
the [explicit native API disposition](docs/NET-02-service-ownership.md).
`advertised_services()` reports live local service topics;
`unadvertise_service()` withdraws their handlers and permits immediate recreation.
Already admitted requests remain available to drain and complete.
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
Graceful Node shutdown withdraws its subscription topics before process BYE.
Local counters expose drops and wire errors; service replies retry
mandatory routing until an explicit deadline. This is not complete upstream transport parity: statistics/auth/logs,
relay/all-interface discovery, the full CLI and other retained behaviors remain
listed in [the protocol contract](docs/PROTOCOL.md) and the 418-case
[upstream ledger](docs/upstream-tests.json).

```rust,no_run
use gz_transport::{Config, Node};
let node = Node::new(Config { partition: "example".into(), ..Config::default() })?;
let publisher = node.advertise("/greeting", "gz.msgs.StringMsg")?;
// Canonical StringMsg field 2; applications normally encode with gz-msgs.
publisher.publish(vec![0x12, 0x02, b'o', b'k'])?;
# Ok::<(), gz_transport::Error>(())
```

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
