# Resolution Plan for the Three Critical LayerFS GC Design Issues

## Background

Based on Chapter 5 "Contracts" of `LayerFS-GC-spec.md` (and its §1.2 open questions), the three issues sit at different implementation layers:

---

## C8b: SQLite Writer Fence (pre-swap writer fence)

### Problem statement
When compact performs the Store file rename swap, a pre-swap writer (a write handle acquired on the old Store fd) can still write to the retired inode, and the new Store does not see those writes. The result is **silent data loss**.

The existing pre-install `(dev, ino)` inode check is not sufficient.

### Root cause
- After the compact rename, the filesystem node (inode) the old fd points at has been retired (deleted or hidden)
- The new Store cannot observe writes the pre-swap writer makes to the old inode
- The new Store and the old Store point at different physical data

### Specification decision (frozen contract §3.8)
**SQLite must implement a writer fence**:
1. **Re-read the incarnation before install** (incarnation / catalog_incarnation)
2. **Any write handle acquired on an old incarnation must fail after install**
3. **Reuse the existing `Conflict` rejection code**; do not add a new rejection code type

### Implementation path

#### Step 1: Identify the incarnation check points
```
Key flow of a write operation:
  admission (store_id, generation, ...)  
    ↓
  take_write_lock_or_fail(generation check)  ← C8b first gate
    ↓
  write operations (sqlite write handles)
    ↓
  commit/flush
    ↓
  install_check(dev, ino, generation)  ← C8b second gate (existing)
```

**Key code locations**:
- `control/crates/layerfs-store/src/identity.rs` — Store identity, lease, generation records
- `control/crates/layerfs-store/src/sqlite/writer_admission.rs` (new) — write admission
- `backend/sqlite/lifecycle/installation.rs` — inode check in the install phase
- `engine/crates/layerfs-persistence/src/history/catalog.rs` — catalog_incarnation increment

#### Step 2: Incarnation mapping design
```rust
// Store side (store/identity.rs)
pub struct StoreGeneration {
    incarnation: u64,  // +1 per compact attempt (before install)
    file_inode: (dev_t, ino_t),
    last_writer_generation: u64,  // incarnation at the last successful write
}

// Write path (new writer_admission.rs)
pub fn writer_admission_check(
    store_id: &str,
    current_gen: u64,
    writer_gen_at_acquire: u64,
) -> Result<WriteHandle, StoreError> {
    if writer_gen_at_acquire < current_gen {
        // This write handle was acquired on an old incarnation and cannot be used after compact
        Err(StoreError::Conflict(
            format!("writer incarnation mismatch: acquired {}, current {}", 
                    writer_gen_at_acquire, current_gen)
        ))
    } else {
        Ok(WriteHandle { /* ... */ })
    }
}
```

#### Step 3: Compact flow integration
```
store.compact(store_name):
  stage 1: "Prepare"
    ├─ lock Store (exclusive, no new writers)
    └─ read_generation() → gen_before = catalog_incarnation + 1

  stage 2: "Mark"
    └─ reachability walk (decode_references + ancestry)

  stage 3: "Rebuild"
    └─ new Store file in temp location

  stage 4: "Verify"  
    └─ check roots resolve, pack signatures valid

  stage 5: "Install"
    ├─ catalog_incarnation += 1  ← incarnation bump
    ├─ install_check(new inode) ← storage-layer inode check
    └─ atomic rename(temp → live)

  stage 6: "Retire"
    └─ delete old file

  → Any write acquired at gen ≤ gen_before will fail Conflict on admission
```

#### Step 4: SQLite-specific details
- **Admission point**: `backend/sqlite/session/write_path.rs` — checked when each write is opened
- **Incarnation persistence**: `history_meta.incarnation` or the `store_generation` table
- **Test**: concurrent compact and write; verify Conflict is triggered correctly

### Recommended implementation order
1. Add the `StoreGeneration` struct in `store/identity.rs`
2. Create `store/sqlite/writer_admission.rs` and implement the admission check
3. Change `backend/sqlite/session/write_path.rs` to call the check when a write handle is opened
4. Change `persistence/history/catalog.rs` to bump the incarnation on compact install
5. Test: concurrent write + compact scenario

---

## N3: Soft Delete Representation (soft delete visibility)

### Problem statement
The `branch` and `layer_stack` tables have no visibility column, so soft deletion (logical deletion with the row kept physically) cannot be implemented.

### Specification decision (frozen contract §3.4)
In **one schema version upgrade**:
1. Add a `deleted_ms` column to `layer_stack` (nullable timestamp)
2. Add a `removed_ms` column to `branch` (nullable timestamp)
3. Turn the global unique index `layer_stack_names` into a **partial index** `ON layer_stack(name) WHERE deleted_ms IS NULL`
4. Turn the branch unique index into a **partial index** `ON branch(layer_stack_id, name) WHERE removed_ms IS NULL`

