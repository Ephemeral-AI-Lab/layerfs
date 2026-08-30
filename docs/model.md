# LayerStack storage model

> **Historical and superseded.** The former three-store model below is not a
> V2 compatibility path. [`v2/spec.md`](v2/spec.md) is the sole authority.

`LayerStack` is the whole architecture. It is not a row, ID, history, store,
or operation target.

```text
LayerStack
├── BranchStore     builds Branches and Commits
├── StackStore      optionally builds linear StackHistories
└── LayerStore      stores the complete central model and builds Layers
```

## Authority and companion specifications

This document is authoritative for the three-store architecture, history
shapes, identity relationships, and 3/8/8 schema boundary. [rule.md](rule.md)
is authoritative for the fourteen public operations and their semantic
invariants. [db-transaction-transfer-model.md](db-transaction-transfer-model.md)
is the binding low-level specification for Store API execution, indexed
membership/search, batching, SQLite transactions/durability, visibility,
deduplication, large transfers, CPU/memory bounds, and their proof tests.

The low-level specification cannot add, remove, rename, or change a public
operation or architecture invariant. The detailed mechanics below retain
rationale and bounds; if a repeated mechanical detail diverges, the binding
DB/transfer document wins and this model must be corrected rather than creating
a second implementation path.

## 1. Terms

| Term | Exact meaning |
|---|---|
| `BranchStore` | Private store for Branch refs, CommitHistory, and new local CAS objects. |
| `StackStore` | Optional intermediate store that owns locally created StackHistory heads and holds selected read-only central data. |
| `LayerStore` | Complete central store for Branches, Commits, StackHistories, Stacks, LayerHistories, Layers, CAS objects, and add results. |
| `Branch` | Mutable ref to one Commit with exactly one tagged `LayerId` or `StackId` base. |
| `Commit` | One immutable local filesystem snapshot. Its root is always an `ObjectId`. |
| `Stack` | One immutable intermediate filesystem snapshot. |
| `Layer` | One immutable accepted filesystem snapshot. |
| `CommitHistory` | DAG formed by Commit parent links; no ID or table of its own. |
| `StackHistory` | Strict parent-linked list of Stacks, based on one Layer, with one mutable head. |
| `LayerHistory` | Strict parent-linked list of Layers with one mutable head. |

Commit, Stack, and Layer use the same canonical filesystem-tree representation.
Their names describe lifecycle and authority, not different payload formats.

## 2. Topologies

### Direct

```text
BranchStore ------------------------------------> LayerStore

Layer L10 -> Branch B1 -> Commit C1 -> Layer L11
```

There is no hidden Stack. A Branch is based on an exact tagged `LayerId`.

```text
push_branch(B1)     = transfer Branch/Commit/new objects only
add_layer(H1, B1)   = three-way check + one LayerHistory head CAS
```

### Stacked

```text
BranchStore ------------> StackStore ------------> LayerStore

Layer L10 -> Stack S1 -> Branch B1 -> Stack S2 -> Layer L11
```

A Branch is based on an exact tagged `StackId`.

```text
push_branch(B1)     = transfer only
add_stack(SH1, B1)  = three-way check + one StackHistory head CAS
push_stack(S2)      = transfer only
add_layer(H1, S2)   = three-way check + one LayerHistory head CAS
```

| Store | Cardinality | Completeness |
|---|---:|---|
| `LayerStore` | exactly 1 | Complete central data and authoritative LayerHistory heads |
| `StackStore` | 0..N | StackHistories plus selected transferred dependencies; only histories matching the configured writer key are writable |
| `BranchStore` | 1..N | Private Branch/Commit state; no accepted payload copy |

## 3. The three histories

### LayerHistory: accepted list

```text
LayerHistory H1

L1 -> L2 -> L3 -> L4
                  ^
                  head_layer_id
```

Every non-genesis Layer has exactly one parent in the same LayerHistory. Only
LayerStore changes the authoritative head by exact compare-and-swap.

### StackHistory: intermediate list

```text
StackHistory SH1
base_layer_id = L4

S1 -> S2 -> S3 -> S4
                  ^
                  head_stack_id
```

Every non-seed Stack has exactly one parent in the same StackHistory. Many
Branches may originate from any retained Stack, but clean results serialize
onto one list.

```text
Branch BA base S1 ----+
                      +-- add_stack against current S3 --> S4(parent S3)
Branch BB base S2 ----+
                      +-- later add_stack against S4 ----> S5(parent S4)
```

The Branch base is the three-way base; it is not required to equal the current
StackHistory head.

### CommitHistory: local DAG

```text
C1 -> C2 -> C3
       \      \
        C4 -> C5 -> C6
                    ^
                    merge Commit with second parent C3
```

A normal Commit has one parent. A merge Commit has a second parent. Branches
are mutable refs into this DAG; there is no BranchHistory or CommitHistory row.

## 4. Core records and IDs

```text
LayerHistory(id, head_layer_id)
Layer(id, history_id, parent_id, root_id)

StackHistory(id, base_layer_id, head_stack_id)
Stack(id, history_id, parent_id, root_id)

Branch(id, head_commit_id, base_id)
Commit(id, root_id, parent_id, merge_parent_id)

AddResult(source_id, result_id)
```

`Branch.base_id` is externally tagged, so routing requires no `base_kind`
column:

```text
LayerId tag -> direct add_layer route
StackId tag -> add_stack route; direct add_layer is forbidden
```

The base object resolves history, root, and ancestry. Branch does not duplicate
a history ID, root ID, parent Branch, or fork Commit.

Every storage-topology ID except `ObjectId` contains an inspectable external
type tag. Mutable/history IDs use independent random UUIDv7 bodies:

```text
BranchId       = branch-tag        || UUIDv7
LayerHistoryId = layer-history-tag || UUIDv7
StackHistoryId = stack-history-tag || verification-key-digest || UUIDv7
```

Immutable topology IDs domain-separate their canonical hash preimages:

```text
CommitId = commit-tag         || H("commit", root_id, parent_id, merge_parent_id)
StackId  = stack-tag          || H("stack", history_id, parent_id, root_id)
LayerId  = layer-tag          || H("layer", history_id, parent_id, root_id)
```

`ObjectId` is the existing untagged 32-byte
`H("layerfs/object\0", canonical_bytes)` digest. Object kind is authenticated
by decoding the canonical bytes in the expected codec context; it is not
inspectable from the digest. Two histories created from the same Layer cannot
collide. Topology tags make Branch routing and AddResult source/result kinds
derivable without a kind column.

## 5. History creation and write authority

### LayerHistory genesis

LayerStore provisioning atomically creates:

```text
LayerHistory H1
Genesis Layer L0(parent null, root canonical-empty-root)
H1.head_layer_id = L0
```

No empty or headless LayerHistory is visible.

### StackHistory seed

`create_stack_history_from_layer(layer_history_id, layer_id)` verifies the
explicit history+Layer pair, then atomically creates:

```text
StackHistory SH1(base L4)
Seed Stack S1(parent null, root = L4.root_id)
SH1.head_stack_id = S1
```

The seed shares the root; it copies no payload.

### Sole StackHistory writer

The StackStore that creates SH1 is its sole head writer. LayerStore and other
StackStores may hold transferred copies, but those copies are read-only.

No per-history StoreId, owner, lease column, or public capability-wrapper type
is added. The frozen operations continue to take `StackHistoryId`; enforcement
uses the StackStore's configured signing key outside the core tables:

```text
writer verification digest = embedded in StackHistoryId
writer capability          = private signing key held by creator StackStore

create locally -> returns StackHistoryId; configured key digest matches the ID
pull remotely   -> copies the same public StackHistoryId but never the private key
add_stack       -> accepts StackHistoryId and verifies the configured key matches
push_stack      -> signs history/head/suffix digest for LayerStore verification
```

`create_stack_history_from_layer` returns IDs, not a capability handle.
`pull_stack_history` returns the same ID-shaped outcome and never transfers the
key. Embedded local configuration creates and persists the key without a
user-managed credential. Cloning a writable StackStore database is unsupported
because it would duplicate one writer. Authority transfer is not supported in
the initial model. Losing the key leaves the history readable but closed to new
Stacks.

Calling `add_stack` for an imported history whose ID does not match the
configured writer key returns
`ReadOnlyHistory<StackHistoryId>` before three-way or mutation.

## 6. Branch creation and lifecycle

Creating a Branch from a Layer or Stack installs a canonical anchor Commit:

```text
base root = R10

Anchor Commit C0(root R10, parent null, merge parent null)
Branch B1(head C0, base tagged L10 or S4)
```

Anchor creation copies zero payload bytes. Equal canonical anchors deduplicate.
`create_branch_from_commit` creates another Branch ref in the same BranchStore,
inherits the same base ID, and reuses the named reachable Commit.

```text
Branch B1: C0 -> C1 -> C2
                         \
                          +-- Branch B2 head C2

B1 later -> C3
B2 later -> C4
```

Commit ancestry derives lineage; no Branch parent/fork columns exist.

A Branch filesystem root is always
`commits[branch.head_commit_id].root_id`, therefore always an `ObjectId`; Branch
does not store another root column.

One `BranchId` may add only one Stack or Layer. The Branch remains readable and
usable as a merge source afterward. `add_results(source_id PK, result_id)`
returns the first successful result when the same source is presented again. In the
accepting StackStore/LayerStore, that AddResult also freezes the same-ID Branch
ref at the exact accepted Commit: a later identical `push_branch` may prove the same
head but may not move it. No accepted-Commit column is needed because the
frozen `branches.head_commit_id` plus `add_results[source_id]` is the complete
mapping. The originating BranchStore may retain its local ref, but the BranchId
has spent its one publication; further publishable work starts from a new
Branch based on the resulting Stack or Layer.

### Cross-base Branch merge

`merge(source_branch_id, target_branch_id, expected_target_head)` may combine
Branches created from different Commits, different Stacks, or different Layers
when they have one provable common immutable ancestor in the same
LayerHistory. Exact base equality is not required.

```text
Commit candidates
├── one closest common Commit -> use it
├── multiple incomparable maximal Commits -> AmbiguousMergeBase
└── zero -> resolve closest common Stack
             ├── found -> use it
             └── zero -> resolve closest common Layer
                          ├── found -> use it
                          └── zero/unrelated histories -> NoCommonBase
```

The resolver first pins the source head and verifies the expected target head.
It then resolves the existing immutable provenance graph with indexed SQLite
recursive CTEs:

```text
Commit -> parent / merge parent
Branch -> tagged Layer or Stack base
Stack  -> parent Stack; seed -> base Layer
Layer  -> parent Layer
Stack  -> resulting Layer through verified AddResult
```

Different LayerHistory IDs are unrelated even when their roots are byte-equal.
Multiple incomparable maximal Commit ancestors return `AmbiguousMergeBase`;
missing ancestry or object data returns `MissingBaseData` and requires Pull
before merge. No merge-base row, Branch-parent column, or virtual base object is
persisted.

Commit ancestry/common/maximal-candidate CTEs use `UNION` deduplication and
SQLite's transient B-tree, then page only final candidates into application
memory. There is no unbounded Rust ancestor `HashSet`. Parent and merge-parent
indexes must appear in the recursive query plan; transient storage/page-cache
bytes are included in memory evidence and never become a product table.

```text
same heads or source already contained -> UpToDate
same base and target is source ancestor -> fast-forward target by CAS
otherwise clean three-way              -> two-parent Commit on target
path conflict                           -> no Commit and no ref movement
```

Only the target Branch moves. Its tagged `base_id` never changes, including a
cross-type merge:

```text
Stack-based source -> Layer-based target -> result remains Layer-based
Layer-based source -> Stack-based target -> result remains Stack-based
```

Thus the target determines the later add route. Merge performs no hidden
mutating Pull and persists no accepted parent object. Its preflight may satisfy
zero-copy base/current/candidate reads through the configured parent using the
same batched layered-read adapter, entirely outside the write transaction. An
unavailable or unverifiable dependency returns `MissingBaseData`. The final
Commit/head transaction is local and small.

Let `L` be actual Store-endpoint layered-parent read turns during preflight,
including indexed structural reads and semantic-digest payload batches. If dependencies are
embedded or already local, network RTT is zero; otherwise preflight costs at
most `L` request/reply turns on the reused parent stream. There is no
per-object turn, `L` is not double-counted as a separate digest multiplier, and
the final write transaction performs zero network I/O.

Workspace tool-operation COW is transient:

```text
tool edits -> in-memory/mount COW -> commit -> objects + Commit + Branch CAS
```

There are no Workspace, draft, tool-operation, or conflict tables. Uncommitted
edits are transient; only Commit makes them durable.

## 7. One shared three-way algorithm

The algorithm lives only in `layerfs-storage-core`.

```text
three_way(base_root, current_root, candidate_root)
    -> Clean(merged_root)
    -> Conflict { path, base, current, candidate }
```

Per filesystem entry:

| Condition | Result |
|---|---|
| current equals base | candidate |
| candidate equals base | current |
| current equals candidate | that value |
| otherwise | conflict |

This combines independent changes. It never chooses a winner for incompatible
changes or writes conflict markers into an immutable snapshot. Traversal order
is canonical lexicographic path order. It returns the first conflict and stops;
there is no unbounded conflict vector, arbitrary truncation, or `truncated`
flag.

| Caller | Base | Current | Candidate | CAS |
|---|---|---|---|---|
| `BranchStore.merge` | closest common Commit, else Stack, else Layer root | target Branch head root | pinned source Branch head root | exact target Commit head |
| `StackStore.add_stack` | source Branch base Stack root | StackHistory head root | source Branch head Commit root | exact `head_stack_id` |
| `LayerStore.add_layer` from Branch | source Branch base Layer root | LayerHistory head root | source Branch head Commit root | exact `head_layer_id` |
| `LayerStore.add_layer` from Stack | StackHistory base Layer root | LayerHistory head root | source Stack root | exact `head_layer_id` |

