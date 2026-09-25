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
package workloads can be claimed. No document here is a product
implementation, performance PASS or release contract.

The target keeps index pages and payloads in local private backing. A Commit
pins one immutable generation while the same FUSE mount accepts later writes
into the next; a second sequential Commit should do the same after the first
is reconciled. Current source has generation capture, but successful
reconciliation under overlapping writes remains unproven and is an explicit
verification gate.

- [Scalable range-based COW architecture](ARCHITECTURE.md) describes current
  and proposed data paths, immutable generation capture, streaming Commit,
  resource limits and unresolved concurrency invariants.
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
- [D/E/F finish handoff](HANDOFF_D_E_F.md) is the current assignment: the
  mounted write-path repair and the frozen control are done, the four gating
  design decisions are recorded and owner-approved, and the remaining packages
  (streaming transport, generations, the route harness, the candidate arm) are
  specified with their gates and stop conditions.

## Checkpoint (2026-09-26)

| Item | Status | Evidence |
| --- | --- | --- |
| Phase 1A root-cause repair | ✅ closed | [phase1-ordinary-shell-repair](evidence/phase1-ordinary-shell-repair/REPORT.md) |
| Phase 1B packages A–C (format, splice, cursor) | ✅ closed | [phase1b-extent-sequence](evidence/phase1b-extent-sequence/REPORT.md) |
| Mounted correctness of A–C (8 defects repaired) | ✅ closed this round | [phase1b-mounted-write-repair](evidence/phase1b-mounted-write-repair/REPORT.md); `76c832f0c`, `fdfc41032` |
| Evidence blocker 1 (retained prepared state) | ✅ closed | `issue245-shell-package-v3-prepared-01` |
| F pre-optimization control arm | ✅ collected and qualifying (4/4 functional PASS) | `issue245-shell-package-v3-control-01` |
| ~5.5 s cleanup diagnosis | ✅ explained and fixed | cleanup ~0.5 s in every passing row |
| Evidence blocker 2 (route harness) | ⬜ not started | chain mapped in the finish handoff §4.1 |
| Package D (streaming transport) | ⬜ not started; design decided | finish handoff §3, §8 D-1 |
| Package E (generations + reconcile) | ⬜ not started; fix shape decided | finish handoff §3, §8 E-1 |
| Package F candidate arm + target freeze | ⬜ not started; target decided | finish handoff §4.0, §8 F-1 |

The four decisions that gate the remaining work (D's frame protocol, E's
reconcile shape, the harness route, F's comparative target) were made with the
owner on 2026-09-26 and are binding for the finish round — see
[HANDOFF_D_E_F.md §8](HANDOFF_D_E_F.md). Roughly half the issue by remaining
effort is complete: the algorithmic foundation is real and proven on the
mounted route; what remains is the streaming wire (D), continuous generations
(E) and the before/after proof (F's candidate).

The first repair round is recorded in
[Phase 1 ordinary-shell repair](evidence/phase1-ordinary-shell-repair/REPORT.md):
three defects on the shared namespace path were localized from a labelled
mounted diagnostic, fixed, and re-verified. The mixed package refresh and the
two rename shapes now pass with an independent full-tree oracle, the four #243
correctness oracles still hold, and every latency row stays **INELIGIBLE**. The
piece index, the streaming Commit, the continuously writable Commit proof and
the frozen Phase 1 performance selection remain **NOT_RUN**; this folder still
holds no release candidate and no admission result.

The retained [#243 Phase 2 report](../243/evidence/phase2-ordinary-shell-v1/REPORT.md)
records a mixed-refresh functional FAIL and three functional observations with
cache-ineligible latency. The [#232 POSIX count diagnostic](../232/evidence/posix-count-diagnostic/REPORT.md)
shows repeated full-piece-list work. Historical receipts remain unchanged.
The prior [ioctl campaign](../232/evidence/phase2-all-ioctl/REPORT.md) is
opt-in evidence and does not qualify the arbitrary-shell route.
