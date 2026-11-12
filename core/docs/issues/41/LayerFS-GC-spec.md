# LayerFS GC Specification (single file)

> **Status:** candidate specification, may be merged into #42 as a whole. **This file is the only authoritative version.** In this directory the other documents derive from it and cite its section numbers.
> **Scope:** the mechanism layer of #41: correctness of collection decisions, sweep recoverability, fences, reporting definitions, acceptance shape.
> **Out of scope:** the policies already settled by #41 (admission matrix, logical/physical separation, no-retry). Where they conflict, the #41 owner policy wins.
> **BrewFS reference:** `github.com/brewfs/brewfs`, pinned 2026-11-11 `#444`. **Every value cited from it is the code default** (`src/meta/config.rs`, `src/chunk/compact/gc.rs`); its `doc/architecture/compaction-gc.md` has already drifted and is not a source of parameters.

---

## 1. Decision Summary

### 1.1 Settled by this spec (design conclusions, no longer open)

| # | Decision | Rationale / Basis | Where it lands |
|---|---|---|---|
| S1 | Reachability granularity = **Store** (all LayerStacks inside one Store) | The #41 matrix states verbatim "Selected Store, all projects/Branches/historical mounts"; one Store may contain several projects | §5, C1 |
| S2 | Exclusivity = **reuse Store lock + lease + op record** | `store push/pull/remove` already use the same set; lease decides liveness by pid+kernel start+boot | C8, §7.1 |
| S3 | Intent = **`Stage` copy + op record**, no sidecar | `sqlite/transfer` already has `Stage/Verify{bytes,sha256,heads}/Install/Retire` and `resolve` | C7, §7.1 |
| S4 | **No object age**; deletion depends only on closure and pinned | `object_location`/`pack` have no time column; compact is an exclusive window, so the closure is computed then | C2, C4 |
| S5 | Candidates = **`object_location` rows + `content_signature` dependent rows**; `pack` is never a candidate | `pack` has immutability triggers on both ends | C2, §7.2 |
| S6 | A SQLite Store that is not **native** **refuses** compact | `StoreRef::Sqlite{machine}`; #49 states remote SQLite is out of scope | §7.2 |
| S7 | Three reporting definitions + peak temporary space + residue; estimates must be marked `estimate`; capped results must report the cap | #41's three definitions + `operator/files.rs` "never silently reports a total" | C5 |
| S8 | Reader fence = **session-level `(catalog_id, incarnation)` check → `StaleStore`** | incarnation is checked only at open; under WAL no lock blocks a reader | C8 |
| S9 | install is binary, `heads` must match before and after | transfer's `Verify{heads}`; I1 | I1, I3 |
| S10 | Logical deletion does not rewrite history or change IDs | CommitId is content-derived; #41's "immutable IDs must not silently resolve" | I5 |
| S11 | Read availability during compact = **exclusive by default** (reads unavailable); if a "reads available" SLO is required (p99 ≤ 1.2× baseline), PG takes A' (pointer + versioned schema) | Decides whether R1 = E or A'; consistent with C10's exclusive admission | §1.3 R1, §7.3 |

### 1.2 Still open for #42 (with recommendation + cost + cost of choosing wrong)

| # | Question | Recommendation (preferred option) | Effort/Cost | Cost of choosing wrong |
|---|---|---|---|---|
| Q1 | Name reuse: forbid reuse (tombstone), or derive with incarnation | **A: forbid reuse** + terminal tombstone + `NameRetired`, zero migration | Small: one terminal marker + check at create; no incarnation column | Reusing an old name shares a `BranchId` with old history (`blake3(stack‖name)`), so old Commits' Branch attribution goes wrong |
| Q2 | How much `remove_file` can actually reclaim | First measure with `fs.get_storage` + `list_overwritten`; if it is always 0, document that it is a **visibility operation** (saves no space) | Small: one measurement script + docs; no mechanism change | Users plan capacity on "deleting saves space", and it does not |
| Q3 | How to set an age threshold when online collection is introduced | Not in this spec; when the time comes, add timestamp columns first, then measure the longest Commit / longest in-flight handle | Medium: add timestamp columns + migration + threshold contract | Without timestamps an age threshold cannot be implemented; forcing it deletes in-flight writes |
| Q4 | Retention cap for interrupted copies | Do not clean automatically; keep at most one, require manual action beyond that | Small: `doctor` report + runbook | Residue accumulates and consumes disk; automatic cleanup violates "no guessed cleanup" |
| Q5 | Reader behavior when PG swaps tables | **Superseded by C8/R2**: readers go through the `(catalog_id, incarnation)` session check → `StaleStore`, and read the new generation after reconnecting; table names are not versioned | Small: session/transaction entry check (shared with SQLite) | Cross-generation reads (no crash, but wrong data) |
| Q6 | Does `store.compact` promise "reclaim pages" or "reorganize data" | **Split into two levels**: `compact(name)` = delete references + `reclaim_space` (cheap, may return 0); `compact(name, reorg)` = replacement (2× space) | Medium: two entry points + two reporting definitions | With a single level, even small reclamation pays 2× space; page reclamation alone means the docs over-promise |

