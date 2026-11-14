# SQLite

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The SQLite backend of the [Store](README.md): one WAL file every opener shares through one kernel.

| | |
| --- | --- |
| Source | [`backend/sqlite/`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite), [`sql/sqlite/`](../../../../engine/crates/layerfs-persistence/sql/sqlite), [`store/open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs), [`store/seal.rs`](../../../../engine/crates/layerfs-persistence/src/store/seal.rs) |

## Schema

```text
 physical                                 history
 store_policy  id=1  next_pack_id,        history_meta id=1  binding, catalog,
               next_ordinal                            recorded schema text
 object_location  object_id -> pack       layer_stack -> head layer
 pack  pack_id  (+ pack_unit, 37/38)      layer  <- branch.base_layer_id
 inode_value_block  first_ordinal         "commit" <- branch.head_commit_id
 content_signature  slot 0..8191          scope_allocator, binding_hint_block
 store_totals  id=1                       file_digest
 lineage:  layer_lineage, commit_lineage, absorbed, reach
 merge:    merge_operation, merge_record
 Init:     init_operation, init_entry, init_host_file   (42-44 only)
```

| Config at creation | Values | Source |
| --- | --- | --- |
| `sqlite_pack_layout` | `Monolithic` (`user_version` 36): whole `pack.body`; `GroupRows` (37): `control` row plus one `pack_unit` row per group; `GroupRowsIndexed` (38): adds index `pack_unit_mapping` | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs), [`objects_units.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/objects_units.sql) |
| `sqlite_acquisition` | `Absent` (+0) or `Tables` (+6, versions 42–44, created with `auto_vacuum=INCREMENTAL`) | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs), [`acquisition/schema.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/acquisition/schema.sql) |
| `sqlite_profile` | `Disposable` in every product opener (see [Store](README.md)) | [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs) |

Every table is `STRICT`; history keys are `WITHOUT ROWID` blobs whose first
byte tags the kind (`0x31` stack, `0x32` Layer, `0x12` Commit, `0x11`
Branch). History foreign keys are `DEFERRABLE INITIALLY DEFERRED`. Triggers
add each new Commit's and Layer's `logical` to `store_totals` and refuse
UPDATE and DELETE on `pack` and `pack_unit`. Open checks `application_id`,
`user_version`, the exact table set and the recorded schema text
([`schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/schema.rs),
[`history.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/history.sql),
[`objects.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/objects.sql)).

| Open | Steps | Source |
| --- | --- | --- |
| `create` | platform check; policy and catalog config valid; `create_new` (an existing file refuses); schema scripts, `application_id`, `user_version`, `store_policy` and `history_meta` rows | [`open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs) |
| `open_writable` | `user_version` 36–38 or 42–44 (16–18 and 22–24 get `SchemaVersion1`, others `Integrity`); additive `add_binding_hints`, `add_lineage` (with backfill), `add_merge_scratch`, `add_file_digests`, each only when the recorded schema is exactly the one before; then sweep merge records of expired owners | [`open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs), [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs) |
| `open_read_only` | same checks, no migrations; every write refused before `BEGIN` | same |
| `open_diagnostic` | read-only, never changes journal or checkpoint settings; see [Diagnostics](diagnostics.md) | [`open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs) |

## Session

| Step in `Session::connect` | Detail | Source |
| --- | --- | --- |
| Writer lock | When `writable`, takes `Writer::acquire(path)` across connect and releases it after `profile::check`; a read-only connect takes none | [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs) |
| Open | `READ_WRITE` or `READ_ONLY`, `NO_MUTEX`; statement cache 64 | [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs) |
| Guard | `busy_timeout=0`; `SQLITE_DBCONFIG_DEFENSIVE` on | same |
| Settings | `profile::apply`, then read back into `ConnectionProfile`; a mismatch is `Integrity`; an existing Store not in `wal` mode is refused | [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs) |
| Checkpoint thread | writable Disposable session only: a second connection on its own thread; failure to start it refuses the open | [`background.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/background.rs) |
| Last close | A writable session's close makes one non-blocking `Writer::try_acquire(path)` (or reuses the lane an uncertain transaction already holds). With the lane, the main connection's close may be SQLite's last and checkpoint or remove `-wal`/`-shm`. Without it (held elsewhere, or the attempt failed), the close sets `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` first, so SQLite takes no exclusive lock, runs no checkpoint and removes nothing; the next holder of the lane finds the WAL. A close never waits, so a writer kept after an uncertain write cannot hang another session's close. If even that setting fails, the connection is kept until process exit. A read-only or diagnostic session cannot checkpoint or remove those files and takes no lane. The checkpoint thread's connection closes first and never takes it. One exception: an open whose checkpoint thread fails to start drops its new connection after the open's lane is released | [`lifecycle.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/lifecycle.rs) |

| Setting | Disposable | Source |
| --- | --- | --- |
| identity | `sqlite-wal-off-v2` | [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs) |
| `journal_mode`, `synchronous`, `fullfsync` | `wal`, 0 (OFF), 0 | same |
| `wal_autocheckpoint` | 0: the session's checkpoint thread copies the log | same |
| shared | `foreign_keys=1`, `checkpoint_fullfsync=1`, `journal_size_limit=4194304`, `cache_size=-2048`, `mmap_size=0`, `temp_store=2`, `page_size=4096`, `busy_timeout=0` | same |

| Call | Transaction | Writer lock | Source |
| --- | --- | --- | --- |
| `run(false, ..)` | `BEGIN` .. `COMMIT` | no | [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/transaction.rs) |
| `run(true, ..)` | `BEGIN IMMEDIATE` .. `COMMIT` | yes, unless nested in the lane | same |
| `read_mapped` | one read-only statement, implicit snapshot | no | same |
| `read_immutable` | no `BEGIN`; each statement its own snapshot | no | same |

`read_immutable` is limited to rows that never change once visible:
`object_location`, `pack`, `pack_unit` and `inode_value_block`. Anything
that picks a snapshot, such as Branch heads, uses `run`. Each turn adds its
SQL work to the calling thread's counters
([`thread_work.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/thread_work.rs)).

