# Resolution Plan for the Three Critical LayerFS GC Design Issues

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Implementation guidance derived from `LayerFS-GC-spec.md`, which is
authoritative: where this plan and the spec differ, the spec wins. Code paths
named here are planning pointers at `layerfs_v2_0_0`; confirm them in the source
before changing it.

## Background

Based on Chapter 5 "Contracts" of `LayerFS-GC-spec.md` (and its §1.2 open questions), the three issues sit at different implementation layers:

---

## C8b: SQLite Writer Fence (pre-swap writer fence)

### Problem statement
When compact performs the Store file rename swap, a pre-swap writer (a connection whose fd was opened on the old Store file) can still write to the retired inode, and the new Store does not see those writes. The result is **silent data loss**.

The `(dev, ino)` check at install is not sufficient: it checks the path the compactor installs into, not the fds other connections already hold.

### Root cause
- `rename(staged, live)` points the Store path at a new inode; an fd opened earlier keeps the old inode alive and writable.
- The Store side has no `locking_mode=EXCLUSIVE`, so nothing at the SQLite level refuses the old fd.
- Everything the old-fd writer reads comes from the retired file, including `history_meta.catalog_incarnation`, which never changes there. A fence that compares incarnations therefore never fires for this writer.

### Specification decision (spec §5 C8b)
1. **At the start of every write transaction**, compare the Store path's current `(dev, ino)` (`stat` of the path) with the open file's `(dev, ino)` (`fstat` of the connection's own fd).
2. **On mismatch, return the existing `Conflict` refusal** before anything is written; no new rejection code, no retry, no automatic reopen.
3. **No new generation field or struct.** C8 forbids a second generation field; `history_meta.catalog_incarnation` stays the only Store generation, bumped at install as C8 requires. It serves the reader fence (C8), not this one.

### Implementation path

#### Step 1: Where the check sits
```
write transaction (every one):
  writer lane acquired (lane/writer_admission.rs, Linux)
    ↓
  BEGIN IMMEDIATE
    ↓
  fence: stat(store_path).(dev, ino) == identity of the connection's file ?   ← C8b
    ├─ equal    → run the transaction body
    └─ differs  → ROLLBACK, return Conflict (nothing written)
    ↓
  COMMIT
```
The compactor holds the old file's write lock (the writer lane plus its own
`BEGIN IMMEDIATE`) from `snapshot` through the rename. A writer's check
therefore either completes before the snapshot (its transaction is in the copy)
or runs after the rename (and sees the mismatch); it cannot pass and then be
overtaken by the swap.

**Key code locations** (at `layerfs_v2_0_0`):
- `engine/crates/layerfs-persistence/src/backend/sqlite/session/transaction.rs` — the write-transaction begin (`BEGIN IMMEDIATE`, after `writer_admission::Writer::acquire`); add the check before the body runs
- `engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs` — record the opened file's identity when the connection opens
- `control/crates/layerfs-store/src/sqlite/transfer/` — `Install` keeps its existing `(catalog_id, catalog_incarnation, dev, ino)` check

#### Step 2: Where the file identity comes from
- Take it from SQLite's own fd (`sqlite3_file_control` with `SQLITE_FCNTL_FILE_POINTER`, then `fstat`), or `stat` the path once right after open, while the lease excludes compact.
- **Never open a second fd to the main file to `fstat` it**: closing any fd on that inode drops every POSIX lock the process holds on it, including SQLite's live locks (the comment in `lane/writer_admission.rs` records this).

#### Step 3: Test
- Concurrent compact and write: a connection opened before the swap gets `Conflict` on its next write transaction, and neither file changes.
- A connection opened after the swap writes normally.

### Recommended implementation order
1. Record the file identity at connection open (`session/connection.rs`)
2. Add the `(dev, ino)` comparison at write-transaction start (`session/transaction.rs`), returning `Conflict`
3. Make the compactor hold the old file's write lock from `snapshot` through the rename
4. Test: concurrent write + compact scenario

---

## N3: Soft Delete Representation (soft delete visibility)

### Problem statement
The `branch` and `layer_stack` tables have no visibility column, so soft deletion (logical deletion with the row kept physically) cannot be implemented.

