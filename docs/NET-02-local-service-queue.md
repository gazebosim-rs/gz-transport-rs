# Admitted service queues across local Nodes

`begin_request` and its blocking `request` wrapper can hand a request to another
live admitted local service in the same explicit service domain before its
multicast advertisement reaches the caller. They still enqueue a caller worker
command and never invoke user code inline. A callback service's existing owned
dispatcher consumes that queue. The separate `request_callback` and
`request_oneway` inline callback contracts remain unchanged.

The existing weak worker index supplies partition, interface, multicast address
and service-port isolation. Qualified topics preserve namespace and remap
rules; message ports do not split a service domain. Matching requires both raw
descriptor strings. A matching own receiver keeps priority, including when its
Serve command has not yet reached the worker. A full own queue never falls
through to a foreign receiver. Selection of one matching local queue is final
on successful handoff; the worker then marks the request sent and does not
send another request over ZeroMQ. Unmatched requests keep their existing
pending/discovery/wire retry path and original deadline.

Each `Service` owns one queue payload; the registry holds only Weak to it.
That payload itself holds Weak to the ORIGINAL admitted channel sender, shared
by Arc through the existing Serve command and worker service state. It adds no
second queue or owned request cache. Worker cleanup releases its strong sender at the existing withdrawal boundary.
With no in-flight sender lease, a retained Service observes the same disconnected
receiver after admitted requests drain. An
observed worker identity cannot retain a queue payload or make a stopped,
dropped or expired service lease callable. Spawn/commit failure drops the
Service payload; a parked callback dispatcher cannot execute that failed
registration, even if a queued request was already admitted.

Weak upgrade creates a temporary sender lease for one nonblocking handoff.
Final owner/stop/active/deadline checks run after upgrade and outside registry
locks. That last live selection is the admission boundary: it may finish its
single transfer after concurrent retirement, like an already admitted request.
A later selection observes retirement and cannot reopen the queue or retry the
same selected handoff. An operation paused with an upgraded sender can delay
receiver disconnect until it exits. There is no hard concurrent disconnect
latency claim; the lease stays within one worker operation, contains no user
code and never escapes into the registry or pending state. The controlled
upgrade/withdrawal test resumes after cleanup, denies handoff at the final
checks, drops the temporary lease, then observes Shutdown with Service held.

Caller command and pending capacities, request deadlines, response abandonment
and shutdown draining stay in the existing worker path. Provider request queue
saturation returns typed QueueFull and increments the provider's queue_drops;
caller pending saturation increments the caller's counter. A receiver that
disconnects during an already-selected try_send also produces QueueFull, matching
the legacy own-queue path; Shutdown here describes quiescent Service receive,
not a new request-admission error. Caller payload
validation remains before command admission; the provider's configured payload
limit is also checked before local handoff, returning InvalidFrame to the
pending result. This explicit bounded local error differs from a remote peer
silently rejecting an oversized wire frame. No queue admission acknowledges
user execution. Empty-response completion still means successful queue or wire
handoff, with no response packet.

Replies use the provider's existing bounded response lane and mandatory ZeroMQ
routing. The route copies the caller worker's ACTUAL bound response address,
response socket ID, qualified topic, Node ID and request ID in the same order
as received wire requests. Provider response limits and caller socket-size
validation remain unchanged. Late replies are ignored after the existing
pending deadline; abandoning a response retains only the original bounded
pending entry until reply/deadline. Already admitted requests can still drain
and respond after unadvertise; provider shutdown rejects their response.
Queued callback errors/panics retain the existing empty/false wire result and
provider routing_errors count, rather than gaining a Rust error channel.

Public provider inspection remains worker-only; queue routing does not insert
foreign records into metadata views. Raw wire frames, per-worker process IDs,
dependencies, callback lifetime guards and existing assertions remain. No
service scope option API or additional upstream credit is included.

A fresh native C++ control registers a PROCESS-scoped Int32 callback and proves
actual metadata plus data-5 response from a second local Node, then observes
withdrawal. Separate author characterization also verifies All/Host remote
visibility and Process remote absence with actual callback counts. These are
reachability observations, not native callback scheduling or full process-global
identity equivalence. Focused private/public tests cover real mandatory replies,
no duplicate request connection, own priority, queue/pending bounds, domains,
payload/error paths, withdrawal, shutdown, expired/abandoned/late replies and
failed callback spawn.

Independent `/root/sdformat_owner` reviewed the complete source, ownership,
selection/retirement interleaving, constructor-only test adaptations and final
contract. Fresh reviewer source/target and a freshly built C++ peer passed 134
candidate library tests and 31 public/native checks under proper loopback
permissions, compared with accepted cf8f2ec's 127 library and 24 retained public
checks. The final seven bounded manual controls also passed. Initial sandbox
EPERM/abort results remain failed setup, not valid source regression evidence.
Root independently approved the production ordering and bounded contract.
Author handoff-removal and sender-retention negatives fail the corresponding
sent-flag and quiescent disconnect assertions; these are separate author
observations. No scope API or upstream credit is added. Normal strict staged
qualification from scratch and exact hosted success remain publication gates.
