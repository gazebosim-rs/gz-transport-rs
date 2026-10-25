# Local delivery with a remote subscriber

The retained `twoProcPubSub.LocalSubscriberReceivesOnceWithRemoteSubscriber`
original runs with a local and a remote `gz.msgs.Vector3d` subscriber on
`/local_and_remote`. After both subscriber processes are visible, it publishes
ten messages whose x coordinates are 0 through 9. The local subscriber must
receive each once, in order, within the original 5 second wait and 10 ms poll,
followed by a 300 ms quiet period without another local copy.

`original_local_subscriber_receives_once_with_remote_subscriber` runs a real
C++ remote subscriber and the Rust local subscriber and publisher. It retains
the ten payloads, ordering, wait and quiet checks. A unique partition keeps
other tests out of this fixture. The READY signal and bounded inspection poll
establish two subscriber descriptors before publishing.

This is a **partial** mapping. Native waits for two distinct subscriber
process UUIDs; two topic-info descriptors are a fixture-specific readiness
surrogate. Neither original nor port asserts delivery to the remote
subscriber. This case does not qualify arbitrary process identity, discovery
timing, or a general delivery guarantee. The exact native original was
replayed separately; the Rust port runs in the strict staged gate. The independent
[review](reviews/local-remote-once.json) reproduced the original, selected
port, and an extra-publish failure. Final protected hook and hosted CI are
separate publication gates.
