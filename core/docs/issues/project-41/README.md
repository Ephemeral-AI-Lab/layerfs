# layerfs-project#41: LayerFS Garbage Collection - Soft Removal & Store Compaction

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Design phase, implementation pending. Issue numbers written `layerfs-project#N`
refer to [layerfs-project](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues);
this repository's own #41 is an unrelated issue.

**Release Target**: LayerFS 2.0.0  
**Release Contract**: Ephemeral-AI-Lab/layerfs#511  
**Project Tracking**: Ephemeral-AI-Lab/layerfs-project#41

## Overview

Deliver explicit LayerFS garbage collection for SQLite and PostgreSQL through soft removal and explicit compaction:

- **`branch.remove(name)`** - Soft-remove a Branch
- **`project.delete(name)`** - Soft-delete a project  
- **`project.remove_file(...)`** - Logically remove a file/folder across history
- **`store.compact(name)`** - Physically reclaim discarded storage: build a fresh
  Store that receives only retained rows, so wholly dead packs are not copied;
  `reorg` additionally repacks live objects out of partially dead packs
  (`LayerFS-GC-spec.md` §1.2 Q6, §7.1, §7.2)

## Key Constraints

The constraints are defined once, as contracts **C1–C10** in
`LayerFS-GC-spec.md` §5. This README does not restate them.

## Files in This Directory

- **README.md** - This file
- **LayerFS-GC-spec.md** - The authoritative single-file specification (§1-§13):
  decisions, contracts C1-C10, Mark, Sweep, operations, acceptance. Every other
  document in this directory derives from it and cites its section numbers.
- **02-operation-contracts.md** - Operation specifications
- **03-decision-record.md** - R1 / Q1 / C3: scenarios, alternatives, decision basis;
  §5 lists the open items the decisions exposed (PostgreSQL rebuild-and-swap vs
  in-place row deletion, and C3's scope; §5.3 records that the C3 sign-off already
  constrains the answer to a confirm-or-reopen), including §5.1a: the restored argument
  for why in-place deletion was ruled out in the first place
