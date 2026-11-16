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
   `none` writes the §6.3 `<store>.compact-visited` scratch, measures it into
   `peakTempBytes`, deletes it, clears the mark's transient `kept_node` rows,
   releases the admission intent and removes its op record. It changes no
   persistent Store row and bumps no generation, and it leaves no artifact: the
   live Store is exactly as it was before the call.
6. **Seal.** `Session::seal_holding(writer)` checkpoints, checked-closes,
   checks `-wal`/`-shm`/`-journal`, and captures `(size, mtime, change
   counter)` while the lane is still held (§7 step 1). The sealed tuple and
   the §6.4 `live_heads_digest` are then re-read through an `immutable=1`
   handle and compared with the mark; a difference is `Conflict`.
7. **Build.** `<store>.compact-visited` (`journal_mode=OFF`) holds the visited
   object and live-pack sets, joined from the mark's own object locators (the
   mark reads `object_location` once). `VACUUM INTO <store>.compact` copies the
   sealed file compactly; with `PRAGMA foreign_keys=OFF` and
   `defer_foreign_keys=ON` (an explicit deviation from §7 step 4's
   `foreign_keys=ON`: SQLite's deferred counter refuses a COMMIT on this
   delete order even when `pragma_foreign_key_check` reports the same final
   state clean — the verification's check is the guard), one transaction keeps
   the retained rows and deletes the complement (dead packs/pack units,
   unreferenced locations, signatures and digests, history/lineage/catalog rows
   outside `kept_node`, markers of collected rows, merge scratch, every
   `gc_intent` row except this run's own, `gc_dead`/`gc_op`, acquisition working
   rows), `binding_hint_block` is cleared,
   `catalog_incarnation` becomes `G + 1`, and `store_totals` is recomputed
   from the retained rows with its `init_*` columns untouched. `VACUUM`, a
   recreated (and checked never to fall) `sqlite_sequence`, and a WAL switch
   with a zero-`busy` TRUNCATE checkpoint finish the artifact. Package
   immutability triggers are dropped for the delete and recreated from their
   stored SQL. Every artifact and the op record are fsynced (file) and the
   Store directory is fsynced once the set is complete.
8. **Verify.** An `immutable=1` reader runs `schema::check`,
   `foreign_key_check`, row counts against the mark (`object_location`,
   `pack`, `kept_node`), both live-heads digests, the allocator tuple, and the
   recomputed totals against their row sums.
9. **Install.** Sidecars and `-mj*` on both files are rechecked, the sealed
   `(size, mtime, change counter)` and the full tuple are re-read last, the
   artifact gets `chmod 0600` and `sync_all`, then `rename` and a directory
   fsync. A rename/fsync failure is installed-but-uncertain: the op record is
   kept at phase `install` and `UnknownOutcome` is returned.
10. **Settle (`cleanup`).** The op records the `cleanup` phase, the artifact
    set is removed, and only then are the admission intent and the transient
    `kept_node` rows cleared from the installed Store in one raw transaction
    (the run holds no session after the seal). The op record is removed last;
    `residue` reports anything that could not be removed, and a failed intent
    clear keeps the op record as the next run's recovery breadcrumb. Between
    steps 6 and 10 the intent lives in the sealed file and then in the installed
    Store, so a crash always leaves one a later `store.compact` adopts.

## Failure and leftovers (§9)

The next `store.compact` reads `<store>.compact-op` under the lane. Because
the lane was free, the recorded owner is gone: it releases that op's admission
intent (`gc_release`), removes the named artifact set, and removes the op
record, judging by the generation it read (`G` = not installed, `G + 1` or a
Store that advanced further = installed/advanced). A record from a *later*
generation is `Integrity` and nothing it names is deleted. An artifact set that
no op record names is refused `Exists` and is never deleted, except the
incomplete `<store>.compact-op.tmp`, which is this Store's own op write that
died before its rename (the open artifact list owns it). A definite
pre-install failure deletes the artifact set and the op record (releasing the
intent with a raw delete after the seal); the old Store is unchanged apart from
the checkpoint and the transient `kept_node` rows, which the next compaction
clears. Failpoints (`AfterSeal`, `AfterBuild`, `AfterWalSwitch`,
`AfterVerify`, `BeforeInstall`, `AfterInstall`) exercise the crash state
without killing the process.

## Artifact set

`<store>.compact{,-journal,-wal,-shm}`, `<store>.compact-visited` and its
`-journal`, the variable `<store>.compact-mj*` super-journals, and the
incomplete `<store>.compact-op.tmp`. `.compact-vacuum` is not an artifact:
SQLite's `VACUUM` writes its rewrite to an unlinked `etilqs_*` temp database,
never a file beside the Store; the old fixed list over-reported it. The op
record `<store>.compact-op` is settled separately and is not an artifact.

## Report (§11)

`CompactReceipt` carries `path`, `generation {before, after}`,
`deadBytesBound`, `deadBytesRefined`, `logicalDiscarded`,
`backendReusable` (free pages × page size), `physicalShrink` (old size − new),
`keptWaste`, `peakTempBytes`, `residue`, `estimate` (`true`: §13) and one
`zeroReason` per zero field from the closed vocabulary. `keptWaste.deltaBases`
keeps at most `DELTA_BASE_REFERENCES_CAP` (1024) identities; when the list is
cut the wire renders it `{value, cap}` (S7) and `bytes` stays the whole kept
set's bytes. `store.status`'s SQLite phase list is now `mark`, `build`,
`verify`, `install`, `cleanup` (§11 amendment, see `gc-spec-gaps.md` G7). `logicalDiscarded` is
the collected history rows' `logical` bytes; `parallel` note: `estimate` is
always true until #52 measures the arms.

`peakTempBytes` on `none` is the bytes of the visited scratch the path creates
and deletes; on `rebuild` it is the §11 peak temporary space as a documented upper bound
(T-G72): `staged_bytes + journal_bound + target_bytes + visited_bytes`. The
staged copy holds the sealed Store's whole extent until the rewrite truncates
it (`VACUUM INTO`); the rewrite holds SQLite's unlinked temp database (the new
file) beside it and journals the pages it overwrites; the journal bound is one
rollback-journal record per staged page (`page_size + 8` B) plus 8192 B of
header. The 50 GB arm's bound is 147.609 GB against a 143.1 GB sampled temp
peak; the old `new + visited` receipt (45.784 GB) dropped the 44.6 GB
rollback journal (T-G66 finding, T-G72 fix). Upstream issue: [Ephemeral-AI-Lab/layerfs#695](https://github.com/Ephemeral-AI-Lab/layerfs/issues/695).

## Tests

`engine/crates/layerfs-persistence/tests/sqlite_compact.rs`:

- empty Store → `none`, `zeroReason: empty-store`, unchanged generation, a
  visited scratch created and deleted and counted in `peakTempBytes`;
- a `none` path leaves no `kept_node` row and no `gc_intent` row
  (`a_none_path_leaves_no_mark_state`);
- an incomplete `.compact-op.tmp` is removed, not refused
  (`an_incomplete_op_record_is_removed_rather_than_refused`);
- a leftover record from another generation is `Integrity` and deletes nothing
  (`a_leftover_record_for_another_generation_is_refused`);
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

T-G72 (`tests/sqlite_compact_capacity.rs`): a moderate fixture samples the
staged copy, the rollback journal and SQLite's unlinked rewrite temp database
from a second thread and asserts the receipt bounds the new file plus the
journal and the whole sampled footprint; `compact.rs`'s unit test pins the
50 GB arithmetic.

Runs: `cargo test -p layerfs-persistence --offline` (all binaries pass),
`cargo check --workspace --offline` passes. `kernel_guard` in
`layerfs-store-service` fails in this container because `/tmp` is tmpfs and
the guard refuses direct Store access; it is unrelated to this change.

## Gaps (for T10/T12)

- ~~The `(dev, ino)` `StaleStore` fence (§10) is not implemented in the SQLite
  session.~~ Implemented in T8.1: `Session` records `(dev, ino)` at open and
  refuses the next explicit transaction, read-only snapshot or immutable read
  with `StaleStore`; the install fence compares the same pair with
  `SealedFacts` before the rename.
- ~~`Capacity` mapping is implemented but no capacity/failpoint test asserts
  it.~~ T8.1 adds `CompactFailpoint::CapacityAtBuild` (ENOSPC after the staged
  file exists) and unit tests for the `ENOSPC`/`EDQUOT`/`EFBIG` mapping.
- ~~The 9%/10%/30% boundary and the `GroupRows`/`GroupRowsIndexed` and
  acquisition layouts are exercised only on the Monolithic layout.~~ T8.1
  exercises 9% (`none`), 10% (rebuild at the boundary) and 30% (rebuild), both
  grouped layouts on the `none` and rebuild paths, and the acquisition layout
  on open/`none`/rebuild with its working rows cleared. The `postgresql`
  feature is not built here.
- A definite pre-install failure now releases its `gc_intent` while the Store
  is still open (T8.1): the admission row cannot be cleared after the seal,
  and a live intent that outlived its op record refused every later GC call
  `BusyOp`. The op record is kept only when the release itself fails, as the
  next run's recovery breadcrumb (§9).
- ~~Peak temp is the new file plus the visited scratch; journal/WAL sizes
  after the checkpoint are zero and are not added.~~ T-G72: `peakTempBytes`
  bounds the pre-truncate staged copy, the new file (the rewrite's temp
  database), the prune/rewrite rollback journal and the visited scratch. It is
  an estimate and over-reports the sampled peak by a few percent.
- The mark runs before the seal under the same lane (the spec's step 1.5 runs
  it after the seal); the sealed tuple re-read makes the window explicit.
