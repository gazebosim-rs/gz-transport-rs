# Duplicate service ownership

Bounded NET-02 task: reject another live registration on the same qualified
service topic within one Rust Node, preserving the existing receiver. Owned
files: runtime, unit/interop tests and this contract. Keep main's separately
reviewed batch assertion slice independent. No packet completion or original
case retirement is claimed.

Pinned detail/Node.hh 288–355 inserts a reply handler before discovery admission;
Discovery.hh 509–535 rejects its duplicate topic/process/node/address identity.
An actual native probe advertises Int32→Int32, then Int32→StringMsg on the same
node/topic. Results are first success, second failure, yet a local StringMsg
response request executes the failed registration's callback. Exact probe output:
`1 0 1 1 0 1 failed-registration-handler`. Native stderr reports failed advertising.

The recorded coordinator decision permits an explicit new Rust API difference.
`Result<Service, Error>` returns `AlreadyAdvertised` without retaining an inaccessible receiver or
handler. The original service remains authoritative. This preserves native
duplicate advertisement rejection and bounded ownership without reproducing its
failed-registration side effect. Tests asserting that legacy side effect cannot
be silently retired; full transport parity remains open.

Use the existing serialized, bounded weak service registry. Check duplicate
qualified topic before reserving/queuing. Drop and explicit withdrawal release
ownership; rollback leaves no ghost on queue failure. Test same/different types,
relative/absolute aliases, independent Nodes, concurrent duplicate admission,
failed queue admission, capacity reuse, first-service delivery and immediate
recreation. Preserve all existing lifecycle/native gates. A different reviewer
must examine the disposition and admission/lifecycle assertions before commit.

Full local verification passes 77 Rust tests and 21 tooling tests, 1282/1304
production lines, 1926/1991 regions and 3/3 changed executable lines. Independent
/root/sdformat review approved the exact production/assertion changes and explicit
native API disposition. It independently replayed 4 duplicate/lifecycle unit tests,
2 integration checks including the actual C++ requester after rejected duplicate
and recreation, and the native failed-registration side-effect probe; all pass.
The serialized check precedes capacity/queue admission; existing rollback, atomic
release and cleanup preserve one owner without slot leaks. No original case is
retired or newly claimed as a faithful port. Exact receipt:
[service ownership review](reviews/service-ownership.json). The matching strict
staged gate remains required before publication; hosted CI must pass on the
published revision before hosted acceptance.
