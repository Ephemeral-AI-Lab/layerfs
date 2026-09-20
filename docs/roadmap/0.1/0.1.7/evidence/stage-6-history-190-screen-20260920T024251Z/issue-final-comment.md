## Latest follow-up: one-second rule applied; candidate rejected

The owner explicitly clarified: **“1 second is good optimization, do not dismiss it.”** A one-second saving is worthwhile. The earlier 2-second screening rule is superseded prospectively; its original receipt/rejection remains preserved.

All successful changes in **PR #194, #195 and #196 remain intact**. Three subagents researched, implemented/tested, and independently reviewed a narrow candidate: omit redundant base-inode requests for New rows inside the existing zero-count batches. Same batch bounds, mandatory validation, canonical outputs and cache policy.

| New stride10 diagnostic | Phase-only baseline | Filter candidate | Observed reduction |
|---|---:|---:|---:|
| Operation | 22.615178250 s | 22.458498667 s | **0.156679583 s** |
| Target zero-count phase | 1.699077376 s | 1.645895414 s | **0.053181962 s** |
| Provider read waves | 66,616 | 66,046 | 570 |
| Complete command | 42.162170750 s | 41.570775750 s | 0.591395000 s |

The operation delta is 53,181,962 ns inside zero-count plus 103,497,621 ns outside it, residual 0. The outside change is unassigned variation, not credited to the filter. Pooled decompressions, value decoding and pack reads are unchanged. Normal-batch structural tests likewise show only 4 / 9 fewer provider pages despite logical demands falling 133 → 4. **This candidate did not demonstrate the requested 1-second reduction**, so it was archived and reverted. No stride3/full-suite continuation for the rejected variant; no broader cache, summary or streaming change.

Both separate read-back checks pass: 5.557719750 / 5.240016750 s against 10 s, 1,083 sampled paths per arm, zero mismatch/missing/unexpected. All 17 roots/save decisions and full Store bytes match; independent review rehashed both binaries and Stores. **Both allocated-storage targets miss:** 49,369,088 / 49,647,616 B vs 49,344,512 B (over 24,576 / 303,104 B). Same apparent size 49,053,696 B and identical file bytes do not erase these misses. O3 remains **INCOMPLETE**, cache admission **INELIGIBLE**. One busy preflight and empty-lock refusals consumed no sample; all retained. No repeated performance sample or release claim.

Final product **85,722 → 85,722 (delta 0)**; reference 65,417 and core 20,305 unchanged. Experimental +7 LOC (+2 span/+5 filter) reverted with its active test; patches/test snapshots retained. Earlier source improvements remain unchanged.

[Report](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-screen-20260920T024251Z/README.md) · [Raw-derived results](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-screen-20260920T024251Z/results.json) · [Independent review](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-screen-20260920T024251Z/review.md) · [Owner revision](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-screen-20260920T024251Z/ONE-SECOND-REVISION.md). Evidence links become available when this documentation commit lands. Earlier matched results below remain historical checkpoints, not a baseline for this new pair. #190 stays open for historical timing and qualification gaps; further **one-second** candidates remain worthwhile if grounded in measured work.
