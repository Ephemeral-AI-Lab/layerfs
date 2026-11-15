# GC rollout, rollback and dry run (T1.6)

> Plan for the frozen `LayerFS-GC-spec.md` (2.0.0; the spec wins, `§N` cites it). This page adds no contract and no option: the four calls take only a resource
> identifier, with no confirm, force or dry run (§1), and are admin-profile only, refused under a folder scope (§10).

## 1. Rollout order

| Step | Content | Contract |
|---|---|---|
| T1.x | Additive schema: marker/intent tables, position state, `gc_op`/`gc_dead`, `trace_salt` | §4.1 |
| T3/T4/T5 | Removals `branch.remove`, `project.delete`, `project.remove_file`: marker rows only, no bytes freed | §4 |
| T6 | Admission: `gc_intent`, `MountsActive`/`Busy`/`Unresolved`, epoch fence | §5 |
| T7 | Mark: reachability closure at `G`, `none`/`rebuild` from wholly dead packs | §3, §6 |
| T8 | SQLite rebuild: seal, `visited` scratch, verify, rename install | §7 |
| T9 | PostgreSQL rebuild: Phase M mark, Phase A build, Phase B swap, Phase C cleanup | §8 |
| T10/T11 | Report (§11 fields) and API surface | §9–§11 |
| T12 | Acceptance (§12 cases) and measurement (§13) | §12 |

Order is forced: a rebuild copies T1.x's tables and cleans T3–T5's marker rows. T8/T9 are independent per backend; no step enables `store.compact` by itself.

## 2. Expand and contract

- **Expand (T1.x, T3–T5).** The schema step only adds tables and columns and one idempotent backfill under the current schema identity; no column changes type or meaning,
  none is dropped, and `history_meta.next_history_serial` is never lowered (§4.1). Removals add marker rows and bump `project_epoch.epoch` (§4, §5). Both are additive: a
  Store that never removed anything and never compacts is a plain 2.0 Store.
- **Old readers.** SQLite takes the step as one more `add_*`; PostgreSQL's open check requires the exact table set and recorded definition, so the step rewrites that
  definition for older Stores (§4.1). An older PG binary that predates the step refuses the upgraded Store, and the spec defines no down-migration: **downgrade is not
  supported**. Roll forward, or restore a pre-upgrade backup.
- **Contract.** Two API contractions are version cuts, not toggles: `project.remove` leaves the call table (an old client gets `InvalidInput`, never a silent hard delete)
  and `project.init` returns `NameTaken` instead of `Exists` (§4, §10). Both land with T1.x and are asserted in the error matrix.
- **Flags.** None are added. `store.compact` is opt-in by the caller invoking it: no automatic collection, no online sweep (§1), and the spec defines no global
  enable/disable switch. Do not invent one; the only "enable" is the release cut plus §7, and an owner may simply not call it.

## 3. Dry run

`store.compact` has no dry-run option (§1). Two non-mutating paths exist and neither is a new call:

1. **Copy rehearsal.** `backup.create`, `backup.restore` into a scratch Store/home, run `store.compact` against the *scratch* name, compare. §9 names `backup.create`
   before compaction as the recovery path for a mark bug; a copy is the closest honest rehearsal.
2. **`none` outcome.** Below 10% wholly dead packs the path is `none`: mark only, nothing written, generation unchanged, `deadBytesRefined == deadBytesBound` (§3, §11).
   It cannot be requested, but on a Store expected to be `none` it is a real non-mutating run.

Compare, before → after (both backends; §11):

| Observation | `none` | `rebuild` |
|---|---|---|
| `store.status` | no op before/after; phase visible during | same; phases `mark`/`build`/`verify`/`install` (SQLite) or `mark`/`build`/`swap`/`cleanup` (PG) |
| `generation {before, after}` | equal, `G` | `G → G + 1` |
| `deadBytesBound` ≥ `deadBytesRefined` | equal | refined ≤ bound, from the payload sweep |
| `logicalDiscarded` | 0 with a `zeroReason` | collected history `logical` bytes |
| `keptWaste` | measured, unchanged by the rehearsal | `{deadInRetainedPacksBytes, deltaBases, inodeBlocksBytes}` |
| `store_totals` | unchanged | `logical` unchanged; `canonical`/`stored`/`metadata` = old − `dead_*` (§7 step 4, §8 step 9) |
| `residue` | empty | empty after settle; nonzero is explained |

Also compare `physicalShrink` and `backendReusable` (§11). A rehearsal that disagrees with the live Store is a fixture difference, not a different path: both obey the
same §3 rule.

## 4. Rollback

- **Schema is additive.** The T1.x step is the only schema change and re-running it is a no-op; there is no down-migration. Do not un-create the tables by hand: without
  `gc_intent`/`gc_op` the Store cannot admit or recover an operation, and PostgreSQL's open check (exact table set, recorded definition) refuses it (§4.1).
