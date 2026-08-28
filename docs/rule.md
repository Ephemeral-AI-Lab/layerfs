# LayerStack operation rules

This is the binding public-operation contract. `MUST`, `MUST NOT`, and `ONLY`
are invariants.

[db-transaction-transfer-model.md](db-transaction-transfer-model.md) is the
binding low-level contract for SQLite transactions, indexed membership/search,
batching, durability, and transfer mechanics. This file remains authoritative
for public operation semantics and invariants; the low-level contract must
implement them without adding or changing public behavior.

`LayerStack` names the whole architecture. It is not a row, ID, history,
store, or operation target.

## 1. Records and histories

| Name | Rule |
|---|---|
| `Branch` | Mutable ref with one Commit head and exactly one Layer-or-Stack base. |
| `Commit` | Immutable filesystem snapshot in CommitHistory. |
| `Stack` | Immutable intermediate filesystem snapshot in one StackHistory. |
| `Layer` | Immutable accepted filesystem snapshot in one LayerHistory. |
| `CommitHistory` | Commit parent DAG. It has no ID or table. |
| `StackHistory` | Strict parent-linked Stack list based on one Layer, with exactly one CAS head. |
| `LayerHistory` | Strict parent-linked Layer list with exactly one CAS head. |

```text
LayerHistory H1                   StackHistory SH1

L1 -> L2 -> L3                   S1 -> S2 -> S3
           ^                                ^
           one CAS head                     one CAS head

CommitHistory

C1 -> C2 -> C3
       \      \
        C4 -> C5
```

Only three refs are mutable:

```text
Branch.head_commit_id
StackHistory.head_stack_id
LayerHistory.head_layer_id
```

There is no Stack version, freeze, lease, or mutable Stack record. Each Stack
is immutable; only `StackHistory.head_stack_id` changes by exact CAS.

### Global identity

| ID | Generation rule |
|---|---|
| `ObjectId` | Untagged 32-byte `OBJECT_DOMAIN` hash of canonical object bytes; object kind is encoded in those bytes, not in the ID |
| `CommitId` | Domain-separated hash of canonical parents and `root_id` |
| `StackId` | Domain-separated hash of history, parent, and `root_id` |
| `LayerId` | Domain-separated hash of history, parent, and `root_id` |
| `BranchId` | Typed, domain-separated UUIDv7 |
| `StackHistoryId` | Typed UUIDv7 plus digest of the creator's verification public key |
| `LayerHistoryId` | Typed, domain-separated UUIDv7 |

Every insert rejects an existing ID whose canonical bytes/record differ.
`ObjectId` itself carries no runtime kind tag; its canonical bytes carry kind
and framing. The other IDs remain distinct SDK/Rust types.
Random-ID collision is an error/retry, never an overwrite. IDs are globally
usable without a `StoreId`; no Branch, Stack, Layer, or history stores one.
For StackHistory, matching an ID prefix/namespace proves nothing: write
authority requires a valid signature under the verification key committed by
the ID.

### LayerHistory bootstrap

LayerHistory provisioning is SDK setup, not another recurring storage
verb. Setup atomically:

```text
generate LayerHistoryId
    -> resolve/create canonical empty filesystem root
    -> create genesis Layer(history, parent=NULL, empty root)
    -> create LayerHistory(head=genesis Layer)
```

Either the history, genesis Layer, and complete empty-root closure all become
visible, or none do. Later changes use only the recurring operations in
section 4.

## 2. Store topologies

### Direct

```text
BranchStore --------------------------------------> LayerStore

Layer L10 -> Branch B1 -> Commit C1 -> Layer L11
```

There is no StackHistory or hidden Stack. The Branch base is an exact Layer.

```text
push_branch(B1)                    transfer only
add_layer(H1, Branch(B1, C1))      create one Layer or conflict
```

### Stacked

```text
BranchStore ------------> StackStore ------------> LayerStore

Layer L10 -> Stack S1 -> Branch B1 -> Stack S2 -> Layer L11
```

The Branch base is an exact Stack.

```text
push_branch(B1)                         transfer only
add_stack(SH1, B1, C1)                  create immutable Stack S2

push_stack(S2)                    transfer only
add_layer(H1, Stack(S2))          create one Layer or conflict
```

| Store | Owns |
|---|---|
| `BranchStore` | Private Branch refs, CommitHistory, and changed objects only |
| `StackStore` | Selected Layer/Commit dependencies plus local/pulled linear StackHistories |
| `LayerStore` | Complete central Branch, Commit, StackHistory, Stack, LayerHistory, Layer, and object records |

Each BranchStore has exactly one configured parent: StackStore or LayerStore.
Its `pull_branch` and `push_branch` verbs stay the same; the Branch base makes
the route legal or illegal.

A BranchStore may serve one or many callers and store one or many Branches.
Using one BranchStore per caller is also valid. The SDK assigns application
workloads to stores and Branch IDs; the storage schema does not persist caller
topology.

## 3. Branch bases

```text
Layer-bound Branch                    Stack-bound Branch

Branch B1                             Branch B2
├── head = Commit C3                  ├── head = Commit C8
└── base = Layer L10                  └── base = Stack S4
```

Rules:

- A Branch stores only the exact tagged `base_id`; its tag is LayerId or StackId.
- Layer/Stack history, root, and ancestry are resolved from that ID.
- A Branch is never bound to both.
- A Stack-bound Branch MUST NOT call `add_layer` directly.
- A Branch base never moves after creation.
- Branch head always means Commit ID.
- One successful `add_stack` or direct `add_layer` maps that Branch ID to one
  immutable result. The same Branch ID cannot publish again.

`create_branch_from_commit` creates a subbranch by pointing a new Branch ID at
the same Commit and inheriting the same base:

```text
Branch B1 -> C3
              \
               +-- create_branch_from_commit(B1, C3)
                         |
                         +--> Branch B2 -> C3
```

No Commit, filesystem payload, or explicit parent-Branch metadata is copied.
Shared Commit ancestry is the subbranch relationship.

### Publication continuation

An addition never rebases or deletes its input Branch:

```text
Direct:  Branch B1 --add_layer--> Layer L11
Stacked: Branch B2 --add_stack--> Stack S5
```

On success, the executor atomically inserts an `add_results` mapping from the
source Branch ID to `L11` or `S5`. Repeated calls return that result. The old Branch
remains readable and mergeable, but cannot produce another addition. After
success BranchStore treats it as read/merge-only; the parent mapping is the
definitive enforcement against stale local activity.

Further publishable work MUST create a new Branch from `L11` or `S5`. An old
Commit remains a valid historical merge source, but `create_branch_from_commit`
inherits the old base and does not mean “continue from the published result.”
This avoids comparing already-accepted edits against the Branch's unchanged
old base without mutating it or adding lifecycle columns.

## 4. Smallest public operation set

| Operation | Executor | Sole semantic effect |
|---|---|---|
| `create_branch_from_layer(layer_history_id, layer_id)` | BranchStore | Create one Layer-bound Branch after membership verification |
| `create_branch_from_stack(stack_history_id, stack_id)` | BranchStore | Create one Stack-bound Branch after membership verification |
| `create_branch_from_commit(source_branch_id, source_commit_id)` | BranchStore | Create one local subbranch; inherit base and reuse Commit |
| `commit(branch_id, expected_head, changes)` | BranchStore | Create one Commit and CAS one Branch head |
| `merge(source_branch_id, target_branch_id, expected_target_head)` | BranchStore | Return UpToDate, fast-forward by CAS, or create one divergent merge Commit on the target |
| `pull_branch(source_branch_id, local_branch_id)` | Configured parent serves | Prepare the pinned source base/Commit dependencies, then safely expose or advance the named local Branch |
| `push_branch(branch_id)` | Configured parent | Transfer Branch/Commit/object rows; same-Branch-ID CAS only |
| `pull_commit_history(branch_id)` | StackStore; LayerStore serves | Pin the central Branch head and pull its missing reachable Commit DAG/objects without creating or moving a Branch ref |
| `create_stack_history_from_layer(layer_history_id, layer_id)` | StackStore | Create one StackHistory and immutable seed Stack sharing the Layer root |
| `pull_layer_history(layer_history_id, through_layer_id)` | LayerStore serves | Pull exact Layer ancestry through the named Layer |
| `pull_stack_history(stack_history_id, through_stack_id)` | LayerStore serves | Pull exact Stack ancestry through the named Stack |
| `add_stack(stack_history_id, branch_id, commit_id)` | Creator StackStore | Three-way integrate one Branch Commit, create at most one Stack, and CAS the StackHistory head |
| `push_stack(stack_id)` | LayerStore | Transfer the missing Stack suffix plus each accepted Branch/AddResult/exact Commit/root provenance closure, then exact-CAS the verified copied Stack head |
| `add_layer(layer_history_id, source)` | LayerStore | Create one Layer and CAS the LayerHistory head, or return conflict |

These fourteen are irreducible: no pair has the same caller, target ref,
visibility effect, and conflict/CAS rule. Replacing explicit create/pull/push
verbs with one enum-dispatch operation would keep every algorithm while adding
invalid variants and match scaffolding, so it is not a reduction.

`source` is one exact typed value:

```text
BranchSource(branch_id, commit_id)
StackSource(stack_id)
```

There are no convenience aliases for latest-history pull, object transfer,
promote, or conflict detection. Semantic correction uses the same explicit
operation again; transport resume/recovery is deferred by section 16.

The paired `history_id + node_id` arguments in create/pull operations are
deliberate scope guards, not redundant lookup data. The executor MUST verify
that the named Layer or Stack belongs to the named history and reject a
wrong-history or wrong-tenant pair before reading payload. The stored Branch
still keeps only its typed base ID.

### One shared three-way implementation

`layerfs-storage-core` owns the only Branch `merge_base` resolver and the only
`three_way(base, current, candidate)` implementation. The resolver owns
LayerHistory isolation and closest common Commit -> Stack -> Layer selection.
Stores supply verified ancestry/roots and persist the outcome; they MUST NOT
copy or specialize either algorithm.

| Caller | `base` | `current` | `candidate` |
|---|---|---|---|
| BranchStore `merge` | selected common ancestor | target Branch head Commit | source Branch head Commit |
| StackStore `add_stack` | source Branch base Stack | StackHistory head Stack | exact source Commit |
| LayerStore `add_layer` | source base Layer | LayerHistory head Layer | exact Branch Commit or Stack |

The shared result is exactly:

```text
Clean(root_id)
Conflict {
    path,
    base: Absent | ObjectId,
    current: Absent | ObjectId,
    candidate: Absent | ObjectId,
}
```

It preserves non-overlapping/identical changes and never chooses a winner,
writes conflict markers, mutates snapshots, or advances a head. Paths are
traversed in canonical bytewise lexicographic order. At the first real
conflict, traversal returns that one record immediately. V1 has no conflict
`Vec`, all-conflicts scan, truncation flag, count, or continuation token.

`three_way` writes candidate canonical objects only into a bounded transient
`DeferredObjectStore` backed by memory with disposable scratch-file spill. It
MUST NOT write the live SQLite `objects` table while conflict discovery is in
progress. On `Conflict`, discard the deferred store and write zero object,
typed, AddResult, or head rows. On `Clean`, admit the deferred canonical
objects with the standard greedy object batches, then place the candidate
Commit/Stack/Layer, AddResult when applicable, and exact head CAS together in
the last transaction. No staging table or persisted deferred-session state is
allowed.

### Minimal shared outcomes

