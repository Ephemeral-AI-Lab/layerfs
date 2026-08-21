# Phase-4 count-change product policy after CP-0009

- Date: 2026-08-21
- Starting committed HEAD: `febc20f046bba84ccdce1256363d77799eabf2db`
- Product profile: K64/F64 + DIR256K, profile `b0ebb845...f4ba1`
- Scope: policy decision only; no simulator, mapping implementation, build, test,
  benchmark, profiler, SQLite run, or filesystem experiment

## Executive decision

**Derived(contract precedence + measured gate):** The actual current Phase-4
contract does not require near-constant count-changing latency beyond the
current at-most-50-ms same-open publication policy through 500 MiB. It
explicitly accepts suffix-linear `O(Z)`, worst-case `Theta(N)`, count-changing
work. CP-0008 measures 500-MiB `+1` publication at 27.140916 ms early and
15.102042 ms middle, so both arms remain inside that policy.

**Derived(scope separation):** H09 does not advance. A prolly mapping could
address fixed-ordinal suffix rewrite only. It cannot be credited as a
full-create optimization and cannot remove the separately required
first-after-reopen authority scrub.

## Evidence rules and authority

This report uses only the required labels:

- `Observed(source/evidence)` for a recorded contract statement, counter, or
  measured row;
- `Derived(equation)` for arithmetic over displayed operands;
- `Hypothesis(test needed)` for a prospective claim requiring a future test;
- `Unavailable(reason/source)` where the required observation does not exist.

**Observed(repository custody):** The assigned worktree was on
`codex/empty-worktree` at the required committed HEAD. The accepted dirty
CP-0007/8/9 and research package was read in place and not modified. No active
Phase-4 H05 or other benchmark/performance process was visible during the
research or immediately before this report was written.

**Observed([algorithm specification](../../../../implementation-detail/phase-4/algorithm/spec.md),
section 1):** The active
[fixed-radix fast-lane amendment](../../../../implementation-detail/phase-4/wp4m/fixed-radix-fast-lane-amendment.md)
has first precedence for the count-changing-edit policy. The amendment calls
fixed ordinal grouping an explicit product tradeoff, accepts suffix-linear
insertion/deletion, and states that a future history-independent/prolly mapping
requires a separate canonical-format specification but is not required by the
fast lane.

**Observed([algorithm specification](../../../../implementation-detail/phase-4/algorithm/spec.md),
sections 11.7 and 19):** Count-changing middle edits are `O(Z)`, worst-case
`Theta(N)`, and the required algorithmic behavior is bounded resident memory
with the accepted suffix-linear policy. The specification forbids claiming a
better complexity class than the promoted format supplies.

**Observed([tests and benchmarks](../../../../implementation-detail/phase-4/algorithm/tests-and-benchmarks.md),
sections 1 and 7.5):** The same amendment controls count-changing policy. The
forced-`+1` ratio is mandatory diagnostic evidence, but it is not a rejection
gate under the prospective contract.

**Observed([logical persistence mapping](../../../../implementation-detail/phase-4/mapping/logical-persistence.md),
sections 12.5 and 12.7):** K64/F64 count-changing edits are not path-local; a
receipt does not change that fact. Fixed radix makes no total-file-independent
latency claim for them, and the 100-GiB rows are analytical work equations, not
wall-time projections.

**Observed([CP-0007 report](../../../../implementation-detail/phase-4/test-checkpoint-report/cp-0007-dirty-88ffb0bd6a30-count-change-proof.md)
and [post-promotion contract](../../../../implementation-detail/phase-4/wp4p/post-promotion-count-change-proof.md)):
The required retain gate was at most 50 ms per affected `+1` median; at most
25 ms was a strong result and at most 15 ms a stretch result. Existing product
authority was recorded as accepting honest suffix-linear cost and not requiring
scale-independent 8–10-ms insertions at multi-GiB/100-GiB scale.

**Observed([CP-0008 report](../../../../implementation-detail/phase-4/test-checkpoint-report/cp-0008-dirty-4f1c97f81f7c-count-change-scale.md)
and [independent analysis](../../../../implementation-detail/phase-4/test-checkpoint-report/cp-0008-dirty-4f1c97f81f7c-count-change-scale-analysis.json)):
The exact current-profile 1/10/100/500-MiB campaign passed, retained K64/F64
under the current policy, and directly classified the operation as
`O(suffix)`, worst-case `Theta(N)`.

