# Snapshot-Isolated Workspace specification

Status: detailed product specification draft, 2026-09-14; not implemented,
not performance-qualified, and **not implementation-ready for the complete
supported FUSE surface while blocker V1 remains unresolved**. This is a
specification of behavior, records, interfaces, and resulting component shape,
not an implementation schedule or task plan.

Governed by [overlay-snapshot-rule.md](overlay-snapshot-rule.md). Architectural
context and source register:
[overlay-snapshot-architecture-design.md](overlay-snapshot-architecture-design.md).
Product tracking: [#123](https://github.com/Ephemeral-AI-Lab/layerfs/issues/123).
Existing benchmark contracts in [#122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122)
remain unchanged unless separately versioned. Source baseline:
`0814cc37f1dafb6041930c74489107f4a5035a26`.

Execution tracking: [implementation #124](https://github.com/Ephemeral-AI-Lab/layerfs/issues/124)
and [benchmark reporting #125](https://github.com/Ephemeral-AI-Lab/layerfs/issues/125),
following the separate [implementation plan](overlay-snapshot-implementation-plan.md).
Its final phase runs the full existing suite **excluding #122-owned cases**, using
the [case-level exclusion manifest](benchmark-exclusions-issue122.json). References
to #122 infrastructure do not authorize running its excluded cases in that campaign.

MUST/MUST NOT are required semantics. Selected draft mechanisms below are concrete
specification choices subject to adversarial review and the governing rules;
selection is not evidence of feasibility or performance. No production source,
Store schema, benchmark implementation, or historical evidence is changed here.

## 1. Scope, selected shape, and completion conditions

One live Workspace maintains current filesystem state. A Commit attempt acquires
one temporary snapshot, constructs canonical objects independently, retains its
candidate in `workspace_stages`, then conditionally publishes branch history.
Commands, file descriptors, inode identities, and the mount survive. Publication
never installs captured content into the live Workspace and never resets newer
changes. Internal storage sequence numbers are not Workspace commit history.

```text
CONTAINER                                  HOST
Commands / FUSE / bounded adapter cache --> current overlay installation authority
SDK requests ----------------------------> same authority/coherence contract
                                                   |
                                      immutable COW root + shared backing
                                                   |
                                 +-----------------+------------------+
                                 |                                    |
                           later mutations                    acquired snapshot S
                                 |                                    |
                           current live root                    owned snapshot reader
                                                                      |
                                           existing shared Init/Commit construction
                                                                      |
                                                               workspace_stages
                                                                      |
                                                        conditional branch history
```

| Decision | Selected draft contract | Important limit |
| --- | --- | --- |
| Installed metadata authority | Host backing owner; container is adapter/cache | An acknowledged ordinary mutation has host-owned readable bytes/metadata. This changes small-write RTT dependencies. |
| Temporary metadata | Immutable published copy-on-write B-tree page graphs and bounded overflow/range indexes | Snapshot capture is a root lease; every write still pays incremental metadata costs. Runtime page encoding is private, not canonical CAS. |
| Payload backing | Stable extent identities in a bounded number of host allocation arenas, with disk-indexed locations and ownership | Snapshot/read ownership can retain space; no one FD per historical segment. |
| Snapshot acquisition | Constant-sized atomic root/retention acquisition | Does not solve kernel-visible dirty mappings absent from host state. |
| Commit coordination | One unresolved attempt per Workspace; bounded queued requests capture only when started | Other Workspaces may construct independently under existing leases and shared budgets. |
| Staging | Reuse the current one-row-per-Workspace candidate-root model | Retry the same candidate; stage presence does not imply inactive live filesystem. |
| Successive Commit reuse | Latest metadata-only provenance description per published inode, indexed source intervals, transient predecessor-relative view | No raw old-spool pin for comparison; metadata traversal/index work is counted, not claimed O(changed bytes). |
| Canonical format | Existing shared CAS/authentication, FULL/DELTA policies, CDC/extents, packing/compression and object admission | No second encoding pipeline and no per-update canonicalization requirement. |

V1-V4 identify unresolved correctness/design obligations, not permission for a
weaker fallback. V5 records later evaluation using existing benchmarks; it is not
a new implementation-readiness blocker:

| ID | Completion condition | Present status |
| --- | --- | --- |
| V1 | Exact supported FUSE cached/mapped visibility without global freeze, full cache drain, or weakened existing mapping semantics | No compliant complete kernel/adapter mechanism established. Host installation solves only bytes it actually owns. |
| V2 | Page/catalog ownership and root installation, failure-safe spill, bounded reclamation, foreground RTT under host authority | Concrete contracts specified; algorithm/encoding and public-path proof absent. |
| V3 | Indexed provenance matching and transient predecessor view preserve localized C2, including duplicate/shifted ranges | Concrete descriptor/index contract specified; builder adapter and complexity proof absent. |
| V4 | Exact publication-outcome resolution, particularly lost UpToDate acknowledgment | Required witness semantics specified; existing Store observation contract needs definition/verification. |
| V5: evaluation follow-up | After correctness, run applicable existing benchmarks and inspect results using their established contracts | Not a new design/implementation-readiness blocker. No new numerical performance gates or mandatory new benchmark families are set here. |

A root-only diagnostic, paused-writer run, correct result obtained by repeated
full-file reconstruction, or arithmetical million-descriptor calculation cannot
close these conditions. Read the [adversarial review](overlay-snapshot-spec-review.md)
before treating this draft as executable product scope.

## 2. Storage, mutation authority, and snapshot contract

This section specifies the resulting storage and snapshot behavior. MUST denotes a requirement, not an implemented or measured property. It does not relax the governing overlay/snapshot rules. The metadata model below is selected for the draft; V1 (architecture G1) remains an explicit release blocker.

### Authority and physical placement

The host Workspace backing owner is the sole authority for installed inode, namespace, content-range, and change-index state. The container live adapter owns kernel-facing handle/reference tracking, request ordering, and bounded caches; those caches are not an independently publishable filesystem state. SDK mutations enter the same installation authority and cache-coherence contract.

Commit construction uses a host-owned snapshot reader and must not read the live mount or request an export of the current dirty prefix. There remains exactly one live FUSE mount. The host stores private metadata and replacement bytes under `<runtime_root>/workspaces/<workspace_id>/`; the persistent Store remains at its independently configured location.

The authoritative current overlay is a root bundle over a host-backed copy-on-write page graph. Metadata keys and large values are indexed rather than stored in one growing serialized inode record. The implementation must retain normal short operation ordering; it must not hold its root-install synchronization across network requests, physical storage I/O, canonical construction, or reclamation.

Changing authority necessarily changes acknowledgment timing: a successful ordinary write cannot depend exclusively on a mutable payload buffer left in the container. Before acknowledging a successful mutation, the host must own the exact replacement bytes and corresponding installed metadata, whether the bytes are in a charged host buffer or private disk backing. The existing container-only filling window is therefore a transfer optimization, not authoritative storage after acknowledgment.

Every ordinary mutation that installs new state has a host acknowledgment dependency, even when its metadata is cached. It does not require a durable flush per operation. Several operations can be carried by one bounded transport batch, but each retains its own atomic ordering/result and is not acknowledged before its own installation. Sequential small writes from one process may not have sufficient concurrency to batch; the specification does not claim zero RTT cost. Host caches may be used for reads only under the chosen coherence contract.

### Logical records

The following are data contracts, not requirements for a trait hierarchy or one service per record.

| Record | Required contents and semantics |
| --- | --- |
| CurrentRoot | Workspace identity, monotonically increasing installation sequence, inode-index root, binding-index root, change-by-key root, change-by-sequence root, stable-inode allocation state, and immutable backing/base context. The whole bundle is installed atomically. |
| InodeRecord | Stable live inode identity, file type and supported attributes, link accounting, revision, and either a file-range root or directory state. No list of every alias is embedded in every file update. Open-unlinked state remains addressable by handles but absent from committed namespace enumeration. |
| BindingRecord | `(parent_live_inode, name)` to live inode or explicit removal relative to backing. Names and overflow values are separately bounded/indexed; rename updates its affected records in one mutation. |
| FileRange | Logical extent kind and length; immutable committed root/range, immutable temporary extent/range, inline bytes within bounded leaf space, or zeros. A fragmented file has a range tree, not a blob reserialized in full on each edit. |
| ChangeRecord | Stable affected key, latest installed sequence, current effect, and required reference/namespace accounting. Two captured roots index it by key and by latest sequence/key; superseded secondary entries are removed atomically. It is not an action log. |
| SnapshotHandle | Workspace identity, captured CurrentRoot identity/sequence, an owned reference to that root graph, and the Commit attempt's separate expected publication context. One root bundle is the snapshot boundary. |
| BackingExtent | Stable extent identity, readable length, storage location or retained immutable buffer, allocation/ownership accounting, and integrity/error state. Logical roots reference identity and range rather than a mutable raw pathname. |
| RetiredRoot | Root identity and release work still owed. It belongs to charged reclamation bookkeeping and does not create user-visible Workspace history. |

Live identity is independent of canonical inode identity. A newly created live inode must remain stable across Commit even when construction assigns or reuses a canonical identity. Commit-side correspondence belongs to the comparison/attempt context; replacing the live root with the captured root is forbidden.

A lookup resolves the current overlay delta against its immutable backing context. A snapshot lookup resolves only the captured bundle and captured backing context. Missing captured state is an integrity/I/O failure; it must never fall back to newer live records.

### Page and payload backing

The temporary metadata index uses bounded copy-on-write B-tree pages, with bounded overflow pages for values exceeding a leaf slot. Page size and cache capacities are internal selected format parameters recorded with the runtime format identity; changing them does not change canonical Store encoding. The page graph must support bounded cursors for inode, name, changed-key, and file-range traversal. Tree height and page fanout appear in accounting and complexity; an unbounded inode-to-page RAM table is forbidden.

Preparing a small update reads and replaces affected paths and overflow pieces only. Pages reachable by another root cannot be modified in place. This draft requires immutable published metadata pages, including pages owned only by the current root; omitting an exclusive-page mutation fast path keeps ownership uniform. Unowned prepared pages may be filled before publication. Snapshot acquisition and the first subsequent mutation must not clone a complete map or enumerate every file/range.

Replacement bytes occupy shared host backing extents; there is no full-file copy-up. Small replacements share allocation segments. Existing base ranges and unchanged temporary ranges remain shared. Payload publication makes each referenced byte interval immutable; non-overlapping appends may extend a segment without changing published intervals.

Temporary storage uses a fixed small set of private allocation arenas plus bounded scratch files, rather than one open file per inode, range, or historical segment. Arena files may use the existing create-then-unlink lifetime because their descriptors remain open for the arena lifetime; segments are address ranges within arenas, not independent descriptors. A disk-backed extent catalog maps stable extent identities to arena locations. Free-space metadata is itself bounded/disk-backed. This is temporary Workspace allocation maintenance, not a new explicit Store compaction operation.

The arena/cursor count is capped by the declared descriptor budget. An implementation choosing independently reopenable segment files instead must specify private-name integrity, descriptor caching, and cleanup; it cannot close the last descriptor of today's unlinked segment and expect to reopen it later.

Freeing a catalog slot is logical reuse, not necessarily physical block release. Each supported host backend must declare how empty allocation segments actually release blocks (for example a supported hole-release primitive, tail truncation, or bounded arena evacuation/replacement). Physical quota accounting uses allocated blocks, and qualification must prove quota recovery after owners release data; an unsupported deallocation primitive cannot be silently treated as successful reclamation.

The catalog allows moving live extents out of partially dead allocation segments: copy to reserved backing, verify length/content identity as required, atomically change the extent location, then release the old location after in-flight physical readers finish. Captured roots continue to reference the same immutable extent identity and bytes. This permits reclamation without changing live contents or rewriting every referencing inode. A relocation must reserve destination and transient resources before starting and never invalidate the source on failure.

Metadata-page physical location can use the same stable-location contract where allocation reuse requires it. The extent/page catalog must offer disk-indexed lookup, bounded cache, failure-safe location replacement, and short physical-read leases. A catalog entry does not mean a resident entry per extent.

### Resource reservation and mutation installation

The operation has four conceptual stages:

1. Acquire an owned source-root lease, resolve current records, and prepare affected replacement records/range paths. Retain the complete source-root identity/sequence and the revisions/predicates needed for revalidation. Preparation retains its root lease across I/O until its candidate has owned backing or it is abandoned.
2. Reserve worst-case admitted payload, metadata pages, catalog/ownership updates, transport buffers, and cleanup records. Acquire host ownership of replacement bytes and construct privately readable new pages. A bounded operation can retry if relevant revisions change; retry cannot expose a partial mutation.
3. Compare the complete prepared source-root identity/sequence with the current root, in addition to revalidating affected revisions and predicates. If the root changed, discard/rebase only the prepared work against a newly leased current root outside installation synchronization; do not install a root based on stale unrelated state. Given leased source sequence s, prepare candidate sequence s+1 and its sequence-dependent index keys before installation, with checked arithmetic. Atomically install that already prepared root bundle and both change indexes only if the current root is still the leased source. A conflict reparses/reprepares sequence-dependent state outside the lock; installation does not patch immutable B-tree pages or perform index I/O. The candidate already owns every referenced page/byte; installation transfers that owned root into the current-root slot and schedules release of the previous current owner. No disk reference-count update or I/O first becomes necessary inside the installation lock.
4. Return the operation result after required kernel/cache-coherence completion. Schedule old-root release outside the installation lock. If preparation fails, the previous installed root remains current and readable. If installation succeeded but acknowledgment is lost, the bounded transport result window below prevents duplicate application.

The allocator/catalog must support a prepared ownership transaction: failure before installation leaves unpublished allocations reclaimable and cannot reduce ownership of the old graph; after installation the new root and all reachable references are retained. Final visibility and ownership activation must not be separately fallible operations that leave an installed root without its backing. Prepared ownership state and retirement queues consume admitted resources. An out-of-space cleanup queue cannot force an unaccounted successful installation.

A root prepared from R0 must not overwrite unrelated changes installed through R1.
Per-inode revision checks alone are insufficient. Retried preparations remain
charged until reclaimed. The scheduler must provide bounded admission and fair
rescheduling for disjoint writers; snapshot acquisition does not wait for pending
uninstalled preparations. Record preparation attempts, root conflicts, discarded
pages/bytes, and retry time. Complexity carries retry factor `r`, not an assumed
one-attempt bound. Exact retry/fairness policy and its contention limits remain V2;
no success may be returned for a discarded update, and benchmark profiles require
all valid mutations to complete without contention errors.

Mutation replay uses `(transport_session, request_sequence)` with a bounded
outstanding window. The host retains results for installed but unacknowledged
requests; the client cumulatively acknowledges received results, advancing a
watermark. Duplicate requests inside the window return the retained result without
reapplying append/rename. Requests below the settled watermark are stale and must
not execute again; requests beyond admitted window capacity are not accepted.
A reconnect must resolve retained session outcomes or return explicit uncertainty,
not silently assign a fresh identity to a possibly installed mutation. The window,
result bytes, session count, and expiration/error policy are resource-bounded;
there is no unbounded per-operation history or host-restart guarantee.

Directory rename/link/unlink updates are one root-bundle transition under their existing operation atomicity contract. This does not provide transactions across arbitrary application syscall sequences. Permission checks, append ordering, open-unlinked lifetime, existing input limits and all trust-boundary validation remain required.

Read paths acquire a current or snapshot record/range handle and a bounded physical-read lease before I/O. Root-install and catalog locks are released before bulk reads. Immutable content permits concurrent live and Commit readers. Catalog relocation must keep the old physical interval alive until already acquired physical leases release it.

### Snapshot acquisition

`AcquireSnapshot(workspace)` is ordered with mutation installation and root reclamation. It acquires ownership of the current root bundle, its captured sequence and base-reading context, and returns a fixed-size handle. The Commit coordinator binds the expected branch head/base and comparison context to this attempt separately.

Acquisition must not enumerate dirty records, serialize a node, copy payload, flush all dirty buffers, move all cache pages to disk, or synchronously visit the reachable ownership graph. It increments/registers a bounded root owner; existing graph ownership covers descendants. Snapshot-owned cached pages remain evictable. At most one Commit snapshot per Workspace is acquired by the Commit coordinator initially; retained read plans are separately bounded owners.

All construction reads use the same handle. Later mutations create new current roots without changing captured state. A queued Commit acquires its handle only when it starts after the previous attempt resolves. Snapshot release schedules bounded reclamation; canonical staging and branch publication do not overwrite current roots or reset dirty keys.

The installed-root operation is a complete snapshot algorithm only for state visible under the G1 kernel/adapter contract below. The implementation must not present host-only atomicity as proof of a correct full FUSE snapshot while G1 is unresolved.

### Ownership and reclamation

Root ownership retains metadata pages; metadata records retain child pages and referenced immutable extents. Snapshot acquisition retains an already registered in-memory root lease without enumerating descendants or updating disk reference counts inside the root synchronization boundary. Prepared root ownership already covers the graph. Creating a page acquires its bounded child references in its prepared ownership transaction. Removing a root releases reachability through a disk-backed, admitted reclamation queue. Persistent-within-runtime accounting is required for disk-resident references; Arc counts are sufficient only for actual in-memory owners, not the complete graph.

Every physical allocation belongs to an accounted category: current/shared, exclusively retained by snapshots/readers, unpublished prepared state, canonical-construction scratch, reclaimable pending cleanup, or fragmentation. Physical shared bytes are counted once. The accounting must permit explaining total allocation without treating logical file size as physical occupancy.

Obsolete roots and intermediate updates that no owner can observe must be reclaimable independently of a long-lived snapshot that needs different pages/bytes. Epoch-wide retention of all writes since capture is not the selected policy. References determine retention.

Logical source identity and physical allocation/liveness are separate. A large
logical source may span many independently reclaimable blocks. The selected
maximum indivisible retained payload block is 4 KiB; small replacements may share
blocks. A descriptor owning a subrange retains only blocks intersecting that range,
not every block of its original large source. Metadata/catalog ownership must
represent interval/block liveness with bounded indexes; interval leases are
normalized by logical source and offset so splitting a large range does not require
walking or incrementing every 4-KiB block in the unchanged prefix/suffix. Large
interval ownership updates need indexed/lazy range operations, not per-block loops
on each small edit. Ownership-index visits and overlap fragmentation are measured; snapshot root acquisition
still does not enumerate all blocks. At block boundaries, retained unused bytes
are charged as fragmentation. Reclaiming a 64-MiB source with one live byte must
retain at most the intersecting payload block plus metadata, reader leases, and
explicit cleanup slack, rather than all 64 MiB.

Relocation moves only live allocation blocks/intervals. Its maximum admitted move
unit is 64 KiB; larger evacuation is a bounded sequence of such units. Each Workspace
reserves at least one maximum destination move unit plus required bounded metadata,
read-buffer and allocator-update reservations as charged recovery headroom that
ordinary payload admission cannot consume. Multiple concurrent moves require the
corresponding multiplied reservation. Failing to reserve recovery capacity rejects
new allocation before installation; it cannot consume recovery headroom and then
claim quota recovery is guaranteed.

Freeing arena offsets does not necessarily shrink physical file allocation.
The selected backend must identify its supported hole-release/truncation or bounded
arena-rotation mechanism and failure semantics before V2 is closed. Free-list
capacity and actual allocated disk bytes have separate limits/receipts. The 4-KiB
liveness ceiling is a draft allocation contract, not a claim that every host can
release storage at 4-KiB granularity. Unsupported physical-release behavior remains
charged and blocks the corresponding storage-efficiency claim.

Each foreground allocation performs only a bounded reclaim assist; background reclamation drains admitted release work and relocates live extents when required to free partially dead segments. If available reclaim progress/reserved headroom cannot satisfy an operation within its admitted resources, return the existing resource error before installation. Do not force a global Commit wait or silently exceed budget. Cleanup I/O failure preserves existing owners/readability and remains charged; retries do not mark failed cleanup as free space.

Reusing backing already canonicalized by a successful Commit is a physical optimization, never a logical checkpoint. A substitution requires exact byte/range equivalence, retained canonical object ownership and a preserved source until readers release it. It must not mutate a file to the snapshot's older contents or assume inode-to-root correspondence alone proves range equivalence. Until safe substitution is possible, duplicate physical bytes remain explicitly charged.

### Budgets and complexity contracts

Runtime admission includes distinct host and container budgets plus aggregate policy across Workspaces. Required caps cover cache/dirty buffers, prepared transactions, active read leases, transport queues, Commit workers, ownership/reclamation records, temporary physical allocation and descriptors. Mmap/kernel cache observations are separate from process cache reservations. No configuration removes canonical validation or existing public limits implicitly.

| Operation | Required asymptotic shape, conditional on the selected balanced indexes | Additional pressure |
| --- | --- | --- |
| Mutation affecting T file pieces and U bytes | O(r*(Hmetadata + Hrange + T) + U), plus actual affected namespace/name work and charged retry allocations | Copied page paths and prepared catalog/ownership state; immutable-page COW adds write amplification even without snapshot |
| Snapshot acquisition | O(1) root/owner bookkeeping independent of file count, changed set and payload | One fixed-size handle; protocol/lock wait latency must be measured |
| Post-capture mutation | Same affected-path bound as ordinary mutation | Retention of old pages and replaced intervals needed by captured owners |
| Snapshot metadata lookup | O(Hmetadata) page accesses before cache effects | Bounded page cache and read leases |
| Extent read | Catalog lookup plus bytes actually read and existing committed-range costs | Read buffers and relocation lease; no lock held over content I/O |
| Release | Bounded foreground registration; total O(visited unowned graph/extent records) | Charged deferred queue; no synchronous graph-wide destruction |
| Relocation | O(bytes moved + catalog/index paths) | Destination reservation, shared-reader source retention and I/O contention |

Constant work is not constant wall-clock latency. Host authority adds transport dependencies to foreground writes; COW metadata adds affected-path copying; index catalog indirection adds lookup/cache misses. These costs must be measured against the current small-workload baseline and during Commit, not hidden in acquisition-only measurements. CAS/FULL-DELTA/CDC construction is unchanged by this temporary storage representation.

### V1: kernel visibility is an explicit unresolved release blocker

Current source requests FUSE capability flags without explicitly requesting `FUSE_WRITEBACK_CACHE`; callback names do not establish negotiated mode. Created handles use direct I/O and are not mmapable; later cached opens support mappings. Current `flush_kernel_cache` invalidates tracked nodes and calls `syncfs` while the freeze path controls admission. That mechanism violates the non-freezing fast-acquisition contract and is not reused for Commit.

This specification does not invent a kernel snapshot facility. Three boundaries must be reconciled before the design is releasable: filesystem-visible cached/mapped bytes in the kernel; installed adapter operations and any pending transfer buffers; and host-installed metadata/payload. Host-authoritative installation resolves the second-to-third boundary for acknowledged ordinary operations. It does not by itself make dirty writable mappings visible or prevent them changing while a snapshot is acquired.

The required G1 outcome remains: one consistent captured filesystem view under the existing supported syscall/mapping semantics, no global operation freeze, no whole-cache drainage at acquisition, no silent removal of mmap or change in write visibility. Application-private buffers are outside filesystem state; kernel-visible bytes cannot simply be relabelled application-private. Until a supported mechanism proves this boundary, public full-surface snapshot Commit is NOT specified as implementable or qualified. Ordinary host-installed operations can be reasoned about with the root model, but that restricted proof must not be advertised as fulfillment of the full contract.

V1 cannot close until this specification names the selected kernel/adapter mechanism and exact ordering/error semantics, binds negotiated capability evidence, and defines an independent overlap oracle. If no mechanism satisfies these constraints, report the incompatibility for an explicit owner decision; do not choose direct-I/O-only operation, implicit fsync requirements, older host-only snapshots, or pause-and-drain as an undocumented fallback.

### Resulting additions, replacements, and preserved components

Add a host-authoritative root bundle, snapshot-owned reader, disk-backed page/extent/ownership catalogs, prepared installation ownership, and bounded reclamation/relocation. Replace full dirty-fact export as Commit input, container-only acknowledged authoritative buffers, and whole-segment/Arc-only ownership assumptions. The current immediately-unlinked-file-per-segment organization becomes a bounded arena-file organization for this selected model.

Preserve stable live inode/handle semantics, prepare/acquire/apply validation, compact and range-based file representation where it satisfies incremental metadata access, shared payload batching, host Store placement, bounded transport admission, and the existing canonical candidate builder. Do not delete cache reconciliation, fsync, SDK-coherence or explicit end/discard paths merely because Commit stops using freeze: each still requires its own supported operation semantics. Canonical content format, explicit Store compaction policy, and branch history format are unchanged.

## 3. Commit attempts, staging, and continuation

This section specifies the resulting Commit contract. It does not specify a task sequence or authorize a new committed data format. The current Store admission/staging/publication implementation is reused; live checkpoint installation is removed from ordinary Commit. Requirements remain subject to the explicit kernel visibility and snapshot-storage gates in the governing rules.

### Owned contexts

The following are logical records. Variable-size members are bounded handles to disk-backed indexes/journals, not unbounded in-memory maps. They do not require a new persistent schema or a new Rust trait hierarchy.

| Record | Required fields | Ownership and mutability |
| --- | --- | --- |
| `LiveState` | Current root bundle; live inode identities; handle state; current mutation/change-index position; content provenance/backing handles | Filesystem authority owns it. Only filesystem mutations and logically equivalent backing substitutions change its visible state. Commit builder never borrows it. |
| `PublishedContext` | Workspace and branch identity; last resolved expected head, canonical root and base Layer; covered snapshot boundary; current canonical correspondence index | One current comparison context per Workspace. Replace atomically on resolved publication; do not append Workspace history. |
| `CommitAttempt` | Attempt identity; Workspace/branch ownership; immutable copy of `PublishedContext`; acquired snapshot handle/boundary; admission ownership; optional candidate root/stage identity; result correspondence handle; known/uncertain outcome; cleanup ownership | One unresolved attempt per Workspace. Immutable captured input and expected context cannot be changed by retry. State transitions are owned by the attempt coordinator. |
| `QueuedCommit` | Request identity/completion destination and bounded request metadata | No snapshot, candidate, live map clone, or payload retention until execution starts. |
| `PublishedReceipt` | Exact attempt identity, verified `Committed` or `UpToDate` outcome, resulting canonical context, covered boundary and correspondence handle | Applied once to `PublishedContext`; retained until outcome delivery and cleanup obligations are resolved. Logical runtime bookkeeping, not a Workspace checkpoint. |

Attempt identity is an internal correlation token. It is not a Workspace revision, branch Commit ID, or promise of restartable execution. Existing deterministic Commit identity continues to derive from canonical root, parent Commit and base Layer.

`PublishedContext.canonical_root` and the base roots in `LiveState` file ranges MUST be distinct concepts. The former is the publication predecessor; the latter describe where current bytes come from. They can differ after the first successful Commit. Passing a live piece's base root as Store `expected_root` is forbidden.

### Entry and serialization contract

`begin_commit(workspace)` obtains the next per-Workspace attempt slot, snapshots the current published context, and acquires one consistent filesystem snapshot through the designated snapshot provider. Acquisition occurs when the queued request starts, not when it was queued. It atomically registers snapshot ownership relative to mutation installation and reclamation. If acquisition fails, no partially usable attempt or leaked pin becomes visible.

Only one unresolved attempt, including staged conflicts and uncertain outcomes, may occupy the slot. Queue capacity and queue-full behavior are explicit configured bounds. Waiting for the slot MUST NOT retain a live-operation admission guard, Workspace mutex, or snapshot. No global construction lock is introduced across Workspaces/branches/LayerStacks; existing branch lease admission is preserved.

Once acquired, the coordinator releases live-state synchronization. Construction, admission, stage retention, conditional publication, result delivery and cleanup MUST permit ordinary reads, writes, namespace mutations and supported SDK operations to complete. The per-Workspace attempt lock is not used by ordinary filesystem operations.

The snapshot reader supplies captured inode/binding/range lookup and captured-change enumeration. It never substitutes a live lookup for missing captured data. Namespace records, attributes, bytes, and change-index views all belong to the same boundary.

### Attempt state machine

```text
Queued -> Acquiring -> Building -> CandidateReady -> Staged -> Publishing
             |           |                                  |
             |           +-> BuildFailed                    +-> Published
             +-> AcquireFailed                              |       |
                                                            |       v
                                                            |  ReceiptApplied
                                                            |       |
                                                            |    Cleanup -> Done
                                                            |
                                                            +-> ConflictRetained
                                                            |       |
                                                            |  resolve/abandon same candidate
                                                            |
                                                            +-> OutcomeUnknown
                                                                    |
                                                              inspect SAME attempt
                                                                    |
                                                        known published / unpublished
```

Acquire/build failure leaves the live root unchanged by the failed Commit. Partial admitted objects are managed through existing admission ownership. A build failure may release the snapshot and terminate that request, or retry construction against the same retained snapshot if policy allows. A new snapshot is a new attempt, never an implicit retry of an older attempt.

`CandidateReady` means the canonical root and necessary result correspondence are complete. `Staged` means its canonical objects are independently retained by the Store stage. `Published` means publication is known to have succeeded, not merely that the RPC was sent. `ReceiptApplied` changes only the published comparison/coverage context. Cleanup failures remain separately charged and retryable; they do not turn a known successful publication into a failed branch update.

An unresolved cleanup handle may be moved to a bounded cleanup queue once outcome and coverage are resolved, allowing the next attempt to start if resources permit. It must not occupy an unbounded secondary attempt queue or lose ownership charges. A retained staged conflict or unknown publication outcome is not resolved by moving it to cleanup.

### Canonical construction and shared Init machinery

Construction consumes `(snapshot_reader, captured_changes, expected_comparison_context)` and produces `(candidate_root, admission_handle, result_correspondence, metrics)` through the existing candidate machinery.

Preserve the shared Init/Commit producer/output driver and canonical object/admission machinery where currently shared. Preserve existing small-file FULL/DELTA eligibility, predecessor selection and chain rules; large-file CDC/extents and incremental rope updates; CAS authentication/identity, deduplication and packing/compression. Do not create a second snapshot encoder or duplicate admission engine. Init still uses its own source/task/initial namespace planning. Workspace Commit uses captured changes and incremental namespace planning; it does not import Init's source-discovery scan.

Canonical construction is per distinct changed inode, not per hardlink name. Namespace and reference changes are derived from the captured binding deltas against the expected canonical predecessor. Unchanged content and unaffected tree structure are reused. Optional existing precomputation is accepted only if tied to exactly the snapshot content; invalidation/join behavior must not make ordinary mutations wait for Commit.

The existing checkpoint-result journal may supply bounded live-to-canonical inode/content output. Its name and prior consumer do not justify installing it into live files. Result correspondence and its publishable index root MUST be complete before the candidate is staged. CandidateReady includes that readiness. Post-publication receipt application is bounded root/context replacement only; no hidden million-record correspondence finalization is permitted after publication or before the next attempt. Correspondence failure before staging fails that attempt safely; publication uncertainty never authorizes a duplicate Commit or destructive live reset.

### Staging and conditional publication

The current `workspace_stages` schema remains one row per Workspace: `(workspace_id, branch_id, candidate_root)`. It is a canonical retention root, not another filesystem tree or history Commit. Attempt-boundary and outcome bookkeeping remains attached to the owned runtime attempt unless a later durability contract explicitly requires persistence.

The required publication sequence is:

1. Complete canonical construction and admit remaining candidate objects under the attempt's Workspace admission owner.
2. Stage the exact candidate root for the attempt's Workspace/branch. An existing identical stage may be reused; a different retained stage is an error and must not be overwritten.
3. In a short Store transaction, validate exact stage identity, expected branch head/base, LayerStack ownership, and expected canonical predecessor root.
4. If the canonical root and base are unchanged, resolve `UpToDate` and delete the exact stage transactionally. Otherwise derive/insert the canonical Commit, conditionally advance the branch, and delete the exact stage in the same transaction.
5. Report the known outcome, then atomically apply its published-context receipt without altering live filesystem state.

Object admission and construction MUST NOT hold the Store publication transaction open. Existing bounded admission transactions and writer serialization are retained; they are not a license to hold a global writer across the complete build. Stage removal does not delete objects newly referenced by branch history.

A verified `UpToDate` outcome advances the covered snapshot boundary and appropriate correspondence to the captured state, while leaving the branch head unchanged. This prevents repeated processing of changes whose final effect equals the predecessor. Post-snapshot changes remain pending.

### Conflicts, retry and uncertainty

| Situation | Required action | Forbidden action |
| --- | --- | --- |
| Build fails before stage | Preserve live state, resolve partial admission ownership, retry same snapshot or end attempt according to explicit policy | Retain an unbounded failed-candidate history; reset the live overlay |
| Stage contains another root | Report retained-attempt conflict; preserve exact stage | Overwrite stage or silently discard its candidate |
| Expected head/base/root moved | Retain identifiable candidate and attempt; allow live operations; require explicit conflict handling | Rewrite attempt's parent/base and publish it silently; block all Workspace mutations |
| Transient publication failure, known not published | Retry exact candidate/stage and expected context, validating conditions again | Rebuild candidate from newer live state |
| Publication outcome unknown | Keep attempt slot/context and inspect authoritative Store state | Infer success from absent stage alone; start another attempt with speculative coverage |
| Known published, cleanup fails | Apply or retry idempotent receipt; retain charges; retry cleanup | Republish candidate, install old snapshot in live inodes, or report branch rollback |

Uncertainty resolution must establish a publication witness, not merely object existence. For `Committed`, derive the exact expected Commit identity and validate its record and successful placement in the relevant branch history under the preserved lease/publication model. A Commit row existing somewhere in the Store does not alone prove this branch accepted this attempt. Branch advancement by another authorized actor after publication must be distinguished from non-publication. If evidence cannot resolve the outcome, keep it explicitly unknown.

`UpToDate` has no new Commit identity. Resolution may safely repeat the no-op conditional verification when expected context still holds; it must not infer success from a missing stage after the branch has moved. The Store-level observation/retry contract for this case must be specified before implementation claims idempotent completion. No new crash recovery guarantee is introduced.

Conflict reconciliation is a distinct explicit operation. It must identify the retained candidate and changed branch context and define how any reconciliation result relates to the live state that has continued changing. Ordinary retry is not automatic merge/rebase. Existing reconciliation paths that invalidate on live mutation or refresh presentation cannot be reused unchanged. The specification must not promise transparent conflict resolution until its overlap semantics are defined.

### Coverage and successive Commits

At acquisition, an attempt captures both its starting covered boundary `b0` and ending snapshot boundary `b1`. Construction enumerates relevant final effects after `b0` as visible at `b1`, including removals and inode/namespace changes. `PublishedContext` advances to `b1` only after verified publication or verified `UpToDate`. A failed/conflicted/unknown attempt does not advance coverage.

```text
Published context: C0, covered b0

S1 acquired at b1: A
Live write x installs after b1
S1 builds/stages/publishes C1: A
Receipt: predecessor becomes C1; covered becomes b1
Live remains A + x

S2 acquired at b2
Enumerate effects (b1, b2] from S2
Build against C1 -> C2 contains A + x if x remains effective
```

The selected captured-change layout uses two immutable indexes in every root bundle:

```text
change_by_key:      affected_key -> latest_sequence + final_effect
change_by_sequence:(latest_sequence, affected_key) -> captured entry/reference
```

Installation removes the affected key's previous secondary entry, updates its
current effect, and inserts the new secondary entry atomically with inode/binding
changes. Capture retains both roots. Enumeration scans `(b0,b1]` in the captured
secondary index and resolves effects from the same captured root. A repeated key
has one current tracking entry; older snapshots retain their own index pages.
Cleanup of covered entries is conditional on the current key sequence and runs
under bounded work; the new covered watermark alone lets C2 skip old tracking.

Overlay binding tombstones are distinct from change-index tombstones. If the live
immutable backing contains `/a`, a deletion masking `/a` remains in BindingRecord
even after C1 publishes that deletion. Removing covered ChangeRecord bookkeeping
does not authorize removing that mask and exposing `/a` again. For a transient
name absent from live backing, its overlay entry may disappear, while a later
ChangeRecord removal still conveys deletion from a preceding canonical Commit.

The change index is current-state bookkeeping, not an append-only operation log. Repeated changes to a key may replace its current tracking entry, but the captured index must preserve the version needed by an active snapshot. Entries/tombstones that represent final differences from the published predecessor cannot be removed merely because their live namespace binding is absent.

Exact edge requirements:

- Create before S1, unlink during its build: C1 contains the created inode; C2 removes its binding and updates references correctly.
- Unlink before S1, recreate same name after S1: C1 lacks the binding; C2 uses the new stable inode identity, not resurrected identity from the old path.
- Rename/hardlink during S1 build: C1 contains captured bindings; C2 atomically applies newer binding effects against C1, once per inode content.
- Revert before S2: C2 represents final S2 state, not a promise to retain the intervening action.
- Open-unlinked content remains live for handles but is absent from snapshots' committed namespace unless still reachable by another binding.
- Cleanup of covered tracking must be conditional on key revision/ownership, bounded, and preserve a newer entry for the same key. Global `dirty.clear()` is forbidden.

A filter over every historical dirty key does not provide incremental enumeration. Work must be tied to relevant current changes and necessary tree paths, independent of already-covered historical prefix size under the declared workload. It is acceptable to visit a changed key whose final effect is a no-op; it is not acceptable to repeatedly reconstruct all changes since Workspace creation.

### Canonical range correspondence: required semantics

The existing builder assumes a changed large file's base root matches the preceding canonical content root. Without live checkpoint installation, that assumption can fail after C1. Changing expected head/root alone does not restore incremental CDC.

The resulting design MUST supply a bounded, snapshot-consistent correspondence facility with these semantics:

- Identify the canonical inode and content root produced for captured file state, including newly created live inodes and hardlink identity.
- Preserve enough range provenance or equivalent incremental comparison information to relate S2's current ranges to the canonical bytes committed from S1, even if both still refer physically to an older base or spool.
- Return exact predecessor-relative unchanged spans and replacement spans, or another input accepted by the existing incremental content engine that has equivalent semantics. Length-shifting insertions/deletions, sparse zeros, truncation and changes crossing representation thresholds are included.
- Use current captured state, published correspondence and retained indexed backing; do not require an unbounded operation log, all-piece vector, or full predecessor/file scan to reconstruct a localized later edit.
- Share unchanged correspondence structure and reclaim superseded correspondence when no active owner needs it. One root handle is not proof of bounded underlying retention.
- Never rewrite live file contents, offsets, inode identities or current namespace simply to align them with a predecessor. Equivalent physical backing substitutions are permitted under the storage ownership contract.

The selected metadata-only descriptor and interval-index contract follows below. It removes the need for a second canonical chunk tree or retained raw predecessor spool. Its ordering, duplicate handling, builder adapter, and actual indexed complexity remain V3 proof obligations; a canonical inode-to-root table or mutation counter alone is insufficient.

Required evidence is large C1 followed by small C2 and repeated cycles, with full-file comparison/reconstruction counters, CDC-scanned bytes, visited ranges and temporary retention. Correct output obtained through fallback whole-file reconstruction does not satisfy localized continuation. Existing small-file full-input encoding and legitimate representation-threshold transitions remain allowed and separately measured.

### Selected predecessor description and indexed correspondence

Retain one latest published metadata-only file description per stable inode. Its
header is `(stable_inode_id, canonical_content_root, logical_length,
description_root, provenance_index_root)`. Active attempts may retain earlier
metadata roots under ordinary ownership budgets, but there is no per-Commit list
of Workspace file versions. Unchanged inodes share descriptions; changed inodes
replace theirs; unreachable deleted inodes leave the current correspondence index.

The normalized descriptor stream is ordered by canonical logical offset:

```text
(canonical_offset, length, origin_identity, origin_offset, kind)
```

For nonzero ranges, `origin_identity` is immutable logical occurrence lineage,
not merely a content hash, physical offset, or canonical root. Initial file ranges
have stable origin lineage; splitting/trimming retains origin and coordinates.
New byte coordinates are never reused within an origin. Ordinary contiguous
additions may extend an existing origin into never-before-used coordinates, allowing
sequential appends to coalesce. Duplicating an existing coordinate occurrence gets
fresh lineage even if its bytes or physical backing match the old occurrence. A physical relocation or equivalent canonical backing
substitution preserves that lineage. Within one file description, intervals for a
nonzero origin are disjoint; duplication of an occurrence creates fresh lineage
for the inserted occurrence. This is representation bookkeeping, not user-visible
history. IDs are never reused within any retained comparison lifetime.

Identical nonzero origin coordinates imply identical bytes. Descriptors contain
identities, not payload ownership: old bytes remain readable at the corresponding
canonical content root and logical offsets after obsolete spool storage is freed.
Adjacent descriptors coalesce when both provenance and coordinates are compatible.
Inline bytes without exact stable lineage are replacements; an unchecked hash is
not accepted as an equality proof.

Zeros use explicit zero descriptors and may coalesce without retaining a write
history. Semantic zero equality cannot greedily consume a distant predecessor run
past a surviving nonzero span. Cursor-only matching is also insufficient: deleting
one byte before a huge unchanged zero run would reconstruct the entire run. The
selected zero algorithm is the anchor-bounded metadata alignment below; it needs
no per-write zero lineage or optimal byte sequence matcher.

The predecessor has two disk-backed bounded-cache indexes: its canonical-offset
ordered descriptor stream, and `(origin_identity, origin_start)` for nonzero source
intervals, with interval end and canonical offset as values. Because intervals are
disjoint for each origin in the captured file, overlap queries require one floor/
lower-bound seek and forward iteration to the query end. They do not enumerate all
identical-content occurrences or require a multidimensional duplicate search.

First stream current nonzero descriptors and query exact predecessor origins.
Split at overlap boundaries and retain matches only in increasing predecessor
canonical order, trimming already-consumed prefixes. Write the selected nonzero
anchors `(old_offset, new_offset, length)` to a bounded-buffer temporary journal.
Do not let zero runs advance this predecessor cursor. Ordinary insert/delete/
overwrite preserves the order of surviving occurrence lineages; fresh inserted
lineage cannot steal a match from retained content. Backward-only matches from a
genuine reordered occurrence become replacement input and are explicitly counted.

Second, stream the anchor journal with virtual start/end anchors at `(0,0)` and
`(old_length,new_length)`. Each pair of neighboring anchors bounds one old gap and
one new gap: start at the preceding anchor's end and stop at the next anchor's
start in each coordinate space; virtual anchors have zero length. Emitted base
spans must be disjoint and monotonically increasing, and the verifier checks this
directly. Within that old gap only, stream predecessor descriptors to locate
zero runs. Match each zero run in the new gap against the remaining old zero runs
in order, skipping deleted nonzero regions, clipping to the gap bounds, and
splitting spans as needed. Unmatched new zeros and new nonzero gaps are replacement
input. A zero match never crosses the next selected nonzero anchor. Emit the
completed predecessor-relative plan as a bounded stream/journal; do not retain an
all-piece anchor or candidate array in RAM.

This is two bounded metadata passes, not payload lookahead. Old gaps are disjoint,
so their descriptor traversal totals at most Po (apart from explicit index boundary
seeks). New gaps/anchors likewise cover current descriptors once per pass. It
preserves both zero-prefix insertion and deletion before a large zero region:

```text
Published C1: [A:0..100][X:0..10][A:100..200]
Canonical:      0..100   100..110    110..210

S2: [A:0..50][new Y:0..3][A:50..100][X:0..10][A:100..200]
Plan: reuse C1[0..50], insert Y, reuse C1[50..210]

Adversarial zero insertion:
C1: [A: large unchanged range][zero:1]
S2: [new zero:1][A][zero:1]
Plan: insert zero in the empty old gap before anchor A; reuse A and trailing zero.

Adversarial deletion before zeros:
C1: [A:1 byte][zero:L][B:1 byte]
S2: [zero:L][B]
Plan: next nonzero anchor is B; old gap contains A then zeros.
      Skip deleted A, reuse old zero:L, then reuse B. No CDC of unchanged L.
```

The resulting transient Commit-only file view uses C1 as its base, increasing
predecessor spans, and snapshot-owned replacement/zero readers. It feeds the
existing incremental content engine through bounded cursors and is never installed
in live state. Descriptor output for the next successful Commit records S2's actual
logical provenance, not a replacement of that provenance with transient C1 offsets.

For `Po` predecessor descriptors, `Pn` current descriptors and `J` emitted
overlap/zero-plan fragments, matching has target O(Pn log(Po+1) + Po + J) metadata
work including the anchor-bounded zero pass. Index construction is
O(Po log(Po+1)) unless bulk ordering permits less. Anchor/plan disk records and
all descriptor/index-boundary visits are counted. These are metadata costs, not O(U) byte
costs. Every examined interval is counted; the disjoint-origin invariant is validated
at descriptor construction, so a hidden duplicate-candidate scan violates this
selected algorithm. Streaming RAM is bounded by page/cache/cursor allowances,
not Po/Pn/J-sized vectors. External plan/index storage is quota-charged.

This deliberately accepts changed-file metadata traversal. Shared-subtree skipping
is optional; an elaborate canonical chunk correspondence tree is not required.
Local edits must not trigger full payload hash/CDC solely because their live source
base differs from C1. Full small-file handling and genuine format transitions retain
their existing explicitly counted behavior.

V3 remains unqualified until the cursor adapter and provenance invariant are proved
through source-allocation reuse, equal-content new occurrences, partial overlap,
zero-prefix insertion, deletion before huge zeros, shifted offsets, delete/truncate/append, inode recreation,
physical substitution, and repeated large-C1/small-C2 cases. No unchecked provenance
inference or full-payload fallback may be labelled a locality pass.

### Explicit End, Discard and attempt cancellation

End/Discard is a separate user-authorized lifecycle operation. It may stop further operations under its existing contract; Commit may not borrow this path as error recovery.

Before publication begins, cancellation can terminate construction, release/abort admission or the exact stage as applicable, and release snapshot ownership after workers stop accessing it. Abandoning a candidate does not itself discard live state. Preserve the original published coverage so a later fresh attempt includes all still-effective uncommitted changes.

Once publication is in flight, cancellation cannot claim rollback. Resolve the transaction's outcome first. A Commit that already published remains in branch history even if the Workspace is then discarded. Unknown outcome remains an owned unresolved obligation until it can be resolved; do not delete its stage/evidence prematurely.

End/Discard coordinates the attempt coordinator, workers, current backing, retained readers and open handles. Physical backing is reclaimed only after all relevant owners release. Explicit API behavior for Commit-on-End must use the same snapshot/stage/publication contract and distinguish publication success from subsequent end cleanup failure.

### Existing code disposition

| Existing code | Resulting responsibility |
| --- | --- |
| `lifecycle.rs::commit_workspace_session_with_status` | Replace freeze/quiesce/live-lock build with owned attempt coordination. Preserve result/error translation and telemetry with new phase meanings. |
| `Workspace::commit(&mut self)` | Extract independent construction/publication inputs; no mutable live borrow for build duration. Reconciliation remains explicitly distinct. |
| `pending_stage`, `pending_publication`, `pending_checkpoint` on Workspace | Relocate attempt-owned stage/outcome/correspondence state into `CommitAttempt`; stage/publication no longer implies filesystem inactivity. |
| `ensure_active` | Check actual Workspace lifecycle/operation validity, not ordinary staged candidate or pending publication. |
| Ordinary `install_checkpoint`, `finish_checkpoint`, remote checkpoint/resume | Remove as successful Commit obligations. Preserve separate operations only where their callers still require explicit live replacement. |
| `base_root`, `base_inodes`, `reader`, expected head/base | Separate live provenance/read access from published canonical comparison context. Do not retarget every live inode on publication. |
| `changes.rs::CandidateInputs`, `StableFileInputs` and remote facts borrow | Adapt to owned snapshot access and bounded cursors; remove builder-duration live backing lock and all-facts canonical-map reconstruction. |
| Checkpoint/result journal | Retain useful bounded output as canonical correspondence; remove global live-install consumer. |
| `LayerStackStore::commit_workspace_candidate` | Preserve admission ownership validation and conditional staging/publication; permit exact staged-candidate retry without rebuilding current live state. |
| `staging.rs` single-row insert/identity/delete | Preserve strict identity and atomic retirement; no multi-stage schema needed for one unresolved attempt. |
| Shared Init/Workspace output driver, encoders and admission | Preserve common machinery, format/authentication and existing optimized paths; keep entry-specific task/namespace planning. |

This is a contract-level refactoring boundary. It does not authorize deleting shared pause/cache-control/reconciliation helpers without tracing their non-Commit callers, nor does it prescribe a new service/thread/crate for each logical record.

## 4. Performance, locality, and resource contract

### Observable operation boundaries

Commit MUST produce an owned immutable input before constructing canonical output.
Performance observability MUST expose these logical events, without adding a public
round trip solely to obtain them:

| Event | Definition |
| --- | --- |
| request accepted | Public Commit entered per-Workspace request admission. |
| attempt starts | Prior attempt is resolved; the request owns the one allowed active attempt slot. |
| snapshot acquisition starts | First work necessary to obtain the consistent captured view, including protocol coordination and retention registration. |
| snapshot acquired | Builder can read all captured state independently and retention is registered; no subsequent prerequisite full export, pin walk, or flush remains. |
| candidate staged | Canonical objects have been admitted and exact candidate root is retained in `workspace_stages`. |
| publication resolved | Created/UpToDate/conflict outcome is authoritative, or explicitly unresolved. |
| public acknowledgment | Full promised API outcome has returned; publication/required bookkeeping time stays included. |
| ownership released / cleanup settled | Snapshot and attempt-owned resources are respectively released and physically reclaimed under the declared policy. |

`commit_api_ns` covers request entry through public acknowledgment, including queue
wait. `snapshot_acquire_ns` covers the complete acquisition interval, including
container/host handoff and any required visibility coordination. Queue wait is
reported separately, never disguised as acquisition or subtracted from API time.
Construction, staging, publication, post-publication bookkeeping, and cleanup are
separate attributed phases. Overlapping phase sums are not treated as serial wall.

The snapshot handle is complete only if reads cannot require a later unbounded
export of live facts. Root pin time alone MUST NOT be reported as acquisition.
Buffered captured bytes may remain in retained buffers; acquiring the snapshot
must not make them depend on later mutable state or force all buffers to disk.

### Interface shape and boundedness

The implementation MUST expose the following semantics to the existing builder;
these are contracts, not required public method names or a new trait hierarchy:

| Interface | Result/ownership | Complexity obligation |
| --- | --- | --- |
| acquire snapshot | Owned stable root bundle, readable backing lease, capture identity, expected branch comparison context | Constant-sized work/storage independent of N/D/P/payload. No dirty-map clone, complete cache drain, or whole-prefix fact export. |
| snapshot inode/binding lookup | Captured record or absence | Indexed lookup through bounded cache; no live fallback. |
| snapshot file-range reader | Bounded cursor over exact captured ranges | Work follows requested ranges and index paths; never materializes all P pieces for a bounded read. |
| captured-change iterator | Distinct affected inode/binding keys including removals, relative to covered successful state | Can skip covered records using an index, not filtering a full historical scan. Bounded batches/cursor memory. |
| canonical-result correspondence | Stable inode identity and necessary range provenance linked to exact captured content/output | Disk-backed/bounded, immutable attempt ownership. Identity mapping alone is insufficient if range base differs from the comparison Commit. |
| successful coverage acknowledgment | Covers exactly the published snapshot; preserves newer changes | No scan/reset of all live dirty records or inode backing installation. |
| release snapshot | Releases root/backing ownership | No synchronous graph-wide destructor in foreground; deferred work remains charged and bounded. |

Mutation installation updates current state and current change tracking atomically.
It MUST NOT create a durable checkpoint, append an unlimited operation log, or run
the complete CAS pipeline after every small edit. Current inode IDs and canonical
correspondence must support new files and aliases through successive Commits.

### Locality shared with existing Init/Commit storage

Init and Commit MUST retain the same existing canonical format, authentication,
deduplication, small-file FULL/DELTA selection/chain rules, large-file CDC/extent
representation, compression/packing, and bounded object-admission engine. A second
encoder or alternate benchmark-only format is forbidden. Snapshot metadata remains
private temporary state; it is not a new canonical format.

Init legitimately enumerates and reads its input source. Incremental Commit MUST
NOT import/scan the entire FUSE tree merely to discover modifications. Sharing
canonical builders does not require sharing full-source enumeration. Ordinary
Commit reads captured dirty-inode and directory-binding frontiers and reuses
unaffected canonical objects/structure.

Small-file DELTA physical output does not imply O(changed bytes) logical input
work: existing construction may read the complete file and predecessor, bounded
by the small-file threshold (currently 131072 bytes). Large-file incremental
replacement scans replacement mappings and splits/concatenates unchanged extents;
whole-file CDC is not required merely because a snapshot exists. Threshold-crossing
and other existing full-build cases remain explicit, counted, and separately tested.

The G3 continuation contract is mandatory: after C1, live ranges may still refer
to older immutable roots. C2 MUST use current captured effects and valid C1
comparison/canonical correspondence without resetting live contents. Merely
advancing expected branch head, changing a base-root field, or preserving an
inode-to-root table does not prove this contract. Exact range provenance or an
equivalent incremental mechanism must prevent unsupported predecessor mismatch
from turning each subsequent small edit into full-file hashing/reconstruction.

### Required removals/adaptations and retained machinery

| Existing cost/assumption | Required resulting behavior | Reuse/evidence |
| --- | --- | --- |
| Detached map cloning | Owned snapshot reader, no captured-node map proportional to D | Semantic detached test is useful but `changes.rs:3548-3564` is not fast acquisition. |
| Scan all materialized nodes to reserve new inode serials | Allocation high-water or equivalent bounded identity bookkeeping | `changes.rs:563-593`; retain stable inode reservation semantics. |
| Rename rewrites paths for all materialized nodes | Local identity/binding representation or indexed affected-path processing; audit reporting/reconciliation consumers | `namespace.rs:776-794`; no claim of locality until unrelated-node visits removed. |
| `pieces()` creates O(P) Vec | Bounded piece/range cursor where P can exceed transient allowance | `file_edit.rs:1101`; preserve shared split/merge and compact forms. |
| Generation-equal live checkpoint and global dirty clear | Snapshot-bound coverage receipt; no published-state installation into live files | Reuse useful bounded checkpoint-result correspondence, not live-reset obligation. |
| Old base mismatch invokes full `file_matches` and full build | G3 incremental correspondence preserving later-range changes | `changes.rs:1843-2014`; count these fallbacks explicitly. |
| Full host facts export while live owner frozen | Snapshot-addressable backing continuously satisfies chosen installed-state contract | No delayed full-prefix export after acquiring a nominal handle. |
| Shared lifecycle/backing lock across build | Snapshot-owned construction inputs, bounded shared service critical sections | Preserve normal mutation revalidation/admission. |
| Existing frontier/sorted updates | Retain distinct-inode tasks, binding deltas, reference accounting, tree sharing | `changes.rs:699,850`; `tree/batch.rs:1364`. |
| Existing bounded content pipeline | Retain worker/task journals, output admission, CAS/FULL-DELTA/CDC | `objects.rs:4473`; predecessor worker cap is measured, not silently widened. |

### Complexity contract

Use N total names, Nc materialized nodes, D changed inodes, K affected binding/
attribute records, A known aliases, L path/name bytes processed, P file pieces,
T intersected pieces, Hm/Hp/He actual metadata/piece/extent heights, U new bytes,
R total content bytes read, C CDC-scanned bytes, I canonical objects handled,
E affected metadata pages, B page size, W active builders/workers, and Q admitted
requests. Use actual heights until a balancing proof supports logarithmic bounds. Let r be actual mutation preparation/install attempts, including root conflicts; never omit the retry multiplier or abandoned allocation work.

| Operation | Bound/target | Required caveat |
| --- | --- | --- |
| Local file mutation | Affected paths/pieces/bytes: Hm + Hp + T + U plus required A/L | No whole-file or whole-fragmented-record serialization. Copy-on-write disk bytes approximately U + E*B plus allocator slack. |
| Namespace mutation | Affected keys/paths, approximately O(K*Hm + L) under indexed parent/name design | Large semantic operations can have large K; unrelated Nc traversal is not necessary affected work. |
| Snapshot acquisition | O(1) descriptor/registration work and RAM | Complete handoff included; wall latency includes contention, not mathematically constant. |
| Captured changes | O(D + K) outputs plus index traversal | Repeated passes counted; no scan of all historical keys. |
| File construction | O(P traversal + R + C + affected extent nodes + codec/object work) | R/C overlap as byte categories; do not sum as disjoint physical I/O. Small-file build can read entire threshold-bounded file. |
| Metadata construction | D/K plus canonical nodes read/rewritten | Include sorted-update fallbacks and spill/merge overhead. |
| Canonical admission | I and encoded bytes through bounded cohorts | Include Store transaction/queue waits and W-buffer aggregate. |
| Reclamation | O(G) resource visits for G reclaimed/inspected resources | Can be deferred; charges persist and backlog cannot grow without declared bound. |

All RAM structures, including indexes and ownership catalogs, MUST fit explicit
host/container budget vectors. Q/W times their buffers are included. Storage
ownership pins do not pin pages resident. A million-file capture cannot allocate
a million-entry ownership registry before or after the measured acquisition.

Temporary storage buckets MUST be disjoint: current payload; current metadata/
index/ownership allocations; additional snapshot/reader-only backing; construction
scratch; and allocation/deferred-cleanup slack. Shared allocations counted once.
Store duplication during canonicalization is separately reported; repeated Commits
must have bounded duplicate retention through safe substitution/reclamation.

### Performance evaluation: existing benchmarks first

Owner decision, 2026-09-14: implement the specified behavior correctly, then run
against the existing sophisticated benchmark suite and inspect the actual results.
This specification sets no new numerical performance pass/fail threshold, capture
latency ceiling, fixed sample population, or mandatory new throughput family.
The earlier proposed capture targets and SW/MW schedules are withdrawn.

Correctness, non-pausing operation, localized state access, and bounded resource
ownership remain product requirements. They do not depend on inventing a new
millisecond target. Existing benchmark identities, fixtures, operation surfaces,
verification, timing boundaries, statistical rules and any established acceptance
criteria remain authoritative when those benchmarks are used; this document neither
repeats them as new requirements nor waives them.

| Evaluation area | Use of existing benchmarks and evidence |
| --- | --- |
| Ordinary operations | Run applicable existing SDK/FUSE edit/read/append/namespace workloads; compare actual wall, CPU, throughput and resource costs. |
| Init and Commit | Reuse existing Init/Commit families and shared-pipeline checks; distinguish snapshot acquisition, construction, admission, publication and cleanup diagnostically. |
| Successive Commits | Reuse existing sequence/history workloads to inspect C1/C2 locality and output correctness; identify any missing coverage before adding a focused case. |
| Foreground during Commit | Keep the required correctness overlap proof. Reuse suitable existing workloads for observation; only extend missing overlap coverage narrowly, without creating a new benchmark framework or numerical gate now. |
| Storage and memory | Record existing RSS, spool, Store, cache/index and cleanup evidence; retain required resource-limit and failure-correctness tests. |
| Scale | Reuse applicable capacity infrastructure for the already-required #123 scale proof. No new repeated million-file sampling campaign is mandated here. |

Record the mechanism counters needed to explain results, especially host RPC cost,
COW/ownership writes, retries, acquisition work, large-file fallback, and reclamation.
Use the existing reporting/telemetry facilities; missing metrics are unavailable,
not fabricated zeros. Observe complete workload time as well as per-call latency
so a systematic overhead cannot be hidden by a small per-call noise allowance.

Do not design to arbitrary unmeasured constants or select an implementation solely
to satisfy a newly invented threshold. Retain actual improvements, regressions,
misses and failures; use measured evidence to decide what needs optimization.
Any later proposal for a new performance gate or benchmark contract is an explicit
separate decision. New correctness cases still require clear fixtures/oracles, and
new benchmark extensions still follow existing benchmark policy. Nothing here
runs benchmarks or starts implementation as part of document editing.

### Required fixture shapes and receipts

1. Large unchanged namespace plus a fixed small changed subset: report N/Nc vs D/K,
   namespace nodes visited, alias/path work, identity reservation visits.
2. One Workspace with at least 1,000,000 changed regular files before one final
   explicit C1: fixed byte recipe, hierarchy, quota, RAM, deadline, verification
   coverage, and distinct-inode count. No intermediate Commit or Workspace split.
3. Continue the same Workspace after C1; modify ten fixed files including an
   already-edited large file; C2 must preserve exact new bytes and reuse C1 without
   full-old-base hashing/reconstruction caused by missing correspondence.
4. Hold builder after acquisition for correctness only; complete read/write/rename/
   unlink and SDK edits with same handles; then release, publish and verify C1/C2
   after fresh Store reopen. Timing runs use natural overlap without test hold.
5. Repeat same-range overwrites with snapshot and retained-reader lifetimes;
   distinguish necessary old bytes, unrelated obsolete data, physical segment
   slack, and cleanup delay. Disk-only pages must actually spill/reload under small
   declared caches; no arithmetic-only capacity proof.
6. Threshold cases around 131071/131072/131073 bytes and large-file edits; keep
   canonical-format/oracle policy unchanged across Init and Commit.

Required receipts: full API/phase boundaries, snapshot RPC and lock-held/wait time,
nodes/pages enumerated/copied, payload-copy bytes, dirty-key visits, content tasks,
base-match/full-hash/full-build reasons, predecessor/CDC/equality bytes, piece cursor
peak, worker/queue bytes and producer blocking, Store writer/admission waits,
host/container RSS and accountable reservations, FD/catalog count, physical
current/shared/retained/scratch/slack storage and cleanup backlog. All performance
receipts exclude added benchmark digest/oracle/reopen/fault-injection work.

## 5. Resulting interface and component shape

The names below specify internal responsibilities and record contents, not a new
public snapshot API or a required trait/service/thread per row. Preserve public
Workspace Commit and SDK file-edit surfaces, including same-file batch scope.
Ordinary Commit acknowledgment still includes authoritative publication outcome
and required coverage bookkeeping; returning a snapshot handle alone is not a
successful Commit. `Busy` due to Commit-slot capacity must be distinguishable from
actual Workspace inactivity and cannot be returned merely because a command or
writable handle is alive.

| Boundary | Input / result | Contract |
| --- | --- | --- |
| Prepare mutation | Leased source root; expected inode/binding revisions and predicates; operation request identity; replacement input | Private candidate pages/ranges and worst-case reservation. Full source-root lease remains owned. No branch construction or publication. |
| Install mutation | Prepared root plus ownership/reservation; source-root identity/sequence | Compare complete current root, then install exactly once; all relevant roots/change keys move together. Stale-root attempts reprepare outside installation synchronization and remain charged. |
| Acquire snapshot | Workspace owner identity and admitted attempt | Complete host-readable root lease and sequence; no traversal, full export, payload copy, or delayed prerequisite drain. V1 completion is required before advertising full filesystem snapshot semantics. |
| Read captured inode/name | Snapshot identity; stable live inode or parent/raw component name | Captured record/absence with bounded page access. Byte names and existing canonical validation preserved; never substitute a live lookup. |
| Read captured ranges | Snapshot identity; inode; offset/length | Bounded exact range cursor and retained physical-read leases. Short reads/errors follow existing supported API; never allocate whole file for a bounded read. |
| Enumerate captured changes | Snapshot identity; expected covered boundary | Stream captured secondary-index interval and final typed effects, including removals and necessary inode/link accounting. No global directory or historical-key discovery scan. |
| Build candidate | Snapshot reader; immutable expected comparison context | Existing builder output/admission plus complete canonical correspondence root. No borrowed live maps or build-duration backing/lifecycle guard. |
| Publish candidate | Exact staged root; expected branch/root/base; attempt identity | Existing conditional Store semantics; resolved Created/UpToDate/conflict or explicit unknown, never newer live-state recapture. |
| Apply outcome receipt | Verified attempt outcome, captured boundary, completed correspondence root | Bounded PublishedContext replacement only. No live inode replacement, global dirty reset, or large post-publication index build. |
| Release / abandon | Exact owned snapshot/candidate/read lease and resolved outcome requirements | Bounded owner release, charged deferred cleanup. Discarding stage is separate from discarding live Workspace. |

`CurrentRoot` and `PublishedContext` are small in-memory descriptors over indexed
backing. Change keys are typed inode/binding identities rather than expanded paths.
Per-operation sequence IDs use checked arithmetic and are never reset on Commit.
Overflow fails before installation; it is not worked around by a hidden Commit.
Existing input limits, permissions, canonical authentication, and transport
capability/Workspace-owner validation remain trust-boundary requirements.

### Source organization

```text
layerfs-workspace-core/src/
    lib.rs, file_edit.rs, namespace.rs, backing.rs, limits.rs
        retain logical semantics/types and validation; adapt bounded record access
    checkpoint.rs
        ordinary Commit installation/reset consumer removed; other users audited

layerfs-workspace/src/
    overlay.rs       [new] host root/index/arena ownership, admission and reclamation
    snapshot.rs      [new] owned captured view, range/change cursors
    lifecycle.rs     [adapt] independent attempt queue/outcome and explicit lifecycle
    changes.rs       [adapt] existing builder consumes snapshot/correspondence inputs
    live_backing.rs  [adapt] host authority and authenticated adapter protocol
    file_io.rs       [adapt] shared payload ranges and bounded arena/read ownership
    capture.rs       [retain if qualified] existing optional content precomputation

layerfs-fuse/src/
    filesystem.rs, live_owner.rs, live_runtime.rs, live_wire.rs, live_transport.rs
        adapt operation/visibility/cache protocol; not a second mutable authority

layerfs-content/ and layerfs-layerstack-store/
    retain shared format/content/tree builders, output admission and staged publication
```

Begin with two additional production modules, not a new crate graph. An independent
attempt record can live in `lifecycle.rs`; extracting a module is optional if it
improves the resulting code. These are resulting responsibility boundaries, not
an ordered implementation plan. No blanket deletion of an entire existing file
is justified solely by a row above.

### Explicit remove / replace / add / retain inventory

| Disposition | Existing or new element | Required resulting shape |
| --- | --- | --- |
| Remove from ordinary Commit | `projection::pause`, writer/execution completion gates, quiesce and resume chain | Commit has a root acquisition boundary, not build-duration filesystem exclusion. |
| Remove from ordinary Commit | Local/remote generation-equal checkpoint installation and global dirty/generation reset | Publication cannot overwrite current contents or erase post-snapshot mutations. |
| Remove coupling | `pending_stage` / `pending_publication` imply `ensure_active` failure | Attempt state is independent of live operation availability. |
| Replace | Builder borrowing `LiveWorkspace.nodes`, dirty sets, canonical maps and host fact lock | Owned snapshot reader and bounded cursors. |
| Replace | Complete changed-fact export/host mirror as snapshot input | Continuously host-readable authoritative state; no capture-time map export. |
| Replace | Container-only mutable authoritative bytes after write acknowledgment | Host-owned retained payload/metadata before ordinary successful acknowledgment. RTT cost belongs to foreground benchmarks. |
| Replace | One unlinked file/FD per retained payload segment and whole-extent-only liveness | Bounded arena descriptors, stable physical catalog and range/block ownership; actual host release mechanism required. |
| Replace | All-materialized-node identity scan and global known-path rewrite for ordinary local changes | Stable allocation high-water and indexed parent/name/path-consumer access. No unrelated-node scans hidden in Commit or rename. |
| Replace where unbounded | `pieces()` whole-file Vec consumers | Bounded traversal/plan cursors; necessary changed-file metadata traversal remains counted. |
| Relocate | Pending candidate, publication outcome, checkpoint output ownership | Owned CommitAttempt and complete metadata-only correspondence output. |
| Add | Host immutable root bundle, dual change indexes and source-root leased preparation | Atomic current-state installation and capture; correct unrelated-writer conflict handling. |
| Add | Snapshot reader and provenance correspondence index | Stable captured input and efficient predecessor-relative C2 view without live rebase. |
| Add | Prepared graph/interval ownership, bounded replay results, reclamation headroom | Failure-safe data retention, exactly-once active-session retries, bounded physical allocation recovery. |
| Retain | Init/Commit shared output driver and canonical content/admission machinery | No duplicated CAS, FULL/DELTA, CDC, packing/compression, or admission engine. |
| Retain | Existing one-stage ownership and conditional publication | Exact candidate retry and atomic stage retirement; no multi-stage/history schema merely for overlap. |
| Retain/adapt | Compact edits, range sharing, sorted directory/inode updates and journals | Preserve locality and bounded construction; result journal need not install live state. |
| Audit before deletion | Cache coordination, fsync, SDK edit reconciliation, path reporting, End/Discard | Preserve their independently required semantics. Commit no longer using a gate does not make every other use obsolete. |

Physical runtime backing remains private under the existing Workspace runtime root;
there is no visible snapshot tree. Payload arenas, metadata arenas/catalogs, and
construction scratch have distinct accounting but need not become one directory
or process per responsibility. `workspace_stages` remains in the Store. No current
data-format migration is made by this document; any necessary private runtime
format has explicit identity/validation and no restart guarantee.

## 6. Required correctness examples and adversarial invariants

| ID | Fixed logical example | Required outcome and cost signal |
| --- | --- | --- |
| E01 | S1 captures A; x installs while builder held; S2 acquired after C1 resolves | C1=A, C2 includes x if still effective; same fd/mount; actual completed operation overlap. |
| E02 | Writers A and B prepare distinct-inode changes from R0 | Second stale root cannot overwrite first; both final effects present; root retries and abandoned allocations counted. |
| E03 | Original live base contains `/a`; C1 publishes its deletion; covered tracking is cleaned | Live `/a` stays absent; overlay tombstone survives independently of cleaned change-index entry. |
| E04 | C1=[large nonzero A][zero:1], insert zero prefix; separately C1=[A:1][zero:L][B:1], delete A | Nonzero anchors bound zero alignment: preserve large A and unchanged zero:L reuse in their respective cases; no CDC of unchanged large spans. |
| E05 | Small edit after C1, whose live ranges still reference C0 | Transient predecessor view reuses C1; no full payload comparison/CDC solely because base roots differ. |
| E06 | Equal-byte new occurrence, partial source overlap, physical extent relocation, source ID reuse attempt | New lineage distinct, retained lineage stable, illegal ID reuse rejected; exact output and bounded indexed matching. |
| E07 | 64-MiB logical source reduced to one live byte with no old reader/snapshot | Retain only intersecting <=4-KiB payload block plus accounted metadata/slack, not the whole source; backend allocated-byte release separately verified. |
| E08 | Near physical quota with mostly dead blocks and pinned readers | Recovery headroom remains available, source preserved on failed relocation, no allocator overshoot or false freed-byte receipt. |
| E09 | Append installs; result reply is lost and retried in active session | Exactly one append; bounded replay result; stale/out-of-window request never reapplied. |
| E10 | Candidate stages; head conflict or reply loss after publication | No live inactivity, no rebuilt newer candidate as retry, no success inferred from missing stage; unknown outcome remains explicit. |
| E11 | Supported mmap/cache-visible update precedes acquisition but is absent at host | Full snapshot must still satisfy visibility. Host-root-only result cannot pass; V1 remains blocking until an actual mechanism supplies it. |
| E12 | Large C1 then ten-file C2, with open-unlinked handles and rename | Exactly relevant distinct-inode content tasks; no full namespace reconstruction; orphan handles preserved without namespace resurrection. |

The test hold in E01 blocks only a test builder after acquisition and belongs only
to verification. Timed performance uses natural overlap with a complete fixed-work
schedule. Root acquisition probes must report the actual pending D at each sample,
not a previously covered million-entry overlay used for later no-op Commits.

## 7. Contract decisions still preventing complete readiness

V1 is a feasibility boundary, not merely a missing test. The existing source does
not explicitly negotiate `FUSE_WRITEBACK_CACHE`, but cached opens support mappings
and current Commit drains kernel state. Linux documents cached mmap support and
distinct write-through/writeback behavior; the deployed kernel's exact capability
and mapped-write path must be bound to the visibility mechanism. See the primary
[FUSE I/O documentation](https://docs.kernel.org/filesystems/fuse/fuse-io.html).
A root lease over host-installed bytes is not by itself a snapshot of all
kernel-visible state. No direct-I/O-only or implicit user-fsync exception is accepted.

For V2, the spec selects COW page graphs, interval ownership and arenas, but the
private page encoding, allocator/ownership transaction protocol, retry fairness,
backend physical block-release mechanism, complete resource defaults, and cleanup
latency limits still need a coherent validated definition. A requirement for
efficient indexed interval updates is not evidence that a particular index supplies it.

For V3, lineage-disjoint nonzero interval indexing and anchor-bounded zero matching
remove the reviewed greedy-locality counterexample. Integration must still prove
that the existing incremental builder accepts bounded transient predecessor views
without an earlier full-file equality scan. Correctness/complexity evidence is
required for the exact selected representation, including origin normalization.

For V4, the publication witness for every uncertain Created/UpToDate case is not
fully selected. Do not add an unbounded outcome log or claim stage absence is a
witness. Any selected runtime/Store receipt must specify atomic relation to the
publication transaction, retention/acknowledgment lifetime and failure behavior;
any Store schema change must be explicit and minimal rather than hidden in a helper.

V5 is an evaluation follow-up, not a newly introduced implementation-readiness
blocker. Implement correctly, use applicable existing benchmarks, then inspect and
report the results. No new capture latency target, mandatory sample count, or
throughput pass/fail gate is required by this specification. Existing benchmark
contracts remain in force; extensions are justified only by identified coverage gaps.

## 8. Rule mapping, source evidence, and review status

| Governing rule | Spec coverage |
| --- | --- |
| 1: current state, not Workspace history | Sections 1-3; ordinary mutation and bounded latest-key tracking. |
| 2: non-pausing lifecycle | Snapshot/attempt contracts, interface and removal tables, E01/E11. |
| 3: consistent boundary and sequential Commit | Captured `(b0,b1]`, dual indexes, attempt state machine, E01/E02. |
| 4: host backing and FUSE placement | Selected host authority, runtime layout, V1 acknowledgment caveat. |
| 5: efficient storage and ownership | Arenas, range/block liveness, lineage-safe substitution, E06-E08. |
| 6: fast acquisition | Complete acquisition events, O(1) root lease target, zero-work counters, actual pending-D observations under the selected existing workload. |
| 7: shared formats and locality | Shared Init/Commit pipeline, provenance/C2 adapter, scan/Vec removal tables, existing-workload throughput observations. |
| 8: staging, publication, retry | Exact candidate/context, conditional transaction, receipts and V4, E09/E10. |
| 9: budgets and FUSE visibility | Reserved transient/reclaim resources, bounded replay/queues, V1/V2. |
| 10: qualification | Existing benchmark contracts, explicit required scale proof, performance/verification separation; no new numerical gates. |
| 11: decisions | Concrete selected mechanisms plus narrowly stated remaining readiness blockers, not unsupported performance claims. |
| 12: reuse | Existing source boundaries below and architecture source register. |

Primary code evidence is recorded in the architecture document's source register.
Key implementation entry points for this specification are:

- [Live Workspace/Commit lifecycle](../../../../crates/layerfs-workspace/src/lifecycle.rs)
- [Current snapshot inputs, candidate builder, file predecessor checks and result journal](../../../../crates/layerfs-workspace/src/changes.rs)
- [Shared Init construction](../../../../crates/layerfs-layerstack-store/src/layerstack.rs)
- [Shared output/admission engine](../../../../crates/layerfs-layerstack-store/src/objects.rs)
- [Staged conditional publication](../../../../crates/layerfs-layerstack-store/src/workspace.rs)
- [Stage ownership](../../../../crates/layerfs-layerstack-store/src/staging.rs)
- [Host backing/fact protocol](../../../../crates/layerfs-workspace/src/live_backing.rs)
- [FUSE buffered writes, cache handling and current freeze](../../../../crates/layerfs-fuse/src/live_owner.rs)
- [FUSE negotiated flags and handle modes](../../../../crates/layerfs-fuse/src/filesystem.rs)
- [Current range and ownership logic](../../../../crates/layerfs-workspace-core/src/file_edit.rs)
- [Current namespace/path operations](../../../../crates/layerfs-workspace-core/src/namespace.rs)
- [Current resource policy](../../../../crates/layerfs-workspace-core/src/limits.rs)
- [Benchmark policy](../../../general/benchmark_rules.md) and [v0.1.6 execution contract](execution-and-verification.md)

Three subagents drafted storage/snapshot, Commit/staging, and performance sections.
A separate adversarial round attacked cross-component correctness and capture/Commit
speed. The [review record](overlay-snapshot-spec-review.md) identifies findings,
counterexamples, corrections and remaining blockers. Review is static evidence;
no product changes, live benchmark samples, or implementation qualification were
performed for this specification draft.
