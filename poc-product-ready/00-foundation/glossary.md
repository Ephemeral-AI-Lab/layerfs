# LayerFS terminology and glossary

This glossary is normative for the product architecture. Current Rust names
are evidence of implementation status, not permission to replace these terms.

## Status words

| Status | Meaning |
|---|---|
| **Implemented** | Present in current source. |
| **Qualified** | Implemented and supported by retained evidence for one exact source, artifact, and environment. |
| **Target** | Required product behavior or public API that is not yet fully implemented or qualified. |
| **Deferred** | Deliberately outside the current delivery milestone, but not removed from the architecture. |

## Canonical filesystem state

| Term | Definition | Status |
|---|---|---|
| **Canonical bytes** | Deterministic, versioned bytes for one typed LayerFS object. | Implemented. |
| **ObjectId** | BLAKE3 identity of complete canonical object bytes, including their role and framing. | Implemented. |
| **Chunk** | Bounded content payload emitted by the frozen FastCDC profile and stored as a canonical object. | Implemented. |
| **Extent** | Logical file segment that references an immutable content object and records its logical length and source range. | Implemented. |
| **Extent rope** | Persistent byte-measured B+ tree mapping logical offsets to extents. A splice path-copies changed spines instead of renumbering an unaffected suffix. | Implemented. |
| **Namespace** | Persistent directory/name graph mapping names to stable logical inode identities. | Implemented. |
| **InodeId** | Stable logical inode identity allocated from an issuer `StorageId` and serial, independent of native inode numbers. It remains canonical and is transferred unchanged between storage databases; “storage-scoped” describes allocation, not portability. Current source calls the issuer `StoreId`; the target rename does not change encoded bytes. | Implemented format; target public name. |
| **Metadata tree** | Canonical portable metadata associated with logical filesystem objects. Host-only metadata is excluded unless explicitly modeled. | Implemented core surface. |
| **RootId** | `ObjectId` of one complete immutable logical filesystem state. | Implemented. |
| **StorageId** | Stable identity of one physical `layerfs-storage` database. Working and Durable databases always have different StorageIds. It is provenance identity, not content identity. Current source calls this `StoreId`. | Implemented mechanism; target public name. |

## Storage algorithms

| Term | Definition | Status |
|---|---|---|
| **FastCDC / CDC** | Content-defined chunking with the frozen 8/16/32 KiB minimum/target/maximum profile. It creates identities for new or replacement byte streams; it is not a background whole-database scan. | Implemented. |
| **CAS** | Immutable content-addressed storage keyed by `ObjectId`. A missing object is inserted; an incumbent is authenticated before reuse. | Implemented. |
| **COW** | Persistent logical copy-on-write. New roots reuse unchanged chunks and extent, namespace, inode, and metadata nodes. It is independent of host block cloning. | Implemented. |
| **Deduplication** | Convergence of equal canonical bytes to one authenticated `ObjectId` per storage database. It occurs during canonical insertion, not through a later duplicate-file scan. | Implemented. |
| **Structural sharing** | Reuse of unchanged immutable subtrees across OperationVersions, Branches, and Layers. | Implemented by the canonical trees. |
| **Compaction** | Verify the retained reachable union, copy it to a replacement storage generation, and atomically install that generation. It never rechunks objects and is not destructive in-place GC. | Implemented mechanism; the complete product retention graph is Target. |

## The two version levels

LayerFS has exactly two retained version levels. `WorkingRecorded` versus
`DurablyAccepted` is an acceptance class for the same version identities, not a
third version level.

