# LayerFS V2 binding specification

Status: binding cold-replacement contract.

This document integrates the Pull-through and LayerStack/Branch naming
refinements. `pull_refinement.md` and `layerstack_branch_name_refinement.md`
remain the design-history proofs for those changes, but this file is the
single consolidated normative V2 contract. Older specifications, handoff
prompts, examples, schemas, APIs, and architecture prose are historical only
where they conflict with this file.

## 1. Product boundary

LayerFS V2 has exactly two durable databases:

```text
LayerStackStore
  authoritative named LayerStacks
  immutable Layers
  pushed named Branch candidates
  immutable Commits
  complete canonical objects

BranchStore
  exact pulled LayerStack prefixes
  local and pulled named Branch histories
  immutable Commits
  locally admitted or replicated canonical objects
  complete-root receipts
  receiver-local serving scopes
```

Workspace is ephemeral and has no database.

One SDK context binds exactly:

```text
one LayerStackStore endpoint
one BranchStore database
one immutable BranchStore -> LayerStackStore parent route
one Monitor
one Workspace worker per active Workspace UUID
```

One database may contain many LayerStacks, Layers, Branches, Commits, and
Workspaces. A second database pair requires a second context. There is no
connection vector, active selector, reparenting operation, Project entity, or
third durable database.

The only runtime Store read route is:

```text
Workspace / Diff / transfer verifier
          |
          v
root-keyed SnapshotReader
          |
          +-- BranchStore local CAS
          |
          `-- exact-missing LayerStackStore fallback, Reference only
```

Branch provenance never selects the object reader.

## 2. Explicit non-goals and removals

V2 contains no:

- V1 or Phase-One compatibility façade;
- head-only or isolated Layer/Commit Pull;
- remote acquisition inside Fork;
- placement-bearing Fork source;
- generic Merge operation;
- Branch-to-Branch Diff;
- Layer Push or Branch Advance operation;
- Project type, table, ID, or namespace;
- per-scope object copy, object refcount, or implicit garbage collection;
- rename operation for LayerStacks or Branches;
- TUI, Ratatui, or Crossterm crate/dependency/installer;
- OverlayFS implementation.

FUSE and explicit materialization are the supported Workspace projections.
OverlayFS is deferred and must not weaken the Store, Pull, ownership, or
completeness contracts when introduced later.

## 3. Identity, names, and immutable facts

### 3.1 Typed IDs

Typed IDs remain authoritative:

```text
StoreId       32 random bytes
LayerStackId  17 tagged UUIDv7 bytes
BranchId      17 tagged UUIDv7 bytes
LayerId       33 tagged deterministic bytes
CommitId      33 tagged deterministic bytes
ObjectId      32 content-derived bytes
```

Layer and Commit identity is recomputed during admission. Names never alter
LayerStackId, BranchId, LayerId, CommitId, ObjectId, CDC boundaries, canonical
encoding, or filesystem identity.

Canonical object identity uses the frozen V2 domain
`layerfs/object/v2\0`. User payload chunks are explicitly framed with the
`LFS4CHK\0` role before object hashing, so a user file prefix cannot be
misclassified as an internal canonical object.

### 3.2 EntityName

`EntityName` is immutable and validated at the API boundary and by SQLite:

```text
length: 1..=63 ASCII bytes
first:  [a-z0-9]
last:   [a-z0-9]
middle: [a-z0-9._-]*
```

LayerStack names are unique within one LayerStackStore. Branch names are unique
within immutable `LayerStackId` ownership. The same Branch name may therefore
exist in different LayerStacks in the same Store.

Pull preserves the authority name exactly. A different incoming ID claiming an
existing scoped name returns typed `LayerStackNameConflict` or
`BranchNameConflict`, including the existing and incoming IDs. Names reserve
their uniqueness as soon as immutable facts are admitted, even if interrupted
Pull leaves the facts unscoped and invisible.

### 3.3 Immutable fact model

```rust
struct LayerStackFact {
    id: LayerStackId,
    name: EntityName,
}

struct LayerFact {
    id: LayerId,
    layer_stack_id: LayerStackId,
    parent_layer_id: Option<LayerId>,
    root_id: ObjectId,
    source_branch_id: Option<BranchId>,
    source_commit_id: Option<CommitId>,
}

