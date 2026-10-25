# Explicit subscription retirement

`Node::unsubscribe` retires all subscription receivers for an original qualified
topic on this Node. Valid absent or already-retired topics succeed; invalid names
return `InvalidName`, cancellation returns `Shutdown`, and a poisoned registry
returns the existing explicit native error. A full command queue returns
`QueueFull` before changing local leases. Validation precedes mutation. The
subscription registry mutex serializes retirement with admission, and each active
lease releases its capacity slot exactly once. Retired handles can be dropped
later without releasing a slot owned by a replacement.

Pinned Node.cc 792 and NodeShared.cc 2086–2193 qualify the original input for
handler, interest and filter retirement despite computing a remapped name.
Rust preserves that observed metadata behavior: after subscribing through an
alias, retiring the alias leaves the canonical registration, while retiring the
canonical name removes every matching local receiver. Another Node's interest
is unaffected. Type-specific receivers for one topic retire together.

Retirement uses the existing bounded command queue before changing local leases.
The existing worker observes inactive leases, removes the last socket filter for
a topic and sends END notifications. A successful absent or repeated call also
queues a generic original-name END refresh, preserving native withdrawal intent.
That refresh can supplement cleanup notifications; exact native directed-packet
counts are not claimed.
It also skips registrations retired before worker admission. Recreation can
therefore proceed before cleanup without giving an old registration authority
again. The call uses nonblocking queue admission and adds no separate pending
operation collection, native call, network wait or IO runtime.

Rust receivers own already-admitted bounded message buffers. Those messages stay
available to drain after retirement, and worker deliveries selected before
retirement may complete. Subsequent selections reject retired receivers; after
the worker drops its sender, a drained receiver returns `Shutdown`. Native
NodeShared also removes queued callback handlers, using a remapped name for that
queue step. This API has no callback queue and does not reproduce that separate
queued-callback cancellation behavior. It never discards already-owned messages
silently or changes the existing receiver API's buffering contract.

The native/Rust metadata comparison retains invalid and absent inputs, duplicate
receivers, alias/canonical names, another Node's live interest, repeated retirement
and immediate recreation. Focused units cover slot release, dropped weak leases,
poison/cancellation, inactive queued admission, queue saturation without local
retirement, generic END refresh for absent/repeated names and owned message-buffer
behavior.
An actual XPUB acknowledgment proves socket filter removal before checking that
no new multipart payload arrives, and proves the recreated subscription's fresh
filter lifetime. Existing duplicate-filter and shutdown assertions remain intact.

The exact native NodeTest.PubSubOneThreadTwoSubs original is additionally required
by the gate and its XML baseline passes one run with zero failures. Its Rust port
remains pending: native same-process Nodes share handlers immediately, whereas
Rust Nodes discover independently. A local first receiver can satisfy the
original HasConnections wait before a second Rust Node is ready. Adding a second
readiness barrier would alter that original precondition. No complete original
mapping, process-global visibility, callback parity, global list/cache timing or
performance acceptance is claimed by this API increment.

The already accepted 34 complete ports and one partial port remain unchanged.

The corrected author full gate passes 125 Rust checks and 22 tooling checks,
1578/1599 production lines, 2402/2468 regions and 32/32 changed executable
production lines. Three new ownership/control units and the real C++ metadata
comparison pass, along with the unchanged native two-node baseline and all prior
assertions. Independent /root/msgs review passed 51 runtime tests, including all
three new units, and the fresh C++/Rust metadata check. It independently reran the
unchanged native original (XML one run, zero failures) and verified source/binary
hashes. Its queue/lease/filter/buffer/END audit accepts the documented boundaries;
that focused subset is distinct from the author full coverage gate. The exact
[authored receipt](reviews/unsubscribe.json) is copied unchanged. Matching final
metadata, the normal strict staged gate and hosted qualification remain required.
No additional original mapping is retired.
