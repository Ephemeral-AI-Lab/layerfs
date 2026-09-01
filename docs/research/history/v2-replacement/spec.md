# LayerFS V2 replacement binding specification

Status: binding destructive cold-replacement contract.

This document is the sole normative LayerFS V2 replacement architecture. It
supersedes the deleted two-database `docs/v2` documents and every older schema,
API, CLI, transfer, placement, and source-tree description that conflicts with
it. Historical experiment artifacts remain evidence of the source measured at
their recorded revision; they are not an architectural authority.

`MUST`, `MUST NOT`, `SHOULD`, and `MAY` are normative. A terminal implementation
must satisfy this document as one system. A compiling subset, compatibility
facade, passing unit suite, or plausible performance claim is not terminal.

## 1. Product boundary

LayerFS has exactly one durable database:

```text
LayerStackStore
  named LayerStacks
  immutable Layers
  named writable Branches
  immutable Commits
  one globally deduplicated canonical-object namespace
```

`Workspace` is ephemeral and has no durable database.

One `Client` binds exactly:

```text
one local LayerStackStore
one Monitor
one Workspace manager
one Workspace worker per active WorkspaceId
```

A second database requires a second `Client`. Independent databases have no
LayerFS relationship. The replacement has no endpoint, parent route, durable
Store daemon, network Store protocol, Store vector, active Store selector, or
implicit database switching. The optional local execution-control daemon
specified below owns no Store and does not change this durable topology.

All durable reads resolve from the bound local `LayerStackStore`. If a visible
Layer or Commit references a missing or invalid canonical object, the result is
`Integrity`; there is no fallback Store.

The product and crate retain the names:

```text
crate: crates/layerfs-layerstack-store
Rust crate: layerfs_layerstack_store
public type: LayerStackStore
```

There is no product crate named `layerfs-store` or `layerfs-storage`.

## 2. Explicit removals and non-goals

The replacement contains no:

- `BranchStore`;
- separate authority and receiver databases;
- `StoreId`, Store role, or Store parent identity;
- `LayerStackEndpoint` or remote Store binary;
- Pull operation of any kind;
- Push operation of any kind;
- Reference or Replica serving mode;
- remote, local, authority, or receiver placement distinction;
- complete-root receipt table;
- transfer inventory, transfer session, missing bitmap, or fact/object wire
  protocol;
- fact signing or transport framing whose only consumer was Store transfer;
- hidden network, Pull, Push, or object copy inside Fork, Commit, Add, Diff,
  query, FUSE, or materialization;
- Project type, ID, table, Store, or SDK family;
- LayerStack or Branch rename;
- generic Merge or Branch-to-Branch Diff;
- object refcount, per-entity object copy, implicit garbage collection, or
  automatic eviction;
- database migration or compatibility facade for the deleted two-Store V2;
- TUI, Ratatui, or Crossterm product;
- OverlayFS implementation;
- persistent execution shell, execution worker pool, or precreated Workspace
  helper/mount.

FUSE and explicit materialization remain supported projections. Each Workspace
execution uses a fresh process. One resident, authenticated, owner-bound local
container-control daemon may launch that fresh process and one fresh FUSE helper
per Workspace. It may not own a Store, cache decoded objects, pool workers,
prewarm a shell or workload, or reuse a Workspace, helper, mount, candidate, or
Store connection across measured cases. OverlayFS, remote synchronization,
garbage collection, backup, and power-loss durability are separate future
designs and must not leave scaffolding in this replacement.

## 3. Identity, names, and immutable records

### 3.1 Typed identities

The authoritative typed IDs are:

```text
LayerStackId  17 tagged UUIDv7 bytes
BranchId      17 tagged UUIDv7 bytes
LayerId       33 tagged deterministic bytes
CommitId      33 tagged deterministic bytes
ObjectId      32 content-derived bytes
WorkspaceId   ephemeral typed UUID
ExecutionId   ephemeral typed UUID
```

`StoreId` is deleted. It had no local-only purpose after Store pairing and
transfer identity disappeared.

Layer and Commit identity remains deterministic. Canonical Object identity
retains the frozen V2 domain and role framing. Names never alter any ID, CDC
boundary, canonical encoding, filesystem identity, or root.

### 3.2 EntityName

LayerStacks and Branches share immutable `EntityName`:

```regex
^[a-z0-9](?:[a-z0-9._-]{0,61}[a-z0-9])?$
```

Therefore a name:

- contains 1 through 63 ASCII bytes;
- starts and ends with `[a-z0-9]`;
- otherwise contains only `[a-z0-9._-]`;
- contains no whitespace, slash, backslash, control byte, terminal escape, or
  Unicode normalization ambiguity;
- is compared byte-for-byte after validation.

LayerStack names are unique inside one `LayerStackStore`. Branch names are
unique inside one immutable `LayerStackId`. The same Branch name may exist in
different LayerStacks. Names are presentation and selection metadata; SDK
operations execute with typed IDs.

A product UI may call a named LayerStack a project. The durable model remains:

```text
project = named LayerStack
```

There is no Project entity.

### 3.3 Durable records

The logical durable records are:

```rust
struct LayerStackRecord {
    id: LayerStackId,
    name: EntityName,
    head_layer_id: LayerId,
}

struct LayerRecord {
    id: LayerId,
    layer_stack_id: LayerStackId,
    parent_layer_id: Option<LayerId>,
    root_id: ObjectId,
    source_branch_id: Option<BranchId>,
    source_commit_id: Option<CommitId>,
}

struct BranchRecord {
    id: BranchId,
    layer_stack_id: LayerStackId,
    name: EntityName,
    base_layer_id: LayerId,
    head_commit_id: Option<CommitId>,
}

struct CommitRecord {
    id: CommitId,
    root_id: ObjectId,
    parent_commit_id: Option<CommitId>,
    base_layer_id: LayerId,
}
```

`forked_from_layer_id`, `forked_from_branch_id`, and
`forked_from_commit_id` are deleted. They existed to delimit a locally owned
Push lane. Without Push, a Layer fork is represented by `base_layer_id` with a
null head, while a Branch/Commit fork is represented by the selected immutable
`head_commit_id` and its existing Commit ancestry.

Every Branch is local and writable. `Branch.layer_stack_id` and both names are
immutable. Only these publication pointers change in place:

```text
branches.base_layer_id
branches.head_commit_id
layer_stacks.head_layer_id
```

`branches.base_layer_id` changes only when an explicit reconciliation Commit
advances the Branch to a newer Layer base. Normal Commit keeps the existing
base. Objects, Commits, Layers, IDs, names, and Branch ownership are immutable.

## 4. Logical content and persistence boundary

