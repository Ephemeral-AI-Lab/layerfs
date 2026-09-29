# #286 round 20260930-history-stride3-v2-r009

> **Status:** Dated **FAIL** for the original immutable canonical O3 pins. Storage and the complete semantic/custody checks pass their distinct gates; family 2 remains incomplete.

This is the **first** stride3 v2 candidate invocation. It used committed source [`2a2453535`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/2a2453535293e7911545a6a970baf30f9317ed52), the unchanged product/compilation seal of passing stride10 r008, locked release binary SHA256 `b02afa16a07804a78f2f4d40142e69ba1c0a8d2fcc8836de1f5c882cf0889fcf`, all53 original selected states and the separately committed SHA-sealed independent v2 root vector. No arm was repeated or dropped.

| Gate | Observation / frozen bound | Result |
| --- | --- | --- |
| Complete driver | **77,535,511,208 ns /170,000,000,000 ns** | PASS absolute budget; numeric time INELIGIBLE (source cache uncontrolled) |
| Separate verifier | **16,559,332,000 ns /20,000,000,000 ns** | PASS wall budget |
| Complete corpus tree/kind/size and selected content | **306,861/306,861** listed path-states, **26,052** selected public content-digest states /127,050,040 logical bytes | native O4 PASS |
| Independent roots and C5 custody | **53/53** independent roots match; public C5 reopens all53 Layers/52 Commits; expected rows/stages/cleanup | PASS |
| Actual exclusive C2+C5 allocation | **60,477,440 B /<64,024,576 B**, margin3,547,136 B | PASS, native and Python |
| Original canonical O3 | **589,480,854 B /73,447 objects** vs **589,423,458 B /73,476 objects** | **FAIL**: +57,396 B /−29 objects |
| Overall | Correctness FAIL, storage PASS, cleanup PASS | **FAIL** |

C2/C5 allocated **60,325,888/151,552 B**, apparent total **59,580,416 B**. C2 has962 packs/5,861 groups, 53,393,778 B pack blob (51,531,586 B bodies, 1,862,192 B framing, zero unused capacity); C5 has53 Branches/Layers,52 Commits and zero live stages. Integrity, schema and sidecar checks pass. The native and Python O3 gates both fail on the unchanged historical pins. Those pins were established by the v0.1.6 candidate/control with exactly the same original fixed corpus and 306,861 path-states; this new representation's matching independent old-Core v2 reference also observes589,480,854/73,447, so changing only the current candidate codec cannot close the discrepancy. The old reference is not a reason to rewrite the historical gate. The v1 reference's larger +493,112/−916 mismatch was reduced by the corrected directory operation, but the residual +57,396/−29 needs a source-backed explanation and correction or an explicit non-admission ruling.

[Unchanged original receipt, exact SHA inventory, native trace/phases, at-run schema/pack allocation and verifier output](20260930-history-stride3-v2-r009/evidence-index.json) are retained; the large original C2 Store remains at its indexed local path. The clean stride10 r008 PASS is not pooled with this failed tier. Explicit stride1 is NOT_RUN here; Init and families3–7 are not run while the history canonical gate fails. PR #285 stays draft/unmerged; no release admission or issue closure.

This report-only commit has exact production LOC reference65,417→65,417 (+0), Core70,045→70,045 (+0), combined135,462→135,462 (+0), measured on first-parent/final staged/committed snapshots with `tools/production_loc.py --json --root <snapshot>` (counter SHA256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`).
