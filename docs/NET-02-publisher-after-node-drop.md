<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Publisher ownership after ordinary Node destruction

An ordinary `Node` drop retires its subscriptions, services and pending
requests. A surviving `Publisher` owns a private lease on that Node's existing
worker, so it can publish on its original qualified topic and remain visible to
same-domain inspection. Dropping the last Publisher releases its registration,
stops and joins the worker, and sends Unadvertise before process BYE. Explicit
`Node::cancel()` and `Node::shutdown()` retain their cancel-all contract;
Publishers then return typed shutdown errors, while surviving Subscription and
Service receivers drain already queued values before returning Shutdown. The wire UUID, sockets,
bounded command queue and public API are unchanged.

The pinned native `Node.cc` and `PublisherPrivate` ownership chain leaves
publisher registration alive after ordinary Node destruction. A source-bound
installed-library probe observed `Valid`, `Publish` and `TopicList` through
10.5 seconds after destroying the Node, then withdrawal after the last
Publisher dropped. A real native subscriber also received a Rust publication
after Rust Node destruction. These are bounded observations; a list entry or
successful `Publish` call does not guarantee delivery.

The new Rust tests cover publisher survival, actual C++ subscriber delivery,
Node-owned receiver drain and retirement, rejected service replies, pending
request retirement, explicit cancel/shutdown, last-lease join and concurrent
inspection. The existing interop and worker tests remain intact. The worker
retains no ownership cycle or socket outside its one IO thread. The
[independent review](reviews/publisher-lifetime-v2.json) replayed native
ownership and private Rust interop, exercised an old-drop mutant, and checked
controlled discovery and native subscriber delays. With each new readiness
check present the relevant test passed; removing that check in a private
source made it fail under the same payload attempts and request deadlines.

An earlier instrumented interop run failed with status 101 under the former
staged verifier, which discarded its child output. Its exact assertion is
**unclassified**. The retained failed log, binary and source patch bind that
limit. The later ten complete interop passes and the two controlled readiness
regressions support this source, but do not identify the earlier failure.
The revised source's first exact strict staged gate passed 2165/2205
production lines, 3256/3389 regions and 89/89 changed lines. Final metadata
binding, normal hook and hosted qualification remain separate acceptance
steps at this review point.

This slice does not claim process-global native `NodeShared` ownership,
universal publication timing, full AllTopicInfo metadata, or that inspection
proves a connection or delivery. Those retained contracts remain open.