- **C8b-N3-C9-Resolution-Plan.md** - Deep dive on three critical issues
- **IMPLEMENTATION-CHECKLIST.md** - Per-issue acceptance criteria, dependencies and
  the critical path (task detail lives in each issue's body)
- **diagrams/*** - Six rendered architecture and sequence diagrams, embedded below

## Implementation Phases

### Phase 1: Design Finalization (layerfs-project#42)
- Confirm the settled conclusions (`LayerFS-GC-spec.md` §1.1, S1-S11)
- Answer the open questions (`LayerFS-GC-spec.md` §1.2, Q1-Q6)
- Sign off R1 / Q1 / C3 (see *Sign-off requests* above)

### Phase 2: Infrastructure (layerfs-project#43, layerfs-project#48)
- Admission enforcement
- Shared data reachability

### Phase 3: Soft Removal (layerfs-project#44, layerfs-project#45, layerfs-project#47)
- Branch removal
- Project deletion  
- File removal

### Phase 4: Physical Compaction (layerfs-project#49, layerfs-project#50)
- SQLite compaction
- PostgreSQL compaction

### Phase 5: Integration (layerfs-project#51, layerfs-project#52)
- API/SDK/CLI exposure
- Comprehensive testing

## Admission & Mount Policy

Which mounts each operation excludes is the admission matrix, **C10** in
`LayerFS-GC-spec.md` §5 — see it there; this README does not restate it.

## Diagrams

Rendered from the design set. Each caption names the issue the diagram belongs to;
the images are stored in `diagrams/` beside this file.

**1 · Documents → design gate → sub-issues** (layerfs-project#42)
The five documents and the gate they feed: `layerfs-project#42` freezes C1-C10, then admission
and reachability run in parallel.

![Documents and sub-issues](diagrams/01-documents-and-sub-issues.png)

*Changed since drawn:* the directory is now `core/docs/issues/project-41/`, and
the diagram's bare `#N` labels mean `layerfs-project#N`.

**2 · One compact = one algorithm, two swap primitives** (layerfs-project#48)
Mark, verify, install, retire over a single exclusive window; the fences sit on
the timeline, not beside it.

![Sweep sequence](diagrams/02-sweep-sequence.png)

*Changed since drawn:* there is no `rootGeneration`; `G` is `catalog_incarnation`,
and the pre-install re-check is `catalog_incarnation` plus `Verify{heads}` (C1).
Wholly dead packs are no longer "never a candidate": the staged copy simply does
not receive them (§7.2).

**3 · R1 = E: the swap is a schema rename** (layerfs-project#50)
Build the replacement schema, copy retained rows, verify with the readers' own
open check, rename both schemas in one transaction, bump the generation, drop the
old schema. The name ends where it started, so nothing that remembered it changes.

![R1-E swap sequence](diagrams/03-r1-e-swap-sequence.png)

**4 · C8b: the writer fence** (layerfs-project#49)
A writer holding the old fd would write into the retired inode; the fence must
refuse that write.

![C8b writer fence](diagrams/04-c8b-writer-fence.png)

*Changed since drawn:* the fence no longer compares incarnations (`g5 < g6`). An
old-fd writer reads `catalog_incarnation` from the retired file, where it never
changes, so that comparison cannot see the swap. The fence is now: at the start of
every write transaction, compare the Store path's current `(dev, ino)` with the
open file's `fstat`; a mismatch returns `Conflict` (spec §5 C8b).

**5 · R1 is decided by one requirement variable** (layerfs-project#50)
L1 and L2 do not discriminate; L3 does, and L3 is governed by S11 alone.

![R1 vs A' decision ladder](diagrams/05-r1-vs-a-prime-decision.png)

**6 · Q1 and C3: same failure family, opposite mechanisms** (layerfs-project#44, cross-linked to layerfs-project#42)
A name cannot recur, bytes necessarily do — which is why one decision forbids
reuse and the other binds the intent to a generation.

![Q1 vs C3](diagrams/06-q1-vs-c3.png)

## Decision ownership

Where each decision or reconciliation entry is realised. The specification
sections are §-numbered as in the GC spec; `R`/`N`/`Q` ids are that spec's
reconciliation and net-new tables.

| Item | What it decides | Contract / record | Implemented in | Verified in |
|---|---|---|---|---|
| **R1** | PostgreSQL Store replacement: E (two `ALTER SCHEMA RENAME`) by default, A' (pointer + versioned schema) if S11 requires reads up | layerfs-project#42 (§1.3 R1, §7.3, S11), 03-decision-record §1 | **layerfs-project#50** | layerfs-project#52 |
| **R2** (= C8, replaces the `compact_intent` row) | Reader fence: `catalog_incarnation += 1` at install; every session/transaction entry checks `(catalog_id, incarnation)` → `StaleStore`; SQLite also `(dev, ino)`; `lfq1` across a generation → `StaleStore` | layerfs-project#42 (C8, generation arithmetic; no second generation field) | **layerfs-project#43** (the session-entry half; shared by both backends), `(dev, ino)` in layerfs-project#49 | layerfs-project#52 |
| **Q1** (= C6) | Branch name reuse: none; terminal tombstone, `NameRetired` | layerfs-project#42 (Q1, C6), 03-decision-record §2 | **layerfs-project#44** | layerfs-project#52 |
| **C3** | Per-object delete binds `(ObjectId, G)` | layerfs-project#42 (C3), 03-decision-record §3 | **layerfs-project#49**, **layerfs-project#50** (consumers of the signature) | layerfs-project#52 |
| **N3** | Soft delete visibility: `removed_ms` / `deleted_ms`; name unique indexes stay full, so a removed name stays retired (`NameRetired`) | layerfs-project#42 (schema change) | **layerfs-project#44** (`removed_ms`), **layerfs-project#45** (`deleted_ms`, migration) | layerfs-project#52 |
| **C8b** | SQLite writer fence: at every write-transaction start, path `(dev, ino)` vs the open file's `fstat` → `Conflict` | layerfs-project#42 (C8b) | **layerfs-project#49** | layerfs-project#52 |
| **C9** | HistoryCatalog stays extensible: GC methods behind a default `Unsupported`, no new `HistoryError` variants | layerfs-project#42 (C9) | **layerfs-project#51** (trait), layerfs-project#49/layerfs-project#50 (overrides) | layerfs-project#52 |
| **C10** | Admission matrix: which mounts each operation excludes | layerfs-project#42 (C10) | **layerfs-project#43** | layerfs-project#52 |
| **Q3** | Rewrite step: transfer phases with one inserted `rewrite` | layerfs-project#42 (§7.1) | **layerfs-project#49** | layerfs-project#52 |
| **Q5** | PostgreSQL reader behaviour | Superseded by C8/R2 — no separate decision | **layerfs-project#43** | layerfs-project#52 |
| **D3** | PostgreSQL fence mechanism (advisory lock / `FOR UPDATE` / schema lock) | Pending sign-off — see layerfs-project#50 | **layerfs-project#50** | layerfs-project#52 |

### Sign-off requests

**The full product-decision inventory — all twelve items, with the issue that owns
each — is the single entry point on
[#42](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/42#issuecomment-6819532224).**
Start there; the three reconciliation decisions below are asked for approval in
the issue that realises each one, with a short reasoning summary and a checkbox
block:

| Decision | Recommended default | Asked in | Comment |
|---|---|---|
| R1 | **E** (exclusive window) | layerfs-project#50 (PostgreSQL GC) | <https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/50#issuecomment-6818137198> |
| Q1 | **A** (no reuse) | layerfs-project#44 (Branch removal) | <https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/44#issuecomment-6818137920> |
| C3 | **`delete(id, G)`** | layerfs-project#42 (contracts) | <https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/42#issuecomment-6818138560> |

R1 is independent. **Q1 and C3 are read together** — same failure family, opposite
mechanisms (`03-decision-record.md` §3.6) — and each request links to the other,
so a reviewer signs them knowing both.

When an answer arrives, record it here and in `03-decision-record.md`; an
unanswered decision keeps its implementation issue blocked.

R2 is not a separate sub-issue: the spec folds it into C8 and assigns C8 to
admission (layerfs-project#43). Its install-time bump is executed by whichever backend installs
(layerfs-project#49 on SQLite, layerfs-project#50 on PostgreSQL), and its reader-facing refusal is what layerfs-project#52
tests across a generation boundary.

## Related Resources

- **Specification PR**: Ephemeral-AI-Lab/layerfs#658 (`core/docs/issues/project-41/`)
- **Release Contract**: Ephemeral-AI-Lab/layerfs#511
- **Sub-Issues**: layerfs-project#42-#52

## References

- See the complete specification in Ephemeral-AI-Lab/layerfs#658
- Three critical issues (C8b, N3, C9) explained in C8b-N3-C9-Resolution-Plan.md

---

**Last Updated**: 2026-11-12
