# #237: 8 MiB wave/transaction capacity experiment

> **Status:** Research; preregistered before either timed public sample. This
> page keeps failed and incomplete attempts. It is not a product contract.

## Hypothesis and exact change

The integrated C1 + C2 source at `df9dbc94ba63f4f3af22551a9f051b0ca6313c49`
has a 512-object pending wave capped below 4 MiB of canonical bytes. Every
wave holds one SQLite write transaction and acknowledges it at the end. The
prior integrated 10k pair reported **1.252324750 s** for its candidate public
Init but did not expose its exact commit count; the earlier D11 file Save
reported 79 commits on another source identity. Reducing transaction frequency
may reduce wall time, but larger waves retain more canonical bytes and hold the
Store writer longer. Neither effect is assumed.

Run one control at the current 4 MiB−1 limit. The only product change in the
candidate is `TRANSACTION_CANONICAL_BYTES_LIMIT = 8 * 1024 * 1024 - 1`, which
also moves `WAVE_CANONICAL_BYTES_LIMIT` through its existing equality. Keep
`BATCH_OBJECT_LIMIT = 512`, the 8,191 transaction-row declaration, pack and
group limits, page size, codec, four construction workers, bounded channel,
request deadline, SQL schema, and verification policy unchanged. The larger
declared wave is a memory and multi-writer latency policy change; any gain
must be weighed against those costs. A single canonical object larger than the
wave bound already takes the product's explicit singleton path in both arms.
An identical temporary count-only `SaveOutcome` stderr report at the file
Save's finish is present in both arms, included in both product seals, and
removed after the pair. It records commits, packs created, pack appends,
object-row INSERT statements, inserted objects, and exact reuse occurrences.
It is not a treatment. Commit count includes the final publication
transaction, so it must not be called the number of waves. These counters
describe this integrated source; they do not establish a matched gain against
the earlier D11 source.
The [exact diagnostic diff](evidence/wave8/count-diagnostic.diff.gz) has
uncompressed SHA-256
`7c74a899134dd2b5209027002c62699ee6ae10e10633eeb4c71c0464c82efd90`.

The two arms use the same `namespace-10000` seed-1 sealed fixture: 10,000
files, 100 data directories, and **300,000,000 total logical bytes**, including
the 100 MB anchor. Run the same research `cold_diagnostic.py` with
`--fixed-operation-identity`, fresh output and fresh SQLite Store for each arm.
The source workspace is reused only for setup. Before **each** public timer,
rehash and invalidate source payload pages, then immediately recheck every
source file without faulting data; require 0 resident pages of 27,503 at both
checks and record the launch gap. The fixed stack and scope must produce the
same canonical root and object IDs. Do not repeat an unchanged arm for a
better time or cleaner telemetry. Record all NOT_RUN, INELIGIBLE, INCOMPLETE,
failed build and failed-verifier evidence.

