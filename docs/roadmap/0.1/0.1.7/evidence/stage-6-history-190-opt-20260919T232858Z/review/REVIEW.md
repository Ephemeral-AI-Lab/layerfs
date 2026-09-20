# Independent review — #190 parent batching and bounded reuse

> Status: Research. Source, raw-artifact arithmetic and custody review. These are diagnostic invocations with uncontrolled cache residency, not cold performance or release admission evidence.

## Verdict

The final candidate preserves all 17 stride10 and all 53 stride3 canonical roots and produces byte-identical SQLite files relative to the corresponding fresh baseline. The actual provider work reduction reproduces directly from raw counters. The final source uses the existing batch API and a bounded final-window reuse policy; no actionable correctness defect was identified. An earlier version did regress bounded ordering acceptance and was rejected before performance collection. That failure is retained and the final version resolves all 56 tested quota outcomes exactly.

The measured treatment is combined parent acquisition batching, batched omitted-metadata acquisition and final-window base-value reuse. Their individual latency contributions were not independently isolated. The historical v0.1.6 gap is not fully resolved by this campaign: this pair has different historical custody/cache/machine conditions, and substantial filesystem residual work remains unnamed.

## Independently reproduced performance arithmetic

All durations below are raw integer nanoseconds, one invocation per cell.

| Quantity | Stride10 baseline | Stride10 candidate | Stride3 baseline | Stride3 candidate |
|---|---:|---:|---:|---:|
| Sum of state operations | 47,161,768,126 | 27,386,866,665 | 125,277,281,254 | 79,607,320,336 |
| Filesystem interval | 34,686,235,752 | 14,832,606,168 | 99,950,581,082 | 53,702,386,711 |
| Directories | 11,506,307,751 | 1,633,180,084 | 28,913,504,290 | 4,958,796,085 |
| Validation | 3,429,981,622 | 3,505,397,540 | 17,333,541,001 | 17,489,653,664 |
| Inodes | 4,470,610,374 | 4,482,754,167 | 15,008,284,830 | 15,739,979,668 |
| Filesystem residual | 15,278,643,093 | 5,210,486,416 | 38,692,689,426 | 15,511,671,619 |
| State residual | 57,440,697 | 44,055,369 | 196,476,425 | 140,592,079 |
| Provider elapsed, overlaps filesystem | 33,559,231,582 | 13,792,721,269 | 98,406,450,952 | 52,197,491,759 |
| Complete command wall | 66,657,423,500 | 40,871,159,292 | 146,622,418,084 | 99,357,294,167 |

Operation deltas (candidate minus baseline) are **−19,774,901,461 ns** and **−45,669,960,918 ns**. Filesystem deltas are **−19,853,629,584 ns** and **−46,248,194,371 ns**. Named intervals plus explicitly retained residuals equal their parents exactly; reconciliation error is zero. Residual is not zero and must not be assigned to a guessed mechanism.

| Actual provider work | Stride10 baseline | Stride10 candidate | Stride3 baseline | Stride3 candidate |
|---|---:|---:|---:|---:|
| Read waves | 110,715 | 66,616 | 210,380 | 111,853 |
| Requested/returned objects | 117,533 | 74,279 | 230,323 | 135,296 |
| Returned canonical bytes | 286,247,942 | 148,826,516 | 706,999,818 | 377,985,893 |
| Group decodes | 513 | 513 | 3,966 | 3,892 |
| Connection opens | 16 | 16 | 52 | 52 |
| Failed provider waves | 0 | 0 | 0 | 0 |

Object-acquisition reductions are **43,254** and **95,027**; wave reductions are **44,099** and **98,527**. These establish less acquisition work without relying on elapsed-time noise. They are not counts of physical storage reads, bytes transferred from disk, or unique objects. Provider elapsed must not be added to filesystem elapsed.

`rederive.py` reconstructs every per-state named span, residual, root and counter from `timing.json`, `phases-perf.json` and `trace-perf.jsonl`. Its final result is `final-perf-rederived.json`; it checks the retained artifact hashes and closes both nested arithmetic equations.

## Correctness, rejected work and limited reuse

See `CANDIDATE-V1-REJECTION.md` for the eight quota acceptance regressions caused by early `note_value`; the first candidate's ordinary test PASS did not prove equivalent quota acceptance. No performance sample was collected for that rejected product version. Final v2 restores the original reducer event order, retaining the existing contents map and only a final nonempty batch of authenticated base records. Earlier records omitted by the caller are reread in batches. Full-operation record memoization is not claimed.

Independent comparison of baseline and final logs reproduces exact equality in all **56** quota cells, including accepted root IDs and refused error limits/actuals. The five-parent single-leaf structural fixture changes omitted-value object acquisitions **23/17/15 → 22/10/6** for widths 1/3/64; explicit-value demands change **18/12/10 → 18/9/6**. All full-batch omitted/explicit roots are identical. New/unreachable parents, partial batches, metadata precedence, deletion, spill and cleanup tests pass. Additional state-root comparison covers 70 roots (17+53), not 70 independent corpus ordinals.

