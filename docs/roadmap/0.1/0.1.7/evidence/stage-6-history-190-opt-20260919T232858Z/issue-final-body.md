# Retained-history parent lookup optimization and remaining qualification gaps

> **Status: implemented and independently reviewed in the working tree; issue remains open for qualification gaps.** No release/cold-performance admission claim. The owner requested the subagent campaign and updates after each step.

## Delivered steps

- [x] **Step1:** measure internal tree-update phases and actual provider read work on fresh stride10/stride3 baseline runs.
- [x] **Step2:** batch repeated directory-parent lookups with existing `lookup_many` and unchanged resource ceilings.
- [x] **Step3:** reuse the final bounded authenticated parent window during the same operation; batch earlier omitted-metadata rereads. Preserve original reducer insertion order and caller metadata precedence.
- [x] **Step4:** prove canonical equality, work reduction and quota parity; confirm on stride3 with separate sampled read-back.
- [x] **Step5:** consider deeper subtree summaries/save streaming and defer them because their avoidable costs are not yet isolated.

Product change: one file, `core/crates/layerfs-content/src/filesystem/update.rs`.
No format, public API, dependency, codec-default, worker-count or validation relaxation.
At most two bounded record windows coexist; no unbounded all-parent cache.

## Fresh diagnostic results

One sample per arm/case with identical harness and dependency identities, unchanged
input history and declared uncontrolled cache residency. These timings describe
this pair; they are not universal or cold-performance guarantees.

| Metric | Stride10 baseline | Stride10 candidate | Stride3 baseline | Stride3 candidate |
|---|---:|---:|---:|---:|
| Operation ns |47,161,768,126|27,386,866,665|125,277,281,254|79,607,320,336|
| Candidate minus baseline ns |—|−19,774,901,461|—|−45,669,960,918|
| Filesystem interval ns |34,686,235,752|14,832,606,168|99,950,581,082|53,702,386,711|
| Provider read waves |110,715|66,616|210,380|111,853|
| Requested objects |117,533|74,279|230,323|135,296|
| Returned canonical bytes |286,247,942|148,826,516|706,999,818|377,985,893|
| Complete command ns |66,657,423,500|40,871,159,292|146,622,418,084|99,357,294,167|

Observed operation reductions:41.92994081173605% /36.45510220277214%, calculated as
`100*(baseline_ns-candidate_ns)/baseline_ns`. The product manifests differ only at
`filesystem/update.rs`.

**All17+53 state roots match. Both final Store files are byte-for-byte identical to
their respective baselines.** All retained save decisions/transaction counts and
non-provider filesystem work counters match. This supports the combined parent
batching/window-reuse work reduction, rather than savings from skipped canonical
or validation work. Provider time is nested, not added again to operation time.

The candidate retains explicit filesystem residual5,210,486,416 /15,511,671,619ns.
Its mechanism is not guessed to be a full scan. These new results do not overwrite
or causally reallocate the older32.566-second receipt.

## Correctness, rejected attempt and tests

The first implementation moved reducer value insertion earlier and made eight
previously supported quota cases fail. It was **rejected before performance**.
The final implementation preserves insertion order and matches all56 baseline
quota outcomes exactly, including success roots and refusal type/limit/actual.
Those eight supported cases are now explicit regression assertions.

The focused shared-leaf example reduces omitted-metadata object acquisitions
23/17/15→22/10/6 for batch1/3/64. Reuse equality is asserted when all parents fit
in the retained window; smaller windows deliberately allow batched rereads.
The old materialization total107waves/303objects becomes104/300 by removing one
second three-page parent lookup; branch grouping, inode-engine work and golden
root assertions remain fixed.

Final validation:

- **490 core tests PASS**, examples compile/test, core Clippyalltargets-Dwarnings,
  core fmt, product boundary guard and its self-tests PASS.
- **117 harness tests PASS**, locked release builds PASS.
- **Harness Clippy still FAILS** at22 statements already present in HEAD; harness
  formatting also FAILS. These are retained failures, distinct from core checks.
