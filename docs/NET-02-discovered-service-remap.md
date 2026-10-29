# Remapped discovered service listing

`original_service_list_remap_discovered` preserves the complete assertion body
of upstream `NodeTest.ServiceListRemap` in `src/Node_TEST.cc`: one Node maps
`/foo` to `/bar`, advertises an Int32 service at `/foo`, queries the service list,
and requires exactly one entry with index zero equal to `/bar`.

The Rust port calls `known_services()`, the bounded worker/discovery query.
Registration and query commands are ordered on the same worker, so it needs no
sleep or discovery polling. The live Service handle retains the registration
that the native Node owns. The older `original_service_list_remap` local-handle
inspection test remains unchanged.

A separate test checks that this service does not enter the publisher list,
that withdrawing the original `/foo` name does not remove the remapped `/bar`
registration, and that destination withdrawal, recreation and handle drop update
the discovered list. These lifecycle checks earn no additional original-case
credit.

This completes the selected one-Node original assertion mapping. It does not
establish process-global service sharing, immediate remote convergence, general
query latency, or complete NET-02 behavior. There are no production, protocol,
dependency or coverage-policy changes.

Run the focused tests with:

```sh
cargo test --locked --test service_assertions service_list_remap
```

The mandatory gate also runs the unchanged native `NodeTest.ServiceListRemap`
original from the pinned C++ reference. Independent review compared both new
tests with the original and replayed the two new tests successfully.
