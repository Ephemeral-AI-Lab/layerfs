# Store

[Architecture](../../README.md) › [Engine](../README.md) › Store

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The one global Store: SQLite and PostgreSQL backends side by side, one writer, short holds and the publish lane.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-persistence` |
| Source | [`engine/crates/layerfs-persistence/src/store`](../../../../engine/crates/layerfs-persistence/src/store), [`engine/crates/layerfs-persistence/src/backend`](../../../../engine/crates/layerfs-persistence/src/backend), [`engine/crates/layerfs-persistence/src/history`](../../../../engine/crates/layerfs-persistence/src/history), [`engine/crates/layerfs-persistence/sql`](../../../../engine/crates/layerfs-persistence/sql) |
| Uses | [Core: Physical storage](../../core/physical-storage/README.md), [Core: History model](../../core/history-model/README.md) |
| Used by | [Store calls](../store-calls/README.md), [Project Init](../project-init/README.md), [Commit](../commit/README.md), [Control: Backups](../../control/backups/README.md) |
| Flows | [A Commit](../../flows/commit.md), [Project Init](../../flows/init.md), [A call, end to end](../../flows/call.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Diagnostics](diagnostics.md) | Read-only integrity, connection, coverage and cancellation semantics. |
| [Lifecycle](lifecycle.md) | Selecting, opening and sealing a Store. |
| [Backends](backends.md) | The backend seam shared by SQLite and PostgreSQL. |
| [SQLite](sqlite.md) | The SQLite backend. |
| [PostgreSQL](postgresql.md) | The PostgreSQL backend. |
| [History publication](history-publication.md) | Writing Branch and Commit history and publishing it. |
| [Metadata and objects](metadata-and-objects.md) | Allocation, locations, signatures and object reads. |

## The picture

```text
 StoreSelection --open--> Handles                              (Lifecycle)
                            |
     +----------------------+------------------------+
     v                      v                        v
 storage: StorageProvider  history: HistoryProvider  acquisition (Init)
 PackPersistence port      HistoryCatalog trait      working tables
 (Metadata and objects)    (History publication)
     |                      |                        |
     +-----------+----------+------------------------+
                 v
        Mechanics / HistoryStatements                  (Backends)
                 |
        Engine::Sqlite(Session) | Engine::Postgres(Session)
                 |                        |
   admission turn -> dir flock      admission turn -> store_lane row
   -> BEGIN IMMEDIATE               -> READ COMMITTED write
                 v                        v
        one Store file (WAL)       one schema in a PostgreSQL database
        (SQLite)                   (PostgreSQL, feature postgresql)