Use generic typed structs instead of per-store error enums:

| Outcome | Caller action |
|---|---|
| `HeadMoved<I> { expected, actual }` | Refresh/re-evaluate the affected Branch, StackHistory, or LayerHistory head |
| `WrongHistory<H> { expected, actual }` | Correct the explicitly scoped history ID |
| `WrongSourceRoute` | Use the Layer-bound or Stack-bound publication route required by the tagged source |
| `ReadOnlyHistory<H> { history_id }` | Send mutation to the creator writer or create another history |
| `NoCommonBase` | Do not merge unrelated LayerHistories |
| `AmbiguousMergeBase` | Pull/resolve ancestry explicitly; do not guess |
| `MissingBaseData` | Pull the named immutable dependencies, then retry |
| `Conflict { path, base, current, candidate }` | Resolve the deterministic first conflicting path in BranchStore, then retry |
| `Integrity { reason }` | Stop; do not retry as a semantic conflict |
| `StoreBusy` | Use the existing owning Store endpoint; do not open a second owner for the SQLite file |

Normal repeats return `UpToDate` or the existing typed `AddResult<T>`. Do not
create role-specific variants for the generic outcomes above.

## 5. Branch operations

### `create_branch_from_layer`

```text
input:  LayerHistory H1, Layer L10
check:  L10 belongs to H1 and its root closure verifies
anchor: reuse/create Commit(root=L10.root, parent=NULL, merge_parent=NULL)
write:  Branch(head=anchor Commit, base_id=LayerId L10)
```

This operation is legal only with a direct LayerStore parent. Anchor Commit
and Branch are inserted atomically; the Commit references the existing root
and copies no payload.

### `create_branch_from_stack`

```text
input:  StackHistory SH1, Stack S4
check:  S4 belongs to SH1; SH1 base Layer and S4 root verify
anchor: reuse/create Commit(root=S4.root, parent=NULL, merge_parent=NULL)
write:  Branch(head=anchor Commit, base_id=StackId S4)
```

This operation is legal only with a StackStore parent. Anchor Commit and
Branch are inserted atomically; canonical Commit identity deduplicates equal
anchors without copying payload.

### `create_branch_from_commit`

The source Commit MUST already exist and be reachable from the source Branch
inside the same BranchStore. It performs no parent-store I/O.

### `commit`

```text
expected_head == Branch.head
    -> apply changes to the layered filesystem view
    -> write only new content-addressed objects
    -> create Commit(parent=expected_head, root=new_root)
    -> CAS Branch.head to Commit

expected_head != Branch.head
    -> HeadMoved<CommitId>{expected, actual}; preserve changes
```

The Commit ID is derived from canonical immutable content. A replay that
already produced the same Commit returns `UpToDate` rather than adding another
Commit.

### `merge`

```text
source head --------------------+
                                +--> merge Commit on target
target head = expected_target --+
```

Requirements:

1. both Branches and their pinned heads MUST be present in the target
   BranchStore. Immutable merge-base/file data may resolve through the
   configured parent during read-only preflight, but a mutating Pull MUST be a
   separate operation and no network read may occur while the target write
   transaction is open;
2. pin the source head for the entire attempt and require the target head to
   equal `expected_target_head` before reading merge inputs;
3. resolve both Branch bases to their LayerHistory and reject unequal
   `LayerHistoryId` values as `NoCommonBase`, even if canonical anchors happen
   to have identical roots;
4. select the closest provable common immutable ancestor in this exact order
   within that LayerHistory:
   - the single maximal common Commit ancestor, when one exists;
   - otherwise the unique nearest common Stack reachable through Stack parent
     chains and any verified `StackId -> LayerId` AddResult provenance edge;
   - otherwise the unique nearest common Layer after resolving Stack seeds and
     Layer parent chains;
5. reject multiple incomparable maximal Commit ancestors as
   `AmbiguousMergeBase` instead of guessing, ordering IDs, or persisting a
   virtual merge object;
   merge-base discovery MUST use one or a fixed few indexed SQLite recursive
   CTE statements, not Rust `HashSet`s or a materialized ancestry vector. The
   CTE walks both `parent_id` and `merge_parent_id`, uses `UNION` deduplication,
   intersects source/target ancestors inside SQLite, removes non-maximal common
   candidates there, and returns only the final candidate page. Read at most
   two maximal Commit candidate IDs: zero continues to the closest common
   Stack and then Layer fallback, one is the Commit base, and two proves
   `AmbiguousMergeBase` without loading the remaining ancestry into application
   memory. `NoCommonBase` is returned only after no valid common Commit, Stack,
   or Layer exists within the required LayerHistory;
6. if both heads are equal, or the source head is already an ancestor of the
   target head, return `UpToDate` without writing;
7. if the target head is an ancestor of the source head and both Branches have
   the same tagged base, fast-forward the target head to the pinned source head
   by exact CAS without creating a merge Commit;
8. otherwise call shared `layerfs-storage-core`
   `three_way(base, target, source)`;
9. a clean divergent result creates
   `Commit(parent=target, merge_parent=source)` and advances only the target by
   exact CAS;
10. target CAS loss rolls back the candidate merge Commit and returns
    `HeadMoved<CommitId> { expected, actual }`; merge does not silently retarget the
    user's operation;
11. a path conflict creates no Commit and is resolved only in BranchStore;
12. the target keeps its exact tagged `base_id`, so the target determines the
    later add route regardless of the source base:
    - Layer-based target -> direct `push_branch` then `add_layer(BranchSource)`;
    - Stack-based target -> `push_branch`, `add_stack`, `push_stack`, then
      `add_layer(StackSource)`.

```text
same/already-contained head -> UpToDate
same base + target ancestor -> fast-forward target by CAS
same Commit DAG             -> closest common Commit root
related Stack bases         -> closest common Stack root
related Layer bases         -> closest common Layer root
different LayerHistory      -> NoCommonBase before Commit inspection
ambiguous Commit DAG        -> AmbiguousMergeBase
missing/unavailable base data -> MissingBaseData; Pull first
```

LayerHistory and StackHistory are strict lists, so their fallback ancestor is
unique. A Branch from a Layer may merge with a Branch from a Stack when the
resolver proves their shared LayerHistory ancestry. This does not let the
source Branch bypass its route: only the target Branch moves, and the result
keeps the target base. No merge-base cache, Branch lineage column, or virtual
merge object is persisted.

The recursive CTE's dedup/work sets belong to SQLite transient storage. V1
uses a benchmark-frozen page-cache limit and `temp_store=FILE` so large DAGs
spill rather than creating an unbounded Rust heap set. This creates no product
or staging table. `EXPLAIN QUERY PLAN` MUST show indexed Commit PK/parent-edge
access and transient B-trees only for recursive `UNION`/final candidate work;
a full `commits` corpus scan is a gate failure.

Branches may originate in different BranchStores, StackStores, or machines,
but merge executes in the target BranchStore after the source Branch and
pinned Commit DAG are present. Its preflight resolver may read immutable
accepted objects through the configured parent without copying them into
BranchStore. That layered read-through is batched, read-only, and completed
before opening the target write transaction; it is not a hidden Pull. Missing
or unavailable data returns `MissingBaseData`. `merge` never changes the
source Branch.

## 6. Branch transfer

### `pull_branch(source_branch_id, local_branch_id)`

Pull is dependency preparation, never merge:

```text
Layer-bound Branch
    Layer ancestry -> missing Commit ancestors -> Branch ref

Stack-bound Branch
    StackHistory base Layer
        -> Stack ancestry through base Stack
        -> missing Commit ancestors
        -> Branch ref
```

Branch becomes visible only after every referenced root closes and verifies.
Authenticated, closure-complete immutable objects and typed rows may remain
after an operation error, but no Branch ref may expose the incomplete result.
Product visibility is reachability from an exposed Branch/history/LayerStore-copy head,
not mere presence of an internal immutable row.

The parent pins `source_branch_id` at one exact base and Commit head. Pull
admits its missing Commit metadata and verifies its accepted root through the
configured parent without copying that accepted payload into BranchStore.

If `source_branch_id != local_branch_id`, the local ID MUST be absent. Pull
inserts that fresh local Branch at the pinned source base/head; an existing
local ID returns `HeadMoved<CommitId> { expected: local, actual: source }` and
no ref changes.

If both IDs are equal:

| Local state relative to pinned source head | Result |
|---|---|
| Local Branch absent | Create it at the exact source base/head |
| Heads equal | `UpToDate`; zero ref writes |
| Local head is an ancestor of source | Exact-CAS local head to source |
| Source head is an ancestor of local | `UpToDate`; preserve the local-ahead head |
| Heads diverge | `HeadMoved<CommitId> { expected: local, actual: source }`; preserve local head |

Resolve divergence without hidden merge or another table by pulling the source
into a fresh local Branch ID, then explicitly merging that fresh Branch into
the existing local target and pushing the target. Pull never overwrites a
divergent local Branch or invokes Merge implicitly.

When a StackStore parent lacks the required Commit ancestry, it may perform
`pull_commit_history(branch_id)` before serving the outer `pull_branch`. The
outer operation still creates/exposes the Branch only in BranchStore; the
StackStore step only prepares shared CommitHistory data.

### `push_branch`

Push transfers the Branch result only. It MUST NOT create a Stack or Layer.

The internal request pins:

```text
BranchId
expected parent Commit head: Absent or CommitId
candidate Commit head
missing immutable ObjectIds
```

Before validating or changing the Branch head, the parent checks
`add_results(source_id=BranchId)`. An existing mapping
returns the existing mapped result without transfer and rejects further
accepted updates.

| Remote state for the same Branch ID | Result |
|---|---|
| Absent and expected Absent | Insert the same Branch ID and candidate head |
| Head equals expected | Exact-CAS head to candidate |
| Head equals candidate or descends from it | `UpToDate`; never rewind |
| Otherwise | `HeadMoved<CommitId> { expected, actual }`; transfer does not choose or merge |

Direct and stacked Push Branch use the same implementation and SQL shapes;
only the configured receiver differs:

```text
1. send BranchId + expected/candidate Commit IDs + tagged base ID
2. receiver performs one joined AddResult/current-head preflight
3. short-circuit existing result/equal-or-contained candidate
4. batch missing Commit ancestry and ObjectId frontier
5. authenticate/admit objects children-first, then closure-complete Commits in
   bounded immutable batches
6. one final transaction inserts/updates Branch by exact head CAS
```

It never calls Add, chooses a merge, or uses a direct-versus-stacked fallback
branch beyond selecting the configured endpoint.

Different Branch IDs never contend on a shared Branch head:

```text
Branch BA push ----> BA head CAS
Branch BB push ----> BB head CAS       both may succeed independently
```

The caller resolves `HeadMoved<CommitId>` by pulling the competing Branch state,
performing explicit Branch `merge`, committing, and pushing again.

### `pull_commit_history`

This operation exists only on StackStore and reads from LayerStore:

```text
pull_commit_history(B1)
    |
    +-- LayerStore reads B1 and pins head C8 for this response
    +-- verify/prepare B1's exact Layer-or-Stack base
    +-- walk C8 through both parent edges backward
    +-- stop each path at the first Commit known by StackStore
    +-- transfer missing Commit rows and root ObjectIds
    +-- authenticate roots before exposing Commits
    +-- return through_commit_id = C8
```