## Lane

```text
 SQLite write: Session::run(true, body)
   1 admission.enter        in-process FIFO turn on this session
   2 state mutex            the connection
   3 Writer::acquire        flock(LOCK_EX) on the Store's parent directory
                            (Linux and macOS)          (skipped if nested)
   4 BEGIN IMMEDIATE        busy_timeout 0 -> Busy
   5 body; COMMIT           or ROLLBACK; Unknown -> quarantine
   6 release the flock; drop the turn

 Session::hold_lane() -> LaneHold (!Send)
   writable? else ReadOnly; admission turn (nested -> Integrity);
   Writer::acquire; turn.hold(): holder = this thread
   inside: run(true, ..) is nested: no flock, BEGIN IMMEDIATE only
```

The lock is taken on a separate descriptor of the Store's directory (on
macOS, a `flock` on the main file itself would meet SQLite's own `fcntl`
locks), never on the main, WAL or SHM file: closing a main-file descriptor
would drop SQLite's own POSIX locks. Several Store files in one directory
share it. It is the same lane on Linux and macOS: a holder process that
exits releases it, a signal during the wait is `Cancelled`, and a writable
open takes it too, so concurrent first openers serialize their opening
PRAGMA statements instead of meeting SQLite's `Busy`; the schema statements
a first open runs then take it once per transaction. Only a writer that
bypasses the lane is left with the exact `Busy`. The lane is one held
admission turn plus that lock, kept by one thread across several short
transactions, and no publication of any process interleaves. `LaneHold` is
`!Send`: it belongs to the thread that took it; a failed release on its drop
quarantines the session and keeps the lock
([`writer_admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/writer_admission.rs),
[`lane.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/lane.rs)).

## Checkpoints

```text
 COMMIT (wal_autocheckpoint=0) appends frames; releases the lane
   -wal longer than 4 MiB? -- no --> nothing
   wake the checkpoint thread: wal_checkpoint(PASSIVE) on its own
   connection; never takes the WAL write lock, never waits

 StoreFile::checkpoint()  (idle: last caller in the process released it)
   read-only? Refused "ReadOnly"   -wal empty? Ok(true), no SQL
   this thread holds the lane? Ok(false)
   writer lane (Linux and macOS), one try; held elsewhere? Ok(false)
   wait for a running PASSIVE pass; wal_checkpoint(TRUNCATE) once
   busy (a reader in the log)? Ok(false)      else Ok(true): -wal is 0 B
```

| Rule | Source |
| --- | --- |
| A pass that cannot copy every frame leaves them valid; the next wake, the idle checkpoint or the last close copies them. Each wake is a new pass, never a retry. | [`background.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/background.rs) |
| A PASSIVE checkpoint takes no lock a writer needs, so it adds no `Busy`. | same |
| The idle TRUNCATE takes the writer lane first, because it holds SQLite's WAL write lock; `Ok(false)` is never retried. The Store stays open. Every host that opens a SQLite Store takes this lane, so a writer waits for it or the checkpoint is left, and only a writer that bypasses the lane meets exact `Busy`. | [`checkpoint.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/checkpoint.rs) |
| Control's open-Store cache calls it when the last call or Sandbox on a Store returns. | [`open.rs`](../../../../control/crates/layerfs-store/src/open.rs) |

