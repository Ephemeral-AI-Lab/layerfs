# LayerFS V3 capture and mixed-edit resilience audit

Status: deferred V3 design input
Audit date: 2026-08-31
Audited repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`
Base commit: `0970008668f54bae841797dafd57acab191fba7f`
Source state: active dirty worktree; revalidate findings before implementation

## Purpose and V2 boundary

This document preserves the large-file, count-changing, fragmented-write, large-repository, and mixed-workload issues found during a read-only multi-agent audit.

These issues are intentionally deferred to V3. The current V2 effort should remain focused on making the frozen V2 architecture perform well and honestly in `fs-bench-plus`, including the public SDK, real FUSE path, Commit, Push, two-Store durability, recovery, evidence, and fair comparison. V2 should not absorb a new mutation-journal architecture, new public filesystem operation family, cache redesign, or compaction system merely to address this document.

If a deferred issue produces a correctness failure in a registered V2 benchmark operation, fix the smallest shared root cause required for that operation. Otherwise preserve it here for V3.

## Executive verdict

The persistent content core is strong, but the end-to-end capture and storage path is not yet proven resilient across the full requested envelope.

| Area | Audit verdict |
| --- | --- |
| Persistent extent split/splice/join | Strong |
| Streaming FastCDC over replacement bytes | Strong |
| Immutable retained history | Strong |
| Small positional FUSE writes | Strong |
| Append and truncate-shrink | Strong |
| Many fragmented writes | Needs redesign or explicit bounded rejection |
| Huge sparse growth | Needs structural zero/hole handling |
| General count-changing edits through public FUSE | Partially supported |
| Large-repository Commit planning | Structural blocker |
| Directory rename and hard-link-set changes | Correctness blockers indicated by code review |
| Reference-aware local admission | Missing |
| K4/K16 Push frontier reuse | Partial in the current unsealed worktree |
| Replica incremental Push | Missing |
| Integrated mixed-workload proof | Missing |

The V3 direction is to preserve CAS, CDC, canonical encodings, immutable roots, and persistent extent trees while replacing the whole-repository Workspace planner and non-union-aware admission/Push wiring around them.

## Existing strengths to preserve

### Persistent file operations

`crates/layerfs-content/src/file/rope/edit.rs` implements replacement as persistent split, splice, and join operations. Insertion, deletion, prepend, append, overwrite, and truncate can reuse unchanged extent subtrees without reading unchanged payload bytes.

Existing evidence includes:

- a 2,000-revision randomized splice model;
- verification of retained historical roots;
- mixed insertion, deletion, and replacement operations;
- large-replacement reachability proof;
- a 64 MiB streaming replacement below the structural memory ceiling;
- rejection of overlapping batched replacements before orphan objects can be produced;
- shifted-stream FastCDC evidence preserving 219 of 220 suffix chunks in the 4 MiB prepend fixture;
- focused Workspace overwrite, no-op, append, shrink, grow, and rename cases.

### Bounded content construction

FastCDC uses fixed-size streaming buffers. Payload objects are emitted while only structural boundary state remains deferred. `FileMutationBatch` prunes superseded intermediate structure and enforces an approximately 8 MiB private structural ceiling.

### Online FUSE capture

The FUSE path captures mutations synchronously rather than scanning the mounted filesystem at Commit. Existing files retain their immutable base root, changed bytes are written to a sparse Workspace spool, dirty and physically charged ranges are tracked separately, and inode identity is preserved for ordinary content and metadata edits.

### Storage safety mechanisms

The storage layer already provides:

- spillable candidate objects and seen-ID sets;
- 512-ID membership pages;
- 128-object and 4 MiB object-admission batches;
- bounded fact batches and paged history reads;
- canonical identity authentication before SQLite writer transactions;
- short object-admission transactions;
- visibility-last authority publication;
- retry membership that avoids retransmitting admitted payload;
- BranchStore and LayerStackStore stable durability barriers.

V3 should reuse these mechanisms instead of creating a parallel storage architecture.

## Deferred V3 issues

### V3-1: Commit planning walks and materializes the complete repository

Priority: structural blocker

Relevant code:

- `crates/layerfs-workspace/src/changes.rs:29`
- `crates/layerfs-workspace/src/changes.rs:433`
- `crates/layerfs-workspace/src/changes.rs:486`
- `crates/layerfs-workspace/src/limits.rs:9`

Every Commit constructs complete `base_manifest()` and `final_manifest()` maps. Both enumerate every path, resolve inode and metadata state, and retain complete path maps in memory.

The default final-delta policy permits 8 MiB and charges approximately `512 + 4 * path_length` bytes per path in each manifest. With ordinary paths, the two manifests can exhaust the limit at only approximately 6,000 to 8,000 entries. A ten-byte edit in a larger repository can therefore fail with `workspace final-delta limit`.

Consequences:

- Commit CPU is `O(total repository paths)`.
- Commit memory is `O(total repository paths)`.
- The first Commit destroys lazy Workspace behavior by materializing the repository.
- Later in-place rebases iterate the materialized state again.
- Repeated tiny edits repay the complete path scan.
- Raising the memory ceiling would postpone, not fix, the problem.

V3 direction:

Replace manifest comparison with a bounded operation-aware mutation journal containing:

```text
file dirty ranges
metadata changes
create and remove
rename source and destination
hard-link add and remove
directory parent mutations
affected path ancestors
```

The incremental candidate planner should update persistent inode and directory roots directly. Desired complexity:

```text
O(changed paths * path depth)
+ O(affected hard-link aliases)
+ O(changed byte ranges)
+ O(persistent-tree height)
```

Unrelated paths must not be enumerated.

### V3-2: Directory rename and hard-link-set changes are not correctly incremental

Priority: correctness blocker indicated by code review

Relevant code:

- `crates/layerfs-workspace/src/changes.rs:62`
- `crates/layerfs-workspace/src/changes.rs:159`
- `crates/layerfs-workspace/src/changes.rs:223`

The current planner recognizes rename equivalence only for files and symlinks. A nonempty directory rename can classify descendants independently while attempting to remove the old still-nonempty directory. An empty directory is removed and recreated instead of retaining inode identity.

Adding or removing a hard-link alias breaks the exact group comparison. The inode can be classified for recreation, causing existing aliases and file content to be rebuilt even though the operation is namespace-only.

V3 direction:

- journal rename and hard-link operations explicitly;
- call the existing persistent `filesystem::rename` and `filesystem::hard_link` primitives directly;
- mutate inode namespace reference counts and source/destination directory spines;
- preserve renamed subtrees and descendant inode identities without enumerating descendants;
- preserve file content roots during hard-link add/remove.

### V3-3: Wide directories exceed the current FUSE protocol

Priority: correctness and scale blocker

Relevant code:

- `crates/layerfs-fuse/src/protocol.rs:4`
- `crates/layerfs-fuse/src/proxy_client.rs:93`
- `crates/layerfs-fuse/src/filesystem.rs:386`
- `crates/layerfs-workspace/src/cow_tree.rs:400`

The current directory response is capped at 16,384 entries. With `.` and `..`, a directory wider than approximately 16,382 children cannot be represented. Workspace and proxy layers also construct and cache complete directory vectors, and the proxy eagerly reads the root during connection.

V3 direction:

Introduce continuation-based directory paging and carry the FUSE offset/cookie through every layer:

```text
kernel FUSE
-> filesystem adapter
-> FilesystemPort
-> proxy protocol
-> persistent Workspace directory
```

Mount readiness must perform zero root enumeration. Targeted lookup should remain targeted, including authoritative `NotFound`; it must not fall back to a complete parent `readdir`.

### V3-4: Fragmented dirty ranges become quadratic

Priority: performance and resource blocker

Relevant code:

- `crates/layerfs-workspace/src/file_io.rs:162`
- `crates/layerfs-workspace/src/file_io.rs:226`
- `crates/layerfs-workspace/src/file_io.rs:419`

Every write and truncate clones the complete dirty and charged interval maps and recomputes total charged bytes. For `R` disjoint writes, bookkeeping trends toward `O(R^2)`.

Late range reads also use `range(..end)`, visiting every earlier interval start rather than only the predecessor and actual overlaps. Dirty interval metadata is not charged through the resource policy.

V3 direction:

1. Find only adjacent or overlapping intervals.
2. Compute the resource-charge delta.
3. Validate the policy before mutation.
4. Perform spool I/O.
5. Mutate interval maps in place.
6. Maintain cached charged-byte and interval-metadata totals.
7. Query the predecessor of `offset` plus `range(offset..end)` for reads.

Desired behavior:

```text
capture R disjoint writes: O(R log R)
range read: O(log R + actual overlaps)
memory: charged and bounded before mutation
```

If a legitimate final mapping exceeds the private structural ceiling, add bounded spill or finalized-prefix sealing. Do not silently remove the ceiling.

### V3-5: Fully dirty replacement reads the old base unnecessarily

Priority: dense-rewrite performance

Relevant code:

- `crates/layerfs-workspace/src/file_io.rs:45`
- `crates/layerfs-workspace/src/changes.rs:747`

Overlay reads fetch the full base interval, zero dirty portions, and then overlay charged spool bytes. A completely dirty 32 MiB or 256 MiB replacement therefore rereads the old file even though every output byte comes from the spool.

V3 direction:

Construct output from:

```text
base complements of dirty ranges
+ logical zero dirty regions
+ physically charged spool ranges
```

A fully dirty range must issue zero base payload reads. If measurement still shows overhead, reuse a caller-provided buffer and persistent read plan across FastCDC reads instead of allocating and reauthenticating every approximately 32 KiB buffer.

### V3-6: Sparse growth and zero writes have unbounded logical-work paths

Priority: CPU and memory safety

Relevant code:

- `crates/layerfs-workspace/src/file_io.rs:195`
- `crates/layerfs-workspace/src/file_io.rs:220`
- `crates/layerfs-workspace/src/changes.rs:563`

Growing a file to a huge logical size can charge zero physical spool bytes while Commit streams the complete zero-filled growth through CDC. A 1 TiB truncate can therefore trigger 1 TiB of logical processing.

A zero write overlapping charged spool data can allocate `vec![0; byte_len]`, allowing a tiny overlap to cause a request-sized allocation.

V3 direction:

- remove zeroed intervals from `charged` while retaining dirty-zero semantics;
- never allocate a zero buffer proportional to the logical range;
- write fixed-size zero blocks only when physical clearing is unavoidable;
- add a structural zero/hole extent primitive;
- add a logical-work budget separate from physical spool bytes;
- preserve bounded deterministic rejection when a representation cannot be built safely.

### V3-7: General count-changing editor workflows are not file-size-agnostic

Priority: semantics and performance clarity

The persistent content API supports efficient insert and delete operations. Ordinary POSIX writes do not express “insert bytes and shift the suffix,” so FUSE observes the actual rewritten suffix.

Current public behavior:

| Operation | Current cost shape |
| --- | --- |
| Positional overwrite | Changed-byte bounded |
| Append | Changed-byte bounded |
| Truncate shrink | Boundary bounded |
| Truncate grow | Proportional to zero growth during Commit |
| Write past EOF | Proportional to the new gap and write |
| Middle insert through rewritten file | Proportional to rewritten suffix |
| Temp-copy-fsync-rename | One complete new-file input scan |
| Direct internal extent insert/delete | Persistent boundary operation |

V3 must either expose authentic public insert/collapse-range semantics or implement a generic reuse algorithm without filename, offset, fixture, or workload specialization. Until then, claims must distinguish direct positional changes from opaque rewritten files.

### V3-8: Workspace and proxy memory grow over long mixed workloads

Priority: resource resilience

Unbounded or incompletely charged structures include:

- Workspace nodes and canonical-node mappings;
- hard-link path alias sets;
- directory overlays and mutation paths;
- proxy attribute and complete-directory caches;
- total simultaneously pending creates;
- materialized but forgotten nodes;
- individually stored 65,536-node reservations;
- one fixed 16 MiB read-ahead buffer with surrounding frame copies.

There is no FUSE `forget` or `batch_forget` integration. Complete-manifest Commit currently forces the repository into these caches.

V3 direction:

- add global pending-create count and byte limits;
- make node reservations lazy and range-backed;
- add forget-aware or bounded eviction while preserving pinned, open, and dirty nodes;
- page directory caches;
- charge mutation and cache structures through the resource policy;
- make read-ahead adaptive rather than a fixed 16 MiB minimum.

### V3-9: In-place rebase scales with all materialized nodes

Priority: repeated-Commit scaling

Relevant code:

- `crates/layerfs-workspace/src/lifecycle.rs:115`

Rebase clones every current node, performs path lookups for every node and alias, and builds replacement maps. Once manifest planning materializes the repository, later rebase cost and temporary memory are `O(total repository paths)`.

After the incremental planner exists, rebase should retain or remap only kernel-visible, pinned, changed, and explicitly bounded cached nodes. Record:

```text
rebase_nodes
rebase_aliases
rebase_path_lookups
rebase_peak_bytes
rebase_transition
```

### V3-10: Public changed-byte capacity is fixed at 1 GiB

Priority: explicit product limit

Relevant code:

- `crates/layerfs-workspace/src/limits.rs:9`

The default Workspace permits at most 1 GiB of physically charged changed ranges. A tiny change in a multi-terabyte file is acceptable, but a dense rewrite larger than 1 GiB is rejected.

This is safe failure rather than corruption. If V3 requires larger dense changes, add bounded disk spill or direct canonical streaming. Do not remove the bound without a replacement resource model.

### V3-11: Reference-aware candidate admission is missing

Priority: storage efficiency

Relevant code:

- `crates/layerfs-storage/src/admission.rs:950`
- `crates/layerfs-branch-store/src/provision.rs:36`

`ObjectBuffer` deduplicates within the candidate, and durable admission checks BranchStore, but candidate admission does not check the immutable parent LayerStackStore. Temp-copy, copy-rewrite-rename, dense rewrite, and opaque count-changing workflows can spill and admit authority-owned canonical objects locally.

V3 direction:

```text
candidate IDs
-> BranchStore membership
-> parent LayerStackStore membership for local misses
-> insert only IDs missing from the union
```

Membership and admission must remain bounded and outside SQLite writer transactions. Do not add per-scope copies or refcounts.

### V3-12: Push frontier retention is partial for mixed checkpoints

Priority: transfer efficiency

The current unsealed worktree contains a bounded, correctness-safe `PushPlan` for small immediate Commit-to-Push operations. It binds Commit ID, base root, new root, at most 512 IDs, and at most a 4 MiB candidate. Authority still verifies independently, and fallback is safe.

Only one plan is stored per Branch. A later Commit replaces the previous plan, so K4, K16, delayed Push, and mixed multi-Commit suffixes fall back to transition rediscovery. Medium and dense candidates also lose the plan when they exceed the thresholds.

V3 direction:

- retain a bounded per-Branch chain of Commit frontiers;
- bind every edge to exact base and new roots;
- page membership in 512-ID pages;
- read missing payload through byte-bounded visits;
- cap the cache by IDs and bytes;
- evict complete oldest chains;
- retain generic restart, eviction, and mismatch fallback.

### V3-13: Replica Push still uses full-root traversal

Priority: offline and complete-root performance

When the source reader is complete, Push takes the full-root path rather than the old-to-new transition path. A Replica Branch with a ten-byte edit can therefore enumerate and authenticate the complete snapshot.

Completeness should provide a trusted local old boundary and stronger pruning. It should not force full traversal. Replica must continue treating a missing locally required object as `Integrity` and must never use the parent to hide local corruption.

### V3-14: Authority validation materializes the complete owned suffix

Priority: long-history memory

Relevant code:

- `crates/layerfs-layerstack-store/src/receive.rs:241`

History reads are paged, but authority validation collects all owned roots into a `Vec`. Memory is proportional to the number of unpushed Commits.

V3 direction:

- use a reverse spool or bounded reverse traversal;
- verify every parent-to-child transition;
- use one operation-scoped spillable deduplication set;
- never materialize the complete suffix.

### V3-15: Generic transfer rejects some valid canonical graphs

Priority: transfer contract resilience

Relevant code:

- `crates/layerfs-storage/src/admission.rs:497`
- `crates/layerfs-storage/tests/v2.rs:837`

The transfer layer deliberately rejects a valid nested graph when active recursive buffering exceeds 34 MiB. Product-generated file trees are less likely to hit this because payloads are leaves and internal nodes are small, but a valid canonical root should stream or spill rather than fail solely to preserve the ceiling.

V3 should release or spill parent canonical bytes before descending while preserving postorder admission.

### V3-16: Candidate finishing amplifies transient disk use

Priority: dense-rewrite scratch efficiency

For a large candidate:

1. objects spill into the original temporary Store;
2. `reachable_from()` copies reachable objects into a second temporary Store;
3. local admission reads the second Store and writes durable SQLite objects.

Transient storage can approach two candidate scratch copies plus the durable copy and WAL overhead. A reachability mark/order table inside the original scratch Store would avoid copying the candidate again.

### V3-17: Failure paths and accounting remain incomplete

Priority: operational resilience

Deferred issues include:

- failed creates can leave unreachable ephemeral nodes or spool files after name-conflict validation;
- a rejected first write can leave a base file converted into an unnecessary overlay;
- hard termination can leave predictable fact-spool files;
- transfer and candidate memory receipts use estimated charges rather than actual RSS;
- transition receipts omit some scratch and ancestor-buffer bytes;
- fixed read-ahead can cause repeated 16 MiB fetches for interleaved small reads.

Use preflight validation or one small rollback guard for Workspace mutations. Create temporary spools securely and unlink them while open where supported. Report production-accounted bytes, scratch bytes, and cgroup/RSS separately.

## V3 resilience campaign

### Test axes

| Axis | Required cases |
| --- | --- |
| File size | 1 MiB, 32 MiB, 256 MiB, 1 GiB; sparse logical 4 GiB and 1 TiB |
| Tiny writes | 1, 16, 1K, 10K, and 100K writes |
| Range shapes | Adjacent, overlapping, repeated, reverse order, widely disjoint |
| Medium edits | 4 KiB, 1 MiB, and 8 MiB |
| Dense rewrites | 32 MiB, 256 MiB, and policy-boundary cases |
| Count increase | Append, write past EOF, truncate grow, prepend |
| Count decrease | Truncate shrink and authentic middle deletion if supported |
| Namespace | File rename, nonempty-directory rename, hard-link add/remove/content change |
| Repository size | 1, 1K, 10K, and 100K unrelated paths |
| Directory width | 1, 16,382, 16,383, and 100K children |
| Checkpointing | K1, K4, and K16 |
| Placement | Reference and Replica |
| History | 1, 128, 512, and 10K unpushed Commits |
| Failures | Spool, fsync, candidate spill, membership, object batch, fact batch, verification, pre-publication |
| Recovery | Restart after every edit class and policy boundary |

### Deterministic mixed marathon

Run at least 10,000 operations:

```text
9,000 distributed ten-byte overwrites
400 appends
200 truncate shrinks
200 truncate grows
100 rename or hard-link operations
80 medium 1-8 MiB rewrites
20 dense 32-256 MiB rewrites or temp-copy-prepends
```

Run the same sequence with K1, K4, and K16 checkpoint schedules.

### Required invariants

- Every retained Commit root remains readable.
- Periodic byte and digest oracles match.
- Replica terminal roots remain readable with the parent unavailable.
- Tiny positional writes do not perform a complete file CDC scan.
- Fragmented capture scales no worse than `O(R log R)`.
- Tiny edit latency and memory do not scale with unrelated repository paths.
- Directory rename preserves the subtree and descendant inode identities.
- Hard-link add/remove preserves content roots and inode identity.
- No authority-owned base objects are duplicated into BranchStore.
- Push sends the exact missing union only.
- The old head remains visible after every injected interruption.
- Retry sends only the remaining missing union.
- RSS and scratch-disk peaks stay within declared bounds.
- A resource-limit failure leaves the Workspace unchanged and retryable.

## Counters required for proof

Add bounded passive counters for:

```text
write calls and written bytes
logical zero bytes
dirty interval current/peak/merges/metadata bytes
charged interval current/peak/bytes
spool logical bytes/allocated blocks/peak
mutation paths and directory deltas
hard-link aliases
materialized/dirty/pinned/unlinked-pinned nodes
proxy pending-create count/bytes
proxy pending-unlink count
proxy request/frame/payload bytes
read-ahead fetched and consumed bytes
base/final paths scanned
candidate changed paths and affected ancestors
range-query intervals visited
dirty bytes compared and CDC bytes scanned
candidate object IDs and bytes
scratch bytes written/read/copied
rebase nodes/aliases/lookups/peak bytes/transition
Push plan edges/IDs/bytes/hits/fallback reason
history pages and maximum live suffix records
production-accounted bytes and cgroup/RSS peak
```

## Recommended V3 implementation order

```text
1. Correct directory rename and hard-link add/remove semantics.
2. Replace complete manifests with a bounded incremental mutation planner.
3. Add paged FUSE readdir and remove complete-directory assumptions.
4. Make dirty-range bookkeeping overlap-local, in-place, and resource-accounted.
5. Avoid base reads for fully dirty ranges.
6. Add structural zero/hole handling and logical-work limits.
7. Bound proxy/Workspace caches and implement forget-aware eviction.
8. Optimize rebase to visible, pinned, changed, and bounded cached nodes.
9. Add Reference-aware union admission.
10. Extend Push frontier retention to bounded chains and Replica.
11. Stream authority suffix verification and valid nested transfers.
12. Run the complete mixed, large, fault, recovery, memory, and storage campaign.
```

## Non-goals

V3 should not:

- replace content-derived ObjectIds;
- rechunk or re-encode canonical objects during transfer;
- add per-scope object copies or refcounts;
- introduce a benchmark-specific mutation operation;
- infer edits from filenames, fixture markers, or known offsets;
- weaken Commit/Push durability or visibility-last publication;
- retain two normative capture architectures;
- introduce compaction before measured fragmentation justifies it.

The shortest sound V3 path is to keep the proven content primitives and make the Workspace journal, namespace planner, admission, and transfer layers genuinely incremental and bounded.
