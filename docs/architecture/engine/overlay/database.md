# Database

[Architecture](../../README.md) › [Engine](../README.md) › [Overlay](README.md) › Database

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

One SQLite file per Workspace.

| | |
| --- | --- |
| Component | [Overlay](README.md) |
| Source | [`engine/crates/layerfs-overlay/src/database`](../../../../engine/crates/layerfs-overlay/src/database), [`engine/crates/layerfs-overlay/sql`](../../../../engine/crates/layerfs-overlay/sql), [`layerfs-store-service/src/overlay`](../../../../engine/crates/layerfs-store-service/src/overlay) |

## One file per Workspace

```text
<n>.sqlite        application_id 1279676210, user_version 39
+-----------------------------------------------------------------+
| fs_inode        (inode, generation)        changed inode rows   |
| fs_entry        (parent, name, generation) names; NULL whiteout |
| fs_data         rowid; (inode, generation, cell_offset) bytes   |
| fs_truncate     (inode, generation, depth) shrink staircase     |
| fs_data_ref     (generation, inode, file_offset) ref. pieces    |
| fs_digest       inode -> BLAKE3 of an append-only file          |
| fs_predecessor  inode -> file a rename replaced                 |
| cleanup_task    (kind, resource, target)   queued reclamation   |
| commit_record   (operation, file_scope, kind, key) op. records  |
+-----------------------------------------------------------------+
```