`layerfs-content` remains SQL-independent. It owns canonical encoding,
authentication, CDC, ropes/extents, filesystem transformations,
reconciliation, hashing, and deterministic root construction through narrow
object-reader and candidate-object interfaces.

FUSE and Workspace mutations do not update durable SQLite rows per syscall.
They update ephemeral Workspace state and a bounded dirty/change frontier.
Base-object reads may use the Store's read-only object reader. Fence, fsync,
pause, Commit, and other synchronization boundaries surface the first deferred
FUSE error. Workspace fsync is a logical ordering, error, and capture boundary;
it does not call `sync_all`/`sync_data` on an otherwise unrecoverable ephemeral
spool. Crash-recoverable Workspaces would require a separate persistent
manifest contract and are not part of this replacement.

At Commit, capture produces:

```rust
BuiltRoot {
    root_id: ObjectId,
    objects: DeferredObjectStore,
    counters: BuildCounters,
}
```

Unchanged canonical objects are reused by `ObjectId`. A changed file, inode,
directory, extent, or root creates new immutable objects only along its changed
frontier. SQL work must not depend on total repository history or unrelated
objects. For an incremental edit it is proportional to the canonical candidate
objects actually emitted by the content algorithm.

The durable `objects` table supports only:

```text
SELECT       yes
INSERT       yes, content-addressed and deduplicating
UPDATE bytes never
DELETE       not in this replacement
```

`UPDATE objects SET bytes=...` is a structural violation. Different canonical
bytes require a different `ObjectId`.

Small candidates remain memory-backed. Candidates exceeding the bounded
memory threshold may spill to an ephemeral local spool and stream bounded
pages into the final transaction. The spool is neither a Store nor a durable
Workspace database and must be removed after success, failure, or Workspace
cleanup. Retain the existing memory-first spill behavior unless measurement
proves a smaller implementation is safe.

## 5. Exact SQLite schema

The replacement has exactly five `STRICT` tables and twenty columns:

```text
objects(2)
commits(4)
branches(5)
layer_stacks(3)
layers(6)
```

The `LayerStackStore` application ID remains `0x4c46534c`. The incompatible
replacement schema uses `PRAGMA user_version=4`. Version 3 and every structural
variant are rejected; no migration is attempted.

The binding DDL is:

```sql
CREATE TABLE objects (
    object_id BLOB PRIMARY KEY
        CHECK (length(object_id) = 32),
    bytes BLOB NOT NULL
) STRICT;

CREATE TABLE commits (
    commit_id BLOB PRIMARY KEY
        CHECK (length(commit_id) = 33),
    root_id BLOB NOT NULL
        CHECK (length(root_id) = 32)
        REFERENCES objects(object_id),
    parent_commit_id BLOB
        CHECK (
            parent_commit_id IS NULL
            OR length(parent_commit_id) = 33
        )
        REFERENCES commits(commit_id),
    base_layer_id BLOB NOT NULL
        CHECK (length(base_layer_id) = 33)
        REFERENCES layers(layer_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE branches (
    branch_id BLOB PRIMARY KEY
        CHECK (length(branch_id) = 17),
    layer_stack_id BLOB NOT NULL
        CHECK (length(layer_stack_id) = 17)
        REFERENCES layer_stacks(layer_stack_id)
        DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]'),
    base_layer_id BLOB NOT NULL
        CHECK (length(base_layer_id) = 33),
    head_commit_id BLOB
        CHECK (
            head_commit_id IS NULL
            OR length(head_commit_id) = 33
        )
        REFERENCES commits(commit_id),
    FOREIGN KEY (layer_stack_id, base_layer_id)
        REFERENCES layers(layer_stack_id, layer_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE layer_stacks (
    layer_stack_id BLOB PRIMARY KEY
        CHECK (length(layer_stack_id) = 17),
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]'),
    head_layer_id BLOB NOT NULL
        CHECK (length(head_layer_id) = 33),
    FOREIGN KEY (layer_stack_id, head_layer_id)
        REFERENCES layers(layer_stack_id, layer_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT, WITHOUT ROWID;

CREATE TABLE layers (
    layer_id BLOB PRIMARY KEY
        CHECK (length(layer_id) = 33),
    layer_stack_id BLOB NOT NULL
        CHECK (length(layer_stack_id) = 17)
        REFERENCES layer_stacks(layer_stack_id)
        DEFERRABLE INITIALLY DEFERRED,
    parent_layer_id BLOB
        CHECK (
            parent_layer_id IS NULL
            OR length(parent_layer_id) = 33
        ),
    root_id BLOB NOT NULL
        CHECK (length(root_id) = 32)
        REFERENCES objects(object_id),
    source_branch_id BLOB
        CHECK (
            source_branch_id IS NULL
            OR length(source_branch_id) = 17
        ),
    source_commit_id BLOB
        CHECK (
            source_commit_id IS NULL
            OR length(source_commit_id) = 33
        )
        REFERENCES commits(commit_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (
        (
            parent_layer_id IS NULL
            AND source_branch_id IS NULL
            AND source_commit_id IS NULL
        )
        OR
        (
            parent_layer_id IS NOT NULL
            AND source_branch_id IS NOT NULL
            AND source_commit_id IS NOT NULL
        )
    ),
    FOREIGN KEY (layer_stack_id, parent_layer_id)
        REFERENCES layers(layer_stack_id, layer_id),
    FOREIGN KEY (layer_stack_id, source_branch_id)
        REFERENCES branches(layer_stack_id, branch_id)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX layer_stack_names
    ON layer_stacks(name);

CREATE UNIQUE INDEX layer_identity
    ON layers(layer_stack_id, layer_id);

CREATE UNIQUE INDEX layers_genesis
    ON layers(layer_stack_id)
    WHERE parent_layer_id IS NULL;

CREATE UNIQUE INDEX layers_child
    ON layers(layer_stack_id, parent_layer_id)
    WHERE parent_layer_id IS NOT NULL;

CREATE UNIQUE INDEX layers_source
    ON layers(source_branch_id, source_commit_id)
    WHERE source_branch_id IS NOT NULL;

CREATE UNIQUE INDEX branch_identity
    ON branches(layer_stack_id, branch_id);

CREATE UNIQUE INDEX branch_names
    ON branches(layer_stack_id, name);
```

The four small metadata tables are `WITHOUT ROWID` because each has a
non-integer typed primary key. `objects` deliberately uses ordinary rowid
storage: its immutable large BLOB payload is append-oriented, while the
separate ObjectId primary-key index remains small enough for bounded random
lookup. Captured same-host evidence showed that `WITHOUT ROWID` forced large
canonical payloads into sparsely occupied overflow pages and materially
increased publication time and storage amplification.

