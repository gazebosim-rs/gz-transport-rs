# Fully qualified topic decomposition

The reviewed `decompose_fully_qualified_topic` returns borrowed partition and
topic slices. It follows pinned native `TopicUtils::DecomposeFullyQualifiedTopic`:
the first `@` must be at the start, the last `@` separates the parts, and the
topic must be nonempty. Both parts use the existing native component grammar.
The native function does not check the combined name length, and this API does
not add such a check. `Err(InvalidName)` maps the native false result; Rust has
no mutable output arguments on failure.

The Rust port retains all three successful partition/topic output pairs and
all five rejected inputs in upstream
`TopicUtilsTest.decomposeFullyQualifiedTopic`. The unchanged native original
passes 1/0 in both the pinned sibling binary and separate source-snapshot
binary. The required gate runs it alongside previously retained TopicUtils
originals. No dependency, wire format or other transport behavior changes.

Independent source and assertion review approved this selected mapping. The
unchanged [review receipt](reviews/decomposition.json) binds private Rust and
both native replays, and records excluded reviewer setup attempts. Other
decomposition and wider NET-01/NET-02 contracts remain open.
