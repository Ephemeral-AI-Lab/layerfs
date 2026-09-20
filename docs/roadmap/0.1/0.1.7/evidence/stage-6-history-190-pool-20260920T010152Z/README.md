# Pooled physical-group reuse — #190

> Status: Research; informative and not a product contract.

The treatment shares the existing bounded decoded physical-group cache with pooled inode-leaf reads. It removes repeated Zstandard work while retaining the same requests, chain traversals, pooled-value materialization, pack acquisition and authenticated output. These are one-sample matched diagnostics with uncontrolled cache residency; admission remains INELIGIBLE. No historical receipt is replaced or relabelled.

## Matched operation results

| Selection | Baseline ns | Candidate ns | Candidate minus baseline ns | Reduction |
|---|---:|---:|---:|---:|
| history-stride10 | 19,660,740,792 | 16,268,082,127 | -3,392,658,665 | 17.25600628% |
| history-stride3 | 56,736,225,583 | 44,591,815,082 | -12,144,410,501 | 21.40503774% |

Reduction = 100 × (baseline_ns − candidate_ns) / baseline_ns. This pair uses the merged parent-lookup optimization on both sides. Do not multiply these percentages by the older campaign reduction: its baseline, ambient state and instrumentation differ. Historical Commit figures11,370,679,212ns / cited24.815s remain numerically below this candidate and are not matched controls.

## Mechanism and tradeoff

| Work | Stride10 baseline | Stride10 candidate | Stride3 baseline | Stride3 candidate |
|---|---:|---:|---:|---:|
| pooled.leaf_requests | 11,268 | 11,268 | 41,958 | 41,958 |
| pooled.chain_edges | 16,570 | 16,570 | 82,387 | 82,387 |
| pooled.physical_record_calls | 55,676 | 55,676 | 248,690 | 248,690 |
| pooled.physical_group_decodes | 55,676 | 2,723 | 248,690 | 20,959 |
| pooled.physical_group_decoded_bytes | 2,522,862,738 | 123,780,247 | 10,302,425,176 | 877,790,656 |
| pooled.physical_group_cache_hits | 0 | 52,953 | 0 | 227,731 |
| group_decodes | 513 | 1,163 | 3,892 | 5,285 |
| pooled.value_group_decodes | 65,337 | 65,337 | 368,074 | 368,074 |
| pooled.pack_fetches | 79,784 | 79,784 | 427,384 | 427,384 |
| pooled.pack_bytes | 7,378,994,999 | 7,378,994,999 | 23,399,127,004 | 23,399,127,004 |
| read_waves | 66,616 | 66,616 | 111,853 | 111,853 |
| requested_objects | 74,279 | 74,279 | 135,296 | 135,296 |
| returned_canonical_bytes | 148,826,516 | 148,826,516 | 377,985,893 | 377,985,893 |

For every state in this corpus: physical_record_calls = 2 × (leaf_requests + chain_edges) = physical_group_decodes + physical_group_cache_hits. All observed physical groups are compressed; this equality is not asserted for arbitrary raw-group fixtures. The 512KiB cache is shared, not enlarged. Ordinary group decodes increase by650/1,393 through eviction: a measured cost, retained rather than hidden. BLOB bytes are SQLite acquisitions/copies, not disk I/O. Value-group decodes include raw and compressed materialization.

## Exact time partition and remaining attribution

Provider elapsed is nested within the operation. Subtracting it produces a disjoint arithmetic partition; it does not isolate codec CPU.

| Selection/arm | Operation ns | Provider ns | Operation outside provider ns |
|---|---:|---:|---:|
| history-stride10/baseline | 19,660,740,792 | 9,856,060,754 | 9,804,680,038 |
| history-stride10/candidate | 16,268,082,127 | 6,648,561,878 | 9,619,520,249 |
| history-stride3/baseline | 56,736,225,583 | 37,227,856,889 | 19,508,368,694 |
| history-stride3/candidate | 44,591,815,082 | 25,319,737,028 | 19,272,078,054 |

The partition residual is0 by exact subtraction. Internal provider time still combines SQL/BLOB acquisition, value-group reconstruction, physical decompression, framing and authentication; no independent codec CPU/time split is measured. Candidate provider work remains6,648,561,878ns /25,319,737,028ns. Candidate filesystem unnamed residual remains2,387,168,047ns /7,233,197,330ns; these are nested, not additive to provider time. The experiment proves reduced decompression work, not a newly avoided full tree scan or a change in traversal complexity.

## Verification, storage and resources

| Selection/arm | Verify work ns | Target ns | Status | Allocated bytes | Allocated ceiling bytes |
|---|---:|---:|---|---:|---:|
| history-stride10/baseline | 6,185,391,917 | 10,000,000,000 | PASS | 49,192,960 | 49,344,512 |
| history-stride10/candidate | 4,241,717,250 | 10,000,000,000 | PASS | 49,192,960 | 49,344,512 |
| history-stride3/baseline | 20,766,917,666 | 20,000,000,000 | TARGET_MISS | 62,144,512 | 64,024,576 |
| history-stride3/candidate | 14,987,183,125 | 20,000,000,000 | PASS | 61,775,872 | 64,024,576 |

