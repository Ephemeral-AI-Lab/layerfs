# LayerFS V2 Pull refinement

Status: **adopted into the binding `docs/v2/spec.md`**.

This document supersedes the V2 clauses that define Layer Pull as acquisition
of one isolated Layer, forbid Branch Pull, or combine remote acquisition with
Branch Fork. The consolidated specification, schema, SDK, CLI, source tree, and
terminal gates now live in `docs/v2/spec.md`; this document retains the detailed
Pull proof and must be read consistently with that file.

`MUST`, `MUST NOT`, `SHOULD`, and `MAY` are normative.

## 1. Final invariant

Pull always means acquisition **through one exact immutable boundary**:

```text
Layer Pull through Ln
    = the complete LayerStack prefix L1..Ln

Branch Pull through Cn
    = the complete Branch history visible through Cn
```

The boundary is never interpreted as one isolated Layer or Commit. Reference
and Replica acquire the same logical history. They differ only in the physical
residency guarantee:

```text
Reference
    complete logical history
    local-first reads
    exact-missing fallback to the bound LayerStackStore is allowed

Replica
    complete logical history
    every required immutable root is verified complete locally
    remote fallback is forbidden
```

Repeated Pulls remain missing-only and incremental. “Through” describes the
postcondition, not the number of facts or objects retransmitted.

## 2. Placement scope

Remote placement is receiver-local state. It is not a Store-wide mode and is
not part of the authoritative identity of a Layer, Branch, Commit, or canonical
object.

One BranchStore may simultaneously contain:

```text
LayerStack S1 through L12     Reference
LayerStack S2 through L8      Replica
remote Branch A through C17   Replica
remote Branch D through C4    Reference
local Branch B                locally owned; no remote placement mode
local Branch E                locally owned; no remote placement mode
```

Each current remote scope has exactly one serving mode:

```text
LayerStackPrefixScope = (LayerStackId, through LayerId)
BranchHistoryScope    = (BranchId, through CommitId)

ServingMode = Reference | Replica
```

There is no per-object, per-file, per-chunk, per-Commit, or whole-BranchStore
serving mode. Root completeness remains separately recorded physical truth so
objects and verified roots can be shared across scopes.

## 3. Exact-boundary admission

Every Pull pins its boundary before transfer begins. If a CLI or frontend asks
for the authority's current head, the plan MUST resolve that moving head to an
exact `LayerId` or `CommitId` before execution:

```text
request:             pull current Branch A as Replica
resolved at plan:    A/C10
authority advances:  A/C11
operation result:    still exactly through A/C10
```

A Pull MUST NOT chase a head that advances during enumeration or transfer.
The newer boundary requires another explicit Pull.

The authority StoreId, LayerStackId or BranchId, selected boundary, and
immutable parent chain MUST be validated before the receiver publishes the new
visible boundary.

## 4. Layer Pull

### 4.1 Logical extent

Given:

```text
LayerStack S

L1 <- L2 <- L3 <- L4 <- L5
```

both:

```text
pull S through L4 as Reference
pull S through L4 as Replica
```

acquire the exact logical prefix:

```text
L1, L2, L3, L4
```

They MUST NOT acquire only L4 and MUST NOT acquire L5. The imported facts
include every Layer's exact parent relationship and source Branch/Commit
provenance.

The receiver MUST reject:

- an ID collision with different immutable fields as `Integrity`;
- a selected Layer outside the expected LayerStack;
- a missing or cyclic parent chain;
- two children for one parent in a supposedly linear LayerStack;
- a genesis other than the authority's exact genesis;
- a fact from a LayerStackStore other than the BranchStore's immutable parent.

There is no Layer merge, prefix reconciliation, or automatic repair.

### 4.2 Reference Layer Pull

Reference imports the complete Layer fact prefix and publishes a Reference
serving mode for the resulting current prefix. It does not require transfer or
verification of any object closure.

Reads are local-first:

```text
requested ObjectId
    -> exact local object
    -> only on exact MissingObject, bound LayerStackStore
```