### Implementation path

#### Step 1: Schema migration (both backends together)

**SQLite** (`sql/sqlite/history.sql`):
```sql
-- 3.5 → 3.6 migration
ALTER TABLE layer_stack ADD COLUMN deleted_ms BIGINT DEFAULT NULL;
ALTER TABLE branch ADD COLUMN removed_ms BIGINT DEFAULT NULL;

-- drop the non-partial indexes
DROP INDEX IF EXISTS layer_stack_names;
DROP INDEX IF EXISTS branch_names;

-- create the partial indexes
CREATE UNIQUE INDEX layer_stack_names 
  ON layer_stack(name) WHERE deleted_ms IS NULL;

CREATE UNIQUE INDEX branch_names 
  ON branch(layer_stack_id, name) WHERE removed_ms IS NULL;
```

**PostgreSQL** (`sql/postgresql/schema.sql`):
```sql
ALTER TABLE layer_stack ADD COLUMN deleted_ms BIGINT DEFAULT NULL;
ALTER TABLE branch ADD COLUMN removed_ms BIGINT DEFAULT NULL;

DROP INDEX IF EXISTS layer_stack_names;
DROP INDEX IF EXISTS branch_names;

CREATE UNIQUE INDEX layer_stack_names 
  ON layer_stack(name) WHERE deleted_ms IS NULL;

CREATE UNIQUE INDEX branch_names 
  ON branch(layer_stack_id, name) WHERE removed_ms IS NULL;
```

#### Step 2: Rework the visibility queries

**Original query** (physical delete mode):
```sql
SELECT * FROM layer_stack WHERE name = ?
```

**New query** (soft delete mode):
```sql
SELECT * FROM layer_stack WHERE name = ? AND deleted_ms IS NULL

-- or a shared macro
#define VISIBLE_STACK "AND deleted_ms IS NULL"
SELECT * FROM layer_stack WHERE name = ? VISIBLE_STACK
```

**When listing all items**:
```sql
-- list active items only
SELECT * FROM layer_stack WHERE deleted_ms IS NULL ORDER BY ...

-- optional for the admin UI: include deleted items
SELECT *, (deleted_ms IS NOT NULL) AS is_deleted FROM layer_stack ...
```

#### Step 3: Turn delete operations into UPDATE

**Original code** (`project/remove.rs`, `branch/remove.rs`):
```rust
// old mode: delete the row
DELETE FROM branch WHERE id = ?
DELETE FROM layer_stack WHERE id = ?
```

**New code**:
```rust
// new mode: record the deletion time
UPDATE layer_stack SET deleted_ms = ? 
  WHERE id = ? AND deleted_ms IS NULL;

UPDATE branch SET removed_ms = ? 
  WHERE id = ? AND removed_ms IS NULL;
```

#### Step 4: Catalog constraints (the counterexample to C9)

Every query method in **HistoryCatalog** must add the visibility condition when it generates SQL:
```rust
// engine/crates/layerfs-persistence/src/history/catalog.rs

impl HistoryCatalog for SqliteCatalog {
    fn list_projects(&mut self, stack_id: &LayerStackId) -> Result<Vec<LayerStack>> {
        // visibility condition included automatically
        self.conn.query("
            SELECT * FROM layer_stack 
            WHERE id = ? AND deleted_ms IS NULL
        ", ...)
    }

    fn list_branches(&mut self, stack_id: &LayerStackId) -> Result<Vec<Branch>> {
        self.conn.query("
            SELECT * FROM branch 
            WHERE layer_stack_id = ? AND removed_ms IS NULL
        ", ...)
    }
}
```

#### Step 5: Migration trigger

An on-open migration is needed:
```rust
// engine/crates/layerfs-persistence/src/history/migrate.rs

pub fn migrate_3_5_to_3_6(conn: &mut Connection) -> Result<()> {
    // 1. add the columns
    conn.execute_batch("
        ALTER TABLE layer_stack ADD COLUMN deleted_ms BIGINT DEFAULT NULL;
        ALTER TABLE branch ADD COLUMN removed_ms BIGINT DEFAULT NULL;
    ")?;
    
    // 2. rebuild the indexes
    conn.execute("DROP INDEX layer_stack_names")?;
    conn.execute("DROP INDEX branch_names")?;
    
    conn.execute("
        CREATE UNIQUE INDEX layer_stack_names 
        ON layer_stack(name) WHERE deleted_ms IS NULL
    ")?;
    
    conn.execute("
        CREATE UNIQUE INDEX branch_names 
        ON branch(layer_stack_id, name) WHERE removed_ms IS NULL
    ")?;
    
    // 3. update history_meta
    conn.execute("UPDATE history_meta SET schema_version = 6")?;
    
    Ok(())
}
```

