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
| S6 | A SQLite Store that is not **native** compacts **where it lives**: `place::plan` already routes `StoreRef::Sqlite{machine}` to that machine's worker, exactly as `store.push` / `pull` / `remove` do, so compact needs **no transfer**. No `Unsupported` refusal (decided 2026-11-13, layerfs-project#42 item 5) | `control/crates/layerfs-store/src/place.rs`, `sqlite/transfer/phases.rs`; the new design item is cross-machine interruption — staged copy on the machine, op record in the caller's home | §7.2 |
| S7 | Three reporting definitions + peak temporary space + residue; estimates must be marked `estimate`; capped results must report the cap | layerfs-project#41's three definitions + `operator/files.rs` "never silently reports a total" | C5 |
| S8 | Reader fence = **session-level `(catalog_id, incarnation)` check → `StaleStore`** | incarnation is checked only at open; under WAL no lock blocks a reader | C8 |
| S9 | install is binary, `heads` must match before and after | transfer's `Verify{heads}`; I1 | I1, I3 |
| S10 | Logical deletion does not rewrite history or change IDs | CommitId is content-derived; layerfs-project#41's "immutable IDs must not silently resolve" | I5 |
| S11 | Read availability during compact = **exclusive by default** (reads unavailable); if a "reads available" SLO is required (p99 ≤ 1.2× baseline), PG takes A' (pointer + versioned schema) | Decides the window, not the swap mechanism: inside the exclusive window PostgreSQL swaps **three tables** (2026-11-13, item 1 revised) | §1.3 R1, §7.3 |

### 1.2 Still open for layerfs-project#42 (with recommendation + cost + cost of choosing wrong)

