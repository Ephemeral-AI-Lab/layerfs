# GC operator runbook: `store.compact` and the three removals

> Operational page for the frozen `LayerFS-GC-spec.md` (the spec wins). Baseline
> `a6d84131b`; calls land in T3–T11. Admin-profile only, refused under a folder
> scope (§10). No confirm/force/dry-run; a lost reply stays uncertain, nothing retries itself (§1, §9).

## 1. Triage: stuck compact or a phase in `store.status`

`layerfs store status <store>` shows `holders`, `state` (`busy`/`unresolved`) and
`op = {id, name, phase, started, uncertain}` (§11). Phases — SQLite `mark`, `build`,
`verify`, `install`; PG `mark`, `build`, `swap`, `cleanup`.

- **SQLite.** The op/phase is the *home op* (`store/<name>/record.json`, field `op`;
  a remote home is read via `step_on`); the Store DB has the admission row:
  `sqlite3 -readonly <store>.sqlite "SELECT kind, scope_kind, state, uncertain, phase, generation, artifact FROM gc_intent WHERE state <> 'done';"`
  Artifacts sit beside the Store file: `<store>.compact{,-journal,-wal,-shm,-visited,-visited-journal,-mj*}`.
  A direct reader may see `busy`/`locked` in the exclusive window; don't loop. Never unlink `-wal`/`-shm` (§7 step 1).
- **PostgreSQL.** `store.status` reads the Store-side `gc_op` (§11). On a direct or session-pooled `psql`:

  ```sql
  SELECT generation, phase, scratch_schema, artifact, pid, backend_start, incarnation FROM gc_op;
  SELECT (SELECT count(*) FROM gc_dead), (SELECT catalog_incarnation FROM history_meta);
  SELECT kind, scope_kind, scope_id, state, uncertain, phase, artifact FROM gc_intent WHERE state <> 'done';
  SELECT nspname FROM pg_namespace WHERE nspname LIKE 'lfs_gc_%';
  ```

  `gc_op` is the one compaction row; `gc_dead` is its persisted dead set (membership,
  not a predicate); `gc_intent` is admission. `lfs_gc_<hex>` is recorded in `gc_op`
  **before** `CREATE SCHEMA` (§8). Never INSERT/UPDATE/DELETE `gc_intent`, `gc_dead`,
  marker tables or `project_epoch` by hand.

## 2. `Unresolved`, live vs dead owner, lost reply

`Unresolved` = a provably dead owner left an **uncertain** op; a non-owning call observes
it, the owning/next call settles it (§5.1, §10). A live owner is `Busy` — wait, don't kill it.

1. Liveness comes from the recorded owner, not the clock: SQLite home op
   `owner.pid`/`pid_start`/`pid_boot`; PG `gc_op.pid` + `backend_start` against `pg_stat_activity`.
2. Dead owner, uncertain op: run `layerfs store compact <store>` again; it adopts the dead
   op with the artifact/high-water in the record (§9). `sandbox.close`/`machine.remove`/
   `store.remove` clear their own ops; `logs.reset` cannot (§10). A removed machine record alone is no proof (§5.1).
3. **PG lost `COMMIT` reply** — resolve on a *second* connection **before** the exclusive key
   (§8). The swap recorded `pid`, `backend_start`, `usename` and the incarnation in
   `gc_op` before the swap (`usename` is a `gc_op` column, §4.1). Prove the backend
   ended, then read the generation:

   ```sql
   SELECT pg_terminate_backend(a.pid, 30000)
     FROM pg_stat_activity a
    WHERE a.pid = <gc_op.pid> AND a.backend_start = '<gc_op.backend_start>'
      AND a.usename = '<gc_op.usename>' AND a.datname = current_database()
      AND a.pid <> pg_backend_pid();
   ```

   Check the boolean, poll until the `(pid, backend_start)` pair is gone, then read
   `history_meta.catalog_incarnation`: `G` = not committed, `G+1` = installed. Needs PG 14+
   and `pg_signal_backend`/membership, else `Unsupported` (§8). Never terminate a live owner to clear `Busy`.

## 3. Safe cleanup

