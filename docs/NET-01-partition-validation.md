# Partition validation assertion port

The reviewed port retains all 18 predicates in pinned upstream
`TopicUtilsTest.testPartitions`: eight accepted and ten rejected strings,
including the valid empty partition, invalid slash-only partition, forbidden
tokens, and 65535/65536-byte boundary. It calls production
`Config::set_partition` for each original input and checks the stored value on
success. Rejected inputs also check that an existing partition remains intact.
The setter applies the same private component grammar as native
`IsValidPartition`; full qualified-topic length is checked separately.

The unchanged native original passes 1/0 from pinned Gazebo Transport SHA
`41913b9dfc3dbce33ac5464ddcb427cc82450f26`. The mandatory gate now runs
it alongside the previously required TopicUtils originals. A private negative
control removes only the production setter's grammar guard; the new Rust port
then fails on the first invalid `/` input. No production rule, dependency or
public API changed.

Independent assertion review approved this selected mapping. The unchanged
[review receipt](reviews/partition-validation.json) binds fresh positive-only
Rust and both native binary replays, and records the excluded reviewer setup
failure. Broader TopicUtils and NET-01/NET-02 contracts remain open.
