# Topic and namespace validation assertion ports

The reviewed ports preserve every predicate in the pinned upstream
`TopicUtilsTest.testTopics` and `TopicUtilsTest.testNamespaces` originals: 21
topic observations and 18 namespace observations. They retain the empty-topic
rejection versus empty-namespace acceptance, slash-only and forbidden-token
cases, and the 65535/65536-byte boundary. The Rust tests call the private
component validator used by topic qualification and configuration; the topic
test also applies the existing nonempty topic rule. No production validation
rule or public API changed.

The empty-topic row calls `qualified_name` directly. A private negative control
removed its production `topic.is_empty()` guard; the new port then failed with
`Ok("@@/")` where `InvalidName` was required. Restoring the exact source passes.

The full gate now executes both unchanged native originals together with the
previously required `testFullyQualifiedName` and `asValidTopic`. The pinned
C++ reference SHA is `41913b9dfc3dbce33ac5464ddcb427cc82450f26`.
The strict gates passed all 22 tooling checks and the full native/Rust suite,
with at least 1640/1645 handwritten production lines and 2459/2509 regions
covered across the observed runs.
There are no changed executable production lines. The initial sandboxed gate
attempt stopped in the existing native Node baseline at `getifaddrs`; the
unchanged gate passed with native network access.

Independent source and assertion review approved both exact mappings. The
[review receipt](reviews/name-validation.json) records private Rust, mutation,
and two distinct native binary replays. This slice does not cover other pending
TopicUtils cases, mutable MessageInfo setters, or broader NET-01/NET-02 acceptance.
