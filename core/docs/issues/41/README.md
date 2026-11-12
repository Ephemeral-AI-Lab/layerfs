# Issue #41: LayerFS Garbage Collection - Soft Removal & Store Compaction

**Status**: Design phase, implementation pending  
**Release Target**: LayerFS 2.0.0  
**Release Contract**: Ephemeral-AI-Lab/layerfs#511  
**Project Tracking**: Ephemeral-AI-Lab/layerfs-project#41

## Overview

Deliver explicit LayerFS garbage collection for SQLite and PostgreSQL through soft removal and explicit compaction:

- **`branch.remove(name)`** - Soft-remove a Branch
- **`project.delete(name)`** - Soft-delete a project  
- **`project.remove_file(...)`** - Logically remove a file/folder across history
- **`store.compact(name)`** - Physically reclaim discarded storage

## Core Design Principles

1. **Logical-First**: All remove/delete are logical; only `store.compact()` performs physical reclamation
2. **Exclusive Compaction**: `store.compact()` requires full Store isolation
3. **Cross-Project Safety**: All shared content and delta bases remain protected
4. **Immutable ID Protection**: Existing immutable IDs remain safe after file removal
5. **PostgreSQL Remote**: Full compaction support required (not a stub)
6. **Writer Fence**: SQLite incarnation uses atomic swap with writer exclusion

## Key Constraints

| Constraint | Description | Impact |
|-----------|-------------|--------|
| **C1** | Reachability = Store | GC scope boundary |
| **C2** | Pinned object protection | Cross-project safety |
| **C3** | Historical mount inclusion | Closure completeness |
| **C8b** | Incarnation protocol | SQLite atomicity |
| **C9** | HistoryCatalog binding | Immutable ID contract |
| **C10** | Admission matrix | Mount validation |

## Files in This Directory

- **README.md** - This file
- **LayerFS-GC-spec.md** - The authoritative single-file specification (§1-§12):
  decisions, contracts C1-C10, Mark, Sweep, operations, acceptance. Every other
  document in this directory derives from it and cites its section numbers.
