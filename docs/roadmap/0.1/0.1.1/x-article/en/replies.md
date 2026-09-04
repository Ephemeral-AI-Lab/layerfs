# LayerFS v0.1.1 long-form X replies

Post these three long replies in order under the English X Article announcement. Copy only the text inside each fenced block. Attach the indicated images to the corresponding reply.

## Reply 1/3 — Baseline and the v0 defect

```text
1/3 — Where LayerFS v0.1.1 started

LayerFS represents file content, metadata, directories, and the global inode table as immutable canonical objects. That structure is useful for ordinary filesystem edits: changing one file can reuse unaffected content and rebuild only the leaf-to-root paths touched by the edit.

LayerFS 0.1.0 made a flawed assumption, however: it reused that point-mutation path to initialize an entire existing directory.

For every source entry, v0 resolved the path against the current immutable namespace, built the file or structural object, rebuilt the affected directory path, rebuilt the global inode path, and emitted a new namespace root. Metadata such as mtime could then trigger another resolution and another pair of tree-path rebuilds.

That meant the importer preserved a long sequence of valid but externally useless namespace states. Nobody could observe the namespace after file 4,237; only the final state mattered. Yet the implementation generated canonical objects for those intermediate roots anyway.

We established the defect with a controlled namespace fixture: unique deterministic files, exactly 2,500 bytes each, grouped 100 files per directory. Fixture generation stayed outside LayerFS timing. Each tier used a fresh process and Store, and the public lifecycle ran through real Linux FUSE.

The v0 results were:

• 100 files / 0.25 MB: 37.651–38.268 ms, about 6.6 MB/s
• 1,000 files / 2.5 MB: 780.515 ms, 3.20 MB/s
• 10,000 files / 25 MB: still CPU-bound after 524.79 s, below 0.0476 MB/s

We stopped the 10,000-file process and retained the result as a lower time bound rather than retrying it away. After that failure, attempting 100,000 files on v0 would have added little information.

The repeated immutable-tree work explained part of the curve. One persistent-tree update costs approximately O(log N); repeating several updates for N entries produces O(N log N) structural work and intermediate object emissions.

The sharper cliff came later. Candidate objects used an ordered in-memory index capped at 8 MiB. At roughly 64 bytes per entry, the index held about 131,072 objects. Below the limit, exact lookup was indexed. After overflow, v0 discarded the index and scanned the candidate spool from byte zero.

One lookup could therefore become O(E) in object count, or O(S) in spool bytes. Repeating it for later candidates introduced an O(E·S) upper bound. With bounded average object size, S = O(E), so the path commonly became O(E²). A related exact-ID set had a similar linear spill-scan fallback.

V0 also admitted no more than 127 objects per transaction, multiplying transaction and statement-lifecycle overhead after candidate counts grew.

This is why we do not present the eventual timing difference as a speed boast. The old baseline was not a respectable algorithm that we narrowly out-optimized. It was a bulk operation implemented as thousands of increasingly expensive point mutations, followed by a capacity-triggered worst-case lookup path.

The significance of the baseline is methodological: test past internal thresholds, preserve stopped runs as evidence, and write fallback behavior into the complexity model. “We use an index” is not a useful claim if the system silently becomes a repeated linear scan at the scale users care about.
```

Attach:

- `./images/02-v0-cliff.png`

## Reply 2/3 — The v0→v1 correction

