# Finalized-object handoff: ownership, backpressure and completion

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Tracking: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).
This document owns the proposed C1 -> C2 handoff contract. It records the five
handoff decisions and their source-backed simplification targets. Exact Rust APIs,
algorithm extensions and performance/resource qualification remain open.

Read with [canonical objects](canonical-objects.md),
[physical storage](object-storage.md), [whole-operation I/O](content-io.md) and
the [source/memory audit](content-io-memory-audit.md). The source baseline is the
audit's pinned v0.1.6 product tree. No implementation or measurement is claimed.
The [save/persistence design](admission-and-persistence.md) applies this handoff to
one mutation owner, bounded transactions, read visibility and single-attempt cleanup.

## 1. Contract at a glance

```text
C1: checked construction                  C2: bounded storage admission
  stable input / known edits
  unfinished boundaries
             |
             +-- move finalized object --> accept ownership
             |                              membership / encoding / packing
             |<-- acceptance or error ----  bounded DB writes
             |
  input and construction succeed
  return root + logical length/count summaries
                                            finish accepted output
                                            acknowledge storage completion

External workflow owns any history publication and completion policy.
```

There is one owned-object handoff and one storage-completion barrier. Ordinary
functions and existing ownership types are sufficient; no plugin registry,
buffer service, request/reply protocol or public state-machine framework is added.

Illustrative operation meanings, not finalized API signatures:

```text
accept(object, optional hints) -> bounded acceptance or error
construct(input, reader, output) -> constructed root + logical length/count summaries, or error
finish storage -> all accepted output accounted for, or failure/unknown outcome
```

Acceptance is not persistence. Construction completion is not history publication.
Individual file completion does not finish the shared many-file storage operation.

### Root IDs and returned fields

A root is the ObjectId that opens the constructed logical structure. For a small
file it can identify its whole-file canonical object; for a chunked file it
identifies the file-state object referring to the mapping. A filesystem root
identifies its canonical filesystem descriptor and inode-table references.

Use concrete result fields: a file constructor returns its root and logical byte
length. A tree builder may also need an entry count or level to build its parent
without rereading a node; keep those fields internal unless a caller needs them.
The earlier phrase logical summaries refers only to these existing values. It
does not introduce a universal summary type, table, report or extra scan.

C1/C2 define no checkpoint record or lifecycle. Root IDs are content addresses,
not checkpoint IDs; they carry no timestamp, parent-version link or publication
status. Storage completion and any later history/workspace policy remain separate.

## 2. Object fields and optional hints

Use ordinary fields on existing object/result types. Carry a value only when a
consumer needs it, and derive trivial values such as byte length from the buffer.
These fields require no separate abstraction or component.

```text
Finalized object                       Separate optional hints
  ObjectId                               bounded predecessor IDs / correspondence
  owned canonical bytes
  established semantic role
  direct logical references, if any
```

| Field or result | Decision |
| --- | --- |
| ObjectId | Carry the ID associated with the immutable canonical bytes |
| Canonical bytes | Move the existing allocation; no copy solely for the boundary |
| Canonical length | Derive from bytes.len(); charge bytes.capacity() for memory |
| Semantic role | Carry the checked role where C2 needs classification; it is not FULL/DELTA or a pack format |
| Direct logical references | Carry already-known direct child IDs in bounded ownership/view; leaves need no reference allocation; never retain the whole graph |
| File length, subtree count/height and ordering summaries | Keep in C1's frontier/result so parents can be built without rereading emitted nodes |
| Predecessor/correspondence hints | Optional, bounded and non-canonical; preserve useful candidate policy without adding a global search |
| Chosen delta base, chain depth, compression and physical location | C2 selects and validates these |

Reuse the existing [owned canonical value and private authenticated wrapper](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L139).
Exact field layout is open. A borrowed view cannot outlive its owner; asynchronous
handoff must move any required reference data or derive a view from bytes C2 owns.
Charge descriptor capacity as well as payload capacity. Do not allocate a wrapper
or reference Vec for every leaf merely to fit a generic representation.

```text
mapping -- required logical reference --> chunk
target  -- optional predecessor hint ---> candidate old object
DELTA   -- required physical dependency -> selected base
```

An optional hint does not make a stored delta's required base optional. Missing
eligible hints can select FULL normally; corruption or unexpected execution errors
do not trigger a fallback algorithm. C2 still proves dependency availability and
handles repeated IDs and collisions. Finality does not establish uniqueness.