All70 state roots and both paired complete Store files match. All save decision counters match. The sampled verifier compares1,083/3,377 paths per arm with zero mismatches, missing or unexpected entries; this is not exhaustive read-back. All four allocated readings meet their ceiling. Apparent bytes remain49,053,696/61,767,680; the allocation difference at stride3 is not an algorithmic storage saving because file bytes are identical.

| Selection/arm | Complete perf command ns | Invocation CPU user+system ns | Lifetime peak RSS bytes |
|---|---:|---:|---:|
| history-stride10/baseline | 33,741,919,208 | 27,101,144,000 | 257,146,880 |
| history-stride10/candidate | 30,502,824,583 | 23,419,171,000 | 257,245,184 |
| history-stride3/baseline | 74,713,341,541 | 66,315,391,000 | 269,860,864 |
| history-stride3/candidate | 61,657,848,334 | 53,732,312,000 | 262,373,376 |

CPU is invocation CPU, not codec-only or child-only CPU. RSS is process-lifetime peak, not an incremental phase peak. All selected performance commands fit their unchanged120/240s diagnostic ceilings and verification commands fit60s hard limit. These do not waive ordinary admission budgets. Each preflight observed no named competitor and at least70% aggregate CPU idle; desktop activity/cache residency remained uncontrolled.

## Correctness and scope

The existing session cache remains bounded at512KiB decoded bodies, plus its existing next-group transient of at most64KiB, map overhead and bounded record copies. Pooled values retain their previous per-leaf cache and decoded-work behavior. Public writer-facing PoolReader methods remain uncached; raw-group copying is unchanged. Cached reads still validate current pack/header/group framing and lengths, chronology, chain charges, publication ceiling and final canonical identity. Existing immutable-published-group semantics apply: this is not a promise to freshly detect arbitrary external changes to previously cached compressed bodies.

Focused tests cover FULL and real delta chains, repeat-wave reuse, unchanged fresh value decoding, more-than-cache-capacity eviction (18decompressions/2hits), warm-cache damaged record ordinals and lowered visibility, and cold/warm canonical/encoded budget refusals. An initial SQLite fixture conversion compile error was fixed before sampling and retained. A collector dynamic-import error occurred before any resource command. Verification launch deferrals and an empty stale private-checkout lock recovery are retained; no measurement sample was restarted and no live owner was interrupted.

## LOC and remaining gaps

Production LOC:85582→85723 (delta+141), measured with unchanged tools/production_loc.py; reference65417→65417, core20165→20306. Instrumentation adds85; the isolated reuse treatment adds56. Tests, harness, documents and receipts are excluded. Exact first-parent/staged-tree counts are recorded in [COMMIT-LOC.json](COMMIT-LOC.json).

- Historical time tripwire remains open.
- All four history rows retain `g1.o3-pinned-counters: INCOMPLETE`.
- Cache/performance admission remains INELIGIBLE; these are diagnostics only.
- Baseline stride3 verification is TARGET_MISS; candidate meets its target in this run.
- Codec CPU, value-group codec split and SQL/BLOB CPU are NOT MEASURED.
- Stride1, signature reuse, new-row filtering, subtree summaries and pack-write coalescing are NOT_RUN in this experiment.
- Whole-tree qualification and release admission are not claimed.

## Reproduction and custody

See [protocol](PROTOCOL.md), [results](results.json), [baseline identity](baseline-identity.json), [candidate identity](candidate-identity.json), [matched source check](matched-source-check.json), [independent review](review/REVIEW.md), and checks/*.json for exact commands. `python3 analyze.py` rederives results from committed traces/receipts. The harness and dependency maps match; only encoding/pool/read.rs and encoding/delta/read.rs differ between measured arms. Source patches plus companion new-product-file patches reconstruct both dirty source arms from9d82685f3.

Store files and binaries are retained locally, excluded from Git with explicit manifest paths and hashes. Numeric claims reproduce from traces; repeating file-byte custody needs retained local artifacts. Existing receipts remain append-only.


## Validation

- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked`:493 tests PASS, including final warm/cold budget assertions.
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --examples`: all12 example targets compile,0 example tests.
- `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings`: PASS.
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`: PASS. Initial invocation without `--all` failed to find targets; retained as a command failure, not a source-format failure.
- Product boundary guard:122 production Rust/SQL files PASS; its6 self-tests PASS.
- Both locked release builds PASS.

No CI, retired preflight, third-party patch or dependency change.

Harness validation:117 tests PASS. Harness Clippy FAILS with22 diagnostics at inherited statements; harness formatting FAILS across existing files and also flags the new telemetry tuple layout. The measured harness is kept byte-identical to both arm manifests; no formatting rewrite, counter-policy relaxation or rerun has been used to hide these failures. See checks/harness-clippy.log and checks/harness-fmt.log.

Disposition: retain the bounded reuse change based on deterministic work reduction and correctness, while keeping #190 open. Candidate verification meets both targets in this diagnostic; historical timer parity, missing pins and cold-cache qualification remain unresolved. Do not infer a95% operation-time gain from the decompression count reduction. Further optimization requires attribution of the remaining value-group/pack/SQL work.
