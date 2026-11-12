# LayerFS GC Three-Issue Resolution Summary

## 📋 Problem Overview

Three key design issues identified from `LayerFS-GC-spec.md`:

| ID | Category | Issue | Impact | Status |
|----|------|------|------|------|
| **C8b** | New code | SQLite writer fence (inode generation tracking) | P0 risk: silent data loss | To implement |
| **N3** | Schema change | Logical delete representation (layer_stack / branch) | Belongs to #42 acceptance point | To implement |
| **C9** | API contract | HistoryCatalog 8-impl maintenance constraint | Reduces maintenance cost | To follow |

---

## 🔧 C8b: SQLite Writer Fence (Pre-swap Writer Fence)

### Core problem
After the Store file rename swap during compact, a pre-swap writer's old fd can still write into the retired inode, which the new Store cannot observe → **silent data loss**.

### Solution
**Generation mechanism (incarnation protocol)**:
- The Store holds `incarnation: u64`, incremented on every compact install
- Writes are checked at admission: `writer_gen_at_acquire < current_gen` → `Conflict`
- Two gates: catalog_incarnation is incremented before install + every write is checked when it opens

### Key code locations
```
store/identity.rs → StoreGeneration struct
store/sqlite/writer_admission.rs → new, admission check logic
backend/sqlite/session/write_path.rs → integrates the check
persistence/history/catalog.rs → incarnation incremented at install
```

### Implementation order (5 steps)
1. Add the `StoreGeneration` struct (store/identity.rs)
2. Create `writer_admission.rs`
3. Modify write_path.rs to integrate the check
4. Modify compact install in catalog.rs
5. Concurrency test (compact + write)

---

## 📊 N3: Logical Delete Representation (Soft Delete Visibility)

### Core problem
There is no visibility column, so soft delete (keep the row but hide it) cannot be implemented, and name reuse / Branch history protection is impossible.

### Solution
**One schema upgrade from v3.5 → v3.6**:
- `layer_stack` → add `deleted_ms` (nullable BIGINT)
- `branch` → add `removed_ms` (nullable BIGINT)
- Indexes become partial indexes: `WHERE deleted_ms IS NULL` and `WHERE removed_ms IS NULL`
- Delete operations become UPDATE SET deleted_ms/removed_ms, no longer DELETE

### Key SQL changes
```sql
-- Old: globally unique
CREATE UNIQUE INDEX layer_stack_names ON layer_stack(name)

-- New: partially unique (allows multiple tombstones)
CREATE UNIQUE INDEX layer_stack_names 
  ON layer_stack(name) WHERE deleted_ms IS NULL
```

### Implementation order (6 steps)
1. Modify the schema definitions (sqlite + postgresql)
2. Create migrate.rs (v3.5 → v3.6)
3. Add the visibility condition to all queries in catalog.rs
4. Change remove.rs: DELETE → UPDATE SET deleted_ms
5. Change the API remove → delete semantics
6. Migration test (old DB → new schema visibility verification)

---

## 🧩 C9: HistoryCatalog Extension Constraints

### Core problem
32 methods × 8 implementations = a maintenance explosion. Without a default body, every new GC method has to be changed in 8 places. Adding new HistoryError variants is also forbidden (two wildcard mapping points would silently degrade).

### Solution
**Design by convention**:

1. **New GC methods must have an `Unsupported` default implementation body**
   ```rust
   pub trait HistoryCatalog {
       fn mark_branch_removed(...) -> Result<()> {
           Err(HistoryError::Unsupported("mark_branch_removed".to_string()))
       }
   }
   ```
   - In-memory / test doubles inherit it automatically, no changes needed
   - Real backends (SQLite / PostgreSQL) override it in their impl blocks

2. **No new HistoryError variants**
   - The existing 6 variants (Unsupported, StaleStore, Conflict, NotFound, InvalidInput, Internal) suffice
   - Sub-classify errors with a String detail: `Conflict("branch_removed_cannot_fork")`

### Key code locations
```
core/crates/layerfs-history/src/contract/catalog.rs → default for the 4 GC methods
backend/sqlite/src/catalog.rs → override
backend/postgresql/src/catalog.rs → override
core/crates/layerfs-history/src/contract/error.rs → 8 variants (frozen)
```

### New GC methods (4)
- `mark_branch_removed(branch_id, removed_ms) -> Result<()>`
- `mark_project_deleted(stack_id, deleted_ms) -> Result<()>`
- `get_retained_roots() -> Result<RetainedRoots>`
- `remove_unreachable_history(keep_node_ids) -> Result<RemovalStats>`

### Implementation order (5 steps)
1. Add the 4 methods to the trait + Unsupported default
2. SQLite impl override + implementation
3. PostgreSQL impl override + implementation
4. Run the unit tests (the test doubles need no changes)
5. Integration test verifying the default Unsupported

---

## 📈 Excalidraw Architecture and Sequence Diagrams

Three visual design diagrams have been generated:

### 1. C8b - Writer Fence Sequence Diagram
**Link**: https://app.excalidraw.com/s/9Zq6XgRe98z/5fb5lT5yDKY

Shows: Writer → Admission → Store generation check → Compact concurrency scenario → Conflict triggered

### 2. N3 - Schema Migration Diagram
**Link**: https://app.excalidraw.com/s/9Zq6XgRe98z/1dZkWEGOgA7

Shows: v3.5 hard delete → migration steps → v3.6 partial-index soft delete

### 3. C9 - HistoryCatalog Architecture
**Link**: https://app.excalidraw.com/s/9Zq6XgRe98z/8ZPm9UyP100

Shows: 32-method trait → 8 impls (2 real + 6 test) → default pattern for the 4 GC methods

---

## ✅ Integration Verification Checklist

| Issue | Key verification | Expected result |
|------|--------|--------|
| C8b | Concurrent compact + write (pre-swap) | Conflict triggered correctly, no silent loss |
| N3 | Old DB migration + list visibility | Soft-deleted rows hidden, name reusable |
| C9 | Run the existing unit tests | 8 impls unchanged (inherit Unsupported) |

---

## 🎯 Decision points (open questions)

| ID | Item | Recommendation | Ownership |
|----|------|------|--------|
| D1 | BranchId generation encoding | Freeze: identity_format vs a separate column | Product |
| D3 | PostgreSQL fencing method | Confirm: advisory lock vs FOR UPDATE vs schema lock | Product |
| D6 | Timestamp precision | Proposal: nullable BIGINT (ms) | Product |
| D7 | Source Branch removal permission | Proposal: allowed (a fork keeps what it used) | Product |

---

## 📁 Deliverables

- ✅ `C8b-N3-C9-Resolution-Plan.md` (1,179 lines, 34 KB)
  - Detailed implementation path, code examples, key locations
  - Recommended implementation order for each issue

- ✅ Three Excalidraw architecture and sequence diagrams
  - C8b Writer Fence Sequence
  - N3 Schema Migration Flow
  - C9 HistoryCatalog Architecture

---

## 🚀 Next Actions

1. **Product confirmation** of decision points D1, D3, D6, D7
2. **Development order** (recommended)
   - Phase 1: N3 schema migration (infrastructure)
   - Phase 2: C8b writer fence (risk mitigation)
   - Phase 3: C9 GC method implementation (functional completeness)
3. **Test coverage**
   - Unit tests: generation, visibility, and error mapping for each module
   - Integration tests: schema upgrade, concurrent compact+write
   - End-to-end: verification of the full GC flow