Corruption, invalid canonical bytes, wrong object kind, hash mismatch, I/O
failure, and any other local error MUST NOT trigger fallback.

### 4.3 Replica Layer Pull

Replica imports the same facts and makes every Layer snapshot in the prefix
locally complete. Its required object set is:

```text
closure(L1.root)
union closure(L2.root)
...
union closure(Ln.root)
```

Copying only `closure(Ln.root)` is not a prefix Replica. A later Layer root is
a complete filesystem snapshot, not an archive of objects deleted from earlier
Layer snapshots.

For example:

```text
L1 contains old.dat
L2 still contains old.dat
L3 deletes old.dat
L4 contains current.dat
```

`L4.root` need not reach the objects for `old.dat`. A successful Replica
through L4 MUST nevertheless support offline read, Diff, FUSE projection,
materialization, and Fork from L1, L2, L3, or L4.

One completeness receipt is required for each distinct Layer root. Equal roots
and shared descendants remain stored once.

### 4.4 Incremental extension

If the receiver already has L1..L2, pulling through L5 validates the existing
prefix and transfers only missing facts L3..L5. Replica additionally transfers
only receiver-missing objects from the union of required roots.

The successful postcondition remains the full prefix L1..L5.

## 5. Branch Pull

Branch Pull is restored as an explicit remote-acquisition operation. It is not
an alias for Fork and it never creates a new BranchId.

```text
Pull Branch
    acquire one exact remote Branch snapshot under the same BranchId
    store it as a read-only remote placement

Fork Branch
    create one new locally owned BranchId from an already available boundary
```

### 5.1 Logical extent

Given:

```text
remote Branch A

C1 <- C2 <- C3 <- C4 <- C5
```

both:

```text
pull A through C4 as Reference
pull A through C4 as Replica
```

acquire the complete immutable history visible through C4. They MUST NOT
import only the C4 fact and MUST NOT import C5.

The logical acquisition includes:

- the exact remote Branch identity and immutable fork origin;
- every Commit fact in the parent ancestry visible through the selected
  Commit;
- the Branch-origin facts required to preserve inherited fork boundaries;
- every required base Layer fact;
- the complete LayerStack prefix through every required base Layer;
- no sibling Branch and no Commit after the selected boundary.

Pulled remote Branches are read-only inside BranchStore. Workspace Commit and
Push MUST reject a pulled remote Branch as a publication target.

### 5.2 Inherited ancestry and ownership boundaries

Suppose:

```text
Parent P

P1 <- P2 <- P3
            |
            `-- Branch A at P3
                A1 <- A2 <- A3
```

A full Pull of A through A3 makes the visible ancestry available:

```text
P1 <- P2 <- P3 <- A1 <- A2 <- A3
```

It MUST preserve the semantic boundary `A forked from P/P3`. Acquisition does
not flatten authorship or make the inherited prefix part of A's locally owned
lane.

The implementation therefore MUST expose two distinct traversals from one
shared owner:

```text
history ancestry
    all visible Commit ancestry through an exact boundary
    used by Pull, read, historical Diff, FUSE, materialization, and frontend

owned lane
    Commits authored after a Branch's immutable fork boundary
    used by Push, publication, and authority CAS
```

These are two stop conditions over the immutable parent graph, not two stored
Commit histories or duplicated algorithms.

### 5.3 Reference Branch Pull

Reference imports the complete Branch, Commit, origin, and required Layer
facts through the selected boundary. It copies no required object closure.

Every selected historical root remains readable through the immutable parent
route. The selected remote Branch head is pinned locally and does not follow a
later authority head until another explicit Pull.

### 5.4 Replica Branch Pull

Replica imports the same logical facts and makes every selected historical
snapshot locally complete. Its required object set is:

```text
union of closure(commit.root)
    for every Commit in the visible history through Cn

union of closure(layer.root)
    for every Layer in every required LayerStack prefix
