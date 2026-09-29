# #286 round 20260930-history-reference-r002

> **Status:** Dated engineering/reference-acquisition checkpoint; no compound candidate gate or release admission.

Family 2 engineering follows the successful [Init r001](20260929-init-r001.md). This round includes one independent sealed-reference acquisition per stride10/stride3 and a retained-vector reuse for stride1, plus the actual compound harness patch. The candidate has not been sampled by this round. All reference raw files, failed construction checks and final narrow checks are preserved (raw log publication uses deterministic gzip; decompression reproduces the original hash) in the [hash manifest](20260930-history-reference-r002/manifest.json).

## Sources, commands and observations

Reference checkout `/Users/yifanxu/.codex/worktrees/issue286-history-reference/layerfs` is clean, detached at sealed source `2f07f1f37af3e06a92a00880c68882b3c91923ef`, tree `faec3221657ebaf2ba055929fe60c16822ae0928`. The owner approved independent pin generation, then explicitly directed this agent to make subsequent decisions and continue all seven families without questions. Reference source was never edited; no other owner's worktree or PR head was altered.

Reference release build: `cargo +1.85.1 build --release --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked --bin fs-bench-storage-content`; worktree-local `core/target`, eight build jobs, construction workers1, explicit complete ARMv8 cfg/target-feature flags because the older reference has its historical configuration. Build completed in **14020630208 ns**, exit0, within the prospective170s infrastructure envelope. Executable SHA256 **04a272c69e1e593475657baa836556473a074a231536fa6168bd8303aaae450f**. This is an oracle-generation artifact, not an Init release arm or a paired speed control.

| Reference invocation | Selected states | Complete acquisition ns | Child-sum operation ns | C2 at-run allocated / apparent bytes | Canonical bytes / distinct objects | Root generation | Compound correctness/storage/numeric/cleanup gate |
| --- | ---: | ---: | ---: | --- | --- | --- | --- |
| stride10-r001 | 17 | 46799667833 | 27075090919 | 51900416 / 51867648 | 380921328 / 52032 | complete | NOT_RUN: reference C2-only schema7, no C5 or independent retained-product verifier; no numeric admission |
| stride3-r001 | 53 | 82025077000 | retained in phases-perf.json | 65679360 / 65675264 | 589916570 / 72560 | complete | NOT_RUN: reference C2-only schema7; original O3 pins disagree |
| stride1 vector reuse | 157 | no new invocation | unavailable here | archived historical readings not substituted | required pins remain871588115 /104705 | full retained vector reused | old historical INELIGIBLE stays unchanged; no new PASS |

Exact argv and method environment are in each published `command.json`/`result.json`; the commands call the existing driver with `--case history-stride10` or `history-stride3`, the original corpus path and a fresh at-run case directory. Each watchdog was prospectively170s and neither timed out. Both old drivers printed `PASS gates=2`: that only covers their two implemented gates and is **not** compound qualification. The new history storage thresholds were not applied retroactively to those reference receipts. First-use acquisition is not repeated for faster numbers.

Reference C2 schema is `user_version=7`, whereas the candidate profile explicitly checks10/six application tables. Store hashes: stride10 `7ea2fe6ccf13bc5aee7ba59bddef2d60d61d50d6b90329ee1488b1f096228358`; stride3 `d0aaff8a7a360469136a3a9d1c47548dc712f36ae5dba30542e1947877fa445d`. The actual original files remain local; their allocations came from those at-run files and not from a publication copy.

Stride1's full root vector is extracted from the original committed #209 merged receipt/trace. Its recorded source `8be0ae1a779a455145ea6c2b75e2154521536fad` has byte-identical C1 implementation, corpus reader and history driver to the chosen sealed reference (`git diff --name-only` over those owners is empty). All157 roots are retained, not selected from timing windows. The historical source, trace hash and its limitations remain in [the ledger manifest](../oracles/history-reference-v1/manifest.json). No historical numeric result is promoted.

## Implemented method and checks