### Specification decision (spec §4 N3, §5 C6, §1.2 Q1)
In **one Store schema change**:
1. Add a `deleted_ms` column to `layer_stack` (nullable timestamp)
2. Add a `removed_ms` column to `branch` (nullable timestamp)
3. **Keep the unique indexes full**: `layer_stack_names ON layer_stack(name)` and `branch_names ON branch(layer_stack_id, name)` stay as they are. A removed row keeps its name, so the name is never handed out again (Q1 = A: terminal tombstone).
4. A create whose name is held by a removed row returns **`NameRetired`**; one held by a live row returns **`NameTaken`** — decided 2026-11-13, so neither case leaks out as `Integrity` from the unique index.

Partial unique indexes (restricted to rows whose `removed_ms` / `deleted_ms` is null) are **not** used: they would free the name, and a recreated Branch would reproduce `BranchId::named = blake3(stack ‖ name)` and take over the old history (spec §5 C6, `03-decision-record.md` §2).

The version step for this change is decided by layerfs-project#42 — **and it was decided on 2026-11-13: no step.** The columns land as an additive migration under the same schema identity (`REBUILD.md kind 2`), and 1.0 Stores are converted offline by `tools/store_blocks.py migrate`. At `layerfs_v2_0_0` the Store records `store_policy.schema_version` (`CHECK(schema_version=1)`) and `history_meta.identity_format = 3`; this plan does not invent a version number.

### Implementation path

#### Step 1: Schema change (both backends together)

**SQLite** (`sql/sqlite/history.sql`):
```sql
-- Store schema change (additive migration, same version - decided 2026-11-13)
ALTER TABLE layer_stack ADD COLUMN deleted_ms INTEGER DEFAULT NULL;
ALTER TABLE branch ADD COLUMN removed_ms INTEGER DEFAULT NULL;

-- layer_stack_names and branch_names are unchanged: full unique indexes
```

**PostgreSQL** (`sql/postgresql/schema.sql`):
```sql
ALTER TABLE layer_stack ADD COLUMN deleted_ms BIGINT DEFAULT NULL;
ALTER TABLE branch ADD COLUMN removed_ms BIGINT DEFAULT NULL;

-- layer_stack_names and branch_names are unchanged: full unique indexes
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

**At create** (the name check runs on every row, removed or not):
```sql
SELECT removed_ms FROM branch WHERE layer_stack_id = ? AND name = ?;
-- no row                    → create
-- row, removed_ms NOT NULL  → NameRetired
-- row, removed_ms IS NULL   → NameTaken
```
The same rule applies to `layer_stack.name` and `deleted_ms`. The full unique index is the backstop; the explicit checks make the refusal `NameRetired` / `NameTaken` rather than a constraint failure.

#### Step 3: Turn delete operations into UPDATE

**Original code** (`project/remove.rs`, `branch/remove.rs`):
```rust
// old mode: delete the row
DELETE FROM branch WHERE id = ?
DELETE FROM layer_stack WHERE id = ?
```

**New code**:
```rust
// new mode: record the deletion time; the row and its name stay
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

