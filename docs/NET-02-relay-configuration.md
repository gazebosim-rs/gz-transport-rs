<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->
# Initial typed discovery peers

This separate candidate builds on bounded relay forwarding. An explicit
`Config::discovery_relays` vector seeds both worker-owned discovery channels
before worker startup. Each destination uses its channel's existing configured
port. No process-global relay object, extra thread, socket, timer or implicit
environment capture is introduced.

The additional public Config field requires exhaustive downstream struct literals
to supply `discovery_relays`, or to use `..Config::default()`. Default/environment
constructors retain empty seeds. Downstream revision pins are adopted separately.

`Config::add_discovery_relay(Ipv4Addr)` shares the runtime API's explicit address
validation: unspecified, multicast and broadcast destinations return
`InvalidConfig`. Duplicate values succeed without consuming another slot;
capacity failure returns `QueueFull`. Both preserve prior configuration on
rejection. Public vectors are checked before any worker construction and may
contain at most `Config::capacity` entries; duplicates within that input bound
are coalesced at startup, preserving first occurrence order. The worker consumes
its seed copy into two independently bounded channel vectors. Caller clones and
subsequent configuration edits do not mutate an admitted worker.

Empty defaults preserve existing behavior and allocate no seed payload. Existing
`with_environment` absence/empty precedence is retained, and nonempty GZ_RELAY
continues to return the same Unsupported error. The vector is an explicit typed
Rust setup API, not an implementation of native environment grammar or native
process-global discovery sharing.

Pure native characterization used the pinned `transport::env`, `split` and libc
`inet_addr` calls without constructing Nodes or opening sockets. Seventy-eight
recorded cases show: absent/empty variables add nothing; colon splitting retains
empty pieces; short, octal and hexadecimal IPv4 spellings are accepted; trailing
ASCII whitespace terminates parsing even before further text; invalid tokens
become the same broadcast sentinel as literal 255.255.255.255. Numeric duplicate
values retain only their first insertion. These semantics cannot be silently
replaced by strict dotted-quad parsing or the typed API's validation.

An isolated maintained-library evaluation found 28 corpus differences for
`std::net::Ipv4Addr` and eight for `url` 2.5.8's public Host parser. A small
C-lexical adapter around the latter matches this retained corpus, but that is
neither complete grammar qualification nor a production dependency choice. All
native observations and evaluation source remain retained for later compatibility
work. No legacy parser or malformed-value normalization is adopted here.

All thirteen pure configuration controls pass, including two new admission,
rejection, clone, pre-allocation validation and unchanged environment-precedence
cases. A private control observes both channel seeds in the first announcement
before any worker command or heartbeat cycle. Five public relay tests pass,
including independent configuration/worker ownership, duplicate admission and
capacity after startup. These nineteen checks pass in debug, release and
instrumented profiles; all-target Clippy passes. Focused changed-line evidence is
30/30 executable lines, not the full strict crate/repository coverage gate.
Independent rendering-owner source review found no blocking issue and passed
the two added pure controls, the first-announcement control and the public
ownership/capacity control directly from the author worktree with a fresh
reviewer-owned target, not a source export. Initial sandbox EPERM in
the socket controls was a setup failure; the same tests passed outside sandbox.
Normal strict staged checks and exact hosted qualification remain pending. No
original ledger credit or broader NET-02 completion is claimed.
