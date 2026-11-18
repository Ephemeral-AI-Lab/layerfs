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
| `sqlite_profile` | `Normal` in every product opener since 2026-11-17 (see [Store](README.md)); a test may select `Disposable` | [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs) |

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
| Checkpoint thread | writable Normal or Disposable session only: a second connection on its own thread, opened with the session's `synchronous` value and read back; failure to start it refuses the open | [`background.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/background.rs) |
| Last close | A writable session's close waits for `Writer::acquire(path)` like any write (or reuses the lane an uncertain transaction already holds), so its close, which may be SQLite's last, checkpoints and removes `-wal`/`-shm` under the lane; a pull's verify close and a restore's in-use check rely on that. If the wait itself fails (interrupted, or the directory cannot be opened), the close sets `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` first, so SQLite takes no exclusive lock, runs no checkpoint and removes nothing; there is no second attempt. If even that setting fails, the connection is kept until process exit. A read-only or diagnostic session cannot checkpoint or remove those files and takes no lane, so it never waits behind a writer kept after an uncertain write. The checkpoint thread's connection closes first and never takes it. One exception: an open whose checkpoint thread fails to start drops its new connection after the open's lane is released | [`lifecycle.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/lifecycle.rs) |

| Setting | Normal (Disposable where it differs) | Source |
| --- | --- | --- |
| identity | `sqlite-wal-normal-v2` (`sqlite-wal-off-v2`) | [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs) |
| `journal_mode`, `synchronous`, `fullfsync` | `wal`, 1 (NORMAL; Disposable 0, OFF), 0 | same |
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
                                               (a cloned file refuses: kept)
```

| Operation | Bound or rule | Source |
| --- | --- | --- |
| `seal` | Consumes the sole session; never runs on a live shared Store (a daemon keeps its Store open across Saves and Commits). It holds the writer lane from its checkpoint to its sidecar check; a refused seal closes its connection under that same hold and then frees the lane. A compaction seals through the same steps under the lane it already holds (`seal_holding`), including the macOS refusal of a Store path that is a symbolic link or has a second hard link, but without the extent release below: its rename replaces the sealed file | [`seal.rs`](../../../../engine/crates/layerfs-persistence/src/store/seal.rs), [`lifecycle/seal.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/seal.rs) |
| macOS extent release | Only in `seal`, never in a compaction. Unused extents past logical EOF move to a same-volume scratch (`F_TRANSFEREXTENTS`), which is removed; no logical byte is copied or truncated. APFS refuses the move with `ENOENT` for a file that was ever one side of a clone (`cp -c`, `clonefile`, `std::fs::copy`, a Finder duplicate; the origin as well as the copy, also after the other side is deleted or every byte is rewritten) and has blocks past its end. Measured on macOS 26.4.1 the refusal moves nothing, and the release checks that (same length and block count, empty scratch) before it removes the scratch and reports nothing released: the seal succeeds and the file keeps its allocation. Any other unknown result, or a refusal after which a block count changed, is `Uncertain` and keeps the scratch. The one module allowed `unsafe` | [`seal_allocation.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/seal_allocation.rs), [`file_control.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/file_control.rs) |
| `snapshot` | One read transaction; page size, `auto_vacuum`, `user_version` and `application_id` carry over; destination must not exist; a busy source is `Busy`, never retried; a failed copy may leave a file for the caller to remove | [`snapshot.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/snapshot.rs) |
| Free-space guard | A pack batch must leave 64 MiB after counting its bytes twice (WAL, then file); metadata-only batches are not guarded | [`headroom.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/headroom.rs) |
| `reclaim_space(pages)` | `1..=RECLAMATION_PAGE_LIMIT` (512) pages by `incremental_vacuum`, stepped in one short write until the free list is empty or the page budget is spent (SQLite 3.40 removes one page per statement, not `N`); only Stores created with `auto_vacuum=2` | [`reclamation.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/reclamation.rs) |

## Compaction rebuild

`SqliteHandles::compact` marks under the writer lane, seals the file, builds
`<store>.compact` beside it from the retained rows, checks it through an
`immutable=1` reader and renames it into place. The build's cost is the copy
of the retained packs, so its steps are shaped around that copy.

| Step | Rule | Source |
| --- | --- | --- |
| Op record | `<store>.compact-op` is rewritten by rename at admission and at the build, verify, install and cleanup phases. The admission record stands through the mark: a later call judges a leftover by the run, generation and owner it names, and `store.status` reads its phase as `mark` | [`op.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/op.rs), [`run.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/run.rs) |
| Fresh file | The sealed Store's tables, then the rows, then its indexes and triggers; `journal_mode=OFF` and `synchronous=OFF` while loading, then WAL. The sealed file is attached `immutable=1` with a 256 MiB `mmap_size` | [`build.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/build.rs) |
| File size | macOS sizes the fresh file for the retained pack bytes before the load (one advisory `F_PREALLOCATE`, one `set_len`), because on APFS a page write that also moves the end of the file costs about four times one inside a sized file. The closed file is cut to the pages its header counts. Linux leaves the file to grow | [`space.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/space.rs) |
| Pack copy | The table that holds the pack payload (`pack`, or `pack_unit` in the group-row layouts) is copied one window per statement: `WINDOW_BYTES` (64 MiB) of retained payload, ended early by a dead run longer than `GAP_BYTES` (8 MiB). Every other table is one statement. When that table is `pack`, each window's rows are summed for `store_totals` (`stored`, `metadata`) right after the window is written, while its pages are in the build's cache: a pass over `pack` after the load reads one page per pack back from the file, spread over the whole file (22,729 page reads on a 4.9 GB Store of 38,666 packs; 65 ms with the file in memory, 11.9 s in one run under memory pressure). The verification still recomputes both sums from the closed file | [`window.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/window.rs), [`build.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/build.rs) |
| Read-ahead | Before each window the sealed file's matching range is asked for once, through a second read-only descriptor: `F_RDADVISE` on macOS, `POSIX_FADV_WILLNEED` in 128 KiB steps on Linux. Without it a source that is not in memory is read one pack per device round trip. The range assumes packs lie in the file in id order; a hint only, so another layout loses the benefit and nothing else | [`window.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/window.rs) |
| Locators | The mark reads every `object_location` row once, in the table's key order (no sort in SQL; 1,693,320 rows on a 4.9 GB Store, about 1.5 to 2 s when the file is not in memory). The walk's identity map is filled as the rows arrive. The pack, group and record inventory that kept waste is attributed from is built afterwards from the same rows sorted by locator, each ordered map collected from a run already in its order; filling the three nested maps in identity order took 0.76 s, sorted about 0.2 s | [`objects.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects.rs) |
| Pack lengths | The decision (dead bytes against a tenth of the stored bytes) and the copy's windows need every pack's length: one statement, `gc_pack_lengths`, run inside the mark's transaction after the walk. Where a pack's payload is in its row (the `Monolithic` layout), that scan visits one table page per pack, 38,680 scattered pages on a 4.9 GB Store: about 2.5 s read cold before the mark, 0.1 to 0.9 s after the walk has read the same pages for the packs it reached | [`run.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/run.rs), [`gc_pack_lengths.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/queries/history/gc_pack_lengths.sql) |
| Visited scratch | The mark hands every reached object and its pack to a sink that keeps them in memory (40 bytes an object) until the path is chosen. A `none` run drops them: it creates no temporary file and reports `peakTempBytes` 0. A `rebuild` writes `<store>.compact-visited` before the mark's transaction commits, in one transaction of its own: the `visited` rows in identity order and the retained packs (`live_pack`) in pack order, 128 rows a statement, so both tables are filled by appending. Writing one row per object during the walk put each row on a random page of the table and took about a third of a `none` run on a 4.9 GB Store | [`compact_visit.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact_visit.rs) |
| Repack (prototype, [decision D6](../../../../core/docs/issues/project-41/gc-42-decisions.md)) | A retained pack in which the mark found a record no reached object owns leaves `live_pack` before the pack copy and is written again after the `object_location` copy, from the kept locators in `repack_kept` (a table in the visited scratch, keyed by pack, group and record, read one pack at a time). The pack keeps its id; its bytes are built by `layerfs-storage`, and its kept objects' locators move to their new group and record numbers | [`repack.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/repack.rs) |
| Inode value blocks (prototype, [decision D7](../../../../core/docs/issues/project-41/gc-42-decisions.md)) | The mark notes every ordinal its walk asks the Store for, which is every ordinal a read of a kept inode leaf can ask for. The build writes them as runs to `live_ordinal` in the visited scratch and copies an `inode_value_block` row only when it holds one of them, whole and unchanged; `next_ordinal` does not move. The report's `inode_blocks_bytes` is then the bytes of the blocks the rebuilt Store holds | [`values.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/values.rs), [`ordinals.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects/ordinals.rs) |
| Verification | One `immutable=1` reader of the fresh file: the schema check, the `pack` count and sums, one statement that reads the other row counts and recomputed sums with one pass per table, the live-heads digest and the tuple. `pack` is read first and one window at a time, by the build's plan for the fresh file (the retained packs alone, in the order they were written): before a window's rows are read the file is asked for the window's range in one sequential request, when the window's packs average under 512 KiB. A pack's row is one page beside its payload, so read in one pass the rows of a file the kernel no longer holds cost one device round trip per pack (23,330 page reads on a 4.9 GB Store of 38,666 packs: 2.3 s with the file out of memory and 3.9 s as the build left it, against 0.7 s and 0.6 s window by window). The windows only place the read-ahead; the ranges read are consecutive and cover every pack identity, so the count and sums are those of one pass over the table. The rows are then in the reader's cache for the foreign-key pass. The reader has the build's 256 MiB page cache: the foreign-key pass looks up the pack row of every object in object identity order, and through the default 2 MiB cache almost every lookup read its page from the file again (1.71 million page reads against 47 thousand on a 4.9 GB Store of 38,666 packs). Each pack written again is read back whole and hashed against its row and the build's digest, and one plain scan of `object_location` requires its locators to be exactly the records its own directories state. One ordered pass over the block keys requires every ordinal the mark read to be inside a staged block. The sealed file's tuple and digest are read back through one reader as well | [`verify.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/verify.rs), [`repack.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/repack.rs), [`values.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/values.rs) |
| Synchronization | The op record is synchronized at each write, the fresh file before the rename and the directory after it | [`op.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/op.rs), [`install.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/install.rs) |
| Settle | The run lets go of the writer lane after the rename, removes its artifacts, then takes the lane again for the one transaction that clears its admission intent and the `kept_node` rows in the installed Store. A writer has no busy handler, so a settle that wrote beside one would leave it `Busy`; the settle waits its turn instead. The op record is removed last | [`install.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/compact/install.rs) |

### The mark's reads

The mark walks every retained Commit's tree in waves, each in pack order.
A metadata object reached in the pack being walked (anything but a file
payload) is read in that same visit instead of waiting for the next wave: a
Commit's root, inode pages, directory pages, file states and extent pages
are written into one pack, so a pack is acquired once and not once per tree
level. File payloads and objects of other packs wait for the next wave,
where payload record headers are read a pack at a time
([`objects.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects.rs),
`ClosureSource::admit`).

