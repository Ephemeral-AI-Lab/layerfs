# LayerFS product-readiness contract

Status: **target contract, not a claim that current HEAD is product-ready**.

A disposition binds one exact source tree, Store schema, executable/image,
platform, CPU architecture, integrity mode, presentation, selected
WorkingStore/DurableStore topology, and workload envelope. Passing one
presentation envelope does not qualify another.

## 1. Product topology

LayerFS has one Store topology:

```text
one disk-backed WorkingStore per participating host/security domain
    +
one distinct DurableStore as the shared system of record
```

The Stores are physically distinct SQLite databases using the single schema
and low-level mechanisms owned by `layerfs-storage`. Crate/API boundaries fix
Working versus Durable policy; there is no runtime selector and no
standalone/one-Store product mode.
`layerfs-working-store` provides execution speed and host recovery;
`layerfs-durable-store` independently authenticates and provides system
durability/cross-host recovery. DurableStore may be deployed on the same host,
LAN, or remote service; network distance is not part of its semantics. Direct,
FUSE, and APFS presentations are
qualified separately without changing this topology.

## 2. Decision rule

```text
PRODUCT_READY(envelope, artifact)
    = every applicable MUST gate PASS
    + no unresolved P0/P1 defect
    + exact source/artifact/Store-format manifest
    + final independent evidence recomputation PASS
    + public limitations equal the measured envelope
```

Evidence labels:

| Label | Meaning |
|---|---|
| `ProvenForBoundArtifact` | Raw evidence binds the exact source/artifact and envelope |
| `ImplementedNotQualified` | Product code exists, but release evidence is incomplete |
| `TargetNotImplemented` | Normative architecture is not yet implemented |
| `NotApplicable` | Outside the selected envelope for a stated reason |
| `Unavailable` | Required observation cannot be obtained; never reported as zero |

## 3. Architecture and terminology gates

| ID | MUST requirement |
|---|---|
| A01 | LayerStack versions are immutable `Layer`s; Branch versions are immutable `OperationVersion`s |
| A02 | The only commit action is `OperationCommit`, which binds one `OperationDelta`, creates one `OperationVersion`, and CASes the Branch head |
| A03 | The only merge actions are `ChildBranchMerge` to the immediate parent Branch head and `LayerStackMerge` from any Branch depth to that Branch's inherited originating LayerStack head |
| A04 | `LayerBranchFork` starts from a retained Layer; `ChildBranchFork` starts from a completed parent `OperationRecordRef` |
| A05 | Every child inherits its parent's originating LayerStack; origin is durable and rejects a Branch merge into a non-parent Branch or a LayerStack merge into any unrelated LayerStack |
| A06 | `OperationDelta`, `BranchDelta`, and `LayerDelta` have distinct semantic records while referencing one versioned authenticated root-transition representation; RootIds remain filesystem authority |
| A07 | Public architecture does not introduce a competing generic history graph, extra version boundary, or orchestration-policy model |
| A08 | Current crate locations and target crate ownership are both stated; current source cannot silently redefine the target boundary |
| A09 | `layerfs-core::logical` is the only portable exact-version read/mutation/diff/three-root-merge/candidate owner, generic over `ObjectRead`/`ObjectStore` and free of SQLite, platform, workspace, and authority policy |
| A10 | Final ownership is `layerfs-core`, `layerfs-storage`, `layerfs-working-store`, `layerfs-durable-store`, `layerfs-sync`, `layerfs-workspace`, `layerfs-mount`, `layerfs-materialization`, thin `layerfs-sdk`, and `layerfs-service`; current `layerfs-engine`/`layerfs-vfs` names remain evidence only during one-way extraction, and no temporary `layerfs-fs` crate is created |
| A11 | SDK snapshot reads (`stat`, `list`, `read_range`, stream, `readlink`) pin an immutable VersionRef already verified in WorkingStore storage and never create an Operation, move a head, or trigger sync; an absent durable version requires explicit Fetch first |
| A12 | Migration extracts Workspace/FUSE/APFS ownership first, then moves the remaining portable VFS semantics directly into `layerfs-core::logical` and deletes the old VFS implementation after conformance |
| A13 | Search, rollout, and MCTS policy remain external consumers of Branch/version primitives and never enter LayerFS schema, merge law, or correctness claims |

