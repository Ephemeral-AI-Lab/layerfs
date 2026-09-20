# Catalogue statement reuse: return on complexity

> Status: Research; informative and not a product contract.

**Disposition: retain one local change and stop this optimization round.** Reuse the existing prepared-statement cache in `sqlite::pool::group_for`. The implementation is one production line smaller, adds no telemetry or data cache, and preserves SQL, parameters, fresh results and error checks. The matched operation samples improve on both selections; stride10 complete-command time regresses and is reported plainly.

## Attribution before implementation

One native `sample` run used the immutable PR #195 executable without rebuilding or adding product timers. Its graph contains **7,307 main-thread stack-residence samples**, of which **1,759** descend from StoreProvider. These are sampled observations, not exact CPU or phase durations.

| Disjoint provider work | Samples |
|---|---:|
| Pack query preparation | 43 |
| Pack query execution/copy/other | 550 |
| Catalogue preparation | 204 |
| Catalogue execution/decode/other | 420 |
| Value decompression | 32 |
| Value authentication | 53 |
| Value decoding/allocation/other | 59 |
| Physical record processing/other | 41 |
| Leaf reconstruction/other | 44 |
| Other provider work | 313 |
| **Total** | **1,759** |

Independent review splits catalogue work into preparation 204, finalize/drop 12, reset 87, execution 312, row decoding 5 and other 4. Preparation plus finalize/drop identifies a **216-sample opportunity**, not an elapsed-time estimate. Reset is still necessary, and finalization is deferred to eviction/connection closure rather than abolished. Value categories exclude nested pack acquisition. The profile selected a target; it does not establish a speedup.

The sampler launched 4,679,541 ns after child launch; its report timestamp is 88 ms after its reported process launch time. Actual first-sample delay is unavailable. Sample interval 5 ms; exact PID attached; child and sampler exited successfully within 120 s. Its 24,639,852,787 ns operation and 42,415,136,209 ns collector wall are a separate profiled diagnostic, excluded from the matched comparison. Profile Store bytes match the normal stride10 Stores. Separate profile verification was NOT_RUN; no profile admission claim is made.

## One treatment and its proof

Only `core/crates/layerfs-storage/src/sqlite/pool.rs::group_for` changes: `Connection::query_row` becomes `prepare_cached` followed by `Statement::query_row`, using exactly the same SQL and binding. The existing connection cache remains at its pinned default of 16 statements. Results, rows and BLOBs are not cached. No pack lookup change, wider pooled-value lifetime, new cache, capacity, policy, codec, worker or format change.

The public rusqlite authorizer callback counts actual SELECT preparation events after fixture setup. Baseline: **five preparations for five helper queries**. Candidate: **one preparation for those five queries**, and still one after a sixth query observes a row inserted after absence. Updated digest, changed ordinal binding, range errors, deletion/missing rows and invalid-ordinal rejection before SQL remain correct. Cache eviction may cause later preparations; this fixture is not a claim of one preparation for an entire history run. Whole-lane preparation counts are NOT_MEASURED.

## Unprofiled matched results

One sample per case/arm; same harness and dependency maps; only the catalogue helper differs. Formula: `100*(baseline_ns-candidate_ns)/baseline_ns`.

| Selection | Baseline operation ns | Candidate operation ns | Candidate minus baseline ns | Reduction |
|---|---:|---:|---:|---:|
| history-stride10 | 23,491,957,417 | 22,180,444,124 | -1,311,513,293 | 5.58281828% |
| history-stride3 | 64,870,176,420 | 59,039,478,665 | -5,830,697,755 | 8.98825636% |

| Selection | Baseline complete command ns | Candidate complete command ns | Delta ns |
|---|---:|---:|---:|
| history-stride10 | 39,267,932,708 | 39,909,975,875 | +642,043,167 |
| history-stride3 | 88,637,897,000 | 82,042,953,250 | -6,594,943,750 |

**Stride10 has no end-to-end win:** outside-operation complete wall grows15,775,975,291→17,729,531,751 ns, outweighing the operation reduction. This residual includes lifecycle/scheduling/reporting and untimed work; it is not all corpus reads. Single samples do not establish repeatability. Do not compound this campaign with the earlier optimization percentages or compare its absolute times as a matched historical v0.1.6 run.

| Selection/arm | Operation ns | Nested provider ns | Operation outside provider ns | Invocation CPU user+system ns | Lifetime peak RSS bytes |
|---|---:|---:|---:|---:|---:|
| history-stride10/baseline | 23,491,957,417 | 9,596,396,756 | 13,895,560,661 | 33,510,728,000 | 239,861,760 |
| history-stride10/candidate | 22,180,444,124 | 8,437,296,469 | 13,743,147,655 | 32,304,434,000 | 251,953,152 |
| history-stride3/baseline | 64,870,176,420 | 36,676,151,222 | 28,194,025,198 | 77,940,045,000 | 249,135,104 |
| history-stride3/candidate | 59,039,478,665 | 31,155,004,681 | 27,884,473,984 | 71,847,405,000 | 250,085,376 |

