# #190 group compression level 1: live time/space tradeoff

> Status: Research; diagnostic evidence, not release admission.

**Disposition: retain GROUP_LEVEL 1.** One constant changes group compression effort; payload level 3, 16 MiB encode workspace, window/integrity limits, workers, cache capacities and delta-selection policy remain fixed. The owner accepts a one-second improvement and small allocation overages for good time reduction. Both observed operation pairs improve by more than one second. Single samples do not establish repeatability or universal speedup.

The prior parent-lookup, pooled physical-group reuse and catalogue statement-cache improvements remain intact. No pack-range read change, new cache, level sweep, stride1 run, dependency or format migration is included.

## Matched results

All durations are integer nanoseconds. Positive reduction means baseline minus candidate; allocated/apparent growth means candidate minus baseline. Store save intervals are disjoint begin + accept + finish. Provider time is nested within filesystem time and is not added again.

| Selection | Baseline operation | Candidate operation | Reduction | Save reduction | CPU reduction |
|---|---:|---:|---:|---:|---:|
| history-stride10 | 22,506,420,003 | 20,914,489,418 | 1,591,930,585 | 1,685,255,628 | 2,558,923,000 |
| history-stride3 | 58,410,516,121 | 56,577,520,957 | 1,832,995,164 | 2,463,118,415 | 2,124,834,000 |

CPU is whole-invocation user + system, **not codec-only CPU**. Exclusive codec CPU remains NOT_MEASURED. The single treatment and reduced save interval support the group-compression explanation; outside-save variation is reported separately rather than attributed to it.

| Selection | Baseline minus candidate save | Baseline minus candidate outside save | Operation reduction |
|---|---:|---:|---:|
| history-stride10 | 1,685,255,628 | -93,325,043 | 1,591,930,585 |
| history-stride3 | 2,463,118,415 | -630,123,251 | 1,832,995,164 |

Both partitions close exactly, residual 0. Other operation work becomes slower in both samples. More encoded pack bytes increase read work; this is part of the tradeoff, not a claim that every phase improves.

| Selection | Complete command baseline / candidate ns | Apparent baseline / candidate B | Allocated baseline / candidate B | Numeric allocation target B |
|---|---:|---:|---:|---:|
| history-stride10 | 41,663,305,750 / 32,762,497,334 | 49,053,696 / 49,324,032 | 49,651,712 / 50,249,728 | 49,344,512 |
| history-stride3 | 82,295,809,041 / 79,910,471,208 | 61,767,680 / 62,152,704 | 62,783,488 / 62,152,704 | 64,024,576 |

Stride10 allocated readings both miss the historical target: baseline by 307,200 B, candidate by 905,216 B; matched growth is 598,016 B. Record these numeric misses alongside the owner's conditional acceptance; do not relabel raw receipts. Apparent growth is 270,336 / 385,024 B. Stride3 allocated size falls despite greater apparent size; allocation variation is **not an algorithmic storage improvement**. Both stride3 readings meet their numeric target.

Stride10 complete wall improves by 8,900,808,416 ns, but untimed corpus-read work accounts for 7,336,564,411 ns of that difference. Do not credit the whole wall gain to compression. Stride3 wall improves by 2,385,337,833 ns; its corpus-read difference is 578,822,457 ns. [Raw-derived results](results.json) include every state and named phase; original timing trees and traces remain under runs/.

| Selection | Lifetime peak RSS baseline / candidate B | Largest state heap increment baseline / candidate B |
|---|---:|---:|
| history-stride10 | 252,936,192 / 255,803,392 | 75,508,821 / 75,599,425 |
| history-stride3 | 254,623,744 / 254,066,688 | 75,975,324 / 76,104,240 |

Lifetime RSS is not phase-incremental memory. No memory-saving claim is made.

## Work and semantic results

All 70 selected state roots match. All admitted object IDs, roles and canonical lengths match between each pair; the post-verification read-only inventory proves this without requiring physical Store equality. Physical bytes are expected to change with compression level. Both Stores per case pass SQLite quick_check.

| Selection | Objects | Canonical bytes | Pack bodies baseline / candidate B |
|---|---:|---:|---:|
| history-stride10 | 52,032 | 380,921,328 | 45,035,732 / 45,297,954 |
| history-stride3 | 72,560 | 589,916,570 | 56,192,535 / 56,590,252 |

Delta-selection and logical provider-work counters match. Pooled pack bytes rise 7,378,994,999 → 7,985,771,449 on stride10 and 23,399,127,004 → 25,105,269,671 on stride3. These are bytes acquired into application buffers, not physical disk traffic. Provider elapsed increases 52,050,079 / 499,991,335 ns. Save commits remain 1,149 on stride10 and change 1,470 → 1,471 on stride3 under the unchanged byte-charge thresholds. Existing physical-group/value decode counts are unchanged.

