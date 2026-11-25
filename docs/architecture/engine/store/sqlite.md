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
 Init:     init_operation, init_entry, init_host_file   (62-64 only)
```

| Config at creation | Values | Source |
| --- | --- | --- |
| `sqlite_pack_layout` | `Monolithic` (`user_version` 56): whole `pack.body`; `GroupRows` (57): `control` row plus one `pack_unit` row per group; `GroupRowsIndexed` (58): adds index `pack_unit_mapping` | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs), [`objects_units.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/objects_units.sql) |
| `sqlite_acquisition` | `Absent` (+0) or `Tables` (+6, versions 62–64, created with `auto_vacuum=INCREMENTAL`) | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs), [`acquisition/schema.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/acquisition/schema.sql) |
| `sqlite_profile` | `Normal`, the only profile a Store opens with (see [Store](README.md)) | [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs) |

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
| `open_writable` | `user_version` 56–58 or 62–64, schema version 3 (#776 R1: `commit.attempt` and its partial unique index `commit_attempt`); 36–38 and 42–44 (schema version 2, no Commit attempt) get `SchemaVersion2` ("not supported by this release and not migrated; create a new Store"), 16–18 and 22–24 `SchemaVersion1`, others `Integrity`; then one read (`needs_migration`): a Store that recorded this build's schema source and has `history_meta.next_history_serial` is current and runs no in-place migration and no write transaction for one (#776). Only a Store that recorded an older source (a schema version 1 Store converted by `backup restore` or `tools/store_blocks.py migrate`, or one whose first open was interrupted between steps) runs the additive steps `add_gc_columns`, `add_binding_hints`, `add_lineage` (with backfill), `add_merge_scratch`, `add_file_digests`, `add_gc_columns` again, `add_gc`, `add_kept_node`, `add_position_state`, each in its own write transaction and only when the recorded schema is exactly the one before; then sweep merge records of expired owners | [`open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs), [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs) |
| `open_read_only` | same checks, no migrations; every write refused before `BEGIN` | same |
| `open_diagnostic` | read-only, never changes journal or checkpoint settings; see [Diagnostics](diagnostics.md) | [`open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs) |

## Session

| Step in `Session::connect` | Detail | Source |
| --- | --- | --- |
| Writer lock | When `writable`, or read-only with no `-shm` beside the Store, takes `Writer::acquire(path)` across connect and releases it after the last step that can fail (see Last close) | [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs) |
| Open | `READ_WRITE` or `READ_ONLY`, `NO_MUTEX`; statement cache 64 | [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs) |
| Guard | `busy_timeout=0`; `SQLITE_DBCONFIG_DEFENSIVE` on | same |
| Settings | `profile::apply`, then read back into `ConnectionProfile`; a mismatch is `Integrity`; an existing Store not in `wal` mode is refused | [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs) |
| Checkpoint thread | writable Normal session only: a second connection on its own thread, opened with the session's `synchronous` value and `checkpoint_fullfsync=0`, both read back (the second as `ConnectionProfile::checkpoint_thread_checkpoint_fullfsync`); failure to start it refuses the open. It runs only the routine PASSIVE passes; the idle TRUNCATE and the last close run on the session's own connection, which has `checkpoint_fullfsync=1` (finding #11, narrowed by the owner 2026-11-23; see [Checkpoints](#checkpoints)) | [`background.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/background.rs) |
| Last close | A writable session's close waits for `Writer::acquire(path)` like any write (or reuses the lane an uncertain transaction already holds), so its close, which may be SQLite's last, checkpoints and removes `-wal`/`-shm` under the lane; a pull's verify close and a restore's in-use check rely on that. If the wait itself fails (interrupted, or the directory cannot be opened), the close sets `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` first, so SQLite takes no exclusive lock, runs no checkpoint and removes nothing; there is no second attempt. If even that setting fails, the connection is kept until process exit. A read-only or diagnostic session cannot checkpoint or remove those files, so its close takes no lane. Its open takes the lane only when the Store has no `-shm`: that open creates the sidecars by path and runs SQLite's WAL recovery, which must not meet an admitted writer's transaction or a compaction's install (#776 C3, S6); with `-shm` there, another connection holds the Store and the open maps it without the lane. Raw read-only connections outside a session follow the same rule through `open_admitted_read_only`, which holds the lane through the first read and checks the file identity after it: the doctor's structure probe (`diagnostic::sqlite_structure`), a compaction's leftover-work check (`unfinished_work`) and a snapshot's source; the last two also refuse a quarantined session (`Unknown`) and a file other than the one the session opened (`StaleStore`). The `-shm` check and the first read are separate steps: a race with the last close can still leave empty sidecars, which a compaction's re-check before its rename catches. Every open captures the file identity before and after opening and refuses `StaleStore` if they differ, so a session never fences on a file other than the one it opened. The checkpoint thread's connection closes first and never takes it; after a quarantine closed the session's connection (see [Quarantine](#quarantine)) it may be SQLite's last, so it then closes with `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` and leaves `-wal`/`-shm` in place (kept until process exit if that setting fails). A writable open keeps its own lane until its last step that can fail (the checkpoint thread's start and the file identity capture), so a failed open's connection also closes under it | [`lifecycle.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/lifecycle.rs) |

| Setting | Normal | Source |
| --- | --- | --- |
| identity | `sqlite-wal-normal-v2` | [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs) |
| `journal_mode`, `synchronous`, `fullfsync` | `wal`, 1 (NORMAL), 0 | same |
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
   2 admit_writer           under the turn: quarantined? release, Unknown;
                            Writer::try_acquire once (flock LOCK_EX|LOCK_NB
                            on the Store's parent directory, Linux and macOS)
                            held by another Session or process: drop the
                            turn; Writer::acquire (blocking flock); enter
                            the turn again; quarantined? Unknown
                                                       (skipped if nested)
   3 state mutex            the connection; quarantined? release, Unknown;
                            identity checked under the lane
   4 BEGIN IMMEDIATE        busy_timeout 0 -> Busy
   5 body; COMMIT           or ROLLBACK; Unknown -> quarantine
   6 release the flock (after closing a quarantined connection);
     drop the turn

 Session::hold_lane() -> LaneHold (!Send)
   writable? else ReadOnly; admission turn (nested -> Integrity);
   admit_writer, refusing Unknown when quarantined or a writer is still
   held (before any wait, and again under the re-entered turn);
   turn.hold(): holder = this thread
   inside: run(true, ..) is nested: no flock, BEGIN IMMEDIATE only

 Reads (run(false, ..), read_mapped, read_immutable)
   admission.enter only: never the flock, so they wait only for this
   session's turn holder's short job or its held lane
```

The lock is taken on a separate descriptor of the Store's directory (on
macOS, a `flock` on the main file itself would meet SQLite's own `fcntl`
locks), never on the main, WAL or SHM file: closing a main-file descriptor
would drop SQLite's own POSIX locks. Several Store files in one directory
share it. Who waits where: a read waits only for the session's FIFO turn,
which is held for one short job or by this session's own held lane; a
writer waits for the turn, then for the flock *without* the turn when
another session or process holds the lane (the one non-blocking attempt is
a readiness probe, not a retry), then for the turn again. So a write
waiting for another process's lane (a compaction's mark holds it for tens
of seconds) blocks neither this session's reads nor `diagnostics()` or
anything else that takes only the state mutex (finding #20, #776). Several
of a session's writers can wait in the kernel at once; they resume in the
kernel's order, not the session's FIFO order, and each checks quarantine
again under its re-entered turn. It is the same lane on Linux and macOS: a holder process that
exits releases it, a signal during the wait is `Cancelled`, and a writable
open takes it too, so concurrent first openers serialize their opening
PRAGMA statements instead of meeting SQLite's `Busy`; the schema statements
a first open runs then take it once per transaction. Only a writer that
bypasses the lane is left with the exact `Busy`. The lane is one held
admission turn plus that lock, kept by one thread across several short
transactions, and no publication of any process interleaves. `LaneHold` is
`!Send`: it belongs to the thread that took it; its drop first closes the
connection of a session that nested work quarantined (see below), and a
failed release quarantines the session and keeps the lock
([`writer_admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/writer_admission.rs),
[`lane.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/lane.rs)).
A session that keeps a writer after a failed release (or after a
quarantined close SQLite refused) still holds the lane; its own writers
already waiting in the kernel then wait with every other process until
that session drops it.

Known limit: a compaction's admission looks for other holders of the
Store in `/proc/<pid>/fd`
([`exclusion.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/exclusion.rs)),
which shows only processes in its own PID namespace (and none on macOS). A
daemon in another PID namespace (another container sharing the volume)
that holds the Store open is not seen: the compaction holds the lane for
its whole mark and then fails `Store sidecar remains after seal`. The
sidecar fence and the re-check before the rename stay the guard; nothing
is installed over a held Store.

### Quarantine

An unknown outcome (`SQLITE_IOERR`, `SQLITE_CANTOPEN`, a generic
`SQLITE_ERROR` and every other code without a definite rule, or a body
error that is uncertain at a higher level) quarantines the session: every
later call is refused `Unknown` and nothing is replayed. SQLite may still
hold the attempt's write transaction, which keeps other connections out
just as the lane does, so the lane cannot simply be released. Instead,
while the lane is still held by this thread (the job's own writer, or a
held `LaneHold` around nested work), the quarantined connection is closed:
`sqlite3_close` rolls back whatever transaction is open and drops SQLite's
locks, and may run SQLite's last-close checkpoint under that same lane.
Only then is the lane released, so SQLite teardown still precedes advisory
release, and other sessions and processes take the lane at once instead
of waiting until this session drops, which for a daemon was process exit
(finding #2). The outcome stays unknown and the session stays quarantined;
its caller opens a new session. rusqlite flushes its statement cache before
the close and every statement or BLOB handle borrows the connection, so
SQLite has nothing to refuse the close on; if it ever did with a
transaction still open, the lane would stay with the connection until the
session drops, as before
([`lifecycle.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/lifecycle.rs)).
A read that quarantines holds no lane and closes nothing.

A panic is a quarantine too (#776 S3). A body that panics inside a write
transaction is caught there: the session is quarantined, its connection is
closed under the job's own writer and that writer released (nested, the
held `LaneHold` closes it before the lane goes), and then the panic
resumes. A thread that panics anywhere else while holding the state mutex
poisons it; every later lock recovers that state as quarantined, so a held
lane still closes the connection before it lets go, and a later write
releases a writer the state kept. Before, a caller that catches panics
(the daemon does) kept a poisoned session whose writer held the lane, or
whose nested write transaction stayed open after the lane was released, so
other processes' writers waited forever or failed `Busy`. `hold_lane`
refuses a quarantined session, or one that still holds its writer, with
`Unknown` before the blocking wait: that writer is the very lock it would
wait for (#776 S4).

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
| Routine passes sync the Store file with plain `fsync` (`checkpoint_fullfsync=0` on the thread's connection). The idle TRUNCATE, `checkpoint_holding` and the last close run on the session's own connection with `checkpoint_fullfsync=1`, so on macOS a checkpoint there syncs with `F_FULLFSYNC` (owner, 2026-11-23: narrowed from every pass, which on the macOS host pushed `kept_node_closure`, `kept_objects_sort_files` and store-service `scale_lineage_cap` past the 120 s test ceiling; one sample each). Linux ignores both settings. | same |
| **Remaining window (macOS only).** On macOS `fsync` does not flush the drive's write cache, so after a routine pass the copied pages may still sit only in that cache. Once a pass has copied every frame, the next write restarts the log over those frames; a power loss before the pages leave the cache can then lose database pages, not only the last commits. A later `F_FULLFSYNC` flushes the whole drive cache and closes the window, but SQLite syncs the Store file during a checkpoint only when that checkpoint copies at least one frame: an idle TRUNCATE or last close that finds every frame already copied truncates or deletes the log with no sync at all. Linux is unaffected. No power-loss guarantee is claimed for the Normal profile until layerfs-project#155 §7.5/§7.6 runs. | [`sqlite3.c`](https://sqlite.org/src/file/src/wal.c) `walCheckpoint` |
| The idle TRUNCATE takes the writer lane first, because it holds SQLite's WAL write lock; `Ok(false)` is never retried. The Store stays open. Every host that opens a SQLite Store takes this lane, so a writer waits for it or the checkpoint is left, and only a writer that bypasses the lane meets exact `Busy`. | [`checkpoint.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/checkpoint.rs) |
| Control's open-Store cache calls it when the last call or Sandbox on a Store returns. | [`open.rs`](../../../../control/crates/layerfs-store/src/open.rs) |

## Lifecycle

```text
 live Store (WAL, writers continue)        sole session (host Init)
   snapshot(destination)                     SqliteHandles::seal(self)
     own READ_ONLY connection (open rule)      storage still shared? Busy
     VACUUM INTO destination                   read-only? Refused "ReadOnly"
     reopen copy, journal_mode=wal             quarantined / open tx? Uncertain
     wal_checkpoint(TRUNCATE), close           take the writer lock (both OSes)
     no sidecar                                wal_checkpoint(TRUNCATE) or Busy
          \                                    close; sidecar left? Refused
           \                                   release the writer lock
            +-------> SealedStore <-----------+
                                               macOS: release extents past EOF
                                               (a cloned file refuses: kept)
```

| Operation | Bound or rule | Source |
| --- | --- | --- |
| `seal` | Consumes the sole session; never runs on a live shared Store (a daemon keeps its Store open across Saves and Commits). It holds the writer lane from its checkpoint to its sidecar check; a refused seal closes its connection under that same hold and then frees the lane. A compaction seals through the same steps under the lane it already holds (`seal_holding`); a refused compaction seal hands the lane back with its error instead of freeing it, so the run's cleanup writes under it, including the macOS refusal of a Store path that is a symbolic link or has a second hard link, but without the extent release below: its rename replaces the sealed file | [`seal.rs`](../../../../engine/crates/layerfs-persistence/src/store/seal.rs), [`lifecycle/seal.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/seal.rs) |
| macOS extent release | Only in `seal`, never in a compaction. Unused extents past logical EOF move to a same-volume scratch (`F_TRANSFEREXTENTS`), which is removed; no logical byte is copied or truncated. APFS refuses the move with `ENOENT` for a file that was ever one side of a clone (`cp -c`, `clonefile`, `std::fs::copy`, a Finder duplicate; the origin as well as the copy, also after the other side is deleted or every byte is rewritten) and has blocks past its end. Measured on macOS 26.4.1 the refusal moves nothing, and the release checks that (same length and block count, empty scratch) before it removes the scratch and reports nothing released: the seal succeeds and the file keeps its allocation. Any other unknown result, or a refusal after which a block count changed, is `Uncertain` and keeps the scratch. The one module allowed `unsafe` | [`seal_allocation.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/seal_allocation.rs), [`file_control.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/file_control.rs) |
| `snapshot` | One read transaction; the copy is written with the Store's own `synchronous` (NORMAL), never OFF; page size, `auto_vacuum`, `user_version` and `application_id` carry over; destination must not exist; a busy source is `Busy`, never retried; a failed copy may leave a file for the caller to remove | [`snapshot.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/snapshot.rs) |
| Free-space guard | A pack batch must leave 64 MiB after counting its bytes twice (WAL, then file); metadata-only batches are not guarded | [`headroom.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/headroom.rs) |
| `reclaim_space(pages)` | `1..=RECLAMATION_PAGE_LIMIT` (512) pages by `incremental_vacuum`, stepped in one short write until the free list is empty or the page budget is spent (SQLite 3.40 removes one page per statement, not `N`); only Stores created with `auto_vacuum=2` | [`reclamation.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/reclamation.rs) |

## Compaction rebuild

`SqliteHandles::compact` marks under the writer lane, seals the file, builds
`<store>.compact` beside it from the retained rows, checks it through an
`immutable=1` reader and renames it into place. The build's cost is the copy
of the retained packs, so its steps are shaped around that copy.

| Step | Rule | Source |
| --- | --- | --- |
| Op record | `<store>.compact-op` is rewritten by rename at admission and at the build, verify, install and cleanup phases. The admission record stands through the mark: a later call judges a leftover by the run, generation and owner it names, and `store get --with live` reads its phase as `mark` | [`op.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/op.rs), [`run.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/run.rs) |
| Fresh file | The sealed Store's tables, then the rows, then its indexes and triggers; `journal_mode=OFF` and `synchronous=OFF` while loading, then WAL. The sealed file is attached `immutable=1` with a 256 MiB `mmap_size`. The build's connection has a 64 MiB page cache for the load and 8 MiB while the secondary indexes are built, and its temporary storage is files (`temp_store=FILE`): SQLite sorts an index's keys in memory only up to the cache size and then through runs in a temporary file, and a statement of the copy it answers from a temporary table (the history copies' `IN` lists, the `reach` union, a window's pack list) spills the same way, so nothing the build sorts or lists is held whole. Those files are SQLite's own, unnamed, in its temporary directory (`SQLITE_TMPDIR`, `TMPDIR`, `/var/tmp`, `/usr/tmp`, `/tmp`, then the working directory), not beside the Store and not in `peakTempBytes`; they are bounded by that directory's space, and by memory where it is a memory file system. In memory (the earlier `temp_store=MEMORY`) the index sorts of a Store of 426 thousand Commits held 34 MB at once, about 80 bytes a Commit, and a window of 375 thousand small packs about 4 bytes a pack; with files the same build holds 4 MB for the indexes and takes the same time (0.50 s against 0.47 s for the indexes), and one sort of the copy may hold up to the cache size (20 MB there) | [`build.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/build.rs) |
| File size | macOS sizes the fresh file for the retained pack bytes before the load (one advisory `F_PREALLOCATE`, one `set_len`), because on APFS a page write that also moves the end of the file costs about four times one inside a sized file. The closed file is cut to the pages its header counts. Linux leaves the file to grow | [`space.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/space.rs) |
| Pack copy | The table that holds the pack payload (`pack`, or `pack_unit` in the group-row layouts) is copied one window per statement: `WINDOW_BYTES` (64 MiB) of retained payload, ended early by a dead run longer than `GAP_BYTES` (8 MiB). No plan of windows is held: they are cut from the stream of pack lengths in pack order and each is copied as soon as it is cut. Every other table is one statement. When that table is `pack`, each window's rows are summed for `store_totals` (`stored`, `metadata`) right after the window is written, while its pages are in the build's cache: a pass over `pack` after the load reads one page per pack back from the file, spread over the whole file (22,729 page reads on a 4.9 GB Store of 38,666 packs; 65 ms with the file in memory, 11.9 s in one run under memory pressure). The verification still recomputes both sums from the closed file | [`window.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/window.rs), [`build.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/build.rs) |
| Read-ahead | Before each window the sealed file's matching range is asked for once, through a second read-only descriptor: `F_RDADVISE` on macOS, `POSIX_FADV_WILLNEED` in 128 KiB steps on Linux. Without it a source that is not in memory is read one pack per device round trip. The range assumes packs lie in the file in id order; a hint only, so another layout loses the benefit and nothing else | [`window.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/window.rs) |
| Locators | The mark never holds the locator table. It reads `object_location` once in key order, 512 rows a statement (`WHERE object_id > ? ORDER BY object_id LIMIT ?`), and keeps the last identity of every block: 32 bytes per 512 objects, in an array of which a fixed number of pages is in memory and the rest in a file (the mark's memory, below). An object's position in that order is its rank; what the walk reached is two bits per rank, in an array of the same kind. An identity is resolved by a binary search of the kept identities, which first tries the block of the identity before it and the block after that, and one read of its block, and the walk resolves each wave's identities in identity order, so the blocks are read forward, 16 at a time. A wave with fewer identities than one in 128 of the Store's objects (the retained roots, a Store whose kept set is small) reads no blocks: each identity is one statement on the table's own key that returns its row and counts the rows between it and the end of the block before, which is its rank. The same first pass counts each pack's located records and sorts the locators of the non-payload objects by pack for the walk. On a 4.9 GB Store of 1,693,320 objects a mark reads the table about five times (8.6 million rows and 4,516 single lookups, about 3 s of an 18 to 26 s mark) where one whole read took 1.5 to 2 s; a wave that reaches more identities than the table has leaf pages cannot do better than one forward pass | [`table.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/table.rs), [`metadata/locations.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/metadata/locations.rs) |
| Pack lengths | The decision (dead bytes against a tenth of the stored bytes) and the copy's windows need every pack's length: `gc_pack_lengths`, 4,096 packs a statement in pack order, run inside the mark's transaction after the walk. Where a pack's payload is in its row (the `Monolithic` layout), that scan visits one table page per pack, 38,680 scattered pages on a 4.9 GB Store: about 2.5 s read cold before the mark, 0.1 to 0.9 s after the walk has read the same pages for the packs it reached. The run does not hold them as one entry a pack: they go to a list of bounded memory (65,536 packs, 1 MiB, in memory; past that the list writes sorted runs of 16 bytes a pack to an unnamed file beside the Store, counted in `peakTempBytes`), which gives the total, the retained bytes and the packs back in pack order as often as they are asked for | [`lengths.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/lengths.rs), [`run.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/run.rs), [`gc_pack_lengths.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/queries/history/gc_pack_lengths.sql) |
| Visited scratch | The mark hands the sink no objects while it walks; what it reached is the two bits it keeps for each rank. A `none` run writes no visited file. A `rebuild` writes `<store>.compact-visited` before the mark's transaction commits, in one transaction of its own: the locator blocks are read once more and each reached object goes to `visited` as it is met, so the rows arrive in identity order, and the retained packs go to `live_pack` in pack order from the mark's list of retained packs, read as a stream, 128 rows a statement; both tables are filled by appending and nothing is listed in memory. A `visited` row is the object's whole locator, as the mark read it with the writer lane held, and whether the walk reached it only as a delta base: the build writes the kept `object_location` rows from this table in one forward scan and looks nothing up in the sealed Store's (on a 4.9 GB Store of 1.5 million kept objects the copy went from 5.1 million page-cache lookups and 1.7 to 1.9 s to 0.7 million and about 1 s, and writing the wider rows took the sink about 0.4 s longer; on a Store of 3.0 million one-object packs the two together went from 5.8 s to 4.5 s), after checking that the fresh table has exactly the six locator columns. Writing one row per object during the walk put each row on a random page of the table and took about a third of a `none` run on a 4.9 GB Store. Either path reports as `peakTempBytes` at least the most the run's sort files held at one time, the mark's and the compaction's together (see the mark's memory below) | [`compact_visit.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact_visit.rs) |
| Repack (accepted by the owner, [decision D6](../../../../core/docs/issues/project-41/gc-42-decisions.md)) | A retained pack in which the mark found a record no reached object owns leaves `live_pack` before the pack copy and is written again after the `object_location` copy, from the kept locators in `repack_kept` (a table in the visited scratch, keyed by pack, group and record, read one pack at a time). The plan and its outcome are tables in the scratch as well, not lists in memory: `repack` names the planned packs with the dead bytes the mark found in each, and `rewritten` the packs written with their digest, length and record count, which the verification reads back in pack order through its own read-only connection. The pack keeps its id; its bytes are built by `layerfs-storage`, and its kept objects' locators move to their new group and record numbers. The report's `keptWaste` states the Store the rebuild installs, in all three parts, so a mark of that Store finds the same figures. The dead records of a pack written again leave `dead_in_retained_packs_bytes`. `delta_bases_bytes` charges a record kept only as a delta base the stored width a read of it pays, and a record in a compressed group pays the group's whole decoded length, which shrinks when the group is written again: each `repack_kept` row carries the mark's delta-base-only flag, and the rewrite takes the width of those records in the old pack off the mark's figure and adds their width in the new one, by the function the mark charges them with (on a 4.9 GB Store with 1,063 packs written again the mark's 16,364,922 became 8,263,626, the figure a mark of the rebuilt Store reports) | [`repack.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/repack.rs) |
| Inode value blocks (accepted by the owner, [decision D7](../../../../core/docs/issues/project-41/gc-42-decisions.md)) | The mark notes every ordinal its walk asks the Store for, which is every ordinal a read of a kept inode leaf can ask for. The mark keeps them as runs of consecutive ordinals in a list of bounded memory (8 bytes a run, sorted and joined when read). The build writes those runs to `live_ordinal` in the visited scratch and copies an `inode_value_block` row only when it holds one of them, whole and unchanged; `next_ordinal` does not move. The report's `inode_blocks_bytes` is then the bytes of the blocks the rebuilt Store holds | [`values.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/values.rs), [`lists.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/lists.rs) |
| Verification | One `immutable=1` reader of the fresh file. Its passes over the objects and the packs read their tables forward and need no cache; it has the build's 64 MiB cache (`CACHE_KIB`, which the build's connection has given back) for the foreign keys of the history tables, which are still SQLite's check, one lookup per key at random over those tables (a Store of 426 thousand Commits read 453 thousand pages for it through the default 2 MiB cache and 65 thousand through this one). The passes: the schema check, the pass by pack, one statement for the history row count, the logical sum and the foreign keys of every table but `object_location`, the inode value blocks, the live-heads digest and the tuple. The pass by pack is what checks that every locator's pack is in the file (`object_location` has that one foreign key, and the verification refuses a schema that gives it another). One plain scan of `object_location` counts the objects, sums their canonical lengths and puts every locator into a sorter of bounded memory (8 MiB held, 16 bytes an object in an unnamed file beside the Store past 524,288 objects, counted in `peakTempBytes`); read back they are in pack, group and record order. The `pack` rows are then read in pack order and each takes its locators off that stream: a locator whose pack the file does not hold is the foreign-key failure. SQLite's own check looks the pack row of every object up in object identity order, at random over the pack table's pages: with the earlier 64 MiB cache that was 224,418 page reads and 3.2 s on a 4.9 GB Store of 38,666 packs, and 2.4 million page reads and 4.9 s on a Store of 3.0 million packs, where the merge reads 53,584 and 108,582 pages for the whole verification (0.8 s and 1.0 s, against 4.3 s and 5.4 s). The same pass counts the packs and sums their stored and metadata bytes, and checks each pack the build wrote again: its stored bytes are read whole and hashed against its row and the build's digest, and its locators, which arrive in group and record order, must be exactly the records its own directories state. The `pack` rows are read one window at a time, by the same cut over the retained packs alone (the fresh file holds them in the order they were written), made again from the stream of pack lengths and not held: before a window's rows are read the file is asked for the window's range in one sequential request, when the window's packs average under 512 KiB. A pack's row is one page beside its payload, so read in one pass the rows of a file the kernel no longer holds cost one device round trip per pack (23,330 page reads on a 4.9 GB Store of 38,666 packs: 2.3 s with the file out of memory and 3.9 s as the build left it, against 0.7 s and 0.6 s window by window). The windows only place the read-ahead; the ranges read are consecutive and cover every pack identity, so the pass is one scan of the table. One ordered pass over the block keys requires every ordinal the mark read to be inside a staged block. The sealed file's tuple and digest are read back through one reader as well | [`verify.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/verify.rs), [`staged.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/staged.rs), [`values.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/values.rs) |
| Synchronization | The op record is synchronized at each write, the fresh file before the rename and the directory after it | [`op.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/op.rs), [`install.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/install.rs) |
| Settle | The run lets go of the writer lane after the rename, removes its artifacts, then takes the lane again for the one transaction that clears its admission intent and the `kept_node` rows in the installed Store. A writer has no busy handler, so a settle that wrote beside one would leave it `Busy`; the settle waits its turn instead. The op record is removed last. A definite failure before the rename clears the intent the same way under the run's own lane: a refused seal hands it back, and a session another holder still shares is `Busy`, its lane released before that clone is dropped (the clone's close may be the last and waits for the lane). `clear_installed_mark` sets no busy handler | [`install.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/install.rs) |

### The mark's reads

The mark walks every retained Commit's tree in waves, each in pack order.
A metadata object reached in the pack being walked (anything but a file
payload) is read in that same visit instead of waiting for the next wave: a
Commit's root, inode pages, directory pages, file states and extent pages
are written into one pack, so a pack is acquired once and not once per tree
level. File payloads and objects of other packs wait for the next wave,
where payload record headers are read a pack at a time
([`walk.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/walk.rs),
`reach`).

The walk also meets many versions of the same pages and leaves. Two things
it has already read are kept for the length of one mark, so that the cost
follows the number of objects and not the number of times a later version
leans on an earlier one:

| Kept | Rule | Bound | Source |
| --- | --- | --- | --- |
| Rebuilt chain elements | A page or pooled leaf stored as a delta is rebuilt from its direct base when the walk's reader has already rebuilt and authenticated that base, instead of from its whole chain (up to eight records, programs and identity hashes deep). The kept element carries its chain's depth and charged bytes, so the depth cap and chain budgets refuse what they refused before. A base that has left the memo is read from its own pack again | `CHAIN_MEMO_BYTES` (32 MiB) | [`objects.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects.rs), [`pool/memo.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/memo.rs) |
| Inode values | A read of `inode_value_block` decodes each block whole to answer one ordinal, so the mark keeps every value of the blocks it read, by runs of consecutive ordinals. A leaf of a later Commit names the ordinals of the values it did not change, and those are answered without a statement | `HELD_VALUES` (262,144 values, about 19 MiB), in two generations | [`values.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/values.rs), [`metadata/values.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/metadata/values.rs) |

None of this changes what the mark keeps or verifies: the closure is the
same fixpoint in any order, a rebuilt page is still checked against the
identity its locator states, and a leaf's canonical bytes, values included,
are still hashed. On a 4.9 GB Store with 4,514 Commits (one sample, macOS)
the kept values and chain elements took the walk's pooled record reads from
674,209 to 175,199, its group decompressions from 3.09 GB to 0.43 GB, the
pack bodies it read to decode metadata from 1.29 GB to 0.40 GB and its
`inode_value_block` statements from 28,969 to 10,025 for 9,999 blocks.
Those figures are with a 16 MiB memo and one wave per tree level. Reading
a pack's metadata in one visit took the walk from eight waves to four and,
with the memo at 64 MiB, its pack acquisitions from 27,682 to 9,792 and its
group decompressions to 0.21 GB; in that order a page's base is met less
soon after it was rebuilt, so the pooled record reads are 209,480. A whole
mark went from 84,286 statements, 57,001 BLOB handles and 1.46 GB of BLOB
reads to 19,387, 11,479 and 0.33 GB.

### The mark's memory

A compaction's mark holds a fixed amount of memory, whatever the Store's
size: nothing on its heap grows with the objects the Store locates, the
packs it retains or its Commits
([`objects.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects.rs)).
What grows with the Store is on disk, in unnamed files beside it. Nothing
the mark keeps is a table of locators, a set of identities, a bitmap, a
list of what it visited, a list of history nodes or a list of packs.

Every size below is stated for one budget, `BUDGET_BYTES` (256 MiB); the
sizes add up to less than it and are not all held at one time. A run has
no option for it. The run's sort space (`Space`) carries the budget and
every buffer and cache is its share of it (`Space::scaled`), down to a
least size of its own. Another budget changes sizes only: none selects
another way of working and none lets a set be held whole. The product
always runs with `BUDGET_BYTES`; `HistoryProvider::set_mark_budget` is
hidden and exists so that a test or a tool can run the same code with
every buffer at a fraction of its size.

| Held | Size under the budget | Least | Source |
| --- | --- | --- | --- |
| Block identities | 4 MiB of pages (the blocks of 67 million objects); the array is 32 bytes per 512 located objects, in a file | one page | [`table.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/table.rs), [`paged.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/paged.rs) |
| Reached as a logical reference, reached as a delta base | 8 MiB of pages (33.5 million objects); the array is two bits an object, in a file | one page | same |
| Read-ahead of locator blocks | 16 blocks of 512 rows | the same | [`table.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/table.rs) |
| Sorters | one buffer each, allocated once at its size, and as much again in windows while its runs are merged: reached identities awaiting their locator 8 MiB, the next wave's locators 8 MiB, reached records for kept waste 4 MiB, the non-payload inventory 4 MiB, located records per pack 4 MiB, group layouts 4 MiB, body reads 1 MiB, objects first reached as a delta base 1 MiB, retained packs 1 MiB, runs of inode value ordinals 1 MiB, history nodes still to follow 1 MiB twice, partly dead packs 256 KiB | 32 KiB each | [`spill.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/spill.rs), [`records.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/records.rs), [`lists.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/lists.rs) |
| A sorter's runs | 16 bytes a run, at most as many runs a level as the sorter merges at a time (128 for 8 MiB), one level for each such factor of items: three levels of an 8 MiB sorter are 16 TiB of items | two runs a level | [`spill.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/spill.rs) |
| The non-payload inventory, once sorted | one sealed run in pack order and at most 65,536 pack ids (512 KiB): the id of every 128th row, and of every 256th, 512th and so on once those are that many; a pack's rows are found by that index and read through one 64 KiB window | 16 ids | same |
| The pack being walked | its non-payload objects' locators and the headers of at most 256 payload packs or 16,384 rows; bounded by the pack format (256 groups of 8,191 records), not by the Store | 64 rows | [`walk.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/walk.rs) |
| Kept-waste round | the reached records of at most 4,096 retained packs, 256 packs whose directory is still unread or 262,144 records | 16 packs, 1,024 records | [`waste.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/waste.rs) |
| Chain memo, inode values | 32 MiB and 262,144 values (about 19 MiB), as above | 64 KiB, 512 values | |
| History | one page of 1,024 `kept_node` rows, one batch of 256 nodes and its statement's rows (at most 8,192) | the same | [`kept.rs`](../../../../engine/crates/layerfs-persistence/src/history/kept.rs) |
| The Store's connection and pack reader | a 2 MiB page cache; 2 MiB of pack bodies and one pack larger than that, bounded by the pack format | the same | [`pack_cache.rs`](../../../../core/crates/layerfs-storage/src/encoding/pack_cache.rs) |
| The compaction around the mark | pack lengths 1 MiB and the verification's locators by pack 8 MiB (sorters); the build's page cache 64 MiB for the load and 8 MiB for the indexes; the verification's page cache 64 MiB. The sealed file's 256 MiB mapping is SQLite's and is pages of the file, not heap | 32 KiB a sorter, 512 KiB a page cache; the mapping does not change | [`build.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/build.rs), [`staged.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/staged.rs), [`lengths.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/lengths.rs) |

The two arrays that are as long as the Store has objects are paged
([`paged.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/paged.rs)):
a fixed number of 4 KiB pages in memory and the rest in an unnamed file of
the sort space, read with `read` and `write`, never mapped. A page lives in
the one slot its number selects, so an array that fits its pages never
touches the file (up to 33.5 million objects for the bits and 67 million
for the block identities), a pass in rank order reads and writes each page
once, and nothing is searched or aged. A changed page is written when
another takes its slot. Most of the mark's reads of both arrays are in rank
order: a wave's identities are resolved in identity order, and the passes
that list what was reached run forward. The reads that are not are the
metadata objects of the pack being walked, which are noted as they are
read.

The retained packs and the inode value ordinals the walk read are lists,
not bitmaps. Pack ids and ordinals are reserved in ranges and most of a
range is never used (a 4.9 GB Store of 38,680 packs has ids up to
18,944,001), so a bitmap by pages of numbers held about 61 bytes a
retained pack. The retained packs are met in pack order by the kept-waste
pass and go to a list of 8 bytes a pack; the ordinals go to a list of runs
of consecutive ordinals, 8 bytes a run, which is sorted and joined when it
is read. Both are sorters: 1 MiB in memory and runs in a file past that.

The bytes of the objects kept only as delta bases are summed without a
second set either. An object first reached as a delta base goes to a
sorter with its rank and stored width (11 bytes); after the walk that
stream is read in rank order beside the bits, and an object whose bits
still say delta base only is charged.

The history closure is not one recursive statement on SQLite. Temporary
storage is process memory here (`temp_store=2`), and the one statement
held its edge table, an automatic index over it and the closure itself:
about 300 bytes a Commit, 390 MB at 454 thousand Commits. The mark seeds
`kept_node` with the live LayerStacks, their Layers and the live Branches
(`kept_node_seed`), then follows the edges of at most 256 kept nodes a
statement (`kept_node_expand`), which inserts what it finds and stops after
8,192 rows. A statement that stopped there may have left nodes unfollowed,
so its rows and its batch are followed again; one that did not leaves
nothing. The nodes still to follow wait in a sorter. Edges are followed by
each table's primary key, so no statement scans or sorts. The retained
roots, the live heads and the report's history are then read from
`kept_node` and the catalog a page at a time. The closure took 12.7 s in
1,794 statements for 454 thousand Commits where the one statement took
7.1 s: every node of a long chain is followed twice. A mark that lists what
it kept (`HistoryProvider::mark`) still returns every node and object, and
lists the retained packs, the partly dead packs and the ordinal runs
(`KeptObjects::list`); a compaction's mark returns counts, the two lists as
streams (`retained_packs`, `inode_ordinal_runs`), the stream of partly dead
packs (`each_partly_dead`) and the kept waste. The compaction reads those
streams and lists nothing: the pack copy and the verification cut their
windows from the stream of retained packs beside the stream of pack
lengths, the retained bytes are summed the same way, the inode value blocks
are planned and checked from the ordinal runs, and the repack plan is
written from the stream of partly dead packs. The compaction's own sorters
(the pack lengths, the verification's locators by pack) share the mark's
sort space, so one count and one budget cover both.

A sorter that fills sorts and folds what it holds and writes it as one
sorted run to a file in the Store's directory. Its buffer is allocated once
at its bound, so what it may hold is what it has allocated. Runs are kept
in levels. When a level holds as many runs as the sorter merges at a time
(its buffer in 64 KiB windows: 128 for 8 MiB, 16 for 1 MiB, never fewer
than two), they are merged into one run of the level above and the level's
file is emptied; a read first merges the lowest levels until at most twice
that many runs are left. A merge therefore never reads a run through less
than half a 64 KiB window, however many items were added (a quarter of its
buffer for a sorter at its least size), where one level of runs divides
the same memory among all of them: 2,600 runs of an 8 MiB sorter would be
read through 3 KiB each. While a level is merged, its runs and the merged
run exist together, so a sorter's files hold up to twice its items for
that time.

The files have no name: Linux opens them with `O_TMPFILE`, and
where that is not available (macOS, a file system without it) a file is
created as `.layerfs-mark-<pid>-<random>-<n>` and unlinked at once. A mark
killed between those two steps leaves that name; the next mark that needs
a sort file removes every `.layerfs-mark-*` name in the directory first, so
the Store's own mark cleans up and no other caller has to. The files hold
fixed-width records in the widths the Store's limits allow (33 bytes an
identity awaiting its locator, 54 a located row, 16 a reached record, 22 a
group layout), and the pages of the two arrays. The mark counts what they
hold together, so `peakTempBytes` is the most the mark's and the
compaction's files held at one time. A directory the mark may not write to
fails the mark as refused I/O (`ContinuityUnavailable`, the class a
read-only Store reports); a full disk, quota or file-size limit fails it as
`Capacity("disk space")`; any other error is `Integrity`. Nothing is left
behind in any of them.

A wave's reached identities are sorted by identity and resolved in one
forward pass over the locator blocks; the located rows are sorted by pack,
group and record and walked a run of packs at a time. A metadata object of
the pack being walked is still read in the same visit. A delta base that is
not in the pack being walked is found by one point lookup in
`object_location`'s own key. Kept waste is attributed by merging three
pack-ordered streams: the located counts, the reached records and the
group layouts the walk saw.

One sample each, macOS, a 4.9 GB Store of 1,693,320 located objects,
38,680 packs and 4,514 Commits. Peak memory footprint of the whole run:

| Run | Budget 256 MiB | Budget 1.28 MiB (one 200th) |
| --- | --- | --- |
| `none` compaction | 171 MB, sort files 107.6 MB | 32 MB, sort files 138.2 MB |
| `rebuild` (1,528,899 objects kept, 1,063 packs written again) | 205 MB | 32 MB |

Both `none` runs wrote the same report apart from `peakTempBytes`, and the
two rebuilt files are the same bytes apart from the time of the mark. With
one 200th of the budget every buffer stands to this Store as the product's
buffers stand to a Store of 200 times its objects, except the chain memo
and the inode values, which then hold too little for a history of this
depth: that run makes 461 thousand statements where the product's makes
172 thousand, and takes 31 s against 18 s.

The same mark, listing what it kept, with the budget cut down (the peak
includes the list of 1.5 million objects it returns):

| Budget | Pages asked for | Not held | Read from the file | Written | Sort files |
| --- | --- | --- | --- | --- | --- |
| 256 MiB | 7,721,502 | 130 | 0 | 0 | 107.6 MB |
| one 10th | the same | 130 | 0 | 0 | 130.1 MB |
| one 50th | the same | 114,324 | 114,194 | 113,634 | 137.2 MB |
| one 200th | the same | 169,107 (2.2%) | 168,977 | 166,835 | 138.2 MB |
| one 4,096th (the least) | the same | 224,937 (2.9%) | 224,807 | 182,790 | 131.5 MB |

The sort files hold 64 bytes a located object at the most while no sorter
has runs enough to merge a level, and up to 82 bytes once they do. The mark
that held its lists peaked at 187 MB on this Store and 390 MB with 450
thousand more Commits; the mark that held the locator table and the visited
set in memory peaked at 1.73 GB.

## Heads

| Function | Returns | Source |
| --- | --- | --- |
| `read_heads(path)` | Sorted lines `stack <name> <head_layer_id>` and `branch <project> <name> <base_layer_id> <head_commit_id or ->` for live rows only, lowercase hex | [`heads.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/heads.rs) |
| `heads_digest(path)` | sha256 of those lines, each ending in `\n` | same |

`read_heads` opens its own read-write but `query_only` connection, because
SQLite cannot open a WAL file read-only without a `-shm`. Its close may be
SQLite's last, which checkpoints and removes `-wal`/`-shm`, so it holds the
writer lane from before the open to after the close, as a writable session
does (#776 C3, C4). It waits for the lane like any writer; a Store busy once
the lane is held is reported, never retried. The heads change exactly when history changes, so
the digest identifies a backup copy
([Control: Backups](../../control/backups/README.md)). Both reads are filtered
by the same §4.2 markers the catalog uses: a row named by `branch_retired`
leaves no `branch` line, and a row named by `layer_stack_retired` leaves
neither its `stack` line nor its Branches' lines. A soft removal is a marker,
not a delete, so the base rows can still be present while the backup digest
already names the visible history; `visibility_guard` scans inline SQL in
`src` too, which is what holds `heads.rs` to that filter.

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

A mark learns a payload record's delta base and stored width from the record's
first bytes, through one BLOB handle moved from pack to pack
([`objects/read.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/objects/read.rs),
[`edges.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/edges.rs)).
On the one-row layout a body past its row's page is a chain of 4 KiB pages
that SQLite reads one page at a time. Two wanted ranges no more than
`RANGE_FILL_BYTES` (16 KiB) apart are therefore read as one, and a group whose
wanted records lie that close on average is read whole. Skipping a page
between two headers saves that page and ends the run of consecutive reads the
operating system reads ahead of, so the next header waits for the device on
its own. Measured on a 4.9 GB Store whose chunk records average 3.9 KB, about
one header per page (macOS, one sample, both arms run at once): the header
reads of one mark took 28.4 s reading headers only and 13.4 s reading through,
from a file not in memory; 0.70 s and 0.85 s from memory. BLOB reads fell from
1,556,579 to 256,704 and BLOB bytes rose from 1.46 GB to 5.92 GB. A gap wider
than the fill is still skipped, so a pack of large records costs its headers
only. The group-row layouts and PostgreSQL ask for exact ranges
(`Mechanics::pack_range_fill` is 0): there every byte read is a byte
transferred.

## Acquisition

`AcquisitionProvider` gives Project Init indexed working tables in the same
file, so a native directory is imported in bounded windows. Each unit is one
transaction or one read snapshot on the shared session; a released or foreign
operation is refused first. A Store created without the tables refuses every
unit ([`acquisition/`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/acquisition)).
The import algorithm is in [Project Init](../project-init/README.md).

The session's epoch (the identity of its first operation, which later
operations reuse) is read without a lock, so a live owner's unit never waits
on, or is refused by, a `begin` that waits for another process's lane. Only
while the epoch is still zero does a `begin` hold a mutex across its
transaction, so two first `begin`s cannot both claim one: the second waits
and reuses the first's. `abandoned` and `discard_abandoned` wait for such a
first `begin` too, so its new operation is never taken for an abandoned one
([`epoch.rs`](../../../../engine/crates/layerfs-persistence/src/backend/epoch.rs),
shared with PostgreSQL). Only Project Init begins operations in production,
from one thread.

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