pub fn migrate_add_visibility(conn: &mut Connection) -> Result<()> {
    // 1. add the columns; indexes are unchanged
    conn.execute_batch("
        ALTER TABLE layer_stack ADD COLUMN deleted_ms INTEGER DEFAULT NULL;
        ALTER TABLE branch ADD COLUMN removed_ms INTEGER DEFAULT NULL;
    ")?;

    // 2. record the version step layerfs-project#42 decides
    //    (store_policy.schema_version is CHECK(=1) today, so the step also
    //    changes that constraint)

    Ok(())
}
```

### Recommended implementation order
1. Change `sql/sqlite/history.sql` and `sql/postgresql/schema.sql`
2. Create `persistence/history/migrate.rs` and implement the migration logic
3. Change `persistence/history/catalog.rs` to add the visibility condition to every query
4. Add the create-time `NameRetired` / `NameTaken` checks for Branch and project names
5. Change `project/remove.rs` and `branch/remove.rs` to use UPDATE
6. Change `api/project/remove.rs` and `api/branch/remove.rs` to `delete` semantics
7. Migration test: old database → new schema, verify visibility is correct and removed names stay retired

---

## C9: HistoryCatalog Constraints

### Problem statement
The `HistoryCatalog` trait has 32 methods and 8 implementations:
- 1 real implementation (SQLite + PostgreSQL)
- 6 test doubles (daemon/mock)
- 1 in-memory implementation

If a new GC method has no default body, all 8 impls must change together, so **maintenance cost explodes**.

Adding a new `HistoryError` variant is likewise forbidden, because both mapping points use the `_ => "Failed"` wildcard, so a new variant degrades silently.

### Specification decision (spec §5 C9)
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

**The GC methods to add** (per spec §5 C9; names are illustrative):

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
}
```

No catalog method deletes history rows: compaction copies retained rows into a
fresh staged Store (spec §7.2) and never deletes in the live Store, so a
`remove_unreachable_history`-style `DELETE` has no place here.

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
1. Add default implementations for the GC methods in `core/crates/layerfs-history/src/contract/catalog.rs`
2. Implement the SQLite version in `backend/sqlite/src/catalog.rs`
3. Implement the PostgreSQL version in `backend/postgresql/src/catalog.rs`
4. Run the existing unit tests and confirm the test doubles and in-memory implementation need no change
5. Add an integration test: verify the default Unsupported is returned correctly

---

## Integration verification checklist

| Issue | Status | Verification method |
|------|------|--------|
| **C8b** | New code | concurrent compact + write, verify Conflict is triggered |
| **N3** | Schema change | migration test: old DB → new schema, list visibility correct, removed names refused `NameRetired` |
| **C9** | API contract | check the 8 impls: no change needed (default only) |

---

## Open design items (decision points)

| ID | Item | Status | Owner |
|---|---|---|---|
| D1 | BranchId incarnation encoding | superseded by Q1 = A: `BranchId::named` is unchanged, no incarnation column | — |
| D3 | PostgreSQL fence choice (advisory lock vs FOR UPDATE vs schema lock) | needs confirmation | Product |
| D6 | Timestamp precision (millisecond vs epoch) | propose nullable BIGINT ms | Product |
| D7 | Whether deleting the source branch is allowed | propose allow (a fork keeps what it uses) | Product |

---

# Considered alternative, not adopted: a separate `HistoryCatalogGC` trait

A CCP/CRP-style split was considered: keep `HistoryCatalog` for the non-GC
methods and put the GC methods on a new `HistoryCatalogGC: HistoryCatalog`
trait, implemented only by the real backend, so test doubles never see GC
methods.

It is **not adopted**. The spec (§5 C9) requires each new GC method to sit on
`HistoryCatalog` itself with a default `Unsupported` body; that already leaves
the six daemon test doubles and the memory implementation unchanged, which is
the benefit the split was meant to buy. A second trait would add a new public
contract surface and a second dispatch path for the same Store. Adopting it
would be a change to C9 and needs owner approval.

---

# C8b In-Depth Design Guidance: Write Fence Implementation

## Root cause

### How silent data loss happens

```
Timeline:
  T1: Writer A's connection opens the Store path → inode (dev=1, ino=100);
      its fd refers to ino 100

  T2: Compact runs
      - builds the staged Store as a fresh file (ino 101) with only retained rows
      - verifies it; catalog_incarnation is 6 in the staged file, 5 in the old one
      - renames staged → Store path

  T3: Rename completes
      - the path now names ino 101
      - ino 100 has no name, but stays alive and writable while A's fd is open

  T4: Writer A begins a write transaction
      - reads history_meta through its fd → catalog_incarnation = 5, unchanged
      - its writes land in ino 100
      - the new Store never sees them ❌ SILENT DATA LOSS
```

### Why comparing incarnations cannot catch it

A fence of the form "writer's incarnation < current incarnation → refuse" must
read the current incarnation from somewhere. Through Writer A's fd that is the
retired file, where `catalog_incarnation` is still 5: the comparison is 5 vs 5
and the write is admitted. To see 6, the writer would have to open the path
again, which is the file-identity check below. This is also why C8 forbids a
second generation field: it would live in the same retired file.

### Why the install-time `(dev, ino)` check is not enough

`Install` checks `(catalog_id, catalog_incarnation, dev, ino)` of the path it
replaces. That protects the swap itself; it says nothing about fds that other
connections already hold on the old inode.

## The fence

### Core mechanism

```
At the start of every write transaction (after the writer lane and BEGIN IMMEDIATE):
  path_id = stat(store_path)            → (dev, ino) the path names now
  file_id = identity of this connection's own file (fstat of SQLite's fd)
  if path_id != file_id:
      ROLLBACK
      return Conflict                   ← existing refusal, no new code
  else:
      run the transaction body
```

- One `stat` per write transaction; reads are unaffected (they are fenced by
  C8's `(catalog_id, incarnation)` session check, plus `(dev, ino)` at session
  entry).
- **Ordering.** The compactor holds the old file's write lock (writer lane plus
  `BEGIN IMMEDIATE`) from `snapshot` through the rename. A writer therefore
  either commits before the snapshot (its write is in the copy) or begins after
  the rename (and its check fails). The check cannot pass and then be overtaken
  by the swap.
