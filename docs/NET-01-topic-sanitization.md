# Native topic sanitization

`as_valid_topic(&str) -> String` ports pinned gz-transport
41913b9dfc3dbce33ac5464ddcb427cc82450f26 TopicUtils.cc385–425. It supports the
simulator's sensor topic metadata without copying transport rules into the
simulator. It does not qualify a topic, add a leading slash or read configuration.

The scan replaces each ASCII space with an underscore, drops @ and ~, and
removes each adjacent := or // pair once. Thus a//b becomes ab and a///b becomes
a/b. Characters removed between slashes or := may form an invalid remaining pair;
that result becomes empty rather than undergoing another scan. Remaining UTF-8
and embedded NUL bytes are preserved, just as the native std::string scan does.
Other whitespace is not treated as ASCII space. Rust's text API accepts UTF-8;
no arbitrary invalid-UTF-8 byte-string API is added.

After transformation the same native topic predicates apply: nonempty, not /,
no remaining forbidden sequences, and at most65535 bytes. Input may exceed that
bound if transformation shrinks it. Native failure is an empty string; the public
helper preserves that sentinel. Normal name resolution remains a separate step.

Original TopicUtilsTest.asValidTopic (TopicUtils_TEST.cc299–345) retains all16
inputs and exact output/validity assertions, including the duplicate /abc row.
The gate runs the original pinned native case. A C++ differential peer receives
raw stdin bytes before constructing any network node;22 additional cases compare
scan order, overlapping slash pairs, formed-invalid pairs, UTF-8/NUL, whitespace,
65535/65536-byte boundaries and an overlength input that shrinks to a valid topic.
No fixture/source hashes, exclusions or thresholds change. Original2014 notices
remain in the assertion port files. Full NET-01 and NET-02 scope remains open.

Independent /root/sdformat source/assertion review approved this helper and
reran the exact native original, the16-input Rust table and22-case C++ differential
test; all pass. Strict full local verification passes47 Rust tests and21 tooling
tests,1184/1208 production lines,1767/1837 regions and19/19 changed executable
lines. Matching staged verification remains required before publication.