- **01-overview-and-decisions.md** - Design decisions (D1-D14)
- **02-operation-contracts.md** - Operation specifications
- **03-decision-record.md** - R1 / Q1 / C3: scenarios, alternatives, decision basis;
  §5 lists the open items the decisions exposed (PostgreSQL rebuild-and-swap vs
  in-place row deletion, and C3's scope)
- **C8b-N3-C9-Resolution-Plan.md** - Deep dive on three critical issues
- **RESOLUTION-SUMMARY.md** - Quick reference for key decisions
- **Full spec**: See pull request tokenaissance/layerfs#70

## Implementation Phases

### Phase 1: Design Finalization (#42)
- Confirm all 11 committed decisions
- Product sign-off on 6 pending decisions

### Phase 2: Infrastructure (#43, #48)
- Admission enforcement
- Shared data reachability

### Phase 3: Soft Removal (#44, #45, #47)
- Branch removal
- Project deletion  
- File removal

### Phase 4: Physical Compaction (#49, #50)
- SQLite compaction
- PostgreSQL compaction

### Phase 5: Integration (#51, #52)
- API/SDK/CLI exposure
- Comprehensive testing

## Admission & Mount Policy

| Operation | No mounts within | Other restrictions | What may remain |
|-----------|------------------|-------------------|-----------------|
| `branch.remove(name)` | Selected Branch | Existing rules | Other Branches |
| `project.delete(name)` | Project + Branches | Close Sandboxes | Other projects |
| `project.remove_file(...)` | Project + Branches | Close Sandboxes | Other projects |
| `store.compact(name)` | Store + all projects | Exclusive access | Other Stores |

## Decision ownership

Where each decision or reconciliation entry is realised. The specification
sections are §-numbered as in the GC spec; `R`/`N`/`Q` ids are that spec's
reconciliation and net-new tables.

| Item | What it decides | Contract / record | Implemented in | Verified in |
|---|---|---|---|---|
| **R1** | PostgreSQL Store replacement: E (two `ALTER SCHEMA RENAME`) by default, A' (pointer + versioned schema) if S11 requires reads up | #42 (§1.3 R1, §7.3, S11), 03-decision-record §1 | **#50** | #52 |
| **R2** (= C8, replaces the `compact_intent` row) | Reader fence: `catalog_incarnation += 1` at install; every session/transaction entry checks `(catalog_id, incarnation)` → `StaleStore`; SQLite also `(dev, ino)`; `lfq1` across a generation → `StaleStore` | #42 (C8, generation arithmetic; no second generation field) | **#43** (the session-entry half; shared by both backends), `(dev, ino)` in #49 | #52 |
| **Q1** (= C6) | Branch name reuse: none; terminal tombstone, `NameRetired` | #42 (Q1, C6), 03-decision-record §2 | **#44** | #52 |
| **C3** | Per-object delete binds `(ObjectId, G)` | #42 (C3), 03-decision-record §3 | **#49**, **#50** (consumers of the signature) | #52 |
| **N3** | Soft delete visibility: `removed_ms` / `deleted_ms`, partial unique indexes | #42 (schema change) | **#44** (`removed_ms`), **#45** (`deleted_ms`, migration) | #52 |
| **C8b** | SQLite writer fence: incarnation check at admission plus install re-read | #42 (C8b) | **#49** | #52 |
| **C9** | HistoryCatalog stays extensible: GC methods behind a default `Unsupported`, no new `HistoryError` variants | #42 (C9) | **#51** (trait), #49/#50 (overrides) | #52 |
| **C10** | Admission matrix: which mounts each operation excludes | #42 (C10) | **#43** | #52 |
| **Q3** | Rewrite step: transfer phases with one inserted `rewrite` | #42 (§7.1) | **#49** | #52 |
| **Q5** | PostgreSQL reader behaviour | Superseded by C8/R2 — no separate decision | **#43** | #52 |
| **D3** | PostgreSQL fence mechanism (advisory lock / `FOR UPDATE` / schema lock) | Pending product sign-off (01-overview) | **#50** | #52 |

### Sign-off requests

The three reconciliation decisions are asked for approval in the issue that
realises each one, with a short reasoning summary and a checkbox block:

| Decision | Recommended default | Asked in | Comment |
|---|---|---|
| R1 | **E** (exclusive window) | #50 (PostgreSQL GC) | <https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/50#issuecomment-6818137198> |
| Q1 | **A** (no reuse) | #44 (Branch removal) | <https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/44#issuecomment-6818137920> |
| C3 | **`delete(id, G)`** | #42 (contracts) | <https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/42#issuecomment-6818138560> |

R1 is independent. **Q1 and C3 are read together** — same failure family, opposite
mechanisms (`03-decision-record.md` §3.6) — and each request links to the other,
so a reviewer signs them knowing both.

When an answer arrives, record it here and in `03-decision-record.md`; an
unanswered decision keeps its implementation issue blocked.

R2 is not a separate sub-issue: the spec folds it into C8 and assigns C8 to
admission (#43). Its install-time bump is executed by whichever backend installs
(#49 on SQLite, #50 on PostgreSQL), and its reader-facing refusal is what #52
tests across a generation boundary.

## Related Resources

- **Specification PR**: Ephemeral-AI-Lab/layerfs#658 (`core/docs/issues/41/`)
- **Release Contract**: Ephemeral-AI-Lab/layerfs#511
- **Architecture Diagrams**: https://app.excalidraw.com/s/9Zq6XgRe98z/
- **Sub-Issues**: #42-#52

## References

- See the complete specification in Ephemeral-AI-Lab/layerfs#658
- Three critical issues (C8b, N3, C9) explained in C8b-N3-C9-Resolution-Plan.md
- Design decisions summary in RESOLUTION-SUMMARY.md

---

**Last Updated**: 2026-11-12
