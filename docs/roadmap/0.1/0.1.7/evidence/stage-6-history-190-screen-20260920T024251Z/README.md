# #190 one-second candidate experiment

> Status: Research; diagnostic evidence, not release admission. Base `ca13fb1709250be64711a9431146e1aceb390955`. The owner explicitly ruled that **one second is a worthwhile optimization**. This report applies that rule.

**Retain PRs #194, #195 and #196 unchanged; reject this additional filter.** The candidate skips base-inode requests for `PendingState::New` while retaining every original batch boundary, prior authenticated absence validation, result order and touched-serial limits. It needs no cache, policy or format change. Its observed stride10 operation reduction is 156,679,583 ns, below the 1,000,000,000 ns bar by 843,320,417 ns. The targeted phase changes by only 53,181,962 ns. Single samples do not prove a causal timing saving of either size.

## Prospective rule and evidence sequence

The initial [protocol](PROTOCOL.md) used the owner's earlier multi-second instruction, interpreted as a 2-second screen. Whole `zero_count` cost was measured once as 1,699,077,376 ns; [screen-result.json](screen-result.json) and the original rejection stay historical. The later [owner revision](ONE-SECOND-REVISION.md) explicitly supersedes that investment bar with one second. The same archived phase-only baseline is reused as evidence, never rerun or relabelled as a new sample. One candidate sample follows. No best-of, averaging, smoothing or discarded performance sample.

Three subagents handled source feasibility/implementation, external structural tests and independent review. Their [source disposition](candidate-source-disposition.md), [structural disposition](structural-disposition.md) and [independent review](review.md) are retained separately. The coordinator uses raw-derived [results.json](results.json); [states.csv](states.csv) records every selected state's operation, target phase, filesystem interval and unnamed state-body residual.

## Matched diagnostic measurements

All times below are integer nanoseconds; positive reduction means baseline minus candidate. Provider time and zero-count time are nested in filesystem time and must not be added to it.

| Quantity | Baseline | Candidate | Reduction |
|---|---:|---:|---:|
| Operation | 22,615,178,250 | 22,458,498,667 | 156,679,583 |
| Whole command | 42,162,170,750 | 41,570,775,750 | 591,395,000 |
| Invocation CPU user + system | 33,241,999,000 | 32,827,434,000 | 414,565,000 |
| Verification work | 5,557,719,750 | 5,240,016,750 | 317,703,000 |
| Lifetime peak RSS B | 249,348,096 | 252,985,344 | -3,637,248 |
| Largest state heap increment B | 75,509,110 | 75,508,823 | 287 |
| Store allocated B | 49,369,088 | 49,647,616 | -278,528 |
| Store apparent B | 49,053,696 | 49,053,696 | 0 |
| Filesystem | 9,536,586,040 | 9,477,586,917 | 58,999,123 |
| Zero-count phase | 1,699,077,376 | 1,645,895,414 | 53,181,962 |

The operation delta closes exactly: **156,679,583 = 53,181,962 zero-count + 103,497,621 outside zero-count**, residual 0. The latter is unassigned observed variation, not credit for this filter. Store begin/accept/finish changes by 84,747,662 ns within that outside interval. Source does not change those operations. Exact codec/SQL CPU and repeatability remain NOT_MEASURED.

The full operation partition also closes arithmetically, with the still-unnamed state-body remainder stated rather than attributed:

| Disjoint operation component | Baseline ns | Candidate ns |
|---|---:|---:|
| `content` | 1,618,536,750 | 1,612,277,332 |
| `harness.predecessors` | 78,870,708 | 78,703,500 |
| `store.create` | 3,815,666 | 3,465,209 |
| `harness.input` | 95,162,458 | 96,098,500 |
| `filesystem` | 9,536,586,040 | 9,477,586,917 |
| `storage.begin` | 11,910,707 | 11,775,625 |
| `storage.accept_loop` | 9,897,005,208 | 9,820,437,293 |
| `harness.index` | 110,545,665 | 110,475,456 |
| `storage.finish` | 1,211,902,750 | 1,203,858,085 |
| `state.uninstrumented` | 50,842,298 | 43,820,750 |

## Actual work and why the gain is small

```
Baseline, each original batch       Candidate, same original batch
all touched IDs                    Existing IDs only
        |                                  |
   lookup_many                         lookup_many
        |                                  |
base result for New discarded       New uses its carried count
        |                                  |
same resulting zero-count IDs ---> same roots / same saved Store
```

Provider read waves fall **66,616 → 66,046**, requested/returned objects **74,279 → 73,709**, canonical bytes **148,826,516 → 147,902,276**. Failed waves remain 0. Nested provider elapsed changes **8,572,820,583 → 8,522,085,111 ns**, an observed 50,735,472 ns reduction.

The expensive pooled work is unchanged: 11,268 leaf requests, 16,570 edges, 55,676 physical-record calls, 2,723 physical-group decompressions / 123,780,247 decoded bytes, 52,953 physical-group cache hits, 65,337 value-group decodes, 79,784 pack fetches / 7,378,994,999 pack bytes. Ordinary group decodes remain 1,163; connection opens remain 16. New high IDs already fail the base-tree maximum bound near the root; mixed batches still require pages for existing IDs. Logical demands therefore exaggerate removable physical work. Source bounds and references are documented in the squad reports.

