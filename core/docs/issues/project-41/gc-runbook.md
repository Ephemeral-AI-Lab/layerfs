# GC operator runbook: `store.compact` and the three removals

> Operational page for the frozen `LayerFS-GC-spec.md` (the spec wins). Baseline
> `a6d84131b`; calls land in T3–T11. Admin-profile only, refused under a folder
> scope (§10). No confirm/force/dry-run; a lost reply stays uncertain, nothing retries itself (§1, §9).

## 1. Triage: stuck compact or a phase in `store.status`

`layerfs store status <store>` shows `holders`, `state` (`busy`/`unresolved`) and
`op = {id, name, phase, started, uncertain}` (§11). Phases — SQLite `mark`, `build`,
`verify`, `install`, `cleanup` (the settle after install); PG `mark`, `build`,
`swap`, `cleanup`. The SQLite `cleanup` phase is a recorded deviation (D4): the
settle writes it while it removes the artifact set. PostgreSQL writes `cleanup` in
its own small transaction before the first Phase C batch, so a crash during the
batched deletes reads `cleanup` and the next call resumes the persisted set.

- **SQLite.** The op/phase is the *home op* (`store/<name>/record.json`, field `op`;
  a remote home is read via `step_on`); the Store DB has the admission row:
  `sqlite3 -readonly <store>.sqlite "SELECT kind, scope_kind, state, uncertain, phase, generation, artifact FROM gc_intent WHERE state <> 'done';"`
  Artifacts sit beside the Store file: `<store>.compact{,-journal,-wal,-shm,-visited,-visited-journal,-mj*}`.
  A direct reader may see `busy`/`locked` in the exclusive window; don't loop. Never unlink `-wal`/`-shm` (§7 step 1).
- **PostgreSQL.** `store.status` reads the Store-side `gc_op` (§11). On a direct or session-pooled `psql`:

  ```sql
  SELECT generation, phase, scratch_schema, artifact, pid, backend_start, usename, incarnation FROM gc_op;
  SELECT (SELECT count(*) FROM gc_dead), (SELECT catalog_incarnation FROM history_meta);
  SELECT kind, scope_kind, scope_id, state, uncertain, phase, artifact FROM gc_intent WHERE state <> 'done';
  SELECT nspname FROM pg_namespace WHERE nspname LIKE 'lfs_gc_%';
  ```

  `gc_op` is the one compaction row; `gc_dead` is its persisted dead set (membership,
  not a predicate); `gc_intent` is admission. `lfs_gc_<hex>` is recorded in `gc_op`
  **before** `CREATE SCHEMA` (§8). Never INSERT/UPDATE/DELETE `gc_intent`, `gc_dead`,
  marker tables or `project_epoch` by hand.

**Free space.** A SQLite `rebuild` holds the old Store plus about one new
Store (the retained bytes) and the visited scratch. Check the free space on the
Store's filesystem and its temp directory before every rebuild: see §8.

## 2. `Unresolved`, live vs dead owner, lost reply

`Unresolved` = a provably dead owner left an **uncertain** op; a non-owning call observes
it, the owning/next call settles it (§5.1, §10). A live owner is `Busy` — wait, don't kill it.

1. Liveness comes from the recorded owner, not the clock: SQLite home op
   `owner.pid`/`pid_start`/`pid_boot`; PG `gc_op.pid` + `backend_start` + `usename` against `pg_stat_activity`.
2. Dead owner, uncertain op: run `layerfs store compact <store>` again; it adopts the dead
   op with the artifact/high-water in the record (§9). `sandbox.close`/`machine.remove`/
   `store.remove` clear their own ops; `logs.reset` cannot (§10). A removed machine record alone is no proof (§5.1).
3. **PG lost `COMMIT` reply** — the next `store.compact` resolves it automatically on a
   *second* connection **before** it takes the exclusive key (§8). The mark recorded
   `pid`, `backend_start`, `usename` and the incarnation in `gc_op` (`usename` is a
   `gc_op` column, §4.1). A recorded owner that no longer holds the Store's exclusive
   key is a lost reply: the call terminates exactly that backend; a recorded owner that
   still holds the key is a live run and stays `Busy`, never terminated. The operator
   can run the same resolution by hand:

   ```sql
   SELECT pg_terminate_backend(a.pid, 30000)
     FROM pg_stat_activity a
    WHERE a.pid = <gc_op.pid> AND a.backend_start = '<gc_op.backend_start>'
      AND a.usename = '<gc_op.usename>' AND a.datname = current_database()
      AND a.pid <> pg_backend_pid();
   ```

   Check the boolean, poll until the `(pid, backend_start, usename)` triple is gone, then read
   `history_meta.catalog_incarnation`: `G` = not committed, `G+1` = installed. Needs PG 14+
   and `pg_signal_backend`/membership, else `Unsupported` (§8); the automatic recovery
   reports the same refusal. Never terminate a live key holder to clear `Busy`.

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

