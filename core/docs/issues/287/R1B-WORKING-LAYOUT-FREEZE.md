# R1b scoped graph working layout and retained acknowledgement freeze

> Prospective implementation contract, 2026-10-01. Parent source inspected:
> `53b6bf741a5693f4d00ec98b914ce34645ee9ab3`. No build or allocation proof
> has run for this delivery. Root coordinates the owning checks and publication.

SC-03, SC-05, SC-07 and SC-08 require actual bounded graph state and retained
consumer ownership. This delivery replaces the unused worst-case inline graph
layout in every legacy Resource. It preserves LFCS1-4 SQL/private/canonical
grammars, operation/source associations, native budgets and graph algorithms.

Ownership: Content graph memory.rs, ack.rs and mod.rs; Storage construction_state
graph_layout.rs, graph_state.rs, graph_build.rs, graph_solve.rs, graph_pop.rs,
graph_retire.rs, graph_session.rs, graph_lifecycle.rs, session.rs, status.rs and
the small authority/mod declaration changes; external construction_layout tests;
the storage architecture document. Other Store, Server, Bridge and namespace
implementation belongs to the integrated owner and other agents.

Each selected graph owner receives one fixed 65,536-byte logical working class.
This is a scoped new admission cap inside the proposed C1 working allowance;
aggregate first-party/allocator/native/physical fit remains unproved. It is not
another native S reservation, not the encoded 64KiB page limit, and not an
increase of the 16MiB/default or explicitly selected native scratch class.

The admitted concurrent maximum is the compiled expression:

```
GraphMemory control allocation
+ size_of::<Graph>()
+ size_of::<GraphAttemptData>()
+ max(size_of::<GraphBuildAck>() + 128 * size_of::<GraphNodeChange>(),
      size_of::<GraphMutationAck>() + 128 * size_of::<GraphNodeChange>(),
      size_of::<GraphPopAck>() + 128 * size_of::<GraphNode>())
<= 65,536
```

The build enforces this inequality, including the newly owned leases and padding.
The earlier six arrays alone derive 41,472 bytes from recorded compiled field
sizes; those are not full attempt or owner sizes. The owning compiled telemetry
and real System allocator observation must publish actual sizes before a PASS.
Resource/ScratchSession inline sizes are reported separately. Caller input pages,
SQL native heap, formatting/error strings and all other C1/C2 owners are outside
this scoped class and cannot be described as covered by this proof.

Resource admission reserves the graph control and prospective boxed Graph layout
before file creation/reservation/open/schema effects. Legacy plans allocate no
graph class. One Graph has no inline attempt arrays; only a real pending attempt
reserves its exact compiled size before Box allocation and before mutation.
Each small owner wrapper stores its actual Box before the lease, so field
destruction frees the allocation before returning credit. Known acknowledgement
releases that attempt lease; Unknown retains its exact
old/proposed rows, solver, seals and credit without retry or guessed refund.

Before allocating acknowledgement vectors or issuing SQL, C2 reserves the full
acknowledgement layout plus prospective actual vector capacities. The completed
acknowledgement owns a non-cloneable lease. Holding several acknowledgements
therefore reduces remaining admission; exhaustion explicitly refuses before the
next mutation. Moving the result or releasing its session does not refund held
result bytes. Drop of the actual last result returns its credit. Ordinary C1
compatibility constructors retain their original independently supplied scope;
they do not claim this C2 admission. No constructor changes wire/canonical bytes.

Actual owner status reports compiled Graph/GraphAttempt/Resource/ScratchSession
and ACK element/layout sizes, working class and current reserved bytes. This is
production ownership telemetry, not a test-only private-type accessor. Requested
layout accounting excludes allocator metadata, process RSS and OS pages.

Exit proofs: legacy compiled inline layout excludes attempt arrays; one maximum
128-target transition and simultaneous held results fit/refuse as declared; real
System allocation requests stay charged through returned results; held-result
exhaustion causes no SQL/native row effects; dropping results restores capacity;
Unknown preserves attempt credit; legacy LFCS1-4 semantic/oracle proofs remain.
No Cargo/check/measurement runs occur independently of root's coordinated freeze.

## Bounded roots retirement/reset design, not implemented by this delivery

Roots completion currently only marks logical release. A later agreed interface
must retain terminal exact seal/count/bytes/last key and delete at most 128 rows
and header-inclusive 64KiB in each acknowledged transaction, including ordinal
projection. Proposed continuation advances only after COMMIT plus observation;
Unknown retains old/proposed progress, selected owner and credit. Terminal reset
checks all implemented tables/partial indexes empty and autocommit, then resets
fixed owner rows in one acknowledged transaction. No population DELETE, schema
recreation, VACUUM, reopen/adoption, retry or cold fallback is permitted.

Pooling requires a continuously owned stable pathname/descriptor/native identity,
fresh StateSelection token and source/subject/header binding per checkout, stale
scope/reader exclusion, matching provider/profile/schema/budget, and explicit
return-to-idle distinct from release close/unlink/refund. Active/resetting/idle/
retained resources keep their captured class charged; mixed classes sum captured
bytes. Exact root reset/retirement and pooling edits await integrated ownership.


## Closed graph attempt allocation correction (prospective source freeze)

Root's actual7 compilation proves the earlier six128 arrays plus new Facts/hot/
raw producer/ACK overlap exceeds64KiB. No budget/window/caller input is changed.
GraphAttemptData keeps its exact scalar before/proposed metadata but arrays become
pre-reserved Vecs with CLOSED kind capacities: Seeds node-pairs128; Root/Expanded node-pairs1; BeginScc no row window; Append node-pairs64 and edge-pairs63 (combined change<=128);
Mutation node-pairs128 plus selected-node128/selected-edge128/code128; Pop node-
pairs128 plus selected root1; Retire node/edge-pairs128 each but combined selected
rows<=128; all other kinds zero windows. Actual old/new/projection/solver metadata
and full Unknown custody remain unchanged. Indexed readers/writers keep the same
bounded selected row semantics and128 port ceiling.

Before any Vec/Box allocation, one GraphMemory reserve covers compiled metadata
payload, small wrapper and exact closed-kind vector Layout capacities. Each
try_reserve_exact result must match its declared capacity before initialization;
no geometric growth, fallback, hidden larger cap or lazy uncharged allocation.
Metadata/Vec owner destruction precedes credit return. All unused mode windows
are zero-capacity/no allocations. Product layout reports metadata payload plus
perkind requested allocation and maximum; aggregate includes actual aliases/
Facts/hot/raw producer/ACK/page/fold maxima as before. Root's covering compilation
and external System observations establish numeric sizes; no worker compiles or
claims estimates as proof. Existing old layout receipts retain their source.

The old external assumption that exactly one maximum128 ACK can be held before
refusal is replaced by actual shared64KiB exhaustion: hold as many full128 ACKs
as their real combined prospective attempt+result fits, then prove the next
refusal precedes SQL mutation and that independent data credits return only on
actual last-owner drop. This tests the same finite class without intentionally
retaining allocations unused by the current closed operation.

Mutation runtime refinement before source: new_with_items captures validated
logical request length and summed affected() node targets, each<=128, before any
allocation. Selected node/edge/code windows use logical length; old/new node
windows use exact affected count (Descend2, ordinary mutation1, LeaveRoot0).
Maximum compiled class still uses128 for each applicable window; ports/encoded
checks/duplicate target refusals and Unknown exact actual data do not change.
