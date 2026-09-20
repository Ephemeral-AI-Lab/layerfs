# Legacy comparison review scope

> Status: Research; informative and not a product contract.

User requested another subagent round reading v0.1.6 `crates/` to identify remaining gaps against replacement `core/`. This round reviews source and existing evidence only: no product changes, benchmark runs, builds, workload reads or new performance claims. Preserve all changes in PRs #194/#195/#196 and the rejected #197 experiment. One-second stride10 gains remain worthwhile. Small allocated-storage misses are conditionally acceptable when accompanied by good time reduction; report both measurements.

Current source pin: `4391f66d8f39fc4d179caf557176021fc6f2c700`. Timed historical compiled commit `ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb`; applicable product/harness reference `7fab1027a0061e8b932345d4fcd6ac22a089b155`, as established by retained source/hash comparison. Do not assume a release tag is the measured source.

Three independent document owners: TREE.md (namespace/tree/validation), STORAGE.md (save/read/codec/index), BOUNDARIES.md (timer/workload equivalence and identity). Coordinator owns SYNTHESIS.md and issue/ledger updates. Agents may write only their assigned documents. Each candidate needs source references on both sides, preserved semantics/bounds, counter-evidence, a smallest decisive experiment, and a measured containing interval or an explicit NOT_MEASURED. No guessed seconds or summed overlapping timers. At most two candidates per squad; synthesize at most one recommended next experiment. A bounded review may conclude no credible next candidate.

The latest normal baseline contains only the temporary same-operation zero-count timer on top of retained product code. Existing measurements are historical evidence, not new samples. The source review cannot demonstrate that any proposed change saves a second. Only a later declared, locked, single-sample matched diagnostic could test that; do not launch one in this review.
