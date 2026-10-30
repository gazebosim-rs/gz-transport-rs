<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->
# Explicit native interface setup

`Config::from_env()` on Linux GNU selects an interface when captured GZ_IP is
absent or empty. A valid nonempty override bypasses interface inventory and
hostname resolution; independent partition identity lookup still occurs when
GZ_PARTITION is absent. Config::default remains loopback, with_environment
preserves its explicit base and Node::new reads no environment. Recognized
configuration is validated before automatic setup. Existing invalid,
unspecified and non-Unicode override errors remain; native's raw invalid GZ_IP
return is not admitted by the Rust IPv4 API. GZ_RELAY remains Unsupported.
The same existing unspecified-address guard applies after automatic selection:
if native's first eligible choice is 0.0.0.0, Rust returns InvalidConfig instead
of accepting it or silently substituting a later interface. A paired controlled
native/public case preserves this additional admission difference.

Setup reuses the safe nix interface inventory used by Host scope. Null/unknown
families are skipped; a successful interface index is remembered before IPv6,
exact 127.0.0.1, down and non-multicast filtering. Duplicate indices suppress
later aliases; duplicate addresses preserve first order. No eligible address
yields loopback. Failed index lookups are skipped and inventory failures remain
Rust errors rather than native process exit. An ambiguous lossily decoded
interface name still returns Unsupported.

For a successful nonempty/non-localhost hostname, only the first legacy NSS
IPv4 A record is considered. A compatible candidate outside native's
192.168/10./169.254 private prefixes and preferred-only 127.0. prefix wins.
Otherwise setup chooses the first eligible native-public address, then the
first eligible address. These predicates intentionally differ from RFC-private
and full loopback classification: 172.16/12 is not native-private here. No later
NSS address is tried. No getaddrinfo sorting or DNS protocol implementation is
substituted.

The constructor uses one owned inventory snapshot. Native can enumerate again
after a rejected candidate; changing interfaces between those calls remain an
explicit setup translation outside the stable-input value proof. No network
convergence, native global identity or interface-churn timing is claimed.

## Isolated owned NSS boundary

The parent gz-transport package retains unsafe_code=forbid. The separately owned
gz-transport-nss package contains only a safe first-A function over GNU
gethostbyname_r, using exact existing libc 0.2.191 types/version (MIT OR
Apache-2.0). Its two explicit unsafe sites call libc and read one validated
pointer cell through the owned scratch allocation. No foreign pointer is
dereferenced directly, no later record is traversed, and only four IPv4 bytes
are returned as an owned value. No static hostent data, raw pointer or scratch
escapes, and there is no manual Send/Sync implementation. Pointer/lifetime and
buffer invariants are documented at each operation in nss/src/lib.rs.
The first pointer cell and address must both fit wholly inside caller scratch.
Out-of-scratch storage is rejected visibly; admission of other NSS provider
storage layouts is not inferred from the qualified GNU files/numeric cases.

Scratch doubles from 1024 to 65536 bytes, at most seven calls, only for ERANGE
together with NETDB_INTERNAL. Names have a 1023-byte bound. An actual 10,000-alias
native fixture demonstrates the intentional difference: legacy native lookup
finds a valid first address beyond this scratch budget; Rust returns visible
Unsupported("resolver scratch limit"). It does not silently fall back. Allocation,
name-bound and malformed-result errors remain visible Native diagnostics.
Ordinary native NSS lookup failure supplies no preferred candidate, matching
native fallback intent.

These are memory/call-count bounds, not time bounds. Synchronous NSS can block
this explicit setup constructor and cannot be preempted here. Construct explicit
Config values when lookup is unwanted. Never perform this setup in worker polls,
callbacks or authoritative simulator steps. The new package creates no thread,
worker, socket, cache or background refresh; NSS itself may use system services.
No Node statistics/Metric enable-disable policy is implemented.

## Qualification and retained scope

The independent private study passed the range unit and 21 injected failure/
ownership controls in debug/release, Clippy and the pure policy's four controls
in both profiles. Actual immutable-image glibc 2.39 tests matched 21 within-bound
legacy/reentrant first-A cases and reproduced the separate scratch-limit
difference. Those were private boundary/value reviews, not approval of this
production integration. The earlier author run with weaker tag binding remains
identified separately; it is not relabeled as immutable-image evidence.

The production candidate retains all 32 stable controlled native host/inventory
cases, including three explicit-override cases and their two retained Rust
admission differences, and adds automatic-selected-unspecified as case 33.
Child-only interposition
supplies identical inputs to the pinned Gazebo library and public Rust API;
actual call counters prove override bypass and one-inventory setup. No Nodes or
real discovery sockets are created. Existing configuration tests and original
assertions remain unchanged; added checks preserve default/injected interfaces,
hostname bytes, fallback, errors and explicit override precedence.

The separate actual NSS runner uses the official Ubuntu 24.04 amd64 digest in
docs/nss-reference.json. It resolves an immutable image ID before children and
checks the reference afterward. Each container is read-only, network-none and
uses private hosts/nsswitch/host.conf files; host NSS files are never modified.
Native C values and the actual owned Rust test executable are compared. Coverage
profiles get distinct case names because container PIDs repeat. DNS servers,
remote NSS services, concurrent environment/locale mutation and other libc
versions/platforms remain unqualified.

Both owned crates require nonempty strictly >95% line and region coverage,
separate normal compiler-input proofs and strictly >95% changed executable
lines, as well as the aggregate gate. Neither the sys package nor moved
inventory code is classified as an excluded dependency. Prepare the required
test image with `python3 scripts/nss-reference.py --prepare`. The ordinary
staged/hosted gates execute all original native groups, all workspace tests,
new public controls and the actual NSS runner. Independent bounded production
source/gate review is complete; full normal staged checks and exact hosted
qualification remain pending.
No ledger entry is promoted; retained public NetUtils hostname()/username()
originals and whole NET-02 remain separate scope.

Author affected regressions pass 191 library + 111 unchanged interop + two
configuration-process + three public interface + five sys tests (312), with all
peers exited. The sys unit/injected controls and all 22 actual NSS cases also
pass in release. Both normal compiler-input audits pass. A separate source
export compiles a no-patch canonical consumer with one Msgs/prost/libc identity
and unchanged external versions, reusing compiler/dependency caches; no remote
Git/fresh-cache qualification is claimed. Independent production review used
fresh owned source/binaries for actual NSS 22; parent public/error replay used
the latest author-built executable. Its unspecified-address finding and the
test-only inventory error controls preserve every existing admission guard.