| Table | Holds | Page |
| --- | --- | --- |
| `fs_inode` | One row per changed inode per generation: kind, mode, mtime, nlink, size, `inherited_cutoff`, `born`, `entries`, `subdirs`, shrink `epoch`/`height`, `referenced`, content `sequence`. Generation `-1` is the orphan domain. | [Namespace](namespace.md#inodes) |
| `fs_entry` | One row per changed name per generation; `inode` NULL is a whiteout; `inherited` says the lower view binds the name. | [Namespace](namespace.md#directory-entries) |
| `fs_data` | Changed bytes: one cell (at most 4096 bytes, optional validity mask) or a dense run of whole cells inside one aligned 32 KiB slot. | [Payload](payload.md#runs-and-cells) |
| `fs_truncate` | The shrink staircase of one layer. | [Payload](payload.md#layers) |
| `fs_data_ref` | Reference pieces: byte ranges that equal committed content; no bytes stored. | [Payload](payload.md#pieces) |
| `fs_digest` | Whole-content BLAKE3 of an append-written file, pinned to a content sequence. | [Payload](payload.md#digests) |
| `fs_predecessor` | The file a rename put this one in place of: an advisory delta base. | [Namespace](namespace.md#predecessors) |
| `cleanup_task` | One row per reclamation item, with phase, cursor and `ready`. | [Lifetimes](lifetimes-and-reclamation.md#garbage-collection) |
| `commit_record` | Indexed operation records, at most 65 536 bytes per value. | [Operation records](operation-records.md) |

All tables are `STRICT`; most are `WITHOUT ROWID`. The schema has no
Workspace row: the bound Workspace's state (`WorkspaceState`) lives in the
`Overlay` value in memory. Source: [`schema.sql`](../../../../engine/crates/layerfs-overlay/sql/schema.sql),
[`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs).

## Template and startup

```text
daemon start (before ControlReady)          Mount of one Workspace
----------------------------------          -----------------------------
Overlay::create_observed(overlay path)      Workspaces::open(incarnation)
  create_new file, mode 0600                  namespace = next (never reused)
  Linux: reopen O_NOFOLLOW, same dev/ino,     Overlay::copy_template_observed(
         fstatfs type recorded                  <dir>/<namespace>.sqlite)
  open SQLite, apply profile                    create_new, copy template bytes
  run schema.sql                                open, apply profile
  read back version 39 / app id                 read back version 39 / app id
close_database_file -> template (closed)        refuse a populated template
                                              Backing::new (dev, ino, nlink 1)
                                              open_workspace(base_root)
```

| Step | Rule | Source |
| --- | --- | --- |
| Create | `create_new`; an existing path is refused; mode `0600` | [`startup.rs`](../../../../engine/crates/layerfs-overlay/src/database/startup.rs) |
| Copy | A plain byte copy into a fresh file, never a hard link; the schema DDL is paid once by the template | [`startup.rs`](../../../../engine/crates/layerfs-overlay/src/database/startup.rs) |
| Template check | Template size must equal its size at creation; the copy must have no row in any table | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs), [`startup.rs`](../../../../engine/crates/layerfs-overlay/src/database/startup.rs) |
| Failure | `Creation` keeps the original result and its `CreationWork`; a failed artifact stays where it is; the roster slot becomes `Failed` | [`startup.rs`](../../../../engine/crates/layerfs-overlay/src/database/startup.rs) |
| Identity | Each engine gets a process-wide, never-reused `identity`; typed tokens carry it and are refused by another engine | [`startup.rs`](../../../../engine/crates/layerfs-overlay/src/database/startup.rs) |

## Profile

| Setting | Value | Why |
| --- | --- | --- |
| `page_size` | 4096 | Fixed page layout |
| `auto_vacuum` | `NONE` | No background file reshaping |
| `journal_mode` | `MEMORY` | Disposable: no on-disk journal |
| `synchronous` | `OFF` | No sync on disposable backing |
| `locking_mode` | `EXCLUSIVE` | One connection owns the file |
| `mmap_size` | 0 | Reads go through the pager |
| `foreign_keys` | `ON` | — |
| `temp_store` | `MEMORY` | — |
| `cache_size` | `-pager_kib` (default 2048 KiB) | Per connection; a hint, not a resident ceiling |
| `max_page_count` | `max_pages`, or `u32::MAX - 1` | `None` means SQLite's format ceiling |
| `busy_timeout` | 0 | No busy handler, no retry |
| `SQLITE_DBCONFIG_DEFENSIVE` | on | — |

Every value is read back into `DatabaseProfile`; any difference is
`Invalid("profile readback differs")`. The SQLite build must be 3.51.0 or
3.53.2 (`rusqlite` bundled on Linux and macOS). Source:
[`profile.rs`](../../../../engine/crates/layerfs-overlay/src/database/profile.rs),
[`Cargo.toml`](../../../../engine/crates/layerfs-overlay/Cargo.toml).

## Statements

```text
caller thread      Shared.state (one Mutex)       Overlay::transaction
     |                     |                               |
     |---- lock ---------->|                               |
     |   wait if behind a waiting Capture/install frontier  |
     |   readiness check (Capture/Install), if asked       |
     |---- action(db) ------------------------------------>|
     |                     |       reads: autocommit       |
     |                     |       1st write: BEGIN IMMEDIATE
     |                     |       ok: COMMIT, journals commit
     |                     |       err: ROLLBACK once, memory
     |                     |            state and tickets restored
     |<--- unlock ---------|                               |
     |   drop OpenDisposals; maintain() steps while pending
```

| Rule | Source |
| --- | --- |
| A job that writes nothing begins no SQLite transaction. | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) |
| SQL text is a fixed set of constants; the prepared-statement cache holds 256, above the about 224 literal texts. | [`statements.rs`](../../../../engine/crates/layerfs-overlay/src/database/statements.rs), [`startup.rs`](../../../../engine/crates/layerfs-overlay/src/database/startup.rs) |
| Every statement is counted under a `StatementKind` family: `Startup`, `Begin`, `Commit`, `Rollback`, `Inode`, `DirectoryEntry`, `Payload`, `Frontier`, `Capture`, `OperationRecord`, `Explain`, `Reclaim`. | [`metrics.rs`](../../../../engine/crates/layerfs-overlay/src/diagnostics/metrics.rs) |
| A capacity I/O error on a full device (`blocks_available() == 0`) becomes `NoSpace` after rollback; any other unsafe state is `Uncertain` and quarantines. | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs), [`physical.rs`](../../../../engine/crates/layerfs-overlay/src/database/physical.rs) |
| A panic inside a job is caught; the connection is kept in `WorkspacePanic` and the Workspace refuses later calls. | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) |

## Accounting

| Observation | What it reports | Source |
| --- | --- | --- |
| `resources()` | `StoredCounts` (row counts, payload and record bytes, owners, maintenance targets), `OpenStateCounts`, allocation, page and freelist counts, `debt_upper_bytes` | [`accounting.rs`](../../../../engine/crates/layerfs-overlay/src/database/accounting.rs) |
| `allocation()` | Logical and allocated bytes and the largest allocation seen; path and descriptor identity checked each time | [`physical.rs`](../../../../engine/crates/layerfs-overlay/src/database/physical.rs) |
| `diagnostics()`, `payload_work()`, `allocation_work()` | Cumulative counters; the owner records their difference per job as `JobWork` | [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs), [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) |

`resources()` visits rows on demand and projects lengths only; ordinary
jobs maintain no accounting row. `debt_upper_bytes` is a declared upper
bound, not exclusive physical debt.

## Related

- [Namespace](namespace.md), [Payload](payload.md), [Operation records](operation-records.md): what the tables hold.
- [Lifetimes and reclamation](lifetimes-and-reclamation.md): `cleanup_task` and file deletion.
- [Daemon: Startup](../daemon/startup.md): where the template is made.
- [Store: SQLite](../store/sqlite.md): the global Store, a different database with its own profile.
- [Glossary](../../glossary.md).

---

← [Engine: Overlay](README.md) · [Up: Overlay](README.md) · [Overlay: Namespace](namespace.md) →