```

Replica success MUST allow the authority to become unavailable while the
receiver reads, diffs, mounts, materializes, or forks from any imported Commit
or required Layer through the selected boundary.

It is not sufficient to copy only the selected head Commit root and its current
base Layer root.

### 5.5 Incremental Branch advance

If the local remote placement of A is through C3 and an explicit Pull selects
C6, the authority must prove that C3 is an ancestor of C6 in the same Branch
history. The receiver transfers only C4..C6 facts and missing objects, but the
postcondition is the complete history through C6.

If the selected boundary is incomparable with the current local boundary,
Pull returns `HeadMoved` or `Integrity`. It never merges, rebases, rewrites, or
silently creates a new Branch.

Existing local Branches forked from A/C3 remain pinned to A/C3 after A is later
pulled through C6.

## 6. Older, equal, and newer boundaries

For both LayerStack-prefix and Branch-history placements:

```text
requested == current
    -> optionally change serving mode at the exact current boundary
    -> otherwise UpToDate

current is an ancestor of requested
    -> validate and acquire only the missing suffix
    -> publish the requested boundary and requested mode last

requested is an ancestor of current
    -> AlreadyContained
    -> do not move the visible boundary backward
    -> do not use the older request to change the current scope's mode

requested and current are incomparable
    -> HeadMoved or Integrity
```

This prevents a request for an older Replica boundary from creating an
ambiguous current scope with a Replica prefix and Reference suffix.

## 7. One serving mode, shared physical residency

Each current remote scope exposes exactly one serving mode. Physical object
residency is separately shared and may be stronger than the current guarantee.

```text
Reference mode
    local exact object hit -> use local
    exact local MissingObject -> use bound authority

Replica mode
    local exact object hit -> use local
    exact local MissingObject -> Integrity
```

Reference is not remote-only. It permits parent-backed reads; it does not
require needless network reads for valid local objects.

### 7.1 Mode transitions

| Current | Requested | Required result |
|---|---|---|
| absent | Reference | Acquire complete facts; publish Reference last. |
| absent | Replica | Acquire facts and missing object union; verify all roots; publish Replica last. |
| Reference | Reference | Extend facts if needed; remain Reference. |
| Reference | Replica | Fill and verify every required root; publish Replica last. |
| Replica | Replica | Extend and complete only the missing suffix; remain Replica. |
| Replica | Reference | Publish Reference policy at the current/new boundary; do not delete objects implicitly. |

A `SnapshotReader` pins the scope boundary, serving mode, and relevant root
state once. A concurrent mode or boundary change MUST NOT tear an already-open
snapshot.

### 7.2 Replica to Reference is not deletion

Replica objects do not belong exclusively to the Replica scope. They may be
shared with:

- another Replica LayerStack prefix;
- another Replica Branch history;
- a locally owned Branch Commit;
- an active or retained Workspace delta;
- another root in the same history;
- an in-flight FUSE, materialization, Diff, Push, or transfer reader.

Changing a scope from Replica to Reference is therefore a serving-policy
change only. Existing objects and truthful completeness receipts remain
available for deduplicated local-first reads.

Pull MUST NOT hide destructive eviction. The current V2 phase has no GC. A
future explicit GC/eviction owner must mark all locally required roots, drain
pinned readers, sweep only unreachable objects, and update completeness state
before it may reclaim bytes. Per-object reference counts or scope-owned object
copies MUST NOT be introduced merely to simulate cleanup.

## 8. Pull, Fork, Push, and Add

The final verb grammar is:

```text
Pull Layer
    LayerStackStore -> BranchStore
    acquire one complete LayerStack prefix through an exact Layer

Pull Branch
    LayerStackStore -> BranchStore
    acquire one read-only remote Branch history through an exact Commit

Fork Branch
    BranchStore-local
    create a new locally owned BranchId from an already available Layer or
    Branch/Commit boundary

Commit Workspace
    create one new Commit on a locally owned target Branch

Push Branch
    BranchStore -> LayerStackStore
    publish only the locally owned Branch suffix

Add Layer
    accept a pushed Branch head as the next LayerStack Layer