- Independent reviewer re-derived all timings/counters, checked70roots, Store
  bytes, source/binary/lock/raw custody, quota parity and production LOC.

## Production LOC

| Scope | Before | After | Signed delta |
|---|---:|---:|---:|
| Legacy reference |65,417|65,417|0|
| Replacement core |20,116|20,165|+49|
| **Combined** |**85,533**|**85,582**|**+49**|

Method: same `tools/production_loc.py --json` production-only scanner before/after;
includes runtimeSQL, excludes tests/docs/harness/tools and inline legacy tests.
Changed product file405→454 counted lines. No commit/push; this is a worktree
comparison, with exact source manifests and independent recount retained.

## Remaining acceptance gaps — why the issue stays open

1. **Historical tripwire remains open.** The candidate17-state operation is
   27,386,866,665ns, above cited legacy Commit11,370,679,212ns. Historical/current
   timer surfaces, source custody and cache conditions differ; the new pair is
   not a matched v0.1.6 comparison. Both versions already acquired siblings before
   subtree pruning; old directory work also lived inside Content, not solely Namespace.
2. **Stride3 verification TARGET_MISS:** baseline29,241,424,750ns and
   candidate29,978,286,084ns against20,000,000,000ns. Stride10 work8,124,985,000 /
   8,000,840,000ns meets10s. Every verifier fits60s hard wall. Samples compare
   1,083/3,377 paths per arm with zero mismatch/missing/unexpected entries;
   this is not exhaustive read-back.
3. **O3 pinned counters remain INCOMPLETE** for all four history performance rows.
4. **Cache/performance admission remains INELIGIBLE.** Same declared uncontrolled
   intra-chain/OS cache, one construction worker, no priming/purge. Prospective
   diagnostic120/240s execution ceilings do not waive ordinary gates. Preflight
   deferrals were retained without starting or discarding a performance sample.
5. **Baseline stride10 allocation FAIL:**49,688,576B exceeds49,344,512B.
   Candidate49,192,960B passes. Stride3 baseline/candidate62,402,560/62,103,552B
   pass64,024,576B. Apparent sizes49,053,696/61,767,680B and complete file contents
   are unchanged, so allocated differences are not claimed as algorithmic savings.

Deeper changes remain deferred. Candidate inode spans include lazy reference work;
no isolated evidence justifies changing subtree metadata/trust contracts. Save
accept includes real encoding/index/transaction work, while clone handoff is only
49,171,738/87,294,823ns. No extra workers, increased buffers or shifted timers.
No stride1 tuning or217-row qualification claim.

## Step-by-step issue record

- [Execution kickoff](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190#issuecomment-5746130570)
- [Step1 measurements](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190#issuecomment-5746168205)
- [Rejected first implementation](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190#issuecomment-5746201767)
- [Steps2–3 revised implementation and LOC](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190#issuecomment-5746222069)
- [Step4 correctness and stride3 confirmation](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190#issuecomment-5746297931)
- [Step5 disposition](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190#issuecomment-5746304024)

## Local evidence and published references

Current files are not yet committed/pushed; the actionable results are reproduced
above rather than linked to unavailable GitHub paths.

- Plan: `docs/roadmap/0.1/0.1.7/retained-history-optimization-plan.md`.
- Final report/raw/CSV/checks/independent review:
  `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-opt-20260919T232858Z/`.
- Archival investigation: `.../stage-6-history-190-20260919T225614Z/`.
- Ledger: `docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md`, L34–L35.
- Source HEAD9f35c49ad62956f131dc2676787f99d69659686e plus declared worktree changes.
- Baseline binaryc5826d20f217a73ed4d814b82c3f8d8cdcaaa7af94ead18c0291918c88d4a855;
  candidatef3afbe222248aa1040004dd09095b63cd94dc6a8f919f48c18a99f3ce42b916e.
- Corpus manifest03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271,
  tipb0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed.

Published context: [lane specification](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/retained-history-storage.md),
[original re-check](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.7/evidence/stage-6-recheck-20260920T000000Z/README.md),
and [legacy report](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md).
