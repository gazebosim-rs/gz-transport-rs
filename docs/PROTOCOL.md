# Pinned wire contract

Upstream: gz-transport main `41913b9dfc3dbce33ac5464ddcb427cc82450f26`.
Canonical schemas: gz-msgs `e1800c3e8d058ce7358b393e5f7aba6dd912920f`.

Discovery (`src/Discovery.hh`, especially DispatchDiscoveryMsg, SendMsg,
RecvDiscoveryUpdate) uses IPv4 UDP multicast 239.255.0.7, message port 10317,
service port 10318. Datagrams contain a native-endian uint16 body length followed
by canonical `gz.msgs.Discovery` protobuf bytes. Maximum datagram size is 65507.
Version 10 is ordinary ZeroMQ; statistics-enabled peers use 110. Mismatched
versions and malformed delimiters/protobuf are discarded. Heartbeats are 1000ms;
peers expire after 3000ms. ADVERTISE/SUBSCRIBE/UNADVERTISE/HEARTBEAT/BYE are
implemented. Delayed subscribers elicit advertisement responses. Rust subscribers
register with NEW_CONNECTION: their process/node UUID replaces the advertised
identity and msg_pub.ctrl carries the publisher process UUID. C++ requires this
registration before serializing publications (`NodeShared::OnNewConnection`).

Ordinary pub/sub ZeroMQ frames (`NodeShared::Publish`, RecvMsgUpdate):

1. Fully qualified topic bytes (`@/partition@/namespace/topic`, or `@@/topic`).
2. Publisher TCP address.
3. Unmodified protobuf bytes.
4. Canonical protobuf descriptor full name (`gz.msgs.StringMsg`, no slash).

A subscriber matches the whole topic and descriptor name after ZeroMQ's prefix
filter, preventing `/foo` from accidentally accepting `/foobar`. An empty Rust
subscription type accepts all advertised types. Same-node publications use the
same typed-byte contract without an unnecessary socket hop.

Services use ROUTER sockets, not REQ/REP. The requester sends nine frames:
provider routing identity, topic, response receiver address, response receiver
routing identity, requesting node UUID, per-request UUID, protobuf bytes,
request descriptor name, response descriptor name. The provider receives the
requester's connection identity in place of the first routing frame. Replies
contain response receiver routing identity, topic, node UUID, request UUID,
response bytes, ASCII `1`/`0`. The receiver sees the provider's connection identity
in the first frame. Both topic and node/request identity must match before a
pending request is removed. The responder connects back to the requester's
advertised response receiver. Mandatory routing retries until the socket's
ZMTP handshake completes or the explicit reply_timeout expires (default 5s).

The library uses zmq 0.10.0 / zmq-sys 0.12.0, which builds unmodified libzmq 4.3.4.
The independently built C++ reference links system libzmq 4.3.5. Native dependency
code is outside handwritten Rust coverage; no owned Rust FFI or unsafe code is
used. Preserving native ZeroMQ is deliberate: replacing it with an unrelated
Rust-only messaging bus would break existing clients.

# Ownership, cancellation and overload

`Node::new(Config)` creates sockets then transfers their exclusive ownership to
one named IO thread. The library installs no runtime. Config is explicit and its
default interface is loopback; callers select an externally reachable IPv4
interface when needed. No environment variables are implicitly captured.
`Node::cancel(&self)` is safe across threads. `shutdown(&mut self)` and Drop cancel
and join the worker. Socket linger is zero; polling is nonblocking, with a 2ms idle
interval. A simulator does not await a socket while stepping.

Command queues, subscriptions, service request queues, outstanding requests,
registration permits, retained peers, response connections, and delayed replies
are bounded by `capacity`; native queues use the explicit positive send/receive
high water marks. Message frames are bounded by `max_message_bytes`.
Multipart decoding drains malformed messages while retaining only the expected
frame count. Publishers report command queue saturation. Slow subscription/service
receivers drop new arrivals when full; ZeroMQ high water marks can also drop
publications as in the reference. Service overload can yield QueueFull or timeout.
Applications choose their own response execution context. Request.respond reports
acceptance into the bounded command queue. Node.statistics exposes queue drops,
malformed frames, native send failures and expired reply deadlines; these local
counters do not claim the unobservable native PUB high-water-mark losses or the
version-110 wire statistics contract. Dropping a subscription,
service, or publisher releases its registration, removes filters or advertises
UNADVERTISE. Request deadlines expire without unbounded retries; shutdown drops
pending reply senders so blocked callers receive Shutdown.

# Remaining retained scope

This is a first interoperable NET-01 slice, not complete transport parity. Pending:
HOST scope across multiple hosts; process-wide multi-node discovery identity;
END_CONNECTION / SUBSCRIBERS_REQ/REP bookkeeping; relayed discovery/GZ_RELAY;
IPv6 and all-interface routing; legacy default host/user/interface selection; topic throttling and
wire statistics version 110; authentication; oneway Empty services; complete
legacy CLI grammar; SQLite log record/read/replay and performance qualification.
Each remains retained in the 418-case runtime ledger and non-gtest inventory.

Explicit supported environment capture and pending modes are documented in
[NET-02 configuration](NET-02-config.md).
