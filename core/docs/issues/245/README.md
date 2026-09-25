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
