# Historical retained-history storage baseline for SP1

> **Status: Research; informative and not a product contract.**

Read-only historical evidence inventory, 2026-10-02. No new benchmark,
no new source qualification and no historical receipt relabeling. SP1 remains
prospective/unimplemented. Owner asks to consider stride10/3/1 after SP1.

The latest reused Phase B Family2 records are r046 (10/3) and r047 (1),
case version v4. Logical bytes are cumulative complete file bytes across selected
states, not size of one checkout. Total retained allocation is C2+C5 at-run
st_blocks*512. Ratio = cumulative logical / total retained allocation. It combines
deduplication, compression, deltas and storage overhead, not Zstandard alone.

|Stride|States|Logical B|C2 allocated B|C5 allocated B|Total B|Logical/total|Reduction|
|---|---:|---:|---:|---:|---:|---:|---:|
|10|17|561010345|52428800|45056|52473856|10.6912x|90.6465%|
|3|53|1676767835|65011712|131072|65142784|25.7399x|96.1150%|
|1|157|4936693030|85983232|196608|86179840|57.2836x|98.2543%|

|Stride|Canonical objects B|Pack bodies B|Canonical/pack bodies|Payload PREFIX selections|Pooled delta leaves|
|---|---:|---:|---:|---:|---:|
|10|380559460|45594892|8.3465x|38154|747|
|3|589480854|57146716|10.3152x|54324|3016|
|1|871337620|74887668|11.6353x|70626|7719|

Canonical/pack-body ratio includes physical encoding, deltas and pooling; it
is not isolated codec compression. prefix_selected includes eligible chunk and
whole-file payload selections, not a chunk-only count. Retained prefix_records
also include pooled delta leaves; do not count them all as chunk deltas.

All three have semantic/storage/cleanup PASS under the owner-approved v4
up-to10% allocation profile. Original v0.1.6 reference totals are49344512/
64024576/83947520 B (ratios11.3693/26.1894/58.8069x). Current deviations are
+6.3418/+1.7465/+2.6592%; they do not pass the original strict-below targets.
Historical strict FAILs remain unchanged. Numeric timing remains INELIGIBLE;
this does not invalidate the separately measured storage outcomes.

r046 source a12ab932e969fabcd1193c62611bde5caf5f2170; r047 source
b2ea44f0f0f3e0112f8b862ce71f2087e018c7a7. Compilation seal shared:
00914f9020517febcec8a794b89f16e03ab8d05e8de5f40c5d89767608c4f72a.
This is in-process C1/C2/C5 history, not SDK Exec/FUSE latency. The frozen
157-checkpoint corpus is separate from the owner requested entire current
deepseek-harness checkout (including untracked/ignored content).

Historical reports:
[r046](../../../../issues/286/experiments/20260930-history-regression-r046.md),
[r047](../../../../issues/286/experiments/20260930-history-stride1-regression-r047.md).

Read-only raw receipts were SHA256-checked against the committed compact index;
exact owned copies and arithmetic are published in
[the evidence manifest](sp1/evidence/history-baseline/manifest.json) and
[derived arithmetic](sp1/evidence/history-baseline/derived.json). Exact raw hashes:
- stride10: `c5ece417804841ecc42c20473f8f0524706a4fc2e1afd99947515213e9fee33a`.
- stride3: `00a408c4f1086c367d5331e8d20018d33e45ef7fbc6a1099511b38e38e5fb237`.
- stride1: `ba9e04c731bd166b2dea47ec86d906853d49a385a754b4124e1b6646e666c547`.

After SP1, use existing three selections/oracles and a prospectively declared
MinIO/global-SQLite storage adapter/profile in the existing runner. Report encoded
file-payload pack bodies plus encoded SQL metadata/pool records and groups,
MinIO object bytes, actual provider disk allocation, required SQLite
allocations, temporary high-water, duplicate physical packs, FULL/PREFIX/pooling
counts and base-chain reads separately. Never compare MinIO payload bytes alone
to historical C2+C5 allocated bytes. Preserve one sample per case/profile and
the existing lane-specific bounds; stride1 remains run-only, not tuning input.
No campaign run is authorized/performed merely by this read-only inventory.