| Term | Definition | Status |
|---|---|---|
| **OperationVersion** | Branch-level immutable version created by a WorkingRecorded `OperationCommit` or `ChildBranchMerge`. It binds a Branch, sequence, parent version, `RootId`, and creating record; an explicit `Push` can make the same identity DurablyAccepted. Two OperationVersions may reference the same `RootId` because history identity and filesystem identity are different. | Target product record; current refs/roots provide lower-level mechanisms. |
| **Layer** | LayerStack-level immutable version. A Layer binds its originating LayerStack, parent Layer, `RootId`, and source candidate/merge record. A prepared candidate is immutable but does not become the visible LayerStack head until `LayerStackMerge`. | Target product record; retained roots are the current lower-level equivalent. |
| **LayerStack** | Accepted linear lineage of Layers with one guarded head. A default LayerStack may be named `main`, but a LayerStack is not a Branch and has no OperationVersion history. Every retained Layer is directly readable by `RootId`; the lineage is not a replay-only delta chain. | Target product model. |
| **Branch** | First-class retained line of OperationVersions with immutable origin, guarded generation, and head. A top-level Branch begins at a retained Layer; a child Branch begins at an exact completed `OperationRecordRef` on its immediate parent. Branch nesting depth is unbounded. A pushed Branch and its accepted history are retained by DurableStore; unpushed Branch state exists only in WorkingStore. | Target product model. |

## The three delta scopes

The scopes below describe why a transition exists. They all reference the same
versioned authenticated parent-root/result-root transition representation; they
do not create three competing content formats. RootIds remain filesystem
authority, so reads never depend on replaying that representation.

| Term | Definition | Status |
|---|---|---|
| **OperationDelta** | Exact normalized filesystem effect of one isolated Operation: base root/version to result root/version, including an empty state delta when a WorkingRecorded operation leaves the filesystem unchanged. | Target product record. |
| **BranchDelta** | Four-root merge record: immutable base, selected source Branch head, exact destination head, and merged result. It references a source transition (`base -> source`) and applied transition (`destination -> result`). When destination equals base, those transitions may be identical. | Target product record. |
| **LayerDelta** | Transition from the current parent Layer to the prepared candidate Layer accepted by `LayerStackMerge`. | Target product record. |

## One commit, two merges, two forks, two rollbacks

These names are intentionally not interchangeable.

| Kind | Term | Exact meaning |
|---|---|---|
| **Commit** | **OperationCommit** | The only public commit action. It atomically records one `OperationDelta`, creates one `OperationVersion`, and advances the target Branch head by exact expected-head CAS. A stale head returns conflict and preserves the candidate. |
| **Merge** | **ChildBranchMerge** | Three-root merge from a child Branch into its exact immediate parent Branch: the child's immutable fork OperationVersion is the base, child head is the source, and exact expected parent head is the destination. It creates a parent `OperationVersion` only if that complete destination head still matches. |
| **Merge** | **LayerStackMerge** | Three-root merge of any-depth Branch state into the one originating LayerStack inherited from its top-level ancestry. It makes a prepared candidate the visible next Layer only if the exact expected LayerStack head still matches. |
| **Fork** | **LayerBranchFork** | Create a top-level Branch from a specific retained `LayerRef`. Its legal LayerStack merge destination is that originating LayerStack. |
| **Fork** | **ChildBranchFork** | Create a child Branch from a specific completed `OperationRecordRef` on its immediate parent Branch. Its only legal branch-to-branch merge destination is that exact immediate parent; it inherits the top-level ancestry's originating LayerStack. |
| **Rollback** | **BranchRollback** | Expected-head move to an earlier OperationVersion on the same Branch, with lease-guarded hard release of the unused suffix. |
| **Rollback** | **LayerStackRollback** | Expected-head move to an earlier Layer on the same LayerStack, with lease-guarded hard release of the unused suffix. |

Child depth is unbounded by the model. A child Branch may itself become the
immediate parent of another `ChildBranchFork`, but every edge is created from an
exact operation-created `OperationRecordRef`. Parentage is immutable. The only
branch-to-branch direction is child to exact immediate parent: sibling, cousin,
unrelated, parent-to-child, skipped-grandparent, cyclic, and reparenting merges
are invalid. Independently, a Branch at any depth may `LayerStackMerge` directly
to its inherited originating LayerStack and no other LayerStack. A nested source
proposes its complete root, including parent-Branch state inherited at its exact
fork OperationVersion. Every merge leaves the source Branch alive and unchanged.