- **Markers are not undone by dropping tables.** Dropping `branch_retired`, `layer_stack_retired` or `path_removed` would resurrect names and paths whose history a
  rebuild may already have collected (a deleted project's rows are dropped by the rebuild, §7/§8), and §1 promises no undo. The only supported re-creation is a Commit
  **after** the frontier that writes the path again: a post-frontier re-creation is visible, and the marker hides only inherited content (§4.3). A Branch or project name
  is re-created after a rebuild frees it; the history is gone (§4.2, §12 "Names").
- **A failed compaction keeps `G`.** A definite failure before the rename aborts and deletes the artifact set
  `<store>.compact{,-journal,-wal,-shm,-visited,-visited-journal,-mj*}` (SQLite) or the recorded `lfs_gc_<hex>` scratch schema (PostgreSQL); the old Store is unchanged
  apart from step 1's checkpoint (§7 step 8, §8, §9). Keep `G` and re-run later.
- **Installed-but-uncertain.** A failure of the rename/fsync itself, or a lost `COMMIT` reply, is not pre-install: keep the new Store at `G + 1`, settle the op `unknown`
  and report it; never delete the artifact as if nothing happened (§7 step 8, §8). On PostgreSQL resolve a lost reply by proving the backend ended (`pid`,
  `backend_start`, `usename`), then read the incarnation (§8).
- **Artifacts belong to the next `store.compact`.** Only a compaction that names them from its op removes them; a staged name no record names is `Exists` and is not
  deleted (§9). Leave crash leftovers alone by hand.

## 5. Verification gates

| Phase | Tests | `store.status` / report | Doctor |
|---|---|---|---|
| T1.x | converted 1.0/2.0 Stores open; backfill (`history_serial`, `project_epoch`, `trace_salt`, never-lowered serial); `tools/store_blocks.py` | unchanged, no op row | full, before and after |
| T3–T5 | marker transaction, §4.2 visibility guard, admission races, whiteout on merge/mount/publish | no op left; `Removed`/`NoSuch*` shapes | full |
| T6 | intent lifetime, `machine_stopped`, `Unresolved` ownership, `holders` | `busy`/`unresolved` only while owned | full |
| T7 | closure vs reference walk, `live_heads_digest`, "no live object lost" (§12) | phase `mark`; path chosen | full on fixtures |
| T8 | §7 failpoints (mark/build/verify/install/WAL switch/artifact removal/op settle), exact artifact set, `StaleStore` | both paths' §11 fields; `physicalShrink`, `backendReusable` | `validate` before rename; full after |
| T9 | §8 failpoints (mark/build/swap/cleanup/scratch drop), `ACCESS EXCLUSIVE` duration, standby, reply loss, privileges | PG zeros (`backendReusable = 0`); resume from `gc_op` | full after cleanup |
| T10/T11 | §11 snake_case keys, `zeroReason` vocabulary, capped `cap`, scope refusals (§10) | phase and `op` projection | full |
| T12 | whole §12 matrix; independent `diagnostic/scan.rs` walk; 3 runs per arm (§13) | every field with unit and sampling point | full on fixtures |

`quick_check` and the full doctor stay separate commands, not part of install (§7 step 6). A gate is green only on both backends, or on the rows §12 assigns.

## 6. Failure handling

- **Crash.** The Store is wholly old or wholly new (§9). The next `store.compact` judges the recorded `G`: at `G` the artifact is removed (not installed); at `G + 1` the
  outcome is recorded and Phase C resumes. A PG crash in Phase M/A leaves only the scratch schema, which the op names; Phase B is atomic.
- **`Busy`.** A live mount, long-lived client or operation holds the scope. The call does not wait, retry or replay; prepare inactivity and call once later (§1, §5).
- **`Unresolved`.** A dead, uncertain owner's op. A non-owning call observes it; only the owning call (the next `store.compact`) adopts or clears it (§5, §10). An
  unreachable machine reports `unknown`, never clean (§9).
- **Lost reply.** The outcome stays uncertain: read `store.status`, the op and the generation before deciding; never replay blind, and never delete a sidecar or an
  artifact the op names.
- **Standby.** PostgreSQL waits only when `synchronous_commit` is at least `remote_write` and `synchronous_standby_names` is non-empty; the check runs before Phase M and
  again inside the swap. A missing standby abandons with `Busy` (standby list); a standby dying after the check blocks the `COMMIT` unbounded (§8). SQLite has no such
  gate.
- **`Capacity` / disk full.** Aborts, deletes the artifact, leaves the old Store unchanged apart from the checkpoint; `Capacity` is the real refusal path, not a codec
  bound (§7).
- **`StaleStore`.** A connection that bypassed admission is caught at its next transaction; an in-flight transaction at install is a documented limit (§7, §10).

## 7. Go/no-go: enabling `store.compact` on a shared Store

1. `backup.create` succeeds and a `backup.restore` drill of this Store was done (§9).
2. `store.status` is clean: no `holders`, `state` not `busy`/`unresolved`, no live `gc_intent` row, no existing `.compact*` set or `lfs_gc_<hex>` schema.
3. All mounts, daemons and long-lived clients of the Store are gone, and no Init/merge row names it (§5); one owner knows nobody may start one during the window.
4. The backend's T8 or T9 gate and the §12 rows touching it are green; the full doctor is clean on a fixture of the same schema layout (§7 step 6, §12).
5. Disk headroom covers peak temp: old Store + new Store + journal/WAL + `visited` (SQLite); old tables + new heap/TOAST + indexes + live-bytes WAL and `pg_wal` headroom
   (PG) (§7 cost, §8).
6. PostgreSQL only: `CREATE` on the database, `TEMP`, `USAGE`/`CREATE` on the Store schema, table ownership, `pg_read_all_stats`, else `Unsupported`; a direct or
   session-pooled connection (transaction pooling is `Unsupported`); standby replication at the intended level (§5.2, §8).
7. The expected path is known: wholly dead bytes below 10% means `none` and no shrink — decide in advance whether the run is still wanted (§3).
8. The call is made under the MCP admin profile, not a folder scope (§10).
9. Rollback is understood: keep `G`, let only the next `store.compact` remove named artifacts, restore from backup if a mark bug is suspected (§9).
10. One attempt, no replay: a refusal from §6 is inspected before any second call (§1, §9).
