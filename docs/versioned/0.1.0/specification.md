# LayerFS 0.1.0 product specification

> **Status:** Release-candidate specification, normative for the proposed
> LayerFS 0.1.0 tag.

## Architecture

LayerFS has one durable database:

```text
LayerStackStore
  named LayerStacks
  immutable Layers
  named writable Branches
  immutable Commits
  one deduplicated canonical-object namespace
```

A Workspace is ephemeral and has no database. One SDK `Client` binds exactly
one local `LayerStackStore`, one Monitor, one Workspace manager, and one worker
for each active Workspace ID. A second Store requires a second Client and has
an independent identity and object namespace.

Every durable read resolves from the bound Store. A visible root that refers
to a missing or invalid canonical object returns `Integrity`.

## Identity and names

```text
LayerStackId  17 tagged UUIDv7 bytes
BranchId      17 tagged UUIDv7 bytes
LayerId       33 tagged deterministic bytes
CommitId      33 tagged deterministic bytes
ObjectId      32 content-derived bytes
WorkspaceId   ephemeral generated 16-byte typed ID
ExecutionId   ephemeral generated 16-byte typed ID
```

LayerStacks and Branches use immutable `EntityName` values matching:

```regex
^[a-z0-9](?:[a-z0-9._-]{0,61}[a-z0-9])?$
```

LayerStack names are unique within a Store. Branch names are unique within a
LayerStack and may repeat in different LayerStacks. Names never participate in
content, Layer, or Commit identity.

The durable records are:

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

Objects, Commits, Layers, IDs, names, and Branch ownership are immutable.
Publication advances Branch and LayerStack head pointers transactionally.

## Content and persistence

`layerfs-content` owns canonical encoding, authentication, content-defined
chunking, ropes and extents, filesystem transformations, reconciliation,
hashing, and deterministic root construction. It has no SQL dependency.

FUSE operations update ephemeral Workspace state and a bounded dirty frontier.
They do not write durable rows per syscall. At Commit, LayerFS builds one
authenticated candidate root. Existing objects are reused by `ObjectId`, and
only the modified filesystem frontier emits new immutable objects.

The `objects` table permits reads and content-addressed inserts. Canonical bytes
are not updated or deleted by 0.1.0 operations.

Small candidates stay in memory. Larger candidates spill to an ephemeral file
and enter SQLite in bounded pages. The spill is cleaned after success, failure,
or Workspace cleanup.

## Durable operations

### Initialize LayerStack

Initialization accepts an immutable name and either an empty root or an
existing directory. It constructs the genesis root, admits canonical objects,
and atomically publishes the genesis Layer and LayerStack. The result contains
both IDs.

### Fork Branch

Fork accepts a new Branch name and either a Layer or a Commit selected from one
Branch's ancestry. It creates a new Branch ID pointing at the selected
immutable state and copies zero canonical objects.

### Workspace Create

Create reads the Branch, base, head, and effective root from one consistent
Store snapshot; acquires one in-process writable lease for that Branch; and
returns after the requested projection is ready.

### Workspace Commit

Commit pauses mutation, captures the final Workspace state, performs bounded
candidate membership, admits only missing objects, inserts one immutable
Commit, and compares and swaps the Branch base/head. The Branch pointer is the
last visibility write. A root already current returns `UpToDate` without a
write transaction. Competing publication returns the typed actual head.

### Add Layer

Add publishes a Branch head as the next Layer using its existing root. It
copies zero canonical objects and advances the LayerStack head with a
compare-and-swap. No filesystem content beyond the Branch base yields
`NoChanges`; an already published Branch/Commit source yields `UpToDate`.

### Diff and reconciliation

The supported comparisons are Layer-to-Layer, two Commits on one Branch, and a
Branch against a Layer. Diff output is paged. Reconciliation exposes typed
content, type, directory, and hard-link conflicts with Branch, Layer, or
WorkingTree choices. A Commit with unresolved conflicts is refused.

## Transactions and bounds

Filesystem capture, hashing, canonical encoding, Diff, reconciliation, and
candidate construction finish outside SQLite writer transactions.

Object membership uses fixed batches. Each early admission transaction has
fewer than 128 objects and less than 4 MiB of canonical payload. The final
bounded batch shares the visibility transaction. Initialization and Commit may
therefore use several short transactions; Fork and Add use one metadata
transaction.

Publication failure cannot expose a Branch or LayerStack head that references
incomplete state. Immutable objects committed by an earlier admission batch
may remain unreachable and may deduplicate a later candidate.

Frozen bounds:

```text
candidate object memory    8 MiB
candidate reference index  8 MiB
object page                at most 128 objects
object page bytes          at most 4 MiB
history page               at most 128 records
entity query page          at most 512 records
```

## Workspace, FUSE, and execution

One writable Workspace lease exists per Branch among Clients sharing the same
in-process Store owner. The Workspace pins an exact Branch head, base, and root.
Clean End refuses dirty state; Discard End abandons it explicitly.

FUSE and materialization use the same root-keyed object reader. Container FUSE
uses a host-side proxy, capability-authenticated loopback control, one helper
and mount per Workspace, and one fresh process per execution. An owner or
daemon connection loss terminates the work it owns and reports an
infrastructure error.

## Monitoring and deduplication

One passive Monitor belongs to the Client. Operation receipts report the
public operation, typed IDs, outcome, queue/service timing, Store timing, and
candidate insertion/reuse counts where applicable. A passive snapshot performs
no Store query. Exact deduplication analysis is explicit.

For each candidate:

```text
candidate_objects = inserted_objects + reused_objects
candidate_bytes   = inserted_bytes   + reused_bytes
```

For exact SQL, schema, connection, and file contracts, see
[Storage format](storage-format.md). For exported calls, see the
[Rust SDK reference](sdk.md).