```

Fork MUST perform no hidden Pull and accept no placement argument. Every Fork
generates a new BranchId and copies no canonical object merely to establish
the fork.

For example:

```text
pull remote A through C4 as Replica
fork local B from A/C4
commit B1, B2
push B
```

Push transfers B's immutable origin `A/C4`, B1, B2, and receiver-missing
objects required by that local suffix. It MUST NOT upload the pulled A history
back to the authority that already owns it.

Push never performs Pull, Fork, Add, or reconciliation. Pull never creates a
local writable Branch. Add never pushes implicitly.

## 9. Public semantic surface

The SDK must make the through boundary explicit in argument names:

```rust
pull_layer(
    through_layer_id: LayerId,
    placement: RemotePlacement,
) -> PullLayerResult

pull_branch(
    branch_id: BranchId,
    through_commit_id: CommitId,
    placement: RemotePlacement,
) -> PullBranchResult

fork_branch(name: EntityName, source: LocalForkSource) -> BranchId

enum LocalForkSource {
    Layer {
        layer_id: LayerId,
    },
    Branch {
        branch_id: BranchId,
        commit_id: CommitId,
    },
}
```

`LocalForkSource::Branch` contains no `remote_placement`. Remote acquisition
belongs only to Pull.

The CLI grammar is:

```text
layerfs layerstack pull --through <layer-id> --reference
layerfs layerstack pull --through <layer-id> --replica

layerfs branch pull <branch-id> --through <commit-id> --reference
layerfs branch pull <branch-id> --through <commit-id> --replica

layerfs branch fork --name <name> --layer <layer-id>
layerfs branch fork --name <name> --branch <branch-id> --commit <commit-id>

