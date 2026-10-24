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

`Config::default()` remains the documented explicit Rust choice: empty partition
and loopback interface. Legacy hostname:username defaults and automatic interface
selection remain pending; callers can supply a base partition/interface before
applying settings. Topic namespace remains an API setting, as in upstream.

Validation:27 Rust tests pass, including child-process environment capture,
non-Unicode input, integer edge/fallback cases, unsupported settings, native C++
NodeShared differential checks for SND/RCV HWM and discovery ports, and actual
C++→Rust custom multicast/ports/partition/namespace publication. Rustfmt, Clippy,
original mapped naming assertions, fixture/expanded-ledger checks and strict
coverage pass. Production coverage:993/1012 lines (98.12%),1473/1533 regions
(96.09%). Raw data remain in artifacts/; the commit hook repeats validation on the
exact index export.

Quality correction: changed-line accounting now explicitly includes new source
files and carries staged changed-line ranges into the isolated snapshot. CI uses
the committed parent diff. Unstaged edits cannot substitute for staged changes;
coverage is generated afresh and input hashes are checked again after execution.
