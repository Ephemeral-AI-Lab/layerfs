# LayerFS persistence

> **Status:** Current implementation under qualification; no release admission.

`layerfs-persistence` replaces `layerfs-metadata`. It composes the physical
Objects/Metadata provider and one shared history flow over one global Store:
a host-local SQLite file, or (#506, `postgresql` feature) a PostgreSQL schema.
The domain crates have no engine dependencies. An engine is selected
explicitly; there is no error-driven fallback between them.

## Engines (#506)

`src/backend/` holds the engines. `sqlite/` is grouped by role: `session/`,
`lane/`, `objects/`, `metadata/`, `storage/` (merge records), `acquisition/`,
`lifecycle/`, plus `schema.rs` and `snapshot.rs`. `postgresql/` mirrors it:
`session/`, `lane/`, `objects/`, `metadata/`, `history/`, `storage/`,
`schema.rs` and `import.rs`. Code above `backend/` holds no SQL: history,
the Metadata/Objects bounds and publication validation are shared and reach
an engine through two traits (`Mechanics`, `HistoryStatements`). History
statements are named; the shipped SQLite texts are portable and run on
PostgreSQL unchanged except `hint_insert_binding` and `statistics_totals`.

- **Snapshot.** `Handles::snapshot` (and `StorageProvider::snapshot`) copies a
  live SQLite Store to a new file with the online backup API in one step, on
  its own read-only connection: one consistent state, closed, no sidecar.
  Remote push uploads it.
- **PostgreSQL Store.** `PostgresHandles::open_writable`/`open_read_only` over
  a `PostgresSelection` (URL, schema, `synchronous_commit`, lane
  `lock_timeout`). Opening validates the schema and binding and never
  creates or migrates anything. Init stays SQLite only:
  `PostgresHandles::import` copies a SQLite Store (any pack layout) into a new
  schema in one PostgreSQL transaction over one SQLite snapshot, keeping every
  object, root, Commit and inode serial.
- **Transactions.** Reads are `REPEATABLE READ READ ONLY`; writes are `READ
  COMMITTED`, never SERIALIZABLE, with `SET LOCAL synchronous_commit` (default
  `off`) in the same round trip as `BEGIN`. Every write first locks the
  Store's `store_lane` row `FOR UPDATE` under `SET LOCAL lock_timeout`, so
  writes of every process run one at a time, as on SQLite; a held
  publication lane owns the row on a second connection and this thread's
  writes inside it skip the wait. A wait that times out is `Busy`; nothing is
  retried; a COMMIT whose reply was lost is `Unknown` and quarantines the
  session.
- **Objects.** Pack bodies are whole `bytea` values stored `EXTERNAL`;
  ranged reads use `substring`. A publication copies its pack bodies with
  `COPY BINARY` in a write that does not wait for the lane, then adds
  locations, values, signatures and totals in the short lane transaction. A
  failed second step leaves unreferenced packs, never reused ids.
- **Removing a project.** `HistoryCatalog::remove_layerstack` deletes one
  stack's Layers, Branches, Commits and lineage, and their logical bytes from
  the totals, in one write transaction on either engine; the write holds the
  writer for time that grows with that project's history. Packs, objects,
  file digests and the stack's allocator scope stay; reclaiming packs is #411.
- **Attaching a schema.** `PostgresHandles::binding` reads the binding key a
  LayerFS schema records, so a home that did not import it can open it.
- **Not on PostgreSQL in this phase:** Init and acquisition, merge records
  (merges are refused), seal/snapshot/reclamation (no file), and the host
  SQLite cache #420 found necessary for one round trip per Commit.

The twelve logical tables are `pack`; `store_policy`, `object_location`,
`inode_value`, `content_signature`; and `history_meta`,
`scope_allocator`, `layer_stack`, `layer`, `commit`, `branch`, `workspace_stage`.
Schema versions 7 to 12 (three pack layouts, with or without the acquisition
tables) have application ID 1279677264 and 4096-byte pages. Versions 1 to 6 kept
inode values in pooled packs behind `metadata_value_group`. Existing incompatible
Stores are refused, without migration or automatic adoption. Files and
directories remain canonical objects, without persistent SQL mirrors.

`Handles::create`, `open_writable` and `open_read_only` require explicit
application authority. The handles share a connection/session, while each C2
publication and C5 operation has a separate bounded transaction. Published pack
bodies, descriptors, first-wins locators, inode value rows, signatures and
ordinal changes commit together. Already sealed bodies use one final INSERT; construction
is private in the bounded C2 packer, so no published BLOB is appended or rewritten.
`Arc<Vec<u8>>` shares sealed construction storage through publication without a
second whole-body copy. Locator statements split at actual engine limits within
the same transaction. Canonical and physical bounds remain explicit.

The available profile is `sqlite-wal-full-macos-fullfsync-v1`: WAL, synchronous
FULL, foreign keys, fullfsync and checkpoint_fullfsync enabled, busy timeout zero,
mmap zero, page cache -2048 KiB, automatic checkpoint every 1000 pages, retained
journal limit 4194304 bytes. The last setting is not a WAL high-water bound.
Configuration and readback fail closed; other platforms currently refuse the
required macOS profile. The VFS is SQLite's default; its name/sync system calls
are not directly observed by the adapter. Settings establish requested SQLite
behaviour, not a power-loss claim about hardware.

SQL/VM/binding/transaction/body work is observed at actual execution boundaries.
Statement/commit/transaction wall spans overlap. Explicit `Handles::checkpoint`
returns Busy/pending frame information and must remain in the calling
application's lifecycle accounting. Close/checkpoint work cannot be moved out
of a benchmark to improve its number. Unknown outcomes quarantine the shared
session; a definite failure rolls back once. Read-only mutations and contention
never retry.

The host endpoint/transport required for a sandbox is a separate unresolved
application-adapter decision. A local Rust handle does not cross a process
boundary. Namespace entry-count collections are measured but are not yet a
whole-importer memory-bound proof.

The current plan and evidence live in
[issue 302](../../../core/docs/issues/302/SQLITE-DESIGN-AND-PLAN.md). Performance qualification
requires all seven revised Step 10 selections and the prospectively frozen 10%
time margin; unit tests and compilation alone do not complete it.


A complete unobstructed explicit checkpoint releases unused main-file extents
past logical EOF using the safe macOS extent-transfer capability. The adapter
retains a main-file descriptor, transfers only extra allocation to an exclusively
created same-volume empty scratch, and closes/removes that scratch. No logical
Store bytes are copied or truncated. Readonly/busy cases perform no release;
unsupported volume/capability and cleanup failures are explicit, with uncertain
outcomes quarantined. Before/after allocation and total checkpoint wall are
reported. This operation is part of lifecycle accounting, not setup. Process-kill
proof after completed release is not a physical power-loss/in-syscall crash claim.