It does not create or update `branches` in StackStore, create a Branch in a
BranchStore, move any Branch head, merge, or report a path conflict. Its sole
purpose is to prefetch/synchronize the shared CommitHistory so StackStore can
serve later `pull_branch` calls efficiently. If LayerStore advances B1 during
the transfer, the pinned response through C8 remains valid and the next call
pulls the later suffix.

The operation has no `commit_id` argument because the LayerStore Branch head is
the synchronization point. The pinned `through_commit_id` is an operation
result, not another persisted cursor or column.

## 7. History pulls

### `pull_layer_history(history_id, through_layer_id)`

```text
requested L8
    |
    +-- verify L8 belongs to H1
    +-- walk L8.parent -> L7 -> L6 -> ... backward
    +-- stop at first locally known Layer
    +-- transfer missing Layer rows and ObjectIds in forward order
    +-- verify every root closure
    +-- expose exact ancestry through L8
```

It does not require pulling descendants beyond `L8`. A newer locally observed
LayerHistory head is never moved backward.

### `pull_stack_history(history_id, through_stack_id)`

```text
requested S6
    |
    +-- prepare StackHistory base Layer first
    +-- walk S6.parent -> ... -> seed backward
    +-- stop at first locally known Stack
    +-- transfer only that missing ancestry path and ObjectIds
    +-- verify parents and roots
    +-- expose S6
```

StackHistory is a strict list. This pull does not fetch descendants beyond the
named Stack, and it never moves a newer locally observed head backward. There
is no version number; exact Stack IDs scope the prefix. The returned history
is always read-only because pull never transfers the creator write capability.

### Pull invariants

- Pull never performs three-way merge and never reports path conflict.
- Pull walks immutable parents; it does not replay Commit operations or file
  payload history to reconstruct a snapshot.
- Exact IDs, not unrelated counters or timestamps, establish readiness.
- Data transfer is missing-only and bounded; no whole-database copy is legal.
- Each ancestry page uses one bounded source fetch and one destination
  membership query; no Pull performs one SQL query or RPC per parent/node.
- All pages reuse one live stream. Authenticated closure-complete immutable
  rows may commit in bounded batches; the tiny final
  history/LayerStore-copy ref update is folded into the final atomic
  admission/visibility transaction.

## 8. Stack construction and transfer

### `create_stack_history_from_layer`

The exact Layer must already be present in StackStore.

```text
Layer L10(root R10)
    |
    +-- StackHistory SH1(base L10, head S1)
            |
            +-- seed Stack S1(parent none, root R10)
```

The seed Stack shares `root_id`; it copies no filesystem bytes. The operation
returns both `StackHistoryId` and seed `StackId`, and atomically sets
`head_stack_id = seed`.

### StackHistory write authority

Exactly one SDK-configured creator StackStore may execute `add_stack` for a
StackHistory. `create_stack_history_from_layer` returns a writable SDK handle
and atomically establishes a signing keypair:

```text
StackHistoryId embeds H(verification_public_key)
SDK configuration privately persists signing_private_key outside core SQLite
writable handle binds history_id + signer
```

The private signer is not a store ID, database column, or user-managed
credential. It is managed transparently by SDK deployment configuration:

```text
creator StackStore      writable handle -> may CAS head_stack_id
pulled StackStore       read-only handle -> cannot add_stack
LayerStore copy         read-only -> cannot add_stack
```

`pull_stack_history` never transfers the signer and always returns a read-only
handle. A copied ID, matching writer namespace, public key, or database row is
insufficient to write. There is no authority-transfer verb or owner/lease
column. Embedded local use automatically creates/reloads the signer and needs
no user-managed credential. Cloning a writable StackStore database without
its SDK signer yields a read-only copy; copying the signer and running two
writers is unsupported. Deploy one writer or create separate StackHistories.

### `add_stack`

```text
add_stack(SH1, Branch B, Commit C)

base      = B.base Stack
current   = SH1.head_stack_id
candidate = C
```

The creator StackStore executes this algorithm:

One joined indexed preflight loads the source AddResult, Branch/Commit, base
Stack, StackHistory, and current Stack manifest. It MUST NOT query those
relations one at a time.

1. verify its writable SDK handle for `SH1`; pulled/read-only copies return
   `ReadOnlyHistory<StackHistoryId>`;
2. check `add_results` for Branch B; a mapped Stack in `SH1` returns the
   existing `AddResult<StackId>`, a Stack in another history is
   `WrongHistory<StackHistoryId>`, and a mapped Layer is `WrongSourceRoute`;
   an existing result additionally requires the mapped Stack's full root
   closure to verify;
3. require B's tagged base to be a Stack in `SH1`, and require C to equal the
   transferred Branch head;
4. authenticate the exact Branch/Commit/Stack manifests and root IDs; an
   already-admitted root is the closure certificate, so do not rewalk its
   equal descendants;
5. read `head_stack_id` as the exact CAS snapshot;
6. call shared `layerfs-storage-core` `three_way(base, current, candidate)`;
7. on the first lexicographic real path conflict, stop, create no
   Stack/mapping/head change, and return its typed `Conflict` to BranchStore;
8. if the merged root equals current root, conditionally insert
   `B -> current` in `add_results` while head still equals current, then return
   `AddResult<StackId> { result_id: current }`;
9. otherwise create one canonical Stack with `parent_id = current` and merged
   root, insert `B -> new Stack`, and CAS head from current to the new Stack in
   one transaction;
10. if the exact CAS unexpectedly loses, roll back Stack/mapping and return
    `HeadMoved<StackId> { expected, actual }`; never retry, force, or retarget.

The single Store operation queue linearizes callers without discarding
independent work:

```text
BA(base S1) changes /a       BB(base S1) changes /b
          \                   /
           A completes: S1 -> S2
           B enters next and evaluates against S2
           clean: S2 -> S3

same path changed incompatibly
           -> A completes first
           -> B enters next and returns path conflict
```

The Branch base never moves. Further work from the result creates a new Branch
from the returned Stack. StackStore never writes conflict markers, performs
last-write-wins, or creates a Stack outside the successful CAS transaction.

### `push_stack`

```text
StackStore                         LayerStore
    | push_stack(S4)                   |
    |--------------------------------->|
    |  missing SH metadata             |
    |  missing ancestor Stacks         |
    |  missing Branch/Commit/ObjectIds |
    |<---------------------------------|
    |        Transferred / UpToDate    |
```

Push is transfer only. It does not create/reparent a Stack, change the
creator's authoritative StackHistory head, create a Layer, or change any
LayerHistory head. The requested Stack MUST be the creator-attested current
StackHistory head. LayerStore may update only its non-authoritative copied
`head_stack_id` as transferred metadata:

The creator signs this canonical attestation for every push:

```text
history_id
expected_layerstore_head = Absent | StackId
incoming_head = requested StackId
suffix_digest = H(
    ordered predecessor/suffix Stack manifests
    + every accepted BranchId -> StackId AddResult
    + each frozen BranchId/base/head-Commit tuple
    + exact Commit DAG/root IDs
)
request_digest = H(
    canonical push request
    + complete table-specific typed/Object provenance frontier
)
```

The request includes the verification public key and signature, never the
private signer. Before accepting metadata or CASing its copied head, LayerStore
MUST:

1. verify `H(public_key)` equals the digest embedded in `StackHistoryId`;
2. verify the signature over the exact tuple above;
3. verify the suffix is a linear descendant chain in that history;
4. verify every AddResult maps its exact Branch to its exact suffix Stack, the
   frozen Branch base belongs to that StackHistory route, and its head is the
   exact accepted Commit;
5. verify the Commit DAGs, `suffix_digest`, `request_digest`, and every admitted
   Branch/Stack root closure without recomputing `three_way`;
6. when an advance is required, exact-CAS the copied head from
   `expected_layerstore_head` to `incoming_head`.

Invalid/missing attestation is `Integrity { reason: InvalidWriterAttestation }`, even when a writer
namespace or database contents appear to match. It exposes no Stack metadata
and never changes the LayerStore copy's head.

| LayerStore copied StackHistory state | `push_stack` result |
|---|---|
| History absent and attested expected is Absent | Transfer/verify full prefix through incoming, then atomically insert the read-only copied head |
| Copied head equals requested head | `UpToDate` |
| Copied head is a verified descendant of incoming head | `UpToDate`; a repeated older request performs no CAS and never rewinds |
| Copied head equals attested expected head and is a verified ancestor | Transfer missing linear suffix, then exact-CAS the copied head forward |
| Copied head is linearly related, but is neither incoming nor its descendant and does not equal attested expected | `HeadMoved<StackId> { expected, actual }`; creator prepares a new attestation |
| Copied head is divergent/reparented | `Integrity { reason: DivergentStackHistory }`; never merge or force |

This copied-head fast-forward does not create a Stack, run `three_way`, or transfer
creator authority. It makes a pulled copy current enough for reads while the
creator StackStore remains the sole writer.

`push_stack` uses the non-unique `add_results(result_id)` index for every Stack
node transferred, including missing ancestors. It transfers each matching
Branch-to-Stack provenance row and linked Branch/Commit records without
scanning all addition mappings. Stack-to-Layer mappings remain LayerStore-local
until selected pulls need them.

## 9. `add_layer`

### Legal sources

| Source | Legal when | Base Layer |
|---|---|---|
| `BranchSource(branch_id, commit_id)` | Branch is Layer-bound, was pushed to LayerStore, and Commit equals its head at execution | Branch base Layer |
| `StackSource(stack_id)` | Stack and its ancestry were pushed to LayerStore | StackHistory base Layer |

A Stack-bound Branch MUST first use `add_stack`, then `push_stack`, then
`add_layer(StackSource)`. Calling `add_layer(BranchSource)` with a Stack-bound
Branch returns `WrongSourceRoute` before transfer or merge.

### Authoritative algorithm

For one call:

```text
base      = exact source base Layer
candidate = exact Branch Commit or Stack root
current   = LayerHistory.head_layer_id
```

One joined indexed preflight loads the source AddResult, typed source,
source-base Layer, target LayerHistory, and current Layer manifest. It MUST NOT
query those relations one at a time.

1. check `add_results` by exact typed source ID; a mapped Layer in the target
   history returns the existing `AddResult<LayerId>`, a Layer in another
   history returns `WrongHistory<LayerHistoryId>`, and a mapped Stack returns
   `WrongSourceRoute`; an existing result additionally requires the mapped
   Layer's full root closure to verify;
2. validate the exact source route and snapshot: a BranchSource must be
   Layer-bound with the named Commit at its transferred head; a StackSource
   must name a transferred Stack; otherwise return `WrongSourceRoute`;
3. resolve the source base Layer and require its `history_id` to equal the
   target `layer_history_id`; otherwise return
   `WrongHistory<LayerHistoryId> { expected, actual }`;
4. authenticate the exact source/Layer/Commit-or-Stack manifests and root IDs;
   an already-admitted root is the closure certificate, so do not rewalk its
   equal descendants;
5. read and verify the authoritative current head as the exact CAS snapshot;
6. call shared `layerfs-storage-core` `three_way(base, current, candidate)`;
7. if it returns the first lexicographic path conflict, stop, create no
   Layer/mapping/head change, and return that typed `Conflict` to BranchStore;
8. if the merged root equals `current.root_id`, conditionally map the source
   to the current Layer while head still equals current, then return
   `AddResult<LayerId> { result_id: current }`;
9. create one canonical Layer with `parent_id = current` and the merged root;
10. exact-CAS LayerHistory head from `current` to the new Layer and insert the
    source-to-Layer `add_results` mapping in the same transaction;