### Recommended implementation order
1. Change `sql/sqlite/history.sql` and `sql/postgresql/schema.sql`
2. Create `persistence/history/migrate.rs` and implement the migration logic
3. Change `persistence/history/catalog.rs` to add the visibility condition to every query
4. Change `project/remove.rs` and `branch/remove.rs` to use UPDATE
5. Change `api/project/remove.rs` and `api/branch/remove.rs` to `delete` semantics
6. Migration test: old database → new schema, verify visibility is correct

---

## C9: HistoryCatalog Constraints

### Problem statement
The `HistoryCatalog` trait has 32 methods and 8 implementations:
- 1 real implementation (SQLite + PostgreSQL)
- 6 test doubles (daemon/mock)
- 1 in-memory implementation

If a new GC method has no default body, all 8 impls must change together, so **maintenance cost explodes**.

Adding a new `HistoryError` variant is likewise forbidden, because both mapping points use the `_ => "Failed"` wildcard, so a new variant degrades silently.

### Specification decision (frozen contract §5, §2.3)
1. **Every new GC method must have a default `Unsupported` body**
2. **No new `HistoryError` variants**; reuse the existing codes
3. **The default body serves in-memory/test doubles; the real backends override it in their impl blocks**

### Implementation path

#### Step 1: Check the existing structure

**HistoryCatalog location**:
- `core/crates/layerfs-history/src/contract/catalog.rs`

**Error type**:
- `core/crates/layerfs-history/src/contract/error.rs` — `HistoryError` enum

**The 8 implementations**:
1. `backend/sqlite/src/catalog.rs` — SQLite (real)
2. `backend/postgresql/src/catalog.rs` — PostgreSQL (real)
3. `engine/crates/layerfs-store-service/src/mocks/catalog.rs` — test double
4. `core/crates/layerfs-history/src/testing/mock_catalog.rs` — test double
5-8. Other daemon/fixture doubles

#### Step 2: Survey the existing methods

```rust
// existing method inventory
pub trait HistoryCatalog {
    // 22 required (no default)
    fn create_project(...) -> Result<...>;
    fn get_project(...) -> Result<...>;
    // ... 20 more
    
    // 10 optional (have a default that returns Unsupported)
    fn list_projects_with_stats(...) -> Result<...> {
        Err(HistoryError::Unsupported("list_projects_with_stats".to_string()))
    }
    // ... 9 more
}
```

#### Step 3: The new GC methods and their default implementations

**The 4 GC methods to add** (per spec §3):

```rust
// in core/crates/layerfs-history/src/contract/catalog.rs

impl HistoryCatalog {
    // ...existing methods...

    // GC: branch removal marking
    fn mark_branch_removed(
        &mut self,
        branch_id: &BranchId,
        removed_ms: i64,
    ) -> Result<()> {
        Err(HistoryError::Unsupported(
            "mark_branch_removed".to_string()
        ))
    }

    // GC: project soft deletion
    fn mark_project_deleted(
        &mut self,
        stack_id: &LayerStackId,
        deleted_ms: i64,
    ) -> Result<()> {
        Err(HistoryError::Unsupported(
            "mark_project_deleted".to_string()
        ))
    }

    // GC: retained roots for reachability walk
    fn get_retained_roots(
        &mut self,
    ) -> Result<RetainedRoots> {
        Err(HistoryError::Unsupported(
            "get_retained_roots".to_string()
        ))
    }

    // GC: compact-phase cleanup queries
    fn remove_unreachable_history(
        &mut self,
        keep_node_ids: &HashSet<NodeId>,
    ) -> Result<RemovalStats> {
        Err(HistoryError::Unsupported(
            "remove_unreachable_history".to_string()
        ))
    }
}
```

#### Step 4: Real backend overrides

