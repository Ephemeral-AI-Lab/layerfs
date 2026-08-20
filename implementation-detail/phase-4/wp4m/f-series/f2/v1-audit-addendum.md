# WP4-M F2-v1 audit addendum

- Date: 2026-08-19.
- Version: `wp4m-f2-v1-audit-addendum-20260819-v1`.
- Authority: corrective read-only recomputation from the immutable
  `target/wp4m-f2-construction-proof-k64-20260819-v1/f2.raw.jsonl` only.
- Historical v1 disposition remains **FAIL / REVISE**. This addendum does not
  overwrite, relabel, regenerate, or delete any v1 artifact or row.

## Custody

The checkpoint began clean at commit
`4d20b7c5ca61fb2a5f61a198eac10a11bc631cd8`, tree
`9355b1afc5eb082d7df2c5fbb6a94f40b3bf2e2a`. Before this addendum, the
immutable v1 root contained 171 files and its sorted complete file-hash stream
had SHA-256 `1e232ac6f9aa7185904f7c4c2832a88c0b78699a2a5df11b650f93d490ea6de1`.
The original raw JSONL remains SHA-256
`800aa1e8252fe1a39687713b4caaee85f4c746fc6467d6525bc563c9e932fb3f`.

The v1 root has no retained measured environment record, toolchain record,
release-build output, or focused/full/static test-output custody. Their v1
classification is therefore **Unavailable**. The historical report's stated
commands and aggregate pass counts are not promoted into missing retained
outputs.

## Corrected protected post-COMMIT gates

The frozen gate was not median-only. Every protected metric also required at
least four of five adjacent measured pairs at or below `+5%`. Both original
v1 analyzers omitted this per-pair gate for individual post-COMMIT phases.

| Metric | A median | B median | Arm change | Pair changes, B vs A | Pairs <= +5% | Correct result |
|---|---:|---:|---:|---|---:|---|
| fresh reopen | `1.022917 ms` | `1.013541 ms` | `-0.916594%` | `-4.867002, +10.407895, -5.979664, -4.352877, +9.018463%` | **3/5** | **FAIL** |
| fresh scrub | `265.790542 ms` | `267.581958 ms` | `+0.673995%` | `+1.034859, +1.078335, -0.077593, +0.178211, +0.960783%` | 5/5 | PASS |
| reconstruction | `419.187500 ms` | `421.554666 ms` | `+0.564703%` | `+0.615218, +1.848161, +0.949386, -0.620814, -0.436785%` | 5/5 | PASS |
| ranges | `0.671000 ms` | `0.690125 ms` | `+2.850224%` | `+0.532648, +9.080780, +2.850224, +7.128157, -0.849328%` | **3/5** | **FAIL** |

Pair 2 and pair 5 fail fresh reopen. Pair 2 and pair 4 fail ranges. Favorable
arm medians cannot cure the prospectively required `>=4/5` rule.

The already recorded protected COMMIT failure remains independently terminal:
`135.886208 -> 176.823000 ms`, arm `+30.125789%`, paired median
`+28.184244%`, and `0/5` pairs within the ceiling. Thus the corrected v1
decision is still **FAIL / REVISE**, now with three protected failures:
COMMIT, fresh reopen pair-count, and range pair-count.

## Authority and Q corrections

V1's construction proof is not standalone publication authority. Its
`FullCreateExpectation` is populated by a second full database build and full
verifier outside the measured row and binds the proof to externally supplied
root, transition, and closure values. That oracle is valid as independent
shadow/golden evidence only. V1 therefore does not prove arbitrary full create
without a prepared root/transition/closure oracle.

V1's stated exact `Q_proof=21,952` is an analytical charged-capacity total,
but its implementation drops `_frontier_charge` while builder/proof frontier
owners and finalization children remain live, and its root-finalization scan
allocates an uncharged `Vec<usize>`. Consequently v1's measured
`q_high_water=55,325` and terminal zero do not establish exact live-overlap Q.
They remain useful non-acceptance observations only.

V1 also recomputes `chunk_id(bytes)` inside construction observation
immediately after deriving the same raw ID from the same bytes. That
unaccounted third full-source hash pass is not a separate trust boundary.

## Final corrected v1 statement

V1 causally demonstrates that the candidate can reduce pre-COMMIT SQL queries
`5,373 -> 1`, BLOB reads and object authentications `5,373 -> 0`, and durable
wall `929.420 -> 786.868 ms` with 5/5 wins. It does **not** establish standalone
authority, exact Q, protected COMMIT, fresh-reopen pair-count, or range
pair-count acceptance. Physical causality for the COMMIT movement remains
**Unavailable**. V1 remains immutable historical **FAIL / REVISE** evidence;
F3 remains ineligible.