11. if the exact CAS unexpectedly loses, roll back the Layer and mapping and
    return `HeadMoved<LayerId> { expected, actual }`; never retry, force, or
    retarget within the call.

Per-entry three-way rule:

| Base | Current | Candidate | Result |
|---|---|---|---|
| `current == base` | unchanged centrally | candidate changed | candidate |
| `candidate == base` | candidate unchanged | current changed | current |
| `current == candidate` | both agree | same value | that value |
| all different | incompatible change | incompatible change | path conflict |

Queued additions:

```text
non-overlapping queued A and B
    A completes its CAS
    B enters next and evaluates against A
    B creates the next Layer

overlapping queued A and B
    A completes its CAS
    B enters next and evaluates against A
    B returns path conflict
```

LayerStore never overwrites, force-updates, or inserts conflict markers. Real
content resolution returns to BranchStore `merge` and `commit`.

## 10. Transfer/add boundaries

The public operations remain separate and are called in this order:

| Topology step | Operations |
|---|---|
| Direct publication | `push_branch` -> `add_layer(BranchSource)` |
| Local Stack construction | `push_branch` -> `add_stack` |
| Stacked publication | `push_stack` -> `add_layer(StackSource)` |

Push transfers and verifies missing immutable data. Add references those
already-admitted rows and performs three-way/CAS. There is no compound
publish API or persisted coordination state between the two calls.

If push succeeds and add conflicts, transferred immutable data remains valid
and a later corrected Add sends no known payload. The underlying `Read`/`Write`
connection may be reused, but connection lifetime is not product state.
Per-object network calls and unbounded ID/payload batches remain forbidden.
Bounded in-memory page/frontier/bitmap state is allowed for the active call and
discarded afterward; it is never a persisted session or transfer table.

## 11. Idempotence

StackStore and LayerStore persist only the successful semantic addition map:

```text
add_results(source_id, result_id)
```

The primary key is `source_id`. Typed IDs derive both kinds: `BranchId` is the
source for `add_stack` and direct `add_layer`; `StackId` is the source for
stacked `add_layer`. One Branch may add one Stack or direct Layer, and one
Stack may add one Layer. Branch and Stack rows remain readable; the mapping
prevents repeated publication.

Adds return one generic typed value:

```text
AddResult<T> { result_id: T }
```

The inspectable result tag is validation, not another column:

```text
add_stack + mapped StackId in target history -> AddResult<StackId>
add_stack + mapped LayerId                 -> WrongSourceRoute
add_layer + mapped LayerId in target history -> AddResult<LayerId>
add_layer + mapped StackId                   -> WrongSourceRoute
```

The mapped result MUST belong to the explicit history. Mismatch returns typed
`WrongHistory<H> { expected, actual }` before source validation, transfer,
three-way, or CAS.

Duplicate-free state comes from canonical IDs, primary keys, CAS, and these
rules:

```text
CommitId = hash(type, parent IDs, root_id)
StackId  = hash(type, stack_history_id, parent_stack_id, root_id)
LayerId  = hash(type, layer_history_id, parent_layer_id, root_id)
```

No API accepts redundant root, parent, or version arguments when the named
typed manifest already resolves them. Explicit `history_id + node_id` pairs
remain only on create/pull boundaries for scope and wrong-history rejection.

| Repeated operation | Result |
|---|---|
| `push_branch` | Same candidate already at/behind same Branch head -> transfer reports no change |
| `add_stack` | Existing Branch source mapping -> return existing AddResult |
| `push_stack` | Same immutable suffix/copied-head state -> transfer reports no change |
| `add_layer` | Existing Branch/Stack source mapping -> return existing AddResult |

The new Stack/Layer and `add_results` row commit atomically. Repeating the same
typed source returns the existing result even after a history head advances.
Canonical IDs, primary keys, exact CAS, and `add_results` prevent duplicate
Branch updates, Stacks, Layers, and payload without a command log. Historical
error replay is not a core storage requirement.

## 12. Content-addressed objects

`root_id` is the content-addressed root of the complete logical filesystem,
not a delta root.

```text
Commit C8 ----+
Stack S4 -----+--> root R9 -> canonical trees -> CDC chunks
Layer L11 ----+
```

BranchStore stores only objects created or changed locally:

```text
filesystem read
    ├── changed/private ObjectId -> BranchStore
    └── unchanged accepted ref   -> configured parent, hash verified
```

Operations MUST reuse unchanged ObjectIds. Transfer announces IDs, requests
only missing payloads, verifies hashes and complete closure, then exposes
metadata atomically.

### One canonical CAS/CDC pipeline

Commit, Pull, Push, `add_stack`, and `add_layer` use one pipeline:

```text
logical edit
    -> reuse unchanged authenticated extent slices
    -> FastCDC v1 for newly supplied/replaced byte regions
    -> canonical encoded chunk, extent, and tree objects
    -> domain-separated ObjectIds
    -> top-down missing negotiation
    -> child-before-parent admission
    -> typed Commit/Stack/Layer manifest admission
    -> exact ref CAS / atomic AddResult when the operation mutates a ref
```

| Operation | Pipeline endpoint |
|---|---|
| `commit` | Admit changed objects/root/Commit, then exact-CAS Branch head |
| Pull/Push | Admit missing objects and typed manifests; mutate only the explicitly transferred mutable ref/copied head |
| `add_stack` | Shared three-way root -> admit new objects/Stack -> atomic AddResult + StackHistory CAS |
| `add_layer` | Shared three-way root -> admit new objects/Layer -> atomic AddResult + LayerHistory CAS |

There is no alternate chunker, hash, serializer, non-CAS write path,
whole-filesystem-copy fallback, or per-store merge implementation. A store
that cannot complete this pipeline returns `MissingBaseData` or `Integrity`;
it does not switch algorithms.

Payload chunks use exactly the existing LayerFS FastCDC v1 profile:

```text
minimum = 8 KiB
target  = 16 KiB
maximum = 32 KiB
profile = layerfs_core::cdc::profile_id()
```

The stored/transferred payload identity is only:

```text
canonical_bytes = encode_bytes_object(raw_chunk)
payload ObjectId = ObjectId::for_bytes(canonical_bytes)
objects(ObjectId, canonical_bytes)
```

The raw `chunk_id(raw_chunk)` digest is not a persisted ObjectId, extent
reference, missing-set ID, or transfer ID. Stores MUST NOT mix it with the
canonical encoded-object identity or invent another chunk-ID domain.

### Canonicality boundary

Canonicality is per stored object and typed manifest:

```text
same canonical object bytes -> same ObjectId
same manifest fields        -> same CommitId / StackId / LayerId
```

V1 does not claim that logically identical complete file bytes reached through
different edit histories always produce the same extent segmentation,
FileState root, or filesystem root. Editing reuses authenticated old extent
slices and FastCDC-chunks only replacement/new byte streams. This gives local
edit cost and suffix payload reuse without full-file rechunking.

There is no background normalization, full-file rechunk fallback, or unproven
localized-resynchronization algorithm. Shared `three_way` handles the three
regular-file leaves `base`, `current/target`, and `candidate/source` in this
order:

```text
FileState ObjectIds equal
    -> equal without reading bytes

FileState ObjectIds differ
    -> compare logical lengths first
    -> for each remaining distinct root, stream once through ContentDigestWriter
    -> cache at most three transient (length, digest) values

semantic_eq(candidate, base)    -> choose current representation
semantic_eq(current, base)      -> choose candidate representation
semantic_eq(candidate, current) -> choose current representation
otherwise                       -> path Conflict
```

The streaming fallback MUST NOT materialize a whole file and MUST NOT persist
the digest or introduce another ID. Each distinct root is streamed at most
once; it computes each required full digest rather than adding a paired-cursor
early-exit implementation. Across the required roots, the existing V1
rope/inode walkers perform `S` indexed structural-node reads plus
`sum_i ceil(E_i / 64)` batched payload-extent reads across the distinct-root
streams, with `O(1)` comparison memory.
The common ObjectId-equality path remains `O(1)`. Its immutable layered reads
may reach the configured parent during preflight, outside every write
transaction. Thus V1 does not claim equal roots for equal reconstructed bytes,
but it also does not report a false content conflict merely because valid COW
histories produced different FileState roots.

### Child-before-parent admission

A store may answer “known ObjectId” only for an admitted object:

```text
leaf chunk: verify hash -> admit

tree/root:
    receive canonical bytes into unexposed staging
    -> authenticate every referenced child
    -> require every child to be admitted
    -> verify parent hash
    -> admit parent atomically

Commit/Stack/Layer manifest:
    require admitted root
    -> verify typed manifest
    -> admit immutable manifest in a bounded batch

mutable Branch/history/LayerStore-copy ref:
    require complete admitted closure
    -> expose by one exact-CAS transaction last
```

No admitted tree or root may have a missing descendant. A failed operation may
leave unexposed staging bytes, but presence of those bytes is not a successful
`contains(ObjectId)` result. Staging is transient implementation workspace,
not another product table.

After admission, an authenticated root ID is the certificate for its complete
closure. Normal Commit/Merge/Add/Push/Pull preflight authenticates the exact
typed manifest and root ID, then prunes a known root without rereading all
descendants. Full closure traversal occurs only on first admission, an
explicit offline scrub, or integrity recovery. `three_way` descends only the
unequal Merkle frontier.

For Pull/Push, authenticated immutable Commit/Stack/Layer rows and transferred
AddResult provenance facts may also remain unreachable after an operation
error or final-ref CAS loss. They are valid deduplicated facts, not partial
product state. No read API may present them as current until
an exposed Branch/history/LayerStore-copy head reaches them. By contrast, a local
`add_stack`/`add_layer` attempt authors exactly one candidate: that candidate
row, its AddResult, and its head CAS MUST share one transaction and fully roll
back on CAS loss. Only already-admitted object rows may remain.

### Top-down missing negotiation

Negotiation starts from requested roots:

```text
receiver knows admitted root?  yes -> prune the complete subtree
                               no  -> request canonical node/child IDs
                                      recurse only into unknown children
                                      admit children before parent
```

IDs and payloads move in bounded pages/batches over the transfer stream. `N`
in transfer bounds is the visited Merkle frontier, not the full
closure. Implementations MUST NOT issue one `contains` RPC per object or first
enumerate the entire closure; known subtree IDs prune descent.

This 512-ID guarantee belongs to the transfer-specific frontier walker and is
valid only when that walker and its query-count test are implemented. Do not
reuse the existing logical COW, rope, inode, or semantic-digest walkers to
claim the same bound: those V1 walkers still load structural nodes one at a
time and batch only payload extents in groups of 64.

### Bounded database admission

Local Commit and cross-store transfer share authentication/insertion, but only
transfer needs missing-set negotiation:

```text
local Commit:
    CDC -> canonical encode once -> ObjectId hash once outside SQL txn
    -> retain trusted staged (ObjectId, canonical bytes) pair
    -> prepared batched INSERT ... ON CONFLICT DO NOTHING
    -> no re-encode or rehash in SQLite admission
    -> no pre-query
    -> closure/Commit admission
    -> Branch-head CAS last
```

If the local Commit produces zero or one object batch, admit objects, Commit,
and Branch CAS in one transaction. If it produces `J > 1` object batches,
commit the first `J - 1`, then combine the final closure-completing batch,
Commit row, and Branch CAS in transaction `J`. A CAS loss may leave only valid
unreachable objects from earlier batches; it never leaves the candidate Commit
or moved Branch ref.

For each cross-store visited frontier batch:

```text
sender: walk stored typed roots and stored ObjectIds
    -> announce bounded ObjectId batch without rechunking or re-identifying
receiver: one batched existing-ID query
    -> return missing bitmap/IDs
sender: read and send already-stored canonical bytes for missing IDs only
receiver: authenticate hash + canonical codec + child references
    -> exactly once per received missing frame
    -> prepared INSERT ... ON CONFLICT DO NOTHING
    -> no second hash in SQLite admission
    -> one bounded transaction for the admitted batch
    -> admit closure-complete parent/root
    -> admit closure-complete immutable metadata/provenance in bounded batches
    -> expose mutable ref/head in one small transaction last
```

CDC, canonical encoding, and ObjectId generation happen exactly once when new
bytes enter the sender's local CAS, such as during Commit or three-way output.
Pull/Push never rerun CDC over logical file bytes, reserialize an already-stored
object, or mint a new identity for transfer. The sender traverses stored
manifests/roots and streams stored canonical bytes without hashing them again;
the receiver authenticates each missing frame once.

The normal pipeline carries one trusted `(ObjectId, canonical_bytes)` pair from
encode/hash through admission. If transient DeferredObjectStore data spills to
disposable scratch, re-authentication on reload is the sole exception because
the scratch trust boundary changed. That exceptional hash is counted
separately and included in performance benchmarks; it does not authorize a
second normal admission hash.

The unique `ObjectId` primary key resolves concurrent/replayed inserts. There
is no `SELECT` per chunk, transaction per chunk, blind full-payload send, or
deduplication logic inside byte-only `layerfs-storage-core::wire`. Store
operations plan the missing set; `wire` carries only already-filtered frames.

SQLite-backed stores MUST provide batch adapters for object reads and writes.
They MUST NOT use the default `ObjectRead::get_authenticated_batch` behavior or
repeat `ObjectStore::put` directly against SQLite when those paths execute one
statement per object. Core traversal may request a batch, but the SQLite
adapter resolves it with one bounded `IN (...)` read or one prepared batched
insert transaction.

Each object insert page uses one multi-row statement:

```sql
INSERT INTO objects(object_id, bytes)
VALUES ... up to 128 validated rows ...
ON CONFLICT(object_id) DO NOTHING
RETURNING object_id, length(bytes)
```

The IDs returned are the rows admitted by this call. `sent_ids - returned_ids`
are the exact bytes/IDs that raced with an already-admitted insert; compute
that difference in transient memory. Do not issue a follow-up query per object
and do not persist transfer metrics. Store open requires SQLite `>= 3.35`
because `RETURNING` is part of this contract; an older library fails open
rather than selecting another write path.

### Missing bitmap and fixed bounds

`layerfs-storage-core` defines five compile-time bounds, not database/config
state:

```text
FRONTIER_IDS = 512
OBJECT_BATCH_ROWS = 128
FACT_BATCH_ROWS = 128
BATCH_BYTE_TARGET = fixed normal encoded-byte target
```

The one batch packer is deterministic: take entries in canonical transfer
order and greedily append while both row count `<= OBJECT_BATCH_ROWS` and
encoded bytes `<= BATCH_BYTE_TARGET`. If one valid object is larger than
`BATCH_BYTE_TARGET`, admit it alone; it remains
bounded by the existing `MAX_OBJECT_BYTES`. Never derive the transaction count
as the maximum of aggregate row/byte ceilings, because the actual mixed-size
packing can require more batches. The 128-row `objects(object_id, bytes)` batch
uses at most 256 binds. The widest four-column immutable fact batch uses
`FACT_BATCH_ROWS * 4 = 512` binds. Prepare that widest fixed statement at Store
open; narrower fact tables use the same 128-row cap. A 256-row fact batch is
forbidden even if one SQLite build happens to allow more variables.

At Store open, prepare one fixed-shape 512-placeholder existence statement:

```sql
SELECT object_id
FROM objects
WHERE object_id IN (?1, ?2, ... ?512)
```

The announced page is sorted and duplicate-free. Bind its `n <= 512` IDs in
order and pad unused parameters with SQL `NULL`; never prepare dynamic 1..512
query shapes. SQLite result order is irrelevant: receiver builds an in-memory
set from returned IDs, then emits one fixed 512-bit/64-byte position bitmap
where bit `1` means missing and unused tail bits are zero. Sender transmits
canonical bytes only for set bits. Malformed length/order/duplicates/nonzero
tail bits are `Integrity`; there is no alternate missing-ID encoding in V1.
`EXPLAIN QUERY PLAN` MUST show lookup through the `objects.object_id` primary
key/index, never a table scan.

Each typed `H` membership page likewise contains at most 512 IDs and returns a
fixed 512-bit/64-byte typed bitmap. Use one prepared 512-placeholder primary-key
membership statement per relevant typed table, with trailing `NULL` padding,
unordered-result remapping, and an indexed `EXPLAIN` plan; an ordered
Commit/Layer/Stack ancestry page may instead use its named fixed recursive-CTE
page.
Typed IDs never use the `objects` statement. Dynamic 1..512 shapes and per-ID
typed membership queries are forbidden. The companion low-level specification
owns the exact framing and query mechanics.

### Mandatory transfer pipeline

`P_o` is the actual count of 512-ObjectId membership pages after known-root
pruning. `H` is the typed ancestry/membership page count. `P` is the actual
dependency-ordered wire-page count after mandatory coalescing: one wire page
may carry one typed-ID page and one ObjectId page with separate table-specific
missing bitmaps, so `P <= P_o + H`. Typed IDs are never queried against
`objects`, and ObjectIds are never queried against typed tables. These are
actual packer outputs, not aggregate ceilings. A valid stored object above the
normal byte target is a singleton bounded by `MAX_OBJECT_BYTES`.

After the first announcement, payload and the next announcement overlap:

```text
sender                                      receiver
  announce frontier page 1 ----------------->
                         <----------------- missing bitmap 1
  missing payload 1 + announce page 2 ------>
                         <----------------- ack 1 + missing bitmap 2
  missing payload 2 + announce page 3 ------>
                         <----------------- ack 2 + missing bitmap 3
  ...
  missing payload P + final intent ---------->
                         <----------------- final ack/result
```

Each page performs at most one batched ObjectId membership query plus one
table-specific typed membership query when that page carries both sets. On an
already-open stream, any Pull/Push transfer uses at most `P + 1` protocol round
trips. A known requested root can finish in the first reply. Piggybacking is
mandatory; implementations MUST NOT serialize a separate announce RTT and
payload-ack RTT for every page.

One live `Read`/`Write` stream is reused for all
pages and for an immediately following Add to the same endpoint. There is no
handshake per page/operation, connection pool, persisted session, or async
runtime requirement.

### SQLite ownership, durability, and checkpoints

Every BranchStore, StackStore, and LayerStore SQLite file opens with:

```text
PRAGMA journal_mode = WAL
PRAGMA synchronous = FULL
PRAGMA temp_store = FILE
PRAGMA cache_size = benchmark-frozen SDK page budget
```

The page budget is a code constant, not a user/database field. It bounds the
recursive merge-base CTE cache while allowing SQLite transient work to spill
to its temp file.

Exactly one owning Store process/handle may open a database file for product
use. A second owner fails promptly as `StoreBusy`; do not coordinate writers
with a lease/owner table. Remote callers use the Store's operation endpoint and
MUST NOT open its SQLite file over NFS, SMB, FUSE, or another network
filesystem. This is separate from the single-writer StackHistory capability:
file ownership protects SQLite; the signed capability protects remote
StackHistory authority.

That Store owns one SQLite writer and one active mutation pipeline. Concurrent
callers queue before mutation preflight/transactions; the multi-row
`RETURNING` raced-existing path remains a defensive correctness guard, not a
reason to add another production writer. Read work may be batched around the
pipeline, but V1 has no connection pool or parallel write path.

No SQLite write transaction may be open during a network wait, CDC, canonical
encode/hash, StackHistory signature verification, semantic digest streaming,
or shared `three_way`. Finish those steps first. Database critical sections
are limited to a prepared set query/read snapshot, one bounded object/fact
insert transaction, or the final exact CAS. A folded final admission
transaction contains its bounded batch plus a constant number of typed
metadata/AddResult/CAS statements.

Do not apply a blanket 10 ms goal to a folded FULL+WAL batch that may contain a
valid multi-MiB singleton. If a visibility-only transaction has no admission
or fact batch, it is measured separately from folded batch lock time. Tests
record maximum and p95 writer-lock duration for both categories; batch byte
and row limits, rather than an unmeasured second writer, bound the critical
section.

For `J` packed object batches and `F` immutable typed-fact batches, a
ref/head-mutating transfer uses `max(1, J + F)` durable transactions when it
actually changes the ref: the final admission batch also performs the small
visibility CAS. An already-current transfer writes zero. `pull_commit_history`
has no ref and uses exactly `J + F` transactions, or zero when everything is
known. With
`synchronous=FULL`, acknowledging an operation requires the applicable final
Commit/Branch, Stack/AddResult/head, Layer/AddResult/head, or transferred ref
commit to be durable. No explicit checkpoint may run inside that final CAS
window.

V1 exposes no checkpoint setting. Before release, benchmark the fixed
`wal_autocheckpoint` threshold against the representative Commit/Pull/Push/Add
workloads and freeze the chosen constant in code. Benchmark p95 MUST include
checkpoint spikes. If explicit checkpointing is needed, the only allowed call
is `wal_checkpoint(PASSIVE)` between completed operations; never TRUNCATE,
RESTART, or a checkpoint inside an active storage operation. Changing this
policy later requires new measurements, not another database column or SDK
option.

Required storage gate:

```text
10 identical byte streams applied through the same edit path from one base,
sharing one physical CAS-bearing store
    ~= 1 package payload chunk set
     + O(10) Commit/Stack/Layer refs
     + per-install changed structural tree nodes
```

Separate disconnected physical stores may each need one placement copy. No
normal operation may materialize a full accepted filesystem inside
BranchStore. Structural metadata MUST be measured separately from reused
package payload; it must not be falsely reported as duplicate payload.
Different edit histories that end in the same logical bytes may retain
different valid COW roots/chunks. V1 requires clean semantic merging through
the streamed-digest fallback, not edit-history-independent storage
convergence.

Deduplication scope is one physical `objects` table/database plus missing-only
receiver transfer. Ten Branches sharing one BranchStore should approach one
payload copy. Ten independent BranchStore databases are ten independent CAS
placements and may each hold one private copy. V1 does not add a shared global
CAS, registry, or cross-database dedup service.

Normative deduplication boundaries:

| Boundary | Required behavior |
|---|---|
| Within BranchStore | Commit/merge reuse the same CDC-derived ObjectIds; `objects` PK skips existing payload; Commit/ref appears after closure verifies |
| Within StackStore | Pulled/local Commit and Stack roots reuse one objects table; `add_stack` inserts only missing merge objects before Stack/AddResult/head |
| Within LayerStore | Branch/Stack/Layer roots reuse one objects table; `add_layer` inserts only missing merge objects before Layer/AddResult/head |
| Branch push to StackStore/LayerStore | Receiver computes missing set from the same ObjectIds, requests missing payload only, idempotently inserts, verifies closure, then exposes Commit/Branch metadata |
| Stack push to LayerStore | Receiver skips known subtrees/Stacks, transfers missing suffix/provenance/payload only, verifies closure/attestation, then advances copied metadata head |
| Pull operations | Destination performs the same receiver missing-set protocol and exposes requested typed metadata last |
| `add_stack` / `add_layer` | Local receiver plan drops already-present objects; canonical three-way output, PK insert, closure, AddResult, and CAS remain idempotent |

