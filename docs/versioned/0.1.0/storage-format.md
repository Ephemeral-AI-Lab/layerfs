# LayerFS 0.1.0 storage format

> **Status:** Release-candidate storage contract, normative for the proposed
> LayerFS 0.1.0 Store.

## File and connection

A Store is exactly one ordinary SQLite file, conventionally named
`store.sqlite`. LayerFS uses portable SQLite APIs and contains no
filesystem-specific implementation.

```text
application_id=0x4c46534c
user_version=4
page_size=65536
foreign_keys=ON
journal_mode=MEMORY
synchronous=OFF
temp_store=MEMORY
cache_size=-32768
cache_spill=OFF
mmap_size=0
threads=0
locking_mode=EXCLUSIVE
busy_timeout=5000ms
```

One `LayerStackStore` owns one long-lived SQLite connection behind one
application mutex. Create refuses an existing file. Connect requires an
existing regular file and verifies the complete schema before use. SQLite lock
failure is `StoreBusy`.

The acknowledgement boundary is a committed transaction immediately readable
from the same live local Store process. This profile preserves statement and
transaction rollback while the process is alive. It does not provide
process-crash, operating-system-crash, power-loss, or recovery durability.

Normal operation produces no WAL, shared-memory, persistent rollback-journal,
or ownership sidecar. It performs no database-file sync, directory sync,
checkpoint, vacuum, or attached database operation.

## Schema

The schema has exactly five `STRICT` tables and twenty columns:

```text
objects(2)
commits(4)
branches(5)
layer_stacks(3)
layers(6)
```

```sql
CREATE TABLE objects (
    object_id BLOB PRIMARY KEY CHECK (length(object_id) = 32),
    bytes BLOB NOT NULL
) STRICT;

CREATE TABLE commits (
    commit_id BLOB PRIMARY KEY CHECK (length(commit_id) = 33),
    root_id BLOB NOT NULL CHECK (length(root_id) = 32)
        REFERENCES objects(object_id),
    parent_commit_id BLOB
        CHECK (parent_commit_id IS NULL OR length(parent_commit_id) = 33)
        REFERENCES commits(commit_id),
    base_layer_id BLOB NOT NULL CHECK (length(base_layer_id) = 33)
        REFERENCES layers(layer_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE branches (
    branch_id BLOB PRIMARY KEY CHECK (length(branch_id) = 17),
    layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 17)
        REFERENCES layer_stacks(layer_stack_id) DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]'),
    base_layer_id BLOB NOT NULL CHECK (length(base_layer_id) = 33),
    head_commit_id BLOB
        CHECK (head_commit_id IS NULL OR length(head_commit_id) = 33)
        REFERENCES commits(commit_id),
    FOREIGN KEY (layer_stack_id, base_layer_id)
        REFERENCES layers(layer_stack_id, layer_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE layer_stacks (
    layer_stack_id BLOB PRIMARY KEY CHECK (length(layer_stack_id) = 17),
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]'),
    head_layer_id BLOB NOT NULL CHECK (length(head_layer_id) = 33),
    FOREIGN KEY (layer_stack_id, head_layer_id)
        REFERENCES layers(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED
) STRICT, WITHOUT ROWID;

CREATE TABLE layers (
    layer_id BLOB PRIMARY KEY CHECK (length(layer_id) = 33),
    layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 17)
        REFERENCES layer_stacks(layer_stack_id) DEFERRABLE INITIALLY DEFERRED,
    parent_layer_id BLOB
        CHECK (parent_layer_id IS NULL OR length(parent_layer_id) = 33),
    root_id BLOB NOT NULL CHECK (length(root_id) = 32)
        REFERENCES objects(object_id),
    source_branch_id BLOB
        CHECK (source_branch_id IS NULL OR length(source_branch_id) = 17),
    source_commit_id BLOB
        CHECK (source_commit_id IS NULL OR length(source_commit_id) = 33)
        REFERENCES commits(commit_id) DEFERRABLE INITIALLY DEFERRED,
    CHECK (
        (parent_layer_id IS NULL AND source_branch_id IS NULL AND source_commit_id IS NULL)
        OR
        (parent_layer_id IS NOT NULL AND source_branch_id IS NOT NULL
            AND source_commit_id IS NOT NULL)
    ),
    FOREIGN KEY (layer_stack_id, parent_layer_id)
        REFERENCES layers(layer_stack_id, layer_id),
    FOREIGN KEY (layer_stack_id, source_branch_id)
        REFERENCES branches(layer_stack_id, branch_id)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX layer_stack_names ON layer_stacks(name);
CREATE UNIQUE INDEX layer_identity ON layers(layer_stack_id, layer_id);
CREATE UNIQUE INDEX layers_genesis ON layers(layer_stack_id)
    WHERE parent_layer_id IS NULL;
CREATE UNIQUE INDEX layers_child ON layers(layer_stack_id, parent_layer_id)
    WHERE parent_layer_id IS NOT NULL;
CREATE UNIQUE INDEX layers_source ON layers(source_branch_id, source_commit_id)
    WHERE source_branch_id IS NOT NULL;
CREATE UNIQUE INDEX branch_identity ON branches(layer_stack_id, branch_id);
CREATE UNIQUE INDEX branch_names ON branches(layer_stack_id, name);
```

The small metadata tables use `WITHOUT ROWID`. `objects` uses ordinary rowid
storage with a separate primary-key index so large append-oriented BLOBs retain
dense page layout.

## SQL organization

Static application SQL lives under:

```text
crates/layerfs-layerstack-store/sql/
  schema/
  objects/
  layerstack/
  branch/
  workspace/
  query/
```

Each application query is one standalone parameterized `.sql` file.
`src/statements.rs` registers files with `include_str!`; it contains no SQL
text. Rust owns parameter binding, transaction boundaries, typed row decoding,
compare-and-swap interpretation, error mapping, streaming, and timing.

Object membership and reads use fixed 128-position statements padded with
`NULL`. Query pagination uses indexed keyset continuation rather than offsets.

## Object and transaction rules

Canonical object bytes are append-only. A durable read authenticates the BLOB
against its `ObjectId`. Candidate membership runs before a write transaction,
and only absent object payloads are bound for insertion.

Large candidate admission uses short bounded transactions followed by one
final visibility transaction. The visibility pointer is the last mutating
statement. Expensive content work and filesystem I/O occur before writer
transactions.
