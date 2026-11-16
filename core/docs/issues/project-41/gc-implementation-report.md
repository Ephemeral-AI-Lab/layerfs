# GC implementation report: soft removal and Store compaction

> Final report for `layerfs-project#41`. The frozen
> [`LayerFS-GC-spec.md`](LayerFS-GC-spec.md) (2.0.0) wins; `§N` and `AC-nn`
> cite it. This page certifies main layer `32ae51ca` (audited by T-G75 on
> 2026-11-15) in §8; it adds no contract and leaves acceptance sign-off to
> #52.

## 1. Scope and what shipped
Four ids-only, admin-profile calls on SQLite and PostgreSQL:
`branch.remove`, `project.delete`, `project.remove_file` and `store.compact`.
The first three change visibility and retention only; `store.compact`
reclaims bytes.
**Schema and removals.** The additive open step adds `branch_retired`,
`layer_stack_retired`, `path_removed`, `project_epoch`, `gc_intent`, `gc_op`,
`gc_dead` and `kept_node`, plus `history_meta.next_history_serial` (never
lowered), `commit.history_serial`, `layer.history_serial` and
`store_policy.trace_salt`; it backfills serials deterministically, creates one
zero epoch per stack and raises the next serial above the maximum. `branch` and
`layer_stack` stay append-only; 1.0 Stores convert offline; there is no
down-migration (§4.1). `branch.remove` writes `branch_retired`;
`project.delete` writes `layer_stack_retired` plus a retry sidecar and returns
`home_entry_removed: false` on a home-removal failure; `project.remove_file`
writes `path_removed` at frontier `next_history_serial - 1`. Visibility is
filtered statements plus an allow-list guard, not views, and the three removal
markers are never rewritten (§4.2-§4.4).

**Admission and mark.** `gc_intent` is one Store-scoped row per operation
(kind, scope, state, owner pid/start/boot, phase, generation, artifact, high
water). SQLite uses two `BEGIN IMMEDIATE` transactions and reports
`MountsActive`, `Busy` or `Unresolved`; PostgreSQL holds `(kind, id)` advisory
keys at Store, project and Branch scope, records the owner in `gc_op`, and ends
a dead idle owner with `pg_terminate_backend(pid, backend_start)`. The epoch
fence re-checks `project_epoch.epoch` before commit (same transaction on
SQLite, a final `FOR SHARE` on PostgreSQL); a changed or missing epoch is
`Conflict` with no Commit (§5). `HistoryProvider::mark_gc` reads the tuple
(`catalog_incarnation`, `next_history_serial`, `next_pack_id`, `next_ordinal`,
`store_totals`, `MAX(stamp)`, history rows), runs the §6.1 history closure and
the §6.2 object closure (`FinalizedObject::admit` plus
`ChainBases::base_of`, one visit level, delta bases), and writes `kept_node`,
the object/pack set, `deadBytesBound`/`deadBytesRefined`, `gc_dead` and the
§6.4 `live_heads_digest` (absent head is 33 zero bytes). `keptWaste` covers
retained-pack waste, delta bases and all `inode_value_block` rows.

**SQLite rebuild.** `SqliteHandles::compact` holds `writer_admission` from
checkpoint through rename, runs `wal_checkpoint(TRUNCATE)` (busy is `Busy`; the
WAL is never unlinked), seals `(size, mtime, db-header change counter)`,
attaches the old file `immutable=1`, builds `.compact-visited`, runs
`VACUUM INTO .compact`, and in one transaction keeps retained rows, deletes the
complement, sets `G + 1`, recomputes `store_totals` and clears
`binding_hint_block`. Verification checks schema, foreign keys, counts, both
digests, allocator and totals; install rechecks sidecars and `-mj*`, applies
`chmod 0600` and `sync_all`, renames and fsyncs; settle removes the artifact
set and op record. Failpoints `AfterSeal`, `AfterBuild`, `AfterWalSwitch`,
`AfterVerify`, `BeforeInstall`, `AfterInstall` and `CapacityAtBuild` cover the
crash states; the artifact set is
`.compact{,-journal,-wal,-shm,-visited,-visited-journal,-mj*}`; a stray name is
`Exists`, a `(dev, ino)` change is `StaleStore`, and a sealed-tuple change at
install is `Conflict`. The path is `none` below 10 percent, `rebuild` at or
above it; an empty Store is `none` with `zeroReason: empty-store`; a started
rebuild completes (§7).

**PostgreSQL rebuild.** `PostgresHandles::compact` runs four phases. M writes
`kept_node`, `gc_op` (phase `mark`, scratch `lfs_gc_<hex>`), `gc_dead` and the
dead totals. A creates the scratch schema with LIKE copies of `pack` and
`object_location`, copies retained rows in idempotent batches of 512 packs,
then adds the prefix index, foreign key and `ANALYZE`. B is one short
transaction: `ACCESS EXCLUSIVE NOWAIT` (`55P03` is `Busy`), re-read the mark
tuple and digest (`Conflict` on change), copy owner and ACL, swap the tables,
recreate `pack_immutable`, settle the totals, set `G + 1`, re-check
`synchronous_commit` and the standby predicate, drop the scratch, `COMMIT`. C
deletes only `gc_dead` members, clears `binding_hint_block` and the acquisition
rows, reduces `store_totals` once against the row sums, clears `gc_dead` and
`gc_op`, and releases the intent. The next call adopts a dead idle owner, is
`Busy` on a live owner, and resumes Phase C at `G + 1`; a lost `COMMIT` reply
is resolved by proving the backend ended, then reading the incarnation.
PostgreSQL reports `backendReusable = 0` and `postgresql-in-place`, and has no
`inode` table (§8, §9).