The provider/outside-provider partition closes with residual 0 by subtraction; provider time is nested, not additive. CPU is whole-invocation CPU, not isolated SQL CPU. RSS is lifetime peak, not an incremental phase peak; candidate RSS is higher by 12,091,392 /950,272 bytes in these samples. No memory saving or exact attribution of RSS to prepared statements is claimed.

## Correctness, verification and limits

All 70 state roots, save decisions and every provider work counter other than elapsed time match. Both paired complete Store files are byte-identical. The optimization reduces SQL preparation, not object demand, pack acquisition, decompression or query result work.

| Selection/arm | Verification work ns | Target ns | Status | Allocated bytes | Ceiling bytes |
|---|---:|---:|---|---:|---:|
| history-stride10/baseline | 6,237,682,875 | 10,000,000,000 | PASS | 49,180,672 | 49,344,512 |
| history-stride10/candidate | 5,489,793,375 | 10,000,000,000 | PASS | 49,180,672 | 49,344,512 |
| history-stride3/baseline | 22,686,258,458 | 20,000,000,000 | TARGET_MISS | 62,402,560 | 64,024,576 |
| history-stride3/candidate | 18,912,983,625 | 20,000,000,000 | PASS | 62,103,552 | 64,024,576 |

Candidate verification meets 10/20 s in this diagnostic; baseline stride3 is TARGET_MISS. All commands meet unchanged 120/240 s diagnostic performance limits and 60 s verification hard limit. Verification compares 1,083/3,377 paths per arm, with zero mismatch/missing/unexpected entries; sampled, not exhaustive. All allocated readings meet their ceilings. Apparent sizes 49,053,696/61,767,680 B and paired bytes are unchanged; allocation variation is not an algorithmic storage gain.

## Non-passing and unmeasured work

- All four normal history rows retain O3 pinned-counter **INCOMPLETE**.
- Cache/performance admission remains **INELIGIBLE**: OS/intra-chain residency is uncontrolled. No cold or release claim.
- Stride10 complete-command wall regresses by 642,043,167 ns.
- Baseline stride3 verification misses 20 s.
- Historical operation-time tripwire remains unmet; current 22.180444124/59.039478665 s are not matched to cited legacy 11.370679212/24.815 s.
- Candidate stride10 launch was deferred for a competing Cargo process; baseline stride3 launch was deferred at 66.7% CPU idle. Neither consumed a sample. Both observations remain.
- Reviewer hash acquisition was initially refused by an empty private measurement lock. Root preserved it after holding both global flocks and confirming no active owner/open descriptor or path alias. Its origin is UNKNOWN; no process was interrupted. The successful retry is reported separately.
- Exact internal SQL/value/pack elapsed and CPU, whole-lane prepare counts, repeatability and stride1 remain NOT_MEASURED/NOT_RUN.
- Pack-query caching, broader data/value caching, summaries, streaming and further optimization are NOT_RUN. Stop here.

## LOC, custody and reproduction

Production LOC: **85,723 → 85,722 (delta−1)**. Reference 65,417 unchanged; replacement core 20,306→20,305. Attribution adds 0 production LOC. Use identical `tools/production_loc.py`, including runtime SQL and excluding tests, legacy inline test branches, harness, docs, tools and generated files. Exact first-parent/staged counts are recorded in [COMMIT-LOC.json](COMMIT-LOC.json).

Base `81f4f1fefcd5f600742fb8b0fe4c0502354db09d`. Baseline reuses the immutable PR #195 binary with its original build provenance; all compilation hashes match the merged base. Candidate is a locked incremental build. [Matched source check](matched-source-check.json), [baseline identity](baseline-identity.json), [candidate identity](candidate-identity.json), [protocol](PROTOCOL.md), [selected treatment](TREATMENT.md), [raw-derived results](results.json), [profile derivation](profile-derived.json), [independent review](review/REVIEW.md).

Run `python3 analyze_profile.py` and `python3 analyze.py` to reproduce the derived arithmetic. Exact build/test/measurement commands and limits are in checks/*.json and runs/*/*-receipt.json. Store/binary artifacts remain immutable local evidence with explicit size/path/hash retention; traces, source patches, receipts and checks are publishable. No new dependencies, third-party modifications, CI or retired preflight.


## Final validation

- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked`: **494 tests PASS**.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --examples`: all 12 example targets compile, zero example tests.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings`: PASS.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`: PASS.
- Product boundary guard: 122 production Rust/SQL files PASS; six self-tests PASS.
- Locked harness tests: **117 PASS**; locked candidate release build PASS. Baseline executable reused with identical compilation input hashes and preserved original provenance.
- Harness Clippy/fmt were **NOT_RUN again**: harness files are byte-identical to PR #195, whose retained checks contain 22 inherited Clippy errors and formatting failures. See [prior Clippy receipt](../stage-6-history-190-pool-20260920T010152Z/checks/harness-clippy.json) and [prior format receipt](../stage-6-history-190-pool-20260920T010152Z/checks/harness-fmt.json). These remain unresolved, not passes.

No further product optimization is proposed by this round. The small change reuses an existing mechanism, reduces measured preparation in the focused fixture, and improves both operation samples with less implementation code. The sampling, timing, memory and admission qualifications above remain part of that recommendation.