Already-present receiver chunks are removed from the incoming payload plan,
not stored again under another row or reference. Push never deletes sender
objects. Garbage collection and sender pruning are deferred policies, not
implicit consequences of transfer.

Discard/retention behavior is exact:

| State | Required disposition |
|---|---|
| Receiver ID present before negotiation | Omit its bytes from the sender's outgoing plan |
| ID becomes present after negotiation but before insert | `DO NOTHING RETURNING` omits it; drop the duplicate incoming bytes from the active buffer and create no row |
| Invalid hash/codec/child closure | Delete transient receive/deferred scratch before any typed/ref write; return `Integrity` |
| First three-way Conflict | Delete DeferredObjectStore memory/scratch immediately; write zero live rows |
| Valid immutable row admitted before an operation error/final-ref CAS loss | Retain as unreachable, safe CAS state pending separately approved GC |
| Sender object after any Push | Retain; Push never deletes or prunes sender state |

V1 active-memory/CPU bounds are likewise explicit:

```text
transfer frontier       <= 512 IDs + one 64-byte bitmap
object/fact write batch <= 128 rows + BATCH_BYTE_TARGET
oversized object        = one singleton <= MAX_OBJECT_BYTES
semantic comparison     = three small digest records + streaming buffers
merge conflict result   = one core-path-limit-bounded path + three object states
merge-base ancestry     = bounded SQLite page cache + temp-file spill
```

Transfer CPU is linear in the visited frontier plus bytes authenticated for
receiver-missing objects; no present payload is rehashed. Logical merge CPU is
linear in `S + E` until Clean completion or the first deterministic Conflict.
All canonical codec/core object-count, child-count, depth, and
`MAX_OBJECT_BYTES` limits are enforced before admission. No operation may
accumulate an unbounded conflict list, Rust ancestry set, or in-memory transfer
frontier.

The COW-locality and FastCDC proofs are separate:

```text
COW replace fixture:
    start from an admitted rope
    replace one byte range
    FastCDC scans exactly the replacement/new byte stream
    old suffix payload reads = 0
    unchanged old extent ObjectIds outside the cut remain referenced

Independent FastCDC fixture:
    FastCDC-chunk original bytes from scratch
    insert the frozen shift and FastCDC-chunk shifted bytes from scratch
    assert the fixture's exact frozen suffix ObjectId sequence reappears
    run a fixed-block chunker oracle and assert that oracle does not reuse it
```

The COW test proves splice locality, not CDC resynchronization. The independent
fixture proves FastCDC behavior without relying on prior extents. Neither test
promises a universal reuse percentage or requires an edited COW root to equal
a from-scratch rebuilt root.

### Query plans and operation bounds

Ancestry traversal is batched:

```text
Layer/Stack strict list -> named recursive CTE, emitted in <=512-row pages
Commit DAG              -> named recursive CTE over both parents with UNION
                           dedup, emitted in <=512-row pages
membership/base/head    -> one joined indexed query, not one query per relation
```

One read-only recursive-CTE cursor/snapshot may remain open while its pages are
transported; SQLite owns its dedup/spill state. This is one source statement,
while `H` counts emitted homogeneous pages and the `2H` query bound remains
conservative. No write transaction or writer gate may span network I/O.

N+1 relational parent queries are forbidden for Layer, Stack, and Commit
ancestry. Equal subtree hashes stop object descent, and no operation enumerates
a full closure after a known-root hit. This does not relabel the current
logical walker's honest `S` indexed structural-node reads as a 512-ID batch.

Let:

```text
A_t = typed ancestry/membership IDs visited for typed table t after
      known-node pruning
N = ObjectIds visited after known-subtree pruning
H = sum over nonempty typed tables t of ceil(A_t / 512)
    homogeneous ancestry/membership query pages
C = merge-base recursive CTE statement count, fixed and small (`1 <= C <= 3`)
P_o = actual 512-ObjectId membership-query pages after known-root pruning
P = actual coalesced dependency-frontier wire-page count; `P <= P_o + H`
J = actual greedy object-insert batch count
F = actual greedy immutable typed-fact/AddResult plus frozen Push Stack
    Branch-provenance batch count, including the terminal fact when an
    operation has no mutable ref
S = actual indexed structural-node reads made by existing logical walkers
E_i = payload extent/object reads in logical stream i
G = sum over logical streams i of ceil(E_i / 64) payload batch reads
L = actual layered-parent read turns during Branch preflight; current logical
    walkers may contribute individual structural turns plus 64-wide payload turns
```

For composite pulls, `A_t`, `N`, `P_o`, `J`, and `F` include required base
Layer/Stack dependencies, so nested preparation does not add an unbounded
hidden term.

Bounds count indexed SQL queries across both endpoints, bounded write
transactions, and application protocol round trips for one operation's
service time after it enters the Store mutation pipeline. A reused live stream
adds no repeated handshake. Queue wait is reported separately and is not
hidden inside the RTT/p95 service bound.

| Operation | Indexed queries upper bound | Write transactions upper bound | Protocol round trips upper bound |
|---|---:|---:|---:|
| Local `commit` | `1 + S + G`; one Branch/base read, indexed structural walks, 64-wide payload batches, and no existence pre-query | `max(1, J)`; a small Commit combines its object batch + Commit + Branch CAS in one transaction | `0` embedded/prepared, otherwise `<= L`; write transaction has no network |
| Local Branch `merge` | `1 + C + S + G`; joined heads/bases, fixed-count recursive merge-base CTEs, honest structural reads, and payload batches across at most three semantic streams | `max(1, J)` on Clean; Conflict writes zero | `0` embedded/prepared, otherwise `<= L`; write transaction has no network |
| `pull_branch(source_branch_id, local_branch_id)` | `1 + 2H + P_o` (source fetch + receiver membership/existence) | fresh/fast-forwarded ref `max(1, J + F)`, final batch includes Branch ref; equal/local-ahead/divergent `0` ref writes | `<= P + 1` |
| `pull_commit_history` | `1 + 2H + P_o`; pinned head is one query | `J + F`; terminal pinned Commit is in the last fact batch; all-known `0` | `<= P + 1` |
| `pull_layer_history` | `2H + P_o` | changed observed ref `max(1, J + F)`, folded into final batch; UpToDate `0` | `<= P + 1` |
| `pull_stack_history` | `2H + P_o`, including base-Layer pages | changed read-only head `max(1, J + F)`, folded into final batch; UpToDate `0` | `<= P + 1` |
| `push_branch` to StackStore or LayerStore | `1 + 2H + P_o`; first query joins AddResult + Branch head | changed Branch `max(1, J + F)`, CAS in final batch; UpToDate `0` | `<= P + 1` |
| `push_stack` | `1 + 2H + P_o`; attestation/copied-head preflight is one query | changed copied head `max(1, J + F)`, CAS in final batch; UpToDate `0` | `<= P + 1` |
| `add_stack` | `1 + S + G` (joined source/history read + honest structural reads + 64-wide payload batches) | Conflict `0`; Clean `max(1, J)` with candidate Stack/AddResult/head in the last transaction | `0` local or `1` request/result on reused stream |
| `add_layer` | `1 + S + G` | Conflict `0`; Clean `max(1, J)` with candidate Layer/AddResult/head in the last transaction | `0` local or `1` request/result on reused stream |

The Store admits one active operation at a time. A queued Add reads the current
head when it enters, evaluates once, and exact-CASes once. An injected or
illegal out-of-band head movement rolls back the candidate transaction and
returns `HeadMoved`; the operation never retries internally.

Short-circuit bounds:

| State already present | Required work |
|---|---|
| AddResult exists | one indexed query, zero writes, at most one response RTT |
| Branch head equals/contains candidate | one joined query, zero payload/transactions, one response RTT |
| LayerStore copied Stack head equals or descends from incoming | one attestation + ancestry query, zero CAS/payload, one response RTT |

Add never performs network refetch. If Push/Pull did not admit a required
closure, Add returns `MissingBaseData`; caller completes the explicit transfer
before making a new Add call. This prevents duplicate object negotiation
between Push and Add.

End-to-end publication on one reused stream is therefore bounded by:

```text
direct:  push_branch transfer (P_branch + 1) + add_layer result (1)
         <= P_branch + 2 RTTs

stacked: push_branch (P_branch + 1) + add_stack (1)
       + push_stack  (P_stack  + 1) + add_layer (1)
         <= P_branch + P_stack + 4 RTTs
```

## 13. Minimum SQLite tables

Typed Commit, Stack, and Layer rows are canonical searchable manifests. They
MUST NOT be collapsed into a generic object-only table that requires blob
decoding or corpus scans, and they MUST NOT be combined into a nullable god
table.

No safe schema reduction remains:

| Store | Irreducible separation |
|---|---|
| BranchStore | `objects` payload CAS, `commits` DAG, and mutable `branches` refs have different keys/mutability |
| StackStore/LayerStore | The same three plus two independent history/node pairs and `add_results` for single-publication idempotence |

Combining any pair either mixes mutable and immutable rows, loses indexed
ancestry/head CAS, or removes successful-add idempotence. Therefore the cold
implementation installs exactly 3/9 and 8/24 without legacy tables.

### BranchStore: 3 tables / 9 columns

| Table | Exact columns |
|---|---|
| `objects` | `object_id PK`, `bytes` |
| `commits` | `commit_id PK`, `root_id`, `parent_id NULL`, `merge_parent_id NULL` |
| `branches` | `branch_id PK`, `head_commit_id`, `base_id` |

BranchStore deliberately has no local foreign key from `commits.root_id` to
`objects`: an anchor/base root may live only in the configured parent, and a
private root may reference both parent and local objects. The layered resolver
must verify that cross-store closure before Commit admission.

### StackStore: 8 tables / 24 columns

| Table | Exact columns |
|---|---|
| `objects` | `object_id PK`, `bytes` |
| `commits` | `commit_id PK`, `root_id`, `parent_id NULL`, `merge_parent_id NULL` |
| `branches` | `branch_id PK`, `head_commit_id`, `base_id` |
| `layer_histories` | `history_id PK`, `head_layer_id` |
| `layers` | `layer_id PK`, `history_id`, `parent_id NULL`, `root_id` |
| `stack_histories` | `history_id PK`, `base_layer_id`, `head_stack_id` |
| `stacks` | `stack_id PK`, `history_id`, `parent_id NULL`, `root_id` |
| `add_results` | `source_id PK`, `result_id` |

### LayerStore: 8 tables / 24 columns

LayerStore uses the same eight table shapes, holds the complete central rows,
and is the only authoritative writer of `layer_histories.head_layer_id`.

Allowed `add_results` pairs are exactly:

```text
BranchId -> StackId
BranchId -> LayerId
StackId  -> LayerId
```

Required traversal/shape indexes add no columns:

```text
commits(parent_id)
commits(merge_parent_id)
branches(base_id)

UNIQUE layers(history_id)            WHERE parent_id IS NULL
UNIQUE layers(history_id, parent_id) WHERE parent_id IS NOT NULL
UNIQUE stacks(history_id)            WHERE parent_id IS NULL
UNIQUE stacks(history_id, parent_id) WHERE parent_id IS NOT NULL
INDEX  add_results(result_id)
```

The partial unique Layer/Stack indexes may also satisfy ordinary
`(history_id, parent_id)` traversal when the SQLite query plan proves it; do
not add redundant B-trees speculatively.