CHUNK can also encode metadata-rope payloads. Preserve any required regular-file
provenance in explicit construction/admission context, as the existing
[file-payload context](../../../../../crates/layerfs-content/src/object/access.rs#L95)
requires. Do not infer it from the chunk marker or conflate it with diagnostic
flags. Candidate-search signatures belong to physical preparation; retain useful
already-computed data when it avoids a scan, without making it universal metadata.

Private checked constructors can carry the checked role and direct references without decoding their
own output again. External/serialized fields do not carry that ownership proof:
authenticate and check them at the receiving boundary. Structural checks stay
inside their file/tree codecs, with no separate role-validation component.

## 3. When output is final

Emit only when the following hold:

1. Exact bytes, role and direct references are established.
2. Later input cannot change the object's fields or canonical partition.
3. Successful completion of this construction retains the object in its result.
4. New children precede their parents in the emitted dependency order; existing
   children are covered by the storage dependency contract.

```text
finalized tree prefix | unfinished boundary siblings | incoming changes
          |
         emit

retain only unfinished boundaries and needed parent summaries in C1
```

The [complete-file builder](../../../../../crates/layerfs-content/src/file/rope/build.rs#L17)
already returns root and length while emitting chunks and completed nodes. Its
[finalized writer](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L758)
rejects [reads](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L910).
The [sorted tree updater](../../../../../crates/layerfs-content/src/tree/batch.rs#L1)
also retains unresolved siblings. Reuse those algorithms.

The [multi-edit overlay](../../../../../crates/layerfs-content/src/file/rope/edit.rs#L198)
still needs replacement by a proven bounded unfinished-boundary algorithm. Preserve
canonical roots, partitioning, range semantics and complexity before removing it.
The [file-content design](file-content.md#5-immutable-file-cow-and-final-construction)
specifies owned decoded boundaries with the existing split/concat rules. The current
overlay already streams payloads and bounds deferred structural storage; deleting
its maps/prune/redecode work is a separate cut from generic payload spill removal.
Finality must compose: a metadata value final relative to its own root is not
operation-final if its binding can later be overwritten or discarded. Supply
final ordered changes, or retain unresolved decisions before constructing output.
The [filesystem-tree input contract](filesystem-tree.md#checked-inputs-precede-finalized-output)
establishes membership per output/identity as validated bindings and inode changes become available; it
does not require a global final plan or the current Workspace's live graph.

A late EOF/length error can follow emitted objects. They remain individually
valid, but construction has no successful result. Failed-attempt ownership covers
their disposition; no root is reported as a successful stored operation.

## 4. Allocation ownership and release

```text
C1 owns canonical allocation
        |
        | move
        v
C2 batch owns canonical allocation
        |
   encoding / collision checks
        |
        v
C2 owns selected records / assembled pack
        |
        v
DB adapter borrows or owns the bounded write
        |
release each allocation after its last required consumer
```

| Allocation | Release rule |
| --- | --- |
| Emitted C1 bytes | C1 relinquishes ownership at handoff; retain IDs/summaries rather than payload copies |
| Duplicate incoming bytes | Drop after required exact comparison establishes reuse |
| New canonical bytes | Retain through the last encoding/collision/race check that needs them |
| Encoded output / pack | Retain until the selected DB adapter has consumed it |
| Optional comparison cache | Retain only within an explicit charged limit; do not claim immediate release if it remains live |

Memory release and transaction acknowledgement are different events. Codec input,
base, alternatives and pack output may coexist; moving a Vec only removes the
handoff copy. Allocation accounting follows ownership and counts each underlying
allocation once, including descriptor/index capacity and driver copies.

## 5. Batching and backpressure

Use direct calls where appropriate and preserve the existing bounded channel where
construction/storage overlap is useful. The existing [channel](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L397)
and [blocking flush](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L777)
already provide backpressure. Serializing all work is not assumed performance-neutral.

```text
file A output --+
file B output --+--> bounded handoff --> bounded admission --> encode/pack/DB
file C output --+          |
tree output ---+          +-- full: acceptance waits; production stops
```

Do not flush a pack or transaction per file. Limits span the whole operation,
including the producer's current object and a batch blocked during send. Reserve
the supported producer footprint before allocation. A queue slot becomes available
on dequeue, but its bytes remain charged to the consumer until actually released.

```text
live operation allocations include:
  input / unfinished tree state / current object
  producer partial or blocked batch + queued batches
  consumer pending objects / references / indexes
  authenticated bases + codecs + selected encodings / packs
  DB request/response buffers + bounded bookkeeping/reporting
```

Use the existing resource owners, count and byte limits. Preserve the single
construction producer and separate namespace-init exception. Concurrent operations
must fit declared shared allowances. No automatic disk overflow or unbounded queue.

Supported large objects need an explicit singleton path that retires conflicting
work before another maximal object is admitted. The audited writer/admission
capacity mismatch and singleton-pack spill are documented as IO13/IO14 in the
[audit](content-io-memory-audit.md#2-core-optimization-ledger). Accepting a larger
cutoff requires a working path through every stage, without silently increasing
budgets, changing to CDC or reducing existing supported workload coverage.

Consumer failure must wake blocked emission, stop further input work at a bounded
boundary, release buffers and join existing producers before the operation returns.
The [reference driver](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L474)
drains and joins after failure; preserve safe termination while making active
emission observe failure promptly. A cancellation flag alone cannot wake a blocked
send. Fallible storage cleanup remains explicit and its failures remain visible.

## 6. Completion and failure

| Event | Guarantee |
| --- | --- |
| Object accepted | C2 accepted ownership under its bounds; the object may be buffered, reused or written |
| Construction completed | Input and constructor checks passed; root and logical summaries are known and every final output was accepted |
| Storage completed | All accepted output is exact reuse or acknowledged storage, final tails/cohorts are complete, and dependencies are satisfied |
| History published | The external workflow installed its authoritative history reference under its own contract |

```text
C1                         C2                         database
 |-- finalized object ----->|                             |
 |<-- accepted -------------|                             |
 |                         |-- bounded writes/commit ---->|
 |                         |<-- acknowledgement ---------|
 |-- further output ------>|                             |
 |                         |                             |
 | EOF / length checks pass|                             |
 | return root + summary  | final tail may remain       |
 |                         |                             |
 |        finish storage ->|-- final writes/commit ------>|
 |                         |<-- acknowledgement ---------|
 |        storage done <---|                             |
```

The current [writer finish](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L772)
only flushes output. [FinishedOutputAdmission](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2638)
contains residual final work; [admission finish](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4002)
does not persist that work. Names in the replacement must distinguish those completion states.

Storage finish closes input, drains accepted work, resolves reuse, completes
remaining packs and atomic writes, closes outstanding bounded cohorts and waits
for a definitive backend outcome. It returns success with no queued work,
uninserted tail, outstanding transaction or unresolved outcome for this operation.
It does not hold one transaction for an arbitrarily large construction.

### Dependency closure and read visibility

Check dependencies incrementally, preserving the existing
[bounded closure check](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4071).
At acknowledged atomic-write completion, required dependencies must be in that
same atomic write, an earlier acknowledged batch, or existing protected storage.
Carrying direct references avoids reparsing; it does not replace this check.
Physical delta bases and metadata-pool dependencies have their own required checks.

C2 may resolve current pending IDs from its bounded admission buffers. It must not
query remote SQL for a child that has only been accepted into memory. If delayed
pack insertion changes visibility, satisfy dependencies within bounded pending
state or flush the necessary work explicitly. Preserve batching; no per-node flush
or whole-operation pending-object store. A later constructor needing emitted bytes
requires a declared read-visibility point/session contract; acceptance alone is
insufficient. Prefer returning already-known summaries to eliminate such reads.

### Final transaction composition

The reference [publication callback](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1338)
runs before the final cohort commit. Preserve required atomic final-object and
history-reference publication. Standalone C2 save can finish its object transaction
directly; an integrated owner can compose its authorized writes into that final
transaction before acknowledgement. The finish barrier does not mandate another
commit or put Branch/Commit concepts in C1/C2. Exact local/remote composition APIs
follow the [selected composition contract](admission-and-persistence.md#5-transactions-visibility-and-finish);
concrete signatures/backend qualification remain. These completion states need
not require separate transactions.

### Failure ownership and remote outcomes

Late source/consumer failure can follow earlier committed unpublished cohorts.
Clean only data owned by the failed admission attempt, preserving preexisting
objects and successful versions. Local [watermark cleanup](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2508)
depends on exclusive writer ownership; remote independent writers need equivalent
authority or exact attempt ownership. A lost acknowledgement fails the invocation
with an unknown persistence outcome; it does not prove the write rolled back.
Never resend, automatically poll or delete potentially committed data. A separately
requested authoritative inspection may determine what persisted; CAS IDs alone do
not make pack/locator writes idempotent. Apply the
[single-attempt rule](physical-encoding-and-packing.md#one-attempt-no-retries):
any condition requiring retry is failure, including SQL/SDK and stale-state retries.

Successful standalone storage completion retains its result and ends failed-
admission cleanup ownership. A later history failure does not silently undo that
completed save. Disposition of unused completed content belongs to retention
policy, not a new rollback mechanism. Successful published versions are never undone.

Storage completion means acknowledgement under the configured backend contract.
The reference [SQLite settings](../../../../../crates/layerfs-layerstack-store/src/schema.rs#L510)
do not establish a stronger crash-durability promise. Placement in a host, daemon,
container or cloud changes adapters, not the completion and ownership guarantees above; remote writes/reads stay
grouped and bounded rather than becoming one network call per object.

## 7. Concrete simplifications

These are cuts in different paths, not stages every reference object traverses:

```text
CURRENT ON STAGED PATHS                 TARGET ON PROVEN FINAL-ONLY PATHS
construct -> temporary object store    construct with unfinished boundaries
          -> graph selection                     |
          -> payload reread             move final objects to C2
          -> admission                            |
          -> encoding / SQL             encoding / SQL

REPEATED FACT DISCOVERY                 TARGET
emit -> read root -> decode summary     emit bytes + return known summary
encode -> parse references again        carry known direct references
authenticated bytes -> clone -> hash    move immutable authenticated ownership

OVERSIZED SINGLETON PACK                TARGET
canonical -> temporary file write      canonical -> bounded memory assembly
          -> full pack reread                     -> database
          -> database
```

The singleton replacement must reuse allocation where supported or fit explicitly
budgeted input/output overlap; the exact method remains to qualify. Preserve bytes,
collision operands and supported capacity. Removing spill does not authorize
larger hidden allocations. Keep CAS, CDC/COW, delta/compression, useful predecessor
hints, efficient packing, batching and required checks. Reuse the algorithms;
remove avoidable staging, copying and rediscovery around them.

## 8. Qualification and remaining decisions

The [I/O qualification contract](content-io.md#7-measurement-and-completion) owns
the matched comparator and repository measurement requirements. Improvements are
targets, not measured facts. Require existing-or-better speed, storage efficiency
and resource behavior under matched inputs/profile/cache/concurrency, with exact
canonical/readback correctness. One faster phase cannot excuse a complete-operation
regression; reduced temporary disk use is distinct from final database size.

Cover repeated IDs, cross-batch dependencies, many small files, fragmented edits,
size transitions, accepted configuration overrides, slow consumers, failure while
send is blocked, late EOF after early writes, final-commit failure and lost remote
acknowledgement. Include pack occupancy/index cost, canonical/pack coexistence,
driver allocations and relevant OS-cache attribution. Do not shrink workloads,
raise workers or loosen limits to pass. Existing useful overlap remains until a
matched replacement proves better.

Use existing coarse timer scopes. Construction wall time can include backpressure;
storage finish measures the final drain/commit wait, not all storage work performed
earlier. Overlapping spans are not additive CPU time. Do not record a node per object
for an unbounded workload or introduce a new telemetry runtime.

Open implementation decisions:

1. Exact fields/signatures and bounded reference ownership, preserving private
   checked-constructor guarantees across ordinary local calls.
2. Unified producer/admission/codec/pack capacity checks and no-spill singleton
   assembly, including actual simultaneous allocations.
3. [Multi-edit finality](file-content.md) and qualification of
   [compact filesystem reference ordering](filesystem-tree.md); the retained
   records and C2 physical index preclude a blanket zero-temporary-storage claim.
4. Pack lifetime/read visibility and local/remote atomic completion composition.
5. Source-matched equivalence and performance/resource evidence for the first
   complete-file construction -> real save -> authenticated readback extraction.

No new component or crate is required by this contract. The
[co-design checklist](content-storage-co-design.md#remaining-co-design-decisions)
tracks the remaining cross-component work.