- **Pre-GC Store upgrade (current, both backends):** a Store written before the GC
  tables, the `kept_node` closure or the position columns gains them at its first
  writable open — SQLite runs `add_gc`/`add_kept_node`/`add_position_state` (plus the
  definition gate), and since #776 only for a Store whose recorded schema source is
  not the current one: in 2.0 that is a schema version 1 Store converted by
  `backup restore` or `tools/store_blocks.py migrate`; a current Store runs none.
  PostgreSQL runs no additive step since #776 (9f84e467b6): an older version is
  refused by name. A schema whose recorded source/definition matches no known
  earlier step is still refused and is never migrated by hand (§4.1).
  *Historical:* the earlier notes here (T1.3b "PG existing-Store additive", T1.2b
  "old-Store column ALTER") described the baseline `a6d84131b`, before those open
  steps landed.
- **`remove_file` is visibility-only in 2.0:** it hides reads/listings/mounts/merges/publishes
  and frees no bytes; objects reachable only via the path are retained (§6.2, decisions 16/35).
- **No path-level physical reclaim:** path-aware walk and per-entry page pruning are follow-ups; `keptWaste` has no page component (§6.2).
- 1.0 Stores convert offline (§4.1); no online migration.
- **No Store format change for place authority.** The lean 2.0 GC has no
  Store-side authority, tombstone or audit tables and no new `gc_intent` column:
  the proof is the home `MachineRecord.incarnation`, the `SandboxRecord.generation`,
  the machine-level `machine/<machine>/sandbox-retired/*.json` tombstone, the
  tokens a live mount row keeps in `gc_intent.evidence` (`place:v1;...`) and the
  retained `gc_intent` `done` rows. A Store of the base format opens unchanged;
  there is nothing to convert. The owner's release, an uncertain mark and a
  retried registration never delete or revive a `done` row.
