# #237: read-only attribution of the 100k Core SDK Store

This is a **three-agent, read-only analysis** of the closed [one-shot Core SDK release Store](evidence/sdk-100k-release-20260924/store-geometry.json) and retained v0.1.6 evidence. No public Init, verifier, build, database write or source mutation was performed. The Core run used the seed-1 SHAKE 100,000-file/500,000,000-byte manifest `23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`.

## Where the Core allocated bytes are

| Disjoint Core component | Bytes | How measured |
| --- | ---: | --- |
| Declared used length within 2,082 pack BLOBs | **510,615,731** | Pack header `used` across all rows; includes framing and reserved group-directory region, not only source payload |
| Pack BLOB capacity beyond declared used length | **35,168,077** | `sum(length(data))` **545,783,808** minus declared used **510,615,731** |
| SQLite Store pages outside BLOB lengths | **8,228,864** | Store `st_size` **554,012,672** minus BLOB lengths **545,783,808** |
| Filesystem allocation beyond Store `st_size` | **4,046,848** | Store `st_blocks × 512` **558,059,520** minus `st_size` |
| Separate History database | **86,016** | 21 SQLite pages, apparent and allocated equal |
| **Combined allocated Store + History** | **558,145,536** | Sum of the five disjoint rows above |

All **2,082** Core pack rows are fixed **262,144-byte SQLite BLOBs**. The 35,168,077 B of capacity beyond declared used length splits into **17,234,430 B** in 1,217 whole-file packs, **17,143,035 B** in 842 native packs, **647,624 B** in seven ordinary packs and **142,988 B** in 16 pooled-metadata packs. There is also **4,648,820 B of unused group-directory slots inside the declared used length** (within 4,789,248 B of fixed directory reservation). That internal region is **nested in the first table row**; adding it again to the disjoint sum would double-count it.

The mechanism is explicit: Core [`pack_capacity`](../../../crates/layerfs-storage/src/pack/layout.rs) reserves the full 256-KiB limit for each nonsingleton pack, and [`insert_pack`](../../../crates/layerfs-storage/src/sqlite/write.rs) inserts `zeroblob(capacity)` before in-place writes. The reserved directory likewise keeps group-body offsets stable for appends. This avoids rewriting whole BLOBs on each append. The [v0.1.6 admission path](../../../../crates/layerfs-layerstack-store/src/objects/admission.rs) inserts assembled variable-length pack BLOBs. Shrinking Core packs after Save might recover pages but could introduce whole-BLOB rewrites; no such treatment or time/space tradeoff has been measured.

Read-only SQLite `dbstat` accounts for the Store's **135,257 × 4,096-byte pages** with zero freelist pages: `object_packs` B-tree **546,856,960 B** (including **545,783,808 B** BLOB lengths), `objects` B-tree **6,148,096 B**, `content_signatures` **704,512 B**, and all other tables/indexes **303,104 B**. History has **21 × 4,096-byte pages**, also zero freelist. The query method was read-only SQLite URI `mode=ro&immutable=1`, `PRAGMA page_size/page_count/freelist_count`, `SELECT count(*), sum(length(data)) FROM object_packs`, and `SELECT name, sum(pgsize) FROM dbstat GROUP BY name`; pack headers supplied the per-row declared used lengths and group-directory occupancy.

## What can be assigned to the historical gap

The historical [v0.1.6 #152 100k row](../../../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md) reports **515,366,912 B apparent / 520,560,640 B allocated** for its single Store. Its fixture digest `6fc793a9703bd0a21066f9fb12622c3451b16bd6ad7ef8b7382351351ac80a7e` differs from Core's SHAKE input. The old closed database and raw `perf.jsonl` are absent from accessible worktrees. Its pack count/BLOB lengths, B-tree pages and freelist therefore remain **NOT_MEASURED** for this 100k row.

| Arithmetic across the two historical rows | Bytes |
| --- | ---: |
| Core Store + History apparent minus old Store apparent | **+38,731,776** = 9,456 × 4,096-byte pages |
| Core filesystem allocated-minus-apparent | **4,046,848** |
| Old filesystem allocated-minus-apparent | **5,193,728** |
| Difference in filesystem allocation excess | **−1,146,880** |
| **Combined allocated difference** | **+37,584,896** = +38,731,776 − 1,146,880 |

Core's **35,168,077 B** post-used BLOB reservation is numerically **90.8%** of the +38,731,776 B apparent difference. This identifies a dominant physical cost in Core; it does **not** prove that exactly those bytes are a v0.1.6-to-Core regression. The old row used different source bytes and its pack geometry is unavailable. The **4,648,820 B** internal directory vacancy is another Core reservation, not an additional disjoint file-length delta. Do not compare Core BLOB capacity to v0.1.6 *canonical bytes*: they measure different things.

An earlier [exact-source 10k SQLite comparison](v016-core-sqlite-head2head.md) independently found **24,856,188 B** of Core post-used pack reservation within a **29,184,000 B** apparent Core–v0.1.6 gap. That supports the fixed-capacity mechanism but cannot substitute for missing 100k reference geometry.

## Next proof and decision

Run the already planned **single v0.1.6 release Init on an independent copy of the exact Core 100k SHAKE source**, retain its closed Store and full reopened readback, and collect the same `page_size/page_count/freelist_count`, `dbstat`, pack-row/BLOB totals, object/metadata/signature counts, apparent and allocated bytes. Do not rerun the unchanged Core release arm. Separate source/content-format effects, reserved pack tail, internal directory vacancy, B-tree/index pages and filesystem allocation slack. If the pack-space gap persists on matched bytes, evaluate the smallest prospective pack-finalization treatment with one matched control/candidate pair, operation time, bounded memory, SQLite BLOB/4-KiB-page policy, full readback and #229 sparse-history compactness all visible. Do not treat the present five-second SDK time as a waiver of the space issue.