All divergent content integrations use the same Merkle traversal, conflict
classifications, and canonical result-root construction. Branch `UpToDate` and
fast-forward outcomes do not invoke three-way.

The builder writes candidate canonical objects only to a bounded transient
`DeferredObjectStore`: memory first, then a disposable scratch spill when its
8 MiB budget is reached. Only a wholly `Clean` result is authenticated and
admitted to SQLite in `J` batches before the final metadata/head transaction.
`Conflict` discards scratch and changes zero database rows. Cleanup after an
unexpected process failure is deferred for the experiment environment; scratch
is never product state or referenced by a live ref/head.

`BranchStore.merge` creates a Commit only for `Clean`. Conflict creates no
Commit and moves no Branch head. If target-head CAS loses, its transaction
rolls back the candidate Commit and returns `HeadMoved`; it does not silently
retarget the merge.

Every Store has one active serialized operation gate. An Add enters that gate,
reads the current history head, completes preflight/three-way outside any
SQLite write transaction, and executes one exact head CAS. Other callers queue
and later evaluate once against the newly visible head. Therefore normal
concurrent callers do not race the CAS. An injected or otherwise illegal head
movement makes the one CAS fail, rolls back the final transaction, and returns
`HeadMoved`; there is no internal retry loop.

## 8. `add_stack`

Preconditions:

1. StackHistory is locally owned by the executing StackStore.
2. Source Branch has a tagged `StackId` base belonging to that StackHistory.
3. Source Branch/Commit/root closure is fully available.
4. `add_results` has no row for the Branch, unless returning its prior result.

A source Stack from another history returns
`WrongHistory<StackHistoryId>`. A pulled/read-only history returns
`ReadOnlyHistory<StackHistoryId>` before three-way.

Transaction:

```text
read checked_head = SH1.head_stack_id
three_way(source base Stack, checked head Stack, Branch Commit)

Conflict:
    create no Stack
    move no head
    write no AddResult

Clean and merged root == checked head root:
    write BranchId -> checked_head AddResult
    return UpToDate

Clean with new root:
    insert Stack(parent = checked_head, root = merged root)
    CAS SH1.head_stack_id: checked_head -> new Stack
    insert BranchId -> new Stack AddResult
    commit atomically
```

The serialized Store gate makes one exact CAS attempt. An injected/illegal CAS
loss rolls back Stack and AddResult and returns `HeadMoved<StackId>` without
re-evaluation or a sibling Stack.

## 9. `add_layer`

Legal sources:

| Source | Requirement |
|---|---|
| `BranchSource` | Branch base tag is `LayerId`; its Layer belongs to target LayerHistory. |
| `StackSource` | StackHistory base Layer belongs to target LayerHistory. |

A Stack-bound `BranchSource` is rejected as `WrongSourceRoute`.
Any source whose base Layer belongs to another history returns
`WrongHistory<LayerHistoryId>` before three-way.

`add_layer(layer_history_id, source)` has no public expected-head argument. The
LayerStore internally snapshots the exact head used by three-way and CAS.

```text
Conflict:
    create no Layer
    move no head
    write no AddResult

Clean and merged root == checked head root:
    write source -> checked head AddResult
    return UpToDate

Clean with new root:
    insert Layer(parent = checked head, root = merged root)
    CAS LayerHistory head
    insert source -> new Layer AddResult
    commit atomically
```

The serialized Store gate makes one exact CAS attempt. An injected/illegal CAS
loss rolls back Layer and AddResult and returns `HeadMoved<LayerId>` without
re-evaluation.
Existing Layers are never overwritten, so LayerHistory remains a strict list.

## 10. Pull and push

Push transfers immutable data and refs only. It never calls three-way, creates
a Stack/Layer, moves an authoritative creator Stack head, or moves a Layer
head. `push_stack` may exact-CAS LayerStore's verified read-only copied
`head_stack_id` as transferred metadata.

| Transfer | Effect |
|---|---|
| `push_branch` | Missing Commit metadata/new objects plus the Branch ref to configured parent. |
| `push_stack` | Missing StackHistory/Stack/AddResult data plus every accepted Branch ref, Commit DAG, and root-object dependency proving the pushed suffix; may fast-forward LayerStore's read-only copied StackHistory head. |

Pull also transfers only and never returns a merge conflict.

```text
pull_layer_history(H1, through L4)
    -> LayerHistory metadata + missing Layer prefix + root closure

pull_stack_history(SH1, through S3)
    -> base LayerHistory first + missing Stack prefix + root closure

pull_branch(source_branch_id, local_branch_id)
    -> exact tagged base dependency first
    -> missing Commit DAG metadata
    -> verify accepted root closure through configured parent without copying it
    -> create or conditionally move only local_branch_id
```

`pull_branch` never overwrites or merges a divergent local ref:

| Local destination state | Result |
|---|---|
| `local_branch_id` is fresh | Insert a local Branch with the source's exact tagged base and pinned head. |
| Same-ID local ref is absent | Insert the same-ID local Branch. |
| Same-ID heads equal | `UpToDate`; no ref mutation. |
| Same-ID local head is an ancestor of source | Exact-CAS fast-forward the local ref after dependency admission. |
| Same-ID source head is an ancestor of local | Local is ahead; `UpToDate`, never rewind. |
| Same-ID heads truly diverge | Return `HeadMoved`; mutate no ref and perform no hidden merge. |
| A different `local_branch_id` already exists | Return `HeadMoved`; a fresh ID is required for import. |

To resolve divergence, pull the source into a fresh local BranchId, call the
existing `merge(fresh_source, target, expected_target_head)`, then
`push_branch(target)`. This uses the existing operation surface and adds no
tracking column or hidden merge.

### `pull_commit_history(branch_id)`

This is a StackStore operation for reading a remote Branch's Commit DAG without
creating or moving a local Branch ref.

```text
1. read remote Branch head once and pin exact CommitId C8 for this pull
2. prepare the Branch's exact Layer/Stack base dependency
3. pull missing Commit parents reachable from C8
4. pull missing CAS objects needed by those Commit roots
5. return pinned head C8
6. do not insert/update StackStore.branches
```

The remote Branch may advance later; this pull remains an exact snapshot at C8.

### Replicated StackHistory head during `push_stack`

LayerStore may update its non-authoritative copied `head_stack_id` only after
verifying the creator signature against the verification-key digest embedded in
`StackHistoryId`.

For every `BranchId -> StackId` AddResult whose result Stack belongs to the
transferred suffix, `push_stack` includes this provenance closure:

```text
AddResult(BranchId -> StackId)
    -> frozen same-ID Branch ref
    -> exact accepted head Commit
    -> missing Commit parent/merge-parent DAG
    -> every required Commit root object
    -> mapped Stack manifest/root in the signed suffix
```

