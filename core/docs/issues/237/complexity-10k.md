# #237: native 10k Init complexity and batching map

> **Status:** Research; informative and not a product contract.

This note reads the current Core source and retained D5, D11 and D12 diagnostics;
it adds no product edit or performance sample. The scope is the public
`ImportNativeDirectory` route with 10,000 files, 100 data directories,
**300,000,000 total logical bytes including the 100 MB anchor**, four file
workers, one C2 Save owner, and 4 KiB SQLite pages. Let `F` be files, `D`
directories, `B` logical bytes, `O` finalized objects, `G` sealed groups and
`W` preparation waves. At 10k, `F=10,000`, `D=100`, `O=24,562` objects handed
to the owner, `24,364` newly stored, `198` reused, `G≈7,696–7,722`, and
`W≈79–80` committed transactions. The object/group/wave counts are observed
in [D11](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d11-ingest-counts/daemon-host/init_namespace/namespace-10000/receipt.json)
and [D12](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d12-collision-detail/daemon-host/init_namespace/namespace-10000/receipt.json),
which have different instrumentation identities and are **not a time pair**.
Both rows have zero resident source *payload* pages at the immediate recheck;
metadata/dentry cache is unqualified and the official contract remains
`source-cache-uncontrolled-v1`. D11 skipped full verification; D12 also lost a
daemon telemetry event and is `INCOMPLETE`.

## Work along the public path