## Lifecycle

```text
 live Store (WAL, writers continue)        sole session (host Init)
   snapshot(destination)                     SqliteHandles::seal(self)
     own READ_ONLY connection                  storage still shared? Busy
     VACUUM INTO destination                   read-only? Refused "ReadOnly"
     reopen copy, journal_mode=wal             quarantined / open tx? Uncertain
     wal_checkpoint(TRUNCATE), close           take the writer lock (both OSes)
     no sidecar                                wal_checkpoint(TRUNCATE) or Busy
          \                                    close; sidecar left? Refused
           \                                   release the writer lock
            +-------> SealedStore <-----------+
                                               macOS: release extents past EOF
```

| Operation | Bound or rule | Source |
| --- | --- | --- |
| `seal` | Consumes the sole session; never runs on a live shared Store (a daemon keeps its Store open across Saves and Commits) | [`seal.rs`](../../../../engine/crates/layerfs-persistence/src/store/seal.rs), [`lifecycle/seal.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/seal.rs) |
| macOS extent release | Unused extents past logical EOF move to a same-volume scratch, which is removed; no logical byte is copied or truncated. The one module allowed `unsafe` | [`seal_allocation.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/seal_allocation.rs), [`file_control.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/file_control.rs) |
| `snapshot` | One read transaction; page size, `auto_vacuum`, `user_version` and `application_id` carry over; destination must not exist; a busy source is `Busy`, never retried; a failed copy may leave a file for the caller to remove | [`snapshot.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/snapshot.rs) |
| Free-space guard | A pack batch must leave 64 MiB after counting its bytes twice (WAL, then file); metadata-only batches are not guarded | [`headroom.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/headroom.rs) |
| `reclaim_space(pages)` | `1..=RECLAMATION_PAGE_LIMIT` (512) pages by `incremental_vacuum`, one short write; only Stores created with `auto_vacuum=2` | [`reclamation.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/reclamation.rs) |

## Heads

| Function | Returns | Source |
| --- | --- | --- |
| `read_heads(path)` | Sorted lines `stack <name> <head_layer_id>` and `branch <project> <name> <base_layer_id> <head_commit_id or ->`, lowercase hex | [`heads.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/heads.rs) |
| `heads_digest(path)` | sha256 of those lines, each ending in `\n` | same |

`read_heads` opens its own read-write but `query_only` connection, because
SQLite cannot open a WAL file read-only without a `-shm`. A busy Store is
reported, never waited on. The heads change exactly when history changes, so
the digest identifies a backup copy
([Control: Backups](../../control/backups/README.md)).

## Objects

On layouts 37 and 38 a pack is a `control` row plus one `pack_unit` row per
group, so a selected group is read without the rest of the pack. Publication
writes the pack (or its control and unit rows), `object_location ... ON
CONFLICT DO NOTHING RETURNING`, `store_totals` when bytes were added,
`inode_value_block` only where no stored block overlaps, the released ordinal
tail and the signature upserts, in one write transaction
([`objects/publish.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/objects/publish.rs),
[`units_publish.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/objects/units_publish.rs),
[`units_read.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/objects/units_read.rs)).

## Acquisition

`AcquisitionProvider` gives Project Init indexed working tables in the same
file, so a native directory is imported in bounded windows. Each unit is one
transaction or one read snapshot on the shared session; a released or foreign
operation is refused first. A Store created without the tables refuses every
unit ([`acquisition/`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/acquisition)).
The import algorithm is in [Project Init](../project-init/README.md).

## Merge records

| Rule | Value | Source |
| --- | --- | --- |
| Owner | A merge registers an owner row with a random token; every job checks owner and token in the same statement, so a removed owner is never read as empty | [`merge_records.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/storage/merge_records.rs) |
| Resident first | Records stay in memory up to `RESIDENT_BYTES` (8 MiB) with no statement | same |
| Spilled | Then a write-back buffer of `BUFFER_BYTES` (256 KiB), flushed in one write | same |
| Lease | `MERGE_LEASE` 600 s; writable open sweeps expired owners | same |

## Related

- [Store](README.md): the seam, admission, history calls and errors.
- [PostgreSQL](postgresql.md): the same rows on the other engine.
- [Diagnostics](diagnostics.md): auditing a SQLite Store.
- [Project Init](../project-init/README.md): who creates and seals a Store.
- [Control: Backups](../../control/backups/README.md): who uses `snapshot` and `heads_digest`.