`BranchCommit` is not a second public commit action. Older drafts used that
name for sealing a `BranchDelta` into a durable candidate Layer. The product
API should call this **Layer candidate preparation** (for example,
`prepare_layer_candidate`) and reserve **commit** for `OperationCommit`.

## Authority and conflict terms

| Term | Definition | Status |
|---|---|---|
| **LayerRef** | Exact reference to a retained Layer: LayerStack identity, Layer identity, generation/position, and `RootId`. | Target. |
| **OperationRecordRef** | Exact completed-operation reference containing `parent_branch_id`, `operation_id`, `operation_version_id`, and `root_id`. It is the only valid source of `ChildBranchFork`. | Target. |
| **BranchHead** | Exact guarded tuple containing Branch identity, generation, head OperationVersion, and `RootId`. A new Branch with no commits derives its effective head from its fork source. | Target over current `RefState`. |
| **LayerStackHead** | Exact guarded tuple containing LayerStack identity, generation, head Layer, and `RootId`. | Target over current `RefState`. |
| **Expected-head CAS** | Advance a Branch or LayerStack only if its complete previously observed head still matches inside the visibility transaction. | Implemented at current ref/root level; richer heads are Target. |
| **Conflict** | Expected head no longer matches or an exact three-root merge cannot prove a unique result. LayerFS does not retry or retarget implicitly; it preserves the immutable candidate and source Branch for explicit recomputation, inspection, or drop. | Implemented at current ref level; candidate records are Target. |
| **Candidate Layer** | Immutable Layer prepared from a sealed BranchDelta. Preparation does not move a LayerStack head. | Target. |
| **Originating LayerStack** | Immutable LayerStack identity inherited by every descendant of a `LayerBranchFork`. It is the only legal LayerStackMerge destination for that entire Branch tree. | Target. |
| **Cross-tree reference** | Read-only access to retained content or exact versions outside a Branch's ancestry. It grants no merge relationship and cannot move either tree's head. | Target. |

## Operation and workspace terms

| Term | Definition | Status |
|---|---|---|
| **Operation** | One arbitrary isolated unit of filesystem activity. Its private view may be used by SDK filesystem primitives or by an externally launched editor, shell, compiler, dependency installer, or process tree. LayerFS neither executes nor classifies those tools; one workspace boundary yields one candidate RootTransition, and an accepted commit records one final `OperationDelta`. | Target public lifecycle. |
| **OperationWorkspace** | Private mutable presentation pinned to one exact effective `BranchHead` and base `VersionRef`. Sibling Operations never mutate it. It may be direct logical state, a mounted logical overlay, or a native materialized tree. It is runtime state, not a third version type. | Target abstraction over implemented mechanisms. |
| **Exact-version read** | SDK `stat`, `list`, `read_range`, `stream`, and `readlink` against an immutable exact version. It needs no mutable workspace, moves no head, and triggers no synchronization. | Target thin SDK surface over `layerfs-core::logical` with caller-supplied object access. |
| **Direct logical workspace primitive** | Filesystem mutation such as create, replace, splice, truncate, rename, link, unlink, symlink, or metadata update inside one private OperationWorkspace, routed to `layerfs-core::logical`. These are filesystem primitives, not agent-tool or command APIs. | Target SDK/workspace surface. |
| **begin_operation** | Pin the exact Branch head, acquire a lease, and return one private `OperationWorkspace`. | Target facade. |
| **end_operation** | Quiesce the workspace and explicitly commit, discard, or preserve it. An accepted commit produces one OperationDelta and one OperationVersion. | Target facade. |
| **Quiescence** | No admitted callback, process, descriptor, or writable mapping can still mutate the OperationWorkspace. `fsync` or child-process exit alone is not sufficient. | Partly implemented; complete runtime supervision is Target. |
| **Core logical semantics / `layerfs-core::logical`** | Portable whole-root path resolution, reads, mutations, hard-link/namespace rules, Merkle diff, and three-root merge candidate construction. Exact files are `mod.rs`, `resolver.rs`, `read.rs`, `mutate.rs`, `diff.rs`, and `merge.rs`, generic over Core object access. It returns candidates/plans and never owns Storage, Branch/LayerStack, OperationWorkspace, publication, synchronization, FUSE/APFS, or native paths. | Target destination for portable semantic code remaining after presentation extraction from current `layerfs-vfs`. |
| **Presentation** | Direct logical, mounted, or native materialized access to the same canonical state. Presentation is not authority. | Implemented at different maturity levels. |
| **Mount** | Direct filesystem presentation backed by LayerFS logical state. Dirty data stays in a private bounded overlay/spool and is canonicalized at `OperationCommit`; ordinary writes do not require a full physical file. | Implemented on Linux/FUSE in current crates; final ownership/API is Target. |
| **Materialization** | Construct a physical native directory from a RootId. Arbitrary tools edit ordinary files; capture converts exact changed evidence or a complete quiescent scan back into canonical state. | Implemented; Apple/APFS is the retained host PoC. |
| **Capture** | Freeze a materialized workspace, reconstruct exact logical state, create missing canonical objects, and produce an operation candidate. | Implemented routes. |
| **Refresh** | Align a retained materialized workspace from known root A to target root B using exact change evidence where safe and explicit full fallback otherwise. | Implemented routes with source-bound qualification. |

