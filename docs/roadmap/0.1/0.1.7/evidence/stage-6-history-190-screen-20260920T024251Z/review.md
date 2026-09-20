# Independent review: zero-count investment screen

Status: **reject this optimization direction under the prospectively frozen
two-second stride10 investment threshold**. No candidate implementation or
stride3 run is justified by this result. Retain the previous successful changes.

The reviewer read the source patch, protocol, raw timing and immutable performance
trace, and compared small evidence files with the latest ROI candidate. No source
edits, builds, tests, benchmarks, profiles or large-file hashes were performed by
the reviewer. This document is the only reviewer-owned output.

## Independent arithmetic

Direct traversal of `runs/baseline-history-stride10/raw/timing.json` yields
**17** state children, **273** total timing nodes and **16** `zero_count` spans.
State 1 has no base and correctly has no such span. Exact observations, using the
timing child suffixes as recorded rather than historical corpus ordinals:

| Timing child suffix | `zero_count` ns |
| --- | ---: |
| 2 | 2,638,958 |
| 3 | 7,513,958 |
| 4 | 16,021,208 |
| 5 | 29,845,500 |
| 6 | 35,013,416 |
| 7 | 48,394,792 |
| 8 | 60,673,834 |
| 9 | 73,737,417 |
| 10 | 82,179,667 |
| 11 | 142,333,250 |
| 12 | 138,946,042 |
| 13 | 173,537,333 |
| 14 | 221,693,375 |
| 15 | 210,460,917 |
| 16 | 214,146,250 |
| 17 | 241,941,459 |
| **Sum** | **1,699,077,376** |

The whole-operation sum is **22,615,178,250 ns**. The complete-command receipt
records **42,162,170,750 ns**, within the unchanged 120-second diagnostic limit.
The screen threshold minus the measured entire phase is exactly:

```text
2,000,000,000 − 1,699,077,376 = 300,922,624 ns
```

The temporary patch wraps the unchanged `zero_count_serials(...)` call with the
existing `FilesystemPhases.phase("zero_count", ...)` API. It preserves arguments,
control flow, reducer behavior and subsequent counter update. The measured
interval includes touched-row consolidation, lookup preparation, all existing
and new serial lookups, and zero-count evaluation. Filtering new rows would
remove only a subset of this work.

## Correctness and evidence comparison

Compared with
`stage-6-history-190-roi-20260920T014227Z/runs/candidate-history-stride10`:

- All **17** root identities match.
- All **17** recorded save-counter entries match.
- All **272** per-state provider work-counter entries match after excluding
  provider elapsed time.
- All **272** other per-state filesystem work-counter entries match.
- The performance receipts name the same complete Store SHA-256:
  `ab64dab7aaed512ab93b7ccc46fd14f22e57791f642b39fdf2b94650a41a965f`.
- The reviewer's hash of the small screen identity JSON matches its recorded
  identity-receipt hash.

Store equality is a receipt comparison, not an independent large-file hash.
The separate verifier did not start: its launch was refused by an existing empty
harness `O_EXCL` measurement lock. Status is **NOT_RUN_LOCK_HELD**, not a verifier
failure or PASS. Root stopped resource work rather than spending more on a
direction already rejected by its investment screen. The reviewer did not touch
the lock, retry the verifier or hash a large artifact. The prior campaign is a
correctness/work-count reference, not a matched timing control for this screen.

## Correcting the new-ID assumption

The earlier assertion that **all** newly declared IDs exceed the base tree's
maximum was too broad. In `src/ops/history.rs`, `Chain::serial` returns a known
path's old ID at lines 565–571; removal does not discard that map entry, while
`Change::Added` declares the returned ID new at lines 674–676. A reintroduced
path can therefore be an absent **interior** key and require leaf acquisition.

First-seen paths do receive monotonically increasing IDs. For those above the
base maximum, `filesystem/inode/read.rs:132`–135 stops at a branch root when the
partition position equals the child count. A mixed wave may already require the
root for existing IDs, so removing those new demands may save no page read.
The frequency and cost of reintroduced interior IDs were not measured. Neither
the first-seen-path argument nor native profile counts establish an exact
universal upper bound on the proposed change.

## What the screen establishes—and what it does not

For this recorded invocation, removing the entire measured phase while holding
all other work fixed would remove at most **1,699,077,376 ns**, below the required
**2,000,000,000 ns**. A new-row filter cannot remove all of that direct work.
That is sufficient to reject further investment under the frozen screen rule.

This is a **sample-local direct-cost bound**, not proof that a two-second gain
is impossible on every machine or future workload. Changes to cache competition,
allocation, scheduling or subsequent phases could have indirect effects, and
those effects were not measured. They cannot be invoked as an unmeasured reason
to lower the threshold or fund a larger campaign. The root-minus-provider
interval likewise cannot bound this filter, because it could remove provider
calls. Native stack observations are not converted into nanoseconds.

