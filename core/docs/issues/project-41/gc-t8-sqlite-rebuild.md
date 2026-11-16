# T8: SQLite `store.compact` rebuild

> Implementation note for the frozen [`LayerFS-GC-spec.md`](LayerFS-GC-spec.md)
> §7 (SQLite rebuild), §9 (failure and leftovers) and §11 (report). It adds no
> contract: the spec wins and `§N` cites it.

## Placement

`SqliteHandles::compact` is the host-side primitive T10 wraps as
`store.compact`. It consumes the handles like `SqliteHandles::seal`
(`engine/crates/layerfs-persistence/src/backend/sqlite/compact.rs`): a rebuild
replaces the Store file and the caller reopens it afterwards. The public
result is `CompactReceipt`, one value per §11 field, plus the small
`CompactPath`, `CompactKeptWaste`, `CompactResidue` and `CompactZeroReason`
types. The `GcIntent` admission row is registered through the T6
`HistoryCatalog::gc_register` API; a mark is the T7.2 `HistoryProvider::mark`.

## Run (SQLite)

1. **Lane.** `Session::hold_lane()` takes the Store's FIFO turn and the
   cross-process `writer_admission` lock, and keeps both. Every later
   transaction of this thread is nested work of that lane, so no admitted
   writer can interleave between the mark, the seal and the rename.
2. **Checkpoint.** `Session::checkpoint_holding()` runs
   `PRAGMA wal_checkpoint(TRUNCATE)` under the held lane; a nonzero `busy` is
   `Busy`, never retried.
3. **Admission.** The compacting call writes `<store>.compact-op` (the SQLite
   sidecar equivalent of `gc_op`) and registers one Store-scoped
   `gc_intent(kind='compaction')` row with the tuple: generation, mark serial,
   mark pack id, digest, totals, stamp, phase, owner tokens.
4. **Mark.** `mark()` runs in its write transaction under the same lane. The
   §6.1 history closure is written to `kept_node`; the §6.2 closure follows
   logical references and delta bases to its fixpoint, so the packs it keeps
   are already the refined live set. `deadBytesBound` and
   `deadBytesRefined` are therefore equal on both §3 paths.
5. **Path.** `rebuild iff SUM(pack.length) > 0 AND dead * 10 >= SUM(pack.length)`.
   `none` clears its intent and op record and returns; it writes no Store
   table and creates no artifact, so no scratch exists to delete.
6. **Seal.** `Session::seal_holding(writer)` checkpoints, checked-closes,
   checks `-wal`/`-shm`/`-journal`, and captures `(size, mtime, change
   counter)` while the lane is still held (§7 step 1). The sealed tuple and
   the §6.4 `live_heads_digest` are then re-read through an `immutable=1`
   handle and compared with the mark; a difference is `Conflict`.
7. **Build.** `<store>.compact-visited` (`journal_mode=OFF`) holds the visited
   object and live-pack sets. `VACUUM INTO <store>.compact` copies the sealed
   file compactly; inside one transaction the retained rows are kept and the
   complement deleted (dead packs/pack units, unreferenced locations,
   signatures and digests, history/lineage/catalog rows outside `kept_node`,
   markers of collected rows, merge scratch, `gc_intent`/`gc_dead`/`gc_op`,
   acquisition working rows), `binding_hint_block` is cleared,
   `catalog_incarnation` becomes `G + 1`, and `store_totals` is recomputed
   from the retained rows with its `init_*` columns untouched. `VACUUM`, a
   recreated `sqlite_sequence`, and a WAL switch with a zero-`busy` TRUNCATE
   checkpoint finish the artifact. Package immutability triggers are dropped
   for the delete and recreated from their stored SQL.
8. **Verify.** An `immutable=1` reader runs `schema::check`,
   `foreign_key_check`, row counts against the mark (`object_location`,
   `pack`, `kept_node`), both live-heads digests, the allocator tuple, and the
   recomputed totals against their row sums.
