# Received message metadata

`Subscription::recv_with_info_timeout` consumes the existing bounded receive queue
and returns `ReceivedMessage`. `info()` borrows topic, partition and type strings
from its raw message and exposes the captured delivery-origin flag. The envelope
owns its payload independently of the subscription and Node lifetime. Moving it
through `into_message()` preserves the payload allocation and all bytes. Its
read-only dereference exposes the existing three-field `Message`; no mutable
metadata projection, callback executor or additional queue is introduced.

`recv_timeout` retains its original `Result<Message, Error>` API and consumes the
same queue, moving out the raw value. Timeout, shutdown, saturation/drop counters,
filter matching and receiver retirement behavior are unchanged. Already-owned
buffers can drain after retirement. Raw payloads remain opaque, including invalid
protobuf bytes; neither receive path decodes or reencodes them. Existing raw
struct literals and relay code remain supported.

The topic is the unqualified path, such as `/space/topic`. The partition is the
inner qualified-name segment, such as `/metadata`, with its leading slash and
without either `@` marker; the empty partition remains empty. Native
MessageInfo::SetTopicAndPartition and TopicUtils::DecomposeFullyQualifiedTopic
preserve that segment. Differential checks cover empty, slash-normalized and
UTF-8 partition/topic names. Type is the exact advertised or accepted frame
descriptor; a wildcard receiver does not rewrite it.

Only the two actual delivery sites assign origin. Direct publication through the
same Node's worker records true; receiving four ZeroMQ frames records false.
The flag is captured before queueing, so it remains stable after retirement,
recreation and Node shutdown. No address, loopback, UUID or descriptor heuristic
infers origin. A real multipart message carrying the worker's own advertised
address still records false and retains opaque bytes.

At this historical review checkpoint the origin boundary was narrower than
native process-global sharing. Native two Nodes in one OS process reported true,
whereas independent Rust Nodes used their own workers and reported false. The
later [direct-delivery slice](NET-02-process-local-direct.md) retains the
original differential inputs and now requires true from both. Actual C++/Rust processes
on loopback report false in both directions; lifecycle coverage retains a first
message, retires and recreates a subscription, then requires a fresh data 6 value
before checking metadata after shutdown. Repeated remote publication is a bounded
supplement, not a substitute for the original same-Node sequencing.

The reviewed NodeTest.PubSubSameThreadMessageInfo mapping retains a valid Int32
publisher and subscription on `/foo`, data 5, the original HasConnections readiness
predicate and 10000ms/10ms bound, two independently reset single publications,
execution flags and all cbInfo topic/data/partition/type/origin assertions. Fresh
owned Node/test state replaces callback globals; bounded receive completion
replaces callback polling. The complete original 2014 Apache/OSRF notice remains
in the port. The unchanged native original passes XML one run, zero failures
with source/binary/helper hashes recorded and is required by every gate.
Independent review approves this selected assertion/API mapping; it does not
complete native process-global IntraProcess behavior, raw callback parity,
mutable MessageInfo construction/setters, global query timing or NET-01/NET-02.
The other retained original dispositions remain unchanged.


The author full gate passes 135 Rust checks and 22 tooling checks, including the
unchanged native original and all earlier mandatory baselines, real C++ peers,
existing raw relays and lifecycle assertions. Strict production coverage is
1641/1645 lines and 2460/2509 regions, with 45/45 changed executable production
lines and no exclusions. All recorded inputs match the checked working files.
Independent /root/msgs_recovery review raw-verified the baseline and all eleven
frozen inputs, reran all 135 Rust checks with a fresh native peer and separate
source/target, and reran the unchanged native original (one run, zero failures).
A reviewer supplement checked wildcard/remap metadata, alternating raw/envelope
receives, opaque bytes and allocation identity after Node shutdown. The unchanged
[receipt](reviews/message-info.json) approves the selected same-Node original
mapping and preserves the native process-global origin gap. Exact final staged
and hosted qualification remain required.
