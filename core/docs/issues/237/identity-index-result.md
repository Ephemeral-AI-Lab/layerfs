# #237 same-Save identity index: one-pair result

**Verdict: rejected under the preregistered physical-space gate.** The
candidate removed 5,972,410 measured linear ID inspections, but added two
packs and 520,192 apparent Store bytes. A 18.8-ms raw public-time reduction
does not qualify: control telemetry was incomplete and source metadata
residency remains unmeasured. The candidate product commit `a074d3be8` stays
in the isolated `codex/issue237-integrated-hot-profile` branch and must not
be merged into the root research branch. The [preregistration](identity-index-experiment.md)
was committed before either arm.

Both public calls used the same H3 driver/harness seal
`6d9a3e2eb2a1eea1f3f0df15949d6f3bab103657a824b0a04d34482eff8`,
fixed stack/scope identity, independent source byte copies, fresh Stores,
4,096-byte SQLite pages, the 128-KiB cutoff and four Init constructors. The
control product seal was
`f679b91c522a6091c8333c9986f8c6151d239c70591382173912ea9cec1b39d5`;
the candidate's instrumented product seal was
`69b47b8d08276c5fe5aa311ee74d30397d13e86e31a1f88f8527106206b92f2e`.
The temporary [control probe](evidence/identity-index/control/instrumentation.diff.gz)
and [instrumented candidate](evidence/identity-index/candidate/combined-instrumented.diff.gz)
are archived; the [final uninstrumented product diff](evidence/identity-index/candidate/final-product.diff.gz)
is separate. The measurement probes left source dirty by design.

| One sample per arm | Control | Candidate | Candidate minus control |
| --- | ---: | ---: | ---: |
| Public operation | 1,349,030,916 ns | 1,330,246,417 ns | −18,784,499 ns (−1.392%) |
| Throughput on 300,000,000 B | 222.382 MB/s | 225.522 MB/s | +3.140 MB/s, raw only |
| Complete performance command | 2,285,163,458 ns | 2,321,948,041 ns | +36,784,583 ns |
| File Save inserted / reused | 24,364 / 198 | 24,364 / 198 | 0 / 0 |
| File Save COMMITs / object INSERT statements | 81 / 1,401 | 80 / 1,408 | −1 / +7 |
| File Save packs created / appends | 1,259 / 1,072 | 1,261 / 1,098 | +2 / +26 |
| Pending lookup calls / hits | 24,377 / 13 | 24,379 / 15 | +2 / +2 |
| Pending **linear IDs inspected** | 2,173,717 | **0** | −2,173,717 |
| Current-wave sealed lookup calls / hits | 24,364 / 0 | 24,364 / 0 | 0 / 0 |
| Sealed **linear IDs inspected** | 3,798,693 | **0** | −3,798,693 |
| Service caller sampled maximum RSS | 60,096,512 B | 59,785,216 B | −311,296 B (sampled) |

Candidate hash lookups are represented by the same lookup-call/hit counters;
zero linear inspections does not mean a hash lookup is free. The slight changes
in hit count, COMMIT count and placement follow different producer arrival/order
paths; this pair does not isolate their causes. The control row is `INCOMPLETE`
because Service telemetry ingestion lost one event. Its public result and raw
[service stderr](evidence/identity-index/control/service.stderr) contain the
completed root and probe counters, and its performance command passed, so the
failure is retained rather than rerun. Candidate telemetry and cleanup passed.
Both public receipts have `verification.status=SKIPPED` and
`admission_eligible=false`. Both H3 preflight and immediate recheck found
**0/27,503 source payload pages resident** across 10,000 files / 300 MB.
Metadata residency was unmeasured; no fully cold speedup is claimed. The
[control](evidence/identity-index/control/receipt.json) and
[candidate](evidence/identity-index/candidate/receipt.json) receipts retain
CPU, sampled RSS, operation spans, identity and failure status.

| Closed Store physical result | Control | Candidate | Difference |
| --- | ---: | ---: | ---: |
| SQLite page size | 4,096 B | 4,096 B | 0 |
| Apparent bytes | 334,176,256 | **334,696,448** | **+520,192 B** |
| Allocated bytes reported by `st_blocks` | 335,609,856 | 335,609,856 | 0 |
| Pack count | 1,264 | **1,266** | **+2** |
| Pack BLOB capacity | 331,350,016 B | **331,874,304 B** | **+524,288 B** |
| Declared pack used | 305,978,075 B | 305,986,030 B | +7,955 B |
| Pack spare | 25,371,941 B | **25,888,274 B** | **+516,333 B** |
| Object rows | 24,683 | 24,683 | 0 |

Both closed Stores have identical ordered object-ID SHA-256
`4a9f14a45482c2ae3962f633b3655125278dc816ee9f499803302d76aa241789`
and public filesystem root
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`.
The separate one-shot [control](evidence/identity-index/control/readback/readback.json)
and [candidate](evidence/identity-index/candidate/readback/readback.json)
reopened readbacks both passed: 10,101 paths, 10,000 files, 101 directories
and all 300,000,000 bytes checked against kind, mode, mtime and SHA-256. Their
2.217281/2.214379-s walls are outside the performance calls. The verifier
used each arm's archived release binary and a fresh nonzero read-only History
capability; it did no pagination and did not claim to replay the public run's
ephemeral cursor. The [one-shot helper](evidence/identity-index/readback_once.py)
and raw stdout/stderr are retained.

The candidate's physical-space increase triggers the preregistered rejection,
regardless of raw operation time. Scheduling-dependent pack layout is a
possible explanation, not an established cause. No second arm was sampled and
no threshold was changed. Even the raw candidate rate is far below 518.8 MB/s.

Final uninstrumented source checks passed: `python3
core/tools/check_product_boundary.py` (261 production files), all six
`core/tools` unit tests, `cargo +1.85.1 fmt --manifest-path core/Cargo.toml
--all --check`, `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked
--workspace --release`, and `cargo +1.85.1 clippy --manifest-path
core/Cargo.toml --locked --workspace --all-targets --release -- -D warnings`.
Focused candidate tests also passed before its sample: `cas_reuse` 13,
`c2_bulk_admission` 4, `delta_payload` 15, `pack_locator` 10,
`persistence_failure` 8 and `visibility` 9. They cover duplicate/collision,
same-Save pending/queued reads, dependency selection, terminal failure and
publication. The final isolated product commit changed production LOC
**117,109 → 117,131 (+22)**; reference stayed 65,417 and Core moved 51,692
→ 51,714. This source differs from the timed candidate by removal of temporary
counters/logging and has no performance sample of its own.
