# LayerFS GC Specification (single file)

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

> **Authority:** may be merged into layerfs-project#42 as a whole. **This file is the only authoritative version.** In this directory the other documents derive from it and cite its section numbers.
> **Scope:** the mechanism layer of layerfs-project#41: correctness of collection decisions, sweep recoverability, fences, reporting definitions, acceptance shape.
> **Out of scope:** the policies already settled by layerfs-project#41 (admission matrix, logical/physical separation, no-retry). Where they conflict, the layerfs-project#41 owner policy wins.
> **Parameter discipline:** every external numeric value this spec cites is the **code default** of the system it came from, not its documentation; external docs drift and are not a source of parameters.

---

## 1. Decision Summary

### 1.1 Settled by this spec (design conclusions, no longer open)

| # | Decision | Rationale / Basis | Where it lands |
|---|---|---|---|
| S1 | Reachability granularity = **Store** (all LayerStacks inside one Store) | The layerfs-project#41 matrix states verbatim "Selected Store, all projects/Branches/historical mounts"; one Store may contain several projects | §5, C1 |
| S2 | Exclusivity = **reuse Store lock + lease + op record** | `store push/pull/remove` already use the same set; lease decides liveness by pid+kernel start+boot | C8, §7.1 |
| S3 | Intent = **`Stage` copy + op record**, no sidecar | `sqlite/transfer` already has `Stage/Verify{bytes,sha256,heads}/Install/Retire` and `resolve` | C7, §7.1 |
| S4 | **No object age**; deletion depends only on closure and pinned | `object_location`/`pack` have no time column; compact is an exclusive window, so the closure is computed then | C2, C4 |
| S5 | Reclamation unit = **the pack**. Published `pack` / `pack_unit` rows stay immutable *in the live Store*; `rewrite` builds the staged copy as a **fresh Store that receives only retained rows** (packs and pack units holding at least one live object, their `object_location` rows, live `inode_value_block` rows, catalog/history tables), so a wholly dead pack is simply not copied and no trigger fires. `content_signature` is a dedupe hint ring, not a reclamation source | Payload bytes live in `pack.body` / `pack_unit.body`, both guarded by `BEFORE UPDATE/DELETE` triggers; `object_location` maps an object to `(pack_id, group_number, record_number)`, so one pack holds many objects and deleting location rows alone frees almost nothing | C2, §7.2 |
| S6 | A SQLite Store that is not **native** **refuses** compact; the workaround (pull home, compact, push back) costs two full transfers | `StoreRef::Sqlite{machine}`; layerfs-project#49 states remote SQLite is out of scope; compact inside the machine's worker is an open item (§7.2) | §7.2 |
| S7 | Three reporting definitions + peak temporary space + residue; estimates must be marked `estimate`; capped results must report the cap | layerfs-project#41's three definitions + `operator/files.rs` "never silently reports a total" | C5 |
| S8 | Reader fence = **session-level `(catalog_id, incarnation)` check → `StaleStore`** | incarnation is checked only at open; under WAL no lock blocks a reader | C8 |
| S9 | install is binary, `heads` must match before and after | transfer's `Verify{heads}`; I1 | I1, I3 |
| S10 | Logical deletion does not rewrite history or change IDs | CommitId is content-derived; layerfs-project#41's "immutable IDs must not silently resolve" | I5 |
| S11 | Read availability during compact = **exclusive by default** (reads unavailable); if a "reads available" SLO is required (p99 ≤ 1.2× baseline), PG takes A' (pointer + versioned schema) | Decides whether R1 = E or A'; consistent with C10's exclusive admission | §1.3 R1, §7.3 |

### 1.2 Still open for layerfs-project#42 (with recommendation + cost + cost of choosing wrong)