Indexes not justified by a binding query are not retained. Commit and Layer
ancestry follow the parent ID through the primary key. Reverse-parent,
Branch-head, and fork-origin indexes must not be added without an actual query
and `EXPLAIN QUERY PLAN` proof.

The replacement contains no tables named:

```text
store
branch_scopes
layer_stack_scopes
complete_roots
transfers
receipts
projects
workspaces
operations
```

## 6. SQLite runtime and connection contract

The primary deployment and benchmark authority is the current macOS 26.4.1
M3 Max host, where the portable SQLite file is physically backed by APFS.
Linux ext4/XFS runs are portability comparisons. The implementation contains
no APFS-specific code.

The durable Store directory contains exactly one file named `store.sqlite`.
Every Create and Connect applies and verifies this connection-local profile:

```text
foreign_keys=ON
journal_mode=MEMORY
synchronous=OFF
temp_store=MEMORY
cache_size=-32768       # 32 MiB
cache_spill=OFF
mmap_size=0
threads=0
locking_mode=EXCLUSIVE
busy_timeout=5000ms
```

New Stores select a 64 KiB SQLite page size before creating the schema.
Connect does not rewrite the page size. `journal_mode=MEMORY` is not persistent,
so its exact returned value is checked on every Create and Connect.

The public acknowledgement contract is transaction committed and immediately
readable from the same live local Store process. `MEMORY` journaling retains
statement and transaction rollback while that process remains alive. This
profile explicitly abandons process-crash, OS-crash, power-loss, and recovery
durability. `journal_mode=OFF` is forbidden because it also disables ordinary
statement/transaction rollback and can leave indexes corrupt after errors.

Normal SDK operations perform no database-file `fsync`, directory `fsync`,
stable barrier, `VACUUM`, or `ATTACH`. No `store.sqlite-wal`,
`store.sqlite-shm`, `store.sqlite-journal`, or `store.sqlite.owner` file may
exist at a normal transaction, close, or reopen checkpoint.

`LayerStackStore` owns one long-lived SQLite connection behind one application
mutex. `locking_mode=EXCLUSIVE` is set and an immediate transaction acquires
the SQLite lock during open. SQLite lock failure is `StoreBusy`; there is no
custom owner sidecar, reader connection, connection pool, or per-syscall
connection. Ephemeral Workspace `.runtime`, candidate, spool, and helper state
lives outside the durable Store directory and is cleaned at its lifecycle
boundary.

## 7. Standalone SQL statement contract

All static application SQL lives in standalone `.sql` files inside
`crates/layerfs-layerstack-store/sql`. Rust owns parameter binding, transaction
boundaries, streaming, typed row decoding, CAS interpretation, error mapping,
and timing.

The binding statement layout is:

```text
crates/layerfs-layerstack-store/
├── sql/
│   ├── schema/
│   │   ├── v4.sql
│   │   ├── schema_objects.sql
│   │   ├── table_columns.sql
│   │   └── foreign_key_check.sql
│   ├── objects/
│   │   ├── get.sql
│   │   ├── get_many_128.sql
│   │   ├── membership_128.sql
│   │   ├── insert.sql
│   │   ├── equal.sql
│   │   └── page.sql
│   ├── layerstack/
│   │   ├── get.sql
│   │   ├── get_by_name.sql
│   │   ├── list.sql
│   │   ├── insert.sql
│   │   ├── get_layer.sql
│   │   ├── list_layers.sql
│   │   ├── insert_layer.sql
│   │   ├── find_layer_by_source.sql
│   │   ├── load_add_snapshot.sql
│   │   ├── advance_head.sql
│   │   ├── current_head.sql
│   │   └── history_page.sql
│   ├── branch/
│   │   ├── get.sql
│   │   ├── get_by_name.sql
│   │   ├── list.sql
│   │   ├── insert.sql
│   │   ├── get_commit.sql
│   │   ├── list_commits.sql
│   │   ├── history_page.sql
│   │   └── contains_commit.sql
│   ├── workspace/
│   │   ├── load_snapshot.sql
│   │   ├── insert_commit.sql
│   │   ├── advance_branch.sql
│   │   └── current_branch.sql
│   └── query/
│       ├── store_counts.sql
│       ├── canonical_storage.sql
│       ├── layer_roots_page.sql
│       ├── commit_roots_page.sql
│       └── branch_roots_page.sql
└── src/
    ├── lib.rs
    ├── statements.rs
    ├── store.rs
    ├── schema.rs
    ├── objects.rs
    ├── layerstack.rs
    ├── branch.rs
    ├── workspace.rs
    └── query.rs
```

`src/statements.rs` is a small compile-time manifest of named `include_str!`
constants grouped by the same family. It contains no SQL text. A missing file
is a compile error. The CLI and SDK contain no SQL.

Every `.sql` file except `schema/v4.sql` contains exactly one parameterized
statement and a header documenting:

```text
operation family
statement name
parameter number and meaning
result columns or affected-row contract
```

`schema/v4.sql` is the sole multi-statement file. Application query files do
not contain `BEGIN`, `COMMIT`, or `ROLLBACK`; transaction control remains in
Rust. There is no runtime SQL loader, ORM, DAO, repository layer, query builder,
or database-backend abstraction.

`objects/get_many_128.sql` and `objects/membership_128.sql` use one fixed
bounded placeholder shape and pad unused positions with `NULL`, so SQL remains
static and cached. Query pages use keyset continuation, never `OFFSET`.

Except for connection PRAGMAs applied through the `rusqlite` API, production
Rust files must not embed or dynamically construct application SQL. A contract
test enumerates every `.sql` file, proves each is registered, prepares each
statement against the exact schema, checks parameter counts, and rejects
unregistered or inline SQL.

## 8. Mutation transactions

All expensive work occurs before the SQLite write transaction:

- filesystem capture and dirty-path enumeration;
- CDC and content hashing;
- canonical encoding and ObjectId authentication;
- Diff and reconciliation;
- Commit/Layer deterministic ID derivation;
- history or closure traversal required by an explicit diagnostic;
- bounded candidate construction and spill.

No writer transaction performs network I/O, full repository scan, full history
enumeration, full closure verification, content hashing, execution, FUSE I/O,
or unbounded materialization.

### 8.1 Object insertion

Candidate objects stream through one reused prepared statement:

```sql
INSERT INTO objects(object_id, bytes)
VALUES (?1, ?2)
ON CONFLICT(object_id) DO NOTHING;
```

Before opening the writer transaction, the Store performs bounded local
membership queries over candidate IDs and encoded lengths. This is not a
transfer plan: it performs no endpoint call, sends no payload, and examines no
history or closure. It prevents a reuse-heavy candidate such as a 32 MiB
prepend from rebinding and comparing tens of MiB already present in the same
Store.