- **Four SQLite deviations from the frozen text remain** (D1, D3, D4, D5),
  plus the settled D2 build decision; all are recorded in
  [`gc-42-decisions.md`](gc-42-decisions.md) and the frozen spec text is
  unchanged: the fresh bulk load runs with `foreign_keys=OFF`; the rebuild is
  the fresh-copy build (no whole-Store copy, no `VACUUM`, peak old + new +
  visited, §8 — it follows S5's fresh-Store sentence but deviates from §7 step
  4's payload sweep and rollback journal, D2); `none` writes only a transient
  visited scratch and rolls its mark transaction back; the settle records a
  `cleanup` phase; `store.compact` of a Store pushed to another machine runs
  in that machine's worker.

## 6. First 10 minutes

0. Check free space first (§8): the old Store plus about one new Store and
   its visited scratch — roughly the Store's size again (with margin). A full
   disk is `Capacity`.
1. `layerfs store status <store>`: read `state`, `holders`, `op.phase`, `op.uncertain`. Don't kill a live owner; `Busy` is expected.
2. `Unresolved`: prove the owner dead (home pid/start, or PG `pid`+`backend_start`), then
   re-run `layerfs store compact <store>` to adopt. For a lost PG swap reply, use §2 on a second connection.
3. Before manual steps, back up (`backup.create` on SQLite; server backup on PG) and record generation `G`, op id, artifact name.
4. Compare artifact/schema with the op; leave anything a record names to the next compact (`G` removes, `G+1` records).
5. Never hand-edit `gc_intent`, `gc_dead`, markers or `project_epoch`; never unlink `-wal`/`-shm`; never drop a schema `gc_op` names. The retained `done` audit rows are read with `layerfs store repair-mount-holders <store>/<project> <branch> audit` (scoped to that Branch, paged by `--limit`/`--cursor`); a Branch keeps its newest 100 done rows (a release deletes the oldest beyond them), and the jsonl archive export is deferred (#775).
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
rows collected. A `none` outcome (<10 % wholly dead; on SQLite
[decision D6](gc-42-decisions.md), accepted by the owner, also counts dead records inside retained
packs) keeps everything, and the retired names stay taken. Follow the compaction procedure in §6 and the
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


## 8. Disk headroom and peak temp for a SQLite rebuild (S5, D2)

The rebuild writes a fresh Store with only the retained rows
(S5; decision D2 in [`gc-42-decisions.md`](gc-42-decisions.md)). Peak temp is
the new file plus the visited scratch; there is no whole-Store staged copy, no
rollback journal and no `VACUUM` rewrite (the frozen §7 cost sentence includes
a rollback journal/WAL; D2 records why the fresh path has none). Peak disk is
therefore

    old Store + (retained bytes + visited scratch) + margin

The retained bytes are at most the live bytes, so the run needs roughly the
Store's size again, never a multiple of it. The validated prototype measured
1.007× live written and a 1.003× `(old + live)` peak on a 10 GiB fixture (the
earlier `VACUUM INTO` build wrote 4.03× and peaked at 1.577×). Reserve free
space before a rebuild on both the Store's filesystem and its temp directory:

| Store / dead share | Reserve |
|---|---|
| any `rebuild` | old Store + about one live Store (the retained bytes) + a margin |
| any `none` run | a few GB (the visited scratch only) |
| 50 GB Store, ~10% dead | about the Store's size again [E] |

Per-GB rates on the measured host class (**Dell PowerEdge R730, PERC H730 Mini
controller, rotational HDD — not NVMe**; 88 vCPU, 125–135 GB visible RAM,
ext4): the historical `VACUUM INTO` arm measured mark 17.2 s/live GB, build
93.0 s/new GB at 50 GB and 10.3 s/new GB at 10 GB, verify 1.87 s/new GB,
install 7.311 s. The fresh-copy prototype's 10 GiB run measured 32.5 s against
120.8 s for that build; the 10 GB arm is the agent budget, the 50 GB arm is the
owner-final measurement (#42 item 44). Its receipt is
`engine/crates/layerfs-persistence/tests/support/fixtures/sqlite_compact_capacity_measure_50gb.json`
(`none` 1,633.585 s, `rebuild` 5,131.835 s, one run per path on the superseded
build), and the whole-Store 10 GB medians are kept in
`.../sqlite_compact_capacity_measure_whole_store.json` (a transcribed summary:
the raw per-run samples were not retained). The alternative build (clone +
delete + `incremental_vacuum`, below a size threshold on reflink filesystems) is
recorded in D2 and is not adopted.

The per-GB rates above were measured on the earlier whole-file-only fixture
(one whole-file object per pack, one genesis Layer per project). The current
harness (`tools/ci/gc-bench.md`) models Layer history depth, cross-project
shared content and delta chains and records the cold-cache method and 3-run
variance; re-measure the arm before quoting a rate against the current
fixture. The gated 50 GB owner-final arm has not been re-run on it.

## 9. Implementation notes: deviations and the D2 build decision (D1–D5)

Four frozen sentences still differ (D1, D3, D4, D5); D2 is the settled build
decision, whose fresh-copy rebuild follows S5's fresh-Store sentence but
deviates from §7 step 4's payload sweep and rollback journal and from §11's
peak-temp line. All five are recorded in
[`gc-42-decisions.md`](gc-42-decisions.md), which is the only place they are
written: the frozen spec file is unchanged. None changes the artifact set, the
op record or the next-call adoption.

| Entry | Frozen sentence | Implemented |
|---|---|---|
| D1 (§7 step 4) | `PRAGMA foreign_keys=ON` outside the transaction (frozen text, unchanged) | the fresh bulk load runs `foreign_keys=OFF`; the step 6 `foreign_key_check` on the `immutable=1` open is the guard |
| D2 (S5/§3/§7/§11) | "copies only retained rows into a fresh Store"; §7 step 4's payload sweep and rollback-journal transaction; peak old + new + journal/WAL + visited | fresh-copy build (pack-as-unit, no payload sweep, no rollback journal; indexes and triggers after the load); peak old + new + visited (§8) — a recorded deviation from §7/§11, not an edit to the frozen text |
| D3 (§3/§7 step 1.5) | `none` writes nothing; delete the scratch and stop | `none` writes no visited scratch (since 2026-11-18): the mark keeps the reached set in memory, `<store>.compact-visited` is written only when the run rebuilds, and `peakTempBytes` is 0 on `none`. The mark and its intent are one transaction that is rolled back, so no `kept_node`/`gc_intent` row and no WAL frame survives. The `<store>.compact-op` record is still written at admission and removed before the reply; no residue survives |
| D4 (§11) | SQLite phases `mark`, `build`, `verify`, `install` | the settle also writes `cleanup` (§1) |
| D5 (S6/§7) | "A SQLite Store compacts on the machine where it lives"; §7 "Runs as one operation on the Store's machine (S6)" | resolved 2026-11-18: a pushed sqlite Store is compacted by its machine's worker (`store.compact` worker operation), as S6 says |

A10 is an amendment rather than a deviation: it records that
`deadBytesRefined` equals `deadBytesBound` on every path, because the mark
leaves no second payload sweep to refine it. A report never names a different
refined figure on either backend.

Operators do not need to act on D1–D5 during an incident: the artifact set, the
op record and the next-call adoption are unchanged. They matter when reading
the phase or the reported peak, and to a client that compares the frozen text
with the shipped report.