struct BranchFact {
    id: BranchId,
    layer_stack_id: LayerStackId,
    name: EntityName,
    forked_from_layer_id: Option<LayerId>,
    forked_from_branch_id: Option<BranchId>,
    forked_from_commit_id: Option<CommitId>,
}

struct CommitFact {
    id: CommitId,
    root_id: ObjectId,
    parent_commit_id: Option<CommitId>,
    base_layer_id: LayerId,
}
```

`BranchFact` has exactly one origin form: Layer, or Branch plus Commit.
`Branch.layer_stack_id` is immutable. Mutable heads, bases, receiver-local
scopes, serving modes, and completeness receipts are not signed fact fields.

Wire encoding uses one canonical `signing_bytes` representation for full
immutable facts. V2 does not claim an in-repository cryptographic authority
signature or Store transport: `LayerStackEndpoint` is the authenticated
in-process boundary, while the public frame codecs are available to a future
transport. Membership compares the full canonical fact, not merely an ID. A
matching ID with different immutable bytes is `Integrity`.

## 4. Exact schema

Both databases use SQLite `STRICT` tables, foreign keys, WAL, synchronous
`FULL`, a 5-second busy timeout, a fixed page cache, and `PRAGMA user_version=3`.
V2 performs no migration from an older schema; an old or structurally different
database is rejected.

The application IDs are:

```text
LayerStackStore 0x4c46534c
BranchStore     0x4c465342
```

### 4.1 LayerStackStore: exactly 6 tables / 25 columns

```text
store(2)
  singleton INTEGER PRIMARY KEY CHECK singleton=1
  store_id BLOB UNIQUE NOT NULL, 32 bytes

objects(2)
  object_id BLOB PRIMARY KEY, 32 bytes
  bytes BLOB NOT NULL

commits(4)
  commit_id BLOB PRIMARY KEY, 33 bytes
  root_id BLOB NOT NULL -> objects
  parent_commit_id BLOB NULL -> commits
  base_layer_id BLOB NOT NULL -> layers

branches(8)
  branch_id BLOB PRIMARY KEY, 17 bytes
  layer_stack_id BLOB NOT NULL -> layer_stacks
  name TEXT NOT NULL, EntityName CHECKs
  base_layer_id BLOB NOT NULL
  head_commit_id BLOB NOT NULL -> commits
  forked_from_layer_id BLOB NULL
  forked_from_branch_id BLOB NULL
  forked_from_commit_id BLOB NULL -> commits

layer_stacks(3)
  layer_stack_id BLOB PRIMARY KEY, 17 bytes
  name TEXT NOT NULL, EntityName CHECKs
  head_layer_id BLOB NOT NULL

layers(6)
  layer_id BLOB PRIMARY KEY, 33 bytes
  layer_stack_id BLOB NOT NULL -> layer_stacks
  parent_layer_id BLOB NULL
  root_id BLOB NOT NULL -> objects
  source_branch_id BLOB NULL
  source_commit_id BLOB NULL -> commits
```

Required composite foreign keys enforce LayerStack ownership for Branch bases,
Branch origins, Layer parents, and Layer sources. Genesis has no parent/source;
non-genesis Layers have all three. Authority Branch heads are non-null.

Required indexes:

```text
UNIQUE layer_stack_names(name)
UNIQUE layer_identity(layer_stack_id, layer_id)
UNIQUE layers_genesis(layer_stack_id) WHERE parent_layer_id IS NULL
UNIQUE layers_child(layer_stack_id, parent_layer_id) WHERE parent IS NOT NULL
UNIQUE layers_source(source_branch_id, source_commit_id) WHERE source IS NOT NULL
INDEX  layers_parent(parent_layer_id)
INDEX  commits_parent(parent_commit_id)
UNIQUE branch_identity(layer_stack_id, branch_id)
UNIQUE branch_names(layer_stack_id, name)
INDEX  branches_head(head_commit_id)
INDEX  branches_fork(forked_from_branch_id, forked_from_commit_id)
```

### 4.2 BranchStore: exactly 9 tables / 33 columns

```text
store(3)
  singleton INTEGER PRIMARY KEY CHECK singleton=1
  store_id BLOB UNIQUE NOT NULL, 32 bytes
  parent_store_id BLOB NOT NULL, 32 bytes

objects(2)
  object_id BLOB PRIMARY KEY, 32 bytes
  bytes BLOB NOT NULL

commits(4)
  commit_id BLOB PRIMARY KEY, 33 bytes
  root_id BLOB NOT NULL
  parent_commit_id BLOB NULL -> commits
  base_layer_id BLOB NOT NULL -> layers

