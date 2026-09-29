# #286 round 20260930-history-stride1-v3-r036

> **Status: TIMEOUT/TARGET_MISS and INCOMPLETE, not a storage PASS.** The
> explicit stride1/157 child exceeded the unchanged170 s complete-command
> bound before publishing its final state. There is no separate verifier or
> cleanup proof. Family 2 is not checkpointed at this source. No unchanged
> stride1 arm will be repeated to select a better time.

At clean source `57c511a2c`, the single explicit invocation reused the exact
locked-release r035 binary SHA-256
`4cdaf99ea524cd2a3cb9205bdd8a8c633715ca2391e06cbaece8e74a160bda80`,
compilation seal `c7057d6bb01412e684540abf87568652ddd62b64fc8703c3b11d8dbcac683bdc`
and harness seal `5956492473a737e8b136cbd116f6cc1f98f2454c9a636ce75d59e916c6385a6e`.
`build.json` records exact-binary reuse and wall0. Exact command:
`LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/benchmark/fs-bench-pro/runner.py run --case history-retention-stride-1-total-storage-v3 --out benchmark-results/fs-bench-pro/issue286-history-stride1-v3-r036`.
The fixed corpus/pins, InProcess C1/C2/C5 schedule, original strict storage
ceiling and one worker were unchanged; time is cache-uncontrolled and numeric
latency remains INELIGIBLE.

| Required result | Observed | Verdict |
| --- | ---: | --- |
| Complete driver | **170,018,091,458 ns /170,000,000,000 ns**, child timed out, no exit code | **TARGET_MISS** |
| Separate verifier | not run; no complete driver result | **NOT_RUN** |
| Original C2+C5 allocated | **76,742,656 B partial** (C2 76,546,048; C5 196,608) /strict `<83,947,520 B` | **INCOMPLETE**, never PASS |
| Public retained chain | **156 Layers /156 Branches /155 Commits** versus required157/157/156 | **INCOMPLETE** |
| Independent O3, state trees, selected bytes, roots | no completed trace or verifier | **INCOMPLETE** |
| Cleanup | timed child was stopped | **INCOMPLETE** |

The partial C2 apparent length was74,442,752 B. `sample_count` is
unavailable because no `timing.json` was emitted, while the receipt records
one attempted invocation. The native trace has only13 setup rows and zero
state-root rows; it cannot prove state157 or any of the independent semantic
gates. The C5 row-count mismatch and smaller partial allocation describe an
unfinished run, not a final-size comparison. `runner.py` kept the status
**FAIL** due the command miss, with semantic/storage/cleanup marked
INCOMPLETE. The prior r035 stride10/3 PASS rows remain separate; r031/r032
PASS at an older source seal and historical FAIL rows remain immutable.

[Exact failure receipt, partial original files, trace, command and SHA-indexed
raw evidence](20260930-history-stride1-v3-r036/evidence-index.json) are
retained append-only. The large partial C2 file stays at its indexed local
path. The next action when work resumes is a count-driven diagnosis of the
final save's work from retained r035/r036 evidence, then a meaningful
source/method change before any new stride1 sample; no timeout, worker,
cache or workload adjustment can convert this run to PASS. The requested
coordination stop takes effect after publishing this report. No Init or
family3–7 case ran at this source; #285 stays draft and #286 open.

This report-only commit changes no production code: reference65,417→65,417,
Core70,219→70,219, combined135,636→135,636 (delta+0), counted with
`tools/production_loc.py --json --root <snapshot>` on exact first-parent and
staged/committed Git archives, including runtime SQL and excluding tests,
examples, harness and docs (counter SHA-256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`).