Only membership-missing candidate bytes enter SQLite. The primary key remains
the final deduplication guard. Newly constructed objects are authenticated
before membership; objects read later are authenticated against their IDs. A
present ID with an unexpected encoded length is `Integrity`. If an insert
unexpectedly reports a conflict after the prevalidated membership result,
`objects/equal.sql` must prove byte equality or return `Integrity`.

One shared `admit_planned_objects` primitive partitions every large candidate
in authenticated spool order. Each early `BEGIN IMMEDIATE` transaction admits
fewer than 128 objects and fewer than 4 MiB of canonical payload, then commits.
The final bounded batch remains for the visibility transaction. This primitive
is used by Initialize, normal Commit, fallback/large Commit, and reconciliation;
it is never called by Exec, FUSE write, fsync, capture, or any work before T2.
Candidate and reference indexes remain independently bounded to 8 MiB.

### 8.2 Initialization

Initialization requires a name and `Empty` or one existing directory. It
builds and authenticates the full genesis root, completes bounded membership,
and admits all non-final object batches before the visibility transaction:

```text
zero or more bounded object-admission transactions
BEGIN IMMEDIATE
insert final bounded candidate batch
insert genesis Layer
insert named LayerStack
COMMIT
```

Deferred foreign keys permit the Layer/LayerStack cycle. Failure exposes
neither record; prior admitted immutable objects remain unreachable. The result
returns both IDs.

### 8.3 Fork

Fork is a local metadata operation:

```rust
fork_branch(name: EntityName, source: LocalForkSource) -> BranchId
```

For a Layer source, Fork inserts a new Branch with that base Layer and a null
Commit head. For a Branch/Commit source, Fork verifies the selected Commit is
in the source Branch's visible ancestry, then inserts a new Branch pointing at
that existing Commit and its base Layer.

Fork always creates a new `BranchId`, requires a unique scoped name, copies
zero canonical objects, and performs no hidden acquisition. SQLite uniqueness
claims the name atomically. A conflict returns a typed error containing the
existing and incoming IDs.

### 8.4 Workspace Create

Workspace Create obtains the Branch, base, head, and effective root from one
consistent joined snapshot query. It acquires one in-process writable lease for
that `BranchId`, creates the requested FUSE or materialized projection, and
returns only after the projection is ready.

There is no remote/read-only Workspace form. Multiple `Client` values sharing
the same in-process `LayerStackStore` owner share the lease set.

### 8.5 Workspace Commit

After capture creates a validated candidate, Commit performs:

```text
bounded local candidate-ID membership
zero or more BEGIN IMMEDIATE / bounded object batch / COMMIT transactions
BEGIN IMMEDIATE
insert final bounded candidate batch with the same cached INSERT
insert immutable Commit
UPDATE branches
  SET head_commit_id = new,
      base_layer_id = new_base
  WHERE branch_id = target
    AND head_commit_id IS expected
    AND base_layer_id = expected_base
COMMIT
```

The Branch compare-and-swap is the last visibility statement. Zero changed
rows roll back and return typed `CommitHeadMoved` with the actual head. A
candidate equal to the current root and base is `UpToDate` and performs no
write transaction.

Normal Commit performs no full-root closure scan. Completeness is inductive:
the visible base is complete, the authenticated builder reads only visible
objects, every newly referenced object is in the bounded candidate, all new
objects are admitted before visibility, and the head changes last. Early
admission is part of the timed public Commit, after T2; it is never shifted into
Exec or capture.

### 8.6 Add Layer

Add accepts one local `BranchId`, derives its LayerStack, head Commit, base
Layer, and current LayerStack head from one joined snapshot, and copies no
canonical objects. If the Commit root equals the base root, it returns
`NoChanges`. If the same Branch/Commit was already added, it returns
`UpToDate`.

Otherwise Add performs:

```text
BEGIN IMMEDIATE
insert immutable Layer referencing the existing Commit root
compare-and-swap layer_stacks.head_layer_id from Branch base to new Layer
COMMIT
```

A stale LayerStack head returns `HeadMoved`; it never Pulls, Pushes, merges, or
silently rebases. `NotPushed` and `LayerNotPulled` are deleted because they are
impossible in one Store.

### 8.7 Failure and visibility

Failure injection at every statement must prove that no Branch or LayerStack
head points to incomplete state. A failed final transaction or CAS may leave
objects committed by earlier admission batches. They are unreachable immutable
canonical rows, may later deduplicate, and are not candidate reuse credit for
the failed operation. There is no refcount or synchronous garbage collection.
Fork and Add remain single metadata transactions; Initialize and Commit use
bounded admission transactions plus one final visibility transaction.

## 9. Read, history, Diff, and reconciliation

Point reads use primary keys. Object batches are bounded. Entity listing uses
indexed keyset pagination with limits `1..=512`; ancestry pages use limits
`1..=128`.

Commit and Layer history pages use one bounded recursive CTE per page rather
than one Rust-to-SQL query per ancestor. Normal Commit, Fork from Layer, and
Add do not enumerate full history. Fork from Branch/Commit may prove ancestry
with one bounded SQLite traversal; it must not materialize unbounded history.

The supported Diff requests remain exactly:

```rust
DiffRequest::BranchCommits {
    branch_id,
    from_commit_id,
    to_commit_id,
}
DiffRequest::BranchLayer {
    branch_id,
    layer_id,
}
DiffRequest::Layers {
    from_layer_id,
    to_layer_id,
}
```

Branch-to-Branch Diff remains unrepresentable. All records and roots come from
the same Store.

Reconciliation retains typed, paged conflicts:

```text
Content
Type
Directory
HardLink
```

The choices remain distinct:

```text
Branch
Layer
WorkingTree
```

Only a later mutation intersecting an affected path by equality, ancestor, or
descendant relation invalidates that choice. Unresolved Commit is refused. No
conflict marker or conflict row enters the database.

## 10. Public SDK

### 10.1 Store and Client

```rust
LayerStackStore::create(path) -> Result<LayerStackStore>
LayerStackStore::connect(path) -> Result<LayerStackStore>
Client::connect(store: Arc<LayerStackStore>) -> Result<Client>
```

`create` refuses an existing path. `connect` never creates and verifies the
exact application ID, schema version, schema objects, column count, indexes,
and foreign keys. The Client constructs one Monitor and Workspace manager for
that Store.

### 10.2 Shared value types