```text
2/3 — From mutation replay to final-state construction

V1 changed the meaning of initialization. Instead of asking, “How do we replay every file creation through the public mutation machinery?” it asked, “What one canonical namespace must exist when initialization returns?”

That shift removed work before adding concurrency.

Independent top-level directories became bounded worker tasks. Each worker scanned its assigned source directories, read file contents once, and collected final children plus compact inode records. Results were merged in deterministic task order. Deferred local B-trees built directory and inode structures, obsolete transient nodes were pruned, and only final reachable structural nodes were emitted. The importer then constructed one namespace root and published the Layer and LayerStack last.

The object model changed from roughly:

v0 emissions = O(C + N log N), including intermediate states

to:

v1 emissions ≈ O(C + N + D), representing final reachable state

Candidate handling remained indexed in the measured range, removing the v0 E·S spool-scan term. The resulting measured-range model became:

O(B + K + Σ(k_d log k_d) + N log N + A log U)

Here B is logical source bytes, K is canonical bytes encoded and hashed, A is SQLite row attempts after current-batch deduplication, and U is unique stored objects. Those terms represent work the eager importer still has to perform: discover paths, read bytes, construct authenticated content, build the final inode structure, and maintain the SQLite object index.

Four supporting corrections removed additional amplification:

• A provably empty Store skips membership queries whose answer must be “not present.” Nonempty Stores retain exact checks.

• Admission batches grew from at most 127 objects to at most 8,191 objects, with a separate payload ceiling below 4 MiB. This removed thousands of small transaction boundaries without creating one unbounded transaction.

• Final-only initialization stopped building an O(E) object-reference graph it never consumed. Workspace mutation and reconciliation kept the graph where they actually need it.

• Independent root directories used bounded parallel workers and deterministic merge order. Parallelism changed wall time, not the required total work or canonical ordering.

On the identical 2,500-byte fixture, v1 measured:

• 100 files: 7.732 ms
• 1,000 files: 38.232 ms
• 10,000 files: 489.426 ms
• 100,000 files: 6.799 s

The comparison to v0 proves the old defect was removed; it is not a universal throughput promise. V1 also retained a documented spill fallback beyond roughly 1.4 million unique IDs. The audited 100,000-file sample produced about 808,000 objects and stayed below that boundary, so its complexity statement describes the measured regime rather than every possible namespace.

The correction preserved the invariants that make LayerFS useful: canonical encoding, directory order, content-defined chunking, the five-table Store schema, eager initialization, collision rejection, hard-link behavior, visibility-last publication, and public SDK and CLI semantics. With the same source metadata, bytes, and LayerStack-derived inode seed, the reference and optimized builders must produce the same final reachable canonical state.

V0.1.1 also prevented the bottleneck from simply moving into later operations. Localized Commit previously built complete base and final namespace manifests for a ten-byte overwrite. When topology is unchanged, the retained fast path resolves the touched inode, writes the changed range, rebuilds affected paths, and moves planning from O(N) toward O(log N + changed bytes). Rename, link, and type changes keep the complete-manifest fallback.

Real-FUSE traversal also stopped issuing one redundant attribute lookup per child by using READDIRPLUS, and directory parent lookup moved from scanning materialized nodes to an explicit O(1) relationship.

The broader lesson is not “persistent data structures are slow.” They are the right representation for immutable snapshots and localized edits. The lesson is that a correct point-operation API is not automatically the correct bulk algorithm. When only final state is observable, construct final state directly and prove equivalence at that boundary.
```

Attach:

- `./images/01-throughput-comparison.png`

## Reply 3/3 — The v1→v2 experiments and significance