## 4. Canonical correctness and deduplication gates

| ID | MUST requirement |
|---|---|
| C01 | Equal canonical bytes produce the same domain-separated `ObjectId`; host paths, native inode numbers, runtime IDs, database row IDs, and network location never enter identity |
| C02 | FastCDC uses the frozen 8/16/32 KiB profile for new content construction unless a new canonical format is explicitly versioned |
| C03 | Extent, namespace, inode, and metadata trees validate roles, ordering, bounds, occupancy, measures, and complete root closure |
| C04 | Persistent mutation path-copies changed spines and reuses exact unchanged objects/subtrees |
| C05 | Every fetched, new, and incumbent object is authenticated at its trust boundary; unequal incumbent bytes are an integrity failure, never replacement |
| C06 | Every retained Layer and OperationVersion root is directly readable without replaying historical deltas |
| C07 | Deduplication is exact authenticated ObjectId reuse, with created/reused/hashed/read byte counters |
| C08 | Old roots remain exact after divergent edits, both merge forms, rollback, restart, synchronization, and compaction |

## 5. Operation, Branch, and Layer gates

| ID | MUST requirement |
|---|---|
| V01 | `OperationRecordRef` binds parent Branch, Operation, OperationVersion, and RootId |
| V02 | A WorkingRecorded OperationCommit, including an intentionally empty delta, creates exactly one host-recoverable OperationVersion and Branch transition; explicit Push makes the same identity DurablyAccepted |
| V03 | A discarded or failed-before-publication Operation creates no visible OperationVersion |
| V04 | OperationCommit uses exact expected Branch head; stale head returns Conflict and preserves the candidate |
| V05 | ChildBranchMerge uses exact expected parent head; stale parent preserves the child result and returns Conflict |
| V06 | LayerStackMerge from a top-level or any-depth nested Branch uses its inherited originating LayerStack and exact expected stack head; stale stack preserves the source Branch and candidate Layer and returns Conflict |
| V07 | No action performs hidden retry, implicit rebase, or implicit merge |
| V08 | BranchRollback and LayerStackRollback preflight all affected leases, move the head, and logically release the unused suffix only when safe |
| V09 | Physical CAS reclamation occurs only through later authenticated reachability compaction |
| V10 | Push of Branch/Operation history may create a durable Branch or advance its exact durable head without LayerStackMerge; LayerStack state changes only through a separately requested LayerStackMerge |
| V11 | Successful and failed ChildBranchMerge/LayerStackMerge preserve the source Branch as active; only transient merge leases end, while its immutable fork-origin lease remains until explicit Branch drop so later Operations and repeated merges stay valid |

## 6. OperationWorkspace and concurrency gates

| ID | MUST requirement |
|---|---|
| W01 | Every Operation pins one exact base and owns one private OperationWorkspace; immutable snapshot reads are not Operations |
| W02 | WorkingStore persists OperationId, exact BranchHead/base VersionRef, base-version lease, and workspace recovery record before a driver becomes usable |
| W03 | `layerfs-workspace` is the universal isolation/quiescence/finalization contract for direct logical, mount/FUSE, and materialization/APFS; there is no runtime WorkspaceMode |
| W04 | Direct, mount, and materialization drivers keep distinct dirty representations while `layerfs-core::logical` alone constructs portable candidates |
| W05 | Sibling operations never observe or mutate each other's dirty state; multiple operations may begin from one Branch head and exact CAS chooses the winner |
| W06 | Arbitrary Bash/process work is committed only after descendant writers, mapped writers, and presentation-specific mutation sources are quiescent |
| W07 | One Operation ends exactly once as accepted, conflicted, discarded, failed, or indeterminate; conflict preserves enough host-recoverable candidate state for explicit resolution |
| W08 | Workspace directories default to `<working-root>/workspaces/<operation-id>-<nonce>/{owner,recovery,view,spool}` adjacent to working SQLite, are `0700`, host-local, marker-validated without following links, safely cleaned by exact identity, and never synchronized |
| W09 | Operation, mount, materialization, nested-Branch origin, merge, and sync ownership use explicit leases with bounded cleanup and terminal residue accounting |
| W10 | Child Branch nesting is recursive; every ChildBranchFork pins an immediate-parent OperationRecordRef, every ChildBranchMerge targets only that immediate parent, and every descendant inherits the originating LayerStack and may independently LayerStackMerge directly to it with complete inherited state; origin leases block affected rollback |
| W11 | The SDK exposes no agent-tool taxonomy and no Bash/npm/shell execution API; generic process/descriptor/mapping tracking exists only to prove quiescence |
| W12 | Inside an OperationWorkspace, FUSE/APFS callers receive only the private driver view path, while the direct driver may expose thin `layerfs-core::logical` primitives without a physical path; mutations normalize into one candidate RootTransition, and only an accepted OperationCommit binds it as one OperationDelta |