```rust
EntityName
LayerStackId, LayerId
BranchId, CommitId

LayerStackInitialization::{
    Empty,
    Directory(PathBuf),
}

LocalForkSource::{
    Layer { layer_id },
    Branch { branch_id, commit_id },
}

DiffRequest::{
    BranchCommits { branch_id, from_commit_id, to_commit_id },
    BranchLayer { branch_id, layer_id },
    Layers { from_layer_id, to_layer_id },
}

ResolveChoice::{Branch, Layer, WorkingTree}
EndWorkspaceMode::{Clean, Discard}
```

### 10.3 Operation families

```rust
Client::initialize_layerstack(
    name: EntityName,
    source: LayerStackInitialization,
) -> InitializeLayerStackResult

Client::fork_branch(
    name: EntityName,
    source: LocalForkSource,
) -> BranchId

Client::diff(request: DiffRequest) -> OperationHandle
Client::add_layer(branch_id: BranchId) -> AddLayerResult

Client::create_workspace_session(request) -> WorkspaceId
Client::workspace_conflicts(workspace_id, cursor) -> ConflictPage
Client::resolve_workspace_conflict(workspace_id, conflict_id, choice)
Client::commit_workspace_session(workspace_id) -> WorkspaceCommitResult
Client::end_workspace_session(workspace_id, mode)
Client::exec_workspace_session(workspace_id, argv) -> WorkspaceExecution
Client::shell_workspace_session(workspace_id) -> WorkspaceExecution
Client::workspace_output(execution_id) -> OutputReader
Client::stop_workspace_execution(execution_id)

Client::query(query: Query) -> QueryPage
Client::monitor_snapshot() -> MonitorSnapshot
Client::analyze_dedup() -> DedupAnalysis
```

There are no public or hidden `pull_layer`, `pull_branch`, `push_branch`,
remote endpoint, placement, serving-mode, or parent-route methods.

Query kinds are exactly:

```rust
LayerStacks
Layers
Branches
Commits
Workspaces
Monitor
```

There are no `Authority*` variants. Query records contain names and typed IDs,
not scope, placement, mode, through boundary, or Store role. JSON schema
version 4 reflects the incompatible public shape; Rust `Debug` is not JSON.

## 11. CLI grammar

The CLI is a thin parser, planner, completion, and presentation layer over the
public SDK. It contains no SQL and no alternate fast path.

```text
layerfs
├── db
├── context
├── layerstack
├── branch
├── workspace
├── monitor
└── query
```

### 11.1 Database and context

```text
layerfs db create <path>
layerfs db connect <path>

layerfs context use --store <path>
layerfs context show
```

There is no Store role, `--parent`, database pair, or second location.

### 11.2 LayerStack

```text
layerfs layerstack init --name <name> --empty
layerfs layerstack init --name <name> <directory>

layerfs layerstack diff --from <layer-id> --to <layer-id>
layerfs layerstack add <branch-id>
```

### 11.3 Branch

```text
layerfs branch fork --name <name> --layer <layer-id>
layerfs branch fork --name <name> \
  --branch <branch-id> --commit <commit-id>

layerfs branch diff --branch <branch-id> \
  --from <commit-id> --to <commit-id>
layerfs branch diff --branch <branch-id> --layer <layer-id>
```

### 11.4 Workspace

```text
layerfs workspace create <branch-id> \
  --at <mount-path> \
  [--container <container-id>] \
  [--projection fuse|materialize]

layerfs workspace exec <workspace-id> -- <program> [arguments...]
layerfs workspace shell <workspace-id>
layerfs workspace output <execution-id> [--follow]
layerfs workspace stop <execution-id>
layerfs workspace conflicts <workspace-id> [--after <cursor>]
layerfs workspace resolve <workspace-id> <conflict-id> \
  (--branch | --layer | --working-tree)
layerfs workspace commit <workspace-id>
layerfs workspace end <workspace-id> [--discard]
```

### 11.5 Monitor and query

```text
layerfs monitor snapshot
layerfs monitor analyze-dedup

layerfs query layerstacks
layerfs query layers
layerfs query branches [--layerstack <layer-stack-id>]
layerfs query commits
layerfs query workspaces
layerfs query monitor
```

Completion may display `layerstack-name/branch-name (BranchId)` but substitutes
the exact typed ID. No name-based duplicate SDK method or stored composite name
is added.

The parser must reject all deleted grammar, including:

```text
layerstack pull
branch pull
branch push
--reference
--replica
--through
--parent
authority-* queries
```

## 12. Workspace, FUSE, execution, and capture

One writable Workspace lease exists per Branch among Clients sharing the same
in-process Store owner. A Workspace pins one exact Branch head/base/root
snapshot. Commit uses exact head/base CAS. `Clean` End refuses dirty state;
`Discard` explicitly abandons it. End never commits and Add never commits a
Workspace.

FUSE and explicit materialization use the same root-keyed local object reader.
FUSE reads may batch canonical-object queries. FUSE mutations remain ephemeral
until Commit and must not start a durable SQLite write transaction for every
filesystem syscall.

Exec contains only the fresh requested process, real projection I/O, required
Workspace synchronization, and the existing canonical capture work. SQLite
membership and object admission start only inside public Workspace Commit after
T2. Moving admission into Exec to report a smaller Commit is a benchmark and
architecture violation.

Exec and Shell run only in an active Workspace. Every Exec/Shell request starts
a fresh process using the standard shell/runtime; there is no persistent,
prewarmed execution shell. Output remains bounded and typed, and Stop targets
the exact `ExecutionId`. When the local control daemon is selected, one
owner-authenticated connection binds the Client process to the daemon. Native
Linux uses the protected Unix socket and peer credentials. A native macOS
Client controlling one prepared Linux container uses capability-authenticated
TCP published by Docker only on host `127.0.0.1`; it never treats claimed
PID/UID/GID values as authority. Every TCP Exec/Mount stream proves possession
of the capability over the daemon boot identity, owner identity, a fresh nonce,
and the bound request. Each Exec uses a separate bound stream and newly spawned
process, while each FUSE Workspace uses a separate bound stream, helper process,
and mount. Owner-connection or daemon loss terminates owned work and is an
infrastructure error, never an implicit fallback during an active operation.

For benchmark preparation, a container and image may already exist. Store,
context, image, and container preparation are excluded from the timed operation.
Workspace Create, mount readiness, fresh-process execution, Commit, and End are
included when their corresponding lifecycle measurement is reported.

Capture coalesces intermediate operations into the final Workspace state. A
file created and deleted before Commit does not become durable merely because
it existed transiently. Count-changing edits use the existing rope/extent and
CDC implementation; they do not ask SQL to rewrite or enumerate the unchanged
file.

