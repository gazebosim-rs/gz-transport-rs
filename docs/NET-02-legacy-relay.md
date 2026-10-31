<!-- Copyright (C) 2026 Open Source Robotics Foundation -->
<!-- SPDX-License-Identifier: Apache-2.0 -->
# Legacy relay environment capture

`Config::from_env` and explicit `Config::with_environment` recognize `GZ_RELAY`.
Absent input preserves the base's captured legacy seeds; explicitly empty input
clears those seeds. Neither operation changes typed `discovery_relays`. Default
configuration remains empty and never reads the environment implicitly.

The pinned Discovery constructor splits a nonempty value on every colon,
including empty tokens. `AddRelayAddress` converts each token using `inet_addr`,
then deduplicates the converted IPv4 values in insertion order. Thus numeric
short, decimal, octal and hexadecimal forms can name the same address, and
invalid input shares the broadcast value with literal `255.255.255.255`.
Unspecified, multicast and reserved numeric addresses are retained too. Text
after the first ASCII whitespace following a valid numeric address is ignored;
leading whitespace, trailing dots, bare hex prefixes and nonnumeric names fail
conversion. Colons are separators, not IPv6 support. This is legacy numeric
interpretation, with no hostname lookup.

Rust keeps these values in a separate opaque `LegacyRelaySeeds`, exposed through
`Config::legacy_discovery_relays` and its read-only `addresses()` slice. Its
public Config field preserves struct-update construction; exhaustive struct
literals must supply the new field, as with the earlier typed-seed addition.
Clone owns an independent copy. Typed configuration and runtime add APIs still
reject unspecified, broadcast and multicast destinations. A typed duplicate of
a legacy destination consumes no additional combined peer slot but does not
permit an otherwise invalid typed address.

Worker startup merges legacy seeds first, then typed seeds, deduplicating by
IPv4 value. Both channel copies exist before the first announcement, use their
existing configured ports and retain independent learned state. The existing
public query remains a **sorted union**, while sends retain insertion order.
Existing failed-send break behavior is preserved: later destinations are not
attempted after a failed unicast. Native global singleton identity is not added.

Setup has explicit bounds absent from native's unbounded vector: a maximum
64 KiB UTF-8 environment string and at most `capacity` unique combined legacy
and typed destinations. Overlength or embedded NUL returns `InvalidConfig`;
unique-capacity exhaustion returns `QueueFull`. Rejection precedes worker/socket
creation and leaves the base state intact. Raw typed vector length and address
validation remain enforced even when duplicates could fit the union. Shrinking
a captured configuration's capacity is validated again before startup. Existing
non-UTF-8 environment rejection remains unchanged. Normal allocation failure is
ordinary Rust allocation behavior, not a new fallible-memory guarantee.

The safe implementation reuses `url =2.5.8` rather than copying libc's numeric
parser or adding unsafe code. Its public Host parser implements the same base
and component-width arithmetic. A small adapter rejects native-incompatible
trailing-dot and empty-hex forms, restricts input to ASCII numeric syntax before
URL/IDNA normalization, and handles the native whitespace suffix. Rust std's
strict IPv4 parser differed on 28 of 78 retained observations; unadapted URL
parsing differed on eight; the adapter matched all 78. A separate generated
19,280-input glibc 2.39 comparison found no differences. These are corpus results,
not a universal formal equivalence claim. The native source was audited, not
copied; owned code remains Apache-2.0 and the parent crate still forbids unsafe.