Layer uniqueness enforces one genesis and at most one child per LayerHistory
parent. Stack uniqueness enforces one seed and at most one child per
StackHistory parent, so both histories remain strict lists.

Deliberately absent:

| Not stored | Why |
|---|---|
| LayerStack table/ID | LayerStack is the architecture |
| CommitHistory table/ID | Commit parents form the DAG |
| Stack version/freeze/lease/owner | StackHistory head is the only mutable Stack ref; authority stays in SDK configuration |
| Branch parent/fork/history/root | Commit ancestry and base object derive them |
| LayerHistory sequence | Follow Layer parents from its head |
| Snapshot payload copies | Commit, Stack, and Layer reference `root_id` |
| Object closure table | Traverse canonical tree objects |
| Command-result/receipt/outbox table | `add_results` stores successful semantic addition identity; command logs remain outside the core |
| GC/rollback tables | Policy is deferred |

Workspace copy-on-write state is transient BranchStore runtime state backed by
memory or disposable scratch files. It is not a core table and never changes
the three-table BranchStore schema.

## 14. Crate ownership

| Crate | Sole ownership |
|---|---|
| `layerfs-storage-core` | Typed IDs/manifests/contracts, exact 3-table/9-column and 8-table/24-column schemas/queries, canonical hashing/CDC, closure admission, CAS primitives, shared Commit/Stack/Layer `merge_base`, shared `three_way`, and byte-only `wire` |
| `layerfs-branch-store` | BranchStore persistence and create/commit/merge semantics; transient Workspace COW |
| `layerfs-stack-store` | StackStore persistence, creator write capability, StackHistory creation, `add_stack`, Stack head CAS, and StackStore-side transfer orchestration |
| `layerfs-layer-store` | LayerStore persistence, SDK genesis provisioning, `add_layer`, authoritative Layer head CAS, read-only copied Stack heads, and LayerStore-side transfer orchestration |

BranchStore calls `layerfs-storage-core` for `merge_base`; all three store
crates call it for `three_way`. None owns a second implementation. Stores
encode/decode shared typed contracts and orchestrate missing-ID negotiation.
`layerfs-storage-core::wire` is at most 250 handwritten LOC and provides opaque
framing, bounded batches, backpressure, checksums, and incomplete-frame
rejection over `Read`/`Write`. It MUST NOT interpret ObjectId, history, CAS,
conflict, deduplication, admission, or SQLite semantics. Stores communicate
through those byte protocols rather than
depending on another store crate's persistence internals.

The cold storage architecture has four packages: `layerfs-storage-core` and
the three store crates. There is no `layerfs-transfer`, `layerfs-sync`, or
`layerfs-server` crate. Deployment may embed stores or wrap them in an
external service without moving storage semantics into a server god crate.

## 15. Forbidden behavior

| Forbidden | Required replacement |
|---|---|
| Stack versions, mutable Stack rows, freeze/lease/owner columns | Immutable Stack list + one history-head CAS + SDK writer capability |
| Multiple writable copies of one StackHistory | One creator StackStore writer; pulled and LayerStore copies are read-only |
| `merge_stack`, promote, or Stack-to-Layer mutation hidden in push | Explicit `push_stack` then `add_layer` |
| Stack-bound Branch calling `add_layer` | `add_stack`, `push_stack`, then `add_layer(StackSource)` |
| Push creating a Stack or Layer | Push transfers only |
| Pull using historical version arguments or latest aliases | Exact `through_layer_id` or `through_stack_id` |
| Pull replaying operations/payload history | Walk parents backward, stop at known nodes, transfer missing data |
| Last-write-wins or automatic path resolution | Structured conflict returned to BranchStore |
| Conflict Vec, all-conflicts scan, count, truncation flag, or continuation | Return the first canonical lexicographic Conflict tuple and stop |
| Rust ancestry `HashSet`/materialized Commit DAG | Indexed recursive SQLite CTE with `UNION` dedup and at most two final candidates |
| Blind LayerHistory CAS retry | Return exact `HeadMoved`; queued callers evaluate the current head once when admitted |
| Whole-database or full-closure copy into BranchStore | Layered reads plus changed objects only |
| Generic store superclass or role flags | Three explicit store responsibilities |
| Convenience object/sync verbs | Fold into the fourteen public operations |
| Legacy aliases, dual schemas, role adapters, migration shims, or fallback algorithms | Cold install only the exact contract in this file |
| Bloom filters, compression, connection pools, async runtimes, transfer/temp tables, or alternate local fast paths | Fixed bounded synchronous `Read`/`Write` pages and prepared SQLite batches until measurement proves a specific need |
| Multiple Store owners or remote raw-SQLite/network-filesystem access | One owning Store handle per DB file; remote callers use storage operations over the Store endpoint |
| Runtime checkpoint tuning or checkpoint inside an operation | Benchmark-frozen auto-checkpoint; optional PASSIVE only between completed operations |

Retired names MUST NOT appear in APIs or new implementation:

```text
WorkingStore
DurableStore
DurableStoreCache
MainLane
SideLane
Origin
Mirror
Repository
Worktree
```

## 16. Deferred transport and process recovery

The experimental V1 assumes the Store process and endpoint remain available
for the duration of an operation. An interrupted or incomplete frame returns
an error, admits no incomplete row, and exposes no incomplete ref/head.

The following are deliberately outside the current implementation and terminal
acceptance gates:

- reconnect/resume after a partial transfer;
- lost-acknowledgement replay guarantees;
- process-crash kill/reopen matrices; and
- cleanup of scratch files left by process death.

Canonical IDs, idempotent inserts, `add_results`, exact CAS, and atomic
visibility transactions remain required. They preserve the data model without
adding request/session/recovery state, but V1 does not claim the deferred
recovery behaviors above.

## 17. Acceptance checklist

Required concurrency/conflict tests:

| Test | Required proof |
|---|---|
| Cross-base merge resolution | Closest common Commit wins before Stack, Stack before Layer; unrelated LayerHistories return `NoCommonBase` |
| Merge result route | Layer/Stack cross-type merge preserves the target tagged base and later add route |
| Merge fast-forward | Equal/contained is `UpToDate`; same-base target ancestor fast-forwards by exact CAS without a merge Commit |
| Merge dependency and ambiguity | Missing ancestry returns `MissingBaseData`; zero/one/two final Commit CTE candidates continue to Stack/Layer fallback / select one Commit base / return `AmbiguousMergeBase`; `NoCommonBase` occurs only after no common valid Commit, Stack, or Layer remains, and no rejection writes |
| Merge-base bounded memory | Large diamond DAG uses `C <= 3` recursive CTE statements with `UNION` dedup, returns at most two final candidates, allocates no ancestry `HashSet`, stays within frozen page-cache memory, and spills only to SQLite temp storage |
| Merge-base query plan | EXPLAIN uses Commit PK/parent indexes and transient recursive/final-candidate B-trees, never a full commits-table scan; temp bytes, page-cache peak, and candidate-page size are recorded |
| Injected Merge CAS loss | CAS loss rolls back the candidate Commit and returns exact expected/actual heads without automatic retargeting |
| Shared three-way fixture | Branch `merge`, `add_stack`, and `add_layer` return byte-identical clean roots or the identical first `Conflict { path, base, current, candidate }` for the same three roots |
| First conflict only | Multiple conflicts are arranged out of input order; traversal returns the first canonical bytewise-lexicographic path and stops with no Vec/count/truncated flag or later-path reads |
| Conflict is zero-write | Repeated Branch Merge/Add conflicts use only transient DeferredObjectStore; live object/typed/AddResult/head row counts remain byte-for-byte unchanged and scratch is discarded |
| Queued clean `add_stack` | A completes first; B then reads A's Stack as current, evaluates once, and creates the next Stack; history is linear |
| Queued conflicting `add_stack` | A completes first; B then evaluates once and returns the first deterministic path conflict with no Stack, mapping, markers, or head overwrite |
| Queued clean `add_layer` | A completes first; B then reads A's Layer as current, evaluates once, and creates the next Layer |
| Queued conflicting `add_layer` | A completes first; B then evaluates once and creates no Layer/mapping, returning the exact first deterministic conflicting path |
| Injected Add CAS loss | Candidate Stack/Layer, AddResult, and head movement roll back together; return exact `HeadMoved` with no internal retry |
| No-op additions | Atomically record `add_results` to current Stack/Layer and create no snapshot row |
| Known-root Add | Normal/no-op Add authenticates typed source/root IDs but reads zero descendants for an already-admitted equal root; three-way visits only unequal frontier nodes |
| Read-only StackHistory | Pulled copy and LayerStore copy reject `add_stack`; creator handle succeeds |
| Remote writer attestation | Missing/forged key, signature, expected head, suffix digest, or request digest is rejected before metadata exposure/copied-head CAS |
| Pull capability | Pulled history receives public verification material only, never the private signer, and remains read-only |
| Writable clone guard | A cloned database without the SDK signer cannot write; namespace equality alone grants nothing |
| Stack copy transfer | Equal is `UpToDate`; ancestor fast-forwards verified suffix; divergent copied head is integrity error |
| `pull_commit_history` pin | Return the exact Branch head pinned when the serialized operation enters; mutate no Branch ref |
| Pull Commit all-known | When pinned Commit/closure is already known, `pull_commit_history` performs zero writes; otherwise terminal Commit is in the last immutable-fact batch with no synthetic ref transaction |
| Pull Branch fresh local ID | Create the requested absent local ID at the pinned source base/head with zero accepted-payload copy; an existing different local ID returns `HeadMoved` without mutation |
| Pull Branch same-ID states | Absent creates, equal is `UpToDate`, local-ancestor exact-CASes forward, source-ancestor preserves local-ahead, and divergence returns `HeadMoved` with zero ref writes |
| Pull Branch divergence | Pull the source into a fresh local ID, Merge it into the existing target, then Push; no hidden merge, overwrite, ref table, or new operation |
| Closure fault | Missing/corrupt child prevents parent/root/Commit/Stack/Layer visibility |
| Repeated payload | Ten Branches in one BranchStore applying identical byte streams through the same edit path from one base yield approximately one payload set; independent BranchStore DBs may each keep one placement copy |
| COW replace locality | Replacing a range scans exactly replacement bytes, reads zero old suffix payload, and retains unchanged old extent IDs outside the cut |
| Independent FastCDC resync | From-scratch original and frozen-shift fixture share the exact expected suffix ObjectId sequence; a fixed-block oracle on the same bytes fails that reuse |
| Edit-history representation | Two edit sequences may end with equal logical bytes and different valid roots; each object/manifest identity remains canonical and closure-complete |
| Representation-neutral merge | Different FileState roots with equal logical bytes compare equal after full streamed digests; each distinct root is streamed once with no whole-file materialization or persistent digest |
| Chunk identity | For every FastCDC chunk, all stores persist/transfer `ObjectId::for_bytes(encode_bytes_object(raw))`; raw `chunk_id(raw)` never appears in object rows or extent refs |
| One encode/hash | Local new objects encode/hash once before trusted staging and SQLite does neither again; sender streams stored bytes unchanged; receiver authenticates each missing frame once; scratch-spill reload hashes are separately counted exceptions |
| Cross-store missing-only | Preseed receiver subset, then assert transferred ObjectIds equal exactly receiver-missing ObjectIds |
| Discard/retain matrix | Preexisting bytes are never sent; raced duplicates leave the receive buffer with no new row; invalid/conflict scratch is deleted; valid unreachable immutable rows remain unexposed; sender rows are never deleted |
| Bounded operation memory | Peak memory stays within one 512-ID frontier, 64-byte bitmap, 128-row/byte-capped batch or singleton, streaming buffers, and scalar Conflict; large ancestry spills through bounded SQLite temp/page cache |
| Fixed missing query | Short/full pages reuse one 512-placeholder prepared statement with NULL padding; bitmap is 64 bytes with zero tail bits; shuffled SELECT results map correctly; EXPLAIN uses the ObjectId PK |
| Batched admission | For each bounded ID batch assert one existing-ID query and one bounded insert transaction, never one per chunk |
| Mixed-size batch packing | Assert deterministic greedy count+byte packing; an object above the byte target but within `MAX_OBJECT_BYTES` is one singleton transaction |
| Typed fact batch shape | Widest four-column statement is prepared at open for exactly 128 rows/512 binds; all fact tables cap at 128 and never select a 256-row shape |
| SQLite batch adapter | Instrument transfer/payload batch reads and object writes and prove they do not fall back to one `get_authenticated_batch`/`put` statement per payload object; do not misreport structural logical-walker reads |
| SQLite insert race | Two test connections race the same validated 128-row page; `RETURNING` partitions newly inserted versus raced-existing IDs exactly, final rows are unique, and no per-object follow-up query runs |
| Byte-only wire | `storage-core::wire` receives only filtered missing frames and has no ObjectId lookup/dedup branch |
| Local Commit admission | Assert zero existence pre-query, batched idempotent object inserts, and one final Commit/Branch-CAS transaction |
| Ancestry query count | Layer/Stack pages and the one stepped Commit cursor produce bounded typed pages; destination membership is one set query per page, never N+1 per node |
| Commit ancestry cursor | One prepared UNION-dedup recursive CTE cursor emits at most 512 rows per page without LIMIT/OFFSET reruns or a Rust seen set; its read snapshot may span page transport while no writer gate/write transaction does |
| Transfer frontier walker | Explicit transfer walker uses true 512-ID pages, one existence-query turn per page, and known-root pruning; the test does not substitute the existing logical walker |
| Logical walker query count | Branch Commit/Merge/Add reports `S` indexed structural reads plus `sum_i ceil(E_i/64)` payload batches; layered-parent `L` includes both and no write transaction opens until all reads finish |
| Connection reuse | Multi-page Pull/Push and adjacent Push/Add use one live stream and no repeated handshakes |
| Pipelined frame order | Payload/ack for page i is paired with announcement/bitmap for page i+1 under bounded backpressure and transfer never exceeds `P + 1` RTTs |
| Add after Push | Add performs zero network refetch; missing closure returns `MissingBaseData` |
| Folded transfer visibility | A changed ref/head is committed with the final object/fact admission batch (`max(1, J + F)` total); UpToDate writes zero and no extra final-only transaction is emitted |
| Local Add final-CAS fault | Candidate Stack/Layer row, AddResult, and head change all roll back together; previously admitted object rows may remain |
| Store ownership | A second owning process/handle for the same DB fails `StoreBusy`; remote endpoint use succeeds without opening SQLite over a network filesystem |
| Ten-caller writer load | Ten callers share one fair serialized mutation pipeline and one active buffer set; each operation preserves child-before-parent, typed-fact, AddResult/ref/head-last order, exact-CASes last, leaves the correct final heads with PK-deduplicated rows and no partially visible closure, and completes without starvation; independent Store files may progress independently |
| Writer-lock scope | Instrument max/p95 lock duration and prove no write transaction spans network wait, CDC/encode/hash, signature verification, semantic digest, or three-way; report visibility-only separately from folded bounded batches |
| WAL checkpoint | Benchmark includes checkpoint spikes in p95; no checkpoint occurs inside an operation/final CAS window; if enabled, PASSIVE runs only between operations |
| Payload accounting | Report payload-chunk bytes separately from changed structural tree and Commit/Stack/Layer metadata bytes |
| No fallback | Commit/Pull/Push/Add fixtures use the same CDC/hash/serialization IDs and fail instead of selecting another algorithm |