branches(8)
  branch_id BLOB PRIMARY KEY, 17 bytes
  layer_stack_id BLOB NOT NULL -> layer_stacks
  name TEXT NOT NULL, EntityName CHECKs
  base_layer_id BLOB NULL
  head_commit_id BLOB NULL -> commits
  forked_from_layer_id BLOB NULL
  forked_from_branch_id BLOB NULL
  forked_from_commit_id BLOB NULL -> commits

branch_scopes(4)
  branch_id BLOB PRIMARY KEY -> branches
  scope_kind TEXT NOT NULL, local|remote
  through_commit_id BLOB NULL
  serving_mode TEXT NULL, reference|replica

layer_stacks(2)
  layer_stack_id BLOB PRIMARY KEY, 17 bytes
  name TEXT NOT NULL, EntityName CHECKs

layer_stack_scopes(3)
  layer_stack_id BLOB PRIMARY KEY -> layer_stacks
  through_layer_id BLOB NOT NULL
  serving_mode TEXT NOT NULL, reference|replica

layers(6)
  layer_id BLOB PRIMARY KEY, 33 bytes
  layer_stack_id BLOB NOT NULL -> layer_stacks
  parent_layer_id BLOB NULL
  root_id BLOB NOT NULL
  source_branch_id BLOB NULL
  source_commit_id BLOB NULL

complete_roots(1)
  root_id BLOB PRIMARY KEY -> objects
```

An unscoped imported Branch fact may temporarily have null base/head and is
inert. Final local or remote scope publication atomically fills the mutable
pointer and makes it visible. A scoped local Branch has null through/mode. A
scoped remote Branch has both. The composite Branch-scope pointer foreign key
is deferred so an exact C3→C6 pointer/scope advance is atomic.

Required indexes equal the LayerStack indexes where applicable, plus:

```text
UNIQUE branch_pointer(branch_id, head_commit_id)
```

Receiver record and scope pages exclude unscoped facts.

## 5. Lifecycle and ownership invariants

### 5.1 Initialize

Initialization requires an `EntityName` and either an empty source or one
directory. It builds and authenticates canonical objects before opening the
small publication transaction, then atomically publishes the named LayerStack
and genesis Layer. It returns both IDs.

### 5.2 LayerStack and Branch ownership

Every operation independently verifies:

```text
Branch.layer_stack_id
  == Branch base Layer.layer_stack_id
  == every Commit base Layer.layer_stack_id
  == every pushed/added target LayerStackId
```

No Branch can move between LayerStacks during base advancement. Add derives its
target from the Branch rather than accepting a caller-selected LayerStack.

## 6. Pull-through semantics

Pull always means “acquire the selected history through this exact boundary.”
The boundary is never one isolated Layer or Commit.

### 6.1 Layer Pull

For authority prefix:

```text
L1 <- L2 <- ... <- Ln
```

Pull through `Ln` imports the named LayerStack fact and every Layer fact
`L1..Ln` in dependency order.

Reference publishes the exact prefix after facts are admitted. Replica also
transfers the missing union of every root `root(L1)..root(Ln)`, verifies every
closure, and records every complete-root receipt before scope publication.
Objects deleted from later roots therefore remain offline-readable through
older Layers in a Replica prefix.

### 6.2 Branch Pull

Pull through Commit `Cn` preserves authority BranchId/name/LayerStackId and
imports the complete visible ancestry through `Cn`, including ancestry inherited
across Branch origins. It also imports each required immutable origin Branch
fact and every required LayerStack prefix through all selected Commit bases.

Replica completes and receipts every selected Commit root and every required
Layer root. With the parent unavailable, every selected historical Commit and
Layer snapshot must still read. A missing promised object is `Integrity`, not a
network fallback.

Pulled Branches are remote read-only scopes. They may be queried, diffed,
mounted, and read. Commit and Push return typed `ReadOnlyBranch`.

### 6.3 Exact state transitions

For each LayerStack-prefix or Branch-history scope, there is one current
boundary and one serving mode:

```text
no scope                         -> Created
same boundary, same mode         -> UpToDate
same boundary, different mode    -> ModeChanged
requested descendant             -> Advanced
requested ancestor               -> AlreadyContained
incomparable history             -> HeadMoved
```

An older request never moves the boundary backward and never changes the
current mode. Replica→Reference changes serving policy without deleting objects
or receipts. Reference→Replica completes all prior visible roots locally plus
any new remote suffix before publishing Replica.

### 6.4 Incremental transfer

If C3 is visible and C6 is requested, the remote ancestry endpoint returns only:

```text
C6, C5, C4
```

It does not enumerate or retransmit C1..C3. The same stop-exclusive rule applies
to Layer prefixes. Reference→Replica obtains the prior visible prefix from local
facts and combines it with the remote suffix in one union-root traversal.

Facts, objects, and complete-root receipts become durable before the final
scope publication. Interruption before that point may leave reusable inert
facts/objects but no partially visible placement.

## 7. Reference and Replica readers

Each reader pins the record, scope, boundary, base/head pointer, and effective
root from one SQLite read transaction.

Reference:

```text
read local object first
if and only if exact object is absent, read immutable parent route
authenticate returned canonical bytes
```

Replica:

```text
read local only
never retain or call a parent route
missing promised object -> Integrity
```

Mixed-root reconciliation uses per-root policies. A Reference root cannot
weaken another root's Replica promise. A reader for a receipted complete root
retains no parent route.

`complete_roots` is a claim about an authenticated full closure. A receipt is
published only after successful bounded verification. It is not inferred from
the presence of a root object alone.

## 8. Fork, history traversal, Push, and Add

### 8.1 Fork

Fork is local-only:

```rust
fork_branch(name: EntityName, source: LocalForkSource) -> BranchId
```

It requires an already visible local Layer or Commit, creates a new BranchId,
copies zero canonical objects, performs no endpoint call, and publishes a local
writable scope. Branch names are checked atomically within LayerStackId.

### 8.2 Two traversal meanings

The implementation must not conflate:

```text
full visible history
  used by Pull, Diff, Fork membership, and user-visible ancestry