The StackStore enumerates AddResults by the existing `add_results(result_id)`
index. It proves that each result names the suffix Stack, the Branch base is a
Stack in the same StackHistory, the Branch ref still names the accepted Commit,
and every Commit/Stack root authenticates and closes. A seed Stack needs no
Branch provenance. LayerStore negotiates typed facts and objects against their
own tables and admits only missing rows. A preexisting same-ID Branch with a
different head, a wrong result mapping, a wrong-history base, or a root mismatch
is `Integrity`; copied-head visibility does not move.

The signed suffix digest covers the ordered Stack manifests, transferred
AddResults, frozen BranchId/head-Commit pairs, and their typed provenance. The
signed request digest covers the table-specific typed and Object provenance
frontiers. Verification precedes the copied-head CAS; ID namespace equality
alone grants no authority. Complete Branch refs may become visible only after
their own Commit/root closure; the copied Stack head becomes visible only after
the entire suffix provenance is admitted.

| Relationship to copied head | Result |
|---|---|
| equal | `UpToDate` |
| incoming head is an ancestor of copied head | `UpToDate`; never rewind |
| incoming head is a verified descendant | transfer missing suffix, then fast-forward copied head |
| divergent or reparented | integrity error; copied head unchanged |

This transfer never runs three-way, creates/reparents a Stack, creates a Layer,
or grants write authority. It remains missing-only transfer plus copied-head
visibility. Because LayerStore now has the frozen Branch ref and Commit DAG,
`pull_commit_history(branch_id)` can serve every centrally accepted Stack
provenance without consulting the creator StackStore.

## 11. Layered CAS, CDC, and transfer

The mechanics in this section are summarized architecture constraints. The
binding executable contract—including exact Store envelopes, SQL shapes,
transaction formulas, large-file pipeline, discard/retain behavior, and test
matrix—is [db-transaction-transfer-model.md](db-transaction-transfer-model.md).

BranchStore never copies accepted payload into its database:

```text
read ObjectId
├── local/private -> BranchStore objects
└── accepted      -> configured parent objects
```

`commits.root_id` therefore has no local BranchStore foreign key. The layered
resolver validates the combined local+parent closure before Commit admission.

Commit, Stack, and Layer may share one root:

```text
Commit C8 ----+
Stack S4 -----+--> Root R9 --> trees --> CDC chunks
Layer L11 ----+
```

CAS admission is children-before-parent and metadata-last:

```text
chunks -> child trees -> parent trees -> root
       -> immutable Commit/Stack/Layer facts
       -> AddResult / Branch ref / history or copied head visibility last
```

Presence of a tree/root ObjectId therefore certifies complete reachable
closure. Top-down negotiation prunes known subtrees and transfers missing IDs
in bounded batches.

Normal `add_stack`/`add_layer` authenticates the typed source/current/base
manifests and their root IDs; it does not re-walk a root whose presence already
certifies closure. A repeated/no-op Add performs zero descendant reads. A
divergent Add visits only the unequal Merkle frontier. Full descendant traversal
belongs to first admission, not every Add; scrub/recovery is deferred.

The store, not transport, owns deduplication. Local admission and transfer
admission use different shortest paths:

```text
Local Commit:
    CDC newly supplied bytes outside transaction
    -> canonical-encode once + ObjectId-hash once
    -> trusted staged (ObjectId, canonical_bytes)
    -> reuse authenticated unchanged extent ObjectIds
    -> bounded object batches
    -> prepared INSERT ... ON CONFLICT DO NOTHING
    -> no per-chunk existence query

Cross-store transfer:
    announce bounded typed page and/or ObjectId page
    -> receiver performs table-specific membership queries
    -> receiver returns separate typed/Object missing bitmaps
    -> sender authenticates stored canonical row at most once and sends it unchanged
    -> receiver hashes/authenticates each missing frame exactly once, never re-encodes
    -> prepared INSERT ... ON CONFLICT DO NOTHING handles races

Both:
    one WAL transaction per bounded object/fact batch
    -> chunks/children first, trees/parents next
    -> closure-complete Commit/Stack/Layer/AddResult facts in dependency order
    -> Branch ref or history/copied head becomes visible in the last bounded transaction
```

There is no query or transaction per chunk and no persistent transfer table.
Object primary-key uniqueness resolves concurrent negotiation races and retries.
Push never deletes sender objects; GC remains deferred.

Normal local admission does not re-hash its trusted staged pairs inside the
SQLite batch adapter. Remote transfer does not CDC or canonical-encode. A
scratch-spill reread may re-authenticate a staged object for corruption safety;
that exceptional hash is counted separately from the normal-path one-hash
budget. Test counters reject duplicate encoding/hashing on every normal path.

One coherent CAS pipeline serves every path. CDC and canonical encoding occur
only when new logical bytes enter a local CAS. An incremental edit sends only
its newly supplied byte stream through CDC; authenticated unchanged extents
remain content-addressed inputs to the new mapping tree. Pull/Push start from
stored roots and ObjectIds and send already-stored canonical bytes; they never
rechunk, re-encode, or re-hash logical files.

```text
new or rewritten byte stream
    -> one CDC implementation + one parameter set
    -> canonical type-domain encoding
    -> one ObjectId over those canonical bytes
    -> children-before-parent closure admission
    -> subtree-hash-pruned missing-ID negotiation
    -> idempotent INSERT by ObjectId
    -> Commit / layered read / Pull / Push / add_stack / add_layer
```

| Consumer | Required use of the pipeline |
|---|---|
| `commit` | Chunk newly supplied bytes once; reuse authenticated unchanged extent ObjectIds; admit the new root before Commit. |
| Layered read | Resolve one ObjectId locally first, then through configured parent; never copy the accepted closure. |
| Pull/Push | Enumerate stored roots/ObjectIds, probe in batches, and send receiver-missing canonical rows exactly; CDC invocation count is zero. |
| `add_stack` | Reuse three-way result root; create no payload copy for the Stack row. |
| `add_layer` | Reuse three-way result root; create no payload copy for the Layer row. |

There is no alternate chunker, eager materialization path, or full-copy
fallback.

The sole CDC profile is the existing hashed FastCDC profile:

```text
minimum = 8 KiB
target  = 16 KiB
maximum = 32 KiB
profile = canonical hashed profile ID
```

The storage contract has one content identity:

| Identity | Exact role |
|---|---|
| `ObjectId` | Hash of authenticated canonical object bytes; this is the `objects` primary key and transfer identity for payload, tree, and root objects. |

Canonical encodings carry their object-kind domain. `FileState` carries the
mapping/CDC profile reference. The current `ChunkId` alias over a raw-byte hash
is neither persisted nor transferred and is not part of the cold storage API;
remove it from this pipeline rather than pretending it is a second identity.
Storage code must not store raw bytes under an `ObjectId`, silently mix a raw
payload hash with a canonical-object hash, or invent a third identity.

### Incremental-edit contract

V1 deliberately preserves authenticated COW extent splicing:

```text
old canonical extent sequence
       | split at edit range
       v
unchanged prefix + FastCDC(replacement bytes) + unchanged suffix
       |
       v
deterministic splice/rebalance -> new FileState root
```

It does **not** rescan old surrounding bytes to find a new whole-file CDC
alignment, and it does not full-file rechunk. Equal final logical file bytes
reached through different edit histories are therefore not required to have
the same extent segmentation or `FileState` root. Every individual object is
still canonical and authenticated, and retained extents preserve maximal
payload reuse.

Let `x` be supplied replacement bytes and `t` be extent-tree nodes read or
rebuilt by split/concat. Normal edit cost is `O(x + t)` time and bounded CDC
memory; it is not `O(file_bytes)`. The implementation records
`cdc_bytes_scanned == x` and must not hide a surrounding/full-file rescan.

Root inequality must not create a false merge conflict for logically equal
files. `three_way` applies ordinary `ObjectId` fast rules first. Only when a
regular-file leaf still appears divergent does `merkle.rs` compare logical
lengths and stream each distinct base/current/source root at most once through
the existing `ContentDigestWriter` and layered batch reader. It caches at most
three transient digests and applies the semantic rules in this exact order:

```text
semantic_eq(source, base)   -> keep destination/current
semantic_eq(current, base)  -> take source/candidate
semantic_eq(source, current)-> keep destination/current
otherwise                   -> Conflict
```

The fallback is outside every write transaction. For maximum file length
`B_file`,
`S` structural rope-node visits, and extents `E_i` per distinct root, it costs
at worst `O(3*B_file)` streamed bytes, `S` individual indexed structural reads, and
`O(sum(ceil(E_i/64)))` payload-range batches. It uses `O(1)` memory and creates
no object, ID, or persistent state. V1 does not pretend the existing rope
walker batches those structural reads.

| Work | Bound |
|---|---|
| Exact object/head lookup | indexed `O(1)` expected |
| Incremental file edit | `O(x + t)` for replacement bytes `x` and touched extent-tree work `t`; no old-suffix CDC scan |
| Transfer | `O(ceil(N/B))` ID batches + `O(m)` missing payload bytes |
| Pull ancestry | `O(a + m)` missing rows and payload |
| Three-way Merkle check | `O(v + d*h)` visited nodes plus changed leaves over height `h` |
| Equal-content merge fallback | worst `O(3*B_file)` bytes, `S` indexed structural reads, and `O(sum(ceil(E_i/64)))` payload batches |
| `add_stack` / `add_layer` | one node, one AddResult, one head CAS, genuinely new objects only |

`N` is probed IDs after known-subtree pruning, not full closure; `B` is batch
size; `m` is missing bytes.

### Fixed transfer budgets

Initial constants are deliberately boring and bounded:

```text
ID_BATCH_COUNT      = 512 ObjectIds
OBJECT_BATCH_COUNT  = 128 objects
OBJECT_BATCH_BYTES  = 4 MiB canonical bytes
FACT_BATCH_COUNT    = 128 fixed-width typed rows
FACT_BATCH_BYTES    = 64 KiB row fields
FINAL_METADATA_BYTES <= 64 KiB
FINAL_METADATA_STATEMENTS <= 8
```

The deterministic greedy packer preserves dependency order and stops a batch
before the next object would cross either count or byte bound. A valid
canonical object larger than 4 MiB but no larger than `MAX_OBJECT_BYTES`
(currently 16 MiB) is admitted as one singleton batch; it is never rejected or
split merely to satisfy the target. Fixed-width immutable fact rows use the
same rule with their fact bounds.

Let `O = max(OBJECT_BATCH_BYTES, MAX_OBJECT_BYTES)`, currently 16 MiB. A
transfer operation holds at most two `O` buffers plus two ID/bitmap pages,
bounded framing/metadata, and Merkle traversal state:

```text
transfer buffers < 34 MiB per active operation at current limits
three-way traversal + in-memory DeferredObjectStore <= 8 MiB
application operation working set < 42 MiB
SQLITE_PAGE_CACHE_BYTES = benchmark-frozen P1 bound
SQLite temp_store = FILE; recursive-CTE spill is not retained in RAM
bounded total < 42 MiB + SQLITE_PAGE_CACHE_BYTES + fixed SQLite overhead
```

No complete closure list, filesystem, ancestor set, or conflict list is held in
application memory. Tests record SQLite page-cache high-water and temp-file
bytes in addition to the application buffers.

### SQL and transaction formulas

Let:

```text
A_t = typed ancestry/membership rows for exact typed table t after pruning,
      including history, Branch, Commit, Stack, Layer, and AddResult provenance
H   = emitted typed pages, sum_t ceil(A_t / 512) over nonempty table sets;
      a source recursive CTE may emit them from one read-only statement
P_o = actual 512-ObjectId membership pages after known-root pruning
P   = actual coalesced dependency-ordered wire turns after piggybacking typed
      and Object announcements; P <= P_o + H
J   = actual object insert batches emitted by the count+byte packer
F   = immutable typed-fact batches plus frozen provenance-Branch batches
L   = actual Store-endpoint layered-parent read turns during preflight
D   = final metadata statements, hard bounded at 8
C   = merge-base recursive CTE statement count, 1 through 3
S   = actual indexed structural-node reads by existing logical walkers
E   = payload extents read across logical streams
G   = actual 64-entry payload-read batches
```

Every ObjectId membership page counted by `P_o` contains at most
`ID_BATCH_COUNT` ObjectIds.
Each `H` page is homogeneous to one exact typed table. Paged history,
Branch/AddResult, Commit, Stack, and Layer provenance belongs to `H`; point
current-head/scope/attestation preflights are counted separately as
`operation_preflight`, not folded into `H`.
`max(ceil(count/count_limit), ceil(bytes/byte_limit))` is only a lower bound
and must not be used as `J`; object-size ordering can require more greedy
batches. `F` is the same deterministic packer count under fixed fact bounds,
including frozen Branch-provenance rows rather than hiding them in `D`.

At Store open, prepare exactly one 512-placeholder ObjectId existence query and
the widest 128-row/four-column fact insert (512 binds); open first requires
SQLite 3.35 or newer, then fails if either statement cannot be prepared. Every
frontier input is sorted and
duplicate-free, unused placeholders are bound to `NULL`, unordered returned
IDs are mapped back to input positions, and the reply is one fixed 512-bit
missing bitmap. Do not prepare 1..512 query variants. Object insertion uses at
most 128 two-column rows (256 binds), further constrained by bytes; only the
bounded shapes actually needed are generated/cached.

Each object batch uses multi-row
`INSERT ... ON CONFLICT(object_id) DO NOTHING RETURNING object_id, length(bytes)`.
The returned set is exactly newly admitted rows; `sent_missing - returned` is
the set/byte count that lost a negotiation race to an existing identical ID.
This needs no metric table and no second per-object query.

Cross-store receiver cost:

```text
ObjectId membership SELECTs   = P_o
typed ancestry/membership     = H
object INSERT statements      = J
immutable-fact INSERTs        = F
metadata/CAS statements       = D
total SQL statements          = P_o + H + J + F + D, with D <= 8
durable write transactions    = max(1, J + F) for a ref/head transfer
```

The ObjectId SELECT count in the block above is `P_o`, not `P`; `P` never
counts SQL. The sender/receiver pair counts typed pages `H` and Object pages
`P_o` separately. A conservative complete SQL bound is
`2H + 2P_o + 2J + 2F + D + operation_preflight`, plus layered filesystem
preflight `L`. All object reads are primary-key/bounded `IN` queries; typed
ancestry uses the appropriate typed tables and indexes.

Each Object page uses one `objects` primary-key query and its own position
bitmap. Each typed page queries only its exact relevant typed table and
returns a separate position-preserving typed bitmap. Typed IDs are never bound
to the `objects` statement, and ObjectIds are never bound to a typed-table
statement. Store orchestration coalesces typed and Object announcements into
`P <= P_o + H` wire turns.
Each object, immutable-fact, or frozen-provenance Branch batch uses one prepared
multi-row idempotent insert. Transferred Commit/Stack/Layer/AddResult facts and
signed frozen Branch provenance may be admitted in these bounded batches before
copied-head visibility because each row's own dependency closure is complete.
The last bounded admission transaction also exposes only the applicable Branch
ref, history head, or copied head (plus a new history row when Pull creates its
local read-only history view); if
there is no admission batch, one small visibility transaction is used. SQL row
presence alone is not visibility; every public listing/read starts from an
exposed ref/head and follows reachable facts.

`pull_commit_history` creates no local mutable ref/head. Its pinned terminal
Commit is admitted in the last fact batch, so it uses `J + F` write
transactions—or zero when every required object and Commit is already known.

A CAS loss can therefore leave valid unreachable immutable facts. Retry reuses
them by primary key; no staging table or cleanup path is required. Locally
authored `add_stack`/`add_layer` remains different: its single candidate row,
single AddResult, and head CAS are folded into the last object batch transaction
or, when `J = 0`, one metadata transaction.

No network read, CDC, hashing, canonical authentication, signature
verification, or Merkle traversal runs while a SQLite write transaction is
open.

Local Commit admission performs no `P_o` ObjectId existence queries:

```text
SQL statements     <= J + 2        # object batches + Commit insert + Branch CAS
write transactions  = max(J, 1)    # append metadata to final local batch when present
```

Add Stack/Add Layer with new merge objects uses `J + 3` write statements and
`max(J, 1)` write transactions; the authored candidate + AddResult + head share
the last object transaction after read preflight. An UpToDate result uses one
small AddResult transaction. Conflict performs zero writes. On injected or
illegal head-CAS loss,
the last batch and typed rows roll back; earlier closure-complete immutable
object batches may remain unreachable and reusable.

Every object write transaction is bounded to 128 objects or 4 MiB, except one
valid object up to `MAX_OBJECT_BYTES` may occupy a singleton transaction. Every
transferred immutable-fact transaction is bounded to 128 rows/64 KiB. The
final visibility portion is bounded to 8 statements/64 KiB. All three
stores require WAL with `synchronous=FULL`; benchmarks must not weaken either
setting. A ref/head transfer performs `max(1, J + F)` durable commits because
visibility folds into the last bounded admission transaction. A pinned
Commit-history pull performs exactly `J + F`. Lock-duration gates measure these
fixed units rather than
allowing a transaction to grow with store/history size.

P1 measures checkpoint cost under the fixed batch profile, selects one
`WAL_AUTOCHECKPOINT_PAGES` constant, and freezes it for all stores. Warm-WAL p95
includes automatic-checkpoint spikes. If an explicit checkpoint is later
needed, only `PASSIVE` between operations is permitted; it never runs while a
final ref/head CAS transaction is open. No checkpoint worker, table, or
configuration surface exists in v1.

Reference warm-WAL budgets are target object/fact-batch p95 at or below 25 ms
and standalone visibility-only p95 at or below 10 ms, including automatic
checkpoint spikes. When visibility folds into the last object/fact batch, the
whole transaction uses that batch's p95 class; it does not also owe an
impossible second 10 ms wall bound. The incremental visibility portion remains
bounded by `D <= 8`, 64 KiB, and at most `1.25x` isolated CPU time for those
prepared statements. An oversize singleton is judged against an isolated
FULL+WAL transaction with the same byte count rather than the 4 MiB absolute
target. On slower runners, no class may exceed `1.25x` its matching isolated
transaction after applying the benchmark-noise rule.

### Long-lived connections without semantic sessions

Each Store process owns one local lock-safe SQLite file. Its handles reuse an
admitted connection and serialize writes through one gate. Another machine
uses the Store endpoint; multiple machines must never open the raw SQLite file
over NFS or another shared filesystem. A second owning process/handle for the
same file fails promptly with `StoreBusy`, without an owner/lease table. Each
remote endpoint reuses one TCP
stream for an operation and, where
the caller immediately performs Push then Add, for that two-operation sequence.
There is no connection pool in v1.

One Store handle admits at most one active transfer/mutation working set;
additional callers enter one fair queue before allocating batch buffers. Thus the current
per-handle working-memory bound is below 42 MiB plus the benchmark-frozen
SQLite page-cache bound and fixed connection overhead, not that amount
multiplied by connected clients.

The writer gate is acquired only for a bounded object/fact transaction or the
final/folded CAS transaction. No writer gate or SQLite write transaction spans
network, CDC, canonical encoding, hashing, signature verification, or three-way
traversal. One read-only recursive CTE cursor may retain its SQLite read
snapshot while <=512-row ancestry pages cross the endpoint; it holds no writer
gate or write transaction. The source CTE is one statement, `H` counts emitted
pages, and the `2H` SQL term remains deliberately conservative.
RTT and p95 service-time formulas exclude queue wait and report it separately.

One owning writer and one active working set should make negotiated
raced-existing rows nearly zero; `INSERT ... RETURNING` set subtraction is
defensive PK-idempotence for test-injected insert interleaving, not a reason to
add writers or a pool.
A ten-caller serialized-load gate reports queue wait, throughput, peak memory,
fairness/starvation, insert/visibility order, correct final heads, busy behavior,
and maximum per-stage writer-lock duration. The queue is per Store database;
independent Store databases operate independently.

Transport connection state contains only frame/parser buffers. Every operation
envelope is self-contained and there is no session ID, session table, or
connection-bound authority. An incomplete frame is never admitted and returns
an error. Automatic reconnect, resume, and server/network-failure recovery are
deferred for the experiment environment.

### Direct and stacked round trips

Pipelining is normative. For coalesced wire turn `i`, a sender frame carries
missing canonical payload for turn `i` plus the typed and/or Object announcement
for turn `i+1`; the receiver reply carries admission acknowledgement `i` plus
separate position-preserving typed and Object missing bitmaps for turn `i+1`.
The final reply closes closure verification and visibility.

