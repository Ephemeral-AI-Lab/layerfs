# GC acceptance matrix: #52 evidence plan for the §12 cases

> Draft matrix for the frozen `LayerFS-GC-spec.md` §12 (the spec wins; this
> page adds no contract). Main layer `32d78561a`. AC-01..AC-57 are the §12
> bullets one-to-one, in spec order; AC-58..AC-60 pin sub-steps of §7/§8 that
> §12 folds into broader cases. Each row's expected result restates spec text
> only; a mismatch is a spec bug, not a matrix change.
>
> Status vocabulary: `open` (no §12-conforming evidence recorded yet),
> `pass`, `fail`, `waived`. §13 says nothing has been measured: every E3 row
> starts `open`. Tiers are this page's evidence scale, not contract.

## Evidence tiers (E0-E3)

| Tier | Meaning |
|---|---|
| E0 | Spec reasoning only; no executable evidence. Does not accept a case. |
| E1 | One deterministic test on the named backend, fixed fixture, exit asserted. |
| E2 | Deterministic CI test on both backends (or the named backend's full named-phase/failpoint matrix), fixed fixture, state, refusal and digest asserted. |
| E3 | Measured or statistical: at least 3 runs with median and range (hardware, caches and versions declared), a property test over random histories, or a 50 GB scale arm. |

## Matrix

| Case ID | Contract (spec §) | Backend | Fixture / command | Expected result | Tier | Status |
|---|---|---|---|---|---|---|
| AC-01 | §12 Crash injection; §9 | both | Store with one wholly dead pack; `kill -9` the daemon at mark, build, verify, before and after install; next `store.compact` | Store is wholly old or wholly new; the leftover is judged by §9; generation and digest asserted | E2 | open |
| AC-02 | §12 Backend matrix; §4.1 | both | Run every removal, admission, report and name case on SQLite and PostgreSQL; open a converted 1.0 Store on both | Each case's §12 result holds on both backends; the additive step adds the §4.1 tables on both | E2 | open |
| AC-03 | §12 Crash matrix; §9 | both | Named failpoints: SQLite mark, build, verify, install, WAL switch, artifact removal, op settle; PG mark, build, swap, cleanup, scratch drop; plus lost COMMIT, power loss (fsync/rename) and unresolved op/lease takeover | §9 state, generation, digest and closure hold after resume; only a live owner blocks; an unreachable machine reports unknown, never clean | E2 | open |
| AC-04 | §12 No live object lost; §6.1-§6.2 | both | Property test over random fork, commit, merge, publish, `branch.remove`, `project.delete` and `remove_file` histories; compare closure sets before and after | No live object reaches a collected row; the retained closure is unchanged | E3 | open |
| AC-05 | §12 Independent walk; §9 | both | After compaction, walk the new Store with `diagnostic/scan.rs` and run the full doctor on the fixtures (#155 D-23) | Walk and doctor pass on the compacted Store; no dangling reference | E2 | open |
| AC-06 | §12 Regression; §6.4 | both | After compaction, Save bytes whose `file_digest` row named a collected root | The Save succeeds; no stale digest row blocks it | E2 | open |
| AC-07 | §12 Fences; §10 | both | A connection that bypassed admission; a second compaction; a remote PG daemon holding a mount; a one-shot SDK transaction open at swap time | Bypass gets `StaleStore`; second compaction gets `Busy`; the mount blocks compaction and the matching removal, naming itself; the open transaction makes the swap abandon | E2 | open |
| AC-08 | §12 Admission races; §5.1 | both | Start a mount and a removal of the same Branch together | Never both succeed; the loser gets `MountsActive` or `Busy` | E2 | open |
| AC-09 | §12 Names; §4.4 | both | `NameTaken` after removal (Branch and project); create collision (Branch and project; project create was `Exists`); free name after a rebuild collects the row; `ReservedName` for `main` | Exact codes per §4.4; only a rebuild frees an unreferenced name | E2 | open |
| AC-10 | §12 remove_file; §4.3 | both | `Removed` for Commits at or below the frontier, before and after compaction; the next Commit of a mounted older head records the deletion; a file created later at the path is visible | Codes and visibility per §4.3; 2.0 frees no bytes for the removed path | E2 | open |
| AC-11 | §12 remove_file and derived nodes; §4.3 | both | Non-FF `branch.merge` of two heads at or below the frontier; a post-frontier re-creation; a FF merge control; `branch.publish` of an at-or-below Commit; a merge that cannot apply the whiteout | Path stays hidden, whiteout materialized; post-frontier file visible; FF merge creates no tree; published Layer hidden, its mount whiteouts, next Commit records the deletion; inapplicable whiteout refused `Removed` | E2 | open |
| AC-12 | §12 Removed path in a merge; §4.3 | both | Non-FF merge of two at-or-below heads; merge of an at-or-below head with a post-frontier re-creation | No conflict for the removed path, it is omitted; the re-created file is kept and only an ordinary edit conflict on it is reported | E2 | open |
| AC-13 | §12 Unmerged Branches; §4.3 | both | `remove_file` then a Commit on an unmerged Branch created before the frontier; another Commit after the frontier | The earlier Commit's path is hidden, the later one does not inherit it | E2 | open |
| AC-14 | §12 Layer-headed mount; §4.3 | both | Branch whose head is empty and whose base Layer is at or below the frontier; mount it | The path is whiteouted at mount | E2 | open |
| AC-15 | §12 Visibility only; §6.2 | both | Object reachable only through the removed path; marker hiding reads/listings; mount, merge and publish whiteouts | No bytes freed; the object is retained; reads/listings hide the path; derived trees whiteout it | E2 | open |
| AC-16 | §12 Published Branch; §4.4 | both | Branch published by a retained Layer; Branch named by a retained `commit.branch_id`; `branch.remove` then rebuild | Name stays taken; only an unreferenced Branch's name is freed by a rebuild (not by a `none` pass) | E2 | open |
| AC-17 | §12 Late base; §7 step 4 | both | Delta base in an already-passed pack; a depth-2 chain | Worklist copies both; `pack_id` preserved | E2 | open |
| AC-18 | §12 Digest roots; §6.4 | both | Uncommitted saved file's `file_digest` row; another retained root reaching its content | Row collected unless the retained root reaches the content | E2 | open |
| AC-19 | §12 Path grammar; §4.3 | both | `/a` and `a`; empty, `/`, `//a`, `a/`, `a//b`, `./a`, `a/./b`, dot and dot-dot components, backslash, NUL, >4096 bytes, >256 components; directory subtree; child of a removed non-directory; symlink leaf and intermediate component; hardlink | Accepted paths hide correctly; bad forms are `InvalidInput`; directory hides its subtree; child of removed non-directory `NotFound`; intermediate symlink `InvalidInput`, leaf hidden; hardlink survives | E2 | open |
| AC-20 | §12 Repeated removal; §4.3 | both | Second `remove_file` after a re-creation; repeat with no newer project node; a commit in another project; never-existing path | Re-creation hidden; repeat `UpToDate`; other project neither rewrites nor advances the marker; never-existing path accepted, hides nothing | E2 | open |
| AC-21 | §12 Marker epoch; §5 | both | One-shot write racing `branch.remove` or `project.delete`; merge racing `remove_file` | `Conflict`, no Commit left; retry applies the whiteout | E2 | open |
| AC-22 | §12 Concurrent merge and removal (property); §4.3 | both | Property test: a merge and a `remove_file` started together | The path is never left visible in a derived tree; one side wins, the other sees the marker or gets `Conflict` and retries with the whiteout | E3 | open |
| AC-23 | §12 Marker cleanup; §6.4 | both | Rebuild of a Store with a deleted project and a live project with markers; a `none` pass | No `path_removed` row for the deleted project; live markers kept; `none` writes nothing and retains them | E2 | open |
| AC-24 | §12 Position state; §4.1 | both | Converted Store; `store.compact`; a marker write | Serials backfilled; `next_history_serial` never falls; `project_epoch` copied and bumped only on a marker write | E2 | open |
| AC-25 | §12 Marker survival; §4.4 | both | Retained `branch_retired`, `layer_stack_retired` and `path_removed`; a collected base row | Retained markers survive a rebuild; a collected row takes its marker with it | E2 | open |
| AC-26 | §12 project.delete retry; §4 | both | Marker commits and home removal fails; same-home retry; own-Init identity save failure | Hidden, entry present, `home_entry_removed: false`; retry finishes and returns the stored `deleted_ms`, no second marker; the identity save precedes the marker, a save failure removes nothing | E2 | open |
| AC-27 | §12 Home listing; §4.2 | both | Second home's stale entry; `project.list` and `project.get`; Store calls | Stale entry absent from `project.list`, `project.get` answers `NoSuchProject`; Store calls agree | E2 | open |
| AC-28 | §12 PG cleanup; §8 Phase C | PG | Commit/save during Phase C; crash mid-cleanup; failpoint at the marker/base-row batch boundary | The late write survives the high-water bound; cleanup resumes from `gc_op`; marker and base row deleted in one batch, no resurrection or orphan | E2 | open |
| AC-29 | §12 Admission; §5.1 | both | Mount from another home or machine; `machine_stopped` evidence; a second mount of one project; an uncertain dead op | The mount is visible in the Store-side intent; `machine_stopped` ignored; the second mount proceeds; `Unresolved` observed and settled by the owning call | E2 | open |
| AC-30 | §12 NameTaken; §4.4 | both | `NameTaken` detail for live, retired and deleted holders; forced two-reference fixture (Layer FK and `commit.branch_id`); one-reference and no-reference controls; error matrix | Detail carries holder kind, name and state; two references keep the name, the controls do not; matrix covers `MountsActive`, `Busy`, `NoSuchBranch`, `NoSuchCommit`, `NoSuchProject`, `NameTaken`, `ReservedName`, `Removed`, `InvalidInput` | E2 | open |
| AC-31 | §12 Trace salt; §4.3 | both | Call log with path arguments; `Removed` refusal; rebuild | Log stores `<N bytes>`, trace key is the keyed hash; `Removed` echoes the path; the salt is copied by the rebuild | E2 | open |
| AC-32 | §12 Admission (liveness); §5.1 | both | Live-but-unreachable Sandbox; `machine_stopped`; `gc_intent` from another home; non-owning call | Live-unreachable is `Busy`; `machine_stopped` ignored; the other home's intent visible; `Unresolved` observed by the non-owner and settled by the owner | E2 | open |
| AC-33 | §12 Checkpoint/install; §7 steps 1, 7 | SQLite | Live reader during `wal_checkpoint(TRUNCATE)`; a sidecar or `-mj*` present at install | `busy>0` makes the checkpoint `Busy` with no artifact and no op; a sidecar at install is `Conflict` with no writes | E2 | open |
| AC-34 | §12 Bind failure; §5.1 | both | Committed intent whose bind fails; failed intent delete | Intent deleted in the error path; the failed delete is reported; no `MountsActive` remains | E2 | open |
| AC-35 | §12 PG refcount; §5.2 | PG | Two mounts, one unmount; failed all-or-none acquire | Store and project advisory keys stay after one unmount; the failed acquire unlocks the taken subset | E2 | open |
| AC-36 | §12 Worker/lost reply; §4, §9 | both | Unreachable worker op; lost reply; retry | Op is `unknown`, never clean; the sidecar is kept; the retry resolves retired, live or absent | E2 | open |
| AC-37 | §12 Trace; §4.3 | both | Six path names; refusal details; upgraded Store | The six path names are redacted; refusal details are not traced; the salt is backfilled | E2 | open |
| AC-38 | §12 Artifacts (cross-backend); §9 | both | SQLite artifact set `.compact{,-journal,-wal,-shm,-visited,-visited-journal,-mj*}`; PG `lfs_gc_<hex>` schema; a stray schema | SQLite leaves and removes the full set; PG records the name before CREATE, a remote machine can remove it, a stray schema is `Exists` | E2 | open |
| AC-39 | §12 Position state (2.0 backfill); §4.1 | both | Every upgraded Store (2.0 included); `gc_op` settle; serial allocation | One `project_epoch` row per stack; `gc_op` cleared on settle and not copied; `next_history_serial` never lowered | E2 | open |
| AC-40 | §12 Digest agreement; §6.4 | both | Same fixture on SQLite and PostgreSQL; retired head | Same `live_heads_digest`; a retired head changes it | E2 | open |
| AC-41 | §12 PostgreSQL mark; §8 Phase M | PG | Mark transfer; table copy; a base object held only by a dead pack | Only `markBytes` crosses the wire; the copy moves no pack bytes; the base is kept | E2 | open |
| AC-42 | §12 Reserved pack; §8 Phase B | PG | Save reserves a pack/ordinal before the mark and publishes during Phase A (or C); cleanup failpoint | `store_totals` moves; the swap `Conflict`s; the pack stays in the old Store; the failpoint keeps it; no dangling location | E2 | open |
| AC-43 | §12 PG cleanup (resume); §8 Phase C | PG | After the swap at `G+1`; crash mid-cleanup; totals, ACL, trigger, hints | Store usable at `G+1`; cleanup resumes; totals/ACL/trigger/hints asserted; no `inode` on PG | E2 | open |
| AC-44 | §12 PG privileges and reply; §8 | PG | Missing `CREATE`, `TEMP` or `pg_read_all_stats`; lost `COMMIT` reply | `Unsupported`; the lost reply is resolved by proving the backend ended, then reading the incarnation | E2 | open |
| AC-45 | §12 SQLite layouts; §7 step 2 | SQLite | Schema 36 (`Monolithic`), 37 (`GroupRows`), 38 (`GroupRowsIndexed`) and their 42/43/44 acquisition variants rebuild | `Monolithic` copies `pack` body and no `pack_unit`; `GroupRows` copies both; `GroupRowsIndexed` copies `pack_unit` and the schema creates its index; `sqlite_sequence` kept when present; `init_*`/acquisition tables empty | E1 | open |
| AC-46 | §12 Install fence; §10 | SQLite | Connection that bypassed the writer lane opening a transaction after install; in-flight transaction; sealed-state or sidecar change at install | `StaleStore`; the in-flight transaction is excluded by the lane (documented limit); a sealed-state or sidecar change is `Conflict` with no writes | E2 | open |
| AC-47 | §12 Artifacts (SQLite exact); §9 | SQLite | Crash mid-compact; next compact; `none` pass; stray name with no op | Crash leaves the full `.compact` set; the next compact removes all of it; `none` leaves no `.compact`; a stray name is `Exists` | E2 | open |
| AC-48 | §12 Threshold and failures; §3 | both | 9%, 10% and 30% wholly dead; empty Store (0/0); started rebuild whose refined share falls below 10%; `Capacity`; verify failure | 9% is `none`, 10% and 30% are `rebuild`; empty Store is `none` with `zeroReason: empty-store`; the started rebuild completes; `Capacity` and verify failure abort, delete the artifact set and leave the old Store unchanged apart from step 1's checkpoint | E2 | open |
| AC-49 | §12 Mount/compact race; §5, §8 | both | Mount during compact; mount wins the race; Commit during the rebuild | `Busy` with the compact op; compaction returns `MountsActive` when the mount wins; the Commit is not lost (PG Phase A write makes the swap `Conflict`; Phase C survives the high-water bound) | E2 | open |
| AC-50 | §12 Profile and scope; §10 | both | All four calls under a folder scope; MCP admin profile | Refused `out of scope`; allowed only in the admin profile | E2 | open |
| AC-51 | §12 Progress; §11 | both | `store.compact` running; `store.status` | `store.status` shows the op phase: SQLite `mark`, `build`, `verify`, `install`; PG `mark`, `build`, `swap`, `cleanup` | E2 | open |
| AC-52 | §12 Report; §11 | both | `none` and `rebuild` paths; every §11 field | `kind: Compacted`, snake_case keys, every §11 field with its unit and sampling point (`freelist_count * page_size`, old size before rename, history `logical` delta, peak temp, `physicalShrink`, `keptWaste`, `residue`, `estimate`, capped `cap`, `zeroReason`); `markBytes` is a #52 metric, not a result field; bound at least refined; PG `backendReusable` = 0 and PG/none `deadBytesRefined` equals `deadBytesBound` | E2 | open |
| AC-53 | §12 Read surface; §4.3 | both | `file.read`/`stat`/`list`/`glob`/`diff`/`revision`/`export`/cp-from before and after compaction; `edit`/`grep`/`blame`/`rm`/`mv`/`mkdir`/`cp`/`apply`; `branch.diff`/`commit.diff` | §4.3 shapes; visible/hidden/absent diff table; `removed_count` 0 and >0; one whiteout case per one-shot tree creator | E2 | open |
| AC-54 | §12 Restore and listing; §4.2 | both | `backup.restore` before and after a removal; second home's stale entry | Restore succeeds; the stale entry stays out; the §4.2 guard test and allow-list hold | E2 | open |
| AC-55 | §12 Error matrix; §10 | both | Exact code and message per case, including `StaleStore`, `Removed`, `InvalidInput` for `project.remove`, `Exists`, `Unsupported`, `Capacity` | Exact code and message; snake_case details | E2 | open |
| AC-56 | §12 Digest golden vector; §6.4 | both | Fixed-id fixture (out-of-order ids, a retired Branch and stack, differing timestamps); absent head | One literal vector on both backends; absent head is 33 zero bytes, not 32; widths fixed at 17/33 | E3 | open |
| AC-57 | §12 Speed and space; §13 | both | At least 3 runs per arm, median and range, declared caches, generated fixtures; §13 arms: `none` at 9%, `rebuild` at 10% and 30%, 50 GB total/35 GB live, 50 GB live, each removal type, removal-then-compact, `remove_file` 0-freed | Downtime per GB live and per GB freed, bytes written, peak disk, PG `ACCESS EXCLUSIVE` duration; absolute bytes when nothing is freed; build phase vs plain `VACUUM INTO`; mark reported separately; a 50% time/5% space option rejected | E3 | open |
| AC-58 | §7 step 1.5 (refined decision) | SQLite | After the seal: `ATTACH` the old file `immutable=1`, create `<store>.compact-visited`, run the complete §6.1+§6.2 closure, choose `none`/`rebuild` from the refined live set | `none` deletes the scratch and stops with no `.compact` and no residue; `rebuild` keeps the scratch, reuses `visited` in step 4; step 2 creates the new file only on `rebuild` | E2 | open |
| AC-59 | §7 step 1 (seal) | SQLite | `PRAGMA wal_checkpoint(TRUNCATE)`; checked-close the last session; sidecar check under the writer guard; `seal_holding()` | `busy == 0` required; WAL never unlinked; sealed `(size, mtime, db-header change counter)` recorded before release; the guard passes into the seal | E2 | open |
| AC-60 | §8 Phase B step 10 (standby) | PG | Effective `synchronous_commit` at least `remote_write` with `synchronous_standby_names` set; standby missing before Phase M; standby missing inside the swap; standby dies after the check | Wait only when required; `FIRST`/`ANY` standbys must be `state=streaming` with matching `sync_state`; a missing standby abandons before COMMIT with no writes (`Busy`, standby list); a standby dying after the check blocks COMMIT | E2 | open |

## Coverage map (required area -> cases)

- Removals: branch AC-09, AC-16; project AC-26, AC-27; file AC-10..AC-21, AC-23..AC-25, AC-54.
- Admission: AC-07, AC-08, AC-29, AC-32, AC-34, AC-35, AC-49; epoch AC-21, AC-24, AC-39.
- Mark and closure: AC-04, AC-05, AC-15, AC-17, AC-18, AC-25, AC-40, AC-41.
- SQLite compaction (step 1.5, seal, sidecars, install, artifacts): AC-33, AC-46, AC-47, AC-58, AC-59; crash coverage AC-01, AC-03, AC-48.
- PG compaction (mark, swap, cleanup, scratch, standby, lost reply): AC-28, AC-41, AC-42, AC-43, AC-44, AC-49, AC-60; scratch in AC-38.
- Report and `store.status`: AC-51, AC-52.
- API and error shapes: AC-09, AC-26, AC-30, AC-34, AC-36, AC-50, AC-55.
- Read surface: AC-53, AC-54.
- Crash failpoints: AC-01, AC-03, AC-28, AC-36, AC-46, AC-47.
- 9/10/30% threshold: AC-48; measured arms in AC-57.
- Golden digest: AC-40, AC-56.
- 50 GB capacity: AC-57 (§13 arms).
- Names: AC-09, AC-16, AC-30.
- Trace: AC-31, AC-37.

## Notes

- `markBytes` is measured in AC-41 and AC-57, never reported as a `store.compact` field (§11).
- A case is accepted at E2 only when the named backend's failpoints are exercised; E3 rows additionally need three runs or the property test, per §12 Speed and space.
- Evidence recording (run id, fixture hash, spec layer, artifact paths) goes beside the status once #52 runs each case.