Every failed local Branch-Merge/Add CAS test MUST assert that its authored
candidate typed row and `add_results` row were rolled back, not merely that the
head stayed unchanged.
Storage/performance tests derive counts from canonical tables or external test
instrumentation; they MUST NOT add metrics tables to production schemas.

- [ ] Public operation list is exactly the fourteen verbs in section 4.
- [ ] SDK setup atomically provisions LayerHistory plus canonical empty genesis.
- [ ] Content IDs are domain-separated hashes; Branch/History IDs are typed UUIDv7 and collision-checked; StackHistoryId also commits to its verification key; no StoreId exists.
- [ ] Direct push is transfer-only; only `add_layer` creates a Layer.
- [ ] Stacked pushes are transfer-only; only `add_stack` creates a Stack.
- [ ] StackHistory is a strict list with one exact-CAS head and no version.
- [ ] Exactly one creator StackStore may write each StackHistory; pulled/LayerStore copies are read-only.
- [ ] StackHistoryId commits to the verification key; every LayerStore copied-head fast-forward verifies the exact signed tuple before CAS.
- [ ] Pull never transfers the private signer; embedded SDK signer management requires no user credential.
- [ ] LayerHistory has exactly one head and advances only by exact CAS.
- [ ] CommitHistory has no ID or table.
- [ ] Branch base is exactly one LayerId or StackId and never moves.
- [ ] Branch creation atomically installs a canonical non-null anchor Commit without payload copy.
- [ ] `create_branch_from_commit` copies no Commit or payload.
- [ ] `pull_branch(source_branch_id, local_branch_id)` is non-destructive: fresh local IDs create safely, same-ID pulls fast-forward only, local-ahead never rewinds, and divergence leaves the local ref unchanged for explicit fresh-ID Pull plus Merge.
- [ ] `pull_commit_history(branch_id)` pins one central head, pulls only missing reachable Commit DAG/object data, and mutates no Branch ref.
- [ ] Branch merge enforces LayerHistory isolation, resolves Commit/Stack/Layer ancestry in core, advances only target, and preserves target base.
- [ ] Branch merge handles `UpToDate`, same-base fast-forward, divergent two-parent Commit, `MissingBaseData`, `AmbiguousMergeBase`, `NoCommonBase`, `Conflict`, and target `HeadMoved<CommitId>` without hidden transfer or mutation on rejection.
- [ ] Merge-base discovery uses `C <= 3` indexed recursive SQLite CTE statements with `UNION` dedup, at most two final candidates, bounded page cache/temp spill, and no Rust ancestry set.
- [ ] Shared three-way stages output outside live SQLite; every Conflict writes zero live rows and discards transient scratch.
- [ ] Conflict is one first bytewise-lexicographic `{ path, base, current, candidate }` tuple; traversal stops immediately and exposes no collection/truncation API.
- [ ] Sibling Branch IDs push independently; same Branch ID uses exact-head CAS.
- [ ] Queued `add_stack` calls evaluate once in order, preserve non-overlap, and reject real conflicts; an injected CAS loss rolls back and returns `HeadMoved` without retry.
- [ ] Pull history stops at known parents and never replays payload history.
- [ ] Create/pull history scope pairs reject wrong-history membership.
- [ ] Stack-bound Branch cannot bypass `add_stack` to call `add_layer`.
- [ ] Queued `add_layer` calls evaluate once in order; an injected CAS loss rolls back and returns `HeadMoved` without retry.
- [ ] `add_layer` rejects a target LayerHistory different from the source base history.
- [ ] Branch `merge` calls the one core merge-base resolver; Branch `merge`, `add_stack`, and `add_layer` call the same core three-way implementation.
- [ ] No-op `add_stack`/`add_layer` still atomically records `add_results`.
- [ ] Repeated addition validates result kind and explicit history before returning `UpToDate`.
- [ ] Push and Add stay separate; a subsequent corrected Add sends no payload already admitted by Push.
- [ ] Local Commit performs no existence pre-query; cross-store admission performs one batched existence query per ID page.
- [ ] Object existence uses one prepared 512-placeholder PK query and a fixed 64-byte missing bitmap.
- [ ] Write batches use deterministic greedy row+byte packing, including bounded oversized-object singleton admission.
- [ ] Object and immutable-fact batches both cap at 128 rows; widest fact statement uses 512 binds and is prepared at Store open.
- [ ] Pull/Push piggybacks payload/next announcement and ack/next bitmap, bounding one transfer to `P + 1` RTTs on a reused stream.
- [ ] Pull/Push ancestry traversal is paged/recursive, never N+1, and reuses one live stream without repeated handshake.
- [ ] Branch Commit/Merge/Add reports `S` individual indexed structural reads and `sum_i ceil(E_i/64)` payload batches inside `L`, with no network I/O inside a write transaction.
- [ ] Add performs no hidden network refetch and returns `MissingBaseData` for incomplete dependencies.
- [ ] `push_stack` only fast-forwards a verified read-only LayerStore copy; it never runs three-way or transfers authority.
- [ ] `push_stack` never rewinds the LayerStore copied Stack head.
- [ ] Repeating the same semantic input cannot duplicate a Branch update, Stack, Layer, or payload.
- [ ] Transfer visibility comes only from final exposed refs; valid unreachable immutable rows are reusable, while local Add candidate/AddResult/head are one rollback unit.
- [ ] Preexisting/raced/invalid/conflict/unreachable/sender object disposition follows the exact discard/retain matrix; Push never deletes sender rows.
- [ ] Frontier, batch, conflict, semantic-digest, and merge-base memory remain bounded with no unbounded Rust collection.
- [ ] BranchStore retains changed objects only and reads missing accepted refs through its parent.
- [ ] Admitted roots are closure-complete; negotiation prunes known subtrees and admits children first.
- [ ] Every store uses FastCDC v1 and canonical encoded-object payload IDs; raw `chunk_id` is never persisted or transferred.
- [ ] Normal object admission encodes/hashes once locally or authenticates once on remote receive; SQLite does not repeat either step.
- [ ] Equal logical file bytes never conflict solely because COW edit histories produced different FileState roots; the fallback compares streams with constant memory.
- [ ] Transfer and payload-extents use bounded SQLite batch adapters; existing logical structural walkers are counted honestly rather than mislabeled as batched.
- [ ] Store open rejects SQLite below 3.35; object pages use one multi-row `DO NOTHING RETURNING` statement with no per-object race query.
- [ ] `add_results(result_id)` supports provenance transfer without scans.
- [ ] SQLite table counts and columns match section 13 exactly.
- [ ] Every SQLite Store uses WAL/FULL, one owning process/handle per DB, Store endpoints for remote access, and the benchmark-frozen checkpoint policy.
- [ ] Each Store has one fair mutation pipeline; service time and queue wait are reported separately, and ten callers complete without starvation.
- [ ] No write transaction spans network or CPU-heavy preflight; writer-lock metrics distinguish visibility-only from folded bounded batches.
- [ ] Crate ownership matches section 14; there is no `layerfs-transfer`, `layerfs-sync`, duplicate three-way, or `layerfs-server`.
- [ ] `layerfs-storage-core::wire` stays byte-only and at or below 250 handwritten LOC.
- [ ] Cold implementation contains no compatibility aliases, role adapters, dual schema, migration shim, or fallback algorithm.
- [ ] V1 contains no Bloom filter, compression layer, connection pool, async runtime, transfer table, or alternate local fast path.
- [ ] No GC or rollback table exists.
