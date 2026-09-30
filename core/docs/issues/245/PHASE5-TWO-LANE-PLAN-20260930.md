# Phase 5: Core and deferred optimization lanes — 2026-09-30

> **Status: Dated planning checkpoint; not release evidence or a product contract.**

The owner requests two execution lanes: Core and deferred items for optimization.
The source baseline is merged main `191b6997cd90f875f67c00ee2659dfc9b6c27cbd`.
The [ticket audit](../286/TICKET-AUDIT-20260930.md),
[seven-family checkpoint](../286/SEVEN-FAMILY-CHECKPOINT-20260930.md) and
[proof reuse map](../286/experiments/20260930-seven-family-reuse.json) establish
the current implemented scope. This packet assigns work; each changed mechanism
still needs its prospective source-bound design and proof before measurement.

## Lane 1 — Core

**Quest:** make large-file and many-file Commit processing bounded in resident
memory, preserve incremental generations and writer progress, then enable
independent concurrent Exec and multiple mounted Workspaces.

| Order | Issue | Deliverable |
| --- | --- | --- |
| First | [#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) | Cursor-driven final-delta lowering/descriptor/upload processing, bounded buffers and page windows, remaining count/encoding boundaries, exact successor-base reuse and write progress across all Commit phases. |
| Alongside | [#256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256) | Bounded namespace/frontier/reconciliation traversal, larger listings and handle/count boundaries, many-file successive-Commit and retained-view proof. |
| After #248 acceptance | [#249](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249) | Multiple concurrent opaque SDK Exec calls and mounted Workspaces, command leases/output/process-group lifecycle and one Commit/Stage slot per Workspace. Preserve the explicit command-lifetime decision separately from benchmark and Commit limits. |
| With #249 | [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219) | Operator-configurable live Workspace count with 1/2/3 admission, refusal, isolation, teardown and replacement. |

Parent [#245](https://github.com/Ephemeral-AI-Lab/layerfs/issues/245) remains the
integration/scale tracker. The whole dirty-file/extent/deletion-key/update-patch
RAM materialization recorded in #276 is **Core ownership**. The recent same-quota
C5 page-credit fix does not establish bounded frontier memory.

First checkpoint: audit the current paths and reusable evidence, freeze the
bounded-streaming design and smallest covering proof selection, implement one
coherent shared-path improvement, and publish its exact-root/byte, resource,
writer-progress, known/unknown-outcome and cleanup evidence. Do not stop at a plan
or expand straight into the entire benchmark migration.

Core owns changes to Workspace capture/lowering/upload/reconciliation, namespace
streaming, the necessary SaveFile/metadata Bridge and Server composition, and
later daemon/SDK concurrency. Core owns the corresponding architecture updates.
C1 interface changes and files shared with the optimization lane require an
explicit ownership checkpoint before either lane edits them.

## Lane 2 — Deferred items for optimization

**Quest:** reduce demonstrated work/amplification in already-functional paths
while preserving bytes, identity, pins, budgets and failure/cleanup ownership.
Use the existing [#276](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276)
tracker; do not recreate its retired #277–#282 issue split.

| Priority | Item | Scope |
| --- | --- | --- |
| First | #276 deep270 / pure-move C1 cost | Diagnose and reduce cycle/alias validation work in C1; the [Family 5 checkpoint](../286/FAMILY5-CHECKPOINT-20260930.md) records the 270-component workload. Review ReserveInodes call cost as a separate mechanism. Retain final-batch cycle/unique-parent checks and independent old/new identities. |
| Next | #276 hot normalization pages | Establish necessity and measured page work; remove work only with authenticated fence/child/slot-epoch, old-pin and exact charge/refund equivalence. |
| Next | [#261](https://github.com/Ephemeral-AI-Lab/layerfs/issues/261) | Focused ordinary mounted-write overhead attribution/optimization. Existing uncontained PR #262 is a source donor/reference, not permission for a blanket merge. |
| Later | [#218](https://github.com/Ephemeral-AI-Lab/layerfs/issues/218) | Store write amplification and placement/I/O work, with its own instrument and qualified compatibility decision. |
| Later | [#208](https://github.com/Ephemeral-AI-Lab/layerfs/issues/208) / [#190](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190) | Chunk-predecessor time/space attribution and history read qualification. These are separate registered mechanisms, not a broad compression campaign. |

First checkpoint: classify stale versus current #276 items using the merged
source and existing proofs; inspect deep270 cause evidence; freeze a focused
count-driven hypothesis/proof; implement and verify one compatible improvement
if supported. Initially own C1 namespace validation and its external diagnostics,
tests and architecture description. Any allocator/Workspace, SaveFile/Server,
C2 or shared module edit needs a file-ownership checkpoint with Core first.
Do not edit the other lane's source merely because a related optimization is
convenient. A new canonical owner index or format/schema change needs its own
reviewed compatibility packet before implementation.

## Deferred boundaries and qualification

- The old 10240 case remains OWNER-DEFERRED/SKIPPED. Core may address the shared
  streaming cause, but this assignment does not silently activate that benchmark.
- Dirty-Workspace discard/close policy remains a separate functional design in
  #276; it is not a speed optimization. FUSE interruption/crash-safe unmount and
  statfs remain [#259](https://github.com/Ephemeral-AI-Lab/layerfs/issues/259).
- [#283](https://github.com/Ephemeral-AI-Lab/layerfs/issues/283) remains the separate
  host measurement capability task. Missing cache/phase-memory qualification is
  reported honestly and does not force an unchanged probe campaign.
- Existing 4097-write and 1025-file functional proofs are reused with their
  exact scopes. General arbitrary scale, full writer progress and independent
  concurrent SDK Exec remain new proof obligations. Old #276 body statements
  about absent leases, unmerged integration and test reds require a current-source
  status audit; historical observations are not rewritten.
- Preserve the up-to-10% accepted history allocation profile. Never trade 5% of
  storage for 50% speed loss; roughly 5% storage for 5% speed is an acceptable
  trade only with explicit evidence. Do not restart codec churn to meet obsolete
  strict history thresholds. No routine Family 2 reruns.

## Execution and integration

Each lane is a dedicated user-owned task. Each first establishes its own clean
managed worktree from the published plan source; the primary checkout, prior
owners' worktrees, frozen reference and untracked poster artifacts are preserved.
Each uses a private worktree Cargo target, scratch/output paths and unique
container identities. The lanes are not alone in the repository and must never
revert or overwrite the other's changes.

Read root/core/benchmark AGENTS, measurement/release/documentation rules and
`benchmark_agent_report.md`. Freeze selected workload/oracle/bounds before new
measurements; one sample per case/arm, separate independent verification, explicit
proof/build/fixture reuse and append-only failures. Do not interrupt another run;
declare cross-worktree build or machine interference rather than claiming a quiet
window. No new global benchmark lock or another worktree's Cargo target.

Keep the one-construction-worker rule and Init exception, quota/Budget, actual
operation limits and ARMv8 profile. No third-party patches, extra workers,
fsync/WAL, hidden retries, fixture-specific algorithm or limit inflation.
Verify only changed owning paths and required final checks, preserve historical
INELIGIBLE/FAIL/NOT_RUN, and record exact first-parent/staged/committed production
LOC on every commit. The current totals are reference65417/Core70279/135696.

Each lane publishes a reviewable patch, source-pinned report and draft PR with
scope, proof/reuse and exact LOC; attach created PRs in its own task. Future
implementation PRs are reviewed before integration. Do not automatically close
broader issues or merge separate donor work. Report progress in the lane's own
chat; the coordinating chat can inspect it without unsolicited cross-chat
messages.