```

## What it does

| Question | Answer | Source |
| --- | --- | --- |
| What one Store holds | Every project (LayerStack), its Layers, Branches and Commits, and every immutable pack, object location and inode value they reach, in one database. | [`history.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/history.sql), [`metadata.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/metadata.sql) |
| Who opens it | Each Workspace daemon opens it directly; SDK Store calls open it in the SDK's own process on Linux. On macOS and Windows those calls go to a Linux worker that opens it. Project Init creates it or adds to it. | [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs), [Store calls: Service](../store-calls/service.md) |
| Where a SQLite Store may live | A local filesystem every opener shares through one kernel: ext4, xfs or btrfs on Linux; apfs or hfs on macOS for the native Sandbox. Anything else is `Unsupported` before a file opens. | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| What SQLite and PostgreSQL share | The history code, the Metadata and Objects bounds, the history SQL text, `Admission`, error mapping and the table set. | [`seam.rs`](../../../../engine/crates/layerfs-persistence/src/backend/seam.rs) |
| How they differ | SQLite: one file, `flock` plus `BEGIN IMMEDIATE`, pack-unit layouts, snapshot and seal. PostgreSQL: one schema, the `store_lane` row, `synchronous_commit`, temporary merge tables, no file operations. | [SQLite](sqlite.md), [PostgreSQL](postgresql.md) |

Nothing converts a Store from one kind to the other.

## Invariants

| Rule | Enforced by |
| --- | --- |
| One global database per Store; no sharding or second database | One `Session` per open; the table set is checked exactly at open: [`schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/schema.rs) |
| One writer at a time, across threads and processes | [`admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/admission.rs), [`writer_admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/writer_admission.rs), `store_lane`: [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/profile.rs) |
| Short holds: the lane covers head check, merge step and publish only | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs); owner decision 2026-10-18 |
| Reads never take the lane | Reads use `BEGIN`, a read snapshot or no transaction, never the directory lock or lane: [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/transaction.rs); Store-call readers have their own read-only sessions: [`read_service.rs`](../../../../engine/crates/layerfs-store-service/src/store/read_service.rs) |
| Disposable / WAL / synchronous=OFF is the only SQLite profile used | Product openers select it and refuse others: [`init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs), [`install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs) |
| One attempt per operation; no retry, busy handler or replay | `busy_timeout=0`; `Unknown` quarantines the session: [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/transaction.rs) |
| Published packs, units, locations and inode values never change | Triggers and insert-only statements: [`objects.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/objects.sql) |
| Open never converts or migrates beyond named additive steps | [`open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs), [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Platform for SQLite | macOS or Linux; elsewhere `BackendUnavailable` | [`open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs) |
| Direct opening for Store calls | Linux local ext4/xfs/btrfs (macOS apfs/hfs); else `Unsupported`, no fallback | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| Cross-process writers (Linux and macOS) | Directory `flock` on the Store's parent directory; an uncoordinated raw SQLite writer still gets SQLite `Busy` | [`writer_admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/writer_admission.rs) |
| PostgreSQL without the feature | `BackendUnavailable` | [`selection.rs`](../../../../engine/crates/layerfs-persistence/src/store/selection.rs) |
| PostgreSQL lane wait | `lock_timeout_ms`, default 30 s; then `Busy` | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs) |
| Rows per transaction | `TRANSACTION_ROW_LIMIT` 8 191 | [`publication.rs`](../../../../engine/crates/layerfs-persistence/src/storage/publication.rs) |
| Bytes per publication | The provider's `publication_bytes`: 4 MiB − 1 canonical and physical on SQLite, 16 MiB − 1 on PostgreSQL (`PUBLICATION_BYTES_MAX`); one large payload object exempt | same |
| Disk margin | `STORE_HEADROOM_BYTES` 64 MiB after a pack batch; else `Capacity` | [`headroom.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/headroom.rs) |
| Unknown outcome | `Uncertain` / `UnknownOutcome`; session refuses all later calls | [`records.rs`](../../../../engine/crates/layerfs-persistence/src/backend/records.rs) |
| Work in the lane that grows with tree size, history or Branch count | A defect by owner rule; lineage walks follow runs, not history length | [History publication](history-publication.md#the-publish-lane) |

## Demonstration

Commands are written from source and were not run while writing this page.

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`heads.rs`](../../../../engine/crates/layerfs-persistence/tests/heads.rs) | `layerfs-persistence` | History rows change the heads exactly; a snapshot has the same heads |
| [`writer_admission.rs`](../../../../engine/crates/layerfs-persistence/tests/writer_admission.rs) | `layerfs-persistence` | FIFO admission, the cross-process writer lock and the held lane |
| [`diagnostics_during_lane.rs`](../../../../engine/crates/layerfs-persistence/tests/diagnostics_during_lane.rs) | `layerfs-persistence` | Counter reads do not wait for a held lane |
| [`postgresql_acquisition.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_acquisition.rs) | `layerfs-persistence` | Creating a PostgreSQL Store and Init's acquisition over it |
| [`snapshot.rs`](../../../../engine/crates/layerfs-persistence/tests/snapshot.rs) | `layerfs-persistence` | A live Store copies to a closed file that opens with the saved state |