- **No retry, no reopen.** The writer receives `Conflict`; reopening the Store is
  the caller's decision, and it then meets C8's reader fence like any session.

### Where the file identity comes from

- From SQLite's own fd: `sqlite3_file_control(..., SQLITE_FCNTL_FILE_POINTER, ...)`
  then `fstat`; or a `stat` of the path right after open, taken while the lease
  excludes compact.
- **Never** by opening a second fd to the main file: closing any fd on that inode
  drops every POSIX lock the process holds on it, including SQLite's live locks
  (`engine/crates/layerfs-persistence/src/backend/sqlite/lane/writer_admission.rs`
  documents this hazard).

### Code location list

```
Files to change (layerfs_v2_0_0 paths):

1. engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs
   - record the opened file's (dev, ino) when the connection opens

2. engine/crates/layerfs-persistence/src/backend/sqlite/session/transaction.rs
   - after Writer::acquire and BEGIN IMMEDIATE, before the body:
     compare stat(path) with the recorded identity; mismatch → ROLLBACK, Conflict

3. control/crates/layerfs-store/src/sqlite/transfer/
   - compact's snapshot..install holds the old file's write lock
   - Install keeps its (catalog_id, catalog_incarnation, dev, ino) check

Not changed: history_meta and every identity record. No StoreGeneration
struct, no persisted writer generation, no new table.
```

## Implementation order

1. Record the file identity at connection open
2. Add the write-transaction check, returning `Conflict`
3. Hold the old file's write lock from `snapshot` through the rename in compact
4. Write the concurrency test

### Concurrency test (sketch)

```rust
// tests/ (persistence or store crate; bounded waits only)

#[test]
fn pre_swap_writer_gets_conflict() {
    // 1. open Store S at path P; keep connection W open
    // 2. build a replacement Store file R (same identity, incarnation + 1)
    //    and rename R over P, as compact's install does
    // 3. W begins a write transaction → Conflict; neither file changed
    // 4. a connection opened on P after the rename writes normally
}
```

## Verification checklist

- [ ] file identity recorded at open without opening a second fd to the main file
- [ ] every write transaction compares `stat(path)` with that identity
- [ ] concurrency test: compact + pre-swap write → `Conflict`, no bytes written ✅
- [ ] concurrency test: compact + post-swap write → success ✅
- [ ] the compactor holds the old file's write lock from `snapshot` through the rename
- [ ] no new generation field, struct or table (C8)

---

# N3 In-Depth Design Guidance: Schema Soft Delete

## Root cause

### The limitation of having no visibility column

```
Current schema (layerfs_v2_0_0, sql/sqlite/history.sql):
  layer_stack(..., name)        UNIQUE INDEX layer_stack_names ON layer_stack(name)
  branch(..., layer_stack_id, name)
                                UNIQUE INDEX branch_names ON branch(layer_stack_id, name)

Problem:
  - deleting a project or Branch today deletes its row
  - the row is what history and immutable ids resolve through
  - so a hard delete either fails on references or loses history
```

### What soft delete requires

