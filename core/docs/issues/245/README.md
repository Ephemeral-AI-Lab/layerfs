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

The retained [#243 Phase 2 report](../243/evidence/phase2-ordinary-shell-v1/REPORT.md)
records a mixed-refresh functional FAIL and three functional observations with
cache-ineligible latency. The [#232 POSIX count diagnostic](../232/evidence/posix-count-diagnostic/REPORT.md)
shows repeated full-piece-list work. Historical receipts remain unchanged.
The prior [ioctl campaign](../232/evidence/phase2-all-ioctl/REPORT.md) is
opt-in evidence and does not qualify the arbitrary-shell route.
