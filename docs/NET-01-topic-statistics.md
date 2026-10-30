<!--
Copyright (C) 2026 Open Source Robotics Foundation
SPDX-License-Identifier: Apache-2.0
-->

# Owned bounded topic statistics

`TopicStatistics` is an owned value with explicit reception times and bounded
sender history. It reuses three `SampleStatistics` values for publication
periods, reception periods and message age. It does not read clocks, attach to
a Node, retain received wire metadata or format/publish metric messages. Node's
existing operational `Statistics` counters and sender-side wire sequence history
keep their existing contracts.

The compatibility source is
[`src/TopicStatistics.cc`](https://github.com/gazebosim/gz-transport/blob/41913b9dfc3dbce33ac5464ddcb427cc82450f26/src/TopicStatistics.cc)
at pinned Transport `41913b9dfc3dbce33ac5464ddcb427cc82450f26`. Native reads
steady-clock milliseconds inside Update. Rust deliberately takes that time as
the first `update_at(received_milliseconds, sender, publication_stamp, sequence)`
argument. Callers must supply reception/publication milliseconds from the same
epoch. Clock acquisition, failure policy and runtime receive translation are
separate future work. No hidden timing or readiness wait is introduced.

`new(max_senders, max_retained_sender_bytes)` bounds both sender cardinality and
the total retained name bytes. Native has an unbounded map. A new sender that
exceeds either Rust budget returns `Error::QueueFull` before changing scalar
samples, previous timestamps, drop count or history. Checked byte addition also
fails before mutation. Known senders can continue when either budget is full,
without reallocating their admitted key. History is not evicted or expired.
Zero cardinality rejects all updates; a zero-byte budget still permits an empty
sender if a cardinality slot is available. Limits are explicit and immutable.

Sender identity is exact bytes via `AsRef<[u8]>`, including empty values,
embedded NUL and invalid UTF-8. There is no case folding, Unicode normalization
or UUID parsing. Each clone deep-copies the sequence map and all scalar values,
timestamps, counters, limits and byte accounting. Returned scalar getters are
independent Copy values. Cloning creates another explicitly owned bounded value;
there is no global map or shared history.

## Native update semantics

Previous publication and reception timestamps are topic-wide. Sequence history
is per sender. If the previous publication timestamp is zero, the call skips
all timing/drop samples but still retains the current timestamps and sender's
sequence. A later zero publication stamp re-enters that sentinel boundary on
the next call without clearing history.

Otherwise the samples are wrapping unsigned64 publication delta, reception
delta and reception-minus-publication age, converted to `f64`. Accumulation
uses the accepted [native scalar value](NET-01-sample-statistics.md), including
population deviation and the native extrema sentinels. A sequence other than
the previous sequence plus one increments the dropped count once: this counts
discontinuity events, not missing sequence distance. Duplicates and backward
sequences are events too. For an unseen sender, native map lookup supplies
previous sequence zero; its first sequence1 is consecutive, sequence0 is not
once the topic has a previous nonzero publication stamp. Sequence arithmetic,
timestamp subtraction and drop counts wrap explicitly as native unsigned64.

The raw native study retains 46 getter snapshots across first/zero-stamp,
multi-sender, gap, duplicate/backward, wrap, identity and independent-copy
families. Additional native byte identities distinguish `a`, `a\0` and `ff`.
Native `TopicsStatistics.Constructor` and `TopicsStatistics.DroppedMsg` pass
unchanged before the Rust ports. The constructor's complete original scalar,
publication and reception assertions are retained. The drop case preserves
sender `foo`, stamps1..5, sequences0,1,3,5,6 and drop assertions0,1,2,2.
Explicit generous Rust limits and fixed reception times translate native's
unbounded constructor and implicit clock setup; reception time cannot alter
those original drop predicates. Every admitted call must succeed.

## Verification boundary

The pure C++ helper invokes the actual native value, never Nodes or sockets.
Across 58 fixture commands it emits all getters after constructor, update,
save and copy-restore operations. Rust compares all drop/sample counts, all
publication getter bits and every empty constructor field exactly. Native's
hidden reception time is not injected, so nonempty reception/age numbers are
recorded and parsed but are not falsely compared with explicit Rust times.
Separate deterministic controls check all reception/age numeric getters using
known intervals, backward/wrapping timestamps and the zero-stamp boundary.
This is not a claim of runtime clock, delivery or wire integration equivalence.

Private controls test both admission bounds before all mutation, cumulative byte
budgets, known-sender progress/key reuse, binary identities and deep-copy
independence. Synthetic byte-counter overflow and dropped-counter wrap check
source-defined boundaries without impossible allocations or 2^64 updates.
No old test body, tolerance, dependency or runtime path changes. Independent
fresh source/target and canonical helper review passed four private and six
new public/native checks in both debug and release, plus the six unchanged
SampleStatistics checks in both profiles. The reviewer replayed all 58 native
commands, both unchanged Constructor/DroppedMsg originals and a separate
future-publication age-wrap control. Source ownership, bounds, copy semantics
and the explicit-time setup translation have no remaining findings. Only
Constructor and DroppedMsg gain candidate whole-case credit (107/7/304 becomes
109/7/302); acceptance still requires the normal staged coverage gate and exact
hosted checks. Runtime clock/receive/Metric integration remains outside the slice.
