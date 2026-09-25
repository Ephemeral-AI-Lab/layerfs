# #241 Phase 1 rollout: mounted range edit

> **Status:** Current planning checklist; no release candidate exists.
> Phase 1A's Linux ioctl mechanism and four-case functional proof are complete.
> Phase 1B will diagnose remaining Commit growth before #232's 56-case rollout.
> Raw v4 timing is cache-ineligible; prior receipts remain unchanged.

Tracking: [#241](https://github.com/Ephemeral-AI-Lab/layerfs/issues/241), child of [#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232). Read the [specification](SPEC.md), [implementation and optimization handoff](IMPLEMENTATION_PLAN.md), [Phase 2 plan](../232/ROLLOUT_PHASE2.md), [benchmark rules](../../../../docs/general/benchmark_rules.md) and [Core harness rules](../../../benchmark/fs-bench-pro/AGENTS.md).

## What Phase 1 proved

A cooperating program opened a mounted file, issued the checked Linux STATE/EDIT ioctl, and edited a private Workspace through public WorkspaceApi::exec. The **FUSE ioctl carrier** is independent of the shell or interpreter that launches the issuing program. The **current SDK Exec implementation still launches /bin/sh -c**, so SDK Exec itself has no selectable interpreter yet. LayerFS does not parse shell text or convert arbitrary POSIX writes to EDIT. An unmodified editor that rewrites a suffix still pays for that I/O. Public WorkspaceApi::commit publishes the edited Workspace through ordinary C1/C2 construction. The live benchmark uses no private SDK method or direct Service/Store mutation.

The [position sweep](https://github.com/Ephemeral-AI-Lab/layerfs/blob/cb1bdb70e97628c2c38055ff2600e070e742010e/core/docs/issues/241/evidence/phase3-postfix-v4/REPORT.md) passed **264/264** preregistered position checks at one source identity. The separate [v4 four-case report](https://github.com/Ephemeral-AI-Lab/layerfs/blob/87b10ab1b2144f1d7fb54f1be9419095f891fea0/core/docs/issues/241/evidence/phase4-v4-admission/REPORT.md) passed Exec, Commit, independent full-file oracle, Branch/old-Commit checks and cleanup at every size. Each row retained two STATE callbacks, one EDIT callback, 4,096 accepted replacement bytes and **zero shifted suffix bytes**. These are functional and count-based conclusions, not latency admission. The implemented mechanism lives in:

    core/crates/layerfs-fuse/src/adapter.rs
    core/crates/layerfs-fuse/src/range_ioctl.rs
    core/crates/layerfs-workspace/src/filesystem/write.rs
    core/crates/layerfs-workspace/src/filesystem/projection_counters.rs
    core/crates/layerfs-workspace/src/overlay/pieces.rs
    core/crates/layerfs-workspace/src/commit/lower.rs
    core/benchmark/fs-bench-pro/workload/src/main.rs
    core/benchmark/fs-bench-pro/registry/workspace-exec-insert-v4.json

The current ioctl transfers a fixed 4,192-byte frame and copies k owned payload bytes, with k≤4 KiB. The Workspace piece-vector splice and Commit lowering are O(P) in live pieces P; one pristine middle splice leaves few pieces, but R repeated requests may cost O(sum P_i), potentially O(R²) as piece count grows. C1's Chunked path processes local replacement bytes and affected extent-tree paths; WholeFile/cutoff transitions can process O(N) bytes. C2 service.finish has measured growth but no established asymptotic bound. The complete Exec→Commit path has **no O(log N) proof**. See the [algorithm analysis and conditional change map](IMPLEMENTATION_PLAN.md#algorithm-and-resource-bounds).

## Four retained release rows

| Pristine size | Edit | Commit | Edit→Commit | Admission |
| --- | ---: | ---: | ---: | --- |
| 1 MiB | 25.548 ms | 22.073 ms | 47.632 ms | INELIGIBLE (derived) |
| 10 MiB | 25.342 ms | 23.062 ms | 48.413 ms | INELIGIBLE (derived) |
| 100 MiB | 27.859 ms | 31.414 ms | 59.284 ms | INELIGIBLE (derived) |
| capped 500 MiB | 28.213 ms | 53.452 ms | 81.680 ms | INELIGIBLE (derived) |

All four used locked **release** host driver, verifier, daemon and tool; one sample per case; fresh independent writable copies of closed prepared masters; fresh output paths; and public SDK Project/Branch/Sandbox/Workspace setup, Exec, Commit, Status, Unmount and Sandbox Delete. The operation timer was **layerfs-telemetry LFT1**, starting immediately before SDK Exec and ending at the typed Commit result, with Edit and Commit children. Mount, setup, status, cleanup and the identity-matched independent verifier stayed outside the operation timer. Host and daemon CPU/RSS samples were retained, but their first/last samples did not enclose both scope boundaries. They describe sampled process windows, **not exact per-phase CPU or peak RSS**. No Python wall clock, lifetime cgroup peak or unavailable sample substitutes for operation telemetry.

The original v4 performance receipts remain INCOMPLETE because the first telemetry parser misclassified a normal final sample. A parser-only recheck of retained raw logs derives INELIGIBLE for all four under the frozen Linux FUSE backing cache contract, without taking another sample or promoting those receipts. The historical v0.1.6 G2 target used direct SDK range Edit→Commit and is not a matched Exec/FUSE baseline. Neither a timeout change nor a rerun of these four arms can repair cache eligibility. Preserve original v2, v3 and v4 receipts and failures append-only.

## Phase 1A exit and Phase 1B work

**Phase 1A's Linux mechanism exit is achieved:** kernel delivery, mounted coherence, functional position sweep, four public SDK Exec→Commit completions, independent verification, cleanup, bounded callback/byte counts and no suffix-proportional tool/FUSE work. It does **not** close #232, prove a cold speed gain, or advertise macFUSE/WinFsp support. Each future adapter needs its own live mounted proof; unsupported capabilities fail explicitly. Third-party packages remain published and unmodified.

**Phase 1B** starts with a new-identity, count-driven `service.finish` diagnostic at 1 and capped-500 MiB. Split batch drain, pack seal, index flush/copy, publication and SQLite commit with bounded LFT1 children; record object, statement, page and physical-read counts. Fix only the substep shown to cause the growth. Do not change the ioctl, piece representation, cache policy, deadline or worker count to improve a number. The [implementation handoff](IMPLEMENTATION_PLAN.md#next-decisions-in-order) proposes raw engineering caps of ≤55 ms at 1/10 MiB, ≤60 ms at 100 MiB and ≤70 ms at capped 500 MiB (≤65 ms stretch), with 500-minus-1 MiB `service.finish` growth ≤15 ms, under an equal declared cache state. These are **prospective proposals**, not retroactive v4 gates or speed PASS claims. A new committed specification/scenario identity must freeze any numeric acceptance target before candidate optimization or sampling; a cold speed PASS also requires an enforceable cache contract.

Phase 1B exits when the growth has a retained cause attribution and a decision on the measured substep. If a narrow product fix is justified, prove it with focused correctness checks and a newly frozen four-case SDK Exec→Commit selection, independent verification, cleanup and zero suffix I/O. Report every raw duration and cache status, including a target miss or `INELIGIBLE`; neither is turned into PASS by rerunning. If counts show no justified product change, record that decision and the remaining growth explicitly **without repeating the unchanged four arms**. The [#232 Phase 1C complexity screen](../232/ROLLOUT_PHASE1C.md) follows this decision; its [Phase 2 rollout](../232/ROLLOUT_PHASE2.md) owns all 56 SDK Exec/FUSE edit families under the [unified ioctl contract](../232/UNIFIED_IOCTL_IMPLEMENTATION_PLAN.md).

Phase 1B addresses only the measured `service.finish` growth. Phase 1A already removed the old suffix-copy `O(M)` and repeated-callback `O(C²)` mechanism **for cooperating ioctl editors**. It did not remove the current `O(P)` piece-vector scan or its possible `O(R²)` total across `R` edits, the bounded WholeFile/cutoff reconstruction, or suffix copying performed by an unmodified editor. Those require separate count evidence and, if material, separately scoped changes; a faster `service.finish` cannot fix them.

For new runs: every product operation uses public SDK, the cooperating tool uses only the documented mounted-file ioctl, and operation wall/CPU/RSS comes only from LFT1. Use locked release builds, one construction worker, one sample per case/arm, a fresh append-only receipt, a complete command within 15 s and a separate identity-matched verifier. Reuse validated preparation only outside the timer. Never pre-touch a measured path or let Edit's resident writes credit a cold Commit. Retain every FAIL, INCOMPLETE, INELIGIBLE and NOT_RUN.