One fresh, per-Workspace capture thread may overlap the existing FastCDC/rope
construction with an already-timed new-file write stream. It starts only after
T0, owns one bounded input channel and the existing bounded candidate buffer,
and is destroyed by Commit or End. Eligibility requires one new file written
from offset zero in exact forward-contiguous order. A second file, overwrite,
backward write, gap, truncate, zero-write shortcut, capture error, or later
mutation invalidates the optimization and falls back to the ordinary exact
Commit path. Fsync, unpin, pause, and Commit finish or abort the thread. The
authenticated candidate object set is handed directly into namespace
construction; Commit does not copy a full candidate or hash its payloads again.

The current V2 file-byte path is incremental, but the current Workspace
namespace planner constructs complete base and final manifests. Until that
planner is replaced, candidate namespace planning is `O(total visible paths)`
CPU and memory, bounded by `ResourcePolicy`. This specification must not claim
that tiny-edit Commit latency is independent of unrelated repository paths.
Replacing the manifest planner with an operation-aware mutation journal is an
allowed replacement optimization; if deferred, it remains owned by the V3
capture design.

## 13. Deduplication, storage, and memory

One Store contains one physical row per `ObjectId`. LayerStacks, Layers,
Branches, and Commits share that namespace. Fork and Add copy zero objects.

For every candidate:

```text
candidate_objects = inserted_objects + reused_objects
candidate_bytes   = inserted_bytes   + reused_bytes
```

Receipts independently report object count and encoded bytes. Dedup analysis
reports:

```text
physical canonical object count and bytes
reachable union object count and bytes
candidate, inserted, and reused count and bytes
saved fraction when its denominator is nonzero
logical-to-physical ratio when exactly defined
unreachable physical objects, if an explicit analysis traverses reachability
```

There is no placement factor, cross-Store union, transfer sent/avoided count,
Reference coverage, or Replica coverage.

Object and history work remains bounded:

- object reads and candidate streams use bounded count/byte pages;
- one operation-scoped Seen set deduplicates multi-root diagnostics;
- the Seen set and candidate buffer spill after fixed memory thresholds;
- history pages contain at most 128 records;
- no complete history or object closure is an unbounded in-memory `Vec`;
- no canonical object is rechunked, re-encoded, reminted, or copied merely to
  establish a Branch or Layer.

The initial frozen ceilings remain:

```text
candidate object memory       8 MiB
candidate reference index     8 MiB, optional with exact fallback
object page                   at most 128 objects
object page bytes             at most 4 MiB
history page                  at most 128 records
entity query page             at most 512 records
```

No implicit GC runs during Commit, Add, Fork, End, query, or Store close. A
future explicit GC requires its own root/pin and interruption contract.

## 14. Monitor and instrumentation

One Monitor belongs to the Client. A passive snapshot uses retained receipts
and performs zero Store SQL. Exact storage/dedup analysis is explicit.

Operation families are:

```text
LayerStackInitialize
LayerStackDiff
LayerStackAdd
BranchFork
BranchDiff
WorkspaceCreate
WorkspaceExec
WorkspaceShell
WorkspaceOutput
WorkspaceStop
WorkspaceConflicts
WorkspaceResolve
WorkspaceCommit
WorkspaceEnd
Query
DedupAnalyze
```

Transfer and placement families are deleted. Receipts contain the exact public
operation, IDs, name when created, queue/service time, candidate/inserted/reused
counts and bytes where applicable, and database timing where applicable.

Database timings use stable operation and phase names, for example:

```text
workspace.commit.connection_wait
workspace.commit.object_insert
workspace.commit.metadata_insert
workspace.commit.branch_cas
workspace.commit.sqlite_commit
```

Raw SQL tracing and per-statement failure injection are test/debug features.
Production benchmark runs must not pay for collecting full SQL strings or
unbounded traces. Store file census is collected at bounded lifecycle
checkpoints outside the timed operation.

## 15. Frozen production source tree

The terminal production workspace is:

```text
crates/
├── layerfs-content
├── layerfs-layerstack-store
├── layerfs-workspace
├── layerfs-fuse
├── layerfs-materialization
├── layerfs-monitor
├── layerfs-sdk
└── layerfs-cli
```

`tools/layerfs-eval` and benchmark crates are evidence tooling, not additional
Stores.

Move into `layerfs-layerstack-store` before deleting their former owners:

- typed durable IDs, records, errors, and receipts still used locally;
- exact SQLite open/schema/query implementation;
- canonical-object reading and append-only insertion;
- local Fork, Commit, SnapshotReader, Branch/Commit/Layer queries, and Diff
  adapters;
- Workspace lease ownership;
- reconciliation preparation and publication;
- bounded candidate and Seen spill primitives that remain necessary.

Delete:

```text
crates/layerfs-branch-store
crates/layerfs-storage
crates/layerfs-layerstack-store/src/receive.rs
crates/layerfs-layerstack-store/src/remote.rs
crates/layerfs-layerstack-store/src/bin/layerfs-layerstack-store.rs
```

Delete related Cargo members, dependencies, features, public reexports,
endpoint traits, transport/wire code, facts used only by transfer, Pull/Push
plans, scope/placement records, and compatibility aliases. Do not leave empty
crates, wrapper types, deprecated methods, or old grammar.

Retain the tracked deletion of the TUI crate and installer. Do not restore a
TUI, Ratatui, or Crossterm dependency.

Handwritten production Rust files remain below 1,500 lines. SQL is organized
into the standalone files in section 7 rather than exempting one giant Rust
module from the ceiling.

## 16. Destructive implementation sequence

This is a cold replacement, not an in-place migration:

1. Freeze this specification and structural tests.
2. Install the five-table schema and standalone SQL manifest in
   `layerfs-layerstack-store`.
3. Move local object, SnapshotReader, Fork, Commit, query, lease, Diff, and
   reconciliation code into that crate.
4. Rewire Workspace, FUSE, materialization, Monitor, SDK, and CLI to one Store.
5. Delete Pull, Push, transfer, placement, Reference/Replica, parent route,
   remote server, BranchStore, and `layerfs-storage` code.
6. Delete compatibility APIs and old CLI grammar.
7. Reconcile active documentation, benchmark orchestration, evidence tooling,
   and source manifests.
8. Run focused proof, full gates, real FUSE, and the current public-SDK
   benchmark until terminal.

Opening an old two-Store V2 database returns `WrongStoreSchema`. The user must
create a new replacement Store and reinitialize content. No converter is part
of this scope.

## 17. Benchmark contract and performance expectations

Benchmarks invoke public SDK operations only. They must not call private Store
helpers, bypass validation, hard-code results, precreate the Workspace, reuse a
persistent execution shell, disable canonical authentication, skip Commit, or
substitute an earlier run.

The standard environment uses:

