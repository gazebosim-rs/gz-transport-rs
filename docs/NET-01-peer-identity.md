# Independent discovery channels and peer identity

The initial Rust peer key combined process, node and topic into one string. A
same-node message publisher and service on the same topic overwrote each other.
A deterministic regression on stable ee0e98e retained one peer instead of two;
its failing output remains in artifacts/peer-identity-reproduction.log. No Rust
public API or protocol changes are needed to correct that loss of native state.

Pinned native Discovery owns independent message/service TopicStorage instances.
TopicStorage.hh51–90 identifies entries by topic, process, node and address. A
repeated advertisement with the same identity keeps the original metadata.
DelPublisherByNode220–258 removes all addresses for a topic/process/node in that
storage. Rust now uses a structured channel/process/node/topic/address key,
retains first metadata on duplicate advertisements, admits connection/registration
side effects only for a newly accepted identity, and limits withdrawal to the
matching discovery channel. The existing global capacity bound remains unchanged.

Discovery.hh1233–1250 refreshes process activity for each received packet before
its opcode dispatch. Rust now refreshes known-process peers only on that packet's
channel, including subscriber-list requests. BYE removes only that channel's
process entries. Pub/sub connection admission and retention use message-channel
peers; RPC connection admission and retention use service-channel peers. Metadata
received on the opposite channel cannot establish or preserve the wrong endpoint.
Unknown opcodes refresh activity as native dispatch does, then are ignored;
they cannot become default ADVERTISE entries. No new unbounded activity map is
introduced.

Six unit tests check same-topic channel coexistence, multiple addresses, duplicate
metadata retention, every withdrawal identity field, process/channel activity,
BYE and actual ZeroMQ connection admission/cleanup. A genuine pinned C++
TopicStorage probe repeats message+service insertion, second-address insertion,
duplicate type change, message withdrawal and service withdrawal: observed total
counts2,3,3,1,0 and original metadata match Rust assertions. Another real C++ Node
publishes StringMsg and serves StringMsg on the same topic; Rust receives the
publication and completes request/reply. Pipe completion keeps the C++ provider
alive until the actual response arrives; repeated publications establish readiness.

The mandatory gate also executes original TopicStorageTest.TopicStorageAPI; its
native baseline passes. This does not claim the original's entire storage/query
API is ported. Its disposition remains pending; all418 retained cases and source
hashes remain. This slice is focused conformance, without test retirement,
coverage exclusions, performance claims or full NET-01/NET-02 completion.

Independent /root/msgs review compared the native identity/activity/admission
source and reran all33 Rust library tests, both newly compiled native C++
interop tests and the exact original TopicStorageAPI case; all pass. It found
and rechecked the corrected duplicate/capacity registration side effect. Strict
full local verification passes55 Rust tests and21 tooling tests,1202/1226
production lines,1785/1853 regions and40/40 changed executable lines. The matching
strict staged hook remains mandatory before publication.