**Report, status, metrics.** `CompactReport`/`CompactStatus` in
`engine/crates/layerfs-persistence/src/report.rs` and the single serializer in
`control/crates/layerfs-store/src/report.rs` render every §11 field in
snake_case (`dead_bytes_bound`, `kept_waste`, `residue`, `estimate`, and one
`zero_reason` per zero field from
`empty-store|none-path|no-history|no-waste|postgresql-in-place`). `store.status`
adds `op {id, name, phase, started, uncertain}`, `state` (`busy` or
`unresolved`; `uncertain` is true for every remaining record) and the backend
`phases` (SQLite `mark/build/verify/install`; PostgreSQL
`mark/build/swap/cleanup`). `compact_metrics.rs` publishes counters (`runs`,
`operations{outcome}`, `refusals{code}`, `settles{kind}`, `retries{reason}`,
`status{state}`, `slo{objective,result}`), phase durations and last-run §11
gauges, with labels only from closed enums.

**API, MCP, CLI, SDK.** The call table is 76 entries (38 work / 38 admin); the
four calls are admin-only and refused under a folder scope with `out of
scope: <call>`. `project.remove` is retired: old clients get `InvalidInput`,
MCP reports an unknown tool, and the CLI exits 2; `project.init` on a held name
is `NameTaken`. MCP names are `branch_remove`, `project_delete`,
`project_remove_file` and `store_compact`; the CLI has `layerfs branch remove`,
`project delete`, `project remove-file` and `store compact`; JS and Python
expose the same four methods. The call log redacts path-bearing arguments
(`<N bytes>`) and hashes keys with `trace_salt`.

| Expected-base CAS upstream | `CommitRequest` carries `expected_head`, `expected_base` and `expected_root`; a moved Branch is `HeadMoved` and nothing is written, so a stale daemon commit cannot clobber a removal |
| Whiteouts | `remove_file` hides at the frontier; merge, publish and mount materialize the whiteout; a merge that cannot is `Removed` with full path elements |
| `gc_intent` / `gc_op` / `gc_dead` | intent is admission and liveness; `gc_op` is the one compaction row with phase, scratch and owner; `gc_dead` is persisted membership, not a predicate |
| Generation | `catalog_incarnation` is `G`; a rebuild installs `G + 1`; leftovers, fences and resume judge by the recorded generation |
| Reclaim unit | the pack; a rebuild copies retained rows into a fresh Store; no in-place pack delete; `rebuild iff total > 0 AND dead * 10 >= total`; a started rebuild completes |
| Visibility-only `remove_file` | 2.0 hides the path and keeps objects reached only through it; a path-aware walk is a follow-up |
| Thresholds | `deadBytesBound` from the metadata walk, `deadBytesRefined` from the payload sweep; PostgreSQL always reports them equal |

## 3. Test evidence

T-G75 re-ran the suites from main `32ae51ca` on 2026-11-15 in a Sandbox
Workspace (Branch `task-g75`); the counts below are those runs unless the row
names an earlier run. Earlier lanes were recorded on main `32df79da` or the
Branch the row names. The final lanes are:

| Suite | Result | Recorded by |
|---|---|---|
| `layerfs-persistence` (full lane) | 271 passed, 0 failed, 3 ignored | T-G75 (T-G24: 264/0/2) |
| `layerfs-store-service` | 214 passed, 0 failed, 15 ignored | T-G75 (T-G24: same) |
| `layerfs-store` | 59 passed, 0 failed, 0 ignored | T-G75 (T-G24: same) |
| `postgresql_compact` | 20 passed (real `pg_terminate_backend` reply-loss arms and the restricted-role privilege arms) | T-G38/T-G54 |
| `postgresql_status` | 3 passed (persistence) + 1 passed (control) | T10.5/T-G53 |
| `postgresql_compact_fixtures` | 5 passed | T12.3b |
| `sqlite_compact` | 20 passed | T-G52; re-run T-G75 |
| `sqlite_compact_crash` | 23 passed (13 in-process + 9 real-process SIGKILL + the re-executed child placeholder) | T-G28/T-G60, re-run T-G52 and T-G75 |
| `sqlite_compact_fixtures` | 1 passed | T-G52; re-run T-G75 |
| `sqlite_compact_matrix` | 1 runner, 8 executed arms | T-G52; re-run T-G75 |
| `sqlite_compact_capacity` | 1 passed, 1 ignored (the opt-in 50 GB arm is not run by default) | re-run T-G75 |
| PostgreSQL doctor (physical) | 16 passed | T-G55 |

**SQLite lane (T12.3-era baseline; the final counts are the table above).**