On an already connected stream, a transfer with `P` coalesced wire turns costs
at most `P + 1` RTT, where `P <= P_o + H`. TCP connection establishment is a
separate transport cost, not hidden in this operation formula. A following Add
is a distinct semantic command and adds one RTT on the same stream:

```text
Direct remote Push Branch + Add Layer:
    P_branch_layer + 2 RTT

Stacked remote sequence:
    BranchStore -> StackStore Push Branch + Add Stack
        P_branch_stack + 2 RTT

    StackStore -> LayerStore Push Stack + Add Layer
        P_stack_layer + 2 RTT

    total stacked publication
        P_branch_stack + P_stack_layer + 4 RTT
```

Embedded hops have zero network RTT. The two stacked physical receiver hops and
the Push-before-Add correctness barriers are irreducible. Removable overhead is
forbidden in a healthy operation: no new connection per phase/page, per-object query/ack, full-closure
announcement, duplicate head read, or sender-delete handshake.

### Byte formula

For ObjectId width `I_o`, typed-ID width `I_t`, announced ObjectId/typed counts
`N_o`/`N_t`, coalesced-turn header bytes `h_p`, and metadata `M`:

```text
wire bytes <= missing canonical payload bytes
            + I_o * N_o
            + I_t * N_t
            + 64 * (P_o + H)                # separate fixed position bitmaps
            + h_p * P
            + M
```

Known-subtree pruning reduces announced IDs before this formula. Compression,
Bloom filters, and global object indexes are absent until byte evidence proves
the exact missing-set protocol insufficient.

Honest repeated-install bound for identical installation byte streams and edit
paths:

```text
10 identically produced installs sharing one physical CAS-bearing store
    ~= one package chunk payload set
     + per-install Commit/Stack/Layer refs
     + per-install changed structural tree nodes
```

Independent offline BranchStore files may temporarily retain one private
placement copy each. Cross-file single-copy claims require a shared physical
CAS and are not made here. `objects(object_id PK)` deduplicates within each
physical store, and missing-only transfer deduplicates all senders at a common
receiver. For one-machine maximal dedup, use one BranchStore containing many
Branches; ten identically produced Branches there approach one payload set plus
`O(10)` small Commit/tree/ref metadata. Different edit histories may retain
different boundary payload objects even when their final logical bytes match.
Ten separate BranchStore databases may each
hold one private pre-push payload copy; v1 adds no shared-CAS subsystem.

Dedup matrix. “Cross-store flow” means: announce table-specific typed/Object
pages -> return separate missing bitmaps -> send missing canonical bytes/facts
only -> idempotently admit -> verify closure -> expose ref/head last.

| Case | Required flow and scope |
|---|---|
| Repeated writes/Commits in one BranchStore | Local CDC/canonical hash -> idempotent admit -> verify -> expose; no per-chunk pre-query; identical new streams reuse ObjectIds. |
| Multiple Branches in one BranchStore | Same local flow; shared canonical ObjectIds have one row in that DB, while history-dependent boundaries are not claimed equal. |
| Pulled/pushed data in one StackStore | Cross-store flow; existing Branch/Commit/Stack/Layer facts and objects are checked against their own tables and skipped. |
| All received histories in LayerStore | Every sender uses cross-store flow; the common receiver deduplicates. |
| BranchStore -> StackStore Push Branch | Cross-store flow; Branch ref last. |
| BranchStore -> LayerStore Push Branch | Same flow; no Stack/Layer creation or authoritative head move. |
| StackStore -> LayerStore Push Stack | Same flow for Stack suffix plus every mapped frozen Branch/Commit/root provenance closure; then writer verification and read-only copied-head CAS. |
| Reverse Pulls | Requester is receiver for cross-store flow; history/ref visible last. |
| Add Stack/Add Layer | New merge objects use local flow and existing/equal roots are reused. Every newly accepted Branch creates one Stack metadata node even when the root is unchanged; equal-root Add Layer may write only AddResult. Repeating an already mapped source writes nothing. |

Sender deletion is never part of Push. GC remains deferred.

### Discard, retention, and safety

| State | Exact action |
|---|---|
| Receiver already has announced ID | Clear its missing bit; sender never allocates/sends that canonical frame. |
| Negotiated-missing ID loses an insert race | `RETURNING` omits it; discard the received frame buffer immediately and count it as raced-existing. |
| Invalid ID/canonical bytes/dependency order | Reject `Integrity`; discard the unadmitted frame or DeferredObjectStore scratch; expose nothing. |
| Three-way Conflict | Return the first bounded Conflict; discard all deferred memory/scratch; write zero DB rows. |
| Injected/illegal CAS loss | Roll back the last candidate/ref/head transaction; expose no partial new closure. |
| Sender after Push | Keep every sender object/ref; Push never prunes or deletes. |

Unreachable admitted facts remain until future GC. V1 adds no GC, refcount,
staging, transfer, or scratch table merely to reclaim them.

| CPU/memory guard | Frozen bound or behavior |
|---|---|
| Canonical object | `MAX_OBJECT_BYTES = 16 MiB`; field at most 8 MiB; authenticate before admission. |
| Child fanout / decode | At most 100,000 child references; canonical decode nesting at most 8. |
| Rope | Extent-tree level at most 31; streaming reads, no full-file materialization. |
| Normal DB admission | 128 objects/4 MiB target; one valid oversize object is a <=16 MiB singleton. |
| Transfer | Backpressured one-page pipeline, one active working set, no complete closure list. |
| Conflict | First lexicographic conflict only; no accumulated conflict list. |
| Merge base | Recursive CTE dedup spills through `temp_store=FILE`; page cache uses the benchmark-frozen P1 bound. |
| COW edit | `O(x + t)` normal work; unchanged suffix is referenced, not read/copied. |
| Representation equality | Rare unavoidable `O(3*B_file)` streamed digest work; O(1) application memory. |
| Empty receiver | Unavoidable `O(closure objects + closure bytes)` authentication/transfer once; still bounded and streaming. |

Required proof gates:

1. Count distinct payload objects separately from structural metadata for ten
   identically produced installs.
2. COW locality fixture: insert bytes near the start of a large file and prove
   `cdc_bytes_scanned == replacement_bytes`, zero old-suffix payload reads, and
   unchanged old extent `ObjectId`s retained. This is not the CDC-quality gate.
3. Standalone CDC fixture: chunk original and prefix-shifted deterministic byte
   streams from scratch, derive canonical payload ObjectIds, freeze the exact
   reused suffix ID/byte counts, and prove a fixed-block oracle fails. Make no
   universal reuse-percentage promise; adversarial input may churn the full
   file.
4. Produce the same final logical bytes by full write, one edit, and multiple
   edits. Assert reads are equal and every object authenticates; permit roots
   to differ. Merge different-root/equal-byte variants and prove the three
   cached-digest rules return clean without materializing or persisting data;
   report the representation-byte difference honestly.
5. Assert transferred ObjectIds equal the receiver's missing set per batch,
   transferred bytes exactly equal stored canonical rows, and the transfer CDC
   invocation counter remains zero.
