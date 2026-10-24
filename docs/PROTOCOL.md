# Pinned wire contract

Upstream: gz-transport main `41913b9dfc3dbce33ac5464ddcb427cc82450f26`.
Canonical schemas: gz-msgs `e1800c3e8d058ce7358b393e5f7aba6dd912920f`.

Discovery (`src/Discovery.hh`, especially DispatchDiscoveryMsg, SendMsg,
RecvDiscoveryUpdate) uses IPv4 UDP multicast 239.255.0.7, message port 10317,
service port 10318. Datagrams contain a native-endian uint16 body length followed
by canonical `gz.msgs.Discovery` protobuf bytes. Maximum datagram size is 65507.
Version 10 is ordinary ZeroMQ; statistics-enabled peers use 110. Mismatched
versions and malformed delimiters/protobuf are discarded. Unix HOST-scoped
advertisements and withdrawals are accepted only from an eligible local IPv4
interface or any127.* address, matching native Discovery.hh. Interface inventory
is captured before worker startup using safe nix bindings, preserving native
index deduplication, IPv4/up/multicast filters and loopback fallback. No system
interface query runs while publishing or processing a simulation step. Ambiguous
lossy interface names return Unsupported. Other platforms currently recognize
only the configured interface and loopback; full topology qualification remains
pending. Heartbeats are 1000ms;
peers expire after 3000ms. ADVERTISE/SUBSCRIBE/UNADVERTISE/HEARTBEAT/BYE are
implemented. Delayed subscribers elicit advertisement responses. Rust subscribers
register with NEW_CONNECTION: their process/node UUID replaces the advertised
identity and msg_pub.ctrl carries the publisher process UUID. C++ requires this
registration before serializing publications (`NodeShared::OnNewConnection`).

Message-channel SUBSCRIBERS_REQ receives one SUBSCRIBERS_REP per live local
subscription, even without a publisher. Replies contain the local pub/sub
address, process/node UUIDs, ALL scope, an empty control address and the subscribed
type; a wildcard uses native `google.protobuf.Message`. Dropping the last local
subscription lease for a topic sends a generic END_CONNECTION, including when
no publisher exists. This removes native subscriber-list entries while the Rust
process remains alive. Service-channel list requests receive no reply. Receiving
and querying remote subscriber lists from Rust remains pending.

Peer identity separates message/service channels and includes address alongside
process/node/topic, matching the native independent TopicStorage instances.
Duplicate advertisements retain first metadata and cannot trigger new connection
or registration side effects; capacity-rejected peers cannot register either. Each decoded discovery packet
refreshes known process activity on its own channel. Unknown opcodes are ignored
after that activity update instead of being treated as advertisements. Withdrawal removes all
addresses for the matching process/node/topic on that channel; BYE removes that
channel's process entries. Pub/sub connections use only message discovery and
RPC connections only service discovery. The global peer capacity remains bounded.

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

A response descriptor of `gz.msgs.Empty` identifies a native one-way request.
Providers enqueue the request without sending a reply. `Request::expects_response`
returns false and `respond` simply consumes it. `Node::request_oneway` bounds
provider discovery/send retries with the supplied timeout, then releases the
pending slot after local queue or ZeroMQ handoff. Success acknowledges this
handoff, not callback execution; no application-level acknowledgement exists.

The library uses zmq 0.10.0 / zmq-sys 0.12.0, which builds unmodified libzmq 4.3.4.
The independently built C++ reference links system libzmq 4.3.5. Native dependency
code is outside handwritten Rust coverage; no owned Rust FFI or unsafe code is
used. Unix identity lookup uses unmodified nix0.31.3 safe system bindings.
Preserving native ZeroMQ is deliberate: replacing it with an unrelated
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
frame count and checks cancellation before each native frame. Cancellation is
terminal: unread multipart tails are discarded when the worker closes its
sockets, and cancellation itself is not counted as a malformed wire message. Publishers report command queue saturation. Slow subscription/service
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
HOST advertisement controls and actual multi-host qualification; process-wide
multi-node discovery identity;
END_CONNECTION / SUBSCRIBERS_REQ/REP bookkeeping; relayed discovery/GZ_RELAY;
IPv6 and all-interface routing; automatic interface selection, exceptional NSS
fallback and Windows default identity; topic throttling and
wire statistics version 110; authentication; complete
legacy CLI grammar; SQLite log record/read/replay and performance qualification.
Each remains retained in the 418-case runtime ledger and non-gtest inventory.

Explicit supported environment capture and pending modes are documented in
[NET-02 configuration](NET-02-config.md).
