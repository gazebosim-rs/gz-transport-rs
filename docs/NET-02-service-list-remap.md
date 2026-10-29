# Remapped service listing

The original local-inspection port, `original_service_list_remap`, retains the
native `/foo` to `/bar` remap, service registration, one-entry list and index-zero
`/bar` assertions through `advertised_services()`. That test remains unchanged.

The [discovered-service port](NET-02-discovered-service-remap.md) now checks the
same original predicates through `known_services()`, the actual bounded
worker/discovery query. Independent review approved the selected one-Node case
as a faithful port. The earlier [partial review](reviews/service-list-remap.json)
remains historical evidence for the local-inspection test.

The unchanged native original is required in every gate. This selected mapping
does not qualify a process-global registry, remote convergence, general query
latency, or broader NET-02 completion.
