<!--
Copyright (C) 2026 Open Source Robotics Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Statistics transport framing

`Config::statistics_wire` opts into the pinned native statistics transport
profile. It defaults to false. Explicit environment capture enables it only for
`GZ_TRANSPORT_TOPIC_STATISTICS=1`; another present value disables it, and an
absent value preserves the explicit base configuration. No environment is read
implicitly by a Node. The selected profile is immutable after Node construction.
The qualified metadata ABI is Linux x86_64; other targets reject this opt-in
profile. Its frame limit must accommodate the 16-byte metadata frame.

Both message and service discovery use version 110 in this profile, while the
default remains version 10. Receiving discovery checks the selected version
before updating peer activity or dispatching records. The profile also belongs
to each admitted local message and service domain. Local topic inspection,
pub/sub delivery, service resolution, queued handoff and inline callbacks cannot
cross profiles through the weak owner indexes. RPC request and response frames
keep their existing layout, limits, deadlines and provider-owned reply lane.

A statistics publication retains the original four frames and appends one
16-byte frame: native-endian unsigned 64-bit publication timestamp followed by
unsigned 64-bit sequence number. The pinned native layout has size 16, alignment
8 and offsets 0 and 8. The timestamp comes from the existing qualified Linux
monotonic clock, truncated to milliseconds at the worker's wire attempt. The
application payload stays opaque, including empty payloads and bytes that are
not valid protobuf. The raw receive API and captured message information are
unchanged; this slice does not introduce another metadata inspection API.

The native sender advances its topic sequence in `NodeShared::Publish`, rather
than on every admitted publication. The statistics profile follows that boundary:
no matching registered remote subscriber means no wire attempt and no consumed
sequence; direct local delivery remains available. Rust reserves the metadata
and sequence before its nonblocking multipart call; a failed call consumes that
attempt and increments the existing send-failure counter. This is an explicit
bounded-send policy: native increments after its first three frame sends, so
native multipart failure-point and rollback equivalence remain unqualified.
A metadata clock or range failure fabricates no
timestamp or frame, consumes no sequence and increments that counter; existing
local delivery still proceeds. Publication return values retain command-admission
semantics. Unsigned sequence wrap is defined.

History belongs to the worker's advertised wire-process identity and survives
same-worker unregister/re-register, descriptor changes and ordinary Node drop
while a Publisher retains its worker. It is held through existing Sender
ownership, including orphan publishers and the worker. At most `capacity`
distinct qualified publication topics may be successfully admitted to that
identity's statistics history. Active registration slots still retire normally,
but history is neither evicted nor reset: a new distinct topic after exhaustion
returns `QueueFull` and increments `queue_drops`. Reusing an admitted topic
preserves its counter. Failed registrations consume no history, including queue
saturation and failures after command admission. The history lock protects only
admission and contains no user callbacks; packet emission uses its shared atomic
counter without taking that lock. The default profile allocates no history.

This bounded history is an explicit Rust capability. The native history is an
unbounded process-global topic map: measured sequences continue across two
native Nodes and destruction/recreation of all Nodes in that process. Rust Nodes
retain their already documented independent worker UUIDs and addresses; a new
worker has a new wire identity and starts its own sequence history. This slice
does not claim native global singleton continuity across Rust Node recreation.
Registered interest is not delivery acknowledgement, and profile isolation is
not access control for an already known socket address. The default profile's
existing best-effort wire policy is unchanged.

The receiver requires exactly five frames in the statistics profile and exactly
four in the default profile. It checks the metadata length before cloning that
frame, drains rejected multipart messages, counts each malformed message once
and makes progress to the next message. Cancellation keeps the existing rule:
stop at a native frame boundary and retire the socket rather than reuse an
abandoned multipart tail. Metadata has no separate unbounded cache, and existing
application queue/drop accounting remains. Invalid metadata tests target Rust;
no malformed metadata is sent to the native SDK's unchecked cast.

Native characterization covers both matching profiles and both mismatches,
actual four/five-frame bytes, monotonic stamps, per-topic sequences and ordinary
callback delivery. A successful native publish before any remote subscriber did
not advance the first observed sequence. Native unregister/re-register, a second
Node and last-Node recreation produced consecutive sequences 0 through 4, with
actual identity/address records retained in the private investigation.

Author controls exercise the production receiver, exact configuration transition,
clock/error/wrap cases, mode domains, retained history, rollback, shutdown and
orphan ownership. Actual C++ and Rust pub/sub and RPC pass in both directions
across the four mode pairs, including opaque/empty payloads and same-worker
sequence continuity. Independent review used a fresh source export, Cargo target
and rebuilt canonical SDK peers: 149 library checks and 46 public/native checks
pass, including the final error-accounting test. The reviewer also rebuilt and
replayed the native-only sequence lifecycle and compared the successful framing,
version and ownership policies with the pinned source. The failure test injects
missing sequence ownership and a nonblocking socket failure, not a clock failure.
Strict normal staged coverage and exact hosted qualification remain required.
No ledger rows gain credit from this slice.
Aggregate `TopicStatistics`, metric publications, native
global identities and broad statistics or NET-01 parity remain open; operational
`Node::statistics()` counters retain their existing meaning.
The separate [SampleStatistics value](NET-01-sample-statistics.md) implements
scalar sample accumulation without changing this transport profile.