- one already prepared container and image when container placement is tested;
- the public SDK, `LayerStackStore`, Workspace runtime/spool, Monitor, and
  `ProxyHost` running natively on the macOS host against the native Store file;
- only the accepted control daemon, fresh per-Workspace FUSE helper, fresh
  per-Exec process, workload binary, and prepared fixture inside the container;
- no Store, runtime, result, binary, helper, or fixture host bind in the
  prepared container;
- one daemon TCP port published only on host `127.0.0.1`, with all Docker
  preparation and custody checks completed before T0;
- real FUSE, not a native-directory substitute;
- a fresh process for every Exec/Shell command;
- the same host, prepared-container class, fixture bytes, workload semantics,
  acknowledgement boundary, and cache policy for compared products;
- setup caches only before timing;
- current source with commit, dirty-source seal, timestamps, host/runtime
  metadata, commands, and raw artifacts recorded.

The one-Store lifecycle checkpoints are:

```text
T0  immediately before public Workspace Create
T1  Workspace Create returns; projection is ready
T2  fresh-process public Exec returns
T3  public Workspace Commit returns; Commit/head is visible in LayerStackStore
T4  public Workspace End returns
```

Report independently:

```text
workspace_create_ns       = T1 - T0
execution_ns              = T2 - T1
commit_api_ns             = T3 - T2
layerstack_visible_ns     = T3 - T0
workspace_end_ns          = T4 - T3
complete_lifecycle_ns     = T4 - T0
```

There is no BranchStore-visible checkpoint and no Push or authority-visible
checkpoint. `Add Layer` is an optional separately named publication
measurement and is not silently included in Workspace Commit.

For the current fs-bench-plus host/harness, the terminal planning goals are:

```text
Workspace Create                            <= 20 ms hard; 6-14 ms preferred
complete cold-create-32m                    <= 150 ms hard; 125-145 ms preferred
complete one-small-edit diagnostic          <= 30 ms hard; 20-28 ms preferred
small-edit Workspace Commit                 preserve 3-6 ms
deterministic EDIT16                        <= 200 ms hard; 130-160 ms preferred
prepend-temp-copy-rename 10 B over 32 MiB  <= 250 ms hard; 190-225 ms preferred
read-to-sink 32 MiB                        <= 150 ms hard; 105-130 ms preferred
registered four-row total                  <= 700 ms hard; 580-660 ms preferred
inner 32 MiB FUSE write throughput         >= 300 MiB/s hard; 640-800 MiB/s preferred
```

These goals do not authorize caching across measured cases, persistent shells,
precreated Workspaces, worker pools, warmed Stores or mounts, direct private
APIs, omitted durability work, or reduced correctness. The one accepted daemon
is only the authenticated resident container-control transport. Every result
includes the exact measured boundary. A failure requires phase-timer diagnosis
and another measured iteration; it must not be hidden by changing the workload
or discarding a cold sample.

### 17.1 Expected impact on the earlier four-row benchmark

The following is historical context and planning projection, not an achieved
replacement result. Projected speedups must not be published before 7-9 fresh
paired samples under matched non-crash-durable acknowledgement semantics.
The earlier reported sealed bands were:

```text
Create 32 MiB   2.441-2.512 s
EDIT16          1.026-1.058 s
Prepend 10 B    0.840-0.864 s
Read 32 MiB     0.458-0.466 s
```

The following Round 070 table is historical two-Store evidence for one
uninterrupted **cold 32 MiB create**. It is not a replacement target and its
`Workspace Commit` number is not a small-edit Commit:

```text
Workspace Create       41.923 ms
fresh-process Exec      96.789 ms
Workspace Commit       268.434 ms  historical cold-create Commit
Push                    244.011 ms  deleted operation
Workspace End             8.421 ms
complete                659.578 ms
local-Store visible     407.146 ms
```

The historical cold-create Commit decomposed as:

```text
content/CDC/encoding     79.257 ms
candidate finalization  59.935 ms
local admission proof   18.645 ms
SQLite publication     105.294 ms
unattributed              5.303 ms
total                   268.434 ms
```

The replacement deletes Push rather than optimizing it, so its time is exactly
absent from the new lifecycle. Deleting Push mechanically removes `244.011 ms`
from that historical sequence. It does not prove the final one-Store result
because the replacement also changes schema, connection, local membership,
candidate finalization, transaction, and durability policy.

The preregistered terminal cold-create phase budget on the primary host is:

```text
Workspace Create         6-16 ms; 20 ms hard
fresh Exec/FUSE/capture  75-95 ms
Commit with admission    35-55 ms; 65 ms hard
Workspace End              3-4 ms
complete                125-145 ms; 150 ms hard
```

The intended small-edit Commit budget is separate:

```text
pause/fence/capture       <1 ms
content + candidate       2-6 ms
local membership          <1 ms
Commit insert + CAS       2-4 ms
rebase/resume              1-2 ms
total                     5-12 ms target; 20 ms hard
```

The expected first optimized one-Store ranges on the same host are:

| Workload | Earlier reported band | Expected one-Store band | Why |
|---|---:|---:|---|
| Create 32 MiB | 2.441-2.512 s | 0.23-0.32 s | One canonical admission and one visibility transaction; no second 32 MiB Store placement, Push, receiver proof, or authority transaction. |
| EDIT16 | 1.026-1.058 s | 0.63-0.74 s | The best earlier run spent about 0.276 s in sixteen Pushes. Those disappear; small-edit Commit is already about 7.9 ms/edit in the best retained run. |
| Prepend 10 B | 0.840-0.864 s | 0.40-0.52 s | Push disappears and reuse is decided by bounded local ID membership. The public temp-copy/fsync/rename workload still copies and processes 32 MiB, so its irreducible work is reported rather than hidden. |
| Read 32 MiB | 0.458-0.466 s | 0.33-0.42 s | No Push existed to remove. Improvement comes only from direct one-Store reads, removal of scope/endpoint routing, and a reused Workspace reader; fresh-process execution remains the dominant fixed cost. |

The replacement benchmark must report measured medians and distributions; it
must not present these projected ranges as results. If a row misses its range,
the phase timers determine whether Store SQL, content construction, FUSE,
process startup, or host noise is responsible.

The timed Read workload reads exactly 32 MiB through the public Workspace into
a process-local sink and reports the byte count. It does not hash inside T0-T4:
the same prepared SHA-256 helper costs more than the complete Read hard gate on
the primary container and would measure hash CPU rather than filesystem
throughput. The existing post-T4 public-SDK proof remounts the committed root
and verifies exact size and SHA-256 bytes for every Read/Prepend case.

