# v0.1.6 sandbox snapshot architecture review

Research date: 2026-09-15. Source inspected: `7b73c4b33c950c3ce3cd192ea7b571398bda3f8f`; v0.1.5 tag: `5c7c9b0b01107461c3c144bd910539a6d73b12af`. Three independent subagents traced the old execution path, the current storage representation, and benchmark evidence. This is a research recommendation, not an implemented change or a new benchmark run.

The owner clarified that durability is outside this algorithm experiment and that hundreds of megabytes of temporary overlay state are unacceptable. These instructions supersede the motivation for retaining the current host-before-acknowledgment architecture in this proposed experiment. This review does not change existing release contracts or claim previously unresolved correctness work is complete.

**Recommendation: selectively restart the mutable workspace/snapshot layer from v0.1.5's sandbox-local ownership model. Keep the canonical content store and useful publication/correctness work. Replace the mandatory per-operation host route and the sparse, eagerly versioned disk metadata representation. Do not merely move that representation into the sandbox.**

## What actually regressed

The current attachment path deliberately creates `HostRuntime` for container FUSE placement and bypasses the legacy local owner. SDK and mounted operations share that host authority. The spec explicitly requires every installed mutation to have a host acknowledgment dependency, even when metadata is cached; it also recognizes that sequential writes cannot necessarily be batched. These are selected architecture rules, not requirements imposed by snapshots or FUSE. The earlier architecture draft even identified live-owner authority as an alternative.