6. Count one local canonical encode/hash per new object, at most one sender
   stored-row authentication, one receiver frame authentication, and separately
   report any scratch-spill re-authentication.
7. Assert one prepared 512-ID existence query, fixed bitmap/remap, no per-ID DB
   default, separate indexed typed-table pages/bitmaps, no cross-table ID
   queries, `P <= P_o + H` coalescing, deterministic greedy batch counts, exact
   RETURNING/race set arithmetic, and valid 16 MiB singleton admission.
8. Prove fair ten-caller serialization: one active working set, no starvation,
   child objects before parents, immutable facts before AddResult/ref/head,
   exact CAS last, PK dedup, correct final heads, and zero partial-visible
   closure. Independent Store databases may run independently.
9. Prove repeated/no-op Add reads zero descendants, divergent Add reads only
   unequal frontier nodes, and Conflict after scratch spill changes zero DB
   rows.
10. Under WAL + FULL and the frozen checkpoint threshold, include checkpoint
    spikes and a ten-caller serialized-load run reporting queue wait,
    throughput, memory, fairness, busy behavior, and per-stage lock time.

## 12. Minimal SQLite schemas

Typed Commit/Stack/Layer rows are canonical indexed manifests, not duplicate
CAS manifest objects. They preserve DB-native history/member queries without
mixed-blob scans or a nullable god table. `objects` stores only filesystem
chunks, trees, and roots.

SQLite `PRAGMA user_version` carries schema version. The ordinary-rowid
`objects` layout is frozen by the 100,000-row FULL/WAL comparison recorded in
`implementation-plan.md`; it was both smaller and faster than `WITHOUT ROWID`
for the binding object shape, with indexed exact-ID lookup in both variants.

### BranchStore: 3 tables, 9 columns

| Table | Exact columns | Count |
|---|---|---:|
| `objects` | `object_id PK`, `bytes` | 2 |
| `commits` | `commit_id PK`, `root_id`, `parent_id NULL`, `merge_parent_id NULL` | 4 |
| `branches` | `branch_id PK`, `head_commit_id`, `base_id` | 3 |
| **Total** | | **9** |

### StackStore: 8 tables, 24 columns

| Table | Exact columns | Count |
|---|---|---:|
| `objects` | `object_id PK`, `bytes` | 2 |
| `commits` | `commit_id PK`, `root_id`, `parent_id NULL`, `merge_parent_id NULL` | 4 |
| `branches` | `branch_id PK`, `head_commit_id`, `base_id` | 3 |
| `layer_histories` | `history_id PK`, `head_layer_id` | 2 |
| `layers` | `layer_id PK`, `history_id`, `parent_id NULL`, `root_id` | 4 |
| `stack_histories` | `history_id PK`, `base_layer_id`, `head_stack_id` | 3 |
| `stacks` | `stack_id PK`, `history_id`, `parent_id NULL`, `root_id` | 4 |
| `add_results` | `source_id PK`, `result_id` | 2 |
| **Total** | | **24** |

### LayerStore: 8 tables, 24 columns

LayerStore uses the same eight table shapes, holds complete central rows, and
is the sole authoritative writer of `layer_histories.head_layer_id`. For a
transferred StackHistory it stores `head_stack_id` as a read-only observed copy;
only the creator StackStore may author its changes.

Allowed AddResults:

```text
BranchId -> StackId
BranchId -> LayerId
StackId  -> LayerId
```

Required structural indexes:

```text
UNIQUE layers(history_id) WHERE parent_id IS NULL
UNIQUE layers(history_id, parent_id) WHERE parent_id IS NOT NULL
UNIQUE stacks(history_id) WHERE parent_id IS NULL
UNIQUE stacks(history_id, parent_id) WHERE parent_id IS NOT NULL
```

The Layer and Stack constraints enforce one seed/genesis and at most one child
per parent, matching their strict lists.

Required reverse/history/transfer indexes:

```text
commits(parent_id)
commits(merge_parent_id)
layers(history_id, parent_id)
stacks(history_id, parent_id)
add_results(result_id)
```

The ordinary Layer/Stack indexes may be satisfied by the unique indexes when
SQLite's query plan proves equivalent; do not create redundant B-trees.

### Derived and absent

| Not stored | Why |
|---|---|
| `base_kind` | Externally tagged `base_id` routes safely. |
| CommitHistory/BranchHistory row | Commit parents plus Branch refs derive them. |
| Branch parent/fork/root/history | Commit ancestry and tagged base derive them. |
| Stack version/fork/owner/lease | StackHistory is one linear list; writer capability is store configuration. |
| Layer/Stack sequence | Follow parents from the history head. |
| Workspace/draft/tool-operation tables | COW before Commit is transient. |
| Object kind/closure | Canonical bytes encode kind; an admitted root certifies closure. |
| Duplicate derived/read-model tables | Query canonical typed rows. |
| Request/session/outbox/GC/rollback tables | AddResult enforces single-publication/idempotent source mapping; recovery systems are deferred. |

## 13. Integrity and idempotence

All stores reuse the shared rejection vocabulary: `HeadMoved<I>`,
`WrongHistory<H>`, `WrongSourceRoute`, `ReadOnlyHistory<H>`, `NoCommonBase`,
`AmbiguousMergeBase`, `MissingBaseData`, `Conflict`, `Integrity`, and
`StoreBusy`. There are no per-store copies of these error/result types.

| Record | Required rule |
|---|---|
| LayerHistory | Head belongs to history; genesis exists. |
| Layer | Parent belongs to same history; at most one child per parent. |
| StackHistory | Base Layer exists; head belongs to history; only creator capability writes head. |
| Stack | Parent belongs to same history; at most one child per parent. |
| Branch | Head Commit exists; tagged base resolves to exactly one Layer or Stack. |
| Commit | Parents exist when present; merge parent differs from first parent. |
| AddResult | Source has at most one successful result; result type matches legal route. |
| Root | Complete canonical tree closure exists through layered resolution. |

`add_stack` and `add_layer` check AddResult before work. Successful new-node and
UpToDate paths insert AddResult atomically with the returned result. Conflicts
and injected/illegal CAS-lost attempts insert none.

Historical error payloads are not persisted. Automatic reconnect/resume,
server/network-failure recovery, crash matrices, scratch recovery, GC, and
rollback are deferred for the experiment environment.

## 14. Final model

```text
LayerStack = whole architecture

CommitHistory = local DAG
StackHistory  = linear intermediate list + exact head CAS
LayerHistory  = linear accepted list + exact head CAS

BranchStore.merge  --+
StackStore.add_stack--+--> one layerfs-storage-core three_way algorithm
LayerStore.add_layer--+

BranchStore.merge base = closest common Commit -> Stack -> Layer
BranchStore.merge result keeps the target Branch base and add route

push      = transfer only
add_stack = clean three-way -> one Stack + StackHistory head CAS
add_layer = clean three-way -> one Layer + LayerHistory head CAS
```
