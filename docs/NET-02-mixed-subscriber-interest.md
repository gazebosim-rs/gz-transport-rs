<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Mixed subscriber interest and individual retirement

Pinned `twoProcPubSub.PubSubTwoProcsMixedSubscribers` is in
`test/integration/twoProcsPubSub.cc:574`. It creates a valid Vector3d `/foo`
publisher, observes no initial connections, starts a subscriber process, waits
with native 10-second / 10-millisecond defaults and publishes `(1, 2, 3)` ten
times with a 500-millisecond sleep after each publication.

The unchanged subscriber auxiliary actually creates four callbacks, despite a
stale three-subscriber comment: a legacy typed callback, a typed raw callback and
two individually owned typed subscribers. It waits at most 100 iterations of
100 milliseconds for all four flags, asserts all four, resets every flag and
unsubscribes only the first owned subscriber. After one second, only that flag
must remain false. It resets all flags, unsubscribes the Node from `/foo`, waits
another second and requires all four flags false. Every callback preserves the
three `EXPECT_DOUBLE_EQ` values; the raw callback also checks the type name and
successful parsing.

The first Rust test preserves the publisher body and runs that unchanged native
auxiliary. Fallible factory and publication results translate native successful
returns. `connection_snapshot()` is the explicit nonblocking known-membership
read translation, not delivery or liveness. The ten publications and all waits
remain unchanged. Native child exit status is additionally checked, rather than
discarding a failed child through the original join wrapper.

The reverse-direction test uses a small SDK helper retaining the same publisher
assertions, native `waitUntil` and ten-publication schedule. Its `READY` line only
places the initial no-subscribers assertion before Rust registrations begin.
Rust mirrors the complete auxiliary body, holding its first typed and raw owned
handles through Node-wide unsubscribe to translate native Node-owned legacy
registration lifetime. The other two handles map the individually owned native
subscribers. It does not insert a flush, callback barrier or new readiness wait.
That variant drops the handles and Node immediately after the second quiet
check, preserving the auxiliary's return/destruction boundary before joining
the publisher's remaining calls. A separate stronger variant retains that Node
until the publisher finishes, distinguishing logical withdrawal from owner
destruction or BYE. Neither variant invents native process-global identity.

Callback entry counters increment before validation, and persistent error state
records wrong values, raw decode/type failure or unexpected local origin outside
the dispatcher's panic-catching boundary. This prevents caught panics or a false
callback flag from hiding an invocation. The positive expected values retain
GoogleTest's four-ULP bound, including explicit four-versus-five boundary checks;
no general numeric algorithm is introduced. Supplemental counter checks require
no further entry into retired callbacks and retain quiet state through the
remaining original publications. They earn no additional original case credit.
After that unchanged schedule the native publisher also reports its actual
`HasConnections()` value; the Rust test requires zero cached remote interest.
This supplemental observation checks logical withdrawal rather than merely
local callback suppression. It does not establish physical socket retirement,
global identity, convergence or general delivery guarantees.

Both directions use a child-only UUID partition and held distinct discovery
ports. The full unchanged original replaces its partition but inherits these
port overrides; no host environment is changed. Diagnostic output is bounded,
and child watchdogs kill and reap unfinished peers. The reverse helper's setup
line has a five-second receive bound; child joins have a fifteen-second watchdog
after the Rust assertions, and the full unchanged original has a fifty-second
outer process-group watchdog. Diagnostic collection is capped at 64 KiB. These
test cleanup/setup bounds do not replace the original readiness or callback
waits. The helpers and Rust tests
retain their original OSRF years and Apache notices. There are no production,
schema, protocol, default, dependency, consumer-pin or NSS-control changes.

Author qualification passes all four final controls in both debug and release:
the two original-direction ports, the positive value-boundary check and the
retained-Node withdrawal supplement. Canonical linkage was checked for the new
helper; the unchanged full native original, all 42 tools and workspace all-target
Clippy pass. Root's independent fresh accepted-d01 archive plus exact six-path
overlay and owned target passes all four controls in debug, with a freshly built
helper linked to all three canonical Gazebo libraries. Its separate unchanged
native original passes under the same held-port and process-group bounds. The
complete source, predicates, cleanup, four-ULP checks and literal versus
retained-Node distinction were reviewed with no findings. The reviewer does not
claim release, tooling or full-gate replay.

Exactly this original is a conditional mapping: accepted 126 faithful, 7 partial
and 285 pending becomes 127/7/284 only after ordinary full staged coverage and
exact hosted success with downloaded source evidence. Supplements earn no extra
credit. No production defect or performance claim is established. Whole NET-02
and global identity remain open.