## 7. Store and synchronization gates

| ID | MUST requirement |
|---|---|
| S01 | `layerfs-storage` is the single concrete SQLite/object/schema/transaction/integrity/compaction mechanism; WorkingStore and DurableStore compose it without duplicate SQL or a runtime role/mode switch |
| S02 | `layerfs-working-store` owns host-recoverable Branch/Operation authority, exact begin pins/leases, candidates/conflicts, recovery records, and WorkingRecorded OperationCommit |
| S03 | `layerfs-durable-store` is a distinct system of record owning pushed Branch/Operation history, LayerStack state, exact heads, durable leases/retention, compaction, backup, and recovery; deployment may be local or networked, and clients never open its SQLite file over a network filesystem |
| S04 | Losing WorkingStore may lose history not yet pushed, while every `DurablyAccepted` version is recoverable from DurableStore on a fresh execution host |
| S05 | Fetch reads an exact durable receipt, sends hashes/ObjectIds first, transfers only negotiated missing immutable objects/history in resumable bounded batches, charges retransmission, authenticates in WorkingStore, verifies closure, and records `DurableTrackingRef` without merging or moving a dirty working Branch |
| S06 | Push starts from an already WorkingRecorded closure, sends hashes/ObjectIds first, transfers only negotiated missing objects/history in resumable bounded batches, and asks DurableStore to authenticate/verify independently before creating or advancing one durable Branch, advancing an exact immediate-parent head through ChildBranchMerge, or separately advancing the originating LayerStack through LayerStackMerge |
| S07 | Sync transfers canonical objects, RootTransitions, accepted version/delta records, exact heads, and receipts only; it never transfers workspace paths, owner/recovery markers, spools, dirty maps, mounts, processes, descriptors, mappings, or native files |
| S08 | Object transfer alone never creates durable visibility; WorkingRecorded and DurablyAccepted are separate receipts and transactions, never a distributed transaction |
| S09 | DurablyAccepted, Conflict, and Indeterminate receipts are distinct; Indeterminate requires fresh DurableStore reconciliation keyed by the same publication request |
| S10 | Each host/security domain normally uses one disk-backed WorkingStore shared by its authorized agents, Branches, and OperationWorkspaces; a host may separate security domains into separate WorkingStores, and multiple WorkingStores never form peer authority or exchange accepted state except through DurableStore |
| S11 | DurableStore rejects TrustedLocalDev history and never accepts WorkingStore trust claims as verification |
| S12 | Fetch/Push work is resumable and bounded in hash/object/byte queues and does not construct a complete object inventory in memory; actual missing and retransmitted bytes are counted |
| S13 | No snapshot read, mount syscall, filesystem mutation, close/fsync, tool exit, workspace finalization, or WorkingStore-only OperationCommit contacts DurableStore; Fetch/Push is explicit at durable version-control boundaries |
| S14 | Physical copies may exist once per Working/Durable storage database; authenticated ObjectId equality deduplicates within each database and drives cross-store missing-object transfer without claiming one global physical copy |

## 8. Mount and materialization gates

