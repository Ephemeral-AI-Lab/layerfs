# Implementation Checklist — GC Issue layerfs-project#41

> **Status:** Current planning checklist; no release candidate exists.

Acceptance criteria and dependencies per sub-issue. **Task-level detail is not
duplicated here**: it lives in each issue's body, with the contracts in
`02-operation-contracts.md` and the guidance in `C8b-N3-C9-Resolution-Plan.md`.
The authority for every criterion below is `LayerFS-GC-spec.md` (§ cited).

Legend: 🔲 not started · ⏳ blocked on a decision · ✅ done

## Per-issue acceptance

| Phase | Issue | Deliverable | Depends on | Status |
|---|---|---|---|---|
| 1 | **layerfs-project#42** | Contracts frozen, decisions answered | — | ⏳ |
| 2 | **layerfs-project#43** | Admission matrix enforced | layerfs-project#42 | 🔲 |
| 2 | **layerfs-project#48** | Reachability closure | layerfs-project#42 | 🔲 |
| 3 | **layerfs-project#44** | `branch.remove` | layerfs-project#42, layerfs-project#43, layerfs-project#48 | 🔲 |
| 3 | **layerfs-project#45** | `project.delete` | layerfs-project#42, layerfs-project#43, layerfs-project#48 | 🔲 |
| 3 | **layerfs-project#47** | `project.remove_file` | layerfs-project#42, layerfs-project#43, layerfs-project#48 | 🔲 |
| 4 | **layerfs-project#49** | SQLite compact | layerfs-project#42, layerfs-project#43, layerfs-project#48 | 🔲 |
| 4 | **layerfs-project#50** | PostgreSQL compact | layerfs-project#42, layerfs-project#43, layerfs-project#48 | 🔲 |
| 5 | **layerfs-project#51** | API / SDK / CLI | layerfs-project#42–#50 | 🔲 |
| 5 | **layerfs-project#52** | Acceptance suite | layerfs-project#42–#51 | 🔲 |

---

### layerfs-project#42 — Design finalization (§1.1, §1.2, §5, §6)

- [ ] Every settled conclusion (§1.1, S1–S11) confirmed  
      — S11 already decided (exclusive → R1 = E); N3 decided as an additive migration with no version step
- [x] Every open question (§1.2, Q1–Q6) answered or excluded; Q3 is out of this release; **S11 and Q6 are decided (2026-11-13): exclusive window → R1 = E with the unavailability measured under #52, and level 2 ships**; still open here: Q2 and the Q1/C3 confirmations;
      **Q6 is answered with §5.1 as one item** (see the next bullet)