**SQLite implementation** (`backend/sqlite/src/catalog.rs`):
```rust
impl HistoryCatalog for SqliteCatalog {
    fn mark_branch_removed(
        &mut self,
        branch_id: &BranchId,
        removed_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE branch SET removed_ms = ? WHERE id = ?",
            params![removed_ms, branch_id.to_bytes()],
        )?;
        Ok(())
    }

    fn mark_project_deleted(
        &mut self,
        stack_id: &LayerStackId,
        deleted_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE layer_stack SET deleted_ms = ? WHERE id = ?",
            params![deleted_ms, stack_id.to_bytes()],
        )?;
        Ok(())
    }

    fn get_retained_roots(&mut self) -> Result<RetainedRoots> {
        let mut roots = RetainedRoots::default();
        
        // Layers from visible stacks
        let mut stmt = self.conn.prepare(
            "SELECT id, root_id FROM layer 
             WHERE layer_stack_id IN (
                 SELECT id FROM layer_stack WHERE deleted_ms IS NULL
             )"
        )?;
        // ...populate roots.layers...
        
        // Commits from visible branches
        let mut stmt = self.conn.prepare(
            "SELECT id, root_id FROM commit 
             WHERE branch_id IN (
                 SELECT id FROM branch WHERE removed_ms IS NULL
             )"
        )?;
        // ...populate roots.commits...
        
        Ok(roots)
    }

    fn remove_unreachable_history(
        &mut self,
        keep_node_ids: &HashSet<NodeId>,
    ) -> Result<RemovalStats> {
        // Mark unreachable commits/layers for removal
        let mut stats = RemovalStats::default();
        
        // Delete unreachable commits
        let deleted_commits = self.conn.execute(
            "DELETE FROM commit 
             WHERE id NOT IN (?1) AND id IN (
                 SELECT id FROM commit WHERE removed_ms IS NOT NULL
             )",
            // ... bind keep_node_ids ...
        )?;
        stats.deleted_commits = deleted_commits;
        
        // Similar for layers, objects, etc.
        
        Ok(stats)
    }
}
```

**PostgreSQL implementation** (`backend/postgresql/src/catalog.rs`) — similar

#### Step 5: Error type constraints (no new variants)

**Existing HistoryError** (`core/crates/layerfs-history/src/contract/error.rs`):
```rust
#[derive(Debug, Clone)]
pub enum HistoryError {
    Unsupported(String),
    StaleStore,
    Conflict(String),
    NotFound(String),
    InvalidInput(String),
    Internal(String),
    // do not add new variants!
}

// two mapping points: API layer + error reporting
// api/crates/layerfs-api/src/error.rs
impl From<HistoryError> for ApiError {
    fn from(e: HistoryError) -> Self {
        match e {
            HistoryError::Unsupported(s) => ApiError::MethodNotSupported(s),
            HistoryError::StaleStore => ApiError::StaleStore,
            HistoryError::Conflict(s) => ApiError::Conflict(s),
            HistoryError::NotFound(s) => ApiError::NotFound(s),
            HistoryError::InvalidInput(s) => ApiError::InvalidInput(s),
            HistoryError::Internal(s) => ApiError::Internal(s),
            _ => ApiError::Internal("Unmapped HistoryError".to_string()),
            // note: with a new variant, the _ here degrades silently to Internal and is hard to debug!
        }
    }
}
```

**Better approach**: check at compile time
```rust
// Cargo feature: strict_error_mapping
#[cfg(feature = "strict_error_mapping")]
impl From<HistoryError> for ApiError {
    fn from(e: HistoryError) -> Self {
        match e {
            // list every variant explicitly, no _ wildcard
            HistoryError::Unsupported(s) => ...,
            // ... a new variant will fail to compile
        }
    }
}
```

#### Step 6: Maintain the rule document

**State explicitly in the spec or README**:
```markdown
## HistoryCatalog extension rules

1. **A new GC method must have an `Unsupported` default**
   - in-memory/test doubles automatically answer Unsupported
   - the real backends override it in their impl blocks with an actual implementation

2. **No new HistoryError variants**
   - the existing 6 variants cover every case
   - when finer errors are needed, reuse an existing variant with String detail
   - e.g. `Conflict("branch_removed_cannot_fork")` vs a new `BranchRemoved`

3. **Implementation checklist**
   - [ ] the new method has a default implementation in the trait
   - [ ] SQLite impl overrides it (backend/sqlite/src/catalog.rs)
   - [ ] PostgreSQL impl overrides it (backend/postgresql/src/catalog.rs)
   - [ ] test doubles need no change (they inherit the default)
   - [ ] a unit test verifies the Unsupported error code
```

### Recommended implementation order
1. Add default implementations for the 4 GC methods in `core/crates/layerfs-history/src/contract/catalog.rs`
2. Implement the SQLite version in `backend/sqlite/src/catalog.rs`
3. Implement the PostgreSQL version in `backend/postgresql/src/catalog.rs`
4. Run the existing unit tests and confirm the test doubles and in-memory implementation need no change
5. Add an integration test: verify the default Unsupported is returned correctly

---

## Integration verification checklist

| Issue | Status | Verification method |
|------|------|--------|
| **C8b** | New code | concurrent compact + write, verify Conflict is triggered |
| **N3** | Schema change | migration test: old DB → new schema, list visibility correct |
| **C9** | API contract | check the 8 impls: no change needed (default only) |