locally owned lane
  used only by Push and authority publication validation
  stops before the immutable fork boundary
```

These are two traversal algorithms over one stored history, not duplicated
history tables.

### 8.3 Push

Push requires a local Branch scope. It validates the exact Layer or
Branch/Commit origin on authority, permanent ownership, name claim, base/head
consistency, and the incoming owned lane before the final write transaction.

Push transfers only locally authored Commits after the immutable origin or an
already acknowledged authority head. It never sends pulled ancestry back to
authority. All suffix roots share one union traversal and Seen domain. Facts are
sent in dependency order. Objects and Commit facts publish before the small
authority name-claim/head-CAS transaction.

An empty Branch-origin Fork at its inherited boundary returns `NoChanges` and
does not create an authority Branch row. Divergent or incomparable authority
heads return `HeadMoved`; Push never silently merges or retries.

### 8.4 Add

Add accepts only a pushed local Branch. Up-to-date Add is a no-op. A stale base
requires explicit Pull and an ephemeral reconciliation Workspace. Add never
Pushes. LayerStack head publication occurs only after resolution Commit and
re-Push.

## 9. Workspace and reconciliation

One writable Workspace lease exists per Branch across all Clients sharing that
BranchStore process. Commit uses exact head/base CAS. Failure preserves final
Workspace state and releases or retains the lease according to lifecycle.

Workspace may use real FUSE or explicit materialization. All reads go through
the same pinned SnapshotReader. FUSE may defer mutation acknowledgements for
throughput, but the first stored error must surface at Fence, fsync, pause,
Commit, or another synchronization boundary.

Reconciliation reports paged typed conflicts:

```text
Content
Type
Directory
HardLink
```

`Branch`, `Layer`, and `WorkingTree` are three distinct choices. Branch restores
the exact Branch-side record, Layer selects current Layer state, and WorkingTree
preserves the validated Workspace state. A later mutation invalidates only a
choice whose affected path intersects the mutated path by equality,
ancestor, or descendant relation. Commit refuses unresolved conflicts.

Operation-history-equivalent final filesystems produce identical canonical
roots. No textual conflict markers or conflict rows enter either database.

## 10. Transfer, deduplication, memory, and transactions

Canonical objects are never rechunked, re-encoded, reminted, copied per scope,
or refcounted per entity during transfer.

Missing-only equations are independent for objects and every fact kind:

```text
announced_ids = sent_ids + avoided_ids
announced_bytes = sent_bytes + avoided_bytes
candidate_bytes = inserted_bytes + reused_bytes
```

Membership uses bounded count/byte pages. History endpoints return at most 128
records; query pages accept 1–512 records. Object transfer uses bounded ID and
object batches and fixed transfer buffers.

One operation-scoped spillable Seen set covers all required roots. Shared
descendants and duplicate roots are visited once. Large history uses a bounded
temporary fact spool. Neither a complete history nor a complete object closure
is materialized as an unbounded in-memory `Vec`.

SQLite writer transactions contain no network call, full history enumeration,
object-closure walk, content hashing, deterministic ID derivation, or unbounded
materialization. Expensive validation occurs first; final transactions admit a
bounded batch or publish one small pointer/scope/CAS change.

## 11. SDK, CLI, query, completion, and JSON

The exact public SDK and CLI operation families are specified in
`sdk-cli-operation-families.md`.

Initialization and local Fork require names. Pull accepts no replacement name.
CLI plans, completion, receipts, JSON, and snapshots call acquisition IDs
`through` boundaries.

Completion displays qualified names with typed IDs and substitutes the exact
ID. Query pages are direct keyset SQL reads, not N+1 point queries. LayerStack
and Branch continuation plans must use indexed `SEARCH`, including
`branch_identity(layer_stack_id, branch_id)` for project-scoped Branch pages.

CLI JSON schema version 3 emits structured query records. Names, LayerStack
ownership, local/remote scope, serving mode, and through boundary are distinct
fields; Rust `Debug` output is not the query payload.

## 12. Monitor contract

One Monitor belongs to the context. Semantic operation receipts include exact
operation family, names where created, Branch/LayerStack IDs, through boundary,
placement, outcome, queue/service time, process fragments, local admission, and
transfer receipts.

A passive snapshot uses retained data and performs zero Store SQL. Exact dedup
analysis is explicit and reports:

```text
physical CAS bytes
union CAS bytes across both Stores
cross-store placement bytes
placement factor
local candidate/inserted/reused bytes
transfer announced/sent/avoided bytes
```

Serving policy and physical coverage are separate: Replica→Reference changes
policy but retained objects and receipts remain visible in physical analysis.

## 13. Frozen production source tree

The production workspace is:

```text
crates/
├── layerfs-content
├── layerfs-storage
├── layerfs-layerstack-store
├── layerfs-branch-store
├── layerfs-workspace
├── layerfs-fuse
├── layerfs-materialization
├── layerfs-monitor
├── layerfs-sdk
└── layerfs-cli
```

`tools/layerfs-eval` is benchmark/evidence tooling, not a product database.
No `layerfs-layer-store`, `layerfs-stack-store`, or `layerfs-tui` member exists.

Handwritten production files remain below 1,500 lines except storage SQL/schema
implementation, which is explicitly exempt from an artificial LOC split. Test
and evidence source may exceed that limit where a single end-to-end proof is
clearer than splitting it artificially.

## 14. Terminal verification gates

A terminal V2 pass requires all of the following together:

```text
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

