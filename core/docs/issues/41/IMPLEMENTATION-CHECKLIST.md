# Implementation Checklist — GC Issue #41

Acceptance criteria and dependencies per sub-issue. **Task-level detail is not
duplicated here**: it lives in each issue's body, with the contracts in
`02-operation-contracts.md` and the guidance in `C8b-N3-C9-Resolution-Plan.md`.
The authority for every criterion below is `LayerFS-GC-spec.md` (§ cited).

Legend: 🔲 not started · ⏳ blocked on a decision · ✅ done

## Per-issue acceptance

| Phase | Issue | Deliverable | Depends on | Status |
|---|---|---|---|---|
| 1 | **#42** | Contracts frozen, decisions answered | — | ⏳ |
| 2 | **#43** | Admission matrix enforced | #42 | 🔲 |
| 2 | **#48** | Reachability closure | #42 | 🔲 |
| 3 | **#44** | `branch.remove` | #42, #43, #48 | 🔲 |
| 3 | **#45** | `project.delete` | #42, #43, #48 | 🔲 |
| 3 | **#47** | `project.remove_file` | #42, #43, #48 | 🔲 |
| 4 | **#49** | SQLite compact | #42, #43, #48 | 🔲 |
| 4 | **#50** | PostgreSQL compact | #42, #43, #48 | 🔲 |
| 5 | **#51** | API / SDK / CLI | #42–#50 | 🔲 |
| 5 | **#52** | Acceptance suite | #42–#51 | 🔲 |

---

### #42 — Design finalization (§1.1, §1.2, §5, §6)

- [ ] Every settled conclusion (§1.1, S1–S11) confirmed
- [ ] Every open question (§1.2, Q1–Q6) answered; Q3 is out of this release;
      **Q6 is answered with §5.1 as one item** (see the next bullet)