| # | Question | Recommendation (preferred option) | Effort/Cost | Cost of choosing wrong |
|---|---|---|---|---|
| Q1 | Name reuse: forbid reuse (tombstone), or derive with incarnation | **A: forbid reuse** + terminal tombstone; **two codes** (decided 2026-11-13): `NameRetired` when a retired row holds the name, `NameTaken` when a live one does, so a create never surfaces as `Integrity`; zero migration | Small: one terminal marker + the two checks at create; no incarnation column | Reusing an old name shares a `BranchId` with old history (`blake3(stack‖name)`), so old Commits' Branch attribution goes wrong |
| Q2 | How much `remove_file` can actually reclaim | **0, by construction** (decided 2026-11-13, layerfs-project#42 item 3): I5 makes it a **visibility operation**, C6 records it as a *shared* tombstone, and the closure walks retained roots — history among them — so the objects stay reachable and no pack can become dead at either level. Measuring is kept only to publish the number and to catch a regression that drops references | Small: one regression case + docs; no mechanism change | Users plan capacity on "deleting saves space", and it does not |
| Q3 | How to set an age threshold when online collection is introduced | **Out of this release (decided 2026-11-13, layerfs-project#42 item 11).** 2.0 deletes nothing on a live Store and needs no age guard: the closure is computed over the frozen generation inside an exclusive window, so there are no in-flight writers to protect against. Not in this spec; when the time comes, add timestamp columns first, then measure the longest Commit / longest in-flight handle | Medium: add timestamp columns + migration + threshold contract | Without timestamps an age threshold cannot be implemented; forcing it deletes in-flight writes |
| Q4 | Retention cap for interrupted copies | **Decided 2026-11-13 (layerfs-project#42 item 4): cap = 1, and it is enforced by refusal, not by cleanup.** While an unjudgeable leftover exists, `store.compact` is refused **`OrphanArtifact`** naming `phase`, `artifact` and both paths. `check` retires only what it can *judge* (`transfer/resolve.rs::leftover`); an `Unresolved` leftover is cleared only by record authority (`doctor` + runbook), never by compact and never by a time window. `doctor` enumerates the machine side through the worker, and an unreachable machine reports **`unknown`, never `clean`** | Small: `doctor` report + runbook + one new code | Residue accumulates and consumes disk (and pins objects, distorting byte accounting); automatic cleanup violates "no guessed cleanup" |
| Q5 | Reader behavior when PG swaps tables | **Superseded by C8/R2**: readers go through the `(catalog_id, incarnation)` session check → `StaleStore`, and read the new generation after reconnecting; table names are not versioned | Small: session/transaction entry check (shared with SQLite) | Cross-generation reads (no crash, but wrong data) |
| Q6 | Does `store.compact` promise "drop dead packs" or "reorganize data" | **One call, `store.compact(name)`, choosing its own path (decided 2026-11-13, layerfs-project#42 item 2, per #155):** nothing collectable → no copy; wholly dead packs below **10% of the bytes a rebuild would replace** → the light path (locator rows only, live Store); otherwise the rebuild. Wholly dead packs are dropped by not copying them, and partial-pack repacking is **reported, not performed**, in this release | Medium: one entry point, one receipt with a `path` field and the three byte definitions | Level 1 alone leaves dead objects sharing a pack with a live one; promising "all discarded bytes are reclaimed" without repacking over-promises, so the receipt must say what was kept |

> **Q6 is answered together with §5.1** of `03-decision-record.md`, and revised on
> 2026-11-13: there is **one call**, and it picks among three paths — no copy, the
> light path (locator rows only), or the rebuild. Nothing repacks, so the earlier
> "level 1 / level 2" split no longer exists, and §5.1 reading (b) (a separate
> in-place entry point) stays unadopted.

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
| Bounded traversal | `engine/crates/layerfs-persistence/src/diagnostic/scan.rs` (the reference walk) | Mark must **not** reuse `qualify`: it skips file mappings, symlink targets and attribute trees and ignores delta bases (`qualify/walk.rs:38`); `FinalizedObject::admit(..).references()` + `ChainBases::base_of` is the walk `diagnostic/scan.rs` already does |
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
| N3 | Representation of logical deletion (`branch`/`layer_stack` have no visibility column) | Schema change, belongs to layerfs-project#42 — **decided 2026-11-13: an additive migration under the same schema identity (no version step), with 1.0 Stores converted offline** |
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
- The unit that frees bytes is the pack: payload lives in `pack.body` and, in the units layout, `pack_unit.body`. A pack is **retained** when at least one object it holds (`object_location.pack_id`) is not collectable; a pack with no such object is **dead** and is not copied into the staged Store. Live objects of partially dead packs are **not** repacked in this release; the receipt reports those bytes as `keptWaste`. Catalog tables such as `history_meta`/`store_policy`/`scope_allocator` are always copied.
- Published `pack` / `pack_unit` rows are never updated or deleted in the live Store (I2); the staged Store is fresh, so their triggers never fire.
- `content_signature` is an 8192-slot ring of dedupe hints that reference `object_location`. It is not a reclamation source: entries naming an object that is not copied are cleared in the staged Store.
- The empty root set is legal (after the sole project is soft-deleted): the closure is empty and all objects are collectable, but compact must still produce a Store that can be opened and `doctor`ed.
- `pinned` contains at least: objects referenced by ops and staged copies, both Stores during install, artifacts under an `Unresolved` op name.
- The decision must land as a receipt (G, Roots fingerprint, closure size, collectable count, the three definitions).

### C3 Death decision bound to `(ObjectId, generation)`
For per-object/per-row deletion (PostgreSQL, future online GC), the delete intent must be bound to
`(ObjectId, G)`, where `G` is `catalog_incarnation` at the time the intent was formed; ABA (same bytes
written again after a delete) must not apply an old intent. Whole-file replacement in SQLite is covered
by C1's re-check.

**Scope, decided 2026-11-13 (layerfs-project#42 item 9, revised with the light path).** In this release
the only per-row deletion is inside `store.compact`'s **light path** (§7.2, §7.3), and C3 is satisfied
there **without a separate intent path**: the intent is the compact op record (which names `G`, C7 — no
sidecar), the deletions run inside the exclusive window, and the delete transaction re-checks
`catalog_incarnation == G` before it commits. So C3 stays a **forward-looking** contract, for an online
sweep that would delete rows *outside* such a window — not a 2.0.0 deliverable.

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

- `logical_discarded` and `physical_shrink` differ by design: a collectable object in a retained **pack is not freed**, because repacking is reported rather than performed (it is the receipt's `keptWaste`).

- Available sources: a `commit` row already carries `logical`/`new_canonical`/`new_stored`/`new_metadata`/`created_ms`.
- Must also report: peak temporary space, elapsed time, residue (enumerable).
- Mark estimates `estimate: true`; a diagnostic with a cap must report the cap too.

### C6 Name reuse and terminal state (Q1)
- `BranchId::named = blake3(stack ‖ name)`, and the comment relies on "names are never reused" (`core/crates/layerfs-history/src/contract/identity.rs:266`); once `branch.remove` lands, that premise fails.
- Recommended A: forbid reuse + terminal tombstone; a same-name create returns `NameRetired` when the holder is retired and `NameTaken` when it is live; zero migration. Alternative B: incarnation participates in the derivation, requiring migration.
- `branch`/`layer_stack`/`layer` have no visibility column (`persistence/sql/sqlite/history.sql`); a **marker table** keeps that dimension in one place and leaves the history rows untouched, at the cost of an anti-join in every query that lists or resolves a Branch or project — and that list is the deliverable (see the decision bullet below) — **the column was chosen (2026-11-13), so the list of queries that must filter on visibility is a deliverable of layerfs-project#42**. layerfs-project#42 must list "which queries must filter".
- The split between shared / exclusive project tombstones is decided by layerfs-project#42; this spec requires only "terminal state + decidable generation".
- **Decided (owner, 2026-11-13, layerfs-project#42 item 7; storage revised to #155's shape):** the tombstone lives in **new marker tables**, added at open the way `add_file_digests` was (an additive migration under the same schema version), so `branch` / `layer_stack` stay append-only and no row is rewritten. `project.delete` is **exclusive** while `project.remove_file` is **shared**. "Exclusive" is not "deleted now" — it only removes that history from the root set at mark time, so reclamation still happens only in an explicit `store.compact`.

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
  closure = walk(Roots@G)        // FinalizedObject::admit().references() + ChainBases::base_of; base-only then full
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
           - judgeable leftover (the record names it and its state can be decided): retire it here
           - unjudgeable leftover (op `Unresolved`): refuse `OrphanArtifact`; only doctor + runbook clear it
           - a machine that cannot be reached reports `unknown`, never `clean`
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

### 7.1a The path set, at a glance

`store.compact(name)` picks one of **three** paths, and the receipt's `path` field is that closed set
(`none | light | rebuild`) — so the API and the acceptance tests can enumerate it.

| path | when | SQLite | PostgreSQL | frees |
|---|---|---|---|---|
| `none` | nothing collectable | nothing; zero report | same | 0 |
| `light` | wholly dead packs < **10%** of the bytes a rebuild would replace | checkpoint WAL → delete the four row families → `incremental_vacuum` **only under `auto_vacuum=2`** | delete the same rows → `COMMIT` → plain `VACUUM` | locator bytes and, conditionally, pages; **payload 0** |
| `rebuild` | ≥ 10% | fresh file, retained rows only, `rename` into place | three-table swap | the dead packs' payload |

The three share one `mark` (closure + `pinned`) and one receipt shape; `light` and `rebuild` differ only
in applying the retained predicate as `DELETE` or as "do not copy".

**Not paths, and not to be re-opened as paths:** `reorg`/level-2 repacking (reported as `keptWaste`,
never performed — v14 groups cannot move byte-for-byte into v17+ packs); in-place row deletion
(§5.1 reading (b), needs the pack immutability triggers disabled); `reclaim_space` / `VACUUM FULL`
(a rebuild wearing another name, and `reclaim_space` is not part of compact); and an online sweep
(跨窗口 → C3's full contract, a forward-looking item). A' is a different **swap mechanism**, not a
fourth path.

### 7.1b Light path — the general rule

**Trigger.** Wholly dead packs are below **10% of the bytes a rebuild would replace** — SQLite: the
file's `page_count × page_size`; PostgreSQL: `pg_total_relation_size` of `pack` + `object_location` +
`content_signature`. The numerator is the bytes of wholly dead packs (`pack.body` / `pack_unit.body`);
10% is a first value to revisit after the first measurement.

**Rule.** The light path applies the **same retained predicate as the rebuild**, with `DELETE` in place
of "do not copy", and **never touches a `pack` row** (I2 and the immutability triggers stand):

| Table | Light path |
|---|---|
| `object_location` | delete rows of collectable objects (outside the closure and `pinned`) |
| `content_signature` | delete entries naming a deleted object (the rebuild's cleanup rule) |
| `file_digest` | delete rows whose root is not retained (the rebuild's correctness rule) |
| `inode_value_block` | delete a block only when **every** ordinal in it is dead |
| `pack` / `pack_unit` | **nothing** — so by construction the light path frees no payload bytes |
| history / catalog tables | untouched |

**No install, no generation bump.** It deletes only unreachable rows, so the Store's semantics do not
change: `catalog_incarnation` stays at `G`, the receipt reports `generation.before == generation.after`,
and readers are **not** sent `StaleStore`. Because it does delete rows it is still covered by C3 — and
C3 is satisfied locally (op record names `G`, exclusive window, re-check before commit).

### 7.2 SQLite (layerfs-project#49)
0. **Run where the Store lives — no refusal.** `place::plan` routes a non-`native` SQLite Store (`Placement::Machine(m)` / `Unknown`) to that machine's worker, the route `store.push` / `pull` / `remove` already take, so compact copies and renames in place and transfers nothing. The earlier rule (refuse with `Unsupported`, then "`store.pull` home, compact, `store.push` back") is **superseded** (decided 2026-11-13, layerfs-project#42 item 5): it charged the user two full transfers for something the architecture already does in one place. What this costs: **cross-machine interruption** — the op record lives in the caller's home while the staged copy lives on the machine, so `Unresolved` and `doctor` must observe both ends; `Capacity` comes from the machine's disk; and the C8b `(dev, ino)` check is only meaningful inside the worker, since the caller sees a different inode space.
1. Pass the old Store through `doctor`/`diagnose` first, then enter `check`.
2. `rewrite` creates the staged copy as a **fresh Store** (same schema script) and inserts only retained rows: retained `pack` / `pack_unit` rows, the `object_location` rows of live objects, live `inode_value_block` rows, `content_signature` entries that still name a copied object, and the catalog/history tables; it then recomputes totals. Dead packs are not copied, so nothing is deleted and the immutability triggers never fire. Encodings and object IDs are preserved. Partial-pack repacking is **reported, not performed** in this release (Q6 decided: one call, internal path; `keptWaste` in the receipt), so `rewrite` never rewrites a live object's locator.
3. Verification has two layers: `sqlite_structure(path, …)` (any path, read-only) → once it can be opened as a Store, `SqliteHandles::diagnose(options, …)`. The CLI's `doctor store integrity <store>` only accepts a registered Store and **cannot** be used on a copy; a copy goes through `control/.../diagnostic.rs::sqlite(path, identity, …)`.
4. On top of diagnostic, add only two assertions: closure consistency, and that the copy's object, pack and row counts equal the retained counts recorded by mark.
5. install reuses `StoreStep::Install`, checking `(catalog_id, catalog_incarnation, dev, ino)`. WAL files are named after the path, not the inode: before the rename, every connection on the old file is checkpointed and closed and no `-wal`/`-shm` content remains beside it (transfer's `steps.rs` already checkpoints a leftover `-wal`), and the staged Store is installed fully checkpointed, so no opener of the new file can replay the old file's log. layerfs-project#49 confirms this in its crash-injection cases.
6. The old copy is deleted by `StoreStep::Retire`.
7. Insufficient disk reports `Capacity`; on failure keep the op and the staged copy, and do not clean up on your own.
8. **Insertion order is decided by the FK graph**: `pack_unit.pack_id → pack.pack_id`, `object_location.pack_id → pack.pack_id`, `content_signature.object_id → object_location.object_id` (`sql/sqlite/objects*.sql`, `sql/sqlite/metadata.sql`), constraints IMMEDIATE, `foreign_keys=ON`. Insert `pack` → `pack_unit` → `object_location` → `content_signature` into the staged Store. In-place deletion in the live Store stays ruled out (`03-decision-record.md` §5.1a).
9. `reclaim_space` is not part of compact: the fresh staged Store has no free pages to return. It only reports the old file's `backend_reusable`.

**Light path, SQLite specifics.**

1. Checkpoint the old WAL first (`checkpoint_leftover_wal`); pages in the WAL cannot be returned.
2. One write transaction: the deletions above, then re-check `catalog_incarnation == G`.
3. `PRAGMA incremental_vacuum` — **only when `auto_vacuum = 2`** (`lifecycle/reclamation.rs` already
   requires it), in its page batches, until the freelist is empty or a cap is reached.
4. When `auto_vacuum ≠ 2`: no vacuum; report `physical_shrink = 0` and `backend_reusable` as the
   freelist bytes, saying in the receipt that `auto_vacuum=2` is what would return them to the OS.


### 7.3 PostgreSQL (layerfs-project#50)
- **Scope**: remote PostgreSQL Stores are in scope (including remote servers); out of scope are remote SQLite and remote object-storage copies.
- A Store is one schema; `{s}` is replaced by `self.schema` at `backend/postgresql/mechanics.rs:21`, and `self.schema` comes from `schema_of` (`open.rs:37`), which **requires** the schema on every call: `schema_of(None)` refuses `InvalidInput("postgresql.schema is required")` — there is no `layerfs` default.
- **The swap method is decided by S11**:
  - **Three-table swap (2026-11-13, item 1 revised; replaces the whole-schema rename)**:
    one transaction, inside the exclusive window —
    `LOCK TABLE pack, object_location, content_signature IN ACCESS EXCLUSIVE MODE NOWAIT`;
    re-check `catalog_id`, `incarnation == G` and the heads;
    `INSERT … SELECT` the live rows into scratch tables in a **short-named** schema (`c<8hex>_<n>`);
    rebuild the pack foreign key and the prefix structures;
    `DROP TABLE` the three old tables; `ALTER TABLE scratch.… SET SCHEMA <store>`;
    `catalog_incarnation += 1`; recompute `store_totals`; `COMMIT`; then drop the empty scratch schema.
    Why not the whole-schema rename: copying `commit`/`layer` re-fires `add_logical`
    (`schema.sql:182-185`) and double-counts `logical`; copying `init_operation` needs
    `OVERRIDING SYSTEM VALUE` and reissues operation ids (`acquisition.sql:2`); new objects
    have new OIDs, so every GRANT is lost; and `schema + "_c_<n>_old"` can exceed PostgreSQL's
    63-byte identifier limit. Only `object_location.pack_id` references `pack`, so three tables
    cover all payload and history/acquisition are never touched.
  - **A' (read-available window)**: data lives in the versioned schema `layerfs_c_<n>`; the fixed schema holds only a pointer row; `self.schema` is resolved from the pointer at open; swap = one UPDATE of the pointer row, then `DROP SCHEMA` of the old generation. "Versioned table names" (A) is not adopted, because it would add a second placeholder to every SQL statement.
  - **B (alternative)**: paired `ALTER TABLE RENAME` in the same transaction (ACCESS EXCLUSIVE per table, `lock_timeout` must be verified).
  - **C, view indirection**: writability needs triggers; the last alternative.
- Whichever is chosen, the reader fence (C8/R2) is required.
- `pack` has immutability triggers on the PG side too, so deleting rows of the old table is forbidden; reclamation is "new schema + new triggers": the replacement schema receives only retained packs and rows, exactly as on SQLite (§7.2 step 2), and dead packs leave with `DROP SCHEMA … CASCADE`.
> **[CLOSED 2026-11-13 — owner chose (a), then revised the mechanism]** Reading (b), an
> additional in-place path with its own deletion order ("references first, payload second"),
> was **not** adopted. Reading (a) is now the **three-table swap** above: live rows are
> inserted into scratch tables and the old tables are dropped in the same transaction, so
> **no object row is ever deleted one at a time** and the `backend_reusable` intermediate
> state does not arise. Consequence for
> layerfs-project#42 item 9: C3 (`delete(id, G)`) has **no caller in 2.0.0** and is a
> forward-looking contract for a future online sweep (`03-decision-record.md` §5.1).
- Reader fence: the lane blocks only writers; readers uniformly go through C8's `(catalog_id, incarnation)` session check → `StaleStore` (Q5 is superseded by C8/R2 and is no longer chosen separately).
- Reclamation metric: the delta of `pg_relation_size` / `pg_total_relation_size`; `autovacuum` stays enabled but is not counted.

**Light path, PostgreSQL specifics.**

1. One transaction, inside the exclusive window and the advisory lock: re-check the incarnation, run the
   deletions. `pack` is never locked exclusively for this path.
2. `COMMIT`, then a **plain `VACUUM`** (never `VACUUM FULL`, which is a rebuild with a different name and
   needs `ACCESS EXCLUSIVE`) on the three tables: dead tuples and dead index entries go, pages become
   reusable. A failed or skipped `VACUUM` does not fail the call; the receipt records
   `vacuum: { ran, reason }`.
3. `physical_shrink = 0` **by definition** here, with the reason "dead tuples await autovacuum";
   `backend_reusable` carries the `n_dead_tup` estimate.


### 7.4 Leftover recovery
- Every `check` runs `resolve` first, and judges by the op's phase whether that phase holds anything.
- When it cannot be judged, report `Unresolved`; automatic install or automatic cleanup is forbidden.
- An orphan file with no op reports `OrphanArtifact` and is not deleted.

---

### 7.5 Code-level corrections from the #155 source review

Each is a place where this spec did not match the tree. Adopting them changes no contract.

| # | Spec said | Code requires |
|---|---|---|
| 1 | mark reuses `qualify` | `FinalizedObject::admit(..).references()` + `ChainBases::base_of` (see §6) |
| 2 | delta bases not mentioned | bases live inside pack records (`encoding/delta/record.rs:9-11`); tree pages, inode leaves and chunks are deltas, so the closure follows base edges with two visit levels |
| 3 | `file_digest` not mentioned | keep only rows whose root is retained — a Save reuses a recorded root without checking it exists, so an orphan row later fails `MissingDependency` |
| 4 | retained roots by `commit.branch_id` | it can be NULL, and a removed Branch's Commits stay live through `layer.source_commit_id`: use a recursive history closure |
| 5 | compact is a `rewrite` phase of the transfer pipeline | transfer's `Install` refuses an existing Store file and `Retire` deletes it: compact is its own module, reusing only `Snapshot`'s idle/WAL check |
| 6 | build the file with `journal_mode=OFF` | `SQLITE_DBCONFIG_DEFENSIVE` (`session/connection.rs:220`) forbids it: one rollback-journal transaction, file built from `schema::scripts` |
| 7 | copy `store_totals` | copying `commit`/`layer` re-fires `add_logical` (`history.sql:162-167`): do not copy `logical`; let the triggers rebuild it and assert equality; recompute `canonical`/`stored`/`metadata` |
| 8 | drop dead `pack_unit` rows | the reader needs a complete contiguous unit set (`objects/units_read.rs:55-75`): never leave a dangling unit |
| 9 | finished init/merge scratch rows could be cleared | leftovers mean unfinished work: refuse `Busy` while they exist; clear through the existing recovery path |
| 10 | the entry check runs on every transaction | autocommit `Immutable`/`Statement` have no `BEGIN`: the shared advisory lock and the incarnation read are two statements in one simple-query message (#42 item 12b) |

## 8. Operations surface

- Method: `store.compact(name)` registers an op, and progress is read from `store.status`; `operation.get` must not be added. **It takes no `PROGRESS` option** (unlike `store.push`/`store.pull`), for the same reason it takes no other option: the four operations accept only a resource identifier (#155 §9.3, settled 2026-11-13).
- All four operations take only a resource identifier: `branch.remove(name)`, `project.delete(name)`, `store.compact(name)`, `project.remove_file(project, path)`, and they jointly forbid `confirm`/`force`/`dryRun`/`stopSandboxes`/Store or volume options (layerfs-project#41 verbatim). Implementations do no implicit unmount, no waiting, no retry; an unclean environment returns `MountsActive`/`Busy`/`InUse`.
- Backend route: `backend.store.compact` (following the `backend.store.*` naming family); CLI `store compact`.
- Rejection codes: reuse `Busy`/`InUse`/`MountsActive`/`NoSuchStore`/`Unsupported`/`NotFound`; add `StaleStore`/`NameTaken` (a live row already holds the name)/`NameRetired` (the name belongs to a retired row)/`OrphanArtifact` (a leftover staged copy/schema that no record can judge — Q4)/`Capacity` (cf. `StaleSandbox`). `Unresolved`/`Stale` are op states, not return codes.
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
| first open after install | The generation bump takes effect; a session that was opened before the install **outside admission** receives `StaleStore` |

### 9.2 No-accidental-deletion property tests
- Random DAG: `fork / commit / merge / publish / branch.remove / project.delete`.
- Assertion: not one object in the closure of a live root is deleted; closure sets and sizes are compared one by one before and after.
- ABA variant: after deleting candidate `A`, immediately write back the same bytes; the old intent must not take effect.

### 9.3 Contract tests
- Deletion decisions depend only on closure and pinned: assert the implementation reads no time column.
- A second concurrent compact returns `Busy`, and produces no second op or second staged copy.
- Empty root set: after deleting the only project, compact still produces a Store that can be opened, `doctor`ed, and addressed.
- After `remove_file`, the bytes reclaimed agree with the receipt's explanation (0 must be explained).
- A connection that **bypassed admission** (a direct `psql`, a leftover process) receives `StaleStore` after the swap; reconnecting reads the complete new generation. Through the API there is no reader to observe, because C10 excludes sessions and reads for the whole operation.
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

### 13.2 Two-phase replacement (record, then confirm)

- **Goal.** A crash in the middle of a compaction must never leave a half-old,
  half-new Store, and the system must be able to tell "replacement intended but not
  confirmed" from "replacement happened".
- **Mechanism.** Record the intent first (the op record names the staged copy and
  the generation), build the staged copy by **copying only retained rows** — under
  the replacement path there is **no delete step at all** (§7.2 step 2: "nothing is
  deleted and the immutability triggers never fire") — then confirm with a single
  `install`; the old copy is retired afterwards. C3's `delete(id, G)` would apply
  only to a path that actually deletes objects, which 2.0.0 does not have.
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
  with `FinalizedObject::admit(..).references()` plus `ChainBases::base_of`, the same
  walk `diagnostic/scan.rs` performs — **not `qualify`**, which skips file mappings,
  symlink targets and attribute trees and ignores delta bases. A new *walker* is still
  forbidden; this is the existing reference walk. Delta-base edges are part of the
  closure, and an object seen only as a base is upgraded if later reached fully; `collectable(o) := o ∉
  closure(Roots@G) ∧ o ∉ pinned`; the walk is bounded and keeps an indexed
  visited set; no second persistent database.
