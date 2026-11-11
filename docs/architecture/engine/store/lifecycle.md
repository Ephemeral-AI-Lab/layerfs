# Lifecycle

[Architecture](../../README.md) › [Engine](../README.md) › [Store](README.md) › Lifecycle

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Selecting, opening and sealing a Store.

| | |
| --- | --- |
| Component | [Store](README.md) |
| Source | [`engine/crates/layerfs-persistence/src/store`](../../../../engine/crates/layerfs-persistence/src/store) |

## Store selection

```text
               StoreSelection
         +-----------+-----------+
         v                       v
  Sqlite(PersistenceConfig)   Postgres(PostgresSelection)
         |                       |
         v                       v
  SqliteHandles::open_*      PostgresHandles::open_*   (feature postgresql;
         |                       |                      without it the arm
         +-----------+-----------+                      is BackendUnavailable)
                     v
                  Handles
       Sqlite(SqliteHandles) | Postgres(PostgresHandles)
```

| Item | Meaning | Source |
| --- | --- | --- |
| `StoreSelection` | The one Store choice: a SQLite file or a PostgreSQL schema. | [`selection.rs`](../../../../engine/crates/layerfs-persistence/src/store/selection.rs) |
| `Handles::open_writable` / `open_read_only` | Open the selected kind with a binding key and cursor key. | [`selection.rs`](../../../../engine/crates/layerfs-persistence/src/store/selection.rs) |
| `Handles::close` | Closes admission; queued and later calls fail `Closed`. | [`selection.rs`](../../../../engine/crates/layerfs-persistence/src/store/selection.rs) |
| `Handles::file` | `Some(&dyn StoreFile)` for SQLite (snapshot, heads digest); `None` for PostgreSQL. | [`selection.rs`](../../../../engine/crates/layerfs-persistence/src/store/selection.rs) |

Nothing converts one kind into the other. A PostgreSQL schema is reached
only by creating it ([PostgreSQL](postgresql.md)).

## Config

| Field | Values | Rule | Source |
| --- | --- | --- | --- |
| `PersistenceConfig.path` | host-local file | Never a network endpoint. | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs) |
| `sqlite_profile` | `Durable`, `Disposable` | Selected before open; never from failure or environment. | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs) |
| `sqlite_pack_layout` | `Monolithic` (36), `GroupRows` (37), `GroupRowsIndexed` (38) | Creation only; open reads the stored version. | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs) |
| `sqlite_acquisition` | `Absent` (+0), `Tables` (+6) | Creation only; schema versions 42–44 carry Init's tables. Versions 36–44 are schema version 2 (#91, blocks); the version 1 Stores 16–18 and 22–24 are refused at open, never converted. | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs) |
| `PostgresSelection.url` | libpq string or URL | Redacted by `Debug`. | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs) |
| `PostgresSelection.schema` | `[a-z_][a-z0-9_]*`, at most 63 bytes, not `pg_*` | Checked by `Settings::new`. | [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/profile.rs) |
| `synchronous_commit` | `Off` (default), `Local`, `RemoteWrite`, `On`, `RemoteApply` | `SET LOCAL` per write. | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs) |
| `lock_timeout_ms` | default 30 000 | Lane wait bound; reaching it is `Busy`. | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs) |