---

## Open design items (decision points)

| ID | Item | Status | Owner |
|---|---|---|---|
| D1 | BranchId incarnation encoding (identity_format vs a separate column) | proposed approach frozen | Product |
| D3 | PostgreSQL fence choice (advisory lock vs FOR UPDATE vs schema lock) | needs confirmation | Product |
| D6 | Timestamp precision (millisecond vs epoch) | propose nullable BIGINT ms | Product |
| D7 | Whether deleting the source branch is allowed | propose allow (a fork keeps what it uses) | Product |


---

# C9 In-Depth Redesign: Guided by CCP/CRP

## Root-cause analysis

### Symptoms
- adding 4 GC methods each time
- 8 implementations change at the same time: SQLite, PostgreSQL, 6 Mocks
- maintenance cost = 8x

### Root cause
**Two component principles are violated:**

1. **Violates CCP (Common Closure Principle)**
   - the trait mixes "core queries" and "GC operations"
   - the two change for different reasons (business logic vs GC implementation)
   - result: two reasons to change are forced onto one trait

2. **Violates CRP (Common Reuse Principle)**
   - SQLite + PostgreSQL (production) need the GC methods
   - Mocks (tests) do not need the GC methods
   - result: the usage patterns differ, yet Mock is forced to stub GC

## CCP/CRP solution

### Step 1: Split the trait (CCP)

```rust
// core trait (stable, rarely changes)
pub trait HistoryCatalog {
    fn list_projects(...) -> Result<...>;
    fn get_project(...) -> Result<...>;
    // ... 20+ non-GC methods (reason to change: business logic)
}

// GC extension trait (changes often, isolated)
pub trait HistoryCatalogGC: HistoryCatalog {
    fn mark_branch_removed(...) -> Result<()>;
    fn mark_project_deleted(...) -> Result<()>;
    fn remove_file_from_history(...) -> Result<()>;
    fn get_retained_roots(...) -> Result<()>;
}
```

**Benefits:**
- ✅ core trait stays stable, little churn
- ✅ GC trait changes do not affect other code
- ✅ code that changes for different reasons is isolated

### Step 2: Organize by usage (CRP)

```
core/crates/layerfs-history/src/
├── contract/
│   ├── catalog.rs              # HistoryCatalog
│   └── catalog_gc.rs           # HistoryCatalogGC ← NEW
│
├── backend/sqlite/
│   ├── catalog.rs              # impl HistoryCatalog
│   └── catalog_gc.rs           # impl HistoryCatalogGC ← NEW
│
├── backend/postgresql/
│   ├── catalog.rs              # impl HistoryCatalog
│   └── catalog_gc.rs           # impl HistoryCatalogGC ← NEW
│
└── testing/
    ├── mock_catalog.rs         # impl HistoryCatalog (NO GC)
    ├── daemon_mock.rs          # impl HistoryCatalog (NO GC)
    └── fixture_catalog.rs      # impl HistoryCatalog (NO GC)
```

**Key:** Mock implements only `HistoryCatalog` and automatically gets the `Unsupported` default impl of `HistoryCatalogGC`

### Step 3: Effect comparison

| Approach | Change points | Code size | Reviews |
|------|------|------|--------|
| ❌ Current | 8 | 32 lines | 8 |
| ✅ CCP/CRP | 2 | 8 lines | 2 |
| **Improvement** | **-75%** | **-75%** | **-75%** |

### Step 4: Against the SOLID principles

| Principle | Problem | Resolution |
|------|------|------|
| **S** | trait has two reasons to change | split into two traits |
| **O** | a new method forces changes to 8 impls | open for extension (new trait), closed for modification |
| **I** | clients are forced to depend on GC | separate the interfaces; depend only on what is needed |
| **L** | Mock is forced to stub GC | Mock does not need to implement HistoryCatalogGC |

## Implementation steps

### Phase 1: Add the GC trait

1. Create `core/crates/layerfs-history/src/contract/catalog_gc.rs`
2. Define the `HistoryCatalogGC: HistoryCatalog` trait (4 methods)
3. Implement the default impl (returns Unsupported)

### Phase 2: SQLite & PostgreSQL impl

1. Create `backend/sqlite/src/catalog_gc.rs`
   - implement the 4 GC methods

2. Create `backend/postgresql/src/catalog_gc.rs`
   - implement the 4 GC methods

### Phase 3: Mock (no change needed!)