**Observed([CP-0009 report](../../../../implementation-detail/phase-4/test-checkpoint-report/cp-0009-dirty-b073a7e04c7a-current-product-baseline.md)
and [current baseline contract](../../../../implementation-detail/phase-4/baseline/current-baseline-v1.md)):
CP-0009 is the current product-workflow baseline; CP-0008 remains the
count-changing scale authority. Its handoff says to consider prolly mapping
only if the product requires near-constant latency beyond CP-0008.

**Observed([current implementation index](../../../../implementation-detail/README.md),
[Phase-4 status](../../../../implementation-detail/phase-4/README.md), and
[current research decision map](../../decision-map.md)):
The current policy is summarized as at most 50 ms through 500 MiB. The
8–10-ms scale-independent SLA is consistently conditional, and the current
research map routes the product-requirement decision before any prolly
simulator or code. The pre-CP-0009 `DELAY_WP4_P_FOR_V2` conclusion is therefore
not current authority and was not used.

**Unavailable(explicit stricter product SLA / repository requirement search):**
No current product requirement, workload, baseline, algorithm contract, or
handoff defines a hard near-constant same-open count-changing SLA at 500 MiB,
multi-GiB, or 100 GiB. Repository-wide searches for count-changing,
near-constant, scale-independent, 8–10 ms, 25/15/10-ms gates, 500 MiB, and
multi-GiB found only:

1. the current at-most-50-ms required gate;
2. 25-ms strong and 15-ms stretch result labels at 100 MiB;
3. hypothetical/conditional 8–10-ms language; and
4. research routing that keeps H09 deferred until such a requirement exists.

## Recomputed CP-0008 same-open curves

The values below were recomputed directly from the eight arms in the CP-0008
independent-analysis JSON. `Mapping wall` is the recorded mapping/proof-fold
component median. `Total publication` is the recorded row-level publication
median and is not reconstructed by summing component medians; medians of
components are not generally additive.

**Observed(CP-0008 independent analysis):**

| Size | Operation | Suffix references | Mapping bytes | Mapping wall (ms) | COMMIT (ms) | Total publication (ms) |
|---:|---|---:|---:|---:|---:|---:|
| 1 MiB | `+1` early | 53 | 4,073 | 0.386375 | 0.532833 | 0.957833 |
| 1 MiB | `+1` middle | 27 | 4,073 | 0.392958 | 0.727750 | 1.080959 |
| 10 MiB | `+1` early | 531 | 37,121 | 0.746250 | 0.871542 | 1.738709 |
| 10 MiB | `+1` middle | 266 | 19,601 | 0.632833 | 0.733459 | 1.393625 |
| 100 MiB | `+1` early | 5,284 | 365,495 | 3.437042 | 3.948541 | 7.403083 |
| 100 MiB | `+1` middle | 2,642 | 185,915 | 2.051333 | 3.646292 | 5.715209 |
| 500 MiB | `+1` early | 26,533 | 1,833,348 | 15.158666 | 11.930625 | 27.140916 |
| 500 MiB | `+1` middle | 13,267 | 918,921 | 7.590083 | 7.335875 | 15.102042 |

For every metric `X`, each adjacent curve ratio is recomputed as
`X(larger size) / X(smaller size)`.

**Derived(adjacent-ratio equation):**

| Operation | Size interval | Suffix refs | Mapping bytes | Mapping wall | COMMIT | Total publication |
|---|---:|---:|---:|---:|---:|---:|
| `+1` early | 1→10 MiB | 10.018868x | 9.113921x | 1.931414x | 1.635676x | 1.815253x |
| `+1` early | 10→100 MiB | 9.951036x | 9.846044x | 4.605751x | 4.530523x | 4.257804x |
| `+1` early | 100→500 MiB | 5.021385x | 5.016069x | 4.410381x | 3.021527x | 3.666164x |
| `+1` middle | 1→10 MiB | 9.851852x | 4.812423x | 1.610434x | 1.007845x | 1.289249x |
| `+1` middle | 10→100 MiB | 9.932331x | 9.484975x | 3.241508x | 4.971364x | 4.100966x |
| `+1` middle | 100→500 MiB | 5.021575x | 4.942694x | 3.700074x | 2.011873x | 2.642430x |

**Derived(100→500 work scaling):** File size grows 5.000x. Early/middle
suffix references grow 5.021385x/5.021575x and mapping bytes grow
5.016069x/4.942694x. Those direct-work curves track size and disprove a
scale-independent interpretation. Mapping wall and total wall grow less than
the exact work over this interval because fixed and COMMIT costs remain
material; that does not change the algorithmic classification.

