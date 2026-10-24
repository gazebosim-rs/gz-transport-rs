# Native subscriber inspection slice

Pinned source: gz-transport41913b9dfc3dbce33ac5464ddcb427cc82450f26.
This slice makes a Rust subscriber visible to existing C++ topic inspection,
including a subscription with no publisher. It introduces no Rust query API.

`Discovery.hh`845–869 sends SUBSCRIBERS_REQ on message discovery. Native
`NodeShared::OnSubscribers`1294–1307 answers once per live handler using
`HandlerWrapper::Convert`1522–1555: local pub/sub address, empty control field,
local process/node UUIDs, ALL scope, and handler descriptor name. Rust now sends
these same SUBSCRIBERS_REP envelopes; an empty Rust type uses the native wildcard
`google.protobuf.Message`. Requests on service discovery and this worker's own
process requests are ignored. Reply work remains bounded by registration capacity.

Native `NodeShared::UnsubscribeHelper`2134–2189 broadcasts END_CONNECTION only
after that node's last handler for the topic is removed. Rust lease cleanup now
does the same with the generic type and empty control field; duplicate lease
drops produce one withdrawal. Native Discovery removes its remote subscriber
entry, and native publisher registration also stops for the same process/node.
No new retained remote-subscriber state is introduced in Rust.

Two original cases in test/integration/twoProcsPubSub.cc are mapped:
`TopicListSubscriberOnly`385–412 and `TopicListRemovesUnsubscribed`417–454.
Both original executables pass (two cases,0 failures); the gate executes them.
Their C++ observer is retained in tests/cpp_peer.cc and the auxiliary subscription
is Rust. Discovery is initialized before subscription; original Int32 descriptor,
/subscriber_only and /unsub_test names, present5s/4s deadlines, absent8s deadline,
TopicInfo success, zero publishers, one subscriber and live-process withdrawal
observations are retained. Pipes replace fixed auxiliary lifetime/unsubscribe
delays. Extra checks cover exact-once listing, descriptor/control fields, generic
subscriptions and duplicate lease retention. The node remains alive until the
native observer confirms absence, so BYE/process expiry cannot satisfy removal.
Original2014 attribution is retained in both assertion port sources.

Two UDP unit checks observe exact canonical response fields, dead-handler
exclusion, service-channel filtering and last-lease withdrawal without races.
The real C++ observer test passes for both original topics and the wildcard
supplement. The strict full gate passes44 Rust tests and21 tooling tests,
1165/1189 production lines,1735/1805 regions and34/34 changed executable lines.
Independent /root/sdformat source/assertion review approved the frozen slice and
reran both exact native originals, both UDP units and the real C++ observer test;
all pass. The matching strict staged hook remains mandatory before publication.

Remaining scope includes receiving/querying remote subscriber lists from Rust,
process-global multi-node discovery identity, full discovery APIs and the other
retained cases. This slice does not complete NET-01 or NET-02.
