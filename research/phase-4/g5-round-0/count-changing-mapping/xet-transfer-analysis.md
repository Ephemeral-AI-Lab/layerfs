# Xet transfer analysis

Pinned upstream source: `huggingface/xet-core@af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7` (current main observed 2026-08-22). The prior Codex task was used only as a hypothesis source; conclusions below were re-derived from official documentation, the CIDR paper, pinned source, and an external read-only probe.

## Evidence classes

- **Observed in LayerFS source/evidence**: directly read from this checkpoint or sealed local evidence.
- **Observed/documented in Xet primary source**: directly read from official Xet documentation, paper, or pinned `xet-core` source.
- **Derived**: arithmetic or complexity consequence of observed primary-source behavior.
- **Hypothesis**: a proposed LayerFS effect that still needs a shadow measurement.
- **Unavailable**: the public sources or local evidence cannot establish the fact.

No external Xet observation is LayerFS performance evidence.

## Mandatory questions, classified

| # | Answer | Classification and primary basis |
|---:|---|---|
| 1 | Current dirty-range upload is not sublinear end-to-end metadata work: the client scans the complete reconstruction/segment list and the mirrored server collects/scans the complete chunk list. Dirty payload may be expected-local, but metadata is `Theta(S+N)` and expansion may reach EOF. | **Observed/documented in Xet primary source**: [range upload](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_data/src/processing/range_upload.rs#L108-L171), [full composition](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_data/src/processing/range_upload.rs#L336-L419), [chunk-window builder](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_client/src/cas_client/chunk_window_builder.rs#L146-L232). |
| 2 | `MerkleHashSubtree` has expected/observed compactness under suitable pseudorandom cut distributions, not a hard `O(log N)` bound. Hard retained state and one merge are `Theta(N)`; repeated adversarial open streaming can be quadratic. | **Observed/documented in Xet primary source** for the algorithm; **Derived** for bounds: [stable search](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_core_structures/src/merklehash/merkle_hash_subtree.rs#L289-L410), [promotion stop](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_core_structures/src/merklehash/merkle_hash_subtree.rs#L958-L1020), [merge](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_core_structures/src/merklehash/merkle_hash_subtree.rs#L626-L800). |
| 3 | No-cut/every-cut/duplicate/gap-2/gap-8/grinded streams can retain all N open nodes, have no finite pre-EOF resynchronization bound, expand dirt to EOF, change a suffix-linear number of nodes, and require linear memory; repeated merges can do `Theta(N^2/b)`. | **Derived** from the pinned cut/stability/merge rules. Public fixed keys make chosen-input modulo grinding possible; [modulo representation](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_core_structures/src/merklehash/data_hash.rs#L120-L125), [fixed domain keys](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_core_structures/src/merklehash/data_hash.rs#L271-L322). |
| 4 | Gap summaries add little to LayerFS same-count edits, may improve ordinary count-changing resynchronization, add no reopen authority, do not improve range/full-reconstruction/materialization Big-O, and may improve ordinary historical node sharing while making GC/object count harder. | **Observed in LayerFS source/evidence** for current K64/F64 operation shapes; **Derived** for non-benefits; **Hypothesis** for ordinary count-change/history sharing. |
| 5 | Xet's content-defined Merkle aggregation supplies semantic file identity/composition; current physical reconstruction remains a separate flat ordered term list. It is not a persisted authenticated reconstruction tree. | **Observed/documented in Xet primary source**: [hashing](https://huggingface.co/docs/xet/hashing), [file reconstruction](https://huggingface.co/docs/xet/file-reconstruction), [download protocol](https://huggingface.co/docs/xet/download-protocol). |
| 6 | Importing xorbs, shards, global indexes, HMAC lookup, compression, or adaptive network concurrency helps no measured local LayerFS bottleneck and adds carrier/index state. | **Observed/documented in Xet primary source** for roles: [xorb](https://huggingface.co/docs/xet/xorb), [shard](https://huggingface.co/docs/xet/shard), [deduplication](https://huggingface.co/docs/xet/deduplication); **Derived** rejection against LayerFS's measured local SQLite path. |
| 7 | A Xet-inspired profile adds a new canonical grouping/profile, golden vectors, migration/downgrade rules, more objects/SQL, hard caps/fallback, grinding threat model, retained-root/GC traversal, crash-safe index deletion ordering, and storage/resource gates. | **Derived** from Xet rules and **Observed in LayerFS source/evidence** profile/authority constraints. Xet's own [GC fix record](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/docs/simulation-client-gc-fixes.md) documents stale-index/immutable-shard hazards. |
| 8 | LayerFS can research persisting authenticated reference-tree/range-summary nodes instead of flat terms; that could improve expected count-change composition, but K64/F64 already supplies an authenticated persistent reconstruction tree and no candidate is qualified. | **Observed in LayerFS source/evidence** for the current tree; **Hypothesis** for a content-defined persistent shadow. |
| 9 | Semantic content identity, authenticated mapping/reconstruction identity, and noncanonical physical SQLite/extent/carrier location should remain separate. The separation is justified now as architecture discipline; a new canonical grouping is only Canonical-v3 research. | **Derived** from both architectures; **Hypothesis** for any Canonical-v3 implementation. |
| 10 | Content-defined boundaries can improve expected count-change constants/resynchronization; they do not change hard adversarial Big-O without caps/fallback. Publication ordering, idempotency, vectors, fragmentation counters, and GC/index rules improve evidence/safety/constants rather than Big-O. | **Derived**, based on the sources above and [upload ordering/idempotency](https://huggingface.co/docs/xet/upload-protocol). |

## Primary-source index and protocol coverage

- Overview/API/chunking: [Xet docs](https://huggingface.co/docs/xet/), [API](https://huggingface.co/docs/xet/api), [chunking](https://huggingface.co/docs/xet/chunking).
- Identity and reconstruction: [hashing](https://huggingface.co/docs/xet/hashing), [file reconstruction](https://huggingface.co/docs/xet/file-reconstruction), [download](https://huggingface.co/docs/xet/download-protocol).
- Upload ordering, idempotency, term verification/proof of possession: [upload protocol](https://huggingface.co/docs/xet/upload-protocol).
- Carriers/indexes/dedup/locality: [xorb](https://huggingface.co/docs/xet/xorb), [shard](https://huggingface.co/docs/xet/shard), [deduplication](https://huggingface.co/docs/xet/deduplication), [fragmentation hysteresis](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_data/src/deduplication/defrag_prevention.rs#L1-L94).
- HMAC-keyed global-dedup metadata: [pinned shard interface](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_data/src/processing/shard_interface/wasm.rs#L120-L129).
- Design paper: [Git is for Data, CIDR 2023](https://www.cidrdb.org/cidr2023/papers/p43-low.pdf).
- Managed production server behavior beyond official public/mirrored code: **Unavailable**.

## Architecture separation

Xet separates semantic file identity, chunk identities, flat reconstruction terms, xorb carriers, shard/global-dedup indexes, and partial-gap Merkle summaries. LayerFS already has a persistent authenticated reconstruction tree; Xet's Merkle aggregation tree is not its persisted reconstruction recipe. Transferable design lessons are therefore separation of identity/mapping/carrier concerns, immutable-data-first publication-last ordering, idempotent content-addressed upload, golden vectors, and GC/index consistency—not xorbs or shards.

## Exact aggregation rule

Current and historical code effectively create ordinary groups of **3–9** children, not 2–9. Starting at child index 2, the first hash with `hash % 4 == 0` closes the group; otherwise child 9 forces closure. A final remainder may contain one or two children, and a singleton tail is hashed as a unary internal node.

Exact per-level parent bounds are `ceil(n/9) <= p(n) <= ceil(n/3)` and height lies between `ceil(log_9 n)` and `ceil(log_3 n)`. Under independent cut bits, mean ordinary group size is `22389/4096 = 5.466064453125`, not 4.

Primary source: [current grouping](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_core_structures/src/merklehash/aggregated_hashes.rs#L3-L53), [historical off-by-one behavior](https://github.com/huggingface/xet-core/blob/5f13f91d1c0df5411033ac01ee2b69d9f8169a22/merkledb/src/internal_methods.rs#L89-L115), and [official hashing documentation](https://huggingface.co/docs/xet/hashing).

## Adversarial boundary and merge behavior

`MerkleHashSubtree` needs three natural cuts whose adjacent gaps are 3–7 to find a stable boundary. No-cut, every-cut, duplicates, gap-2, and gap-8 streams never qualify. An open summary can therefore retain every leaf: storage and a merge are hard `Theta(N)`, not hard `O(log N)`. Repeated fixed-batch open merges can do `Theta(N^2/b)` cumulative work. Fully closed summaries retain one root, but construction still scans/copies the input.

**Derived adversarial matrix from pinned source:**

| Input | Retained open summary | Resynchronization | Dirty-window expansion | Changed persistent nodes if imported | Worst logical/transient node memory |
|---|---:|---|---|---|---|
| No natural cut | `Theta(N)` / all leaves attainable | no finite pre-EOF bound | to EOF when no stable-sized clean pair | `Theta(remaining suffix)` | `>=40N` retained; current builder/copies `>=120N` bytes before allocator overhead |
| Every child cuts | `Theta(N)` because cut gaps are 1 | no qualifying gap pair | independently may reach EOF | `Theta(remaining suffix)` | same linear bound |
| Repeating gap 2 | `Theta(N)` | none under 3–7 rule | may reach EOF | suffix-linear | linear |
| Repeating gap 8 | `Theta(N)` | none under 3–7 rule | may reach EOF | suffix-linear | linear |
| Duplicate hash | reduces to all-cut or no-cut according to residue | none in either class | zero/max-chunk duplicate file can reach EOF | suffix-linear | linear |
| Public-hash grinding | attacker selects these classes, including at parent levels | attacker-controlled to EOF | attacker can select unstable chunks/windows | suffix-linear per affected level | linear; repeated open streaming can be `Theta(N^2/b)` work |

**Observed/documented in Xet primary source:** dirty-range upload snaps insertion/deletion/replacement intervals outward to reconstruction-segment/chunk windows, coalesces overlaps, and composes uploaded dirty windows with unchanged-gap summaries into a new flat reconstruction ([window selection](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_data/src/processing/range_upload.rs#L119-L171), [composition](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_data/src/processing/range_upload.rs#L273-L419)). Append/truncate are supported edit shapes at the file edge. **Derived:** insertion/deletion create a one-position parsing phase shift that is expected to rejoin on ordinary hashes but has no hard rejoin distance; same-count replacement can also shift a boundary if marker class changes; append/truncate affect the right edge in a persistent tree, while current Xet still scans/composes the full flat reconstruction metadata. Ordinary-case LayerFS gains remain **Hypothesis** until a metadata-only shadow exists.

The pinned code's open-subtree comments and random-only tests describe expected behavior under pseudorandom diverse hashes, not an adversarial resource guarantee. Sources: [stable cut search](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_core_structures/src/merklehash/merkle_hash_subtree.rs#L289-L410), [failure to promote](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_core_structures/src/merklehash/merkle_hash_subtree.rs#L958-L1020), and [merge](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_core_structures/src/merklehash/merkle_hash_subtree.rs#L626-L800).

**Supporting observation, not retained evidence:** an external read-only probe at the pinned commit reported that 16 MiB of zeros becomes 128 identical 128-KiB chunks and that open-boundary summaries retained all 128 nodes; its command/output was not retained in this worktree and it is not LayerFS evidence. **Observed/documented in Xet primary source:** the constant-input test produces maximum-size chunks. **Derived:** extending that documented pattern to 100 GiB yields 819,200 nodes, 31.25 MiB binary node payload, roughly 70.31 MiB decoded JSON node text, and at least 93.75 MiB node payload during construction—already incompatible with LayerFS's 20-MiB process envelope.

## Dirty-range limitation

The current upload client fetches and scans the complete reconstruction, and the mirrored server collects/scans the complete chunk list. A dirty interval closes only after two consecutive clean chunks sized 16–120 KiB; otherwise it can extend to EOF. Zero input produces forced 128-KiB chunks, so a leading one-byte edit can re-upload the whole file. Sources: [range upload](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_data/src/processing/range_upload.rs#L108-L171), [dirty-window builder](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_client/src/cas_client/chunk_window_builder.rs#L49-L84), and [constant-input chunk test](https://github.com/huggingface/xet-core/blob/af1a3ff93e02d60b9e7c3790d9cb143e2e5353a7/xet_data/src/deduplication/chunking.rs#L515-L531).

Public fixed BLAKE3 domain keys do not prevent chosen-input grinding. File salting occurs after aggregation and shard HMAC translation is for lookup; neither changes cut positions. Global-dedup modulus eligibility even implies the modulo-4 natural-cut predicate.

## Adopt / shadow / reject

| Decision | Item |
|---|---|
| Adopt in contracts | identity/mapping/carrier separation; immutable data before publication; idempotency; exact vectors; GC/index ordering; locality/fragmentation counters |
| Shadow only | persistent content-defined grouping over existing `(raw_length, ObjectId)` references, with hard node widths, caps/fallback, and adversarial fixtures |
| Negative/reference arm | exact Xet 3–9 grouping and current gap-summary behavior |
| Reject for local core | xorbs, shards, flat recipes, global dedup cache/index, HMAC lookup machinery, compression, adaptive network concurrency |
| Reject for authority | immutable-gap summaries as proof against unreported mutable-file changes |