| Binary | Tests | Pins |
|---|---|---|
| `sqlite_compact` | 20 | `none`/`rebuild`, 9/10/30 percent, empty Store, capacity abort, layouts, stray name, report fields, leftover op |
| `sqlite_compact_crash` | 23 | in-process failpoints at seal/build/WAL switch/verify/install/settle and the artifact set, plus 9 real-process SIGKILL arms (mark: BeforeSeal/AfterSeal; copy: BeforeVacuum; verify: DuringVerify; rename: DuringRename; build: AfterBuild; install: BeforeInstall/AfterInstall; settle: DuringSettle) and the re-executed child placeholder |
| `sqlite_compact_fixtures` | 1 | golden receipt with every §11 field, digest vector, three outcomes |
| `sqlite_compact_matrix` | 1 runner, 8 arms | checkpoint busy, sidecar at install, status phases, error codes, seal, random shapes, measurement, digest row after collection |
| `compact_metrics`, `admission`, `path_removal`, `branch_removal`, `project_delete` | 4, 5, 3, 3, 5 | metrics, admission races, frontier, markers |
| `epoch_fence`, `gc_backfill`, `kept_objects`, `kept_node`, layouts, upgrade | 7, 4, 5, 7, 7+9, 2+2 | epoch rollback, backfill, retention, layouts |
| `gc_trace`, `gc_calls`, `calls_scope` (API) | 5, 8, 4 | redaction, four-call shapes, scope refusal |
| `compact_report_status`, `project_delete` (control) | 3, 15+6+2 | wire shape, status, sidecar and frames |
| `branch_remove`, `admission`, `epoch_fence`, `project_delete`, `kernel_guard` (store-service) | 3, 1, 1, 1, 7 | service removals and guard |

**PostgreSQL lane (T12.3b baseline; the final counts are the table above).**
PostgreSQL 17.12, `cargo test -p layerfs-persistence
--features postgresql`, `--test-threads=1`: 15 test binaries, 73 tests passed
at T12.3b. The run covers `postgresql_compact` (8 at that layer; 20 on
`32df79da`), `postgresql_compact_fixtures` (5,
three consecutive runs), `postgresql_position_columns` (4),
`postgresql_open_check` (3), `postgresql_diagnostic` (5),
`postgresql_acquisition` (4), `postgresql_acquisition_compaction` (4),
`postgresql_identical_ids` (3), `layerstack_removal` (2),
`postgresql_commit_round_trips` (1) and the SQLite controls. The new fixture
proves the §6.4 digest is byte-identical to the SQLite golden and the rebuild
report and markers hold on PG.

**Harness and CI.** `tools/ci/test_store_ext4.sh` requires
`LAYERFS_TEST_TMPDIR` on ext4, xfs or btrfs (apfs or hfs on macOS), refuses
tmpfs, overlay, FUSE and network mounts before cargo, and points `TMPDIR`,
`CARGO_TARGET_DIR` and `CARGO_HOME` there. `--quick` runs a cross-suite
subset; `--full` runs every Store suite and adds `--features postgresql` when
`LAYERFS_TEST_POSTGRES_URL` is set. `.github/workflows/store-ext4.yml` is the
CI lane: `quick` on push and pull request, `full-postgresql` (a `postgres:17`
service) on schedule and manual dispatch; it is report-only.

## 4. Acceptance matrix status

The matrix has 60 cases (47 both, 6 SQLite-only, 7 PostgreSQL-only). Only
#52 records acceptance; these notes record evidence and tier.

| Runner | Executed | Covered/pass | Partial | Deferred | Gaps |
|---|---|---|---|---|---|
| T12.3a SQLite half (pre-T12.3c) | 5 | 24 | 16 | 7 (PG-only) | 8 |
| T12.3b PostgreSQL half | 0 | 11 | 11 | 8 | 0 |
| T12.3c SQLite gap closure | 8 | 29 | 16 | 7 (PG-only) | 0 |
| T-G28 real-process SIGKILL (AC-01) | 8 | 28 | 17 | 7 (PG-only) | 0 |
| T-G38/T-G54 PG reply loss and privileges (AC-44) | 8 | 29 | 17 | 6 (PG-only) | 0 |
| T-G60 remaining SIGKILL points (AC-01) | 8 | 29 | 17 | 6 (PG-only) | 0 |
| T-G65 real-process `store.compact` kill (AC-01) | 8 | 30 | 16 | 6 (PG-only) | 0 |

