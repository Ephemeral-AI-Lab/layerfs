# Cluster-two implementation handoff routing

> **Status:** Current planning checklist; no release candidate exists.
> Owner dispatch reconciled 2026-10-08 at `1a6bb53ef14e1860d8f222df11394e5a654bb34d`.

The earlier S0–S13 copy-and-dispatch prompt is historical and superseded. Its
original text remains in Git at the input pin above and the R0 original hash
receipt. Do not copy/dispatch it, reopen completed S0–S6/pre-S8 work, build host
runtime adapters/daemon command supervision, or publish tracker updates from it.
Current primary contracts and owner directions establish target behavior;
source and retained scope-specific proofs establish implementation.

The fresh owner assignment is the continuous R0–R9 rollout, including actual
native FUSE, complete live incremental Commit, covered cleanup, integrated
qualification and conditional root-reference retirement. Read the
[current dispatch](../307/HANDOFF-S8-IMPLEMENTATION-20261008.md),
[rollout contract](../307/CLUSTER-TWO-LOC-AND-ROLLOUT-20261008.md),
[deepest destination layout](../307/FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md),
[current S8 specification](../307/S8-SPECIFICATION-20261008.md), and
[local rollout ledger](../307/ROLLOUT-LEDGER-20261008.md). They supersede the old
C1-only dispatch and obsolete early-slice/host-runtime file plans.

SDK organization is ProjectApi/WorkspaceApi/SandboxApi. Ordinary Sandbox/runtime
or external executor owns commands, standard streams/status and explicit
cancellation. Optional WorkspaceApi.exec selects mounted cwd and delegates.
Daemon owns exact filesystem request/open/lookup/capture/Commit work and complete
drain, with no supervisor/launcher/per-Exec cgroups/registration/custom Exec wire.
FUSE serves every permitted visible process; shell exit is no filesystem fence.
Forced connection teardown does not kill callers. Host Init/seal/install is
separate and control-only afterward; each daemon opens shared Store in-process.

Local implementation, docs, scoped checks/proofs/applicable registered qualification
and local checkpoint commits are authorized through R0–R9. R9 retirement occurs
only after R8 acceptance plus dependency/replacement coverage. Remote issue edits,
push/release/deployment, new worktrees, destructive resets and unrelated resources
are not authorized. Preserve all historical receipts/verdicts, protected notes,
containers/processes/worktrees and the root reference until conditional retirement.

Use the active native Goal across turns. Read-only subagents provide scoped
research/review; the main owner alone edits and runs serialized Cargo/tests/
measurements. Each checkpoint has deepest-file ownership/cost/custody/proof,
retained exact outcomes, affected documentation, scoped final checks and exact
first-parent/staged/committed production LOC. Current dispatch retains all
profile, one-attempt, bounded-test, benchmark/cache/budget and provenance rules.
R0 contract alignment is not native/Commit qualification or Goal completion.