Focused proof must additionally cover:

- exact schema version/table/column/index census and old-schema rejection;
- name validation, uniqueness, conflicts, wire framing, and ID independence;
- multiple named LayerStacks in one authority and same Branch name across them;
- complete Layer prefix and inherited Branch history through >512 records;
- older/equal/newer/incomparable Pull outcomes and mode transitions;
- incremental stop-exclusive transfer and Reference→Replica local prior prefix;
- historical objects absent from later roots;
- offline full-history Replica and missing promised object `Integrity`;
- scope-last interruption behavior;
- zero-copy/no-network local Fork;
- full-history membership versus owned-lane Push;
- Push suffix-only receipts and no ancestry retransmission;
- no network/hash/history enumeration inside publication transactions;
- bounded transfer membership, union-root deduplication, and known-root pruning;
- read-only remote Branch Commit/Push and writable local Fork;
- all three reconciliation choices and path-scoped invalidation;
- exact query pagination, indexed plans, structured JSON, and named completion;
- shared Workspace lease, head CAS, materialization, real FUSE, and synchronization
  error surfacing;
- passive Monitor zero-SQL and exact dedup/placement equations;
- current FUSE benchmark results, raw evidence, commit, dirty-source seal,
  timestamp, host/runtime metadata, and comparison provenance.

Live host FUSE and Docker/container gates may be opt-in when the platform lacks
the capability, but a terminal report must run them when the current host has
the required FUSE device and Docker daemon. A skipped capable-host gate is not a
pass.