Sources: [attachment](../../crates/layerfs-workspace/src/projection.rs#L25), [spec authority](../roadmap/0.1/0.1.6/overlay-snapshot-spec.md#L90), [earlier alternatives](../roadmap/0.1/0.1.6/overlay-snapshot-architecture-design.md#L725).

The spec also requires immutable published metadata pages even when only the live root owns them. That pays copy-on-write allocation and retirement on ordinary mutations before anyone asks for a snapshot. The implementation adds four expensive details:

- A 4 KiB metadata page accepts at most seven keys, with no retained page cache.
- Insert reconstructs the affected tree path and allocates replacement pages; it has no exclusive-page update path.
- Tiny ordinary payload allocations round up to 4 KiB and maintain multiple allocation/ownership index records.
- Simple content ranges and subsequent canonical correspondence incur separate tree/index work, followed by reclamation.

Sources: [index constants and cache policy](../../crates/layerfs-workspace/src/overlay_index.rs#L1), [insert](../../crates/layerfs-workspace/src/overlay_index.rs#L674), [payload allocation](../../crates/layerfs-workspace/src/overlay_payload.rs#L13), [page contract](../roadmap/0.1/0.1.6/overlay-snapshot-spec.md#L121).

This is excessive work for a temporary mutable upper layer. Bounded memory and safe ownership are useful properties, but implementing their accounting through multiple low-density disk trees for every tiny mutation is not an efficient way to obtain them.

## Evidence and its limits

For `tiny-create-500-mixed-v4`:

| Metric | v0.1.5 historical sample | v0.1.6 Phase 1 diagnostic | R1a + R3a diagnostic, changes now merged |
|---|---:|---:|---:|
| Whole public-call sum | 0.242660 s | 16.519359 s | 17.976272 s |
| Sandbox-to-host backing exchanges | 7 | 4,518 | 3,512 |
| Exec | 0.166715 s | 7.6942 s | 9.3351 s |
| Commit | 0.059709 s | 4.4745 s | 4.5470 s |
| Pre-capture maintenance | — | 3.4042 s | 3.5053 s |
| End | 0.005395 s | 4.3401 s | 4.0837 s |
| End settle | — | 4.0212 s / 512 steps | 3.7781 s / 56 steps |
| Host CPU | 0.117168 s | 13.685048 s | 15.153299 s |
| Container command CPU | 0.118817 s | 1.008736 s | 0.983984 s |
| Host process disk writes | 1.086 MiB | 245.207 MiB | 237.363 MiB |
| Host peak RSS | 33.000 MiB | 34.484 MiB | 31.797 MiB |

These are historical/diagnostic samples across different ownership routes, not a fresh paired speedup test. The earlier same-route n3 M2 comparison was 20.0297 s candidate versus 19.3958 s control: passing that comparison established no material regression against an already slow route, not achievement of v0.1.5 performance. Host CPU and write counts are after-product minus before process counters. Container CPU uses its command window; RSS is a peak, and host/container peaks must not be added. Process write counters measure write activity, not unique retained storage or exact device traffic.

Sources: [Phase 1 report](../roadmap/0.1/0.1.6/evidence/issue144-phase1/README.md#L35), [R3a ledger](../roadmap/0.1/0.1.6/overlay-snapshot-verification-ledger.md#L883). Local raw receipts, all sample records on line 2: [v0.1.5](../../benchmark-results/host-store/issue120/performance/tiny_file_churn/tiny-create-500-mixed-v4/perf.jsonl), [Phase 1](../../benchmark-results/issue144-phase1/runs/create500-instrumented-c2/perf.jsonl), [R3a](../../benchmark-results/issue144-phase2/runs/iter-r3a-1789409235/perf.jsonl). Raw receipt paths are git-ignored local artifacts.

The evidence shows substantial CPU and I/O amplification. It does not show a large small-case RSS regression. Hundreds of megabytes of temporary disk backing must not be described as the same quantity of resident RAM.

The roughly 700 MB+ storage finding is valid but its attribution needs correction. The 25,000-file test creates and writes one byte per file, performs maintenance, and reads the files back. It performs **no Commit and no per-file snapshot**. It reports 117,489,664 B of arena allocation, 233,033,728 B of payload index allocation, and 105,515 live metadata pages. Applying the actual 4,096 B page size gives 432,189,440 B of metadata page bodies and a component total of 782,712,832 B, before omitted metadata overhead. The metadata contribution is calculated from the page count, not a separately logged complete physical-footprint census. The frequently repeated exact “762 MB” figure is therefore not the authoritative component sum.

Its 1,099.52 s wall time is a debug capacity diagnostic including repeated censuses and read verification, not measured production snapshot latency. Neither correction excuses the representation: the enormous overhead already exists before a snapshot is requested.

Sources: [test scope and raw numbers](../roadmap/0.1/0.1.6/evidence/step1-capacity/capacity-trajectory.md#L8), [raw log](../roadmap/0.1/0.1.6/evidence/step1-capacity/run-25000.log), [timing qualification](../roadmap/0.1/0.1.6/overlay-minimal-overhead-implementation-plan.md#L49).

Canonical output storage is a separate observation: 824,450 B of new content grows the durable store by 909,312 B in v0.1.5 and 905,216 B on the new route, around 1.10x in both. That supports retaining the canonical store. It does not make temporary amplification acceptable. [Storage comparison](../roadmap/0.1/0.1.6/evidence/issue144-phase1/README.md#L180).

## Why current optimization sequencing is insufficient

The Phase 1 run has essentially the same FUSE/workload callback count as v0.1.5, but thousands more synchronous host exchanges. The serial workload puts these exchanges on its critical path. Increasing connection concurrency cannot batch syscalls that depend on preceding replies.

The tiny snapshot acquisition timer rounds to zero while Commit spends 3.4 seconds in `maintain()` before acquisition. End spends another four seconds canonicalizing live state that will shortly be discarded. An O(1) root lease is a useful primitive; reporting it alone does not demonstrate a cheap user-visible Commit.

R1a removes unnecessary attribute roundtrips and is useful within the existing route. R3a reduces settle steps from 512 to 56, but the diagnostic settle time improves only 6%. The ledger explicitly identifies remaining per-node planning and payload release work. The previous R1–R4 estimate was approximately 4.5 seconds and did not promise v0.1.5 parity; it is not evidence for completing the original objective. The R3a measurement weakens that estimate further. [Attribution](../roadmap/0.1/0.1.6/evidence/issue144-phase1/README.md#L89), [latest measured remainder](../roadmap/0.1/0.1.6/overlay-snapshot-verification-ledger.md#L911).

There is also a reporting error worth fixing before another admission experiment. The Phase 1 report says bulk-create-100 failed during exec and no Commit/End ran. Its raw receipt actually records exec 28.8039 s, a failed Commit of 21.4897 s including 18.7253 s maintenance, and successful End of 0.7959 s. The error is `scratch allocation exceeds admitted growth`; the exact inner failing allocation remains unidentified. [Report statement](../roadmap/0.1/0.1.6/evidence/issue144-phase1/README.md#L365), [raw receipt](../../benchmark-results/issue144-phase1/runs/bulkcreate100-probe-c1/perf.jsonl).

## What to recover from v0.1.5

v0.1.5 does **not** install a canonical checkpoint per ordinary FUSE write. WRITE updates locally owned pieces and buffered append state; FLUSH returns success; RELEASE unpins. Explicit fsync flushes append state and publishes accumulated dirty facts. Ordinary Commit freezes/captures, builds and publishes the changed frontier, installs its canonical checkpoint, then resumes. `EditCheckpoint` in the nonremote SDK path is an edit rollback object, not a canonical snapshot.

Sources: [v0.1.5 WRITE/FLUSH](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.5/crates/layerfs-fuse/src/filesystem.rs#L814), [fsync](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.5/crates/layerfs-fuse/src/live_owner.rs#L2344), [Commit](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.5/crates/layerfs-workspace/src/lifecycle.rs#L539).

Keep local mutation ordering, shared append buffers, stable live inode identities, dirty-frontier construction, unchanged canonical object reuse, and the existing persistent `PieceTree`. It already represents base ranges, inline bytes, zeros, and retained spool ranges with owned immutable cursors. Keep conditional publication, exact retry receipts, and successive-commit lineage semantics from the newer work.

Do not restore full dirty-prefix export on every fsync, whole-commit pauses, or unconditional checkpoint installation into the live workspace. Repeated v0.1.5 fsync can resend the growing dirty prefix. The old backing CHECK also only checks metadata/high-water marks, whereas the new route calls `sync_data`; identical fsync durability must not be assumed when comparing them. [Dirty-facts export](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.5/crates/layerfs-fuse/src/live_owner.rs#L2774), [PieceTree](../../crates/layerfs-workspace-core/src/file_edit.rs#L730), [new fsync](../../crates/layerfs-workspace/src/host_operations.rs#L503).

## Proposed replacement algorithm

1. **One local owner.** The FUSE daemon owns mutable namespace, inode metadata, change tracking, and local replacement bytes. SDK operations enter this same owner. Ordinary hot metadata operations and writes require no host installation. Immutable-base cache misses can still fetch host data.
2. **Compact current state.** Use dense, bounded-size in-memory metadata tree nodes for the algorithm experiment. Mutate exclusively owned nodes in place. Copy a touched node only when an active snapshot or reader shares it. Validate and reserve fallible work before installing a multi-record namespace mutation. Keep ownership and mutation synchronization local and short.
3. **Compact content forms.** Represent empty, inline, and single-range files directly in inode metadata; use the existing PieceTree for fragmented content. Pack small replacement payloads into shared local chunks. Bound chunk size/retention so one surviving byte cannot retain an arbitrarily large obsolete segment. PieceTree is RAM-resident and lacks the newer origin IDs, so adapt provenance deliberately; it is not a drop-in replacement for the disk range implementation.
4. **Capture a generation.** Under mutation ordering, retain the metadata root, changed-key root, payload owners, and sequence for generation G. The live owner continues on a successor root; only touched shared nodes diverge. Snapshot acquisition does not copy the workspace or flush its metadata trees to the host. A single unresolved Commit initially limits snapshot multiplicity.
5. **Stream the frozen input.** The host builder receives changed descriptors and needed payload in bounded batches from G. Transfer is proportional to changed metadata and bytes, not one RPC per file operation. It reads the frozen generation, never newer live paths through FUSE, and reuses the existing CAS/FULL-DELTA/CDC/packing pipeline.
6. **Publish without overwriting newer state.** Receipt acknowledgment names G. Advance comparison/coverage and preserve all later mutations. Retain compact provenance needed to reuse C1 when constructing C2. Reclaim only unreferenced backing; use version checks or lazy substitution to replace already committed temporary ranges safely. Do not accumulate an unbounded chain of old generations or rebuild C1 for each C2.
7. **End retires the workspace.** Skip canonical substitution whose only purpose is future reuse of a live workspace that is ending. Preserve clean-state checks, verified unmount, uncertain-publication resolution, and ownership-safe cleanup. These are separate from the optional maintenance loop in current code. [End ordering](../../crates/layerfs-workspace/src/lifecycle.rs#L1134), [optional maintenance](../../crates/layerfs-workspace/src/commit_maintenance.rs#L1), [publication resolution](../../crates/layerfs-workspace/src/commit_attempt.rs#L298).

This is a design to measure, not a promised performance result. An `Arc<BTreeMap<...>>` is insufficient: the first mutation after sharing it clones the whole map. The page/node-level ownership is the important part. The current prepare path also deliberately pins its predecessor for every mutation; adding a reference-count conditional without changing that preparation model will not implement exclusive mutation correctly.

For the first experiment, cap and report RAM explicitly and reuse first-party tree/piece algorithms. Do not rebuild a disk pager, allocator catalog, and reclamation database before demonstrating the algorithm at 500 and 25,000 files. Add eviction/spill only if an actual larger-scale memory target requires it; an in-memory success does not establish million-file bounded-memory support. A 1 KiB/file metadata budget would be roughly 24.4 MiB at 25,000 files—an illustrative design budget, not a measurement or formal acceptance threshold. The 25 KB payload should be inline or packed, not 25,000 separate 4 KiB reservations.

## What FUSE and native snapshots do—and do not—provide

A sandbox-local FUSE daemon can own the snapshot algorithm. FUSE does not itself provide a filesystem snapshot primitive. Moving authority solves the host roundtrip issue, but does not automatically capture dirty shared mmap pages held by the kernel. Existing probes found mapped stores with no daemon WRITE request, and retrieval could return pages that changed while retained. A local root snapshot is valid for operations installed in that local owner; it is not proof of the full existing mmap contract.

Measure the ordinary-write algorithm independently and label that scope. Preserve the mmap correctness tests and resolve their boundary before claiming complete non-pausing filesystem snapshots. Do not silently disable mmap, treat `syncfs` as O(1), or assume sequential per-file flushes establish a single atomic filesystem cut. [Kernel FUSE I/O modes](https://docs.kernel.org/filesystems/fuse/fuse-io.html), [repository probe and correction](../roadmap/0.1/0.1.6/overlay-snapshot-contract-resolution.md#L25).

Btrfs subvolume snapshots are worth a separate comparison only where the sandbox controls a compatible backing filesystem. Its documentation notes dirty-data flushing before root-copy snapshot creation. Generic OverlayFS has copy-up and live-layer mutation restrictions; changing the upper directory under a mounted overlay is not a replacement snapshot protocol. Neither is a default drop-in for LayerFS's canonical range representation. [Btrfs performance](https://btrfs.readthedocs.io/en/latest/btrfs-subvolume.html#performance), [OverlayFS copy-up](https://docs.kernel.org/filesystems/overlayfs.html#non-directories), [underlying-layer restrictions](https://docs.kernel.org/filesystems/overlayfs.html#changes-to-underlying-filesystems).

## Execution and rollback recommendation

Keep main and v0.1.5 available as controls. Build the selective replacement in an isolated experimental branch using v0.1.5's local-owner path and compact file-edit core as the starting point. Existing canonical storage, useful benchmark receipts, publication/retry semantics, and correctness tests survive. A hard reset of main or a rewrite of the content store is unnecessary.

First prove local hot writes with explicit host-dispatch counters and compact allocation before Commit. Then prove frozen generation G remains stable while writes proceed and that C2 includes later edits. Then prove repeated Commit/cleanup storage reaches a bounded steady state, including deletion, hardlinks, rename, open-unlinked files, large-file small edits, and publication retry. Finally run end-to-end tiny create/stat/unlink and both bulk cases against a freshly measured v0.1.5 control, reporting commit acquisition, transfer, construction, cleanup, CPU, RAM, backing allocation, and I/O separately.

Use diagnostic samples while iterating; use fresh paired samples for performance claims. Storage density and CPU/write amplification are first-class outcomes, not deferred work after latency. A new version that improves a slow host-route control but remains many times slower than v0.1.5 has not met the original objective.
