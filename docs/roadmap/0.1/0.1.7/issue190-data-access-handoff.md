# Handoff prompt — #190 data access, reuse boundaries and execution pipeline

> Status: Research; informative and not a product contract. Dated continuation checkpoint, 2026-09-20, after PR #199. This prompt contains existing evidence and a bounded next investigation; it is not a new measurement or release claim.

## Mission and decision rule

Continue [#190](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190) from the successful implementation in commit **`5dc51b4f39e3965db37eec8f539982dfebdbba80`**. Explain and reduce remaining avoidable work where **data-access patterns, reuse boundaries and the execution pipeline differ from v0.1.6**.

The incremental tree algorithm is already broadly aligned: sorted changes, bounded child reads and untouched-subtree reuse. Do not begin by replacing the tree or assuming that every state scans the whole repository. Start with the remaining authenticated read work, select one measured mechanism, and test the smallest equivalent treatment. Review the other two areas without implementing three changes together.

Owner rulings persist:

- **One second of stride10 operation reduction is worthwhile.** The former two-second screening bar is superseded. Do not dismiss a credible one-second improvement.
- **A small allocated-storage overage is acceptable when accompanied by good time reduction.** Report both actual time and byte costs. No numerical tolerance or unlimited waiver was specified; do not invent one or relabel historical misses as passes.
- Preserve successful changes and correctness, authentication, bounded memory/work, error behavior and declared operation boundaries. A source difference is not a measured saving.
- Update #190 at attribution, treatment selection, stride10, stride3/verification and final disposition. Record exact production LOC for every commit.
- **Do not communicate with #192 or other independent user-owned tasks.** The owner explicitly stopped cross-task resource-window messages after the lock conflict was fixed. Use the global locks directly; do not revive permission handshakes. Do not create a new task merely to run this prompt.

The continuation is scoped to research, bounded same-semantics experiments and justified local improvements. It is not a blanket instruction to introduce a format/schema change, persistent index, broader cache policy, additional worker or pipeline rewrite. Keep larger changes as proposals until their measured benefit and contract implications are concrete.

## 1. Preserve these completed improvements

| PR | Retained change | Production LOC delta in that commit |
|---|---|---:|
| [#194](https://github.com/Ephemeral-AI-Lab/layerfs/pull/194) | Batched parent lookups and bounded authenticated base-record reuse | +49 |
| [#195](https://github.com/Ephemeral-AI-Lab/layerfs/pull/195) | Pooled physical-group telemetry and reuse through the existing 512 KiB decoded-group cache | +141 |
| [#196](https://github.com/Ephemeral-AI-Lab/layerfs/pull/196) | Existing bounded prepared-statement cache for catalogue `group_for` queries | −1 |
| [#199](https://github.com/Ephemeral-AI-Lab/layerfs/pull/199) | Group compression level 19 → 1, payload level 3 unchanged | 0 |

The current product total at this checkpoint is **85,722 production LOC**: reference `crates/` 65,417; replacement `core/` 20,305. Recount exact snapshots for any later commit; do not assume a newer main still has these totals.

[PR #197](https://github.com/Ephemeral-AI-Lab/layerfs/pull/197) archived and reverted the New-row filter in `zero_count_serials`: operation reduction **156,679,583 ns**, targeted phase reduction **53,181,962 ns**, below the one-second criterion. Do not reintroduce that filter as a new discovery. Validation's repeated absent-ID reads are a different, still-unmeasured mechanism.

[PR #198's source review](evidence/stage-6-history-190-legacy-review-20260920T031633Z/SYNTHESIS.md) informed the group-level experiment. Its descriptions of **current group level 19 are historical and superseded by #199**. Do not repeat the completed compression experiment or use old stop recommendations as current instructions.

## 2. Latest measured starting point

Use the **candidate** arm of [L40's results](evidence/stage-6-history-190-group-20260920T032843Z/results.json). These are existing single-sample diagnostics, not new baseline runs for your treatment.

| Quantity | Stride10, 17 states | Stride3, 53 states |
|---|---:|---:|
| Operation ns | 20,914,489,418 | 56,577,520,957 |
| Matched reduction from group-level-19 baseline ns | 1,591,930,585 | 1,832,995,164 |
| Store begin + accept + finish ns | 9,364,846,704 | 20,161,248,171 |
| Verification work ns | 5,405,493,708 | 18,904,055,292 |
| Verification target ns | 10,000,000,000 | 20,000,000,000 |
| Store apparent B | 49,324,032 | 62,152,704 |
| Store allocated B | 50,249,728 | 62,152,704 |
| Historical allocated target B | 49,344,512 | 64,024,576 |

Stride10 allocated size exceeds the target by **905,216 B**; matched allocation growth was **598,016 B**, accepted under the owner's time/space ruling. Apparent growth was 270,336 / 385,024 B. Stride3's allocation decrease despite increased apparent size is filesystem variation, not a compression-size gain.

All 70 roots and complete sorted `(ObjectId, role, canonical_length)` inventories match between those arms. All four separate verifiers pass, with 1,083 / 3,377 sampled paths per arm and zero mismatch/missing/unexpected. This is not exhaustive read-back of every path. Final validation: 494 core tests, 117 harness tests, 12 example targets, core Clippy/fmt, boundary guard and six self-tests PASS. Inherited unchanged harness Clippy/fmt failures remain unresolved.

Exact stride10 operation partition:

| Disjoint interval | ns |
|---|---:|
| File-content construction | 1,607,234,583 |
| Filesystem update | 9,569,275,834 |
| Store begin + accept + finish | 9,364,846,704 |
| Other child work / bookkeeping | 373,132,297 |
| **Sum** | **20,914,489,418** |

Provider reads occupy **8,609,430,602 ns inside filesystem update**, leaving 959,845,232 ns outside provider calls. Do not add nested provider time to the table. The arithmetic closes; the provider's internal attribution remains incomplete.

Latest stride10 provider work:

| Counter | Value |
|---|---:|
| Read waves | 66,616 |
| Requested / returned objects | 74,279 / 74,279 |
| Returned canonical bytes | 148,826,516 |
| Pooled leaf requests / chain edges | 11,268 / 16,570 |
| Physical record calls | 55,676 |
| Physical-group decompressions / cache hits | 2,723 / 52,953 |
| Physical-group decoded bytes | 123,780,247 |
| Value-group decompressions | 65,337 |
| Pooled pack fetches | 79,784 |
| Pooled pack bytes acquired | 7,985,771,449 |
| Ordinary group decompressions / connection opens | 1,163 / 16 |

**Pack bytes are application-buffer acquisition, not physical disk I/O.** Provider time is our current cost, not a measured excess over v0.1.6. Exclusive codec/SQL/copy/parse CPU is not available from these totals.

## 3. What is aligned, and what differs

```text
                             v0.1.6                         current core
                             ------                         ------------
Before operation       mutable Workspace checks       corpus transition prepares
                       + dirty tracking                an independent update batch
                               |                               |
Inside operation       consume dirty frontier          validate supplied input/base
                               |                               |
Tree update            sorted incremental merge        sorted incremental merge
                       + untouched-subtree reuse       + untouched-subtree reuse
                               |                               |
Required-group read    read pack control data          copy full pack BLOB
                       + selected body range           + parse/extract required group
                               |                               |
Reuse scope            pool reader shared across      per-leaf pool reader; shared
                       targets in a metadata group     bounded physical-group cache
                               |                               |
Construction/save      bounded producer/admission     construct output, then
                       pipeline with overlap           admit to Store sequentially
```

Read the [tree review](evidence/stage-6-history-190-legacy-review-20260920T031633Z/TREE.md), [storage review](evidence/stage-6-history-190-legacy-review-20260920T031633Z/STORAGE.md) and [boundary review](evidence/stage-6-history-190-legacy-review-20260920T031633Z/BOUNDARIES.md). Their source references are pinned; resolve using `git show REV:path | nl -ba` rather than assuming line numbers survived later edits.

Historical applicable source: `7fab1027a0061e8b932345d4fcd6ac22a089b155`. Actual compiled historical commit: `ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb`. Their relevant product/harness trees match, as documented in [S4](evidence/stage-6-history-190-20260919T225614Z/squad-s4/V016-COMMIT-COMPOSITION.md). The later source review found only five example/test additions under current legacy `crates/`; recheck if main has since changed. Do not substitute the v0.1.6 tag for the measured identity.

## 4. Priority A — attribute and reduce data acquisition

This is the first concrete direction. Inspect current:

- `core/crates/layerfs-storage/src/encoding/pool/read.rs`: `group_body`, `pack`, physical-record extraction.
- `core/crates/layerfs-storage/src/sqlite/lookup.rs`: `pack_bytes`, currently `SELECT data ...` into `Vec<u8>`.
- `core/crates/layerfs-storage/src/sqlite/pool.rs`: value-group catalogue.
- `core/crates/layerfs-storage/src/pack/layout.rs`: header/directory parsing and group view validation.
- Legacy `crates/layerfs-layerstack-store/src/objects/read.rs`: BLOB control-area and selected-group reads.

First split provider work sufficiently to distinguish catalogue lookup, BLOB acquisition/copy, header/directory validation, physical-group decoding, value-group decoding/authentication, and reconstruction. Prefer existing counters/profiling; add only necessary bounded aggregate telemetry, with identical instrumentation in both matched arms. No per-object span explosion or test-only product hooks. Keep profiling diagnostics separate from unprofiled performance samples. Stack-residence samples are not exact elapsed/CPU seconds.

Measure actual acquired bytes, demanded body bytes, control-area bytes, opens/range calls, repeated pack/group demands and relevant cache hits/evictions, where feasible without an unbounded instrumentation map. Record which quantities remain unmeasured. Select a treatment only after establishing plausible one-second removable work; do not treat the entire 8.609-second provider interval as removable.

The candidate is a **selected-group BLOB read** rather than whole-pack copying. Existing rusqlite already has BLOB support. However:

1. The current catalogue gives pack/group/ordinal/digest, **not byte offset and length**. Obtain actual BLOB length and parse the control area.
2. Preserve validation of the **complete bounded directory**: continuity, extents, codecs, final coverage, lane, locator, selected-body length, read ceiling and canonical identity. Porting legacy's partial-directory checks would weaken current behavior.
3. Reuse/refactor the existing parser; do not create a second grammar or schema merely to avoid this work.
4. Account for extra BLOB opens/range calls and possible loss of the current 4 MiB bounded per-leaf pack-cache benefit. Fewer copied bytes do not guarantee faster operations.
5. A format-preserving read-only treatment should leave saved Store bytes and canonical inventories unchanged; explain any difference before claiming equivalence.

Use one isolated treatment. Do not combine range reads, new SQL preparation changes, wider caches and streaming in one arm.

## 5. Priority B — reuse boundaries, after attribution

Trace actual lifetimes and repeated demands before changing them. The current shared decoded physical-group cache is already implemented; re-proposing it is not new work. Value reconstruction and pack caching have different lifetimes from that cache.

Current `encoding/delta/read.rs` creates a pool reader per resolved inode leaf. Legacy shares a bounded pool reader across targets within a demanded metadata record group. Ask whether a **bounded reuse scope within an existing immutable read operation** can eliminate measured repetition without new long-lived state. Identify the exact root/snapshot, Store mutation boundary, captured read ceiling, cache key and invalidation contract. Preserve existing byte limits and account for additional retained bytes; never retain entire history proportional state or prime it in setup.

Moving a physical-group cache lookup before pack acquisition is **not an automatically equivalent branch reorder**. The current cache stores decoded bytes; the existing path validates lane/directory/range before consulting it. Any reuse of an authenticated extraction must prove those checks remain valid under its precise lifetime, and cover corruption, missing rows, ceilings and mutation invalidation. Keep that distinct from a range-read experiment.

Validation's positive-only inode memo also repeats some absent-ID reads. This is a fallback attribution question, not the rejected zero-count filter. Measure first absence proof versus repeated negative misses and actual pages/time; many high new IDs stop near the root. The older 2.795-second validation interval does not establish that this subpath can save a second.

## 6. Priority C — execution pipeline, only with measured justification

Legacy pipelines canonical production and admission; the current history driver constructs a buffered object batch and then clones/adopts objects into the Store. Trace ownership, copying and buffering at `core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs`, the C1 output boundary and C2 accept/finish boundary.

Separate caller/harness copying from compression, index work and transaction work. Do not assign all 9.365 seconds of Store time to staging or copies. Distinguish an authentic product improvement from a harness-only ownership change, and report the changed timer/work boundary if any. A harness-only result must not be sold as a core algorithmic speedup.

No additional producer/helper worker or changed worker rule. A single-thread streaming proposal must preserve fallible-call semantics, atomic publication/visibility, validation before acknowledgement, cancellation/error cleanup, bounded memory and output ordering. Treat a new C1/C2 ownership or public API design as a proposal until its measured opportunity justifies the added complexity. File-content construction alone is 1.607234583 seconds in this receipt; its entire duration is not a promised achievable overlap saving.

## 7. Evidence that must not be misused

- Recorded v0.1.6 Commit: **11,370,679,212 ns**. Latest core operation: **20,914,489,418 ns**. Difference **9,543,810,206 ns**, ratio **1.8393351028606961**: descriptive, unmatched historical arithmetic, not an attributed defect budget.
- Legacy Commit includes canonical content construction, directory/tree work and admission. Those were not all outside its timer. Legacy `namespace_ns = 342,355,542` excludes dirty-directory work charged to Content; it is not comparable to the whole current filesystem interval. Legacy timing subfields overlap.
- Historical effective workers, cache and machine state are not matched. Do not rerun v0.1.6 for this continuation or claim that source inspection makes the comparison matched.
- The reviewed retained stride10 validation counters show **zero directory pages read in every state**, and 359 entries examined only at initial build. Whole-base alias/cycle walks exist for other workloads but do not explain that observation. No subtree-summary work based solely on the old full-scan hypothesis.
- Both versions rewrite whole open-pack BLOBs. The persisted candidate index is bounded and does not scan the whole repository. Transaction counts alone do not prove removable SQL time.
- Do not compound prior campaign percentages, mix the older level-19 provider-byte totals with current level-1 totals, or rerun a case for a better number.

## 8. Measurement and resource protocol

Read [AGENTS.md](../../../../AGENTS.md), [core/AGENTS.md](../../../../core/AGENTS.md), [benchmark rules](../../../general/benchmark_rules.md), [benchmark agent rules](../../../../benchmark/AGENTS.md), [quickstart](../../../../benchmark/fs-bench-pro/QUICKSTART.md), release/documentation policies and the lane's [retained-history rulings](retained-history-storage.md) before work.

- One sample per case/arm; fresh output; append-only raw receipts including failures, deferrals and non-passing rows. Freeze identities, treatment, cache declaration, operation boundary and limits first. Reuse setup and valid sealed builds, never measurement results as new samples. New instrumentation requires an equally instrumented matched baseline.
- Iterate on stride10, then confirm a worthwhile candidate once on stride3. No stride1 optimization or level sweep. Reject a small/unattributed benefit and retain its evidence.
- The established #190 raw-driver **diagnostic** command caps are 120 seconds for stride10 and 240 for stride3; separate verification hard cap 60 seconds, work targets 10/20 seconds. These are not ordinary 15/25-second admission rows and must not be promoted as such or enlarged to pass.
- Existing OS/intra-chain cache state is uncontrolled: admission **INELIGIBLE**. Do not invent a cold claim, pre-touch measured inputs, or move required reads into setup. O3 pinned-counter gates remain **INCOMPLETE**. A completed raw driver is not release admission.
- One construction worker; all eight behavior switches explicitly set or unset in receipts: `LAYERFS_HISTORY_ADVISORY`, `LAYERFS_HISTORY_ORDERED_PREDECESSORS`, `LAYERFS_HISTORY_DECLARED_ONLY`, `LAYERFS_HISTORY_SIMILARITY_CANDIDATES`, `LAYERFS_HISTORY_FALLBACK_CANDIDATES`, `LAYERFS_HISTORY_FULL_PRODUCER`, `LAYERFS_HISTORY_CHUNK_PREDECESSORS`, `LAYERFS_HISTORY_DEPTH_LIMIT`. Current baseline has all unset, `LAYERFS_CONSTRUCTION_WORKERS=1`, `LAYERFS_HISTORY_PHASES=1`.
- Serialize builds, tests, performance, verification and large artifact inspection. Acquire global `flock(LOCK_EX|LOCK_NB)` on resolved **`$TMPDIR/layerfs-infra-measurement.lock` then `/tmp/layerfs-infra-measurement.lock`**, deduplicating identical resolved paths; hold descriptors for the resource command. A held lock means defer, without contacting another task or interrupting its process.
- The harness private `.measurement.lock` is a **different protocol**: `shared/receipt.py::measurement_lock` uses `O_CREAT|O_EXCL`, writes a PID/time marker and removes only its own marker at exit. Never open this path with append+flock and never open another checkout's private marker. The prior race was traced to that protocol mix-up and fixed; do not recreate it. Do not blindly delete a marker. Any stale-file recovery requires ownership checks, both global locks and preserved metadata/file evidence.
- Quiet check: no named competing cargo/rustc/fs-bench process, at least 70% CPU idle on the second one-second observation. Declare remaining desktop noise. Separate containers do not isolate host CPU/disk; this lane is native macOS and needs no new containers or coordination service.
- Build with Rust 1.85.1 and `--locked`. No third-party edits, vendoring or patches. No CI or retired `tools/preflight.sh`.

Use the existing [L40 collector](evidence/stage-6-history-190-group-20260920T032843Z/collect.py), [command recorder](evidence/stage-6-history-190-group-20260920T032843Z/run_command.py) and [sealer](evidence/stage-6-history-190-group-20260920T032843Z/seal.py) as references. **Do not execute them into L40's existing paths.** Create a fresh campaign under `evidence/stage-6-history-190-read-<UTC stamp>/`; check their path assumptions and reuse existing mechanics rather than creating another lock system.

The raw driver is `core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content`, with arguments `--case history-stride10|history-stride3 --corpus PATH --out FRESH_PATH --phase perf|verify`. The runner still is not an admission shortcut. Run through the declared collector under locks and preserve `trace-perf.jsonl` before verification appends to the raw trace.

## 9. Source, corpus and artifact custody

The primary checkout may contain another task's work. Do not reset/clean it, import uncommitted changes or send resource-window messages. Start from a clean isolated checkout containing #199 and record actual HEAD. If current main has advanced, inspect changes and revalidate compilation inputs; the checkpoint totals and timings are not automatic claims about newer source.

Corpus: `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`.
Manifest SHA256: `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`.
Tip: `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`.
Stride10 selection: `range(1,158,10) ∪ {157}`, 17 states. Confirm stride3 from harness pins; do not shrink selections.

Retained L40 campaign: [evidence directory](evidence/stage-6-history-190-group-20260920T032843Z/README.md), [results](evidence/stage-6-history-190-group-20260920T032843Z/results.json), [inventory](evidence/stage-6-history-190-group-20260920T032843Z/store-comparison.json), [review](evidence/stage-6-history-190-group-20260920T032843Z/REVIEW.md), [custody](evidence/stage-6-history-190-group-20260920T032843Z/custody.json).

- Measured level-1 candidate binary SHA256: `13bcfe86598d3c13f902d6ab7d0a15d24f9ed26bb2c67dadbfe1a34ea6108a2d`.
- Final comment-corrected build SHA256: `7dff19d828afe760a98ca614a9a99a6630bc94533347d78a467835ba7568a736`. It is **not** the measured binary. Executable source-line equivalence is retained separately; do not rewrite the measured identity.
- Large binaries/Stores are local-only under `/Users/yifanxu/.codex/worktrees/history-parent-lookup/layerfs/docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-group-20260920T032843Z/`; they are excluded from Git. Verify existence/hash when legitimately needed outside measurement. If unavailable, state the custody gap instead of fabricating artifacts or silently substituting a rebuilt binary.
- Small raw traces/timing/phase receipts, source patches and check logs are committed. Build a fresh matched baseline for new source/instrumentation when reuse identities do not match. Reusing an archived binary does not make its old sample a new measurement.

## 10. Work organization, proof and deliverables

Keep the first round finite. If local subagents are used, give separate owned documents to data-access, reuse and pipeline readers, with a read-only adversarial reviewer after synthesis. They must not run heavy work in parallel, edit one another's documents or contact unrelated user-owned tasks. The coordinator alone serializes resource commands. Preserve disagreements and rejected hypotheses.

Produce:

1. An itemized legacy/current call-flow comparison with pinned source references and ASCII diagrams; label source facts, measured costs and hypotheses separately.
2. A per-state provider decomposition and real-work counters, with nested spans and residuals accounted for. Name any unmeasured remainder.
3. One predeclared treatment, its semantic/bounds proof, external tests and exact baseline/candidate identities. All reused mechanisms must be named; no test-only product shortcuts.
4. One stride10 matched pair, actual time/CPU/memory/bytes/work changes and all failures. If worthwhile, one stride3 confirmation and separate identity-matched verification. Compare all roots, canonical inventories and appropriate physical invariants; do not overstate sampled verification.
5. Required explicit core tests/examples/Clippy/fmt, boundary guard/self-tests and affected harness checks if retaining code. Do not rerun unchanged known-failing harness lint and call it a pass; cite gaps precisely. Architecture changes follow algorithm/boundary changes in the same commit.
6. An independent re-derivation of headline arithmetic and identity/custody checks. Only the locked coordinator authorizes heavy hashing; no overlap.
7. A report, an append-only entry after L40 in [the active ledger](../0.1.6/evidence/issue151-experiment-ledger.md), #190 updates and exact first-parent/staged production LOC for each commit. Keep reference/core subtotals and disclose any scope changes. Attach any created PR to the task.
8. A final disposition: retain, reject or unresolved, with the numbers and missing evidence. Do not close #190 merely because a local improvement works; O3/cache qualification and unmatched historical attribution remain separate.

Start by reading the three source reviews and current L40 raw phase/counter results, then trace **pack acquisition → complete directory validation → group/value reconstruction**. The question is how much repeated work can safely be removed from that path, not how to force the total to equal 11.37 seconds.
