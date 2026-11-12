# Implementation Checklist - GC Issue #41

## Overview

This checklist tracks the implementation of all 10 sub-issues (#42-#52) for LayerFS Garbage Collection.

**Foundation**: PR #658 contains complete design specification in `C8b-N3-C9-Resolution-Plan.md`

---

## Phase 1: Design Finalization (#42)

**Goal**: Confirm all design decisions before implementation

**Pre-Implementation**:
- [ ] Read: `core/docs/issues/41/README.md` (overview)
- [ ] Read: `core/docs/issues/41/01-overview-and-decisions.md` (D1-D14)
- [ ] Review: All diagrams in Excalidraw collection
- [ ] Understand: 6 pending decisions requiring product sign-off

**Tasks**:
- [ ] D1: Store-level GC granularity - CONFIRM
- [ ] D2: Logical-first approach - CONFIRM
- [ ] D3: PostgreSQL fence strategy - **PENDING** (product)
- [ ] D4: Exclusive access during compact - CONFIRM
- [ ] D5: Cross-project safety - CONFIRM
- [ ] D6: GC algorithm details - **PENDING** (product)
- [ ] D7: Cross-project Pack protection - **PENDING** (product)
- [ ] D8: Writer-exclusive fence - CONFIRM
- [ ] D9: Name reuse with timestamp - CONFIRM
- [ ] D10: Immutable ID protection - CONFIRM
- [ ] D11: PostgreSQL remote required - CONFIRM
- [ ] D12-D14: Other decisions - CONFIRM

**Acceptance**: All 11 confirmed, 6 pending have product approval

**Status**: ⏳ Waiting for product decisions

---

## Phase 2a: Infrastructure (#43, #48)

### Issue #43: Admission Enforcement

**Pre-Implementation**:
- [ ] Read: `02-operation-contracts.md` (§5 C10)
- [ ] Understand: Admission matrix (branch/project/file/compact)
- [ ] Know the four error codes: MountsActive, Busy, Conflict, Unsupported

**Key Requirements**:
- [ ] Define admission matrix data structure
- [ ] Implement atomic mount state check
- [ ] Implement MountsActive error return
- [ ] Implement Busy error for conflicting users
- [ ] No implicit wait or automatic cleanup

**Acceptance**:
- [ ] Admission matrix enforced on all 4 operations
- [ ] Tests pass on SQLite backend
- [ ] Tests pass on PostgreSQL backend
- [ ] Mount violations return MountsActive
- [ ] Conflicting users return Busy

**Code Locations**: `control/crates/layerfs-store/src/admission.rs` (new)

**Status**: 🔲 Not Started

---

### Issue #48: Shared Data Reachability

**Pre-Implementation**:
- [ ] Read: `C8b-N3-C9-Resolution-Plan.md` (§6 Closure)
- [ ] Understand: 3-step closure (Pinned → Historical → Cross-Project)
- [ ] Understand: Pack sharing and delta-base protection

**Key Requirements**:
- [ ] Implement reachability closure algorithm
- [ ] Implement reference counting
- [ ] Protect transitive delta bases
- [ ] Test cross-project sharing scenarios
- [ ] Verify pack sharing detection

**Acceptance**:
- [ ] All shared data protected from GC
- [ ] Reference counts accurate
- [ ] Cross-project dependencies survive compact
- [ ] Tests pass on SQLite and PostgreSQL

**Code Locations**: `persistence/src/reachability.rs` (new)

**Depends On**: #42 (design finalized)

**Status**: 🔲 Not Started

---

## Phase 2b: Soft Removal (#44, #45, #47)

### Issue #44: Branch Removal

**Pre-Implementation**:
- [ ] Read: `02-operation-contracts.md` (branch.remove)
- [ ] Read: `C8b-N3-C9-Resolution-Plan.md` (N3 - removed_ms)
- [ ] Understand: Soft delete via removed_ms timestamp

**Key Requirements**:
- [ ] Mark Branch as removed (removed_ms timestamp) - N3
- [ ] Prevent new mounts to removed Branch
- [ ] Verify other Branches unaffected
- [ ] Test publish/merge with removed Branch
- [ ] Tests on SQLite and PostgreSQL

**Acceptance**:
- [ ] branch.remove() works correctly
- [ ] Immutable IDs still accessible
- [ ] Name can be reused
- [ ] Tests pass on both backends

**Code Locations**: 
- `persistence/src/branch.rs` (mark_branch_removed)
- `control/crates/layerfs-store/src/admission.rs` (branch.remove policy)

**Depends On**: #42, #43, #48

**Status**: 🔲 Not Started

---

### Issue #45: Project Deletion

**Pre-Implementation**:
- [ ] Read: `02-operation-contracts.md` (project.delete)
- [ ] Read: `C8b-N3-C9-Resolution-Plan.md` (N3 - deleted_ms)
- [ ] Understand: Schema migration v3.5 → v3.6
- [ ] Understand: Soft delete + Store remaining addressable

**Key Requirements**:
- [ ] Schema migration: Add deleted_ms column - N3
- [ ] Mark project as deleted (deleted_ms timestamp)
- [ ] Keep Store addressable for later compact
- [ ] Verify Sandboxes cannot mount deleted project
- [ ] Tests on SQLite and PostgreSQL

**Acceptance**:
- [ ] project.delete() works correctly
- [ ] Store still addressable for compact
- [ ] Name can be reused
- [ ] Database migration succeeds
- [ ] Tests pass on both backends

**Code Locations**:
- `persistence/backend/sqlite/migrations.rs` (v3.5 → v3.6)
- `persistence/src/project.rs` (mark_project_deleted)

**Depends On**: #42, #43, #48

**Status**: 🔲 Not Started

---

### Issue #47: File Removal

**Pre-Implementation**:
- [ ] Read: `02-operation-contracts.md` (project.remove_file)
- [ ] Read: `C8b-N3-C9-Resolution-Plan.md` (C9 - HistoryCatalog)
- [ ] Understand: Immutable ID access protection

**Key Requirements**:
- [ ] Design immutable ID access contract (C9)
- [ ] Implement logical file removal
- [ ] Implement history traversal with removed files
- [ ] Protect immutable IDs before/after removal
- [ ] Tests on cross-project sharing

**Acceptance**:
- [ ] project.remove_file() works correctly
- [ ] Immutable IDs remain accessible
- [ ] History entries marked as removed
- [ ] Cross-project sharing unaffected
- [ ] Tests pass on both backends

**Code Locations**: `persistence/src/history/remove_file.rs` (new)

**Depends On**: #42, #43, #48

**Status**: 🔲 Not Started

---

## Phase 3: Physical Compaction (#49, #50)

### Issue #49: SQLite Compaction

**Pre-Implementation**:
- [ ] Read: `C8b-N3-C9-Resolution-Plan.md` (C8b - SQLite Incarnation)
- [ ] Understand: 5-stage incarnation protocol
- [ ] Understand: Two-gate write fence (admission + install)
- [ ] Understand: Writer-exclusive fence during Stage 3

**Key Requirements**:
- [ ] Implement C8b: Incarnation mechanism
- [ ] Stage 1: Mark discarded rows
- [ ] Stage 2: Create new tables
- [ ] Stage 3: Atomic swap with writer fence (CRITICAL)
- [ ] Stage 4: Reader fence / grace period
- [ ] Stage 5: Drop old tables
- [ ] Recovery after interrupted compact

**Acceptance**:
- [ ] SQLite incarnation protocol works
- [ ] Writer fence prevents silent data loss
- [ ] Physical shrink verified (VACUUM ANALYZE)
- [ ] Tests: concurrent compact + write → Conflict
- [ ] Tests: mount failures, grace periods
- [ ] No silent data loss scenarios

**Code Locations**:
- `backend/sqlite/src/catalog_gc.rs` (impl HistoryCatalogGC)
- `store/identity.rs` (StoreGeneration)
- `store/sqlite/writer_admission.rs` (writer fence)

**Depends On**: #42, #43, #48, N3 schema

**Status**: 🔲 Not Started

---

### Issue #50: PostgreSQL Compaction

**Pre-Implementation**:
- [ ] Read: `C8b-N3-C9-Resolution-Plan.md` (D3 - PostgreSQL fence)
- [ ] Understand: PostgreSQL-specific compaction strategy
- [ ] Understand: Remote server support (not stub)
- [ ] Understand: Physical reclamation verification

**Key Requirements**:
- [ ] Implement PostgreSQL compaction
- [ ] Support remote PostgreSQL servers
- [ ] Implement physical reclamation (pg_total_relation_size)
- [ ] Test network partition recovery
- [ ] Measure space reclamation before/after

**Acceptance**:
- [ ] PostgreSQL compaction works
- [ ] Remote servers fully supported (not Unsupported)
- [ ] Physical shrink verified (pg_total_relation_size)
- [ ] Tests: remote server scenarios
- [ ] Tests: network partition recovery
- [ ] Space reclamation metrics accurate

**Code Locations**: `backend/postgresql/src/catalog_gc.rs` (impl HistoryCatalogGC)

**Depends On**: #42, #43, #48, N3 schema

**Status**: 🔲 Not Started

---

## Phase 4: Integration & Qualification (#51, #52)

### Issue #51: API/SDK/CLI Exposure

**Pre-Implementation**:
- [ ] Read: `02-operation-contracts.md` (all 4 operations)
- [ ] Read: `C8b-N3-C9-Resolution-Plan.md` (C9 - HistoryCatalog)
- [ ] Understand: API contracts for all operations

**Key Requirements**:
- [ ] Add API endpoints (branch.remove, project.delete, etc.)
- [ ] Add SDK methods
- [ ] Add CLI commands
- [ ] Implement C9: HistoryCatalogGC trait (4 GC methods)
- [ ] Write user guides and best practices
- [ ] Document error codes and recovery
- [ ] Add code examples

**Acceptance**:
- [ ] All 4 operations exposed through API/SDK/CLI
- [ ] HistoryCatalogGC trait properly designed (CCP/CRP)
- [ ] Error handling clear and documented
- [ ] Examples cover all scenarios
- [ ] User guides complete

**Code Locations**:
- `contract/src/catalog_gc.rs` (HistoryCatalogGC trait) - NEW
- `api/v1/gc_operations.rs` (API endpoints)
- `sdk/gc_client.rs` (SDK methods)
- `cli/commands/gc.rs` (CLI commands)

**Depends On**: #42-#50 all complete

**Status**: 🔲 Not Started

---

### Issue #52: Acceptance Testing & Qualification

**Pre-Implementation**:
- [ ] Prepare crash injection framework
- [ ] Prepare state machine validator
- [ ] Prepare cross-project pack verifier
- [ ] Prepare receipt byte consistency checker

**Key Requirements**:
- [ ] Integration tests (all operations together)
- [ ] Crash recovery tests at 5 injection points
- [ ] Cross-project sharing verification
- [ ] Performance benchmarks
- [ ] Load testing
- [ ] Documentation validation

**Crash Injection Points**:
1. During Setup (discarded rows marked)
2. During Create (new tables exist)
3. During Swap (one of old/new active)
4. During Fence (no new readers on old)
5. During Drop (old data gone)

**Verification**:
- [ ] Discarded rows correctly marked
- [ ] New tables created with correct schema
- [ ] Swap is atomic (one version active)
- [ ] Reader fence blocks old readers
- [ ] Old data successfully dropped
- [ ] Cross-project packs protected
- [ ] Receipt bytes consistent
- [ ] No silent data loss

**Acceptance**:
- [ ] All tests pass on SQLite
- [ ] All tests pass on PostgreSQL
- [ ] All crash scenarios handled
- [ ] Performance meets targets
- [ ] Load tests pass

**Code Locations**: `tests/gc_qualification.rs` (new)

**Depends On**: #42-#51 all complete

**Status**: 🔲 Not Started

---

## Critical Paths & Dependencies

```
#42 (Design)
  ├─→ #43 (Admission)
  │   ├─→ #44 (Branch RM)
  │   ├─→ #45 (Project RM)
  │   └─→ #47 (File RM)
  │
  ├─→ #48 (Reachability)
  │   ├─→ #44, #45, #47
  │   ├─→ #49 (SQLite)
  │   └─→ #50 (PostgreSQL)
  │
  └─→ #51, #52 (API & Tests)
      ├─→ #43-#50 all complete
      └─→ Cross-platform verification
```

**Critical**: #42 must complete first (design decisions)

---

## Review & Sign-Off

| Phase | Reviewer | Status | Sign-Off |
|-------|----------|--------|----------|
| #42 | Product + Architecture | 🔲 | - |
| #43 | Architecture | 🔲 | - |
| #44-47 | Engineering Lead | 🔲 | - |
| #49-50 | Database Specialist | 🔲 | - |
| #51 | API Lead | 🔲 | - |
| #52 | QA Lead | 🔲 | - |

---

**Last Updated**: 2026-11-12  
**Total Issues**: 10 (#42-#52)  
**Estimated Duration**: 8-12 weeks (with team)  
**Base PR**: Ephemeral-AI-Lab/layerfs#658

