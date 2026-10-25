# Local topic ownership inventory

`Node::advertised_topics` and `subscribed_topics` return sorted unique topic paths
owned by that Node, without partition prefixes. They inspect bounded weak lease
metadata directly; they issue no worker command and perform no discovery wait.
A subscription interest appears even without a known publisher, matching native
NodeShared::SubscribeHelper. Remapping and namespace qualification happen once
when the handle is created, so inventories expose the actual qualified path.

Dropping the final publisher or subscription handle removes its inventory entry.
Duplicate receivers share one path. Failed queue admission, malformed names,
capacity exhaustion and poisoned registry locks cannot leave an inventory entry
or leak a permit. Cancellation returns `Shutdown`. Registry snapshots serialize
with registration, while concurrent lease retirement is observed through the
existing atomic active flag. These APIs are local ownership views, not a global
topic list or a transport acknowledgement.

Subscribe worker admission now performs ordinary retirement cleanup before a
replacement registration and skips a command whose lease was already retired.
This bounds inactive entries under continuous command traffic and orders the old
END before new registration. Each topic owns one ZeroMQ filter while every
receiver keeps its own descriptor/registration. Final retirement removes that
filter once; unread receiver buffer semantics are unchanged. The shutdown trace fixture still tests retired, known filters;
its setup now admits those filters before retiring their handles. Original END
and BYE assertions are unchanged.

The independently reviewed original ports retain PubWithoutAdvertise, PubSubSameThread and
PubSubSameThreadGenericCb, including every ordinary/generic helper assertion.
Owned receipt replaces callback scheduling: execution flags are set only after a
real message is received and processed. Each publish is single-shot; readiness
uses native 10000ms/10ms defaults. Generic processing uses the maintained dynamic
message decoder and successful text conversion containing `5`; it also checks
MessageInfo's partition-free topic assertion via `Message::topic_path`.
No complete protobuf text-format compatibility follows from that fixture.

Author evidence: 57 unit tests, all three proposed ports plus two existing
threaded ports, a strict real C++ namespaced alias/lifetime comparison, and the
three untouched native originals (XML three runs, zero failures) pass. The strict
full gate passes 116 Rust and 21 tooling tests, 1528/1550 production lines,
2323/2389 regions and 51/51 changed lines. Independent /root/msgs review used a
fresh private source/target and passed all 57 units, five publication ports
(the three new cases and two existing threaded cases), the real C++ local
inventory comparison and all three exact native originals (XML 3/0). It approved
the complete selected helper mappings, one-filter ownership and preserved
shutdown assertions. The [authored receipt](reviews/local-topics.json) is copied
unchanged. Exact staged and hosted gates remain required before publication.

Independent review used a production Worker/XPUB probe to prove duplicate socket
filters survived final receiver retirement in the inherited admission path. That failing
behavior and source are preserved in reviewer artifacts. The fix admits one
socket filter per topic. Its deterministic regression requires actual socket
subscribe/unsubscribe acknowledgments, retained delivery for the remaining typed
receiver, and no new multipart delivery after final retirement. Every receiver
and descriptor registration remains represented; this is a resource-lifecycle
fix, not a claim to port native internal socket ownership.

Full TopicList/ServiceList, including their native second-call timing assertions,
and remote TopicInfo subscriber-cache lifetime behavior remain pending. Publisher
membership deliberately remembers different metadata and cannot substitute for
that cache. Explicit unsubscribe, immediate multi-Node process-global sharing,
scope controls and performance acceptance are separate retained contracts.