The receipt declares uncontrolled cache residency, admission ineligible and cold
performance **INELIGIBLE**. O3 history-pin gaps are not repaired by this screen.
No historical v0.1.6 timing claim follows from it.

**Disposition:** preserve the diagnostic patch, binary, identity and receipts;
stop without a new verifier PASS. Root reports the temporary product timer has
been restored and the product diff is empty, leaving final production LOC
unchanged. No filter implementation, matched candidate, broad test sweep, stride3
measurement, lock recovery or second optimization should be started on the
strength of this rejected screen. The **NOT_RUN_LOCK_HELD** verification gap
remains explicit and does not turn this diagnostic into an admission result.

**Subsequent owner ruling — prospective one-second threshold:** The user explicitly states that a one-second optimization is worthwhile. The two-second screen and STOP above remain the historical decision under the earlier threshold; they no longer reject reopening this candidate. Its whole-phase **1,699,077,376 ns** now leaves a possible direct-cost opportunity, but the removable new-row subset and any saved operation time remain **NOT_MEASURED**. The minimal proposal filters `PendingState::New` only within each original bounded wave, preserving touched-row limits, scanned-row accounting, existing-row order and reducer effects; base results must be consumed only for matching `Existing` rows, while actual base-read accounting falls with actual demands. New-identity absence is already checked against the same base by `validate::check_new_identities` before mutation, including absent interior IDs. Matched baseline/candidate instrumentation must retain the same `zero_count` scope; the archived phase-only binary is a control with its original identity, not a new measurement or a substitute for the outstanding verifier. Any implementation still needs unchanged correctness/refusal evidence and measured savings against the newly stated one-second criterion; no gain is asserted by this addendum.

## Reopened candidate: source, identity and comparison review

The frozen candidate keeps the same phase wrapper as the baseline and changes
only the lookup demands within each original `touched.chunks(base_batch.max(1))`
wave. `PendingState::Existing` serials alone enter the demand vector; their base
iterator advances only inside the Existing match arm. New rows retain their
carried counts. The full touched-set quota, scanned-row charge, reducer order,
release logic, root exception, unreachable-parent handling and resource defaults
remain unchanged. Actual `base_records_read` now counts filtered demands. This
is an intentional work-counter reduction, not a relaxed resource quota.

Source review found no blocking issue. The pre-mutation new-identity validation
still authenticates absence in the same base, and tests retain the reused-serial
refusal. The structural baseline/candidate logs report matching roots for both
suffix and interior new IDs, at batch sizes 1 and 32. For the actual default
batch size **32**, provider pages/waves change as follows:

| Fixture | Baseline pages → candidate | Baseline waves → candidate |
| --- | ---: | ---: |
| Suffix IDs | 283 → 279 | 278 → 274 |
| Interior IDs | 556 → 547 | 546 → 540 |

Both fixtures reduce zero-count base-record demands **133 → 4**. The much smaller
actual page/wave reductions are the important qualification; the larger batch-1
effects must not be advertised as default behavior. These are retained focused
test results, not history elapsed-time measurements. The reviewer did not rerun
the tests.

Small-file identity-map comparison confirms both arms name base commit
`ca13fb1709250be64711a9431146e1aceb390955`, record dirty instrumented source,
have identical harness and dependency-lock maps, and differ in product source
only at `core/crates/layerfs-content/src/filesystem/update.rs`. Recorded immutable
executable hashes are:

- Baseline: `5372121b9bd7650896038c32c9f41a3f03d14903d3bbb5ac001f4d2ce3624f74`.
- Candidate: `79b181ee2e9657b8a84069909d8a6f5754b1474ac2e9f3e3f560cc226e00eb33`.

These are identity-file values, not a fresh binary hash. The original phase-only
baseline sample is reused with its original provenance; it is not presented as
a new baseline run. Cache residency remains uncontrolled and admission ineligible.

The pending comparison will separately reconcile operation sum, zero-count
phase sum, provider work and elapsed, storage intervals, complete wall, roots
and Store receipts. The **one-second** owner criterion remains in force. An
overall operation difference of one second is not by itself evidence that the
targeted mechanism saved one second if the zero-count interval and actual
provider reduction point elsewhere. Unmeasured indirect effects cannot be used
to assign unrelated storage or corpus variation to this change. Candidate
performance and fresh verification status are pending at this review stage;
no PASS, FAIL or gain is invented ahead of the retained result.

## Reopened experiment result: below the one-second threshold

This result follows the new owner ruling; it does not rewrite the earlier
two-second screening decision. Independent raw-tree and trace reconstruction
gives:

| Quantity | Baseline ns | Candidate ns | Observed reduction ns |
| --- | ---: | ---: | ---: |
| Sum of 17 operation children | 22,615,178,250 | 22,458,498,667 | **156,679,583** |
| `zero_count` | 1,699,077,376 | 1,645,895,414 | **53,181,962** |
| Operation outside `zero_count` | 20,916,100,874 | 20,812,603,253 | **103,497,621** |
| Whole filesystem interval | 9,536,586,040 | 9,477,586,917 | 58,999,123 |
| Provider interval, nested in filesystem | 8,572,820,583 | 8,522,085,111 | 50,735,472 |
| Store begin + accept loop + finish | 11,120,818,665 | 11,036,071,003 | 84,747,662 |
| Complete performance command | 42,162,170,750 | 41,570,775,750 | 591,395,000 |

The disjoint operation arithmetic closes:
`53,181,962 + 103,497,621 = 156,679,583 ns`. Provider and filesystem rows overlap;
neither is added to the other. The storage reduction is a separate observation,
not a measured consequence of filtering new inode demands. Indirect cache or
scheduling effects remain unmeasured.

The observed operation reduction falls short of the new one-second criterion by
**843,320,417 ns**. The targeted interval changes by only **53,181,962 ns**. This
is a real reduction in requested work, but the retained result does **not**
demonstrate a one-second optimization.

The candidate removes **570** provider waves and **570** requested/returned
objects: waves **66,616 → 66,046**, objects **74,279 → 73,709**. Returned canonical
bytes decrease **148,826,516 → 147,902,276**, a difference of **924,240 bytes**.
Pooled leaf requests, chain edges, physical record calls, physical group
decompression, pooled value materialization, pooled pack fetches/bytes and
ordinary group decompressions are unchanged. Thus the work evidence does not
support a claim that this filter removed expensive pooled reconstruction.

All 17 roots and recorded save counters match. In the expressly reserved quiet
window, the reviewer acquired the two shared flocks and harness lock, directly
hashed both executables and both Stores, and released the locks before notifying
root. The binary hashes match the identity values recorded above. Both actual
Store hashes match both their performance and verification receipts:
`ab64dab7aaed512ab93b7ccc46fd14f22e57791f642b39fdf2b94650a41a965f`.

## Fresh verification and storage misses

The reopened experiment's verifiers subsequently ran successfully. These are
new results following the previously retained `NOT_RUN_LOCK_HELD` event, not a
retroactive relabeling of that event:

| Arm | Internal verification ns | Complete verification command ns | Sampled mismatches |
| --- | ---: | ---: | ---: |
| Baseline | 5,557,719,750 | 5,571,945,292 | 0 |
| Candidate | 5,240,016,750 | 5,284,788,083 | 0 |

Both complete with exit code zero and satisfy the 10-second verification work
target and 60-second hard budget. This is sampled correctness and a verification
time-target pass, **not a blanket PASS**:

| Arm | Performance allocated bytes | Declared ceiling bytes | Excess bytes | Status |
| --- | ---: | ---: | ---: | --- |
| Baseline | 49,369,088 | 49,344,512 | **24,576** | **STORAGE_TARGET_MISS** |
| Candidate | 49,647,616 | 49,344,512 | **303,104** | **STORAGE_TARGET_MISS** |

These exact allocations are present in the immutable performance traces and
must remain reported even though both apparent file sizes are **49,053,696
bytes** and their contents are byte-identical. Allocation variation does not
prove an algorithmic size change or excuse either target miss. Both traces also
retain history O3 **INCOMPLETE**, and both performance receipts declare cache
admission **INELIGIBLE**.

**Final recommendation under the one-second ruling: reject this variant and
stop.** Restore the temporary filter and phase instrumentation while retaining
their diff, binaries, tests and receipts as evidence. Stride3 and the full
workspace test sweep are **NOT_RUN for this rejected candidate**; the reviewer
does not imply otherwise. The successful earlier optimizations remain in place.
This rejection follows a measured **156,679,583 ns** operation difference and
**53,181,962 ns** targeted-phase difference, not dismissal of a worthwhile
one-second gain.

**Final consistency review and conditional storage ruling:** The reviewer read the final `README.md`, `results.json` and `analyze.py` without executing the analyzer or any resource command. They agree with the independently reconstructed operation/phase deltas, unchanged **49,053,696-byte** apparent Stores, the **49,344,512-byte** allocated ceiling, and numerical excesses **24,576 / 303,104 bytes**; full-suite and stride3 work remain explicitly NOT_RUN for this rejected variant. The owner subsequently permits a small allocated-storage miss when accompanied by a worthwhile time reduction. That is a conditional acceptance disposition, not permission to rewrite the measured allocation or its historical `TARGET_MISS` classification, and no new numeric tolerance is invented here. This variant remains rejected because its observed operation reduction is **156,679,583 ns**, with only **53,181,962 ns** in the targeted phase, rather than because of the allocation miss. The report correctly avoids claiming that either single-sample timing difference is a proven causal saving.