- [ ] **R1** signed (asked on [#50](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/50))
- [ ] **Q1** signed (asked on [#44](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/44))
- [ ] **C3** signed (asked on [#42](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/42))
- [ ] **Q6 + §5.1 resolved together**: two commitment levels on SQLite; on
      PostgreSQL the cheap level exists only if an in-place path is kept, which the
      C3 sign-off already rules against (`03-decision-record.md` §5.1, §5.1a, §5.3)
      — <https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/42#issuecomment-6818138560>
- [ ] Contracts C1–C10 frozen; the split between shared and exclusive project tombstones stated (C6)
- [ ] The "which history queries must filter on visibility" list enumerated (C6, C9)

### #43 — Admission enforcement (§5 C10, §2)

- [ ] The admission matrix is enforced on all four operations
- [ ] Mounts and sessions left in place → `MountsActive`; another user's operation in flight → `Busy`
- [ ] No implicit wait, no automatic cleanup, no retry
- [ ] The reader half: every session/transaction entry checks `(catalog_id, incarnation)` → `StaleStore` (§5 C8, R2)
- [ ] Tested on both backends

### #48 — Shared data reachability (§5 C1, C2, §6)

- [ ] Closure computed at the frozen generation, inside the exclusive window; no caching across operations
- [ ] Pinned set covers ops, staged copies, both Stores during install, unresolved artifacts
- [ ] Cross-project sharing and transitive delta bases survive a compact
- [ ] `pack` and catalog tables are never candidates
- [ ] The decision lands as a receipt (G, roots fingerprint, closure size, collectable count), with estimates marked `estimate: true`
- [ ] Tested on both backends

### #44 — `branch.remove` (§5 C6, C9, C10)

- [ ] `branch.remove(name)` sets the terminal marker; only that Branch's mounts and sessions are excluded
- [ ] A same-name create is refused — **`NameRetired`**, not reused (Q1 option A)
- [ ] Other Branches in the Store stay readable and writable
- [ ] Immutable ids remain accessible after removal (C9)
- [ ] Tested on both backends

### #45 — `project.delete` (§5 C6, C10, §8)

- [ ] `project.delete(name)` sets the terminal marker on the project
- [ ] The Store stays addressable afterwards (a later `store.compact` must find it)
- [ ] Mounts and Sandboxes on the deleted project are refused
- [ ] The deleted project's name is terminal (C6)
- [ ] Schema migration for the visibility column is applied by the same change (N3)
- [ ] Tested on both backends

### #47 — `project.remove_file` (§3 I5, §5 C9, §9.3)

- [ ] The call is a visibility operation: history and ids are not rewritten
- [ ] History traversal hides the removed path; earlier Commits stay readable
- [ ] Reclaimed bytes are reported and match the receipt; a 0 is explained (Q2)
- [ ] Cross-project sharing is unaffected
- [ ] Tested on both backends

### #49 — SQLite compact (§7.1, §7.2, §5 C8b)

- [ ] Rebuild-and-swap reusing `transfer`, with only `rewrite` inserted
- [ ] Writer fence: the pre-swap writer fails with `Conflict` instead of writing into the retired inode
- [ ] An interrupted compact is recoverable; the Store always opens and `doctor`s
- [ ] `physical_shrink` reported as the `page_count × page_size` delta, with `reclaimed_pages` separately (C5)
- [ ] Concurrent compact + write has no silent-loss path
- [ ] Non-native Stores are refused

### #50 — PostgreSQL compact (§7.3, §5 C8, §5 C3)

- [ ] Store replacement per R1: **E** (two `ALTER SCHEMA RENAME` in one transaction) by default; **A'** only if S11 requires reads up
- [ ] `catalog_incarnation` bumped at install; the C8 reader fence refuses stale sessions
- [ ] Q6 + §5.1 resolved first (one item): replacement+rename, and therefore no
      cheap `compact(name)` level on PostgreSQL — say so in the docs rather than
      promising two levels
- [ ] `pack` rows never updated or deleted (the trigger forbids it); old data leaves with `DROP SCHEMA … CASCADE`
- [ ] `physical_shrink` reported as the `pg_relation_size` / `pg_total_relation_size` delta
- [ ] Remote servers supported (not `Unsupported`); network-partition recovery tested

### #51 — API / SDK / CLI (§8, §5 C9)

- [ ] The four calls take only the resource name; no `confirm` / `force` / `dryRun` / `stopSandboxes`
- [ ] Progress read from `store.status`; no new `operation.get`
- [ ] Refusals surface as the existing codes (`MountsActive`, `Busy`, `InUse`, `NameRetired`, `StaleStore`)
- [ ] No new `HistoryError` variant; GC methods default to `Unsupported` (C9)

### #52 — Acceptance (§9)

- [ ] `kill -9` crash injection per backend, per phase; every resume reaches a consistent state
- [ ] No-accidental-deletion property tests (§9.2)
- [ ] Contract tests (§9.3), including remove_file's byte accounting
- [ ] A stale reader gets `StaleStore`, and reads the new generation after reconnecting
- [ ] Physical shrink measured before/after on both backends

---

## Critical path

```
#42 (contracts + R1/Q1/C3 signed)
  ├─→ #43 (admission) ──┐
  ├─→ #48 (reachability)┴─→ #44, #45, #47 ─→ #49, #50 ─→ #51 ─→ #52
```

`#42` gates everything: an unanswered §1.2 question or an unsigned R1/Q1/C3
keeps the dependent issue blocked rather than half-built.

## Review & sign-off

| Phase | Reviewer | Status |
|---|---|---|
| #42 | Product + Architecture | ⏳ |
| #43 | Architecture | 🔲 |
| #44–#47 | Engineering Lead | 🔲 |
| #49–#50 | Database Specialist | 🔲 |
| #51 | API Lead | 🔲 |
| #52 | QA Lead | 🔲 |

---

**Base PR**: Ephemeral-AI-Lab/layerfs#658
