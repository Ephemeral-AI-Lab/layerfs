# T12.3b: the PostgreSQL half of the GC acceptance matrix

> Acceptance note for the frozen [`LayerFS-GC-spec.md`](LayerFS-GC-spec.md)
> §8/§9/§11/§12, the companion of
> [`gc-t12-sqlite-compaction-fixtures.md`](gc-t12-sqlite-compaction-fixtures.md)
> (SQLite half) and [`gc-t9-postgresql-rebuild.md`](gc-t9-postgresql-rebuild.md)
> (the PostgreSQL rebuild). It adds no contract: the spec and the
> [`gc-acceptance-matrix.md`](gc-acceptance-matrix.md) win; `§N` and `AC-nn`
> cite them. The matrix statuses stay `open` (only #52 records acceptance);
> this note records the PG-half evidence and its tier.
>
> **T-G28/T-G60 follow-up.** `sqlite_compact_crash` now has 23 tests (9
> real-process SIGKILL arms at mark, copy, verify, rename, build, install and
> settle, plus the child placeholder), not the 13 passed recorded below, and
> the matrix statuses are no longer `open`: AC-01 is `partial` and the other
> rows are `executed`/`covered`/`partial`/`deferred`. This page keeps the
> T12.3b baseline counts.

## Environment

- PostgreSQL **17.12** (`postgresql-17_17.12-1.pgdg12+1`, Debian 12), server on
  `127.0.0.1:55432`, one schema per test dropped at test end, `postgres`
  superuser, `sslmode=disable`.
- `LAYERFS_TEST_POSTGRES_URL` was set for every run below. The suites that pass
  the URL straight to the driver take the keyword form
  (`host=127.0.0.1 port=55432 user=postgres dbname=postgres sslmode=disable`).
  The suites that append startup options (`postgresql_diagnostic`) need the URI
  form (`postgresql://postgres@127.0.0.1:55432/postgres?sslmode=disable`);
  with the keyword form those three tests fail at connect, which is a harness
  URL-shape issue, not a Store result.
- Toolchain: Rust 1.85.1, `cargo test -p layerfs-persistence --features
  postgresql`, `-j 8`, `CARGO_TARGET_DIR=/tmp/target-merged`.
- Branch `task-t12.3b` forked main layer `32c9181e`; it was merged with main
  before publishing, and every result below was re-run on the merged tree
  (main `3284fb82` at the merge), so the commands and counts are the merged
  branch's, not the fork base's.

## The fixture

`engine/crates/layerfs-persistence/tests/postgresql_compact_fixtures.rs` is
the T12.1 fixture rebuilt on PostgreSQL through the public APIs only. It is a
new test target (5 tests); it changes no production code.

| Test | What it pins | Matrix rows |
|---|---|---|
| `the_live_heads_digest_vector_is_the_sqlite_literal` | The §6.4 vector on PG is **byte-identical to the T12.1 SQLite golden**: the Commit id, `absentHeadDigest` (33 zero bytes) and `retiredBranchDigest` are the literals embedded in `fixtures/sqlite_compact_golden.json`. | AC-40, AC-56 (PG half) |
| `the_rebuild_report_and_markers_hold_on_postgresql` | The full T12.1 rebuild fixture on PG: `live_heads_digest` equals the SQLite golden **rebuild** literal; `deadBytesBound == deadBytesRefined`; `backendReusable = 0` with `postgresql-in-place`; `physicalShrink`, `peakTempBytes`, `keptWaste.{deadInRetainedPacksBytes, inodeBlocksBytes, deltaBases}`; `logicalDiscarded == 160`; empty residue; `G -> G+1`; depth-2 chain kept with `pack_id` and pack length unchanged; retained pack keeps its unreached group; markers/digest rows follow the retained set; full doctor; epoch copied, serial never lowered. | AC-02, AC-05, AC-17, AC-18, AC-25, AC-40, AC-41, AC-43, AC-52 |
| `a_save_after_the_swap_survives_the_resumed_cleanup` | `PostgresAfterSwap` aborts the round at `G+1`; a Save lands after the swap; the resumed Phase C deletes only the persisted dead set, the late write (object, pack, totals) survives the high-water bound, and the Store is usable at `G+1`. | AC-28, AC-43, AC-49 (Phase C arm) |
| `every_postgresql_failpoint_leaves_a_resumable_leftover` | All four named §8 failpoints (`mark`, `build`, `swap`, `cleanup`) leave the documented state (generation, dead pack, scratch schema, op phase, owner pid) and the next call adopts/resumes it; the heads digest is unchanged and the doctor is clean. | AC-01 (in-process arm), AC-03 (PG failpoints), AC-28, AC-43 |
| `a_marker_write_bumps_the_epoch_and_the_fence_refuses_a_racing_write` | Commit/Layer leave `project_epoch` alone; a Branch removal and a `remove_path` marker bump it once each; a repeated marker is `UpToDate` and does not bump; compact copies the epoch and never lowers `next_history_serial`; a removal-first stale Commit and a missing epoch row are both `Conflict("project epoch")` and write nothing. | AC-21, AC-24, AC-39 |

The digest equality is not a value comparison on "both backends ran" alone: the
fixture ids, roots and Commit records are content-addressed, so the PostgreSQL
mark hashing the SQLite literal proves the §6.4 vector agrees byte for byte.
Run three times, the five tests passed 3/3 with the same literals (the matrix
lists AC-56 at E3; the deterministic two-backend literal is E2, three
identical runs are recorded here for the E3 arm).

## Matrix: requested PostgreSQL-half rows

Counts: **19 rows — 5 passed, 6 partial, 8 deferred** (AC-33 is a SQLite-only
row and is not part of the PG half).

| Case | Result | Tier | Evidence / leftover |
|---|---|---|---|
| AC-02 | partial | E1 | The additive step on PG (position columns + serial backfill) passes `postgresql_position_columns` 4/4; the report/removal parity is covered below. A converted **1.0** Store is SQLite-only: PostgreSQL is a 2.0 format with no 1.0 reader, so that arm is not runnable on this backend. |
| AC-28 | **pass** | E1 | `a_save_after_the_swap_survives_the_resumed_cleanup`: late Save survives the resumed Phase C; op/generation asserted. Deferred sub-arm: the marker/base-row batch boundary failpoint (no injected point inside `cleanup`; Postgres only has the four round failpoints). |
| AC-29 | partial | E1 | `postgresql_compact` 8/8: a live idle owner is terminated by `(pid, backend_start)` and adopted; the caller's own uncertain round is adopted without self-termination. Mount-work admission belongs to the Store/Workspace layer and is not in this harness (deferred). |
| AC-30 | deferred | E0 | The `NameTaken` detail/error matrix is backend-neutral history work; no PG-specific fixture was added here. |
| AC-31 | deferred | E0 | Call-log/trace-salt redaction is the control-layer path, not the persistence harness. |
| AC-32 | partial | E1 | Same T9 owner-adoption evidence as AC-29 (uncertain op settled by the owning call). "Another home's intent visible" is control-layer, deferred. |
| AC-34 | deferred | E0 | A failed intent bind/delete needs the admission registry around a mount/init call, not this harness. |
| AC-35 | deferred | E0 | The project/Store advisory-key refcount exercise is the mount layer (`pg_advisory` keys), outside the persistence fixture. |
| AC-36 | partial | E1 | Lost swap reply (`postgresql_compact`), lost cleanup reply and lost mark/build replies (`every_postgresql_failpoint_leaves_a_resumable_leftover`) all resolve as documented. An unreachable worker/remote Store is deferred (needs the worker transport). |
| AC-37 | deferred | E0 | Trace redaction is the control-layer call log. |
| AC-38 | partial | E1 | The scratch name `lfs_gc_<op hex>` is recorded before CREATE and removed at the swap/settle: the four-failpoint test asserts `scratch_exists` per phase. Deferred: a remote machine removing the scratch schema and a stray schema refusing with `Exists`. |
| AC-39 | **pass** | E1 | `postgresql_position_columns` 4/4 (one `project_epoch` row per stack, serials backfilled once, never lowered) plus the new fixture: the rebuild copies the epoch, `gc_op`/`gc_dead` are cleared on settle and not copied. |
| AC-40 | **pass** | E2 | `the_live_heads_digest_vector_is_the_sqlite_literal` + `the_rebuild_report_and_markers_hold_on_postgresql` vs the T12.1 SQLite golden; the retired head moves the digest on both. |
| AC-41 | partial | E1 | The depth-2 chain is kept only as delta bases, both reported in identity order; every retained pack keeps its `pack_id` and its `length` after the swap. Deferred: `markBytes` as a wire metric — it is a #52 metric and is not a `store.compact` result field (§11 matrix note), so there is nothing to assert in the receipt. |
| AC-42 | deferred | E0 | A Save reserved before the mark and published during Phase A must make the swap `Conflict`. The PG round has no pause point between build and swap (`stop()` returns immediately), so a publication cannot be interleaved deterministically; a wall-clock race would not be a fixed fixture. |
| AC-43 | **pass** | E2 | After the swap and the resumed cleanup: the four `store_totals` equal their row sums, `pack`/`object_location` keep owner and the `pack_immutable` trigger bound to the schema function (and the trigger still refuses an update), `binding_hint_block` is cleared, there is no `inode` table on PG, the Store opens at `G+1` and passes the full doctor. |
| AC-44 | deferred | E0 | Missing `CREATE`/`TEMP`/`pg_read_all_stats` needs roles the suite may create and a server it may reconfigure; a truly lost `COMMIT` reply needs a killed backend. The `PostgresAfterSwap` failpoint is the deterministic substitute and proves resume, not reply loss. |
| AC-56 | **pass** | E2 (3/3 runs) | The PG digests are the T12.1 SQLite literals: absent head (`6864c4…`), retired branch (`dafcdd…`), and the rebuild vector (`d3a3ea…`); the frozen Commit id `121c28…` is identical on both backends. |
| AC-60 | deferred | E0 (predicate E1) | A real synchronous standby needs a second cluster and `synchronous_standby_names`/`synchronous_commit` changes on a dedicated server; a shared suite URL cannot own those GUCs. `compact.rs` unit tests pin the predicate (no names / below `remote_write` → no wait; names → a streaming sync/quorum standby with a matching `sync_state`). |

## Matrix: additional PostgreSQL rows recorded by this note

Counts: **11 rows — 6 passed, 5 partial, 0 deferred**.

| Case | Result | Tier | Evidence / leftover |
|---|---|---|---|
| AC-01 | partial | E1 | The four in-process PG aborts leave a wholly old (`mark`, `build`) or wholly new (`swap`, `cleanup`) Store and the next call settles. A `kill -9` of a daemon at the same points is not injectable in this harness. |
| AC-03 | partial | E1 | Named PG failpoints `mark`, `build`, `swap`, `cleanup` are all exercised with state, generation, op phase, scratch and digest asserted. Lost `COMMIT`, power loss and lease takeover are deferred (AC-44 note). |
| AC-05 | **pass** | E2 | Full doctor on the compacted PG Store in both rebuild tests; `postgresql_diagnostic` 5/5 (transport, cancellation, denied signature). |
| AC-17 | **pass** | E1 | Depth-2 PREFIX chain copied; `pack_id` preserved for both bases and the target. |
| AC-18 | **pass** | E1 | The collected `file_digest` root is gone, the retained one survives. |
| AC-21 | partial | E1 | Removal-first refusal (`Missing`/`Conflict`, no Commit) and the missing-epoch-row `Conflict` are deterministic on PG; the true concurrent one-shot-write arm (a trigger injecting an epoch change mid-transaction, as the SQLite `epoch_fence` tests do) is not implemented for PG here. |
| AC-24 | **pass** | E1 | Marker write bumps `project_epoch` once; `UpToDate` does not; compact copies it; serial never falls. |
| AC-25 | **pass** | E1 | A retained `branch_retired` row survives a rebuild; a collected Branch takes its marker with it. |
| AC-49 | partial | E1 | Phase C survives the high-water bound (late Save); the mount-wins-the-race arm needs the mount/lease layer, deferred. |
| AC-51 | partial | E1 | The op row records the §8 phase (`swap` asserted after the failpoint, `mark`/`build` in the matrix test). Projecting it through `store.status` needs a Store-home fixture; the T10 serializer test is SQLite-side today. |
| AC-52 | **pass** | E2 | The PG report has every §11 component: `deadBytesBound == deadBytesRefined`, `backendReusable = 0` + `postgresql-in-place`, `physicalShrink > 0`, `peakTempBytes > 0`, `keptWaste` three parts, empty residue, `logicalDiscarded` = history delta, `estimate = true`, zero reasons named. |

## Commands and results

Server: PostgreSQL 17.12 on `127.0.0.1:55432`; `LAYERFS_TEST_POSTGRES_URL` set as
above; `-j 8`; `--test-threads=1`.

| Command (`cargo test -p layerfs-persistence --features postgresql --test …`) | Result |
|---|---|
| `postgresql_compact_fixtures` (new) | **5 passed**; 3 consecutive runs, 5 passed each |
| `postgresql_compact` | 8 passed |
| `postgresql_position_columns` | 4 passed |
| `postgresql_open_check` | 3 passed |
| `postgresql_commit_round_trips` | On the fork base: 1 failed, `13 != 11`; current main already fixes the expectation (see below), the merged tree passes 1/1 |
| `postgresql_diagnostic` (URI URL) | 5 passed |
| `postgresql_acquisition_compaction` | 4 passed |
| `postgresql_acquisition` | 4 passed |
| `postgresql_identical_ids` | 3 passed |
| `layerstack_removal` | 2 passed (one is the PostgreSQL project-removal arm) |
| `sqlite_compact_fixtures` | 1 passed (the golden still matches) |
| `sqlite_compact_crash` | 13 passed |
| `sqlite_compact` | 20 passed |

## Baseline note: the stale round-trip count is already fixed on main

`postgresql_commit_round_trips.rs` expected 11 round trips for `lane + head +
commit_branch` on the fork base; the path takes 13. The two extra statements are
T6.5's writer fence (`gc_epoch_read` at the start of the write transaction and
`gc_epoch_share` … `FOR SHARE` as its last statement, `src/history/epoch.rs`).
Current main already carries the updated expectation (13), so this branch does
not change that file; it only records the finding.

## Gaps (not contract changes)

- **AC-42** (a Save reserved before the mark racing the swap) and **AC-60**
  (real standby, missing standby before/inside the swap, dying after the check)
  need a hook the deterministic round does not have or a dedicated server whose
  GUCs the suite owns.
- **AC-44** privileges and a killed backend's lost `COMMIT` need server roles
  and processes the suite cannot own; the swap failpoint is the recorded
  substitute.
- **AC-28**'s marker/base-row batch boundary needs a failpoint inside Phase C;
  only the four round failpoints exist.
- The mount/lease admission rows (**AC-29/32/35/49**) are Store-layer, not
  compaction-layer; only their compaction-op liveness arm is here.
- **AC-30/31/34/37** (error detail, trace, intent bind, redaction) are
  control-layer rows and are untouched.
- `markBytes` (AC-41) is not a `store.compact` field, so the PG half cannot
  assert it from the receipt.