- **`heads.rs`** creates a Disposable Store, then a project, a fork and a
  Commit, reading `read_heads` after each: one `stack` line, then a `branch`
  line ending ` -`, then a moved head. It snapshots the Store and checks the
  copy's heads and progress reports. macOS or Linux; the no-sidecar check is
  Linux only.
  `cargo +1.85.1 test --locked -p layerfs-persistence --test heads`
- **`writer_admission.rs`** checks first-come order, `Closed` for queued
  writers on close, writers queued behind `publish_lane`, refusal of a
  thread re-entering its own turn, and (with child processes on Linux and
  macOS) the directory lock across processes, concurrent opens and
  reservations, and its hand-over when a holder exits; on Linux it also
  covers `Cancelled` when a signal interrupts the wait.
  `cargo +1.85.1 test --locked -p layerfs-persistence --test writer_admission`
- **`diagnostics_during_lane.rs`** holds the lane for 3 s on one thread and
  requires `storage.diagnostics()` to return within 750 ms. Any platform the
  crate builds on.
  `cargo +1.85.1 test --locked -p layerfs-persistence --test diagnostics_during_lane`
- **`postgresql_acquisition.rs`** creates a schema `lfs533_<tag>_<pid>`,
  refuses a second create, reopens it and runs acquisition windows, aliases,
  roots and refusal order; it drops the schema at the end. Needs
  `--features postgresql` and a server in `LAYERFS_TEST_POSTGRES_URL`; without
  the variable each test prints `NOT_RUN` and checks nothing.
  `cargo +1.85.1 test --locked -p layerfs-persistence --features postgresql --test postgresql_acquisition`
- **`snapshot.rs`** saves content into a Disposable `GroupRowsIndexed` Store
  with acquisition tables, snapshots it, and checks the copy: no sidecar, WAL
  header, preserved pragmas, and the earlier content but not later writes. A
  second snapshot to the same path is refused. macOS or Linux.
  `cargo +1.85.1 test --locked -p layerfs-persistence --test snapshot`

The skeleton listed `sqlite_publication.rs`; it is left out because it opens
Stores with the default `Durable` profile, whose execution the owner has
disabled (`NOT_RUN — disabled by owner until explicit reauthorization`).

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`engine/crates/layerfs-persistence/README.md`](../../../../engine/crates/layerfs-persistence/README.md): covered in [Lifecycle](lifecycle.md) and [Backends](backends.md).
- [`core/docs/architecture/60-shared-store-foundation.md`](../../../../core/docs/architecture/60-shared-store-foundation.md): covered in [Backends](backends.md#one-writer) and [Lifecycle](lifecycle.md#seal).
- [`core/docs/architecture/62-atomic-history-publication.md`](../../../../core/docs/architecture/62-atomic-history-publication.md): covered in [History publication](history-publication.md).
- [`core/docs/architecture/83-store-concurrency-model.md`](../../../../core/docs/architecture/83-store-concurrency-model.md): covered in [Backends](backends.md) and [History publication](history-publication.md#the-publish-lane).
- [`core/docs/architecture/84-global-sqlite-store.md`](../../../../core/docs/architecture/84-global-sqlite-store.md): covered in [SQLite](sqlite.md).
- [`core/docs/architecture/86-remote-store-files.md`](../../../../core/docs/architecture/86-remote-store-files.md): the snapshot and heads digest are covered in [SQLite](sqlite.md#heads-and-snapshots); push and pull are in [Control: Backups](../../control/backups/README.md).
- [`docs/versioned/2.0.0/stores.md`](../../../versioned/2.0.0/stores.md): the user manual for Store kinds; the design is covered in [Lifecycle](lifecycle.md), [SQLite](sqlite.md) and [PostgreSQL](postgresql.md).

---

← [Engine](../README.md) · [Up: Engine](../README.md) · [Store: Lifecycle](lifecycle.md) →