Current methods named `checkpoint` in managed/mounted workspace code perform
lower-level canonical publication. That name is legacy implementation detail,
not public product vocabulary. The target wrapper routes one logical operation
to one `OperationCommit`.

## Stores and synchronization

| Term | Definition | Status |
|---|---|---|
| **Storage database / `layerfs-storage`** | Shared SQLite substrate for canonical objects, version/delta/transition records, heads, retention, leases, integrity, compaction, and exact transaction mechanics. It has no Working/Durable policy. | Current mechanisms live in `layerfs-engine`; target extraction/rename. |
| **WorkingStore / `layerfs-working-store`** | Public execution-host policy over one storage database. It owns the verified cache, host-recoverable working Branch/Operation history, candidates/conflicts, and workspace-recovery policy. Its successful Working receipt is `WorkingRecorded`; loss may discard unpushed work. | Target policy crate. |
| **DurableStore / `layerfs-durable-store`** | Public system-of-record policy over a distinct storage database. It owns `DurablyAccepted` admission, durable Branch/Operation and LayerStack/Layer policy, trusted-mode refusal, retention, backup, and recovery. | Target policy crate over `layerfs-storage`. |
| **Workspace runtime / `layerfs-workspace`** | Owns the isolated `OperationWorkspace` runtime lifecycle, admission guards, quiescence, presentation binding, candidate custody, cleanup, and terminal receipt routing. WorkingStore creates and persists the exact base-version lease; `layerfs-core::logical` constructs the portable candidate through generic object access. Workspace owns neither version SQL nor synchronization. | Target extraction over current VFS/workspace mechanisms. |
| **DurableTrackingRef** | WorkingStore record of an exact DurableStore version/head and verification receipt. A `verified_complete` ref retains its WorkingStore closure until explicit eviction. | Target. |
| **Fetch** | Explicit `layerfs-sync` transfer from DurableStore to WorkingStore of an exact retained version and negotiated canonical object set, charging retransmission. WorkingStore authenticates/verifies the fetched closure and records `DurableTrackingRef`. Fetch never merges or silently moves a dirty Branch. | Target transfer workflow. |
| **Push** | Explicit `layerfs-sync` transfer from WorkingStore to DurableStore of exact accepted canonical objects plus Branch, OperationVersion, OperationDelta, candidate, or requested head-transition records. DurableStore independently authenticates and admits them. Transfer alone is not durable acceptance. | Target transfer workflow. |
| **WorkingRecorded** | Operation/version is host-recoverable in WorkingStore but may be lost with that execution host and is not yet system-durable. | Target receipt state. |
| **DurablyAccepted** | The exact objects, history, and requested head action are accepted by DurableStore and recoverable on a fresh execution host. | Target receipt state. |