> **Q6 is answered together with §5.1** of `03-decision-record.md`, because they
> decide the same thing: §5.1's "in-place deletion path" *is* Q6's level 1 on
> PostgreSQL. If PostgreSQL keeps no in-place path, the two-level promise collapses
> to one level there, and the docs must say so instead of promising two.

> Sign-off: adopt the Q1/Q2/Q4/Q5 recommendations directly (small cost, controlled risk); Q3 is explicitly out of this release; Q6 needs the owner to sign off on the "commitment level", because it decides the documentation wording and capacity planning.

### 1.3 Reconciliation with the old `gc-spec.md`

| # | Old design | This spec | Ruling |
|---|---|---|---|
| R1 | PG: two `ALTER SCHEMA RENAME` in one txn (old design) | **Decided by S11**: exclusive window → **E** (two `ALTER SCHEMA RENAME`, still ending at `layerfs`, so both the record and `{s}` stay valid); read-available window → **A'** (pointer + versioned schema, `self.schema` resolved from the pointer at open). "Versioned table names" (A) is no longer adopted: it needs a second placeholder and buys nothing under the exclusive window | **The old R1 rationale is void**: `open.rs:37 schema_of` accepts the schema passed in by the caller, and the two renames still end at `layerfs`, so "the schema name is fixed, therefore it cannot be renamed" does not hold. The real deciding factor is the read-availability SLO (S11), not the name. |
| R2 | Reader fence = a new `compact_intent` row | `catalog_incarnation += 1` + every session/transaction entry checks `(catalog_id, incarnation)` → `StaleStore`; SQLite additionally `(dev, ino)`; `lfq1` across generations → `StaleStore` | **Adopt this spec**. More complete, and reuses the existing `history_meta` without adding a second generation field. |
| R3 | SQLite writer fence = `Replaced` inode check (the old spec found "it does not exist in the code", P0-level new work) | This spec records it as C8b: re-read the generation before install + a write handle on the old generation must fail (reuse `Conflict`) | **Merged**: keep the old spec's P0 analysis as C8b. |
| R4 | rebuild into fresh container (build a new container → verify with a real open → atomic swap → retire) | Reuse transfer's `Stage/Verify/Install/Retire`, inserting only a `rewrite` phase | **Adopt this spec**. Do not rebuild the mechanism, since `transfer` already exists. |