```
Requirement 1: keep history (spec §3 I5)
  After a Branch/project is removed, its Commits stay reachable by immutable id.
  Goal: mark the row removed, keep it.

Requirement 2: removed names stay retired (spec §1.2 Q1 = A, §5 C6)
  BranchId::named = blake3(stack ‖ name), so a recreated name would reproduce
  the old BranchId and take over the old history.
  Goal: the full unique index stays; a create naming a removed row → NameRetired.

Requirement 3: GC tells active from removed
  Listing and the reachability root set must distinguish live rows.
  Goal: a deleted_ms / removed_ms column.
```

## Schema change (additive migration, same version - decided 2026-11-13)

### New visibility columns

```sql
-- new columns (NULL = active, non-NULL = removed)
ALTER TABLE layer_stack ADD COLUMN deleted_ms INTEGER DEFAULT NULL;
ALTER TABLE branch ADD COLUMN removed_ms INTEGER DEFAULT NULL;

-- indexes: unchanged. layer_stack_names and branch_names stay full unique
-- indexes, so a removed row keeps holding its name.
```

### Implicit conditions on queries

```rust
// every listing filters removed rows

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

The name check at create is the one query that does **not** filter: it must see
removed rows to answer `NameRetired`.

### Migration flow

```
Existing Store:
  layer_stack(id, name, ...)
  branch(id, name, stack_id, ...)

On-open migration (Store schema change):
  1. check the recorded schema version
  2. if it predates the change:
     ├─ ALTER TABLE to add deleted_ms / removed_ms
     ├─ leave layer_stack_names / branch_names as they are
     ├─ record the version step layerfs-project#42 decides
     └─ commit

  3. later connections use the new schema ✓
```

## Implementation steps (4 steps)

### Step 1: Define the migration script

```rust
// persistence/backend/sqlite/migrations.rs

pub fn migrate_add_visibility(conn: &mut Connection) -> Result<()> {
    conn.execute_batch("
        -- add the soft delete columns; indexes are unchanged
        ALTER TABLE layer_stack ADD COLUMN deleted_ms INTEGER DEFAULT NULL;
        ALTER TABLE branch ADD COLUMN removed_ms INTEGER DEFAULT NULL;
    ")?;
    // same version: the recorded schema_source/schema_definition change, the number does not
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
// persistence/history/catalog.rs — overrides of the default-Unsupported
// HistoryCatalog methods (spec §5 C9)

impl HistoryCatalog for SqliteCatalog {
    fn mark_project_deleted(&mut self, stack_id: &LayerStackId, deleted_ms: i64) 
        -> Result<()> {
        // soft delete: mark deleted_ms instead of DELETE; the name stays held
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

### Step 4: Verify that removed names stay retired

```rust
// tests/soft_delete_name_retired_test.rs

#[test]
fn removed_name_is_retired() {
    let mut catalog = SqliteCatalog::new_memory();
    
    // 1. create project_A
    let stack_a = catalog.create_project("project_A").unwrap();
    
    // 2. soft delete project_A
    let deleted_ms = current_timestamp_ms();
    catalog.mark_project_deleted(&stack_a.id, deleted_ms).unwrap();
    
    // 3. verify it is no longer listed
    let projects = catalog.list_projects().unwrap();
    assert!(!projects.iter().any(|p| p.name == "project_A"));
    
    // 4. a create with the same name is refused NameRetired
    assert!(matches!(catalog.create_project("project_A"), Err(e) if e.is_name_retired()));
    
    // 5. the removed project is still resolvable by its immutable id
    assert!(catalog.get_project_by_id(&stack_a.id).is_ok());
}
```

The same test runs for Branches: `branch.remove` then a same-name create →
`NameRetired`, and the old `BranchId` still resolves to the removed Branch.

## Verification checklist

- [ ] migration adds the columns; `layer_stack_names` and `branch_names` stay full unique indexes
- [ ] every listing adds the deleted_ms / removed_ms filter; the create-time name check does not
- [ ] soft delete operations update the timestamps correctly
- [ ] a removed project or Branch name is refused `NameRetired` on create
- [ ] removed rows still resolve by immutable id
- [ ] PostgreSQL implementation equivalent complete
- [ ] database upgrade does not lose existing data