| ID | Mounted/FUSE MUST requirement |
|---|---|
| M01 | FUSE callbacks translate to `layerfs-core::logical` through the mount model and do not implement a second filesystem or canonical identity |
| M02 | Dirty state is private ranges/namespace changes plus bounded spool; unchanged canonical objects remain shared |
| M03 | Count-changing logical edits update extents without rewriting an ordinary native-file suffix |
| M04 | OperationCommit canonicalizes only admitted dirty streams and changed persistent-tree paths, with honest worst-case fallback counters |
| M05 | Multiple active mounted operations share immutable WorkingStore objects but never dirty overlays |
| M06 | The mount driver implements the universal `layerfs-workspace` contract; its private `0700` mountpoint and sibling bounded spool remain host-local and never enter Sync/DurableStore |
| M07 | Every mount syscall terminates at the nearby disk-backed WorkingStore path; no syscall performs a DurableStore RPC, whole-file hydration, or whole-file buffering |

| ID | Materialization/APFS MUST requirement |
|---|---|
| P01 | A native workspace is a derived real-file presentation bound to exact Store, version, RootId, and ownership provenance |
| P02 | Arbitrary external capture is complete and linear unless exact trusted change evidence proves a narrower route |
| P03 | APFS count-changing physical edits never claim that a suffix moves for free; different-length replacement uses an explicit charged route |
| P04 | LayerStackMerge changes canonical authority without rematerializing the candidate; physical refresh occurs only when a caller requests an updated native presentation |
| P05 | Refresh uses changed paths and clone/patch only when semantics and hard-link closure are safe; otherwise it records explicit full fallback |
| P06 | Supported Apple metadata round-trips exactly and environmental metadata policy remains narrowly platform-owned |
| P07 | The materialization driver implements the universal `layerfs-workspace` contract; its private physical view is on the admitted APFS volume for qualified clone routes, but count-changing/full fallback costs remain presentation-specific |

## 9. Durability, recovery, and maintenance gates

| ID | MUST requirement |
|---|---|
| D01 | Each WorkingStore OperationCommit, ChildBranchMerge, or BranchRollback uses one expected-head writer transaction and one SQLite publication COMMIT when state changes; candidate Layer preparation moves no LayerStack head. Each DurableStore Branch action, ChildBranchMerge, LayerStackMerge, BranchRollback, or LayerStackRollback uses its own exact transaction/COMMIT; there is no cross-Store/distributed transaction |
| D02 | Working heads reference host-recoverable records; durable heads become visible only after DurableStore has durably accepted every referenced object and version record |
| D03 | SQLite COMMIT dispatch, acknowledgement, and fresh reconciliation are distinct observations |
| D04 | A lost acknowledgement never causes blind redispatch or a false success |
| D05 | Clean and forced-death reopen select exactly one valid old-or-new head in each Store and never create a missing Store accidentally |
| D06 | DurableStore backup and restore prove exact Verified roots, version graph, leases, and object inventory and recover onto a fresh execution host |
| D07 | `layerfs-storage` supplies one compaction mechanism; each Store owner supplies its retained Layer/Branch/OperationVersion/verified-DurableTrackingRef/lease roots, then copies/authenticates/verifies and switches its own generation safely |
| D08 | Store format migration fails closed before mutation and preserves rollback to the prior compatible binary/store generation |

## 10. Time, space, and resource gates

The exact equations and benchmark matrix are in
[efficiency-and-benchmarks.md](efficiency-and-benchmarks.md). Every applicable
release must additionally satisfy:

| ID | MUST requirement |
|---|---|
| Q01 | Logical range read is path resolution plus `O(log E + X + R)` for extent lookup, intersecting extents, and returned bytes |
| Q02 | Ordinary mounted working edit is proportional to replacement/dirty bytes, bounded CDC work, and changed tree height; unaffected suffix reads/writes are zero |
| Q03 | Branch and Layer history storage is base unique objects plus unique deltas/tree nodes, not version count multiplied by workspace size |
| Q04 | Fetch/Push time and bytes are charged separately for hash negotiation, closure traversal, existing-object checks, missing/resumed/retransmitted transfer, verification, durable Branch publication, and optional LayerStackMerge |
| Q05 | Native materialization/capture/full fallback remains honestly byte-linear where required |
| Q06 | Memory ceilings are independent of workspace/file/version count and size: largest product buffer remains at most the declared <=1 MiB-style bound, queues/batches are bounded, and Q/RSS/FD/connection/thread/handle/spool/temp high-water and terminal state are counted |
| Q07 | No in-memory workspace database, whole-file mount hydration/buffer, all-extents vector, complete namespace clone, complete object inventory, or one-native-copy-per-logical-version is admitted; large dirty/cache state is disk-backed |
| Q08 | Workspace begin/runtime/quiescence/finalization, `layerfs-core::logical` candidate construction, WorkingStore publication, cleanup, explicit sync transfer, DurableStore verification/publication, durable acknowledgement, and complete wall time are separate timers |
| Q09 | `layerfs-workspace` owns userspace-Q accounting; mount/materialization drivers separately own spool/native/path/process resources; terminal cleanup returns each declared owner to baseline |
| Q10 | Many-Branch CAS sharing is measured within one WorkingStore, while cross-store accounting reports one possible physical copy per store plus negotiated missing and retransmitted bytes |

## 11. Evidence and release gates

| ID | MUST requirement |
|---|---|
| E01 | Tests and measurements use the shipped Core logical/Storage/WorkingStore/Workspace/presentation path and, where durable claims apply, Sync/DurableStore/Service path—not copied benchmark algorithms |
| E02 | Raw rows bind source tree, executable/image, Store format, fixture, configuration, platform, architecture, integrity mode, and route |
| E03 | Correctness, roots, deltas, generations, transactions, counters, timer equations, resources, and cleanup independently recompute |
| E04 | Warm, cold, incremental, fallback, WorkingStore, DurableStore, mounted, and materialized populations are never combined into one claim |
| E05 | Performance targets are selected before authoritative measurement and are not weakened after observation |
| E06 | Current HEAD inherits historical evidence only when the audited product path is byte-identical or the exemption is explicitly proven |
| E07 | Published limitations name every unsupported platform, operation, integrity mode, topology, and durability boundary |
| E08 | Durable-flow tests cover Branch Push/create-or-advance, Fetch on another WorkingStore, continue/edit/Push, and a same-durable-head conflict from multiple WorkingStores without peer authority |
| E09 | Merge tests cover parent/child parallel three-root merge, stale parent, forbidden cross-tree Branch/LayerStack destinations, repeated merges, and direct any-depth LayerStackMerge with inherited closure while preserving the source Branch |
| E10 | Resource tests cover many-Branch same-WorkingStore CAS sharing, cross-store missing-byte transfer, mounted small/count-changing edits without full-file memory or work, resumable bounded Fetch/Push, and literal zero Durable RPCs for mount syscalls |

## 12. Current evidence boundary

Retained Stage 1.1 evidence qualifies only its exact Apple/APFS PoC artifact.
Retained Stage 2 candidate evidence qualifies only its exact host Linux/FUSE
artifact and environment. Neither proves the new version graph, final crate
layout, universal Workspace contract, independent DurableStore admission, or
two-Store synchronization model on current HEAD.

## 13. Terminal review

The final reviewer must answer yes to all of these:

1. Does the implementation use the exact one-commit, two-merge, two-fork, and
   two-version model?
2. Are arbitrary operations isolated and conflicts preserved?
3. Are Core identity, CAS, CDC, and COW invariant across presentations and
   deployments?
4. Are Store and synchronization separate without duplicated Working/Durable
   implementations, runtime policy selector, or a one-Store fallback mode?
5. Are mount and materialization claims measured on their actual physical
   paths?
6. Are durability, rollback leases, compaction, resource ceilings, and cleanup
   proven on the exact artifact?
7. Do public claims stay inside the qualified envelope?
8. Does the SDK keep immutable snapshot reads outside OperationWorkspace and
   avoid tool/shell taxonomy while every mutation uses one universal workspace
   boundary and one OperationDelta?
9. Do Branch Push and LayerStackMerge remain independent, while every nested
   Branch retains an immediate-parent Branch merge route and an inherited direct
   LayerStack merge route?
10. Are every WorkingStore and transfer path disk-backed/bounded with no
    per-syscall Durable RPC or workspace-size-dependent memory?

Any “no” is `REVISE`, not partial product readiness.