`PersistenceConfig::sqlite(path)` starts from `Durable`, the enum default.
Every product caller overrides it with
`with_sqlite_profile(SqlitePersistenceProfile::Disposable)`; see
[The Disposable profile](#the-disposable-profile).

## Open and handles

```text
SqliteHandles::create(config, policy,     SqliteHandles::open(config, ..)
                      history)              platform is macOS or Linux?
  platform is macOS or Linux?               Session::connect(writable)
  policy and catalog config valid?            user_version in 36..=38
  create_new(path)                            or 42..=44, else Integrity
    (an existing file refuses)              writable only:
  Session::connect(create)                    add_binding_hints
  Session::initialize: schema scripts,        add_lineage (+ backfill)
    application_id, user_version,             add_merge_scratch
    store_policy row, history_meta row        add_file_digests
             \                              validated(..), same checks
              v                             writable only: sweep merge
  validated(session, binding, cursor_key)     records of expired owners
    cursor_key != [0; 32]
    Session::validate: table set, binding,
    recorded schema source and definition
                      |
                      v
  SqliteHandles { storage, history, acquisition }   (one shared Session)
```

| Handle | Type | Used for | Source |
| --- | --- | --- | --- |
| `storage` | `Arc<StorageProvider>` | Pack publication, reads, allocation; the core `PackPersistence` port | [`provider.rs`](../../../../engine/crates/layerfs-persistence/src/storage/provider.rs) |
| `history` | `HistoryProvider` | The core `HistoryCatalog` over the same session | [`catalog.rs`](../../../../engine/crates/layerfs-persistence/src/history/catalog.rs) |
| `acquisition` | `AcquisitionProvider` | Init's working tables; refused unless the Store has them | [`provider.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/provider.rs) |

All three share one `Session`: one connection and one admission queue
([Backends](backends.md)). A read-only open refuses every write before
`BEGIN`. The four `add_*` calls are additive migrations: each runs only when
the recorded schema is exactly the one before that table existed
([`schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/schema.rs)).
Sources: [`open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs),
[`handles.rs`](../../../../engine/crates/layerfs-persistence/src/store/handles.rs).

`PostgresHandles` has the same three handles (`PostgresStorage`,
`HistoryProvider`, `PostgresAcquisition`) and also `binding(selection)`,
which reads the binding key of an existing schema
([`postgresql.rs`](../../../../engine/crates/layerfs-persistence/src/store/postgresql.rs)).

## Seal

```text
SqliteHandles::seal(self)
  drop history and acquisition
  Arc::try_unwrap(storage), (session) -- still shared? ---------> Busy
  Session::seal
    read-only session? ----------------------> Refused "ReadOnly"
    quarantined or open transaction? ---------------------------> Uncertain
    Linux: take the directory writer lock
    macOS: capture file identity; clear SQLITE_FCNTL_PERSIST_WAL
    PRAGMA wal_checkpoint(TRUNCATE) -- not complete -----------> Busy
    close the connection, then release the writer lock
    a -wal, -shm or -journal sidecar left? --------------------> Refused
    macOS: release unused extents past EOF (seal_allocation)
  -> SealedStore { path, profile, sqlite_version, bytes }
```

## Idle checkpoint (#86)

```text
StoreFile::checkpoint(&self)                  (live shared session; stays open)
  Session::checkpoint_idle
    read-only session? ----------------------> Refused "ReadOnly"
    -wal empty or absent? ------------------------------------> Ok(true), no SQL
    FIFO turn (Writer); this thread holds the lane? ----------> Ok(false)
    quarantined or open transaction? ---------------------------> Unknown
    Writer::try_acquire: directory lane (Linux and macOS), one attempt
      another holder (EWOULDBLOCK) ----------------------------> Ok(false)
    a background checkpoint pass running? wait for it to end
    PRAGMA wal_checkpoint(TRUNCATE), once; release the lane
      busy (a reader still in the log) ------------------------> Ok(false)
  -> Ok(true): the -wal is 0 bytes
```

The control layer's open-Store cache runs it when the last call on a SQLite
Store returns, or a Sandbox on it closes, with no other call running in that
process ([`open.rs`](../../../../control/crates/layerfs-store/src/open.rs)). The Store stays open: a
live daemon still keeps its Store across Saves and Commits, and the 60 s idle
close is unchanged. A checkpoint left by `Ok(false)` is never retried; its
frames stay valid and the last close checkpoints them. A process-level close (`close_idle`, which the CLI runs before it exits) waits for a checkpoint still running and then closes the Store; it never skips it. The lane is taken
first because a TRUNCATE checkpoint holds SQLite's WAL write lock, which a
writer of another process would otherwise meet as `Busy`; every host that
opens a SQLite Store takes this lane, so a writer there waits for it or the
checkpoint is left, and only a writer that bypasses the lane meets exact
`Busy`.
Sources: [`session/checkpoint.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/checkpoint.rs),
[`writer_admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/writer_admission.rs).

Seal consumes the sole session of a Store that one process created (host
Init) and hands one closed file to install. It never runs on a live shared
Store: a daemon keeps its Store open across Saves and Commits. The live-Store
copy is `snapshot` (`VACUUM INTO`), described in [SQLite](sqlite.md#lifecycle).
The macOS file control is the one module allowed `unsafe` code
([`file_control.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/file_control.rs)).
Sources: [`seal.rs`](../../../../engine/crates/layerfs-persistence/src/store/seal.rs),
[`lifecycle/seal.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/seal.rs).

## Background checkpoint

Owner approval, 2026-11-07: a Disposable Store's COMMIT no longer
checkpoints. Its connections run `wal_autocheckpoint=0`, so a COMMIT only
appends its frames to the log, and each writable Disposable session keeps a
checkpoint thread with its own connection to the Store file.

```text
Session::run(true, ..) commits, releases the writer lane
  -wal longer than 4 MiB (journal_size_limit)? -- no ----------> nothing
  wake the session's checkpoint thread (a pending wake covers it)
    PRAGMA wal_checkpoint(PASSIVE) on its own connection
      copies the frames no reader still needs; never takes the
      WAL write lock, never waits; the writer goes on meanwhile
  the next write after a complete pass restarts the log from its
  beginning: the log stays near 4 MiB while the thread keeps up
```

A pass that cannot copy every frame (a reader still in the log) leaves them
valid; the next wake, the idle checkpoint (#86) or the last close copies
them. Each wake is a new pass, not a retry. A PASSIVE checkpoint takes no
lock a writer of this or another process needs, so it adds no `Busy`. The
idle checkpoint waits for a running pass before its TRUNCATE. Seal and
close end the thread and close its connection before the session's own
connection. Durable keeps `wal_autocheckpoint=1000` and has no thread.
Measured in-process (Linux release, medians of 15 interleaved Saves on a
loaded host), 500 MiB of incompressible data in one Save: COMMIT 471 to
208 ms, SQL transactions 851 to 591 ms, Save 2373 to 2283 ms; the log is
still 4 MiB after the Save and the Store file is the same size. A non-WAL journal (v0.1.x `MEMORY`/`EXCLUSIVE`, or `TRUNCATE`)
saved more but failed the concurrent-session, second-process, idle
checkpoint and lock-custody tests, so WAL stays.
Source: [`session/background.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/background.rs).

## The Disposable profile

| Setting | Disposable | Durable (disabled) | Source |
| --- | --- | --- | --- |
| identity | `sqlite-wal-off-v2` | `sqlite-wal-full-v2` | [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs) |
| `journal_mode` | `wal` | `wal` | same |
| `synchronous` | 0 (OFF) | 2 (FULL) | same |
| `fullfsync` | 0 | 1 | same |
| `wal_autocheckpoint` | 0 (the session's checkpoint thread copies the log; owner approval, 2026-11-07) | 1000 | same, [`background.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/background.rs) |
| shared | `foreign_keys=1`, `checkpoint_fullfsync=1`, `journal_size_limit=4194304`, `cache_size=-2048`, `mmap_size=0`, `temp_store=2`, `page_size=4096`, `busy_timeout=0` | same | same |

Disposable / WAL / synchronous=OFF is the only permitted SQLite Store
profile. It survives process termination; a kernel or VM crash can corrupt
the Store or lose acknowledged data
([`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs)).
The `Durable` variant still compiles, but its execution is disabled by the
owner until explicit reauthorization. Product openers select Disposable
explicitly and refuse anything else:
[`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs),
[`init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs),
[`live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs),
[`install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs).

Every connection reads its settings back into `ConnectionProfile`, and
`profile::check` refuses a mismatch as `Integrity`. An existing Store whose
journal mode is not `wal` is refused; open never converts a Store. A
PostgreSQL Store has no profile; its write setting is `synchronous_commit`
([PostgreSQL](postgresql.md#synchronous_commit)).

## Related

- [Backends](backends.md): the session, admission and writer each handle shares.
- [SQLite](sqlite.md) and [PostgreSQL](postgresql.md): what `connect` does on each engine.
- [Project Init: Seal and install](../project-init/seal-and-install.md): who calls `seal`.
- [Store calls: Service](../store-calls/service.md): how a process opens the Store for its calls.
- [Glossary](../../glossary.md).

---

← [Engine: Store](README.md) · [Up: Store](README.md) · [Store: Backends](backends.md) →