```text
3/3 — From sequential phases to bounded direct admission

V1 removed the catastrophic complexity path, but its wall clock still contained a serialization boundary:

construct complete worker object segments → wait for workers → write segments → reread segments → admit into SQLite → finalize structure

If producer work takes P, SQLite admission takes A, and finalization takes F, that design behaves approximately like P + A + F.

Before optimizing it, we changed the benchmark contract. The original uniform fixture exposed namespace scaling but underrepresented the interaction between small-file overhead and bulk byte throughput. The v2 fixture kept the same file-count tiers and 100 files per directory, but used a deterministic small-heavy distribution containing empty, tiny, small, medium, and exact 100 MB anchor files.

Because the algorithm and data distribution both changed, v2 occupies a separate comparison lane. Its MB/s figures must not be presented as a pure v1→v2 product speedup.

The pre-direct 100,000-file result was about 4.502 s and 111.1 MB/s. Instrumentation showed the sequential cost clearly:

• 647 MB written to a temporary canonical-object segment
• 647 MB reread from that segment
• about 543 MB of canonical payload then written into SQLite

We first tested exact metadata reuse. Many files shared the same canonical tuple: inode kind, permission mode, mtime seconds, and mtime nanoseconds. Each importer received an eight-entry operation-local cache mapping an exact tuple to the deterministic root ObjectId produced by the unchanged builder.

At 100,000 files, canonical emissions fell from about 1.132 million to about 439,070. Pending duplicate candidates fell from 708,845 to about 15,200. The cache recorded 99,000 exact hits.

Yet the isolated cache experiment did not materially change wall time while the object spool remained. That negative result mattered: optimizing work inside producers could not remove the larger phase boundary.

V2 therefore connected eight existing producers directly to the calling thread. Every producer runs the same sequence: read/stat/open source files, perform unchanged CDC and canonical construction, reuse exact metadata roots, and fill an owned slab bounded by 256 KiB and 512 objects.

A synchronous queue holds at most four slabs, providing explicit backpressure. Ownership of each payload vector moves through the queue; the parent copies zero payload bytes. The calling thread remains the sole SQLite owner and carries one exact-dedup admission batch across slab and worker boundaries, flushing below 4 MiB or at 8,191 objects.

At 100,000 files, the direct path recorded 2,147 slab handoffs, about 439,070 slab objects, 544.3 MB of canonical payload, a four-slab queue peak, and zero canonical object-segment writes, rereads, or parent payload copies.

Construction and admission now overlap. The critical-path model moves toward max(P, A) + F + pipeline stalls even though the broad Big-O work class remains similar to v1. This is the distinction between total-work analysis and critical-path analysis: one guards against scaling cliffs; the other exposes unnecessary serialization on the user’s clock.

Current selected development medians are 220.820 ms / 566.1 MB/s at 100 files, 269.757 ms / 741.4 MB/s at 1,000, 414.729 ms / 723.4 MB/s at 10,000, and 2.766 s / 180.7 MB/s at 100,000.

The 10,000→100,000 transition is instructive. Logical bytes increase only 1.67×, but files and directories increase 10×, source reads 7.47×, canonical emissions 7.82×, SQLite rows 7.69×, and initialization time 6.67×. This is not a return of the v0 quadratic cliff. It is file and object cardinality becoming visible.

The payload pipeline is bounded, but not all memory is O(1). Modeled named buffers peak at 9.63 MiB; initialization incremental process high-water at 98.23 MiB; complete-lifecycle incremental high-water at 198.73 MiB. Top-level tasks, deferred trees, compact inode pairs, and the durable Store still scale with their inputs.

Publication remains visibility-last. Object-only transactions can commit while producers run, but only the final transaction publishes the Layer and LayerStack. A handled failure cleans objects admitted to the previously empty Store; an abrupt crash can leave unreachable rows, but cannot expose a partial LayerStack.

The complete user journey also stays visible in the evidence. At 100,000 files, Workspace Create is 12.556 ms and a localized ten-byte Commit is 3.857 ms. Exact fresh-reopen verification takes 47.367 s because it intentionally reads and authenticates the full 500 MB namespace; complete product time is 50.165 s.

The path is not universal. Direct admission applies only to an eligible first LayerStack in a proven-empty Store with supported root topology and no detected hard links. The apparent 1,001-top-level-directory boundary still requires preflight containment before release claims can be universal.

That is the significance of v0.1.1: not a flattering multiplier against a broken baseline, but a transition to an architecture whose work, memory ownership, failure visibility, and remaining limits can be stated and tested explicitly.

LayerFS is open source. The implementation, benchmark contracts, raw evidence, and roadmap are here. If this engineering direction is useful, issues, workload reports, and a ⭐ Star all help:

https://github.com/Ephemeral-AI-Lab/layerfs
```

Attach:

- `./images/03-v2-pipeline.png`
- `./images/04-cardinality-pressure.png`
