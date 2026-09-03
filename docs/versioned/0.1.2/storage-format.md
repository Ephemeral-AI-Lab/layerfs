# LayerFS 0.1.2 storage format

> **Status:** Draft compatibility record for the withdrawn `v0.1.2` candidate.

The 0.1.2 release preserves the five-table SQLite schema, canonical object
encodings, identity domains, content-defined chunking profile, immutable
Layer and Commit records, Branch publication ordering, and authenticated read
requirements defined by the
[0.1.1 storage-format contract](../0.1.1/storage-format.md).

The universal edit engine changes only ephemeral Workspace representation and
localized Commit planning. It does not alter reachable canonical bytes,
identifiers, CDC, Store tables, required page size, or publication ordering. A
released v0.1.x Store remains readable.

The reportable 100,000-file unique-content control uses 661,061,632 durable
bytes at the retained ObjectId/SQLite layout, above the 600,000,000-byte goal.
The owner accepted that exact patch-compatible blocker. A reproducible physical
pack experiment establishes only a conservative 562,513,789-byte object-storage
lower bound; it is not a complete Store implementation and is deferred to
[issue #18](https://github.com/Ephemeral-AI-Lab/layerfs/issues/18).