```rust
impl HistoryCatalog for MockCatalog {
    // only the 20+ core methods
    fn list_projects(...) -> Result<...> { /* ... */ }
    // ...
}

// automatically gets the default impl of HistoryCatalogGC (Unsupported)
// if a test needs GC, use the real SQLite:
let catalog = Arc::new(SqliteCatalog::memory());
let gc_ops: Arc<dyn HistoryCatalogGC> = catalog.clone();
```

## Summary

**C9 is not "no new methods allowed" but "let component principles guide the design":**

- **CCP:** separate code that changes for different reasons
  - core queries ← business-logic changes
  - GC operations ← GC implementation iterations
  
- **CRP:** organize implementations by usage
  - production (SQLite + PostgreSQL) ← needs GC
  - tests (Mock) ← does not need GC

**Result:** 8 change points → 2 change points; the cost of a new GC method goes from Θ(n) to O(1)

---

# C8b In-Depth Design Guidance: Write Fence Implementation

## Root cause

### How silent data loss happens

```
Timeline:
  T1: Writer A opens the Store fd → generation = 5
      writer_gen_at_acquire = 5

  T2: Compact runs
      - creates a new Store file
      - prepares the rename swap
      - catalog_incarnation goes from 5 → 6

  T3: Rename completes
      - the old inode (dev=1, ino=100) is retired (possibly deleted or hidden)
      - the new inode (dev=1, ino=101) becomes live

  T4: Writer A keeps writing
      - executes write() on the old fd
      - the old inode is already retired, so the data lands nowhere
      - the new Store cannot observe these writes ❌ SILENT DATA LOSS
      - a second Writer B on the new Store cannot see A's data
```

### Why the existing inode check is not enough

```rust
// the existing install_check (the second of the two gates)
pub fn install_check(dev: u64, ino: u64, expected: (u64, u64)) -> Result<()> {
    let actual = get_inode();
    if actual != expected {
        Err(StoreError::Conflict("inode mismatch"))
    }
}
```

**Problem:** this check runs **after install completes**, but Writer A may already have acquired the write lock before install (on the old incarnation)

```
install_check check points:
  ├─ verifies the new inode is correct ✅
  └─ but cannot stop a write lock from the old incarnation ❌
```

## Incarnation Fence solution

### Core mechanism

```
Store tracks the incarnation:
  incarnation: u64  ← incremented on every compact install

Write path:
  1. Writer acquires the write lock
     - records the current incarnation
     - writer_gen_at_acquire = 5

  2. Writer performs the write
     - validated at the admission check
     - if writer_gen_at_acquire < current_incarnation:
         return Err(Conflict)
     - otherwise the write is allowed

  3. Compact runs
     - incarnation goes from 5 → 6
     - every writer lock from an older incarnation is invalidated

  4. A write from the old incarnation is rejected
     - a Conflict error is returned to the writer
     - the data does not land, which avoids silent loss ✅
```

### The two implementation gates

```
Write flow:

First gate (admission) - new in C8b
  lock → is_valid_for_compact()
       ├─ reads the current incarnation
       └─ checks writer_gen_at_acquire vs current
       
Second gate (install) - existing, retained
  swap → install_check()
       └─ verifies the new inode is correct

Both gates must pass before the write lands ✅
```

### Code location list

```
Files to change:

1. store/identity.rs
   - add the StoreGeneration struct
   - persist incarnation and inode

2. store/sqlite/writer_admission.rs (new)
   - writer_admission_check() logic
   - check writer_gen vs current_incarnation

3. backend/sqlite/lifecycle/write_path.rs
   - modify the write entry point
   - call writer_admission_check()

4. persistence/history/catalog.rs
   - bump the incarnation on compact install
   - incarnation += 1

5. Tests
   - concurrent_compact_and_write_test
   - verify Conflict is triggered correctly
```

## Implementation order (5 steps)

### Step 1: Define the StoreGeneration struct

```rust
// store/identity.rs

pub struct StoreGeneration {
    /// Current incarnation number
    pub incarnation: u64,
    
    /// Current inode of Store file
    pub file_inode: (u32, u64),  // (dev, ino)
    
    /// Last successful write generation
    pub last_writer_generation: u64,
    
    /// Timestamp of last incarnation bump
    pub bumped_at: SystemTime,
}

impl StoreGeneration {
    pub fn new() -> Self {
        Self {
            incarnation: 1,
            file_inode: (0, 0),
            last_writer_generation: 1,
            bumped_at: SystemTime::now(),
        }
    }
}
```

### Step 2: Create writer_admission.rs

```rust
// store/sqlite/writer_admission.rs

pub fn writer_admission_check(
    store_id: &StoreId,
    current_incarnation: u64,
    writer_gen_at_acquire: u64,
) -> Result<(), StoreError> {
    if writer_gen_at_acquire < current_incarnation {
        // Writer acquired at old incarnation
        // Compact has bumped incarnation since then
        return Err(StoreError::Conflict(format!(
            "writer incarnation mismatch: acquired {}, current {}",
            writer_gen_at_acquire, current_incarnation
        )));
    }
    Ok(())
}
```