**Extra findings kept from the old spec:** the 8 HistoryCatalog impls (see C9); `project.remove` deletes the project directory in two places, `remove.rs:62` and `family.rs:354` (#45 must change it to a soft delete and must not delete the home dir; `resolve.rs::target` needs a `ProjectRecord`); the only current producer of `MountsActive` is `sandbox.close`, so the Branch-level check is new code.


---

## 2. Existing machinery (must be reused, do not rebuild)

| Facility | Location | Use |
|---|---|---|
| Store lock | `record::lock` (`entity_lock`) | Mutual exclusion for the exclusive window |
| Lease | `control/crates/layerfs-store/src/lease.rs` | `store/<name>/open/<pid>`; a live holder → `InUse` |
| Op record | `control/crates/layerfs-store/src/record.rs`, `control/crates/layerfs-home/src/op.rs` | `Standing = Running / Unresolved / Stale` |
| Copy-verify-install | `control/crates/layerfs-store/src/sqlite/transfer/{step,phases,resolve}.rs` | `StoreStep`: `Stat/Snapshot/Stage/Unstage/Read/Write/Verify/Install/Retire/Remove` |
| Store identity and generation | `history_meta(catalog_id, catalog_incarnation, binding_key, identity_format=3)` | `catalog_incarnation` is the generation |
| Structure/content audit | `persistence/diagnostic/` (`sqlite_structure(path)`, `SqliteHandles::diagnose`), `control/.../diagnostic.rs::sqlite(path, identity, …)` | Check before install |
| Page reclamation | `backend/sqlite/lifecycle/reclamation.rs` (512 pages per call, requires `auto_vacuum=2`) | The cheap half of physical reclamation |
| Metering | `backend.store.project.get_storage`, `backend.store.fs.get_storage`, `backend.store.project.list_overwritten` | The three definitions and the Q2 measurement |
| Bounded traversal | `core/crates/layerfs-content/src/filesystem/qualify/` | Traversal skeleton for mark |
| Operations surface | `store.*` (create/connect/list/status/push/pull/remove), `backup.*`, `doctor`, `project.export` | No parallel machinery |

**Counterexample:** `filesystem/references/` is a reference-effect reducer (bounded ordering, checked release, custody), **not an object-graph refcount**, and cannot serve as GC's counter.

---

## 3. Invariants

| # | Invariant | Guarantee |
|---|---|---|
| I1 | Retained history does not shrink: the heads of live roots match before and after | `Verify{heads}` + closure comparison |
| I2 | A published `pack` is immutable and never deleted | Triggers on both ends (`pack_immutable_*` / `pack_immutable()`) |
| I3 | install is binary: either the old Store as it was, or the new Store fully in effect | A single `Install` (rename) + op phase |
| I4 | Readers do not cross generations | Session-level generation check → `StaleStore` |
| I5 | Logical deletion does not rewrite history or change IDs | `remove_file` is visibility only; CommitId is content-derived |
| I6 | No automatic collection | #41; this spec defines no periodic job |

---

## 4. Net new (four items only)

| # | Item | Nature |
|---|---|---|
| N1 | Reachability closure + generation re-check before install | The only new mechanism |
| N2 | transfer's `rewrite` step + `resolve` registration | The only new code |
| N3 | Representation of logical deletion (`branch`/`layer_stack` have no visibility column) | Schema change, belongs to #42 |
| N4 | Reader-side session-level generation check + `lfq1` mismatch mapped to `StaleStore` | Small change that decides whether #50 is feasible |

---

## 5. Contracts

The following 8 items are frozen before implementation; changes need owner approval.

### C1 Root set and generation (S1)
- `rootGeneration` increases monotonically; it must bump on: `branch.create/fork`, `commit`, `merge`, `branch.remove`, `project.delete`, `project.remove_file`, and the install of `store.compact`.
- The root set covers all content of the selected Store: every live project's `main` and published Layers + every live Branch's head + retained historical Commit roots + staged copies of incomplete ops + pooled dependencies.
- The closure must be computed inside the exclusive window, starting from the frozen generation; caching across operations is forbidden.
- The generation must be re-read before install; if it does not match, abandon the whole round, with no retry.

### C2 Collectability decision (S4, S5)
```
collectable(o) := o ∉ closure(Roots@G) ∧ o ∉ pinned
```
- No object age is introduced: neither `object_location` nor `pack` has a time column (`sql/sqlite/metadata.sql`, `sql/sqlite/objects.sql`); the exclusive window already eliminates "in-flight writes misjudged".
- Candidates are concrete rows: `object_location` + `content_signature`; `pack` is never a candidate; catalog tables such as `history_meta`/`store_policy`/`scope_allocator` are never candidates either.
- The empty root set is legal (after the sole project is soft-deleted): the closure is empty and all objects are collectable, but compact must still produce a Store that can be opened and `doctor`ed.
- `pinned` contains at least: objects referenced by ops and staged copies, both Stores during install, artifacts under an `Unresolved` op name.
- The decision must land as a receipt (G, Roots fingerprint, closure size, collectable count, the three definitions).

### C3 Death decision bound to `(ObjectId, generation)`
For per-object/per-row deletion (PostgreSQL, future online GC), the delete intent must be bound to `(ObjectId, G)`; ABA (same bytes written again after a delete) must not apply an old intent. Whole-file replacement in SQLite is covered by C1's re-check.

### C4 No object age (S4)
- Object age must not be a deletion condition.
- The only monotonic orders are `pack_id` and `commit.created_ms`; they may be used for audit/reconciliation only, never as deletion criteria.
- Retention of old copies is the responsibility of `Retire` + record authority, independent of age.
- If online/concurrent collection is introduced later, timestamp columns must be added first and the age threshold redefined as a contract (Q3).

### C5 The three byte definitions (S7)
| Definition | SQLite | PostgreSQL |
|---|---|---|
| `logical_discarded` | Sum of content lengths of unreachable objects/rows | Sum of row payloads |
| `backend_reusable` | `freelist_count × page_size` | `n_dead_tup` estimate + reusable pages |
| `physical_shrink` | Difference of `page_count × page_size` from the old file; `reclaimed_pages` reported separately | Difference of `pg_relation_size` / `pg_total_relation_size` |

- Available sources: a `commit` row already carries `logical`/`new_canonical`/`new_stored`/`new_metadata`/`created_ms`.
- Must also report: peak temporary space, elapsed time, residue (enumerable).
- Mark estimates `estimate: true`; a diagnostic with a cap must report the cap too.

### C6 Name reuse and terminal state (Q1)
- `BranchId::named = blake3(stack ‖ name)`, and the comment relies on "names are never reused" (`core/crates/layerfs-history/src/contract/identity.rs:266`); once `branch.remove` lands, that premise fails.
- Recommended A: forbid reuse + terminal tombstone, a same-name create returns `NameRetired`; zero migration. Alternative B: incarnation participates in the derivation, requiring migration.
- `branch`/`layer_stack`/`layer` currently have no visibility column (`persistence/sql/sqlite/history.sql`); adding a column means every history query carries that dimension, adding a table avoids it. #42 must list "which queries must filter".
- The split between shared / exclusive project tombstones is decided by #42; this spec requires only "terminal state + decidable generation".

### C7 Intent = `Stage` copy + op record (S3)
- A new sidecar intent file is forbidden.
- The op must name the target artifact (staged path, generation); failure detail carries at least `phase`, `authority`, `artifact`.
- The new phase (`rewrite`) must be registered in `transfer/resolve.rs`'s list of "phases that hold nothing", or it leaves a permanent `Unresolved`.
- Old copies must not be deleted by a time window; `Retire` + record authority decide.

### C8 Identity, generation, and the reader fence (S2, S8)
- Do not add `store_uuid`: reuse `history_meta`, validated by `validate` at open.
- At install, `catalog_incarnation += 1`; a second generation field is forbidden.
- Required: every session/transaction entry checks `(catalog_id, incarnation)`; a mismatch → `StaleStore` (no retry, no automatic reconnect).
- SQLite must additionally check file identity `(dev, ino)`: the Store side has no `locking_mode=EXCLUSIVE`, so the same path can point to a new inode while an old fd is still valid.
- The `lfq1` continuation token is already bound to incarnation, so it is invalid across generations; it must map to `StaleStore`, not to the `invalid("… immutable pin …")` path.
- Admission reuses the Store lock + lease + op; the only new half is the reader side.

### C8b SQLite writer fence (pre-swap writer)
- The `(dev, ino)` install check alone is not enough: after the rename, a pre-swap writer's old fd can still write into the retired inode, and the new Store never sees it: silent data loss.
- SQLite must therefore have a writer fence: re-read the generation before install, and any write handle acquired on the old generation must fail after install (reuse `Conflict`, do not add a rejection code). Without this invariant, `store.compact` could only run with no concurrent writers, which pushes the mutual-exclusion burden onto the caller.
- That fence is **new code** (v2.0.0 has no evidence of anything like `Replaced`), a P0-level implementation item for #49.

### C9 HistoryCatalog constraints (hard constraints for #42)
- `HistoryCatalog` = 32 methods (22 required + 10 default), with 8 implementations (1 real + 6 daemon test doubles + 1 memory).
- New GC methods (remove_branch / soft_delete_project / remove_file / retained_roots / compact-related queries) must have a default `Unsupported` body, or all 8 impls must change at once.
- Do not add `HistoryError` variants: both mapping points use the `_ => "Failed"` wildcard, so a new variant degrades silently to `Failed`.

### C10 Admission scope per operation (#43)
- `branch.remove`: excludes only mounts/sessions of that Branch; other Branches/projects in the Store stay readable and writable.
- `project.delete`: excludes all mounts/sessions of that project (across Branches and historical Commits); other projects in a shared Store stay active; even for the sole project the Store must stay addressable.
- `store.compact`: excludes the entire Store (mounts, SDK sessions, reads, saves, merge/publication, incomplete transfers).
- Rejection matrix: `MountsActive` (mounts/sessions exist), `Busy` (another op is running), `InUse` (a lease is live).
- `project.remove` is currently a hard delete (`remove_layerstack`); #45 must change it to a soft delete; `project.remove` returning `Unsupported` on PG must be fixed as well.



---

## 6. Mark

```
GC(Roots@G):
  pinned  = op/staged copies ∪ in-flight installs ∪ Unresolved-op artifacts
  closure = walk(Roots@G)        // reuse qualify, no new walker
  candidates = all_objects − closure − pinned
  receipt = (G, fingerprint(Roots), |closure|, |candidates|, bytes…)
  if rootGeneration ≠ G: abort, no writes
```

- Bounded window + indexed visited state; a second persistent database is forbidden.
- Must be able to report three classes of anomaly: missing dependency, corrupt dependency, cross-project shared pack. A missing dependency must abort, never be skipped.
- For multiple roots, use the lowest generation `G_min` as a conservative lower bound; roots with `generation > G_min` must be recomputed before install.

---

## 7. Sweep

### 7.1 Sequence (reuse transfer, insert only `rewrite`)
```
check      outside the lock: files and disks on both sides, resolve leftover copies
           inside the lock: record.unblocked(), collect leases, write op (args name source/target)
snapshot   bytes, sha256, heads of an idle Store
stage      create a consistent snapshot copy beside the Store
rewrite    delete unreachable rows on the copy + reclaim        ← the only addition
verify     bytes / sha256 / heads
install    rename the verified copy into place
retire     delete old copies the record no longer names
```
- `uncertain` follows the existing rules: `Unknown`, or the `record` phase, is uncertain; a definite failure of `send/verify/install` keeps the copy, records the op with a definite result, and lets doctor report it.
- install is the only irreversible point; "half new, half old" is forbidden.

### 7.2 SQLite (#49)
0. **Run on the local machine only**: a non-`native` Store (`Placement::Machine(m)` / `Unknown`) must refuse with `Unsupported`; the docs say "`store.pull` home first, then compact".
1. Pass the old Store through `doctor`/`diagnose` first, then enter `check`.
2. `rewrite` deletes unreachable rows on the staged copy and recomputes totals, preserving immutable encodings and IDs; only when `reorg` (Q6) is selected is data re-encoded.
3. Verification has two layers: `sqlite_structure(path, …)` (any path, read-only) → once it can be opened as a Store, `SqliteHandles::diagnose(options, …)`. The CLI's `doctor store integrity <store>` only accepts a registered Store and **cannot** be used on a copy; a copy goes through `control/.../diagnostic.rs::sqlite(path, identity, …)`.
4. On top of diagnostic, add only two assertions: closure consistency, and conservation of row counts and object counts.
5. install reuses `StoreStep::Install`, checking `(catalog_id, catalog_incarnation, dev, ino)`.
6. The old copy is deleted by `StoreStep::Retire`.
7. Insufficient disk reports `Capacity`; on failure keep the op and the staged copy, and do not clean up on your own.
8. **Deletion order is decided by the FK graph**: `content_signature.object_id → object_location.object_id`, `object_location.pack_id → pack.pack_id` (`sql/sqlite/metadata.sql:12-29`), constraints IMMEDIATE (not DEFERRABLE), `foreign_keys=ON`. Delete in the order `content_signature` → `object_location`; `pack` is untouched. An incomplete delete is refused by the engine and rolled back (a safety property), but dependent rows must be cleared first.
9. Division of labor with `reclaim_space`: first check `auto_vacuum=2`; the reclaimable case uses it to produce `backend_reusable`, and the non-reclaimable case has replacement as its only path.

### 7.3 PostgreSQL (#50)
- **Scope**: remote PostgreSQL Stores are in scope (including remote servers); out of scope are remote SQLite and remote object-storage copies.
- A Store is one schema; `{s}` is replaced by `self.schema` at `backend/postgresql/mechanics.rs:21`, and `self.schema` comes from `schema_of` (`open.rs:37`, passed by the caller or defaulting to `layerfs`).
- **The swap method is decided by S11**:
  - **E (default, exclusive window)**: `BEGIN; ALTER SCHEMA layerfs RENAME TO layerfs_c_<n>_old; ALTER SCHEMA layerfs_c_<n> RENAME TO layerfs; COMMIT;` followed by `DROP SCHEMA layerfs_c_<n>_old`. The final name is still `layerfs`, so every record and `{s}` is unchanged; one namespace lock.
  - **A' (read-available window)**: data lives in the versioned schema `layerfs_c_<n>`; the fixed schema holds only a pointer row; `self.schema` is resolved from the pointer at open; swap = one UPDATE of the pointer row, then `DROP SCHEMA` of the old generation. "Versioned table names" (A) is not adopted, because it would add a second placeholder to every SQL statement.
  - **B (alternative)**: paired `ALTER TABLE RENAME` in the same transaction (ACCESS EXCLUSIVE per table, `lock_timeout` must be verified).
  - **C, view indirection**: writability needs triggers; the last alternative.
- Whichever is chosen, the reader fence (C8/R2) is required.
- `pack` has immutability triggers on the PG side too, so deleting rows of the old table is forbidden; reclamation must be "new table + new triggers".
- Deletion order: delete references first (`object_location` etc.), then payload; the intermediate state is `backend_reusable`, reported separately.
  > **[UNRESOLVED — owner decision pending]** This bullet describes *in-place row
  > deletion*, which R1-E does not perform: the swap path copies retained rows into a
  > replacement schema and renames it, so no object row is ever deleted and there is no
  > "references first, payload second" intermediate state. Choose between (a) replacement
  > plus rename, and (b) an additional in-place path; `03-decision-record.md` §5.1 records
  > both and what each does to C3's scope.
- Reader fence: the lane blocks only writers; readers uniformly go through C8's `(catalog_id, incarnation)` session check → `StaleStore` (Q5 is superseded by C8/R2 and is no longer chosen separately).
- Reclamation metric: the delta of `pg_relation_size` / `pg_total_relation_size`; `autovacuum` stays enabled but is not counted.

### 7.4 Leftover recovery
- Every `check` runs `resolve` first, and judges by the op's phase whether that phase holds anything.
- When it cannot be judged, report `Unresolved`; automatic install or automatic cleanup is forbidden.
- An orphan file with no op reports `OrphanArtifact` and is not deleted.

---

## 8. Operations surface

- Method: `store.compact(name)` registers an op, and progress is read from `store.status`; `operation.get` must not be added.
- All four operations take only a resource identifier: `branch.remove(name)`, `project.delete(name)`, `store.compact(name)`, `project.remove_file(project, path)`, and they jointly forbid `confirm`/`force`/`dryRun`/`stopSandboxes`/Store or volume options (#41 verbatim). Implementations do no implicit unmount, no waiting, no retry; an unclean environment returns `MountsActive`/`Busy`/`InUse`.
- Backend route: `backend.store.compact` (following the `backend.store.*` naming family); CLI `store compact`.
- Rejection codes: reuse `Busy`/`InUse`/`MountsActive`/`NoSuchStore`/`Unsupported`/`NotFound`; add `StaleStore`/`NameRetired`/`OrphanArtifact`/`Capacity` (cf. `StaleSandbox`). `Unresolved`/`Stale` are op states, not return codes.
- Documentation must place side by side: `store.remove` (delete the Store and all its projects) and `project.delete` (soft delete, Store still addressable).

---

## 9. Acceptance (#52)

### 9.1 Crash injection (`kill -9` per backend, per phase)
| Phase | Assertion |
|---|---|
| `stage` | The old Store is fully usable; the op and the staged copy are identifiable; no bytes lost |
| `rewrite` | Same as above; the old Store was not touched by a single byte |
| after `verify`, before `install` | No half-new/half-old state; no automatic install |
| `install` (once before and once after the rename) | Either entirely old or entirely new |
| first open after install | The generation bump takes effect; old sessions receive `StaleStore` |

### 9.2 No-accidental-deletion property tests
- Random DAG: `fork / commit / merge / publish / branch.remove / project.delete`.
- Assertion: not one object in the closure of a live root is deleted; closure sets and sizes are compared one by one before and after.
- ABA variant: after deleting candidate `A`, immediately write back the same bytes; the old intent must not take effect.

### 9.3 Contract tests
- Deletion decisions depend only on closure and pinned: assert the implementation reads no time column.
- A second concurrent compact returns `Busy`, and produces no second op or second staged copy.
- Empty root set: after deleting the only project, compact still produces a Store that can be opened, `doctor`ed, and addressed.
- After `remove_file`, the bytes reclaimed agree with the receipt's explanation (0 must be explained).
- Concurrent readers during a PG table swap receive `StaleStore`; after reconnecting they read the complete new generation.
- Run diagnostic once before and once after replacement/swapping; items such as `store.identity` Pass and no new Fail appears.
- A remote client is rejected with `MountsActive` during compact.
- Every injection ends with proof that no staged / target / op / schema residue remains.

---

## 10. Mapping to #41 subtasks

| Subtask | Delivered by this spec | Section |
|---|---|---|
| #42 contracts | C1–C8, generation arithmetic, Q1–Q6, history filter list | §1.2, §5, §6 |
| #43 admission | Reuse Store lock + lease + op; TOCTOU re-check; Branch/project/Store three-level exclusion matrix (C10) | §2, C8, C10, §7.1 |
| #44 branch.remove | Terminal tombstone + generation (Q1) | C6 |
| #45 project.delete | Soft delete + Store addressability | §8 |
| #47 remove_file | View-layer hiding + reclamation definition (Q2) | §8, §9.3 |
| #48 reachability | Frozen roots + bounded traversal + abort on missing dependency | §6 |
| #49 SQLite compact | Reuse transfer + `rewrite` + FK order + refuse non-native | §7.1–7.2 |
| #50 PostgreSQL GC | Swap method decided by S11 (E default / A' read-available); generation + real size metrics | §7.3 |
| #51 API/SDK/CLI | Reuse op and `store.status` | §8 |
| #52 acceptance | All of §9 | §9 |

---

## 11. Non-goals and parameter discipline

- Automatic periodic collection (forbidden by #41). BrewFS's 600 s compaction / 3600 s GC loops are not borrowed.
- BrewFS's fragmentation thresholds (code defaults `min_slice_count=3`, `min_fragment_ratio=0.1`, `sync_threshold=200`): those measure "slices inside the same chunk overwriting each other", and LayerFS objects are immutable, so "unreachable byte ratio" should be used instead.
- light / heavy two-level compaction: LayerFS's "light" equivalent is logical removal, already an operation of its own.
- `VACUUM FULL`-style whole-table reclamation (blocking, 2× space).
- compact of remote object-storage copies (out of scope for #41).
- Any power-loss durability promise (Disposable remains the only execution profile).

> **Parameter discipline:** any BrewFS value cited must be pinned to the code. Its docs say 5 / 350 / 1h / 100 while the code says 3 / 200 / 600 s / 1000; `LockTtlConfig`(10/30/50/5/300) and `BlockGcConfig`(3600/3600/1000/3600) agree.

---

## 12. Evidence and verification record

- **BrewFS-side evidence:** `src/chunk/compact/{gc.rs,compactor.rs,worker.rs}` (two-phase deletion, `record_uncommitted_slice`/`confirm_slice_committed`, lock Drop+TTL self-healing), `src/meta/config.rs` (`CompactConfig`/`LockTtlConfig` defaults), `doc/superpowers/plans/2026-10-10-packed-v3-temporary-handoff.md` (G13's root generation / Deleting re-check / layer-set algebra, and the still-open reader pins / mark-sweep / no accidental deletion).
- **LayerFS-side evidence:** `control/crates/layerfs-store/src/{lease.rs,record.rs}`, `.../sqlite/transfer/{step,phases,resolve}.rs`, `control/crates/layerfs-home/src/op.rs`, `engine/crates/layerfs-persistence/src/{backend/sqlite/profile.rs,backend/sqlite/schema.rs,backend/sqlite/lifecycle/reclamation.rs,diagnostic/*}`, `engine/crates/layerfs-store-service/src/operator/{dispatch,store,files}.rs`, `core/crates/layerfs-content/src/filesystem/qualify/`, `core/crates/layerfs-history/src/contract/identity.rs`.
- **Verification:** 33 passes in total (including 12 clean passes); the last two clean passes converged with no changes. The last three design-level changes were: remove the object-age grace, add the non-native refusal and the empty root set, and add the candidate row set.
- **Still uncovered (needs code to answer):** the concrete SQL batches and transaction boundaries of `rewrite`; the boundary of the `resolve` list; the cross-machine failure semantics of `Retire`; the PG reader strategy (Q5); the commitment level of `store.compact` (Q6).

---

## 13. Concepts borrowed from BrewFS

BrewFS (`github.com/brewfs/brewfs`, pinned 2026-11-11 `#444`) supplies four
*concepts*. Nothing is copied: not code, not parameter values (its docs and code
disagree for the parameters; every value cited here is the code default). Each
concept below states the goal, the BrewFS origin, the safety it buys, and where
LayerFS implements it.

### 13.1 Root generation + re-check before delete

- **Goal.** A mark-sweep pass must never reclaim an object that became reachable
  *after* the reachability closure was computed. A concurrent `branch.fork` /
  `commit` must not lose the object it just started referencing.
- **BrewFS origin.** G13's *root generation* and the *Deleting re-check*: mark
  from a frozen generation and re-check it before the delete.
- **Safety it buys.** No accidental deletion of a newly created root. A mismatch
  abandons the round; it does not retry until the generation happens to match,
  because retrying is how a re-check turns into a race.
- **Implementation.** Contract C1: `rootGeneration` is monotonic and bumped by
  `branch.create/fork`, `commit`, `merge`, `branch.remove`, `project.delete`,
  `project.remove_file`, and the install of `store.compact`. The closure is
  computed inside the exclusive window from the frozen generation (no caching
  across operations); the generation is re-read immediately before `install`;
  a mismatch abandons the whole round.
- **Difference from BrewFS.** BrewFS stores the generation as a compact-loop
  detail; here it is a contract-level bump point list, so every operation that
  changes the root set is forced to register.

### 13.2 Two-phase deletion (record, then confirm)

- **Goal.** A crash in the middle of a delete must never leave a half-deleted
  Store. The system must be able to tell "delete was intended but not confirmed"
  from "delete happened".
- **BrewFS origin.** `record_uncommitted_slice` / `confirm_slice_committed`:
  record the intent first, confirm after the physical delete.
- **Safety it buys.** A crash before `install` leaves the old Store byte-identical
  (invariant I3: `install` is binary, all-old or all-new); there is no
  "half new, half old" state; an uncertain op is reported, not guessed at.
- **Implementation.** Contract C7: no sidecar intent file. The intent is the op
  record plus the `Stage` copy — the op names the staged path and the generation,
  and its failure detail carries at least `phase`, `authority`, `artifact`. The
  delete happens only on the staged copy. `Install` is the confirmation point and
  is the only irreversible step; `Retire` deletes the old copy afterwards. The new
  `rewrite` phase is registered in `transfer/resolve.rs`'s "holds nothing" list so
  it cannot leave a permanent `Unresolved`.
- **Difference from BrewFS.** BrewFS's intent is a per-slice record; LayerFS's is
  the op record plus the existing `transfer` (`Stage`/`Verify`/`Install`/`Retire`)
  machinery, so the protocol is the one the Store already uses for push/pull and
  not a second recovery protocol.

### 13.3 Lock self-healing (Drop + TTL)

- **Goal.** A holder that dies (kill -9, power loss) must not wedge the Store's
  exclusive lock forever.
- **BrewFS origin.** The lock releases on `Drop` in the normal path and expires on
  a TTL when the holder died (`LockTtlConfig`, code defaults 10/30/50/5/300).
- **Safety it buys.** No permanent wedge; a second compact is refused `Busy` /
  `InUse` instead of running concurrently; a crashed compact leaves a recoverable
  op that `doctor` can report.
- **Implementation.** Contract S2 and §2: exclusive access reuses the Store lock
  (`record.rs::lock`) + the lease (`lease.rs`, `store/<name>/open/<pid>`) + the op
  record (`op.rs`, `Standing = Running / Unresolved / Stale`). The lease's liveness
  is judged from pid + kernel start time + boot, so a live holder is never
  misjudged as dead.
- **Difference from BrewFS.** BrewFS's fallback is a fixed TTL; LayerFS's primary
  judgement is liveness (pid/kernel-start/boot) and TTL is only the backstop, so
  a slow-but-alive holder is not mistaken for a dead one.

### 13.4 Closure / layer-set algebra

- **Goal.** Decide which objects are still reachable. The roots are many
  (project `main`, Branch heads, retained historical Commits, Layers, staged
  copies of incomplete ops, pooled dependencies), references are transitive
  (delta base -> earlier base -> ...), objects are shared across projects, and a
  producer may already be soft-deleted while its objects are still referenced.
- **BrewFS origin.** G13's *layer-set algebra*.
- **Safety it buys.** Retained history never shrinks (I1: `Verify{heads}` plus
  closure comparison); shared content, transitive delta bases and pooled inodes
  stay alive even when the producer Branch/project was soft-deleted; a missing
  dependency aborts the round instead of being skipped (skipping is how a live
  object gets classified as dead).
- **Implementation.** Contracts C1/C2 and §6: the closure walks the retained roots
  with the existing `layerfs-content` `qualify` traversal (a second walker is
  forbidden) and `references`/`decode_references`; `collectable(o) := o ∉
  closure(Roots@G) ∧ o ∉ pinned`; the walk is bounded and keeps an indexed
  visited set; no second persistent database.
- **Difference from BrewFS.** BrewFS defines its own layer model for the algebra;
  LayerFS reuses the traversal and reference decoder it already has, so the GC
  consumes the same reachability the rest of the product uses.