## Separate verification

| Selection | Baseline verification ns | Candidate verification ns | Target ns | Status baseline / candidate |
|---|---:|---:|---:|---|
| history-stride10 | 5,617,963,792 | 5,405,493,708 | 10,000,000,000 | PASS / PASS |
| history-stride3 | 18,824,407,917 | 18,904,055,292 | 20,000,000,000 | PASS / PASS |

Each arm reads back 1,083 / 3,377 sampled paths across every selected state, with zero mismatch/missing/unexpected. Coverage is sampled, not exhaustive. All verification commands fit the unchanged 60-second hard budget. Use exact archived performance identities; final source comment corrections are not retroactively inserted into those identities.

## Validation, protocol and limitations

Both 50-test focused codec/storage selections pass. Full candidate core tests, examples, warning-denying Clippy, formatting, product-boundary guard/self-tests and locked harness tests are recorded individually under checks/. [Correctness review](CORRECTNESS.md) names existing test coverage and its limits; it does not claim payload hostile-frame tests directly exercise every group-frame branch. No added test-only product instrumentation.

Harness source is byte-identical to the retained prior campaign: its inherited Clippy/format failures remain unresolved and are not rerun or called passes. The locked harness builds retain the existing unused_mut warning. No CI or retired preflight.

One performance sample per case/arm, baseline10/candidate10/baseline3/candidate3; no repeats or best-of. Existing corpus, fresh growing Stores, one worker, all eight behavioral switches unset, detailed phases enabled. Complete-command diagnostic ceilings 120/240 seconds and verification hard ceiling 60 seconds are unchanged. These rows do not qualify under ordinary 15/25-second admission limits. Cache state is OS/intra-chain uncontrolled: **INELIGIBLE** for cold/performance admission. Every O3 pinned-counter row remains **INCOMPLETE**. The historical v0.1.6 time comparison remains unmatched and unresolved; no release claim.

Build/test prelaunch first deferred on a held global flock. Empty private-marker refusals/recoveries are retained. Subsequent coordination identified the #192 helper's append+flock use of private O_EXCL markers; that helper was corrected to use only both global flocks. Earlier UNKNOWN labels remain historical. No live run was interrupted. Other task resource windows were explicitly serialized; separate worktrees/targets/containers are not host CPU/disk isolation.

## Identities, reproduction and LOC

Base605f6efc6a095a1b6335cbc5549dc0fda78ed9ab. Baseline binary82fb535ddc74eb84aebfaac98d90058600331eb52ca8a8feffc5341b222a9d11; candidate13bcfe86598d3c13f902d6ab7d0a15d24f9ed26bb2c67dadbfe1a34ea6108a2d. Harness/dependency maps match; only codec.rs differs. Candidate measured patch is exactly GROUP_LEVEL19→1. Dirty-source diagnostic identities and patches are explicit, not clean-seal admission claims.

Final source also corrects stale comments describing payload9/group19 and updates architecture. [Comment-only diff](final-comment-only.patch) and [executable-equivalence proof](final-executable-equivalence.json) preserve the distinction between measured source and comment-corrected final source. Final build identity is separate; do not claim unchanged binary/full-source SHA without checking it.

Corpus manifest03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271, tipb0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed. Exact commands, limits, environments, preflight observations and artifact hashes live in each receipt. collect.py baseline|candidate history-stride10|history-stride3 [--mode verify] constructs the declared command and refuses existing outputs. A new experiment needs fresh output paths and identities. analyze.py writes a fresh named JSON; compare_stores.py is post-verification read-only. Do not overwrite retained evidence.

Large binaries and raw SQLite Stores remain local with explicit custody manifest; small receipts, timing trees, traces, source patches and reviews are committed. Production LOC **85,722 → 85,722 (delta 0)**: reference65,417 and core20,305 unchanged. One value changes; comment line deletions are not production LOC reductions. Exact first-parent/staged counts accompany publication.

## Final validation results

- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked`: **PASS** (494 tests).
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --examples`: **PASS**.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings`: **PASS**.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`: **PASS**.
- `python3 core/tools/check_product_boundary.py`: **PASS**.
- `python3 -m unittest discover -s core/tools -p test_*.py`: **PASS**.
- `cargo +1.85.1 test --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked`: **PASS** (117 tests).
- `cargo +1.85.1 build --release --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked`: **PASS**.

Core examples: 12 targets compile, zero example tests. Boundary guard: 122 production files and six self-tests pass. Final comment-corrected build SHA256 `7dff19d828afe760a98ca614a9a99a6630bc94533347d78a467835ba7568a736` differs from the measured candidate, as explicitly retained in final-identity.json. It is not a new performance sample.