T12.3c closed the eight SQLite gaps: AC-06 (post-compaction Save of a
collected digest), AC-11 (non-FF merge whiteouts and publish at the
frontier), AC-12 (removed path in a merge), AC-13 (unmerged Branch marker
inheritance), AC-14 (layer-headed mount whiteout), AC-22 (concurrent
merge/removal property), AC-34 (bind failure and failed intent delete) and
AC-36 (worker lost reply); its report records "SQLite gaps: none". The
PostgreSQL half's partial and deferred rows are AC-02, AC-29, AC-30, AC-31,
AC-32, AC-34, AC-35, AC-36, AC-37, AC-38, AC-41, AC-42 and AC-60. T-G28
added the real-process SIGKILL arms of `sqlite_compact_crash.rs`; AC-01 moved
from `covered` to `partial` because the kill targeted a test child, not the
production process. T-G60 extended the real arms to the mark (`BeforeSeal`/
`AfterSeal`), copy (`BeforeVacuum`), verify and rename points. T-G65 then
started the built `layerfs store compact` command, killed that process at the
mark, build, verify and install points, and asserted `G`/`G + 1`, the
artifact set, the `store.status` projection and the settling run, so AC-01 is
`covered` again (the long-lived `layerfs worker` is still not the killed
process). T-G38's real `pg_terminate_backend` kills and T-G54's
restricted-role fixtures moved AC-44 from `deferred` to `covered`, so the
final dispositions are 8 executed, 30 covered, 16 partial and 6 deferred.
T-G75 re-checked all 60 matrix rows against `32ae51ca` and kept those
dispositions (AC-01 and AC-44 `covered`, AC-57 `partial`).

## 5. Known gaps and limitations

These are the gaps that remain on `32ae51ca`; tasks that closed an earlier gap
are recorded in §7 instead.

- **Clippy clean (fixed by G58/G63/G71).** The gap is closed: G58/G63 fixed
  the former 11 pre-existing warnings and G71 fixed the last two; T-G75
  re-ran
  `cargo clippy -p layerfs-persistence -p layerfs-store-service --all-targets --features postgresql --no-deps -- -D warnings`
  and it exits 0 with zero warnings on the lib and test targets.
- **Branch-delete CLI deployed (fixed 2026-11-15).** The gap is closed: the
  2.0.0 build `4e65ebbae` is live with host CLI `1dc15838…`, bundle `layerfs`
  `c4737445…`, `layerfs-daemon` `b8dd0766…` and
  `layerfs-sandbox-helper` `3d033ec0…`; `layerfs help branch` ends with
  `branch remove <project> <branch>`, and a probe against a missing name
  returns `NoSuchBranch`. Operators use `layerfs branch remove` directly; MCP
  `branch_remove` and the SDK method stay equivalent. The cutover opened the
  Store with the one-way GC schema migration: the sqlite file grew
  1,111,203,840 -> 1,111,310,336 B (live hash `d5d65e3b…`) on first open.
- **PG `physicalShrink` is a keyword proxy.** PostgreSQL computes
  `physical_shrink` from the `pg_total_relation_size` delta of `pack` and
  `object_location` around the swap (an in-place rewrite), not from file-level
  bytes; `backendReusable = 0` with `postgresql-in-place` names that absence.
  SQLite keeps the file-level measure.