| Stage | Source and complexity | 10k count or observed time | Implication |
| --- | --- | --- | --- |
| Source scan | [`scan_and_save`](../../../crates/layerfs-service/src/operation/import_native.rs#L45-L113) enumerates and sorts each directory's children, stats each entry, and retains one `PreparedEntry` and file `Job`. Work is `O(F+D+Σ dᵢ log dᵢ)` with `dᵢ` children in directory `i`; this fixture fixes 100 per data directory, so sorting is effectively linear in `F`. | D11 scan 48.514 ms; 10,000 jobs. | An arbitrary single huge directory raises the sort term. This fixture does not. The retained entries/jobs use `O(F+D)` memory. |
| File construction | Four workers each open and validate a source file, then [`construct_stream`](../../../crates/layerfs-content/src/file/content.rs#L273-L315) probes at most 128 KiB and hashes/chunks the rest. The source-byte work is `Ω(B)`; per-file open/stat and object creation add `Ω(F+O)`. | D11 file child 1.146653 s; four overlapping worker walls sum to 4.319260 s and send walls to 2.879587 s. The fixture's probes read 99,414,072 B before small/chunk dispatch ([size census](prefix-reserve-10k.md#exact-work-implied-by-the-source)). | Worker sums include overlap and blocked sends; they are not added to caller wall. A 4 KiB initial reserve lost its standalone test and produced 7,506 reallocations ([negative result](prefix-probe-experiment.md)). |
| Bounded handoff | [`save_files`](../../../crates/layerfs-service/src/operation/import_native.rs#L143-L189) sends one `Message::Object` per finalized object through an eight-message channel. Receiver calls one [`SaveHandoff::accept`](../../../crates/layerfs-storage/src/cas/store.rs#L851-L860) per object. Work is `Θ(O)` sends/accepts plus blocking. | D11: 24,562 sends/accepts, 785.607 ms owner accept, 357.453 ms receiver wait. D12: 24,562, 771.409 ms accept, 393.324 ms wait. | Blocked send time is backpressure, not proven queue overhead. The owner and workers run concurrently; `max(producer, owner)` shapes the critical path. |
| C2 preparation | [`PendingBatch`](../../../crates/layerfs-storage/src/cas/batch.rs#L13-L75) holds ≤512 objects and <4 MiB ordinary wave bytes. [`flush_wave`](../../../crates/layerfs-storage/src/cas/save.rs#L34-L141) batches membership and presence queries, then offers each distinct object. Indexed SQLite lookups cost approximately `O(O log M)` for `M` Store rows, plus `O(O)` bounded-map work per wave. | D11: 68 presence query waves; 79 commits. D12: 66 presence waves; 80 commits. | It already avoids one transaction/presence query per object. Raising batch/deadline/cache limits is not a complexity fix. |
| C2 group/pack/write | [`seal_group`](../../../crates/layerfs-storage/src/cas/placement.rs#L110-L219) frames one bounded group, places it, writes its pack increment, inserts its rows, and publishes same-save locators. [`write_in_place`](../../../crates/layerfs-storage/src/sqlite/write.rs#L130-L168) writes at most three BLOB spans rather than rewriting an entire pack. Work is `O(B+O+G)` plus DB index costs. | D11: 1,257 pack creations + 6,439 appends = 7,696 placements; 7,750 object-row `INSERT` statements; 192.313 ms disjoint SQL, 227.454 ms COMMIT. D12: 7,722 placements; 7,776 statements; 188.689/227.521 ms SQL/COMMIT. | Row inserts are already multi-row up to 128 per statement ([writer](../../../crates/layerfs-storage/src/sqlite/write.rs#L170-L194)); the many statements come from many small sealed groups, not one SQL statement per object. A one-pack queue cannot be added without changing same-save lookup semantics ([feasibility finding](c2-grouping-prototype.md)). |
| Collision validation | [`validate_candidates`](../../../crates/layerfs-storage/src/cas/collision.rs#L15-L57) calls the already slice-capable [`lookup::candidates`](../../../crates/layerfs-storage/src/sqlite/lookup.rs#L69-L109) for each newly written row. Work is `O(O log M)` DB probes; same-save exact collision comparison is required. | D12 charged 43.306 ms for the file Save's one-row queries, inside 44.850 ms total validation, about 1.78 µs per inserted row. | Batching 24,364 calls into at least `ceil(24,364/128)=191` query pages has a **43.306 ms measured local ceiling** before new mapping/ordering costs. D12's preregistered 50 ms threshold rejected it as the next 10k treatment ([detail](c2-detail-diagnostic.md)). |
| C1 namespace | [`build_namespace`](../../../crates/layerfs-service/src/operation/history_bootstrap.rs#L115-L198) allocates 10,101 serials/inodes and 101 directory updates. [`update.rs`](../../../crates/layerfs-content/src/filesystem/update.rs#L157-L419) validates 10,100 observed edges, registers values, reduces references and builds sorted inode pages. Generic ordered maps are `O(N log N)`; B+tree page construction is linear in entries at bounded page size. | Native fresh build makes **30,302 reducer `entry` requests**: 10,101 initial values + 10,100 bindings + 101 directory values + 10,000 file values repeated ([derivation](c1-10k-next.md#source-derived-work-still-on-the-basenone-route)). | The original sparse-run rewind did much more than these calls. The current gap fix removes that 10k mechanism, but the reducer still spills/merges. |
| C5 publication | The Service reserves one contiguous inode range, saves the tree, then calls [`initialize_layerstack`](../../../crates/layerfs-service/src/operation/history.rs#L305-L346) once. | D11 `history.finish_tree_save` 29.278 ms. C5 has no dedicated child timer in the receipt. | There is no evidence of a per-file C5 loop; do not assign the unlabelled caller remainder to C5. |

## Where work grew faster than the input

**D5 found a real sparse-run rewind, but it is historical to the current
source.** Before the gap fix, [`RunStore::find`](../../../crates/layerfs-content/src/filesystem/references/runs.rs#L326-L395)
could reread the same low prefix of a newer sparse tier for many ascending
serials held by an older tier. D5 counted **9,011,202 run-row reads after
directory effects** and **13,869,215 by the 5,120-value milestone**
([raw D5 stderr](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d5-reducer-counts/daemon-host/init_namespace/namespace-10000/service.stderr)).
The source-derived native model predicts **18,484,823** full-run lookup row
returns before the fix, versus **33,331** with the current per-tier
proven-absent interval; its 100k numbers, **1,375,401,339 vs 353,971**, are
models, not runtime observations ([analysis](c1-scaling.md#deterministic-count-model)).
That old gap has `O(GapRequests × RepeatedPrefix)` work. Current 10k Init
should not be described as still paying those 18 million reads.

**The remaining C1 work is extra passes, not free merely because it is bounded.**
At 10,101 inodes and a 4,096-row pending ceiling, the same source model
predicts **83,551 96-byte ordering rows written**, or **8,020,896 bytes**
through spill, merge and final consolidation ([breakdown](c1-10k-next.md#source-derived-work-still-on-the-basenone-route)).
It is `O(N log(N/K))` merge work at pending capacity `K`, plus the
`O(N log N)` map operations above. The current gap fix does not remove it.
For arbitrary backward/interleaved requests, the run scan may still restart,
so its worst-case lookup cost is higher than the ascending native pattern;
this note makes no whole-product linearity claim from one fixture.

**The indexed C2 path has a logarithmic term and large fixed call counts.**
Lookup of an object ID in SQLite's indexed `objects` table is not an
all-objects scan, yet 24,364 newly stored rows and thousands of small-group
seals incur thousands of calls and transaction steps. D12 isolates only
43.306 ms in collision queries; it does not show a new C2 `O(N²)` scan.
The pack writer already uses incremental BLOB writes, so 7,722 pack placements
do not imply 7,722 full 256 KiB rewrites. Its 4 KiB DB page size stays fixed.

Neither `O(log N)` nor `O(1)` **total** Init is possible while this operation
must read and authenticate `B` source bytes, inspect `F` files, and persist
`O` objects inside the caller timer. Those bounds are `Ω(B+F+O)` total work.
Constant-time or logarithmic *per-object lookup* is useful; it cannot remove
the scan, read, hashing, CDC, exact CAS, and Store writes. Four workers can
shorten latency when their work overlaps the one C2 owner, while total CPU and
I/O remain. Moving those steps to setup or serving them from warm source pages
would change the measured operation and is excluded.

## Three candidates, ranked for the 10k lane

| Rank | Change and exact call reduction | Added resident bound | Semantic barrier and evidence gate |
| --- | --- | --- | --- |
| **1. Direct fresh-build counts in C1** | For `base=None`, derive counts from observed directory edges and stream values to the existing inode writer. This can remove **30,302 reducer entry requests** and the modeled **83,551 run-row writes** while retaining 10,100 observed edge increments. A separate one-pair prototype reduced the public 10k call **1.563611 → 1.317539 s (15.7%)**, but is a diagnostic ([record](c1-direct-prototype.md#attempt-ledger)). | One `u64` per 10,101 sorted serials is **80,808 B**, charged against the declared ordering budget; existing input vectors and sorted-engine scratch remain. Reusing validation's existing additions map could avoid this extra vector, but must preserve observed-edge ownership and error ordering ([proposal](c1-10k-next.md#one-candidate-stream-a-new-filesystem-directly-after-validation)). | The runner randomized scope/stack, so the prototype's exact-root/whole-Store gate failed; four existing fresh-build ordering-resource assertions also failed. Freeze identities, preserve root/count/cleanup/quota semantics, then run full readback and the #229 sparse lane. It does not alone approach 700 MB/s. |
| **2. C2 group publication redesign within a wave** | D11 made **7,696** group placements and **7,750** insert statements. A hypothetical wave-local 128-row insert floor is `ceil(24,364/128)=191` statements; that is an *upper bound on calls removable*, not a feasible patch or predicted saving. Grouping up to one 256 KiB pack could likewise reduce BLOB-open/locator calls, subject to actual group sizes. | Bound queued **encoded** group bytes to one 256 KiB pack per active lane and keep the existing <4 MiB wave and transaction limits; separately bound member/locator metadata. Do not cache unbounded canonical bytes. | The existing seal publishes locators needed immediately for same-save reuse, delta bases, reads and collision checks. The small placement-only prototype was rejected before timing ([source barrier](c2-grouping-prototype.md#the-barriers-found-in-source)). A correct treatment spans `cas/placement`, `cas/save`, selection, and same-save reads, and must preserve 4 KiB pages, root/readback and #229 Store compactness. D11 SQL/COMMIT times indicate where to instrument, not savings to subtract. |
| **3. Bounded producer slab handoff** | The current channel sends **24,562 objects** individually. A slab of at most 512 objects could theoretically reduce object-message count toward `ceil(24,562/512)=48`, but a 256 KiB slab byte bound will require more messages. The owner would still call `SaveOperation::accept` per object and perform its C2 work. | Four queued slabs × 256 KiB = **≤1 MiB queued encoded/canonical handoff bytes**, plus four producer partial slabs ≤1 MiB; define exact ownership and reject oversize objects or use the existing singleton rule. | D11's **2.880 s summed sender time** mostly records backpressure, not removable channel overhead; receiver wait is 0.357 s and owner accept 0.786 s. First separate send-lock/wakeup cost from blocked waiting and preserve per-object error/ack order. No treatment timing or speedup claim exists. |

The immediately easier-looking options are smaller or disproven here: D12's
collision-query batch has a 43.306 ms measured local ceiling, the smaller
prefix reserve was slower in a synthetic allocation diagnostic, and the
one-pack placement-only queue violates same-save availability. The candidate
ranking therefore orders *potential work removed under a correct design*, not
promised time savings. Any new run needs its own preregistered source and
fixture identities, one sample per arm, payload-page invalidation without
warming, complete CPU/memory/Store accounting, and a separate correctness
proof. Keep the database at 4,096-byte pages.
