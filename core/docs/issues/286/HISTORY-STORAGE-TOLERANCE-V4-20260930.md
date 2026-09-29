# #286 retained-history v4 storage tolerance

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Owner direction on 2026-09-30 accepts a 5–10% deviation from the original
compound C2+C5 allocated-storage targets in exchange for restoring the faster
payload-level-3/group-level-1 Store encoder. This **new v4 selection** fixes
10% as its maximum before collection. Each report also states whether its
exclusive original-owner allocation is within 5%; 5% is informative, not an
alternative gate chosen after seeing a row.

| Case | States | Original strict ceiling | v4 strict ceiling | Complete command | Separate verifier |
| --- | ---: | ---: | ---: | ---: | ---: |
| stride10 | 17 | `<49,344,512 B` | `<54,278,964 B` | ≤60 s | ≤10 s |
| stride3 | 53 | `<64,024,576 B` | `<70,427,034 B` | ≤170 s | ≤20 s |
| stride1 | 157 | `<83,947,520 B` | `<92,342,273 B` | ≤170 s | ≤30 s |

The v4 integer ceiling is `floor(original_ceiling × 110 / 100) + 1`, so its
strict `<` comparison accepts no integer count more than 10% above the
original ceiling. Measurement remains `st_blocks × 512` for distinct,
exclusive original C2 and C5 owners and any separate required persistent
index. Missing counts are INCOMPLETE; shared or reflinked attribution is
INELIGIBLE. All prior v1–v3 cases, limits, receipts and statuses remain
unchanged and are never reclassified.

Only the encoded-byte policy and storage acceptance limit change from the
v3 workload. The same fixed corpus, 17/53/157 selected states, one producer,
public C5 publication and reopen, v2 independent root ledger, v3 applicable
canonical pins, complete state-tree/selected-byte verifier, cache declaration,
cleanup gate and command/verifier deadlines remain. The v4 C5 binding and
incarnation are deliberately the v3 values, so changing the gate does not
silently create a different retained-history workload. The default v4
selection is stride10 then stride3; stride1 remains explicit run-only.
Latency is diagnostic and INELIGIBLE for a cold or comparative speed claim
while source-cache residency is uncontrolled. A single fresh-path sample per
case/source is retained regardless of outcome. This document freezes v4
before any v4 sample; it does not promote an old strict FAIL to a new PASS.
