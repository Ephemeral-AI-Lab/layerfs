# #245 scalable ordinary Workspace range COW

> **Status:** Current planning checklist; no release candidate exists.

Tracking: [#245](https://github.com/Ephemeral-AI-Lab/layerfs/issues/245),
following the [#243 ordinary shell baseline](../243/README.md) and parent
[#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232).

This folder plans the generic `WorkspaceApi::exec(command)` → `/bin/sh -c` →
mounted POSIX/FUSE path. Phase 1 addresses the previously observed root
lockfile rename failure, repeated piece-index work, large-shift progress
failures and verification eligibility. The architecture also identifies the
file, namespace and Commit interfaces that must scale before much larger
package workloads can be claimed. The [E/F report](evidence/phase1-e-f-final/REPORT.md)
records a frozen comparative wall PASS at its source seal; its latency cells
remain INELIGIBLE. This folder is not a product implementation or release
contract.

The target keeps index pages and payloads in local private backing. A Commit
pins one immutable generation; admitted later FUSE writes enter the next. The
strict E gate proved one such write during a held successor-builder page read,
followed by a second sequential Commit at `b2cd0df23`. Other Commit phases
still take a metadata writer gate during frozen-file walks and transfer pulls;
see the [phase-by-phase audit](ARCHITECTURE_COMPLEXITY_RESEARCH.md#what-writes-continue-during-commit-currently-proves).
The #252 SaveFile cutover removed the explicit 4,096 final-run admission check.
[#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) still owns its public 4,097-run and scaling proof;
[#256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256) owns the separate many-file namespace limits;
[#249](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249) owns daemon-control concurrency.
The [resource-constraint lift plan](RESOURCE_CONSTRAINT_LIFT_PLAN.md) records
the later owner direction: one Commit/Stage submission per Workspace is the
only logical serialization rule; multiple Workspaces and commands may overlap
when their charged resources admit them. It also separates per-call chunk sizes
from whole-file limits and specifies the removal of the 30-second whole-Exec
timer. These are target behaviors, not current product claims.

- [Scalable range-based COW architecture](ARCHITECTURE.md) describes current
  and proposed data paths, immutable generation capture, streaming Commit,
  resource limits and unresolved concurrency invariants.
- [Joint #248/#256 tree and resource research](JOINT_248_256_TREE_RESEARCH.md)
  pins the post-#252 source, calculates file and namespace capacities, compares
  current and target costs, and proposes responsibility-based modules and LOC.
- [Resource-constraint lift plan](RESOURCE_CONSTRAINT_LIFT_PLAN.md) inventories
  the remaining file, Exec, page-reference and live-state ceilings, assigns
  their lift paths and defines the multi-Workspace concurrency proof.
- [Architecture and complexity research](ARCHITECTURE_COMPLEXITY_RESEARCH.md)
  maps the earlier pinned private file tree, payload backing, canonical Commit
  result and daemon ownership to asymptotic costs and proof gates. Its companion
  studies cover the [private backing and page tree](PRIVATE_BACKING_AND_PAGE_TREE.md)
  and [final-delta Commit](FINAL_DELTA_COMMIT_COMPLEXITY.md). The
  [animated Commit timeline](assets/incremental-commit-private-backing.gif)
  follows the shell, live Workspace, private backing, frozen root and Store.
- [Implementation plan](IMPLEMENTATION_PLAN.md) sequences root-cause repair,
  local index work, Commit transport, concurrency proof and prospective gates.
- [Phase 1 verification](VERIFICATION_PHASE1.md) distinguishes the retained
  #232/#243 evidence from future one-attempt ordinary-shell checks.
- [Load-bearing case brainstorm](LOAD_BEARING_CASES.md) proposes later
  packages, large files, namespace operations and output/log scenarios without
  claiming they are already registered or passing.
- [Phase 1 implementation handoff prompt](HANDOFF_PHASE1_PROMPT.md) gives a
  new agent the reading order, before/after architecture, provisional file
  ownership and LOC range, complexity model, ordinary FUSE route, and
  verification gates.
- [Phase 1B implementation handoff](HANDOFF_PHASE1B_IMPLEMENTATION.md) carries
  that assignment forward from the completed repair round: what Phase 1A
  already proved and must not be redone, the exact code sites of every remaining
  gap, the work packages and their stop conditions, the operational playbook for
  mounted diagnostics, and the reporting requirements.
- [D/E/F finish handoff](HANDOFF_D_E_F.md) is the prior assignment: the four
  gating design decisions are recorded and owner-approved, and its §8 remains
  binding; everything it marks done stays done.
- [E/F continuation handoff](HANDOFF_E_F_CONTINUATION.md) is the dated
  assignment that specified E/F gates and stop conditions before their final
  outcome was recorded in the report below.

## Checkpoint (2026-09-26)

| Item | Status | Evidence |
| --- | --- | --- |
| Phase 1A root-cause repair | ✅ closed | [phase1-ordinary-shell-repair](evidence/phase1-ordinary-shell-repair/REPORT.md) |
| Phase 1B packages A–C (format, splice, cursor) | ✅ closed | [phase1b-extent-sequence](evidence/phase1b-extent-sequence/REPORT.md) |
| Mounted correctness of A–C (8 defects repaired) | ✅ closed this round | [phase1b-mounted-write-repair](evidence/phase1b-mounted-write-repair/REPORT.md); `76c832f0c`, `fdfc41032` |
| Evidence blocker 1 (retained prepared state) | ✅ closed | `issue245-shell-package-v3-prepared-01` |
| F pre-optimization control arm | ✅ collected and qualifying (4/4 functional PASS) | `issue245-shell-package-v3-control-01` |
| ~5.5 s cleanup diagnosis | ✅ explained and fixed | cleanup ~0.5 s in every passing row |
| Evidence blocker 2 (route harness) | ✅ closed this round | eight route cases PASS on the final source, receipts in `benchmark-results/fs-bench-pro/issue245-route-harness/*-f/` |
| Package D (streaming transport) | ✅ closed this round | `036847824` (+342 LOC); descriptors as body-stream prefix, bounded replacement replay spool; earlier 256-edit/8 MiB ceilings retired, 4,096 final-run ceiling remains |
| Package E (generations + reconcile) | ✅ final functional gates PASS | strict mounted write accepted while a real successor-builder page read was held at `b2cd0df23`; [E/F final report](evidence/phase1-e-f-final/REPORT.md) |
| Package F target freeze | ✅ frozen this round | [phase1-f-target/TARGET.md](evidence/phase1-f-target/TARGET.md), `bf9c3f5e0` |
| Package F candidate arm | ✅ frozen comparative wall envelope PASS; latency INELIGIBLE | final source `b2cd0df23`; all four functional/cleanup/verifier cells PASS in [E/F final report](evidence/phase1-e-f-final/REPORT.md) |
| Three latent extent-sequence defects | ✅ fixed this round | `0d4834f81` (over-ceiling implicit base), `036847824` (O(pages) descend, post-insertion `Io`) |
| 4,097-run public scaling | ⬜ open after #252 | #252 retired the explicit 4,096 admission check; [#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) owns the full SDK/FUSE speed, structural and Commit proof |
| Many-file namespace scaling | ⬜ open | [#256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256) owns 128 dirty identities/names, prepared streaming and adjacent count limits |
| Multiple simultaneous SDK Exec calls and mounts | ⬜ open after #248 | [#249](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249) owns multi-Workspace daemon/session concurrency, one Commit per Workspace and removal of the whole-Exec timer |

The four decisions for D/E/F and their original handoff remain in
[HANDOFF_D_E_F.md §8](HANDOFF_D_E_F.md). D, E and the frozen F comparative
selection are complete at the stated scope. The [final report](evidence/phase1-e-f-final/REPORT.md)
records E's strict overlap gate at `b2cd0df23` and the cache-ineligible F
latency cells. [#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248) and
[#256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256) are the two
scale follow-ups; [#249](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249)
follows #248 for daemon concurrency. The older continuation handoffs remain dated assignments,
not current status reports.

The first repair round is recorded in
[Phase 1 ordinary-shell repair](evidence/phase1-ordinary-shell-repair/REPORT.md):
three defects on the shared namespace path were localized from a labelled
mounted diagnostic, fixed, and re-verified. The mixed package refresh and the
two rename shapes now pass with an independent full-tree oracle, the four #243
correctness oracles still hold, and every latency row stays **INELIGIBLE**. The
piece index, streaming Commit and continuously writable Commit claims in that
**earlier repair report** were NOT_RUN then. Later outcomes are recorded above;
this folder still holds no release candidate or cold-cache latency admission.

The retained [#243 Phase 2 report](../243/evidence/phase2-ordinary-shell-v1/REPORT.md)
records a mixed-refresh functional FAIL and three functional observations with
cache-ineligible latency. The [#232 POSIX count diagnostic](../232/evidence/posix-count-diagnostic/REPORT.md)
shows repeated full-piece-list work. Historical receipts remain unchanged.
The prior [ioctl campaign](../232/evidence/phase2-all-ioctl/REPORT.md) is
opt-in evidence and does not qualify the arbitrary-shell route.