**Observed(CP-0008 independent analysis):** Proof consumption remains at most
0.051625 ms across these arms and authenticates zero objects and zero canonical
payload bytes. The remaining scale is mapping construction plus publication,
not a hidden complete pre-COMMIT closure replay.

## First edit after reopen is a separate authority curve

The current same-open publication policy begins only after required authority
has been established. It must not be presented as first-use-after-reopen
latency.

**Observed(CP-0008 independent analysis):**

| Size | Operation | Authority only (ms) | First-after-reopen total (ms) |
|---:|---|---:|---:|
| 1 MiB | `+1` early | 2.368458 | 3.330791 |
| 1 MiB | `+1` middle | 2.364625 | 3.437667 |
| 10 MiB | `+1` early | 23.445709 | 25.184418 |
| 10 MiB | `+1` middle | 23.062917 | 24.515250 |
| 100 MiB | `+1` early | 240.164125 | 248.664584 |
| 100 MiB | `+1` middle | 240.710750 | 247.129458 |
| 500 MiB | `+1` early | 1,235.301209 | 1,262.771917 |
| 500 MiB | `+1` middle | 1,213.207500 | 1,228.564417 |

**Derived(authority ratio):** From 100 to 500 MiB, authority-only wall grows
`1,235.301209 / 240.164125 = 5.143571x` early and
`1,213.207500 / 240.710750 = 5.040105x` middle. This is a separate
complete authenticated-closure authority cost.

**Derived(H09 scope):** Replacing ordinal mapping boundaries may localize the
mapping mutation after authority exists. It does not establish trustworthy
cross-reopen authority and therefore cannot remove the table above. The
[hypothesis ledger](../../foundations/hypothesis-ledger.md) correctly keeps
cross-reopen authority as separate H16 work.

## Full create is a separate product boundary

**Observed(CP-0009 current baseline):** The current 100-MiB durable full-create
median is 640.109209 ms, decomposed into 504.215417 ms source/CDC/CAS/mapping
and proof fold, 0.038542 ms proof consumption, and 135.855250 ms COMMIT. The
current full-create target is 500.000 ms / 200 MiB/s.

**Derived(full-create gap):** `640.109209 - 500.000000 = 140.109209 ms` remains
to reach the full-create target. That gap is not evidence for H09: the accepted
100-MiB full create produces 105,291,554 new canonical bytes but only 365,262
mapping bytes.

**Observed([mapping and delta research](../../core/cow/mapping-and-deltas.md)):
The current mapping encode/proof lane is too small to be the primary full-create
lever, and a prolly tree adds boundary work during full construction.

**Unavailable(H09 full-create speedup / no candidate evidence):** No
deterministic H09 simulator, implementation, or adjacent CP-0009 A/B exists.
Therefore H09 has no admissible full-create speed claim. It also has no
admissible reopen-authority speed claim.

## Policy and H09 disposition

**Derived(current contract + CP-0008):** Retain this exact policy:

```text
boundary:       same-open durable count-changing publication after authority
profile:        K64/F64 + DIR256K
admitted proof: +1 early and +1 middle at 1/10/100/500 MiB
latency gate:   each affected median <= 50 ms through 500 MiB
complexity:     O(suffix references), worst-case Theta(N)
claim limit:    no near-constant, multi-GiB, or 100-GiB latency claim
```

The policy does not cover first-after-reopen authority and does not weaken its
full-closure authentication requirement. It also does not alter the separate
100-MiB full-create 500-ms/200-MiB/s target.

**Unavailable(multi-GiB/100-GiB count-change wall / no accepted campaign):**
Only analytical suffix-reference and mapping-byte equations exist beyond the
500-MiB measured curve. No latency extrapolation is admissible.

**Observed(required H09 reopen signal / product authority):** Reopen H09 only
when an approved current product requirement or workload artifact defines a
hard, same-open, count-changing durable-publication SLA that is independent of
file size or tighter than the retained policy at a required large-file size,
and current K64/F64 evidence breaches it. For example, an approved at-most-10-ms
SLA at 500 MiB or larger would qualify immediately because CP-0008 already
observes 27.140916/15.102042 ms at 500 MiB. For a different required size or
boundary, collect that exact current-profile row before advancing.

**Hypothesis(test needed):** Only after that reopen signal should a
deterministic, hard-bounded, history-independent prolly-topology simulator be
specified and screened. Until then, building even the simulator would answer a
non-contractual question and would not authorize a format change.

Artifact SHA-256 is reported by the completion handoff after the final bytes
are frozen; embedding a file's own digest in those bytes would change it.

KEEP_CURRENT_COUNT_CHANGE_POLICY