The public-provider test runs suffix-new and interior-hole cases with batch sizes 1 and 32, preserving roots, metadata, reference counts, removal and false-New rejection. At normal batch 32, logical zero-count demands fall 133 → 4, but pages only 283 → 279 (suffix) and 556 → 547 (interior). Both focused baseline and candidate tests pass. Exact test versions/commands/logs are archived. This rejects the claim that **this filter has demonstrated a one-second gain**; it does not prove all further optimization impossible.

## Correctness, limits and all non-passing results

All 17 roots and save decisions match. Both Stores have SHA256 `ab64dab7aaed512ab93b7ccc46fd14f22e57791f642b39fdf2b94650a41a965f`; actual Store and binary hashes independently rechecked. Separate read-back uses each arm's exact archived executable and saved Store: 1,083 sampled paths, 893 file reads / 5,619,947 logical bytes, zero mismatches/missing/unexpected entries per arm. This is sampled, not exhaustive.

- Verification work: **5,557,719,750 / 5,240,016,750 ns**, both PASS against 10,000,000,000 ns; complete verification commands 5,571,945,292 / 5,284,788,083 ns, within the 60-second hard budget.
- **Both allocated-storage targets miss:** 49,369,088 / 49,647,616 B against 49,344,512 B, exceeding by **24,576 / 303,104 B**. Byte-identical contents and unchanged apparent size do not erase these allocation misses. No algorithmic storage gain is claimed. Earlier successful receipts retain their original results.
- Both `g1.o3-pinned-counters` rows remain **INCOMPLETE**; no history pins exist.
- Cache/performance admission **INELIGIBLE**: fresh growing Store, existing immutable corpus; OS and intra-chain residency uncontrolled. No cold, release or admission claim.
- Complete performance commands meet the frozen **120-second diagnostic** ceiling; they do not qualify under ordinary 15/25-second admission ceilings. No ceiling was raised for this experiment.
- Candidate preflight deferred once at 67.72% idle, with no named competitor and no child launch. Actual arm preflights were 83.66% / 82.61% idle; no named resource competitors. Desktop variation remains. Measurement locks serialize builds/tests/performance/verification/hashing.
- Empty private-lock refusals prevented initial baseline-verifier and candidate launches; neither consumed a sample. Recovery preserved the empty files and metadata, held both global flocks and checked no owner/open descriptor/resource process. Origin UNKNOWN. Every deferral and recovery remains under `checks/` and `runs/`.
- **Stride3 NOT_RUN** because stride10 failed the revised one-second screen; stride1, wider cache, subtree summaries and streaming NOT_RUN. No full core/harness suite for the rejected candidate. The focused tests and both locked release builds passed; the final product is restored byte-for-byte to `ca13fb170`. Prior PR #196 checks are historical evidence, not newly rerun checks; inherited harness Clippy/format failures remain unresolved.

## Reproduction and custody

Binary SHA256: baseline `5372121b9bd7650896038c32c9f41a3f03d14903d3bbb5ac001f4d2ce3624f74`, candidate `79b181ee2e9657b8a84069909d8a6f5754b1474ac2e9f3e3f560cc226e00eb33`. Both use the identical phase wrapper; only `filesystem/update.rs` differs. Exact source patches, production LOC, compilation/product/harness/dependency maps and source-dirty declarations are in the two identity files and [matched-source-check.json](matched-source-check.json). The source-dirty diagnostic patches are explicitly retained; this is not a sealed clean-tree admission pair.

Corpus manifest `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`, tip `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`; selection `range(1,158,10) ∪ {157}`. All eight behavioral history switches unset, construction workers 1, detailed phases 1, as recorded in each declaration. Builds use Rust 1.85.1 and `--locked`; no third-party modifications, CI or retired preflight.

Exact commands and execution/limit/identity receipts live under `checks/` and `runs/`. From the repo root, the campaign's `collect.py baseline|candidate history-stride10 [--mode verify]` reproduces the command construction. It refuses existing samples. A new campaign needs fresh output paths and retained seals; do not rerun into these paths. `python3 <campaign>/analyze.py` derives results and CSV and refuses existing derived outputs. No result is silently overwritten.

Small raw receipts, traces, timing tree, test snapshots and patches are committed. Large immutable binaries and two raw Stores remain local in this exact campaign path, excluded by `.gitignore`; [custody.json](custody.json) records sizes and SHA256. They are not promised remotely retrievable. Final product changes: **none**. The experimental +7 production LOC (+2 phase instrumentation, +5 filter) are reverted. Final production totals: reference 65,417, core 20,305, combined **85,722 unchanged**; exact staged/parent accounting accompanies publication.

Disposition: keep the three proven local improvements. A simple one-second improvement is welcome under the owner's rule; this measured candidate does not meet it. Do not expand this filter or add caches merely to pursue a smaller effect. Historical v0.1.6 timer reconciliation, missing O3 pins and cache qualification remain open in #190.

## Subsequent owner allocation ruling

The owner accepts small allocation misses when they bring a good time reduction; see [the exact ruling and disposition](ALLOCATION-RULING.md). Numeric misses above remain measured facts, not automatic rejection grounds under this conditional ruling. This candidate still fails the one-second speed criterion, so the decision is unchanged.