9. **Install.** Sidecars and `-mj*` on both files are rechecked, the sealed
   `(size, mtime, change counter)` and the full tuple are re-read last, the
   artifact gets `chmod 0600` and `sync_all`, then `rename` and a directory
   fsync. A rename/fsync failure is installed-but-uncertain: the op record is
   kept at phase `install` and `UnknownOutcome` is returned.
10. **Settle.** The artifact set is removed, then the op record. `residue`
    reports anything that could not be removed.

## Failure and leftovers (§9)

The next `store.compact` reads `<store>.compact-op` under the lane. Because
the lane was free, the recorded owner is gone: it releases that op's admission
intent (`gc_release`), removes the named artifact set, and removes the op
record, judging by the generation it read (`G` = not installed, `G + 1` or
later = installed/advanced). An artifact set that no op record names is
refused `Exists` and is never deleted. A definite pre-install failure deletes
the artifact set and the op record; the old Store is unchanged apart from the
checkpoint. Failpoints (`AfterSeal`, `AfterBuild`, `AfterWalSwitch`,
`AfterVerify`, `BeforeInstall`, `AfterInstall`) exercise the crash state
without killing the process.

## Report (§11)

`CompactReceipt` carries `path`, `generation {before, after}`,
`deadBytesBound`, `deadBytesRefined`, `logicalDiscarded`,
`backendReusable` (free pages × page size), `physicalShrink` (old size − new),
`keptWaste`, `peakTempBytes`, `residue`, `estimate` (`true`: §13) and one
`zeroReason` per zero field from the closed vocabulary. `logicalDiscarded` is
the collected history rows' `logical` bytes; `parallel` note: `estimate` is
always true until #52 measures the arms.

## Tests

`engine/crates/layerfs-persistence/tests/sqlite_compact.rs`:

- empty Store → `none`, `zeroReason: empty-store`, unchanged generation;
- no wholly dead pack → `none`, `deadBytesRefined == deadBytesBound`, no
  artifact, unchanged generation;
- a wholly dead pack → `rebuild`, `G + 1`, dead object gone, retained object
  readable, no artifact/sidecar, full doctor clean;
- retained Commit/Layer history and its objects survive a rebuild; the full
  doctor passes on the installed Store;
- a stray `<store>.compact` with no op record is `Exists` and not deleted;
- `AfterBuild` leaves the staged Store, visited scratch and op record; the
  reopened Store is still at `G`, and the next call settles and rebuilds;
- `AfterInstall` leaves the installed Store at `G + 1` and the op record; the
  next call judges it and settles;
- generation increases once per installed rebuild.

Runs: `cargo test -p layerfs-persistence --offline` (all binaries pass),
`cargo check --workspace --offline` passes. `kernel_guard` in
`layerfs-store-service` fails in this container because `/tmp` is tmpfs and
the guard refuses direct Store access; it is unrelated to this change.

## Gaps (for T10/T12)

- The `(dev, ino)` `StaleStore` fence (§10) is not implemented in the SQLite
  session; install excludes admitted writers by the held lane, not by a
  per-transaction inode check.
- `Capacity` mapping is implemented (`SQLITE_FULL`/`TOOBIG`/`NOMEM`, `ENOSPC`,
  `EDQUOT`, `EFBIG`) but no capacity/failpoint test asserts it.
- The 9%/10%/30% boundary and the `GroupRows`/`GroupRowsIndexed` and
  acquisition layouts are implemented (unit tables conditional, acquisition
  rows cleared, `sqlite_sequence` preserved) but exercised only on the
  Monolithic layout. The `postgresql` feature is not built here.
- Peak temp is the new file plus the visited scratch; journal/WAL sizes
  after the checkpoint are zero and are not added.
- The mark runs before the seal under the same lane (the spec's step 1.5 runs
  it after the seal); the sealed tuple re-read makes the window explicit.
