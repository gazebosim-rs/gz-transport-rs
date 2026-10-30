# Service discovery scope

`ServiceOptions` contains only discovery Scope and defaults to All. It is a
copied value with the native scope-only Display form. `serve_with_options`,
`serve_callback_with_options` and `serve_oneway_callback_with_options` admit
that immutable scope; changing the source options afterwards cannot change a
service. Existing entry points retain All and their existing validation,
registration, request, thread and queue limits. Rates are message options and
are not silently accepted as service settings.

All is eligible for remote discovery. Host is eligible for peers classified as
local hosts by the existing interface/loopback policy. Process stays within
admitted local discovery and routing. Scope is not authentication or access
control: it does not secure a known socket address. Physical multihost routing,
all-interface topology and global process UUID equivalence remain outside this
bounded explicit-interface profile.

The native Discovery<ServicePublisher> policy has distinct paths. Initial
ADVERTISE, Subscribe/REQ replies and UNADVERTISE suppress Process records;
Host replies also require a local sender. Heartbeats re-advertise EVERY active
record, including Process. Receiving discovery implementations reject Process
records and nonlocal Host records. The implementation preserves those scoped
heartbeat packets rather than claiming that Process creates zero wire traffic.
The actual service record, descriptor pair and reply socket identity are
retained without a new wire format or dependency.

The existing weak admitted service index supplies same-domain type resolution,
inline callback selection and queued request handoff before remote discovery.
This makes Process services reachable from another local Node. Queued replies
still use the provider response lane and mandatory ZeroMQ route; scope does not
replace that bounded reply adaptation. Worker-only inspection does not merge
another Node's declarations, so a local routable Process service need not appear
in another worker's metadata view. This is not remote convergence or native
process-global storage equivalence.

The six pinned AdvertiseOptionsTest.srv* originals preserve default All, copy
and assignment values, all four equality/inequality predicates, the literal
scope-only output and the All-to-Host accessor transition. Their earlier source
notice is retained. Independent review approves these six selected value ports;
their ledger promotion remains conditional on normal staged and exact hosted
qualification. Formatting supplements cover every scope and writer failure
without changing message-option output.

Native packet characterization obtains all eight metadata fields from an ACTUAL
admitted C++ service, then passes that record to the pinned native Discovery
template. A separate controlled discovery channel and first empty heartbeat
establish phase boundaries; its public heartbeat interval is set to 2000 ms for
this test only. Initial, Subscribe, periodic and withdrawal packets are decoded
from the real UDP frames and compared field by field. Every scope must carry a
periodic record, while Process suppresses the other three paths. A second
native local Node actually calls the admitted service with Int32(5), preserving
a true result and one callback. The control's independent Discovery requester
UUID models another discovery participant; it is not a native global-identity
claim. Native defaults and original tests are unchanged.

The template helper is built against the same pinned source/configuration and
SDK as the main C++ peer; the verifier checks BOTH helpers' actual library
resolution. Rust controls check the same packet policies, immutable metadata,
all three local service API forms, real native remote requests and visibility,
raw periodic records, worker-only filtering and rollback/cancellation. A
synthetic nonlocal sender covers the Host Subscribe branch; it does not replace
future physical multihost qualification. No existing test body, predicate or
time bound is weakened; private Serve constructor setups add only their prior
All scope. Callback spawner signatures and old tests remain.


Independent review rebuilt both helpers against the canonical SDK and checked
all six original value predicates against the pinned source. Its fresh source
and Cargo target passed 136 library checks, 42 public/native checks and the six
unchanged native originals, including the final simultaneous port reservations.
The review confirmed both helpers' actual Gazebo library resolution, all eight
native metadata fields, immutable scopes, rollback, the existing queued reply
lane and the distinct packet policies. Only these six value rows gain candidate
faithful-port credit (99 to 105; 7 partial and 306 pending remain). Normal staged
coverage and exact hosted CI are still required. Physical multihost behavior,
process-global identities and complete NET-01/NET-02 parity remain open.