Final retained checks report **490 core tests PASS**, **117 harness tests PASS**, examples compile/test command PASS (zero executable tests), warning-denying core Clippy PASS, core formatting PASS, product-boundary guard PASS and guard self-tests PASS. Reviewer parsed these logs; reviewer did not rerun builds/tests. Earlier observer missing-docs failure, baseline intended structural failure, initial formatting failure, rejected v1 quota differential, and the old exact-wave assertion failure (107 expected, 104 observed) remain in the checks/evidence. The wave assertion was updated with a direct unaffected-materialization demand check; final tests pass.

Final harness Clippy is **FAIL** (22 diagnostic statements), and harness formatting is **FAIL**. The coordinator's `checks/harness-existing-lints-final.json` records that those linted statements already exist at HEAD; reviewer inspected the final lint/format logs but did not rerun lint. These failures remain open and must not be conflated with the passing core Clippy/fmt or 117 harness tests. No harness source was changed after measurements to silence them.

## Verification and open gates

| Selection/arm | Verification work ns | Target ns | Complete command ns | Result |
|---|---:|---:|---:|---|
| Stride10 baseline | 8,124,985,000 | 10,000,000,000 | 8,144,140,292 | PASS target and 60 s hard command bound |
| Stride10 candidate | 8,000,840,000 | 10,000,000,000 | 8,034,066,167 | PASS target and hard bound |
| Stride3 baseline | 29,241,424,750 | 20,000,000,000 | 29,285,548,167 | TARGET_MISS; hard bound PASS |
| Stride3 candidate | 29,978,286,084 | 20,000,000,000 | 30,001,607,291 | TARGET_MISS; hard bound PASS |

All verification commands exit zero; that does not erase both stride3 target misses. Verification remains the existing sampled contract, not exhaustive whole-file verification. All four raw traces retain `g1.o3-pinned-counters=INCOMPLETE`. All four diagnostics remain `admission_eligible=false`, cache/cold performance `INELIGIBLE`. No stride1 optimization/run or historical v0.1.6 rerun occurred.

The stride10 apparent Store is 49,053,696 B in both arms. Allocated bytes differ despite identical file bytes: baseline 49,688,576 B exceeds the historical 49,344,512 B ceiling, while candidate 49,192,960 B is below it. This does not prove a product storage improvement: byte-identical artifacts show filesystem allocation variation. Stride3 apparent is 61,767,680 B in both arms; allocated baseline/candidate is 62,402,560/62,103,552 B. Preserve the reported gates separately from algorithmic claims.

## Source and artifact custody

Under the same three measurement locks, `check_custody.py` independently hashed both binary copies, every retained performance/verification artifact (using the copied performance trace for its original hash), final source manifests, both Cargo locks and the LOC counter. All hashes match their receipts. The performance Store hash remains unchanged after verification. All measured harness-file hashes match across arms. Only `core/crates/layerfs-content/src/filesystem/update.rs` changes in the product manifest. No third-party or lockfile changes were found in these manifests.

- Baseline executable SHA256: `c5826d20f217a73ed4d814b82c3f8d8cdcaaa7af94ead18c0291918c88d4a855`.
- Candidate executable SHA256: `f3afbe222248aa1040004dd09095b63cd94dc6a8f919f48c18a99f3ce42b916e`.
- Both stride10 Store SHA256: `ab64dab7aaed512ab93b7ccc46fd14f22e57791f642b39fdf2b94650a41a965f`.
- Both stride3 Store SHA256: `cfb74bc9b1db6c9470129613283a8f4afa3b139c2819ae7bd7a4f14e548ecdb5`.

The source manifests are dirty-worktree diagnostic identities based on HEAD `9f35c49ad62956f131dc2676787f99d69659686e`, not clean release seals. Reviewer initially resolved harness-relative manifest names against the repository root; the custody script failed before completing, was corrected, and then passed. No benchmark was rerun. `final-custody-v2.json` is the completed independent receipt.

Independent execution of the existing LOC counter matches the final receipt: **Production LOC: 85,533 → 85,582 (delta +49)**. Reference **65,417 → 65,417 (0)**; core **20,116 → 20,165 (+49)**, entirely content **12,512 → 12,561 (+49)**. Storage and telemetry are unchanged. Counting method is `python3 tools/production_loc.py --json`, stable scope/counter hash, excluding tests/docs/harness/tools/blank/comment lines and legacy inline tests. This is a worktree comparison, not a committed-snapshot claim; no commit was made.

## Disposition

The final bounded batching/window-reuse treatment is supported by canonical identity, exact Store identity, reduced acquisition work, tested quota equivalence and passing final workspace checks. Keep the historical tripwire and unresolved verification/O3/cache qualifications open. Remaining cost includes validation, inode merging, save acceptance and an explicitly unnamed filesystem residual. This evidence does not justify a new subtree format or streaming redesign yet; measure the remaining mechanisms before selecting either.