WorkingStore and DurableStore always use distinct physical storage databases
with distinct `StorageId`s and the same `layerfs-storage` schema/transactions.
Storage persists no role discriminator and exposes no policy-mode switch.
Policy comes only from the public `layerfs-working-store` and
`layerfs-durable-store` APIs.
`layerfs-sync` is the explicit Fetch/Push transfer bridge; it never owns
version admission, merge, or head-transition policy.
There is no standalone/one-Store product mode and no background synchronization
from filesystem syscalls or a WorkingStore-only `OperationCommit`.

DurableStore retains the exact OperationVersions and OperationDeltas accepted
through `Push`. Unpushed WorkingRecorded Branch history remains only in
WorkingStore and may be lost with that execution host. Neither `Fetch` nor
`Push` is implicit in a read, Operation, fork, merge, or rollback.

## Rollback, leases, and cleanup

| Term | Definition | Status |
|---|---|---|
| **VersionLease** | Durable or recoverable pin held by a Branch, child Branch, OperationWorkspace, mount, materialization, candidate preparation, merge, sync, or explicit caller. | Target. |
| **BranchRollback** | Expected-head move from a Branch head to an earlier OperationVersion, followed by logical hard-drop/release of the unused suffix. It is rejected while any suffix version is leased. | Target over current guarded ref moves. |
| **LayerStackRollback** | Expected-head move from a LayerStack head to an earlier Layer, followed by logical hard-drop/release of the unused suffix. It is rejected while any suffix Layer is leased. | Target over current guarded ref moves. |
| **Discard** | End private OperationWorkspace state without advancing a Branch head. It never deletes canonical history reachable elsewhere. | Implemented mechanisms. |
| **Ambiguous outcome** | A visibility call lost acknowledgement after dispatch. A fresh independent storage read must classify requested, prior, conflict, or indeterminate; it is never blindly retried. | Implemented at current publication level. |

Hard rollback releases logical retention; physical object deletion happens only
later through verified reachability compaction because CAS objects may be
shared by other OperationVersions, Branches, or Layers.

## Deliberately excluded public vocabulary

Do not use these as product concepts:

- `Checkpoint`: OperationVersion and Layer are the two native retained version levels.
- `Promote`: use `ChildBranchMerge` or `LayerStackMerge`.
- `ChangeSession`: use `Operation` and `OperationWorkspace`.
- a generic unqualified `Fork` or `Merge`: use the origin/destination-specific name.
- a generic Git-shaped commit DAG: LayerFS preserves distinct Branch and LayerStack version levels.
- storage-layer MCTS, scores, rewards, rollout policy, or search policy: an
  external orchestrator may use nested Branches and retained versions for MCTS,
  but LayerFS remains a filesystem and records none of that policy.

Cross-tree content reads and references are valid, but they do not add another
merge. A future selective-apply/cherry-pick feature must run as an ordinary
isolated `Operation` and produce one `OperationCommit`; it is Deferred.

Legacy source method names may be quoted in current-status notes, but they do
not define the public architecture.

## Cost notation

| Symbol | Meaning |
|---|---|
| `F` | complete logical file bytes |
| `E` | extents in one file |
| `H` | extent-tree height |
| `B` | new/replacement bytes |
| `R` | returned bytes |
| `N` | namespace objects/paths |
| `U` | retained reachable objects |
| `Q` | explicitly owned userspace memory; disk spool bytes are accounted separately |
| `S_spool` | owned dirty bytes in the bounded disk spool |

Expected fast-path claims are proportional to changed bytes and changed tree
spines. Cold materialization, arbitrary external capture, verified full-closure
scrub, reachability, and compaction remain honestly linear.