- **Real `kill -9` of the production compaction process (T-G28/T-G60/
  T-G65; AC-01 `covered`).** `sqlite_compact_crash.rs::real_process` spawns a
  separate process on the fixture Store and the parent sends `SIGKILL` while
  it is stopped at the failpoint; T-G60 extended those arms to the mark
  (`BeforeSeal`/`AfterSeal`; the `BeforeSeal` kill keeps the writer
  session's WAL sidecars, which the next open replays), copy
  (`BeforeVacuum`, immediately before `VACUUM INTO` because SQLite runs that
  copy as one uninterruptible call), verify (`DuringVerify`) and rename
  (`DuringRename`). Those arms kill a re-executed test child, so T-G65 adds
  the production process:
  `api/crates/layerfs-cli/tests/sqlite_compact_process_kill.rs` starts the
  built `layerfs store compact` command, the engine's gate stops it at the
  named failpoint, and the test `SIGKILL`s it there, then reopens through the
  CLI and asserts `G`/`G + 1`, the artifact set and sidecars, the
  `store.status` phase/`uncertain`/`unresolved`, the settling next run, the
  retained layer and `doctor`. No daemon serves Store calls (#352 F-020), so
  this is the process that performs the native compaction; the long-lived
  `layerfs worker` that serves a machine's Store is still not the killed
  process.
- **Capacity model: 10 GB and 50 GB measured (T-G61, T-G66).**
  [`gc-capacity-model.md`](gc-capacity-model.md) rests on a real 9.987 GB
  Store measurement (`sqlite_compact_capacity.rs#capacity_measure`): `none` at
  8.88% dead (3 runs, median 149.318 s) and `rebuild` at 10.00% dead (3 runs,
  median 246.371 s; mark 149.089 s, build 94.317 s, verify 0.312 s, install
  1.916 s; dead 1.011 GB, shrink 1.017 GB, peak temp 9.144 GB, 1.90x total
  footprint), plus one WAL/FULL rebuild (284.078 s). T-G66 then measured the
  **50 GB** arm (one run per arm, `sqlite_compact_capacity_measure_50gb.json`):
  49.999 GB pack bytes / 50.863 GB physical, `none` at 8.876% dead
  (1,633.585 s) and `rebuild` at 10.000% dead (5,131.835 s; mark 784.251 s,
  build 4,254.371 s, verify 85.557 s, install 7.311 s; dead 5.062 GB, shrink
  5.100 GB, old-formula receipt peak temp 45.784 GB). The 50 GB numbers are
  **not** linear from 10 GB: the build phase is 93.0 s/GB new instead of 10.3,
  so the rebuild is 85.5 min instead of the [E] ~20 min, and the sampled real
  peak is 143.1 GB (~2.9x the Store). T-G72 replaced the old `new + visited`
  receipt with a bound over the staged copy, the new file, the rollback
  journal and the visited scratch: **147.609 GB (2.90x the old file)** for the
  same 50 GB `rebuild`, 3.2% above the sampled peak, tracked as
  [layerfs#695](https://github.com/Ephemeral-AI-Lab/layerfs/issues/695).
  The 30% dead / 50 GB-live arms and the `ACCESS EXCLUSIVE` duration remain
  [E].
- **PostgreSQL `--test-threads`.** T-G14 gave each `postgresql_compact` test
  its own catalog binding (`postgresql-compact-<tag>`), so its 20 tests pass
  with the default test threads (three consecutive runs); the Store-scoped
  advisory key is no longer shared. Other PG suites that still share one
  schema or session state keep `--test-threads=1` until they get the same
  per-test binding.
- **`.git/index` merge rule.** `/.lww` lists `/.git/index` after
  `%include .gitignore`, so merging a Branch forked from an older layer keeps
  the newer index whole instead of conflicting on the binary file. Only this
  generated file is added; no source file is listed.
- **Branch cleanup is soft and bounded (updated 2026-11-15).** `branch.remove`
  is live, but removal is a marker write: the name stays taken (a re-fork is
  `NameTaken` with holder `state: "retired"`, reads are `NoSuchBranch`) until
  `store.compact` collects the rows, and `main` is `ReservedName`. A Branch
  with a live mount is `MountsActive`, and a Branch pinned by a retained Layer
  or `commit.branch_id` also keeps `NameTaken`; a rebuild needs an idle Store.
  The 2026-11-15 cleanup retired 145 Branches (20 empty + 125 superseded) and
  deliberately left 162 (unknown or non-empty); T-G75 re-probed the live CLI
  on `32ae51ca` (a missing name is `NoSuchBranch`) and counted 174 live
  Branches, the 162 that cleanup left plus later gap-fix task Branches. The
  remaining work is data judgment, not a missing capability.
- **`project.remove` retired.** Old clients get `InvalidInput`, MCP reports an
  unknown tool, and the CLI exits 2; there is no compatibility alias.
- `remove_file` reclaims no bytes in 2.0; a path-aware walk is a follow-up.
  PostgreSQL ACL replay is best-effort, and `markBytes` is a #52 metric,
  deferred and not implemented here, not a report field. Compaction metrics are process-local with no SDK or CLI
  renderer, and PostgreSQL compactions have no metrics yet.
- AC-60's real standby arms and the reserved-pack race of AC-42 are deferred;
  AC-28/41/43 have T12.3b evidence and AC-35's advisory-key refcount does not.

## 6. How to run

```
# SQLite compaction lane, on a filesystem that carries SQLite locks
LAYERFS_TEST_TMPDIR=/mnt/ext4/layerfs-store-tests tools/ci/test_store_ext4.sh --quick
LAYERFS_TEST_TMPDIR=/mnt/ext4/layerfs-store-tests tools/ci/test_store_ext4.sh --full
cargo test -p layerfs-persistence --test sqlite_compact
cargo test -p layerfs-persistence --test sqlite_compact_crash
cargo test -p layerfs-persistence --test sqlite_compact_fixtures
cargo test -p layerfs-persistence --test sqlite_compact_matrix
UPDATE_GOLDEN=1 cargo test -p layerfs-persistence --test sqlite_compact_fixtures

# PostgreSQL, one thread
LAYERFS_TEST_POSTGRES_URL="host=127.0.0.1 port=55432 user=postgres dbname=postgres sslmode=disable" \
  cargo test -p layerfs-persistence --features postgresql --test postgresql_compact -- --test-threads=1
```

The ext4/xfs/btrfs requirement is the kernel guard
(`engine/crates/layerfs-store-service/src/kernel_guard.rs`): tmpfs, overlay,
FUSE and network mounts are refused `Unsupported` because they cannot carry
the WAL locks and the directory flock. Inside a Docker Sandbox the run
directory (`$HOME`, `/layerfs/data/machine/native/run/<sandbox>`) is ext4;
`/tmp` is tmpfs there. The PG value is a libpq keyword string;
`postgresql_diagnostic` needs the URI form.

## 7. Commit and branch index

Main layers are publications; heads are the saved Branch tips. `T12.3b` was in
flight at the baseline and is now published on main (`32223b46`, tidy `32927884`).

| Epic | Branch | Head commit | Main layer | Result |
|---|---|---|---|---|
| T1 schema and position | task-t1.2d-merged, task-t1.7-merged3 | 12eee0da5b, 12fb3019bd | 32b602714d, 32860cbe46 | markers, intents, serials, epoch, trace salt, backfill |
| T2 history contract | task-t2.4-ctors-test-merged3 | 12ebeb2dfb | 32c5500017 | typed `NameTaken`/`Removed`, catalog stubs |
| T3 `branch.remove` | task-t3-final5 | 12dabb4673 | 3256c829a6 | `branch_retired`, `ReservedName`, name lifetime |
| T4 `project.delete` | task-t4.6-merged | 12e8a214b4 | 32aca049c3 | `layer_stack_retired`, sidecar, retry |
| T5 `project.remove_file` | task-t5.8 | 12f58625e0 | 3299d92d08 | `path_removed`, frontier, whiteouts, grammar |
| T6 admission and epoch | task-t6.5-merged4 | 12ebc5d982 | 32e895c4eb | `gc_intent`, codes, epoch fence, PG advisory keys |
| T7 mark and closure | task-t7.2 | 12a41bf33f | 3240699d11 | mark tuple, closures, `kept_node`, digest |
| T8 SQLite rebuild | task-t8.1-merged | 122151d179 | 323465c8b3 | seal, visited, verify, rename install, failpoints |
| T9 PostgreSQL rebuild | task-t9e | 1225694fd7 | 32c9181e3d | phases M/A/B/C, `gc_op` resume, standby |
| T10 report, status, metrics | task-t10.1b, task-t10.3-final | 121262d292, 1236755412 | 32515e1b2a, 3271a46892 | §11 report, status op, `compact_metrics` |
| T11 API, MCP, CLI, SDK | task-t11.3-final | 1271019370 | 3218fea5bc | 76 calls, admin profile, `project.remove` retired |
| T12.1 SQLite fixtures | task-t12.1-merged2 | 12db7c4ea4 | 32444eb25c | golden receipt, §11 fields, digest vector |
| T12.2 SQLite crash matrix | task-t12.2-merged2 | 12433023b7 | 32502588ab | failpoints, artifact set, `StaleStore` |
| T12.3a SQLite matrix | merged3-t12.3a | 1277362ac5 | 327d6a42bd | 5 executed / 24 covered / 16 partial / 8 gaps |
| T12.3b PostgreSQL acceptance | task-t12.3b-final | 1210405949 | 32223b46 | PG-half matrix, 15 binaries / 73 passed (published) |
| T13 spec gaps and runbook | task-t13b, task-g17-publish | 12c0efe9dd, 12dcce8e15 | 32ab564277, 3284fb82 | G1-G6 decisions, operator runbook, testing docs |
| T-G13 default-feature build | task-g13-pub2 | 1299885ee9 | 32971532c5 | PG imports gated behind the feature |
| T-G28 real-process SIGKILL | task-g28-merged | 12106c4dd7 | 32a7d0cf07 | 4 SIGKILL arms; AC-01 `partial` |
| T-G52 SQLite crash re-run | task-g52 | 12c002e409 | 32bf908cf5 | 20/18/1/1, matrix 8 arms |
| T10.5 PG `store.status` | merged-t10.5c | 12f80d29df | 328a16a4c9 | persistence `postgresql_status` 3, control 1 |
| T-G38 PG reply loss | task-g38b-merged | 128162224e | 32dc46a25a | real backend kills and lost `COMMIT` |
| T-G53 PG status control | g53-merged | 12876c4f1f | 322a2c59a1 | control `postgresql_status` +1 |
| T-G54 PG privileges | g54-merged | 12f5197908 | 32a2621dbb | AC-44 `covered`; `postgresql_compact` 20 |
| T-G55 doctor physical | g55-integ | 122880b8b6 | 3285f0d588 | 16 checks |
| T-G57 closeout base | g57-merged | 12babcbaea | 32df79da | T-G57 publication |

The docs-only review rounds A-M are on `gc-phase-a` ... `gc-phase-m` and the
`gc-schema`/`gc-contract` branches; they changed no product contract. The
post-closeout gap fixes are published as `task-g58-merged2` (G58/G63 clippy),
`task-g63`, `task-g64`, `g61-integ` (capacity harness), `task-g65-merged` and
`task-g65-merged2` (real-process kill), `task-g67` (physical doctor proxy),
`task-g69` (runbook and cleanup), `g70-merged` (report), `task-g71` (final
clippy), `task-g66b` (50 GB measurement), `task-g72` (peak-temp bound) and
`task-g74` (report generator); T-G75 audits those layers.

## 8. Closeout

- **Final main layer.** `32ae51ca` (root `d0b744cb`), the layer this audit
  certifies; it is the T-G74 publication (commit `128899cd74`). The chain from
  the previous closeout is T-G57 `32df79da` -> T12.8 `3255cfb1fa` -> T-G60
  `32284e2d4c` -> T-G59 `327b250e45` -> T-G58 `329abfa4b4` -> T-G63
  `32e51aa657` -> T-G64 `32feb7df87` -> G61 `32977da52f` -> T-G67
  `327892e625` -> T-G69 `32cfd1e67a` -> T-G65 `3299d19429` -> `32957696ef`
  -> G70 `32bb6d7557` -> G71 `32f89ff7b8` -> T-G66b `32c363ad8a` -> T-G72
  `327aa16ab0` -> T-G74 `32ae51ca`. This closeout publishes as main's next
  layer.
- **Shipped.** Four admin-only, ids-only calls on SQLite and PostgreSQL:
  `branch.remove`, `project.delete`, `project.remove_file` and
  `store.compact`; the §4.1 additive schema and deterministic backfill; the
  `gc_intent`/`gc_op` admission intent, the epoch fence and the §6.1/§6.2
  mark with the §6.4 digest; the SQLite seal/visited/verify/rename rebuild
  with the 10% `none`/`rebuild` threshold and named failpoints; the
  PostgreSQL Phase M/A/B/C rebuild with `gc_op` resume, ACL and the standby
  predicate; the §11 `CompactReport`/`CompactStatus`, `store.status` and
  `compact_metrics` (SQLite phases plus `pg_mark`/`pg_build`/`pg_swap`/
  `pg_cleanup`); the 76-call API/MCP/CLI/SDK table with `project.remove`
  retired; and the ext4 harness plus report-only CI lane.
- **Final test evidence on `32ae51ca` (T-G75 re-run).** 271 passed / 0 failed
  / 3 ignored (`layerfs-persistence`; T-G24 recorded 264/0/2), 214/0/15
  (`layerfs-store-service`) and 59/0/0 (`layerfs-store`), 544 passed / 0
  failed / 18 ignored in all. The same command re-run on the PR's tree
  measured 544 passed / 0 failed / 18 ignored again; the total is 544, not
  554. The five SQLite compaction targets re-ran to
  `sqlite_compact` 20, `sqlite_compact_crash` 23, `sqlite_compact_fixtures`
  1, `sqlite_compact_matrix` 1 runner (8 arms) and `sqlite_compact_capacity`
  1 passed + 1 ignored (the opt-in 50 GB arm). The PostgreSQL lanes were not
  re-run here (this Sandbox has no server): 20 `postgresql_compact` (T-G54),
  3+1 `postgresql_status` (T10.5/T-G53), 5 `postgresql_compact_fixtures`
  (T12.3b) and 16 doctor-physical checks (T-G55) stay as recorded. The full
  table is in §3.
- **Clippy clean (G58/G63/G71).** G58/G63 fixed the former 11 pre-existing
  warnings and G71 fixed the last two; T-G75 re-ran
  `cargo clippy -p layerfs-persistence -p layerfs-store-service --all-targets --features postgresql --no-deps -- -D warnings`
  to exit 0 with zero warnings on the lib and test targets.
- **Matrix after T12.3c, T-G28/T-G60, T-G38/T-G54 and T-G65.** 60 cases: 8
  `executed`, 30 `covered`, 16 `partial`, 6 `deferred` (all
  PostgreSQL-only), 0 SQLite evidence gaps, 0 `pending`; T-G75 re-checked all
  60 rows on `32ae51ca` (AC-01 and AC-44 `covered`, AC-57 `partial`).
- **Remaining partial (16).** AC-02, 04, 11, 15, 19, 20, 21, 23, 30, 32,
  34, 37, 53, 54, 56, 57. Why: the SQLite half is proven but the PostgreSQL
  half is T12.3b's or not named (AC-02/30/32/34), the full form/read-surface
  table is not one test (AC-19/53), the named arm is not inducible or does not
  exist (AC-11's inapplicable-whiteout branch, AC-34's failed intent delete),
  or the E3 scale/three-run arms remain (AC-04, 56, 57).
- **Remaining deferred (6, PostgreSQL-only).** AC-28, 35, 41, 42, 43, 60.
  T12.3b executes the PG halves of AC-28/41/43; AC-35 (advisory-key refcount)
  and AC-42 (reserved-pack race) have no deterministic pause point, while
  AC-60 (a real standby) needs server control the harness does not have.
- **Capacity model.** [`gc-capacity-model.md`](gc-capacity-model.md) records a
  real 9.987 GB measurement ([M]: `none` 149.318 s, `rebuild` 246.371 s median,
  phase sums and the T-G72 peak bound in
  `sqlite_compact_capacity_measure.json`) and the real 49.999 GB measurement
  ([M], one run per arm, phase sums and 5.100 GB shrink in
  `sqlite_compact_capacity_measure_50gb.json`; 85.5 min rebuild, sampled peak
  143.1 GB). T-G72 replaced the old `new + visited` receipt with a documented
  bound over the staged copy, the new file, the rollback journal and the
  visited scratch: **147.609 GB (2.90x the old file)** for the 50 GB
  `rebuild`, 3.2% above the sampled peak, tracked as
  [layerfs#695](https://github.com/Ephemeral-AI-Lab/layerfs/issues/695).
  AC-57 stays `partial` for the section 13 30% dead / 50 GB-live arms and the
  PostgreSQL arms.
- **Branch-removal cutover and cleanup (2026-11-15, main `327892e6`).**
  `branch.remove` is live end to end. Build identity: version `2.0.0`,
  commit `4e65ebbae`; host CLI `1dc15838…`, bundle `layerfs` `c4737445…`,
  `layerfs-daemon` `b8dd0766…`, `layerfs-sandbox-helper` `3d033ec0…`. The
  Store was opened once by the new build (one-way GC schema migration,
  1,111,203,840 -> 1,111,310,336 B, live hash `d5d65e3b…`). Probe:
  `layerfs branch remove … __deploy_probe_missing__` is `NoSuchBranch`;
  `help branch` lists `remove`. Semantics are unchanged (§4.2): the marker
  hides the Branch, `branch remove … main` is `ReservedName`, and the name
  stays taken until `store.compact` rebuilds. Retirement: 307 -> 287 (20
  empty) -> 162 (125 superseded), 145 total, 0 refusals; 162 remained by
  choice (unknown or non-empty, deliberately untouched). T-G75 re-probed the
  live CLI from `32ae51ca`: `help branch` lists `remove`, a missing name is
  `NoSuchBranch`, and the Store holds 174 live Branches (the 162 that cleanup
  left plus later gap-fix task Branches). Backup:
  `/home/dell/lfs-work/backup-20261115-180517` (earlier
  `/home/dell/lfs-work/backup-20261115-180149`); rollback was not needed.
  `layerfs doctor` is `{"ok":true,"findings":[]}`, and no Sandbox/Workspace of
  the cutover or cleanup remains.
- **Validation checklist (T12.8, re-verified T-G75).** Every artifact below
  exists on `32ae51ca`; rows 12-13 record the 2026-11-15 cutover and are
  re-verified on main `32ae51ca`:

  | # | Artifact | Path | Check |
  |---|---|---|---|
  | 1 | implementation report | `core/docs/issues/project-41/gc-implementation-report.md` | present; updated by T-G75 |
  | 2 | acceptance matrix | `core/docs/issues/project-41/gc-acceptance-matrix.md` | present; 60 rows, 8/30/16/6 |
  | 3 | capacity model | `core/docs/issues/project-41/gc-capacity-model.md` | present; 10 GB (3 runs) and 50 GB (1 run) measured [M]; T-G72 peak bound 147.609 GB (2.90x); 30% dead / 50 GB-live and PG arms [E] |
  | 4 | 76-call coverage | `docs/architecture/api/calls/coverage.md` | generated block: 76 calls, 10 families, no front-end problems |
  | 5 | ext4 harness | `tools/ci/test_store_ext4.sh` | present, executable |
  | 6 | CI lane | `.github/workflows/store-ext4.yml` | present; `quick` and `full-postgresql` |
  | 7 | PG phase metric | `engine/crates/layerfs-persistence/src/compact_metrics.rs` | `PostgresMark => "pg_mark"` |
  | 8 | real SIGKILL arms | `engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs` | `sigkill_before_seal`, `sigkill_after_seal`, `sigkill_during_copy`, `sigkill_during_verify`, `sigkill_during_rename`, `sigkill_after_build`, `sigkill_before_install`, `sigkill_after_install`, `sigkill_during_settle` |
  | 9 | privilege check | `engine/crates/layerfs-persistence/src/backend/postgresql/access.rs` | `compaction_missing` checks both `CREATE`s and `pg_read_all_stats` |
  | 10 | PG conninfo helper | `engine/crates/layerfs-persistence/tests/pg_doctor/conninfo.rs` | present |
  | 11 | PG status control | `control/crates/layerfs-store/tests/postgresql_status.rs` | present; 1 test |
  | 12 | `branch.remove` live | host CLI `layerfs branch remove` | live on 2.0.0/`4e65ebbae`, re-probed T-G75; `help branch` lists `remove`; probe on a missing name returns `NoSuchBranch` |
  | 13 | doctor clean after cutover | `layerfs doctor` | `{"ok":true,"findings":[]}`; 145 Branches retired (20 empty + 125 superseded), 162 remained then and 174 are live on `32ae51ca`; removal is soft, so a name returns only after `store compact` |
  | 14 | clippy clean | `cargo clippy -p layerfs-persistence -p layerfs-store-service --all-targets --features postgresql --no-deps -- -D warnings` | T-G75: exit 0, zero warnings |
  | 15 | final suite re-run | `cargo test -p layerfs-persistence -p layerfs-store-service -p layerfs-store --no-fail-fast` | T-G75: 544 passed / 0 failed / 18 ignored; the five SQLite compaction targets keep `sqlite_compact` 20, `sqlite_compact_crash` 23, `sqlite_compact_fixtures` 1, `sqlite_compact_matrix` 1 and `sqlite_compact_capacity` 1 passed + 1 ignored (the opt-in 50 GB arm) |

  `git diff --check` is clean on the closeout Branch.
- **How to run.**
  `LAYERFS_TEST_TMPDIR=/mnt/ext4/layerfs-store-tests tools/ci/test_store_ext4.sh --quick`
  (or `--full`, adding `--features postgresql` when `LAYERFS_TEST_POSTGRES_URL`
  is set) needs ext4/xfs/btrfs. `cargo test -p layerfs-persistence --test
  sqlite_compact_matrix` runs the T12.3c arms; `UPDATE_REPORT=1 cargo test -p
  layerfs-persistence --test sqlite_compact_matrix` regenerates the report.
  The opt-in capacity arm is
  `CAPACITY_MEASURE=1 UPDATE_CAPACITY=1 cargo test -p layerfs-persistence
  --release --test sqlite_compact_capacity -- --ignored --nocapture` with a
  TMPDIR of >=2.2x the target; it writes
  `tests/fixtures/sqlite_compact_capacity_measure.json`.
  `.github/workflows/store-ext4.yml` is the report-only CI lane.