Default: **let the next `store.compact` judge it** (§9). The op names `G` and the artifact.
Store at `G` = not installed → next compact removes the named artifact set; `G+1` =
installed → it records the outcome. A dead owner's op is adopted; only a live owner blocks.

- Delete by hand only when **no record names it**: no compaction `op`/`gc_op` row (PG: no
  `gc_op.scratch_schema`/`artifact`), no live owner, no exclusive key. A stray at the staged
  name with no record is refused `Exists` and not deleted by the code (§9); after the checks it is safe, otherwise leave it.
- Never remove `<store>.compact-visited*` during a compact: it is the live visited set and part of `peakTempBytes` (§6.3, §11).
- **Never** delete a live Store's `gc_intent`/`gc_dead` by hand: admission and the Phase C
  dead set are cleared only by the owning call on settle (§5.1, §8 Phase C).
- PG: drop `lfs_gc_<hex>` only when `gc_op` names no schema and no session holds the exclusive key; a named schema is the next compact's to remove.
- A definite pre-rename SQLite failure deletes its own artifact set; a rename/fsync failure is
  **installed-but-uncertain** — keep the new Store, settle `unknown`, report it, never delete it (§7 step 8).

## 4. Rollback and reversal (limits)

- **Schema rollback is additive.** Marker/position/admission tables and the
  `history_serial`/`trace_salt` columns are added at open; a downgrade leaves them (§4.1,
  decision 6). No down migration; dropping them is not a rollback. The PG open check records
  an exact table set (§4.1), so a pre-GC PG binary may refuse the added tables — forward-fix or restore instead.
- **Markers cannot be undone by dropping marker tables.** Dropping `branch_retired`/
  `layer_stack_retired`/`path_removed` resurrects retired rows without home entries, breaks
  the "name never reused while rows exist" `BranchId::named` rule (§4.4), and cannot restore rows a rebuild already collected.
- **Supported reversal is re-creation after the frontier, never deleting markers** (§4.3):
  a file is written/created at serial > `frontier_serial`; a Branch is created/forked once a
  rebuild frees the name (new id/history); a project is `project.init`-ed once its retired stack row is collected (new empty project).