- [ ] **R1** signed (asked on [#50](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/50))
- [ ] **Q1** signed (asked on [#44](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/44))
- [ ] **C3** signed (asked on [#42](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/42))
- [x] **Q6 + §5.1 resolved together (decided 2026-11-13 — level 2 ships)**: both levels are replacement copies on
      both backends (level 1 drops wholly dead packs, level 2 also repacks
      partially dead ones), so neither needs an in-place path; on PostgreSQL an
      in-place path (§5.1 reading (b)) exists only if the owner names one, which
      the C3 sign-off already argues against (`03-decision-record.md` §5.1, §5.1a, §5.3)
      — <https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/42#issuecomment-6818138560>
- [x] Contracts C1–C10 and C8b frozen; the split between shared and exclusive project tombstones stated (C6)
      — decided 2026-11-13: dedicated columns (`branch.removed_ms`, `layer_stack.deleted_ms`), `project.delete` exclusive, `project.remove_file` shared; the visibility-query list is a deliverable
- [ ] The "which history queries must filter on visibility" list enumerated (C6, C9)

### layerfs-project#43 — Admission enforcement (§5 C10, §2)

- [ ] The admission matrix is enforced on all four operations
- [ ] Mounts and sessions left in place → `MountsActive`; another user's operation in flight → `Busy`
- [ ] No implicit wait, no automatic cleanup, no retry
- [ ] The reader half: every session/transaction entry checks `(catalog_id, incarnation)` → `StaleStore` (§5 C8, R2)
- [ ] Tested on both backends

### layerfs-project#48 — Shared data reachability (§5 C1, C2, §6)

- [ ] Closure computed at the frozen generation `G = catalog_incarnation`, inside the exclusive window; no caching across operations; re-checked before install by `catalog_incarnation` + `Verify{heads}`
- [ ] Pinned set covers ops, staged copies, both Stores during install, unresolved artifacts
- [ ] Cross-project sharing and transitive delta bases survive a compact
- [ ] Catalog tables are always retained; a `pack` / `pack_unit` is dead only when none of its objects is retained; `content_signature` (a hint ring) is not a reclamation source
- [ ] The decision lands as a receipt (G, roots fingerprint, closure size, collectable count, dead packs), with estimates marked `estimate: true`
- [ ] Tested on both backends

### layerfs-project#44 — `branch.remove` (§5 C6, C9, C10)

- [ ] `branch.remove(name)` sets the terminal marker; only that Branch's mounts and sessions are excluded
- [ ] A same-name create is refused — **`NameRetired`**, not reused (Q1 option A)
- [ ] Other Branches in the Store stay readable and writable
- [ ] Immutable ids remain accessible after removal (C9)
- [ ] Tested on both backends

### layerfs-project#45 — `project.delete` (§5 C6, C10, §8)

- [ ] `project.delete(name)` sets the terminal marker on the project
- [ ] The Store stays addressable afterwards (a later `store.compact` must find it)
- [ ] Mounts and Sandboxes on the deleted project are refused
- [ ] The deleted project's name is terminal (C6)
- [ ] Schema migration for the visibility column is applied by the same change (N3)
- [ ] Tested on both backends

### layerfs-project#47 — `project.remove_file` (§3 I5, §5 C9, §9.3)

- [ ] The call is a visibility operation: history and ids are not rewritten
- [ ] History traversal hides the removed path; earlier Commits stay readable
- [ ] Reclaimed bytes are reported and match the receipt; a 0 is explained (Q2)
- [ ] Cross-project sharing is unaffected
- [ ] Tested on both backends

### layerfs-project#49 — SQLite compact (§7.1, §7.2, §5 C8b)

- [ ] Rebuild-and-swap reusing `transfer`, with only `rewrite` inserted: the staged copy is a fresh Store that receives only retained rows, so dead packs are not copied and no row is deleted in the live Store (the `pack` / `pack_unit` triggers never fire)
- [ ] `reorg` repacks live objects out of partially dead packs; object ids unchanged
- [ ] Writer fence: at every write-transaction start the Store path's `(dev, ino)` is compared with the open file's `fstat`; a pre-swap writer fails with `Conflict` instead of writing into the retired inode
- [ ] An interrupted compact is recoverable; the Store always opens and `doctor`s
- [ ] `physical_shrink` reported as the `page_count × page_size` delta, with dropped packs and their bytes separately (C5)
- [ ] Concurrent compact + write has no silent-loss path
- [ ] Non-native Stores are refused; the pull/compact/push workaround (two full transfers) is documented, and compact inside the machine's worker stays an open item (§7.2)

### layerfs-project#50 — PostgreSQL compact (§7.3, §5 C8, §5 C3)

- [ ] Store replacement per R1: **E** (two `ALTER SCHEMA RENAME` in one transaction) by default; **A'** only if S11 requires reads up
- [ ] `catalog_incarnation` bumped at install; the C8 reader fence refuses stale sessions
- [ ] Q6 + §5.1 resolved first (one item): under replacement+rename, level 1 is
      the replacement schema without dead packs and level 2 also repacks; the docs
      promise an in-place level only if one is adopted
- [ ] `pack` rows never updated or deleted (the trigger forbids it); the replacement schema receives only retained packs, and old data leaves with `DROP SCHEMA … CASCADE`
- [ ] `physical_shrink` reported as the `pg_relation_size` / `pg_total_relation_size` delta
- [ ] Remote servers supported (not `Unsupported`); network-partition recovery tested

### layerfs-project#51 — API / SDK / CLI (§8, §5 C9)

- [ ] The four calls take only the resource name; no `confirm` / `force` / `dryRun` / `stopSandboxes`
- [ ] Progress read from `store.status`; no new `operation.get`
- [ ] Refusals surface as the existing codes (`MountsActive`, `Busy`, `InUse`, `NameRetired`, `StaleStore`)
- [ ] No new `HistoryError` variant; GC methods default to `Unsupported` (C9)

### layerfs-project#52 — Acceptance (§9)

- [ ] `kill -9` crash injection per backend, per phase; every resume reaches a consistent state
- [ ] No-accidental-deletion property tests (§9.2)
- [ ] Contract tests (§9.3), including remove_file's byte accounting
- [ ] A stale reader gets `StaleStore`, and reads the new generation after reconnecting
- [ ] Physical shrink measured before/after on both backends

---

## Critical path

```
layerfs-project#42 (contracts + R1/Q1/C3 signed)
  ├─→ layerfs-project#43 (admission) ──┐
  ├─→ layerfs-project#48 (reachability)┴─→ layerfs-project#44, layerfs-project#45, layerfs-project#47 ─→ layerfs-project#49, layerfs-project#50 ─→ layerfs-project#51 ─→ layerfs-project#52
```

`layerfs-project#42` gates everything: an unanswered §1.2 question or an unsigned R1/Q1/C3
keeps the dependent issue blocked rather than half-built.

## Review & sign-off

| Phase | Reviewer | Status |
|---|---|---|
| layerfs-project#42 | Product + Architecture | ⏳ |
| layerfs-project#43 | Architecture | 🔲 |
| layerfs-project#44–#47 | Engineering Lead | 🔲 |
| layerfs-project#49–#50 | Database Specialist | 🔲 |
| layerfs-project#51 | API Lead | 🔲 |
| layerfs-project#52 | QA Lead | 🔲 |

---

**Base PR**: Ephemeral-AI-Lab/layerfs#658