The native-host hard gates above replace the earlier in-container planning
numbers. Runs `v4-native-host-r001` through `r005` remain retained failure
evidence rather than being rewritten. They proved two previously unmeasured
costs in the accepted deployment: every large FUSE read crosses the Docker
Desktop VM boundary from the native host Store, and every Workspace creates a
fresh helper/mount rather than a prewarmed projection. The revised gates retain
the full public lifecycle and are still substantially stricter than the pinned
Computer create/edit/prepend results; Read remains independently visible rather
than being hidden inside the total.

## 18. Focused proof requirements

A terminal replacement proves all of the following against current source:

### 18.1 Schema and SQL

- exact application ID, user version, five tables, twenty columns, constraints,
  and index census;
- version 3 and structurally different Store rejection;
- all standalone SQL files registered and prepared against the exact schema;
- no inline/dynamic application SQL in production Rust;
- point, scoped-name, and keyset queries use indexed `SEARCH` plans;
- no `OFFSET`, unbounded history query, or N+1 query page;
- one SQLite file, one long-lived exclusive connection, and no WAL, SHM,
  rollback-journal, or owner sidecar at transaction/close/reopen checkpoints;
- exact MEMORY/OFF/temp-memory/cache/mmap/thread/locking PRAGMAs on Create and
  Connect;
- bounded object-admission transactions plus one final visibility transaction
  for Initialize and Commit; one transaction for Fork and Add;
- visibility pointer is the final mutating statement;
- no object `UPDATE` or `DELETE` statement;
- no transfer membership, scope, placement, or complete-root SQL.

### 18.2 Identity, naming, and isolation

- EntityName boundary lengths and every invalid form;
- two named LayerStacks in one Store;
- typed duplicate LayerStack name conflict;
- `main` Branches in different LayerStacks;
- typed duplicate Branch name conflict within one LayerStack;
- names do not alter any deterministic or content ID;
- Branch ownership cannot change across base/head movement;
- two LayerStacks, concurrent Workspaces, FUSE/materialization, Commit, and Add
  do not cross-route records or objects.

### 18.3 Content, Fork, Commit, and Add

- zero-copy Layer Fork and Branch/Commit Fork;
- Branch/Commit Fork accepts inherited history and creates a new BranchId;
- no-op Commit writes no rows and grows no Store file;
- small overwrite, prepend, append, truncate, sparse write, rename, chmod,
  unlink, hard link, symlink, directory tree, and mixed deterministic edit set;
- changed file payload and persistent file-tree candidate bytes do not scale
  with unrelated payload bytes or history;
- candidate insertion is missing-only through the object primary key;
- Commit failure at every statement never exposes a partial head;
- candidate telemetry proves
  `candidate = batch_inserted + final_inserted + preexisting_reused` in objects
  and bytes, with transaction maxima below 128 objects and 4 MiB;
- concurrent Commit admits one CAS winner and returns `CommitHeadMoved` to the
  loser;
- Add copies zero objects, is idempotent by Branch/Commit source, and CASes the
  LayerStack head;
- stale Add requires explicit reconciliation;
- all three reconciliation choices and path-scoped invalidation;
- visible missing or corrupt canonical objects return `Integrity`.

### 18.4 Memory and transaction bounds

- cold 32 MiB and larger candidates spill without unbounded resident memory;
- 32 MiB, 512 MiB, many-small-object, and concurrent-Workspace cases keep host
  Store/SDK RSS below 128 MiB, aggregate lifecycle RSS below 256 MiB, candidate
  size growth below 16 MiB, and produce zero swap/OOM;
- many small edits mixed with medium/large edits remain bounded;
- history over 512 records pages correctly;
- no content hashing, full closure traversal, full history traversal, FUSE I/O,
  execution, or filesystem scan occurs inside a writer transaction;
- pretransaction membership statements are `O(candidate IDs / 128)`, every
  object transaction contains fewer than 128 objects and 4 MiB payload, the
  writer statement count is `O(membership-missing candidate objects) + O(1)`,
  and Fork/Add metadata remain `O(1)`;
- Commit SQL statement count is independent of old Commit history, unrelated
  objects, unrelated LayerStacks, and unchanged file size; current namespace
  planning may still scale with total visible paths as documented in section
  12.

### 18.5 Workspace, FUSE, SDK, CLI, and Monitor

- shared writable lease and exact head/base pinning;
- real host FUSE and prepared-container FUSE when supported by the host;
- materialization and FUSE produce identical canonical roots;
- deferred FUSE errors surface at synchronization boundaries;
- fresh-process Exec/Shell, bounded output, exact Stop, Clean, and Discard;
- CLI calls public SDK and SDK calls one Store operation; neither owns SQL;
- deleted CLI grammar fails parsing and deleted public APIs do not compile;
- completion displays names but substitutes typed IDs;
- JSON schema version 4 has no scope/mode/through/authority fields;
- passive Monitor performs zero Store SQL;
- explicit dedup equations and storage counts are exact;
- benchmark reports contain current-source raw evidence and all lifecycle
  checkpoints from section 17.

## 19. Terminal gates

After every semantic change, run the smallest focused test and its direct
dependents. Diagnose root cause before widening the gate. Terminal requires:

```text
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

Structural inspection must additionally prove:

```text
no Cargo member or dependency on layerfs-storage
no Cargo member or dependency on layerfs-branch-store
no BranchStore or LayerStackEndpoint public symbol
no Pull or Push public/CLI operation
no Reference, Replica, ServingMode, placement, parent route, or Authority query
no scope, receipt, Store identity, project, Workspace, or operation table
no remote Store binary
no TUI/Ratatui/Crossterm restoration
exact standalone SQL manifest and no embedded application SQL
```

Run the current public-SDK fs-bench-plus campaign after correctness gates. Save
raw artifacts and append the iteration report under
`benchmark-results/fs-bench-pro`. A regression, unexplained phase, failed hard
gate, invalid environment, or missing proof requires diagnosis, correction,
and another focused cycle.

Live FUSE and Docker/container gates may be capability-gated only when the
current host genuinely lacks the device or daemon. A capable-host skip is not a
pass. Stop only for a proven external blocker after completing all independent
in-repository work and reporting the exact external action required.

## 20. Explicitly deferred work

The replacement deliberately defers:

```text
remote synchronization and transport
Pull and Push equivalents
Reference/Replica or other serving policies
cross-machine authority and permissions
Store migration/import
garbage collection and object deletion
LayerStack and Branch rename
backup and disaster recovery
explicit stable/checkpointed durability profile
OverlayFS
persistent execution worker pool or prewarmed shell/workload
TUI
operation-aware incremental namespace planning, if not completed in V2
```

Future work must introduce these as explicit new architecture. None is a reason
to retain two-Store V2 code in this local replacement.