layerfs branch push <local-branch-id>
```

Exactly one of `--reference` and `--replica` is required for Pull and forbidden
for Fork. Plans, completion, receipts, JSON, and frontend snapshots must call
the ID the `through` boundary so callers cannot infer single-point semantics.

Pull results must distinguish at least:

```text
Created
Advanced
ModeChanged
UpToDate
AlreadyContained
HeadMoved
```

Transfer counts and timing belong to the operation receipt/events rather than
additional semantic methods.

## 10. Durable representation constraints

The BranchStore must durably distinguish:

- imported read-only remote Branch placements from locally owned Branches;
- the current exact LayerStack-prefix boundary and its one serving mode;
- the current exact remote Branch boundary and its one serving mode;
- immutable semantic Branch facts from receiver-local placement state;
- exact-root physical completeness from scope serving policy.

Receiver-local Reference/Replica state MUST NOT be included in an authoritative
Branch fact, Commit identity, Layer identity, object identity, or Push origin
equality. The former V1 `branches.remote_placement` provenance model is
superseded and must not be repurposed ambiguously.

The reconciled DDL uses the exact `layer_stack_scopes` and `branch_scopes` rows
frozen in `spec.md`. It MUST NOT add:

```text
one object copy per scope
object ownership rows
per-scope object closure tables
durable transfer journals
workspace/output/operation history
automatic GC state
```

`complete_roots` remains shared physical verification state. A root may remain
complete after its current scope changes to Reference, and multiple scopes may
reuse one receipt.

## 11. Transfer and transaction rules

Pull uses the existing missing-only equations independently for facts and
objects:

```text
announced = preexisting + missing
sent      = missing
missing   = inserted + raced_existing
```

Layer and Commit history enumeration, network I/O, CDC, hashing, signature or
canonical-object verification, dependency traversal, Diff, and merge-like
reconciliation MUST NOT occur inside a SQLite write transaction.

Required admission order is:

```text
pin and validate exact boundary
-> enumerate bounded fact/history pages
-> admit immutable facts in dependency order
-> enumerate and transfer receiver-missing canonical objects
-> verify every required Replica root locally
-> insert truthful root completeness receipts
-> publish the current scope boundary and serving mode last
```

Facts and objects may be admitted in bounded idempotent batches. An interrupted
operation may leave reusable immutable facts, unreachable immutable objects,
or receipts for roots that individually completed verification. It MUST NOT
leave the requested boundary visible with a false Replica guarantee.

Object traversal must use known-root pruning, fixed-page membership, bounded
object batches, and one canonical object encoding. Pull never rechunks,
re-encodes, or remints stored content.

## 12. Read and product behavior

Read, historical Diff, FUSE projection, materialization, Workspace base reads,
and frontend snapshots consume the same pinned history/snapshot reader. They do
not independently infer placement from SQL rows or retry on corruption.

No operation may perform a hidden Pull:

```text
Fork from unavailable boundary          -> NotFound / NotPulled
Workspace from unavailable boundary     -> NotFound / NotPulled
Diff against unavailable history        -> NotFound / NotPulled
FUSE/materialize unavailable snapshot   -> NotFound / NotPulled
```

Replica must continue working with the authority disconnected. Reference must
report the authority error when an exact local miss requires fallback; it must
not estimate, materialize a whole database, or silently downgrade correctness.

The frontend must display serving mode separately from physical coverage:

```text
Mode:                 Reference
Through:              C12
History facts:        C1..C12
Locally complete:     C1..C10
Parent-backed:        C11..C12
```

That is one Reference serving mode with retained deduplicated local data, not
mixed Reference/Replica modes.

## 13. Required focused proof

The refinement is not implemented until current-source tests prove:

1. Reference Layer Pull through Ln imports every exact Layer fact L1..Ln and
   no later Layer.
2. Replica Layer Pull through Ln remains fully offline-readable for every
   Layer L1..Ln, including objects deleted from later Layer roots.
3. Repeated Layer Pull transfers only missing suffix facts and missing
   deduplicated objects.
4. Reference Branch Pull imports complete visible Commit ancestry and required
   Layer prefixes through Cn.
5. Replica Branch Pull remains fully offline-readable, diffable, FUSE-ready,
   and materializable at every imported Commit through Cn.
6. A remote Branch pulled at Cn remains pinned when the authority advances.
7. Pulling a descendant boundary advances exactly; pulling an older boundary
   returns `AlreadyContained`; an incomparable boundary fails without merge.
8. A local Fork from a pulled Branch creates a fresh BranchId, performs zero
   remote calls, and copies zero canonical objects.
9. Push of that local Fork transfers only the locally owned suffix and does
   not resend the pulled history.
10. Reference-to-Replica transfers only missing objects and publishes Replica
    mode after full verification.
11. Replica-to-Reference changes one serving policy and deletes no shared
    object or truthful completeness receipt.
12. Concurrent readers pin a consistent boundary and mode while a Pull
    advances or changes the current placement.
13. Interrupted Replica acquisition never exposes a false current Replica
    boundary and retry reuses admitted facts and objects.
14. Exact missing-object fallback occurs only in Reference mode and only for
    exact `MissingObject`; corruption never falls through.
15. SQL traces prove that enumeration, network, hashing, and verification run
    outside write transactions and that the current boundary/mode is visible
    last.
16. Inventory and transfer receipts satisfy missing-only equations for every
    fact kind and canonical objects.

## 14. Required reconciliation

The implementation owner must reconcile, in dependency order:

1. `docs/v2/spec.md` definitions, deleted-model list, placement state machine,
   schemas, operation contracts, CLI grammar, source tree, stages, and terminal
   gates;
2. `docs/v2/implementation-handoff-prompt.md`;
3. `docs/v2/sdk-cli-operation-families.md`;
4. storage records, endpoint traits, schemas, migrations, queries, and transfer
   receipts;
5. Layer Pull from one-record/root acquisition to prefix-through acquisition;
6. explicit read-only Branch Pull with complete-history acquisition;
7. Fork removal of remote placement and hidden acquisition;
8. distinct full-history and owned-lane traversal stop conditions;
9. Push proof that imported ancestry is never retransmitted as local work;
10. SDK, CLI, completion, plans, frontend snapshots, monitor receipts, tests,
    and evidence.

Old single-point Pull behavior, remote acquisition inside Fork, and prose that
still forbids Branch Pull are structural violations after this refinement.
