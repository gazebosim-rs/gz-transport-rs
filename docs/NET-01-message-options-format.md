# Message-option formatting

`AdvertiseOptions` implements Rust `Display` for the ordinary default C-locale
output of the pinned native message-option stream insertion operator. Scope is
Process, Host or All. The native unlimited `u64::MAX` sentinel prints No with no
rate line; every other rate, including zero, prints Yes and its decimal rate.
Tabs, spaces and final newlines are retained exactly. Formatting reads options
without changing scope, rate, wire metadata, clock or queue behavior.

The faithful port of `AdvertiseOptionsTest.msgStreamInsertion` retains its two
literal byte assertions: default All/unlimited and All/rate10. Before production
edits, the unchanged native original passed both predicates and 24 additional
native rows covered three scopes and eight rates. An independent fresh native
peer reproduced every status/stdout/stderr row. The fixture preserves those
actual bytes. A separate Rust supplement requires writer errors to propagate
from the initial prefix and both conditional tails.

Independent [source review](reviews/message-options-format.json) verified a fresh
181-file export, 89 library and three formatting tests, three production controls
and restored passing tests/hashes. Wrong scope labels and incremented rates fail
the fixed original or native bytes; swallowing a tail error fails the Rust
writer supplement. The exact source gate passes 250 Rust and 24 tooling tests, 2781/2859 lines and 4081/4272 regions;
11/11 changed executable lines are covered. All
211 source reports are retained. Full JSON/LCOV and
authoritative changed-line policies were independently checked without filtering.

This qualifies ordinary default stream bytes and Rust formatting error
propagation. Native ostream failbit/exception/flush behavior, stream flags,
width, precision, other locales, manipulators, base/service option formatting,
copy/scheduler behavior and performance remain unqualified. The broader retained
NET-01/NET-02 scope stays open. No assertion or coverage gate is waived.

Final metadata binding, the mandatory normal protected hook and exact new hosted
success remain required for acceptance. Native review's initial still-open
stdout hashing and subsequent post-exit hash correction remain retained; they
did not change source, predicates or runtime results.