Primary sources are the pinned
[Discovery constructor/AddRelayAddress](https://github.com/gazebosim/gz-transport/blob/41913b9dfc3dbce33ac5464ddcb427cc82450f26/src/Discovery.hh),
[glibc 2.39 inet_addr](https://github.com/bminor/glibc/blob/glibc-2.39/resolv/inet_addr.c),
[URL 2.5.8 Host parser](https://github.com/servo/rust-url/blob/v2.5.8/url/src/host.rs)
and [Rust std IPv4 syntax](https://doc.rust-lang.org/stable/core/net/struct.Ipv4Addr.html).
The URL API includes IDNA/ICU dependencies even though the adapter excludes
nonnumeric input. This adds 27 locked packages below, with no existing package
record changed except the root dependency edge. This is an explicit dependency
cost for maintained safe parsing; no performance benefit is claimed. Registry
source/license metadata and lock checksums are retained for review.

| Package | Version | License |
| --- | --- | --- |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 |
| icu_collections | 2.4.0 | Unicode-3.0 |
| icu_locale_core | 2.4.0 | Unicode-3.0 |
| icu_normalizer | 2.4.0 | Unicode-3.0 |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 |
| icu_properties | 2.4.0 | Unicode-3.0 |
| icu_properties_data | 2.4.0 | Unicode-3.0 |
| icu_provider | 2.4.0 | Unicode-3.0 |
| idna | 1.1.0 | MIT OR Apache-2.0 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT |
| litemap | 0.8.3 | Unicode-3.0 |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| potential_utf | 0.1.6 | Unicode-3.0 |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 |
| synstructure | 0.14.0 | MIT |
| tinystr | 0.8.5 | Unicode-3.0 |
| url | 2.5.8 | MIT OR Apache-2.0 |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT |
| writeable | 0.6.4 | Unicode-3.0 |
| yoke | 0.8.4 | Unicode-3.0 |
| yoke-derive | 0.8.4 | Unicode-3.0 |
| zerofrom | 0.1.8 | Unicode-3.0 |
| zerofrom-derive | 0.1.8 | Unicode-3.0 |
| zerotrie | 0.2.5 | Unicode-3.0 |
| zerovec | 0.11.9 | Unicode-3.0 |
| zerovec-derive | 0.11.7 | Unicode-3.0 |

`tests/fixtures/legacy-relays.json` retains the original 78 observations unchanged.
New native checks repeat the pure split/conversion output, both actual Discovery
constructor lists and public Rust `from_env` results. Additional generated numeric,
width, whitespace and long-zero families compare against actual `inet_addr`.
The durable generated control checks 9,777 additional numeric/malformed records.
All constructor and malformed-address worker sends are intercepted in their own
child processes: the test interposer **never calls a real send function**. This
includes destructor BYE and multicast sends. A preload-initializer marker is
cleared by each parent and required before any native constructor or Rust Worker
starts, so a nonfatal loader failure cannot fall through to real sends. A negative
control proves missing interception rejects before startup. The native destructor
send destinations are asserted explicitly. The capture fixture also injects a failed broadcast
attempt to check insertion order and first-failure stopping without broad traffic.
A separate actual loopback control checks both workers' first announcements with
legacy/typed overlap, caller ownership and independent channel copies.

Exactly the two former GZ_RELAY Unsupported checks are replaced by exact positive
assertions for their original inputs, backed by the retained native corpus.
Unrelated errors, typed-invalid guards, old fixtures, the NSS boundary and its 22
immutable Docker controls are unchanged. No original ledger disposition or count
changes: this environment capability does not complete global relay identity,
multi-host topology, CLI or whole NET-02.

Final author debug regression passed 200 library, two process-configuration and
five unchanged relay tests, followed by all five final legacy controls. Release
passed the three new private controls plus one existing metadata control selected
by the filter, five unchanged relay and five legacy controls. Formatting,
workspace all-target Clippy and all 42 tooling checks pass. These focused results
do not replace full strict coverage.

Independent Messages-owner review used a fresh source export and owned target:
all three new private controls and five public/native tests passed in debug,
including the complete retained corpus, 9,777 generated records, failed-send
ordering and missing-preload negative. All-target Clippy passed. The reviewer
checked all 27 added registry archive checksums, extracted source/license bytes,
unchanged old lock records and unchanged NSS source, finding no remaining source
or test-safety issue. This is a bounded review, not an independent release/full
staged/hosted claim. Complete ordinary staged coverage and exact source-bound
hosted qualification remain pending.
