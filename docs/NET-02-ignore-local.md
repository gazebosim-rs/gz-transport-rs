# Ignore local messages per subscription

`SubscribeOptions` defaults to local delivery. A caller can set
`ignore_local_messages` and pass the option to `Node::subscribe_with_options`.
The existing `subscribe` API uses the default. The filter applies only to a
matching registration's direct delivery from its owning Node; other
registrations on that Node retain their own policy. The ignored registration
still participates in subscriber discovery and publisher interest.

The pinned `NodeTest.IgnoreLocalMessages` original uses `/foo`, `gz.msgs.Int32`
with data 5, three publications separated by 100 ms, and a zero-count local
callback after opt-in. The Rust port keeps those observations and checks the
empty receiver. It is a faithful mapping of this one Node case.

The pinned `twoProcPubSub.IgnoreLocalMessagesWithRemoteSubscriber` original
uses a separate subscriber process, a publishing and ignored Node, and a
second local witness Node. The Rust port keeps ten `Vector3d` publications,
ten witness deliveries followed by a 300 ms quiet interval, and zero ignored
deliveries. A real C++ subscriber participates. Native readiness checks
subscriber process UUIDs; the Rust fixture can only poll three known endpoint
descriptors. The C++ child callback is empty and its process wrapper only
joins, so neither case asserts remote receipt. At this review checkpoint Rust
Nodes used separate workers and wire delivery. The later bounded
[direct-delivery slice](NET-02-process-local-direct.md) changes same-process
message origin, while native process-UUID readiness and full process-global
ownership remain unqualified. This two-process original remains a partial mapping.

Both pinned originals pass separately and the exact Rust assertions run in
the strict staged gate. The source review and private filter-removal control
are recorded in the [independent receipt](reviews/ignore-local.json). The
remaining NET-02 assertions and protocol cases remain open.
