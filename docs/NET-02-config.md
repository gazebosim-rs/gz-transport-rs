# NET-02 explicit configuration slice

`Config::from_env()` captures only recognized Gazebo variables at the caller's
request. `Config::with_environment(lookup)` applies injected settings to a base
configuration without mutating the process environment. `Node::new` reads no
environment. Native send/receive high water marks are independent of application
queue capacity, and all must remain finite and positive.

Implemented settings: GZ_PARTITION, GZ_IP, GZ_DISCOVERY_MULTICAST_IP,
GZ_DISCOVERY_MSG_PORT, GZ_DISCOVERY_SRV_PORT, GZ_TRANSPORT_SNDHWM and
GZ_TRANSPORT_RCVHWM. Decimal integer parsing preserves upstream std::stoi's
ASCII leading whitespace, optional sign and numeric prefix, with default fallback
on invalid, negative or overflowing int32 input. Duplicate discovery ports adjust
the service port upward, or downward at65535, exactly as NodeShared does.

Pending modes return Unsupported: relay, PLAIN credentials when both environment
variables are present, statistics wire110 and non-ZeroMQ implementation selection.
HWM0 explicitly returns Unsupported because it requests an unbounded native
queue. Ports0 and values beyond uint16 are invalid. These are explicit bounds of
the current Rust API, not parity claims for legacy invalid/wrapping port handling.
Non-Unicode recognized environment values return InvalidConfig.

`Config::default()` remains the explicit Rust choice: empty partition and loopback
interface. On Unix, `Config::from_env()` now defaults to the native
`hostname:username` partition when GZ_PARTITION is absent. USER takes precedence,
including an empty value; otherwise a UID account lookup supplies the name. Safe
nix0.31.3 bindings perform these system calls before worker startup. This explicit
constructor performs synchronous system identity lookup in its caller; applications
can use an explicit Config to avoid it. GZ_PARTITION, including an empty value,
bypasses identity lookup. `with_environment` preserves its explicit base.

Non-Unicode hostname/USER, absent UID account and failed system lookups return
explicit errors. This does not claim the native exceptional NSS behavior: its
fixed201-byte passwd buffer, ten retries and random error-UUID fallback remain
pending. nix's User API decodes NSS names lossily, so a replacement character in
an NSS result returns Unsupported instead of silently selecting a changed
identity; this includes a literal replacement character from an otherwise valid
NSS name. Raw-byte NSS fidelity remains pending. An explicit UTF8 USER is preserved.
Automatic interface selection and Windows identity are also pending;
non-Unix automatic identity returns Unsupported. Topic namespace remains an API
setting, as in upstream.

Default identity validation includes real C++ child-process comparisons for
present/empty/absent USER, explicit partition precedence and no process-global
environment mutation in the test runner. Pure injected lookup cases verify host
and account errors, no account, invalid Unicode, and USER bypass of NSS. The
original NodeOptionsTest.accessors is mandatory, with only its default-partition
assertion mapped as a partial port; its setter/state assertions remain retained.

Initial explicit-settings slice validation:27 Rust tests passed, including child-process environment capture,
non-Unicode input, integer edge/fallback cases, unsupported settings, native C++
NodeShared differential checks for SND/RCV HWM and discovery ports, and actual
C++→Rust custom multicast/ports/partition/namespace publication. Rustfmt, Clippy,
original mapped naming assertions, fixture/expanded-ledger checks and strict
coverage pass. Production coverage:993/1012 lines (98.12%),1473/1533 regions
(96.09%). Raw data remain in artifacts/; the commit hook repeats validation on the
exact index export.

Quality correction: changed-line accounting now explicitly includes new source
files and carries staged changed-line ranges into the isolated snapshot. CI uses
the full push/PR base range. Unstaged edits cannot substitute for staged changes;
coverage is generated afresh and input hashes are checked again after execution.
