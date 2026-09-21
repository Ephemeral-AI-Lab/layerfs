# Retained-history performance: current progress and next-agent handoff

> **Status: four improvements merged; next work is attribution of remaining data-access, reuse-boundary and execution-pipeline differences.** Latest product checkpoint: [`5dc51b4f3`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/5dc51b4f39e3965db37eec8f539982dfebdbba80), PR #199. Timing results are single-sample diagnostics, not release admission. Keep this issue open.

## Start here

Handoff published in [PR #201](https://github.com/Ephemeral-AI-Lab/layerfs/pull/201), commit `9fe8eb290072d09594cde3ba67c3bbd1963d5e56`.

**[Next-agent handoff: data access, reuse boundaries and execution pipeline](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/issue190-data-access-handoff.md).** It contains source/corpus pins, raw evidence, rejected directions, required measurement and correctness proofs, reproduction guidance and lock protocols.

The owner considers **one second worthwhile**, and accepts a **small allocation overage for good time reduction**. Preserve all measured misses; no unspecified tolerance or unlimited storage waiver is implied. The owner also explicitly stopped cross-task coordination messages once the lock mismatch was fixed: use the shared locks directly, without contacting #192 or arranging resource windows.

## Completed and retained

| PR | Improvement | Production LOC delta |
|---|---|---:|
| [#194](https://github.com/Ephemeral-AI-Lab/layerfs/pull/194) | Batched directory-parent lookups and bounded authenticated base-record reuse | +49 |
| [#195](https://github.com/Ephemeral-AI-Lab/layerfs/pull/195) | Pooled physical-group telemetry and reuse through the existing 512 KiB cache | +141 |
| [#196](https://github.com/Ephemeral-AI-Lab/layerfs/pull/196) | Existing prepared-statement cache for catalogue queries | −1 |
| [#199](https://github.com/Ephemeral-AI-Lab/layerfs/pull/199) | Group compression level 19 → 1; payload level 3 unchanged | 0 |

Current product total at this checkpoint: **85,722 LOC**, comprising reference 65,417 and core 20,305. Source-size accounting excludes tests, comments, harness and documentation and includes runtime SQL. The improvements are separate campaigns; do not compound their percentages.

[PR #197](https://github.com/Ephemeral-AI-Lab/layerfs/pull/197) tested and reverted the `zero_count_serials` New-row filter: operation difference 156,679,583 ns, target-phase difference 53,181,962 ns. It did not meet the revised one-second criterion. [PR #198](https://github.com/Ephemeral-AI-Lab/layerfs/pull/198) completed the legacy source comparison that motivated #199; its proposed codec experiment is now completed, not still pending.

## Latest matched measurement: group level 19 versus 1

| Quantity | Stride10 baseline | Stride10 retained candidate | Stride3 baseline | Stride3 retained candidate |
|---|---:|---:|---:|---:|
| Operation ns | 22,506,420,003 | **20,914,489,418** | 58,410,516,121 | **56,577,520,957** |
| Operation reduction ns | — | **1,591,930,585** | — | **1,832,995,164** |
| Store begin + accept + finish ns | 11,050,102,332 | 9,364,846,704 | 22,624,366,586 | 20,161,248,171 |
| Apparent Store B | 49,053,696 | 49,324,032 | 61,767,680 | 62,152,704 |
| Allocated Store B | 49,651,712 | 50,249,728 | 62,783,488 | 62,152,704 |
| Verification work ns | 5,617,963,792 | 5,405,493,708 | 18,824,407,917 | 18,904,055,292 |

Observed operation reductions: **7.073228815545978% / 3.138125265325286%**, one sample per case/arm. Save reductions are partly offset by increased read work. Exclusive codec CPU remains NOT_MEASURED. The large stride10 complete-wall reduction included unrelated untimed corpus-read variation and is not attributed to compression.

Stride10 numeric allocation misses are **307,200 / 905,216 B** above 49,344,512 B; matched growth is 598,016 B, accepted under the owner's time/space ruling. Apparent growth is 270,336 / 385,024 B. Both stride3 allocated readings meet 64,024,576 B. Its allocation decrease despite larger apparent content is filesystem variation, not an algorithmic storage reduction. Candidate stride3 verification is 79,647,375 ns slower, still within 20 seconds.

All **70 roots and complete sorted ObjectId/role/canonical-length inventories match**. Independent review rehashed both measured binaries and all four Stores and reproduced inventories/arithmetic. Physical Store bytes intentionally differ. All four identity-matched verifiers pass 10/20-second targets with 1,083 / 3,377 sampled paths per arm and zero mismatch/missing/unexpected; verification is not exhaustive.

**494 core tests, 117 harness tests, 12 example targets, core Clippy/fmt, the 122-file boundary guard, six guard self-tests and locked builds PASS.** Inherited unchanged harness Clippy/fmt failures remain unresolved; they were not rerun or relabelled as passes. No CI or retired preflight. Measured binary/source identities remain distinct from the comment-corrected final build.

## Remaining gap: tree algorithm aligned, execution details differ

Latest stride10 operation partitions exactly into:

| Disjoint interval | ns |
|---|---:|
| File-content construction | 1,607,234,583 |
| Filesystem update | 9,569,275,834 |
| Store begin + accept + finish | 9,364,846,704 |
| Other child work/bookkeeping | 373,132,297 |
| **Operation sum** | **20,914,489,418** |

Authenticated provider reads occupy **8,609,430,602 ns inside the filesystem interval**. They are nested, not additional time. Latest counters show **79,784 pooled pack fetches / 7,985,771,449 application bytes**, 65,337 value-group decompressions and 2,723 physical-group decompressions. Application bytes are not disk I/O, and provider time is not a measured excess over legacy.

| Area | Established difference | What remains unmeasured |
|---|---|---|
| Data access | Legacy reads pack control data plus a selected group range; current pooled reads copy whole pack BLOBs. Current full-directory validation must survive any change. | Exclusive acquisition/copy/parse cost and attainable selected-range saving. |
| Reuse boundaries | Legacy shares a bounded pool reader across targets in a metadata group; current pool readers are per leaf, with an already-shared physical-group cache. | Repeated value/pack reconstruction attributable to lifetime boundaries, and safe bounded reuse benefit. |
| Execution pipeline | Legacy pipelines producers/admission; the current harness stages construction then admission and copying. | Removable copying/staging cost under the unchanged single-worker and C1/C2 contracts. |

Both tree implementations already apply incremental sorted changes and reuse untouched subtrees. Reviewed stride10 validation read **zero base-directory pages in every state**; a whole-directory scan is not an established cause of this lane. Legacy's 342,355,542 ns namespace counter excludes dirty-directory work charged to Content. It cannot be compared directly with our entire filesystem phase. Both implementations rewrite whole open-pack BLOBs.

Historical Commit is **11,370,679,212 ns**. The current **9,543,810,206 ns** difference (1.8393351028606961×) is an unmatched historical comparison: timer boundaries, effective workers, cache/machine state and canonical object streams differ or are unrecorded. The full causal gap remains unresolved.

## Next-agent work — not yet run

- [ ] Attribute pack acquisition/copy, complete directory parsing, physical/value-group decoding and reconstruction; retain residuals and actual read counters.
- [ ] Select one justified, bounded treatment, with selected-group BLOB reads as the first concrete direction. Preserve full validation, error behavior, read ceilings and existing capacity bounds.
- [ ] Review within-operation reuse separately. Do not move a cache hit before required validation without a precise authenticated lifetime argument.
- [ ] Consider pipeline changes only after measuring copying/ownership cost; distinguish a harness change from a product improvement. No helper-worker workaround.
- [ ] One matched stride10 pair, then stride3 if worthwhile, with separate verification, independent review, issue updates and exact LOC. Reject small/unattributed effects and retain their evidence.

Do not repeat the completed codec experiment or rejected zero-count filter. Do not start subtree summaries, a wider persistent cache or a schema/pipeline rewrite from a source diagram alone. The handoff is a continuation plan, not proof of another saving; no next-agent task or new experiment was launched while preparing it.

## Evidence and remaining qualification

[Next-agent handoff](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/issue190-data-access-handoff.md) · [L40 report](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/README.md) · [Results](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/results.json) · [Inventory](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/store-comparison.json) · [Independent review](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/REVIEW.md) · [Legacy comparison](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-legacy-review-20260920T031633Z/SYNTHESIS.md) · [Active ledger](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md).

History O3 remains **INCOMPLETE**; uncontrolled OS/intra-chain cache makes cold/performance admission **INELIGIBLE**; historical attribution remains open. Large Store/binary payloads are local-only with explicit custody, while traces/receipts/source patches are committed. Earlier receipts, misses and issue comments remain evidence for their own campaigns. This status update supersedes old next-action instructions, not the recorded measurements.