### Step 3: Change write_path.rs

```rust
// backend/sqlite/lifecycle/write_path.rs

pub fn execute_write(
    store: &Store,
    writer_gen: u64,
    operation: WriteOp,
) -> Result<()> {
    // first gate: admission check
    writer_admission_check(
        &store.id,
        store.generation.incarnation,
        writer_gen,
    )?;
    
    // write operation
    store.conn.execute(&operation.sql)?;
    
    // second gate: install check runs at commit (existing)
    Ok(())
}
```

### Step 4: Change catalog.rs

```rust
// persistence/history/catalog.rs

pub fn install_new_store(
    new_inode: (u32, u64),
) -> Result<()> {
    // second gate: install_check (existing)
    install_check(new_inode)?;
    
    // first gate enhanced: bump the incarnation
    let generation = self.store.generation.clone();
    generation.incarnation += 1;  // 5 → 6
    generation.file_inode = new_inode;
    generation.bumped_at = SystemTime::now();
    
    // persist to Store metadata
    self.store.set_generation(generation)?;
    
    Ok(())
}
```

### Step 5: Write the concurrency test

```rust
// tests/concurrent_compact_and_write_test.rs

#[test]
fn test_concurrent_compact_and_preswap_write() {
    let store = Store::new_memory();
    let writer_gen_at_acquire = store.generation.incarnation;
    
    // start compact
    let compact_thread = spawn(|| {
        store.compact();  // incarnation 5 → 6
    });
    
    // Writer attempts a write while compact is in progress
    let write_result = spawn(move || {
        // this write holds a lock acquired at the old incarnation 5
        store.execute_write(writer_gen_at_acquire, operation)
    });
    
    compact_thread.join().unwrap();
    
    // expected: a Conflict error
    assert_eq!(write_result.join().unwrap().err(), 
               Some(StoreError::Conflict(...)));
}
```

## Verification checklist

- [ ] StoreGeneration struct defined correctly
- [ ] writer_admission_check() logic correct
- [ ] write_path.rs calls the admission check
- [ ] catalog.rs bumps the incarnation on install
- [ ] concurrency test: compact + preswap write → Conflict ✅
- [ ] concurrency test: compact + postswap write → success ✅
- [ ] performance: admission check under 1μs
- [ ] error messages are clear and easy to debug

---

# N3 In-Depth Design Guidance: Schema Soft Delete

## Root cause

### The limitation of having no visibility column

```
Current schema (v3.5):
  layer_stack(id, stack_name, ...)
  branch(id, branch_name, layer_stack_id, ...)
  
Problem:
  - delete a Project → DELETE layer_stack row
  - but if a foreign key references it, DELETE fails ❌
  - or after the delete, the Name cannot be reused (unique constraint conflict)
```

### What soft delete requires

```
Requirement 1: Name reuse (N3)
  Problem: delete project_A, then want to recreate a project with the same name
  Current: after DELETE the name is reusable ✓
  But: if DELETE fails (foreign-key lock), the name stays occupied forever ✗

Requirement 2: keep history
  Problem: after a Branch/Project is deleted, its history must still be reachable
  Current: unreachable after DELETE ✗
  Goal: mark as deleted but keep the row ✓

Requirement 3: GC cleanup
  Problem: GC must tell "active" from "deleted"
  Current: no visibility marker ✗
  Goal: add a deleted_ms column for GC ✓
```

## Schema migration plan (v3.5 → v3.6)

### New visibility columns

```sql
-- new columns (NULL = active, non-NULL = deleted)
ALTER TABLE layer_stack ADD COLUMN deleted_ms BIGINT DEFAULT NULL;
ALTER TABLE branch ADD COLUMN removed_ms BIGINT DEFAULT NULL;

-- index optimization (partial index covering active rows only)
CREATE UNIQUE INDEX idx_stack_name_active 
  ON layer_stack(name) WHERE deleted_ms IS NULL;

CREATE UNIQUE INDEX idx_branch_name_active 
  ON branch(branch_name, layer_stack_id) WHERE removed_ms IS NULL;
```

### Implicit conditions on queries

```rust
// every query filters deleted rows automatically

fn list_projects(&mut self, stack_id: ...) -> Result<Vec<LayerStack>> {
    self.conn.query("
        SELECT * FROM layer_stack 
        WHERE id = ? 
          AND deleted_ms IS NULL  ← filtered automatically
    ", ...)
}

fn list_branches(&mut self, stack_id: ...) -> Result<Vec<Branch>> {
    self.conn.query("
        SELECT * FROM branch 
        WHERE layer_stack_id = ? 
          AND removed_ms IS NULL  ← filtered automatically
    ", ...)
}
```