| # | Question | Recommendation (preferred option) | Effort/Cost | Cost of choosing wrong |
|---|---|---|---|---|
| Q1 | Name reuse: forbid reuse (tombstone), or derive with incarnation | **A: forbid reuse** + terminal tombstone + `NameRetired`, zero migration | Small: one terminal marker + check at create; no incarnation column | Reusing an old name shares a `BranchId` with old history (`blake3(stack‖name)`), so old Commits' Branch attribution goes wrong |
| Q2 | How much `remove_file` can actually reclaim | First measure with `fs.get_storage` + `list_overwritten`; if it is always 0, document that it is a **visibility operation** (saves no space) | Small: one measurement script + docs; no mechanism change | Users plan capacity on "deleting saves space", and it does not |
| Q3 | How to set an age threshold when online collection is introduced | Not in this spec; when the time comes, add timestamp columns first, then measure the longest Commit / longest in-flight handle | Medium: add timestamp columns + migration + threshold contract | Without timestamps an age threshold cannot be implemented; forcing it deletes in-flight writes |
| Q4 | Retention cap for interrupted copies | Do not clean automatically; keep at most one, require manual action beyond that | Small: `doctor` report + runbook | Residue accumulates and consumes disk; automatic cleanup violates "no guessed cleanup" |
| Q5 | Reader behavior when PG swaps tables | **Superseded by C8/R2**: readers go through the `(catalog_id, incarnation)` session check → `StaleStore`, and read the new generation after reconnecting; table names are not versioned | Small: session/transaction entry check (shared with SQLite) | Cross-generation reads (no crash, but wrong data) |
| Q6 | Does `store.compact` promise "drop dead packs" or "reorganize data" | **Split into two levels**, both built as a fresh staged Store (§7.1): `compact(name)` = level 1, copy only retained rows, so **wholly dead packs are dropped** (may return 0 when every pack still holds a live object); `compact(name, reorg)` = level 2, additionally **repack live objects out of partially dead packs** into new packs (object ids unchanged, `object_location` changes) | Medium: two entry points + two reporting definitions | Level 1 alone leaves dead objects that share a pack with a live one; promising "all discarded bytes are reclaimed" without level 2 over-promises |

> **Q6 is answered together with §5.1** of `03-decision-record.md`. Neither level
> deletes rows in place: level 1 is the replacement copy without wholly dead packs,
> which is what R1-E's "copy retained rows" already does on PostgreSQL, and level 2
> adds repacking to that copy. Both levels therefore exist on PostgreSQL under
> §5.1 reading (a); reading (b) would add a separate in-place entry point with its
> own report, and the docs must then say which level it is.

> Sign-off: adopt the Q1/Q2/Q4/Q5 recommendations directly (small cost, controlled risk); Q3 is explicitly out of this release; Q6 needs the owner to sign off on the "commitment level", because it decides the documentation wording and capacity planning.

### 1.3 Reconciliation with the old `gc-spec.md`

| # | Old design | This spec | Ruling |
|---|---|---|---|
| R1 | PG: two `ALTER SCHEMA RENAME` in one txn (old design) | **Decided by S11**: exclusive window → **E** (two `ALTER SCHEMA RENAME`, still ending at `layerfs`, so both the record and `{s}` stay valid); read-available window → **A'** (pointer + versioned schema, `self.schema` resolved from the pointer at open). "Versioned table names" (A) is no longer adopted: it needs a second placeholder and buys nothing under the exclusive window | **The old R1 rationale is void**: `open.rs:37 schema_of` accepts the schema passed in by the caller, and the two renames still end at `layerfs`, so "the schema name is fixed, therefore it cannot be renamed" does not hold. The real deciding factor is the read-availability SLO (S11), not the name. |
| R2 | Reader fence = a new `compact_intent` row | `catalog_incarnation += 1` + every session/transaction entry checks `(catalog_id, incarnation)` → `StaleStore`; SQLite additionally `(dev, ino)`; `lfq1` across generations → `StaleStore` | **Adopt this spec**. More complete, and reuses the existing `history_meta` without adding a second generation field. |
| R3 | SQLite writer fence = `Replaced` inode check (the old spec found "it does not exist in the code", P0-level new work) | This spec records it as C8b: at the start of every write transaction, compare the Store path's `(dev, ino)` with the open file's `fstat`; a mismatch fails with `Conflict` | **Merged**: keep the old spec's P0 analysis as C8b. |
| R4 | rebuild into fresh container (build a new container → verify with a real open → atomic swap → retire) | Reuse transfer's `Stage/Verify/Install/Retire`, inserting only a `rewrite` phase | **Adopt this spec**. Do not rebuild the mechanism, since `transfer` already exists. |