The walk also meets many versions of the same pages and leaves. Two things
it has already read are kept for the length of one mark, so that the cost
follows the number of objects and not the number of times a later version
leans on an earlier one:

| Kept | Rule | Bound | Source |
| --- | --- | --- | --- |
| Rebuilt chain elements | A page or pooled leaf stored as a delta is rebuilt from its direct base when the walk's reader has already rebuilt and authenticated that base, instead of from its whole chain (up to eight records, programs and identity hashes deep). The kept element carries its chain's depth and charged bytes, so the depth cap and chain budgets refuse what they refused before. A base that has left the memo is read from its own pack again | `CHAIN_MEMO_BYTES` (64 MiB) | [`objects.rs`](../../../../engine/crates/layerfs-persistence/src/history/objects.rs), [`pool/memo.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/memo.rs) |
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

## Heads

| Function | Returns | Source |
| --- | --- | --- |
| `read_heads(path)` | Sorted lines `stack <name> <head_layer_id>` and `branch <project> <name> <base_layer_id> <head_commit_id or ->` for live rows only, lowercase hex | [`heads.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/heads.rs) |
| `heads_digest(path)` | sha256 of those lines, each ending in `\n` | same |

`read_heads` opens its own read-write but `query_only` connection, because
SQLite cannot open a WAL file read-only without a `-shm`. A busy Store is
reported, never waited on. The heads change exactly when history changes, so
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
