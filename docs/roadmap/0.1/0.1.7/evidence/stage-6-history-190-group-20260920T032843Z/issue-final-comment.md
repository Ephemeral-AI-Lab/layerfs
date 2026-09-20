## Latest completed optimization: group level 1 retained

The isolated group-level19→1 pair meets the owner's worthwhile-time criterion on both selections. Payload level3,16MiB encode workspace, workers, cache capacities, canonical/physical grammar and integrity checks remain unchanged. All prior PR #194/#195/#196 improvements remain intact. This is one constant change; corrected comments and architecture describe the actual profile.

| Metric | Stride10 baseline | Stride10 candidate | Stride3 baseline | Stride3 candidate |
|---|---:|---:|---:|---:|
| Operation ns | 22,506,420,003 | **20,914,489,418** | 58,410,516,121 | **56,577,520,957** |
| Operation reduction ns | — | **1,591,930,585** | — | **1,832,995,164** |
| Store begin/accept/finish ns | 11,050,102,332 | 9,364,846,704 | 22,624,366,586 | 20,161,248,171 |
| Whole-invocation CPU ns | 32,955,231,000 | 30,396,308,000 | 71,778,254,000 | 69,653,420,000 |
| Apparent Store B | 49,053,696 | 49,324,032 | 61,767,680 | 62,152,704 |
| Allocated Store B | 49,651,712 | 50,249,728 | 62,783,488 | 62,152,704 |
| Verification work ns | 5,617,963,792 | 5,405,493,708 | 18,824,407,917 | 18,904,055,292 |

Observed operation reductions **7.073228815545978% / 3.138125265325286%**, one sample per arm. Save work falls1,685,255,628/2,463,118,415ns; other operation work grows93,325,043/630,123,251ns, exact residual0. Codec-only CPU remains **NOT_MEASURED**. Pooled pack bytes and read time grow with the larger representation. Do not credit the8.900808416s stride10 complete-wall improvement to compression: untimed corpus reads alone changed7.336564411s. No averages or compounded campaign gains.

The owner accepts a small allocation overage for good time reduction. Stride10 matched allocated growth is **598,016B**; both numeric historical targets miss49,344,512B by307,200/905,216B and remain plainly recorded. Apparent growth is **270,336/385,024B**; pack-body growth **262,222/397,717B**. Stride3 allocation decrease is filesystem variation, not a compression-size gain. Both stride3 allocations meet64,024,576B.

All **70roots and complete sorted ObjectId/role/canonical-length inventories match**. Independent review rehashed both measured binaries/four Stores and directly compared inventories; all quick_check resultsok. PhysicalStorebytes intentionally differ. All four separate verifiers pass10/20s targets with1,083/3,377sampledpaths,zero mismatch/missing/unexpected; not exhaustive. Candidate stride3 verification is79,647,375ns slower, still below20s. Save commits change1,470→1,471onstride3 under unchanged thresholds.

**494coretests,117harnesstests,12exampletargets,coreClippy/fmt,122-fileboundaryguard,sixguardselftests and final locked release build PASS.** Inherited unchanged harnessClippy/fmt failures remain unresolved, not rerun or called passes. NoCI/preflight. Measured identities and the separate comment-corrected final build stay distinguished; source executable code matches, full source/binary SHA does not.

Production LOC **85,722 → 85,722 (delta0)**: reference65,417/core20,305 unchanged. Exact first-parent/staged counting includes runtimeSQL and excludes comments/tests/harness/docs/tools. No new cache, telemetry, dependency, level sweep, stride1 or pack-range implementation.

[Report](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/README.md) · [Results](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/results.json) · [Canonical inventory](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/store-comparison.json) · [Independent review](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/REVIEW.md). Publication pending the final commit. **#190 stays open:** all O3 rows INCOMPLETE, uncontrolled cache/performance admission INELIGIBLE, historical v0.1.6 comparison unresolved. All timing remains diagnostic. Cross-task communication stopped after the lock protocol was corrected, as the owner instructed; remaining resource work used shared locks directly.