### Migration flow

```
v3.5 Database:
  layer_stack(id, name, ...)
  branch(id, name, stack_id, ...)
  
On-open migration:
  1. check the schema version
  2. if v3.5:
     ├─ ALTER TABLE to add deleted_ms / removed_ms
     ├─ create the partial indexes
     ├─ set schema_version = 3.6
     └─ commit

  3. later connections use the new schema automatically ✓

v3.6 Database:
  layer_stack(id, name, ..., deleted_ms)
  branch(id, name, stack_id, ..., removed_ms)
```

## Implementation steps (4 steps)

### Step 1: Define the migration script

```rust
// persistence/backend/sqlite/migrations.rs

pub fn migrate_3_5_to_3_6(conn: &mut Connection) -> Result<()> {
    conn.execute_batch("
        -- add the soft delete columns
        ALTER TABLE layer_stack ADD COLUMN deleted_ms BIGINT DEFAULT NULL;
        ALTER TABLE branch ADD COLUMN removed_ms BIGINT DEFAULT NULL;
        
        -- create the partial indexes (active rows only)
        CREATE UNIQUE INDEX IF NOT EXISTS idx_stack_name_active 
          ON layer_stack(name) WHERE deleted_ms IS NULL;
        
        CREATE UNIQUE INDEX IF NOT EXISTS idx_branch_name_active 
          ON branch(branch_name, layer_stack_id) WHERE removed_ms IS NULL;
        
        -- update the schema version
        PRAGMA user_version = 3.6;
    ")?;
    
    Ok(())
}
```

### Step 2: Change every query

```rust
// persistence/history/catalog.rs

impl HistoryCatalog for SqliteCatalog {
    fn list_projects(&mut self, stack_id: &LayerStackId) -> Result<Vec<LayerStack>> {
        self.conn.query(
            "SELECT * FROM layer_stack WHERE id = ? AND deleted_ms IS NULL",
            [stack_id],
        )
    }
    
    fn list_branches(&mut self, stack_id: &LayerStackId) -> Result<Vec<Branch>> {
        self.conn.query(
            "SELECT * FROM branch WHERE layer_stack_id = ? AND removed_ms IS NULL",
            [stack_id],
        )
    }
}
```

### Step 3: Implement the soft delete operations

```rust
// persistence/history/catalog_gc.rs

impl HistoryCatalogGC for SqliteCatalog {
    fn mark_project_deleted(&mut self, stack_id: &LayerStackId, deleted_ms: i64) 
        -> Result<()> {
        // soft delete: mark deleted_ms instead of DELETE
        self.conn.execute(
            "UPDATE layer_stack SET deleted_ms = ? WHERE id = ?",
            (deleted_ms, stack_id),
        )?;
        Ok(())
    }
    
    fn mark_branch_removed(&mut self, branch_id: &BranchId, removed_ms: i64) 
        -> Result<()> {
        self.conn.execute(
            "UPDATE branch SET removed_ms = ? WHERE id = ?",
            (removed_ms, branch_id),
        )?;
        Ok(())
    }
}
```

### Step 4: Verify name reuse

```rust
// tests/soft_delete_name_reuse_test.rs

#[test]
fn test_name_reuse_after_soft_delete() {
    let mut catalog = SqliteCatalog::new_memory();
    
    // 1. create project_A
    let stack_a = catalog.create_project("project_A").unwrap();
    
    // 2. soft delete project_A
    let deleted_ms = current_timestamp_ms();
    catalog.mark_project_deleted(&stack_a.id, deleted_ms).unwrap();
    
    // 3. verify it is no longer visible
    let projects = catalog.list_projects().unwrap();
    assert!(!projects.iter().any(|p| p.name == "project_A"));
    
    // 4. recreate a project with the same name ✓
    let stack_b = catalog.create_project("project_A").unwrap();
    assert_ne!(stack_a.id, stack_b.id);
    
    // 5. verify the new project is visible and the old one still is not
    let projects = catalog.list_projects().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, stack_b.id);
}
```

## Verification checklist

- [ ] migration script creates the columns and indexes correctly
- [ ] every query adds the deleted_ms / removed_ms filter
- [ ] partial indexes are used (EXPLAIN QUERY PLAN)
- [ ] soft delete operations update the timestamps correctly
- [ ] name reuse test passes
- [ ] performance: soft delete under 100ms
- [ ] PostgreSQL implementation equivalent complete
- [ ] database upgrade does not lose existing data