[The frozen compound profile](../HISTORY-PROFILE-V1-20260930.md) records all three case IDs, original schedules/pins, strict storage ceilings, separate10/20/30s verifiers, source-aware schema identities, complete-command ceilings60/170/170s derived prospectively from the stride10 acquisition, and exact deterministic C5 identities/work/row counts.

The existing history driver remains the construction owner. State1 creates the actual C5 catalog/genesis/fork inside its timer. Each later state forks the prior Layer, stages the exact saved root, commits its token and publishes one Layer inside the state timer. A public-API check reopens every selected C5 Layer/Commit and final state. Native evaluator/verifier plus Python collector/retained report enforce actual closed C2+C5 `st_blocks*512`; distinct external indexes are not required and embedded indexes count once. Missing/unreadable measurement is INCOMPLETE, shared attribution INELIGIBLE, and total >=ceiling FAIL.

The new verifier path traverses every state's listed namespace with grouped inode lookups, checks all path/kind/size entries against the corpus, and hashes deterministic10% non-directory content plus endpoints. Digest/length cache is shared across states by distinct content object. O1 reads only the separately committed reference pins, never expected roots from the candidate trace. Old C2-only cases keep their original behavior and at-most64-path verifier. Historical IDs/profiles/receipts are preserved.

Construction attempts retained: the first compile found two harness API-signature mistakes (Workspace authority returns Result; read-only C5 open requires binding/cursor arguments). After correction, the first executable C5 test refused the next Layer with `StackMoved`: public C5 Branch bases are immutable, so a single Branch cannot publish successive Layers with rebased ancestry. The harness was corrected to fork each prior Layer under a deterministic per-state Branch. The17-state public persistence/reopen/allocation check passed. Its scratch was subsequently fixed to a worktree-local directory, with the changed-input check passing once there. Earlier failed test stores/logs are not silently promoted or removed. No product transition, quota, worker count or C5 schema was changed.

Narrow checks: strict Python boundary/missing/shared threshold test **PASS**; three-case registry/root-ledger cardinality/seal test **PASS**;17-state C5 persistence/reopen/wrong-root/missing/shared/over-ceiling check **PASS**; locked release harness builds **PASS**; lock parity **PASS**,47 shared entries,0 mismatches. Two pre-existing harness warnings (unused mutable counter and unused pipeline variable) remain recorded; no warning-denying full-harness claim is made. Core product owning checks are deferred to final changed source; no unchanged Core unit suite, CI/preflight or earlier-family performance sweep was run.

## Finding, gate status and next command

The reference stride3 counters **589916570 /72560** disagree with the preserved original gate **589423458 /73476** by **+493112 bytes /-916 objects**. This is a concrete canonical-profile finding requiring a source/counter explanation and shared-cause correction; its values are not new pins. Stride10's first-run pins **380921328 /52032** are frozen now from the independent acquisition. Every expected root vector is hashed and committed before any compound candidate sample.

No compound family2 case has passed by this report. Next command, after committing this actual patch/profile/pins:

```sh
CARGO_BUILD_JOBS=8 LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/benchmark/fs-bench-pro/runner.py run --case history-retention-stride-10-total-storage-v1 --out benchmark-results/fs-bench-pro/issue286-history-stride10-r003
```

The complete60s driver and separate10s verifier are frozen; neither may be increased after a miss. Inspect its actual per-owner allocation, independent roots/tree/bytes, C5 row counts, counters and cleanup. Diagnose the current family from retained receipts; do not rerun Init until all history correctness/storage gates pass. Families3–7 remain NOT_RUN and will follow the required order; this is not a Phase B completion or stop. PR#285 stays draft/unmerged and no issue is closed.

Production LOC for this harness/reference/profile commit: reference65417 ->65417 (delta +0); Core70022 ->70022 (delta +0); combined135439 ->135439 (delta +0). Method `python3 tools/production_loc.py --json --root <snapshot>` on exact first parent/final staged/committed source snapshots, counter SHA256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`. First-party product Rust/nested API/runtime SQL included; benchmark code, tests/docs/tools excluded. Exact report/code commit and staged tree are recorded in the commit message and #286 progress comment.