**Extra findings kept from the old spec:** the 8 HistoryCatalog impls (see C9); `project.remove` deletes the project directory in two places, `remove.rs:62` and `family.rs:354` (layerfs-project#45 must change it to a soft delete and must not delete the home dir; `resolve.rs::target` needs a `ProjectRecord`); the only current producer of `MountsActive` is `sandbox.close`, so the Branch-level check is new code.


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
| Page reclamation | `backend/sqlite/lifecycle/reclamation.rs` (512 pages per call, requires `auto_vacuum=2`) | Returns pages already on the freelist; it frees no pack bytes, because no pack row is deleted in the live Store |
| Metering | `backend.store.project.get_storage`, `backend.store.fs.get_storage`, `backend.store.project.list_overwritten` | The three definitions and the Q2 measurement |
| Bounded traversal | `core/crates/layerfs-content/src/filesystem/qualify/` | Traversal skeleton for mark |
| Operations surface | `store.*` (create/connect/list/status/push/pull/remove), `backup.*`, `doctor`, `project.export` | No parallel machinery |

**Counterexample:** `filesystem/references/` is a reference-effect reducer (bounded ordering, checked release, custody), **not an object-graph refcount**, and cannot serve as GC's counter.

---

## 3. Invariants

| # | Invariant | Guarantee |
|---|---|---|
| I1 | Retained history does not shrink: the heads of live roots match before and after | `Verify{heads}` + closure comparison |
| I2 | A published `pack` / `pack_unit` is immutable and never deleted **in the live Store**; compact drops a dead pack only by not copying it into the fresh staged Store | Triggers on both ends (`pack_immutable_*`, `pack_unit_immutable_*` / `pack_immutable()`) |
| I3 | install is binary: either the old Store as it was, or the new Store fully in effect | A single `Install` (rename) + op phase |
| I4 | Readers do not cross generations | Session-level generation check → `StaleStore` |
| I5 | Logical deletion does not rewrite history or change IDs | `remove_file` is visibility only; CommitId is content-derived |
| I6 | No automatic collection | layerfs-project#41; this spec defines no periodic job |

---

## 4. Net new (four items only)

| # | Item | Nature |
|---|---|---|
| N1 | Reachability closure + `catalog_incarnation` / `Verify{heads}` re-check before install | The only new mechanism |
| N2 | transfer's `rewrite` step + `resolve` registration | The only new code |
| N3 | Representation of logical deletion (`branch`/`layer_stack` have no visibility column) | Schema change, belongs to layerfs-project#42 |
| N4 | Reader-side session-level generation check + `lfq1` mismatch mapped to `StaleStore` | Small change that decides whether layerfs-project#50 is feasible |

---

## 5. Contracts

The following 11 items (C1–C10 and C8b) are frozen before implementation; changes need owner approval.

### C1 Root set and generation (S1)
- The generation `G` is the Store generation, `history_meta.catalog_incarnation`; no per-root or per-commit counter is added (C8 forbids a second generation field). It is bumped by the install of `store.compact` (C8).
- Under C10 `store.compact` holds the whole Store exclusively, so no `commit`, `fork`, `merge`, `branch.remove`, `project.delete` or `project.remove_file` can run between mark and install; the roots cannot move inside the window.
- The root set covers all content of the selected Store: every live project's `main` and published Layers + every live Branch's head + retained historical Commit roots + staged copies of incomplete ops + pooled dependencies.
- The closure must be computed inside the exclusive window, starting from the frozen generation; caching across operations is forbidden.
- Before install, re-check with the existing facilities: `catalog_incarnation` must still equal `G`, and `Verify{heads}` must match the heads recorded at `snapshot`. A mismatch abandons the whole round, with no retry.

### C2 Collectability decision (S4, S5)
```
collectable(o) := o ∉ closure(Roots@G) ∧ o ∉ pinned
```
- No object age is introduced: neither `object_location` nor `pack` has a time column (`sql/sqlite/metadata.sql`, `sql/sqlite/objects.sql`); the exclusive window already eliminates "in-flight writes misjudged".
- The unit that frees bytes is the pack: payload lives in `pack.body` and, in the units layout, `pack_unit.body`. A pack is **retained** when at least one object it holds (`object_location.pack_id`) is not collectable; a pack with no such object is **dead** and is not copied into the staged Store (level 1). With `reorg` (level 2), live objects of partially dead packs are repacked into new packs. Catalog tables such as `history_meta`/`store_policy`/`scope_allocator` are always copied.
- Published `pack` / `pack_unit` rows are never updated or deleted in the live Store (I2); the staged Store is fresh, so their triggers never fire.
- `content_signature` is an 8192-slot ring of dedupe hints that reference `object_location`. It is not a reclamation source: entries naming an object that is not copied are cleared in the staged Store.
- The empty root set is legal (after the sole project is soft-deleted): the closure is empty and all objects are collectable, but compact must still produce a Store that can be opened and `doctor`ed.
- `pinned` contains at least: objects referenced by ops and staged copies, both Stores during install, artifacts under an `Unresolved` op name.
- The decision must land as a receipt (G, Roots fingerprint, closure size, collectable count, the three definitions).

### C3 Death decision bound to `(ObjectId, generation)`
For per-object/per-row deletion (PostgreSQL, future online GC), the delete intent must be bound to `(ObjectId, G)`, where `G` is `catalog_incarnation` at the time the intent was formed; ABA (same bytes written again after a delete) must not apply an old intent. Whole-file replacement in SQLite is covered by C1's re-check.

### C4 No object age (S4)
- Object age must not be a deletion condition.
- The only monotonic orders are `pack_id` and `commit.created_ms`; they may be used for audit/reconciliation only, never as deletion criteria.
- Retention of old copies is the responsibility of `Retire` + record authority, independent of age.
- If online/concurrent collection is introduced later, timestamp columns must be added first and the age threshold redefined as a contract (Q3).

### C5 The three byte definitions (S7)
| Definition | SQLite | PostgreSQL |
|---|---|---|
| `logical_discarded` | Sum of `canonical_length` of collectable objects | Sum of row payloads |
| `backend_reusable` | `freelist_count × page_size` of the old file (pages SQLite could reuse without a copy; compact does not create them) | `n_dead_tup` estimate + reusable pages (an in-place path only; 0 under replacement, see `03-decision-record.md` §5.1) |
| `physical_shrink` | Old file's `page_count × page_size` minus the installed file's. It comes from packs not copied: report `dropped_packs` and their `pack.length` / `pack_unit.length` sum separately, and with `reorg` the bytes freed by repacking | Difference of `pg_relation_size` / `pg_total_relation_size` between the old and new schema, with `dropped_packs` reported the same way |

- `logical_discarded` and `physical_shrink` differ by design: a collectable object in a retained pack is discarded but not freed until `reorg`.

- Available sources: a `commit` row already carries `logical`/`new_canonical`/`new_stored`/`new_metadata`/`created_ms`.
- Must also report: peak temporary space, elapsed time, residue (enumerable).
- Mark estimates `estimate: true`; a diagnostic with a cap must report the cap too.

### C6 Name reuse and terminal state (Q1)
- `BranchId::named = blake3(stack ‖ name)`, and the comment relies on "names are never reused" (`core/crates/layerfs-history/src/contract/identity.rs:266`); once `branch.remove` lands, that premise fails.
- Recommended A: forbid reuse + terminal tombstone, a same-name create returns `NameRetired`; zero migration. Alternative B: incarnation participates in the derivation, requiring migration.
- `branch`/`layer_stack`/`layer` currently have no visibility column (`persistence/sql/sqlite/history.sql`); adding a column means every history query carries that dimension, adding a table avoids it. layerfs-project#42 must list "which queries must filter".
- The split between shared / exclusive project tombstones is decided by layerfs-project#42; this spec requires only "terminal state + decidable generation".

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
- SQLite must therefore have a writer fence: any write attempted through a handle opened on the replaced file must fail after install (reuse `Conflict`, do not add a rejection code). Without this invariant, `store.compact` could only run with no concurrent writers, which pushes the mutual-exclusion burden onto the caller.
- Comparing incarnations cannot be that fence: an old-fd writer reads `catalog_incarnation` from the retired file, where it never changes, so it always sees its own generation.
- **The fence:** at the start of every write transaction, compare the Store path's current `(dev, ino)` (`stat` of the path) with the open file's `(dev, ino)` (`fstat` of the connection's fd); a mismatch returns `Conflict` before anything is written. No new generation field or struct is added; `history_meta` stays the only identity record (C8).
- The check runs after the write lock is taken (`BEGIN IMMEDIATE`), and compact holds the old file's write lock from `snapshot` through the rename, so a check cannot pass and then be overtaken by the swap. The file identity comes from SQLite's own fd, never from a second fd on the main file (closing one drops the process's POSIX locks on it).
- That fence is **new code** (v2.0.0 has no evidence of anything like `Replaced`), a P0-level implementation item for layerfs-project#49.

### C9 HistoryCatalog constraints (hard constraints for layerfs-project#42)
- `HistoryCatalog` = 32 methods (22 required + 10 default), with 8 implementations (1 real + 6 daemon test doubles + 1 memory).
- New GC methods (remove_branch / soft_delete_project / remove_file / retained_roots / compact-related queries) must have a default `Unsupported` body, or all 8 impls must change at once.
- Do not add `HistoryError` variants: both mapping points use the `_ => "Failed"` wildcard, so a new variant degrades silently to `Failed`.

### C10 Admission scope per operation (layerfs-project#43)
- `branch.remove`: excludes only mounts/sessions of that Branch; other Branches/projects in the Store stay readable and writable.
- `project.delete`: excludes all mounts/sessions of that project (across Branches and historical Commits); other projects in a shared Store stay active; even for the sole project the Store must stay addressable.
- `store.compact`: excludes the entire Store (mounts, SDK sessions, reads, saves, merge/publication, incomplete transfers).
- Rejection matrix: `MountsActive` (mounts/sessions exist), `Busy` (another op is running), `InUse` (a lease is live).
- `project.remove` is currently a hard delete (`remove_layerstack`); layerfs-project#45 must change it to a soft delete; `project.remove` returning `Unsupported` on PG must be fixed as well.



---

## 6. Mark

```
GC(Roots@G):
  pinned  = op/staged copies ∪ in-flight installs ∪ Unresolved-op artifacts
  closure = walk(Roots@G)        // reuse qualify, no new walker
  candidates = all_objects − closure − pinned
  dead_packs = packs (and pack units) holding no object outside candidates
  receipt = (G, fingerprint(Roots), |closure|, |candidates|, |dead_packs|, bytes…)
  before install: if catalog_incarnation ≠ G or Verify{heads} fails: abort, no writes
```

- Bounded window + indexed visited state; a second persistent database is forbidden.
- Must be able to report three classes of anomaly: missing dependency, corrupt dependency, cross-project shared pack. A missing dependency must abort, never be skipped.
- All roots are read at the one Store generation `G`; under C10's exclusive window they cannot move before install, so no per-root generation is kept.

---

## 7. Sweep

### 7.1 Sequence (reuse transfer, insert only `rewrite`)
```
check      outside the lock: files and disks on both sides, resolve leftover copies
           inside the lock: record.unblocked(), collect leases, write op (args name source/target)
snapshot   bytes, sha256, heads of an idle Store
stage      create the staged copy beside the Store as a fresh, empty Store
rewrite    copy only retained rows from the idle Store into it           ← the only addition
verify     heads equal the snapshot's; bytes / sha256 of the staged file as written
install    rename the verified copy into place
retire     delete old copies the record no longer names
```
- `uncertain` follows the existing rules: `Unknown`, or the `record` phase, is uncertain; a definite failure of `send/verify/install` keeps the copy, records the op with a definite result, and lets doctor report it.
- install is the only irreversible point; "half new, half old" is forbidden.

### 7.2 SQLite (layerfs-project#49)
0. **Run on the local machine only**: a non-`native` Store (`Placement::Machine(m)` / `Unknown`) must refuse with `Unsupported`; the docs say "`store.pull` home first, then compact, then `store.push` back". That workaround costs **two full transfers** of the Store. **Open item:** run compact inside the machine's worker instead; off Linux every SQLite Store not on `native` is already opened by that machine's worker (`core/docs/issues/533/DECISIONS.md`, "H §7.7 route table"), on the same kernel as the Store, so compact there would need no transfer.
1. Pass the old Store through `doctor`/`diagnose` first, then enter `check`.
2. `rewrite` creates the staged copy as a **fresh Store** (same schema script) and inserts only retained rows: retained `pack` / `pack_unit` rows, the `object_location` rows of live objects, live `inode_value_block` rows, `content_signature` entries that still name a copied object, and the catalog/history tables; it then recomputes totals. Dead packs are not copied, so nothing is deleted and the immutability triggers never fire. Encodings and object IDs are preserved. With `reorg` (Q6 level 2), live objects of partially dead packs are written into new packs; their ids are unchanged and only their `object_location` rows change.
3. Verification has two layers: `sqlite_structure(path, …)` (any path, read-only) → once it can be opened as a Store, `SqliteHandles::diagnose(options, …)`. The CLI's `doctor store integrity <store>` only accepts a registered Store and **cannot** be used on a copy; a copy goes through `control/.../diagnostic.rs::sqlite(path, identity, …)`.
4. On top of diagnostic, add only two assertions: closure consistency, and that the copy's object, pack and row counts equal the retained counts recorded by mark.
5. install reuses `StoreStep::Install`, checking `(catalog_id, catalog_incarnation, dev, ino)`. WAL files are named after the path, not the inode: before the rename, every connection on the old file is checkpointed and closed and no `-wal`/`-shm` content remains beside it (transfer's `steps.rs` already checkpoints a leftover `-wal`), and the staged Store is installed fully checkpointed, so no opener of the new file can replay the old file's log. layerfs-project#49 confirms this in its crash-injection cases.
6. The old copy is deleted by `StoreStep::Retire`.
7. Insufficient disk reports `Capacity`; on failure keep the op and the staged copy, and do not clean up on your own.
8. **Insertion order is decided by the FK graph**: `pack_unit.pack_id → pack.pack_id`, `object_location.pack_id → pack.pack_id`, `content_signature.object_id → object_location.object_id` (`sql/sqlite/objects*.sql`, `sql/sqlite/metadata.sql`), constraints IMMEDIATE, `foreign_keys=ON`. Insert `pack` → `pack_unit` → `object_location` → `content_signature` into the staged Store. In-place deletion in the live Store stays ruled out (`03-decision-record.md` §5.1a).
9. `reclaim_space` is not part of compact: the fresh staged Store has no free pages to return. It only reports the old file's `backend_reusable`.

### 7.3 PostgreSQL (layerfs-project#50)
- **Scope**: remote PostgreSQL Stores are in scope (including remote servers); out of scope are remote SQLite and remote object-storage copies.
- A Store is one schema; `{s}` is replaced by `self.schema` at `backend/postgresql/mechanics.rs:21`, and `self.schema` comes from `schema_of` (`open.rs:37`, passed by the caller or defaulting to `layerfs`).
- **The swap method is decided by S11**:
  - **E (default, exclusive window)**: `BEGIN; ALTER SCHEMA layerfs RENAME TO layerfs_c_<n>_old; ALTER SCHEMA layerfs_c_<n> RENAME TO layerfs; COMMIT;` followed by `DROP SCHEMA layerfs_c_<n>_old`. The final name is still `layerfs`, so every record and `{s}` is unchanged; one namespace lock.
  - **A' (read-available window)**: data lives in the versioned schema `layerfs_c_<n>`; the fixed schema holds only a pointer row; `self.schema` is resolved from the pointer at open; swap = one UPDATE of the pointer row, then `DROP SCHEMA` of the old generation. "Versioned table names" (A) is not adopted, because it would add a second placeholder to every SQL statement.
  - **B (alternative)**: paired `ALTER TABLE RENAME` in the same transaction (ACCESS EXCLUSIVE per table, `lock_timeout` must be verified).
  - **C, view indirection**: writability needs triggers; the last alternative.
- Whichever is chosen, the reader fence (C8/R2) is required.
- `pack` has immutability triggers on the PG side too, so deleting rows of the old table is forbidden; reclamation is "new schema + new triggers": the replacement schema receives only retained packs and rows, exactly as on SQLite (§7.2 step 2), and dead packs leave with `DROP SCHEMA … CASCADE`.
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
- All four operations take only a resource identifier: `branch.remove(name)`, `project.delete(name)`, `store.compact(name)`, `project.remove_file(project, path)`, and they jointly forbid `confirm`/`force`/`dryRun`/`stopSandboxes`/Store or volume options (layerfs-project#41 verbatim). Implementations do no implicit unmount, no waiting, no retry; an unclean environment returns `MountsActive`/`Busy`/`InUse`.
- Backend route: `backend.store.compact` (following the `backend.store.*` naming family); CLI `store compact`.
- Rejection codes: reuse `Busy`/`InUse`/`MountsActive`/`NoSuchStore`/`Unsupported`/`NotFound`; add `StaleStore`/`NameRetired`/`OrphanArtifact`/`Capacity` (cf. `StaleSandbox`). `Unresolved`/`Stale` are op states, not return codes.
- Documentation must place side by side: `store.remove` (delete the Store and all its projects) and `project.delete` (soft delete, Store still addressable).

---

## 9. Acceptance (layerfs-project#52)

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

## 10. Mapping to layerfs-project#41 subtasks

| Subtask | Delivered by this spec | Section |
|---|---|---|
| layerfs-project#42 contracts | C1–C10 and C8b, generation arithmetic, Q1–Q6, history filter list | §1.2, §5, §6 |
| layerfs-project#43 admission | Reuse Store lock + lease + op; TOCTOU re-check; Branch/project/Store three-level exclusion matrix (C10) | §2, C8, C10, §7.1 |
| layerfs-project#44 branch.remove | Terminal tombstone + generation (Q1) | C6 |
| layerfs-project#45 project.delete | Soft delete + Store addressability | §8 |
| layerfs-project#47 remove_file | View-layer hiding + reclamation definition (Q2) | §8, §9.3 |
| layerfs-project#48 reachability | Frozen roots + bounded traversal + abort on missing dependency | §6 |
| layerfs-project#49 SQLite compact | Reuse transfer + `rewrite` + FK order + refuse non-native | §7.1–7.2 |
| layerfs-project#50 PostgreSQL GC | Swap method decided by S11 (E default / A' read-available); generation + real size metrics | §7.3 |
| layerfs-project#51 API/SDK/CLI | Reuse op and `store.status` | §8 |
| layerfs-project#52 acceptance | All of §9 | §9 |

---

## 11. Non-goals and parameter discipline

- Automatic periodic collection (forbidden by layerfs-project#41): no 600 s compaction / 3600 s GC loop is adopted.
- Fragmentation thresholds (`min_slice_count`, `min_fragment_ratio`, `sync_threshold`) measure slices inside the same **mutable** chunk overwriting each other. LayerFS objects are immutable, so "unreachable byte ratio" is the equivalent measure.
- light / heavy two-level compaction: LayerFS's "light" equivalent is logical removal, already an operation of its own.
- `VACUUM FULL`-style whole-table reclamation (blocking, 2× space).
- compact of remote object-storage copies (out of scope for layerfs-project#41).
- Any power-loss durability promise (Disposable remains the only execution profile).


---

## 12. Evidence and verification record

- **LayerFS-side evidence:** `control/crates/layerfs-store/src/{lease.rs,record.rs}`, `.../sqlite/transfer/{step,phases,resolve}.rs`, `control/crates/layerfs-home/src/op.rs`, `engine/crates/layerfs-persistence/src/{backend/sqlite/profile.rs,backend/sqlite/schema.rs,backend/sqlite/lifecycle/reclamation.rs,diagnostic/*}`, `engine/crates/layerfs-store-service/src/operator/{dispatch,store,files}.rs`, `core/crates/layerfs-content/src/filesystem/qualify/`, `core/crates/layerfs-history/src/contract/identity.rs`.
- **Still uncovered (needs code to answer):** the concrete SQL batches and transaction boundaries of `rewrite`; the boundary of the `resolve` list; the cross-machine failure semantics of `Retire`; the PG reader strategy (Q5); the commitment level of `store.compact` (Q6).

---

## 13. Safety mechanisms

Four mechanisms carry the correctness of the sweep. Each states the goal, the
mechanism, the safety it guarantees, and where it is implemented.

### 13.1 Store generation + re-check before install

- **Goal.** A mark-sweep pass must never drop an object that became reachable
  *after* the reachability closure was computed. A `branch.fork` / `commit` must
  not lose the object it just started referencing.
- **Mechanism.** C10 makes `store.compact` exclusive over the whole Store, so no
  `commit`, `fork` or `merge` can run between mark and install. The closure is
  computed at the Store generation `G = catalog_incarnation`, and immediately
  before install the existing checks run again: `catalog_incarnation` must still
  be `G` and `Verify{heads}` must match the snapshot; a mismatch invalidates the
  whole round.
- **Safety.** No accidental loss of a newly created root. A mismatch abandons
  the round; it does not retry until the generation happens to match, because
  retrying is how a re-check turns into a race.
- **Implementation.** Contract C1: no new counter; `catalog_incarnation` is bumped
  by the install of `store.compact` (C8). No caching across operations; the
  re-check happens just before `install`.

### 13.2 Two-phase deletion (record, then confirm)

- **Goal.** A crash in the middle of a delete must never leave a half-deleted
  Store. The system must be able to tell "delete was intended but not confirmed"
  from "delete happened".
- **Mechanism.** Record the intent first (the op record names the staged copy and
  the generation), perform the delete only on the staged copy, and confirm with a
  single `install`; the old copy is retired afterwards.
- **Safety.** A crash before `install` leaves the old Store byte-identical
  (invariant I3: `install` is binary, all-old or all-new); there is no
  "half new, half old" state; an uncertain op is reported, not guessed at.
- **Implementation.** Contract C7: no sidecar intent file. The intent is the op
  record plus the `Stage` copy — the op names the staged path and the generation,
  and its failure detail carries at least `phase`, `authority`, `artifact`.
  `Install` is the only irreversible step; `Retire` deletes the old copy
  afterwards. The new `rewrite` phase is registered in `transfer/resolve.rs`'s
  "holds nothing" list so it cannot leave a permanent `Unresolved`.

### 13.3 Lock self-healing

- **Goal.** A holder that dies (kill -9, or a host reboot) must not wedge the
  Store's exclusive lock forever.
- **Mechanism.** The lock releases on `Drop` in the normal path; a dead holder is
  detected from its liveness alone (pid + kernel start time + boot id): a lease
  whose process is not alive is stale. There is no expiry timer. After a reboot the boot
  id differs, so every lease from before it is stale. This is a liveness rule,
  not a durability promise (§11).
- **Safety.** No permanent wedge; a second compact is refused `Busy` / `InUse`
  instead of running concurrently; a crashed compact leaves a recoverable op that
  `doctor` can report.
- **Implementation.** Contract S2 and §2: exclusive access reuses the Store lock
  (`record.rs::lock`) + the lease (`lease.rs`, `store/<name>/open/<pid>`) + the op
  record (`op.rs`, `Standing = Running / Unresolved / Stale`). The lease's liveness
  is judged from pid + kernel start time + boot, so a live holder is never
  misjudged as dead.

### 13.4 Reachability closure

- **Goal.** Decide which objects are still reachable. The roots are many (project
  `main`, Branch heads, retained historical Commits, Layers, staged copies of
  incomplete ops, pooled dependencies), references are transitive (delta base ->
  earlier base -> ...), objects are shared across projects, and a producer may
  already be soft-deleted while its objects are still referenced.
- **Mechanism.** Walk the retained roots with the existing content-closure
  traversal, mark everything reached, and classify the rest as candidates subject
  to the pinned set; a pack none of whose objects was reached is not copied into
  the staged Store.
- **Safety.** Retained history never shrinks (I1: `Verify{heads}` plus closure
  comparison); shared content, transitive delta bases and pooled inodes stay alive
  even when the producer Branch/project was soft-deleted; a missing dependency
  aborts the round instead of being skipped (skipping is how a live object gets
  classified as dead).
- **Implementation.** Contracts C1/C2 and §6: the closure walks the retained roots
  with the existing `layerfs-content` `qualify` traversal (a second walker is
  forbidden) and `references`/`decode_references`; `collectable(o) := o ∉
  closure(Roots@G) ∧ o ∉ pinned`; the walk is bounded and keeps an indexed
  visited set; no second persistent database.