Commands, in this order after coordinating host timing:

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py --case namespace-10000 --fixed-operation-identity --out benchmark-results/fs-bench-pro/issue237-wave8-control
# Apply and commit the one-constant candidate and its direct boundary check.
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py --case namespace-10000 --fixed-operation-identity --out benchmark-results/fs-bench-pro/issue237-wave8-candidate
```

This is a perf-only pair: full verification is SKIPPED in the measured runs
and contributes no wall time. Both official rows remain diagnostics because
the harness does not independently certify directory/inode metadata cache.
If the candidate appears useful, run separate full reopened readback at its
frozen source identity, retaining the 5 s verifier watchdog; do not count it
in throughput. The Store must retain 4,096-byte SQLite pages. Capture public
caller and complete-command wall, `history.import_files` span, process CPU and
sampled RSS with their sampling limits, object and pack counts, file/allocated
bytes, pack used/capacity/spare bytes, and source cache sidecars. Obtain actual
Save commit and wave counts through the existing `SaveOutcome` or an identical
bounded count diagnostic for both arms; do not infer them from file size or
historical rows. The 512-object cap may prevent halving the wave count.

The candidate is useful only if it returns the same root and complete object
set, independently reopens and reads all 10,101 paths and 300 MB correctly,
retains bounded memory and a 4 KiB database page, and shows a material caller
improvement against its matched control without a material Store-space
regression. A slower arm, failed test, unqualified cache or absent count is
reported as such. The separate #229 sparse-history compactness lane is not
proven by this dense Init pair.

## Attempts and outcomes

One control and one candidate were run, with no retry. The
[evidence manifest](evidence/wave8/manifest.json) pins source, harness,
compilation, binary and fixture identities; the [control](evidence/wave8/control/receipt.json)
and [candidate](evidence/wave8/candidate/receipt.json) receipts retain the
original status and separate cold sidecars. Exact output Stores remain in the
isolated worktree's private `benchmark-results/fs-bench-pro/issue237-wave8-*`
directories, outside this documentation tree. Their closed-Store geometry is
retained [here](evidence/wave8/control-geometry.json) and
[here](evidence/wave8/candidate-geometry.json). The identical temporary
`SaveOutcome` diagnostic diff was archived before the samples and restored
afterward. The worktree's own Cargo target was seeded by an untimed APFS
copy-on-write copy of the root worktree's target, then both source identities
were built in the private target. Exact release builds passed in **7.272 s**
and **3.204 s**, respectively; the timed commands reused only their
identity-matched immutable binaries.

| Measure | 4 MiB−1 control | 8 MiB−1 candidate | Difference |
| --- | ---: | ---: | ---: |
| Public Init, one sample | 1.281196166 s | 1.234139583 s | −47.056583 ms (−3.673%) |
| 300 MB decimal rate | 234.156 MB/s | 243.084 MB/s | +8.928 MB/s, exploratory only |
| Complete performance command | 2.215663750 s | 2.169729083 s | −45.934667 ms |
| Service file child | 1.089661917 s | 1.068486792 s | −21.175125 ms |
| File Save commits, including final publication | 80 | 55 | −25 |
| File Save pack creates / appends | 1,256 / 1,170 | 1,259 / 1,102 | +3 / −68 |
| File Save object-row INSERT statements | 1,464 | 1,435 | −29 |
| File Save inserted / reused objects | 24,364 / 198 | 24,364 / 198 | unchanged |
| Service sampled maximum RSS | 59,834,368 B | 71,761,920 B | **+11,927,552 B (+19.93%)** |
| Store apparent / allocated bytes | 333,393,920 / 335,609,856 | 334,176,256 / 335,609,856 | **+782,336 / 0 B** |
| Pack capacity / declared used | 330,563,584 / 305,965,480 B | 331,350,016 / 305,977,929 B | **+786,432 / +12,449 B** |
| SQLite page size | 4,096 B | 4,096 B | unchanged |
| Receipt / verifier | INCOMPLETE / SKIPPED | DIAGNOSTIC / SKIPPED | no admission |

Both calls returned root
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`.
All **24,683** persisted object IDs matched as an ordered digest
`4a9f14a45482c2ae3962f633b3655125278dc816ee9f499803302d76aa241789`.
Each independent source preflight and immediate nonfaulting recheck reported
**0 resident payload pages of 27,503** for all 10,000 files / 300 MB;
recheck-to-timer gaps were **6.243 ms** and **1.124 ms**. Each arm used a fresh
4-KiB-page Store. The control daemon dropped one telemetry event, so its row
is `INCOMPLETE` even though the public root, count diagnostic and cleanup
were retained. The candidate telemetry and cleanup passed, but its verifier
was deliberately `SKIPPED` for fast iteration.

**Interpretation:** widening the byte cap removed 25 file-Save COMMITs and
29 object-row INSERT statements in this exact pair. The 512-object cap and
publication work remained. `SaveOutcome.commits` includes the final
publication transaction and is **not** an observed wave count; no separate
wave counter was collected. The public-call difference is one raw observation,
not a stable estimate or a proven causal gain. Candidate setup reused the
same prepared source path after the control. Although source *payload* pages
were absent before both timers, directory and inode metadata residency was
neither measured nor evicted. Warm metadata from the earlier run could credit
the candidate. The official runner accordingly labels both rows
`source-cache-uncontrolled-v1` and `admission_eligible=false`; under the
no-warm-cache rule, **neither row qualifies as a cold performance PASS**.

The candidate increased the declared pending canonical-byte allowance by
4 MiB, sampled Service RSS by nearly 12 MB, Store apparent bytes by 0.78 MB,
and pack capacity by three 256-KiB packs. Allocated Store blocks happened to
be equal; this does not erase the larger SQLite file and pack reservation.
Sampled RSS is a process observation with `boundary_covered=false`, not a
phase peak or a hard memory proof. Combined Service+daemon CPU was essentially
unchanged (1.867172 s control, 1.867594 s candidate). A full reopened
10,101-path / 300-MB readback and the #229 sparse-history space lane were not
run at this candidate identity. Because this small, cache-unqualified raw wall
improvement came with a large RSS increase and larger Store, the 8-MiB cap is
**kept only as an isolated research prototype**, not adopted as a product
optimization. It still misses the historical 518.8 MB/s time by about
**0.656 s**. A future treatment would require a new prospective pair with a
complete equal cache contract and separate readback/space proof; these
attempts will not be resampled.

The candidate's changed boundary passed the focused locked
`layerfs-storage` `storage_limits` (5 tests) and `persistence_failure`
(8 tests) suites. After restoring the temporary count hook, Core formatting,
the product-boundary scan (261 source files), and all six boundary-tool
self-tests passed. Full Core workspace tests, warning-denying Clippy, full
readback and sparse-history checks were not run for this rejected prototype;
the earlier integrated root workspace checks cover a different source
identity and are not claimed here.
