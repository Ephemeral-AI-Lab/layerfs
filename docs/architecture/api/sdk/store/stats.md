# store.stats

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`layerfs store stats <name>` (SDK `store.stats(name)`) returns
`{name, kind, history_logical, canonical, stored, metadata, file, free, init}`.
Counts are bytes, except Init's entries/files/millis. `init` is null when no
Init totals were recorded, otherwise `{entries,files,bytes,canonical,stored,metadata,millis}`.

`history_logical` sums saved trees over Commit **and** Layer records; each
publication counts its tree again. It is not a deduplication numerator.
`canonical` and `stored` are allocated immutable content totals. On SQLite,
removing a project removes its history contribution but does not reclaim immutable objects,
so those allocation counters remain until a separate reclamation mechanism.
Project removal from PostgreSQL is currently refused `Unsupported`.
`metadata` includes accounted metadata and database allocation overhead.

`file` is the existing StoreStatistics database-page accounting, including
pages represented in WAL for SQLite; it is not `store status`'s file length.
For PostgreSQL it is the current schema's relation allocation expressed in
8 KiB pages. `free` is SQLite free-page bytes, null for PostgreSQL.

The engine reads existing StoreStatistics totals in constant work with respect
to history and content size; it performs no history/tree/object scan. PostgreSQL
allocation inspects the schema's relations, whose number is fixed by the schema.
Native SQLite and PostgreSQL run in process. SQLite on another machine runs
through its existing worker, with one attempt. A stopped worker is `Unavailable`;
nothing starts. Store move intent refuses the call under the existing lease rules.