- **Limits.** Old ids/history/bytes never return; a Branch pinned by a retained Layer or
  `commit.branch_id` keeps `NameTaken` (§4.4); `remove_file` reversal does not unhide
  at-or-below versions; `rebuild` install is irreversible (§9). Content recovery is
  `backup.create`/`backup.restore` from before compaction (SQLite; the server's backup for PG);
  no undo copy is kept (§9, #155 D-24). The 2026-11-15 branch cutover keeps its
  pre-change home and Store in `/home/dell/lfs-work/backup-20261115-180517`
  (earlier `backup-20261115-180149`); restore that backup rather than editing
  markers (see §7).
- `project.remove` is gone; an old client gets `InvalidInput` (§4, §10).

## 5. Known gaps at this baseline

- **PG existing-Store additive (T1.3b):** new PG Stores get the GC tables from `schema.sql`;
  an existing 2.0 PG Store needs the additive step at open. Until then open/compact refuses; don't hand-create tables (§4.1).
- **Old-Store column ALTER (T1.2b):** new SQLite Stores get `commit.history_serial`,
  `layer.history_serial`, `store_policy.trace_salt`; older Stores lack them until T1.2b's
  ALTER + definition gate. GC refuses; don't hand-ALTER (§4.1).
- **`remove_file` is visibility-only in 2.0:** it hides reads/listings/mounts/merges/publishes
  and frees no bytes; objects reachable only via the path are retained (§6.2, decisions 16/35).
- **No path-level physical reclaim:** path-aware walk and per-entry page pruning are follow-ups; `keptWaste` has no page component (§6.2).
- 1.0 Stores convert offline (§4.1); no online migration.

## 6. First 10 minutes

1. `layerfs store status <store>`: read `state`, `holders`, `op.phase`, `op.uncertain`. Don't kill a live owner; `Busy` is expected.
2. `Unresolved`: prove the owner dead (home pid/start, or PG `pid`+`backend_start`), then
   re-run `layerfs store compact <store>` to adopt. For a lost PG swap reply, use §2 on a second connection.
3. Before manual steps, back up (`backup.create` on SQLite; server backup on PG) and record generation `G`, op id, artifact name.
4. Compare artifact/schema with the op; leave anything a record names to the next compact (`G` removes, `G+1` records).
5. Never hand-edit `gc_intent`, `gc_dead`, markers or `project_epoch`; never unlink `-wal`/`-shm`; never drop a schema `gc_op` names.
6. To re-use a name after a removal, re-create after the frontier (or after a rebuild frees it); don't delete markers. Escalate if old content must be restored.

## 7. Branch retirement (`branch.remove`, live since 2026-11-15)

`branch.remove` is live on the 2.0.0 build `4e65ebbae` (host CLI `1dc15838…`,
bundle `layerfs` `c4737445…`, daemon `b8dd0766…`, helper `3d033ec0…`);
`layerfs help branch` lists it, and a probe on a missing name returns
`NoSuchBranch` (exit 125). The call is a marker write, not a delete: it hides
the Branch, keeps its rows and its name until a later `store.compact` rebuild
collects the marker, and never touches `main` (`ReservedName`).

**Preflight.**

1. `layerfs sandbox list <project>` is empty, or every listed Sandbox belongs
   to a task you have agreed to exclude; a mounted Branch is `MountsActive`.
2. `layerfs doctor` is `{"ok":true,"findings":[]}`; `machine status docker` is
   `running`.
3. Take a backup; the cutover used
   `/home/dell/lfs-work/backup-20261115-180517` (bundle, host, daemon, helper,
   `bundle.json`, `worker.json` and the Store volume; the earlier one is
   `backup-20261115-180149`). Keep it until the retired names are no longer
   needed.
4. Page the inventory: `layerfs branch list <project> --page-size 100
   --page-pos <next_page_pos>`; the last page has `has_more: false`. Record the
   names and the count before and after.
5. Build the retire list: an empty Branch (`head: null`), or an explicitly
   reviewed superseded Branch (typically a `-merged`/`-integ`/`-final`/
   `-publish` variant) whose content is already on `main` or no longer needed;
   `layerfs branch diff <project> main <branch> --paths-only` shows the paths.
   Never include `main`, a running task's Branch, or a Branch with a live
   mount.

**Procedure.**

- For each name: `layerfs branch remove <project> <branch>`. One call per
  Branch; there is no batch, force or dry-run form. `Busy` is another
  operation holding the Store scope (wait, call once more); `MountsActive` is
  a live mount; `NoSuchBranch` means already gone; `ReservedName` is `main`.
  Record every refusal.
- Verify: the retired name reads `NoSuchBranch`, a re-fork is `NameTaken` with
  the holder's `state: "retired"`, `branch list` no longer shows it, and the
  `main` head is unchanged.
- **Soft name retention.** The name is not reusable until a rebuild collects
  the `branch_retired` rows; plan around that and never hand-delete markers
  (§4). `store.compact` below is the only supported reclaim.

**Reclaim (optional, later).** `layerfs store compact <name>` only when the
Store is idle (no mounts, daemon or other op). A rebuild copies retained rows
into a fresh Store and installs `G + 1`; only then are retired names and their
rows collected. A `none` outcome (<10 % wholly dead) keeps everything, and the
retired names stay taken. Follow the compaction procedure in §6 and the
rollback limits in §4.

**Rollback.** Restore the whole home and Store from the pre-cutover backup
above (or `backup.restore` from a `backup.create` taken during the window);
markers cannot be undone by dropping tables (§4). The 2026-11-15 cutover was
not rolled back: no call failed, and `main` and `Cargo.toml` were byte-identical
before and after.

**2026-11-15 result.** 145 Branches retired (20 empty at cutover + 125
superseded), 0 refusals; 162 remained by choice (unknown or non-empty,
deliberately untouched). `main` stayed `32977da5…` (root `56dbe645…`),
`layerfs doctor` stayed clean, and no Sandbox/Workspace of the cleanup was
left running. The new build's one-way GC schema migration grew the sqlite
Store 1,111,203,840 -> 1,111,310,336 B on first open.
