# v0.1.6: sandbox-local snapshots and the retained-history roadmap

> **Current replacement experiment, 2026-09-15:** the owner-directed
> [sandbox-local snapshot specification and implementation plan](sandbox-local-snapshot-spec-and-plan.md)
> starts from v0.1.5, with local mutable ownership, no pausing/quiescing,
> one Commit worker, one sample per case/arm, and hard v0.1.5 comparison gates
> for create-500, then bulk-create-500, then 25,000 files.

This directory holds the frozen planning documents for the sandbox-local
snapshot experiment that is now the v0.1.6 direction on `main`, together with the
retained-history work they supersede where the two conflict. The scoped
experiment starts from v0.1.5 source; the earlier host-overlay implementation
whose measurements motivated the replacement is preserved on
`archive/v016-overlay-7b73c4b33` and is no longer on `main`.

| Document | Purpose |
| --- | --- |
| [Experimental implementation pipeline](experimental-implementation-pipeline.md) | [#151](https://github.com/Ephemeral-AI-Lab/layerfs/issues/151): one execution track from v0.1.5 through local ownership, snapshot/transport/publication proofs, create-500, bulk-create-500 and 25k gates |
| [Sandbox-local snapshot spec and plan](sandbox-local-snapshot-spec-and-plan.md) | Current replacement experiment: architecture diagrams, owner rules, resource limits, one-sample performance gates, and staged rebuild from v0.1.5 |
| [Reviewed sandbox/host connection architecture](sandbox-host-connection-architecture.md) | [#150](https://github.com/Ephemeral-AI-Lab/layerfs/issues/150): explicit ASCII before/after diagrams, removal list, minimum remaining messages, service fairness and atomic publication boundaries |
| [Connection review record](sandbox-host-connection-review.md) | Three independent subagent reviews, ranked findings, source evidence and dispositions; no implementation/performance claim |
| [Experimental agent handoff prompt](experimental-agent-handoff-prompt.md) | The exact handoff instructions followed by the implementing agent |

Execution evidence: [issue151 experiment ledger](evidence/issue151-experiment-ledger.md)
(append-only). Research motivation:
[why the host-authority route became expensive](../../../research/v016-sandbox-snapshot-review-2026-09-15.md).

Source base: peeled `v0.1.5^{commit}` =
`6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`. Promotion to `main` (2026-09-15,
owner-directed) selected this implementation as the v0.1.6 direction; it did not
close #149/#150, cut a release tag, or claim the remaining gates:

- **B1** (`tiny-create-500-mixed-v4`) was recorded as a FAIL on the
  complete-Commit phase in the first pair. The owner ruled that the one-worker
  direction is expected to cost time and that B1 is therefore not an issue; on
  the current revision B1's commit phase passes its gate anyway and only the
  container-only CPU sub-gate misses (+4.9%).
- **B2** (`tiny-bulk-create-500-mixed-v3`) memory amplification was ruled
  unacceptable, and the sandbox spool now keeps a bounded resident window instead
  of holding the payload in page cache: measured sandbox residency fell from
  ~500 MiB to **<= 2.6 MiB**, and no measured phase is credited by cache warmth
  (a transfer that used to read its own recent writes at 19 GB/s now reads storage
  at 2.1 GiB/s). That honest transfer is what the sandbox-owned model costs: B2's
  complete Commit and declared CPU sum now fail (+21.4% and +6.6%) against limits
  derived from a control whose equivalent read is cache-served, while the whole
  workflow still passes with 5.1% margin.
- **B2's memory verdict is still unresolved**, but for a measurement reason that
  is now evidenced: six identical post-fix runs produced container *lifetime*
  peaks of 24.6-189.8 MB while the product's own residency stayed ~2 MB, so the
  frozen harness's only symmetric sandbox number cannot decide the gate.
- **B3** (25,000 one-byte files) has not been run; the pipeline's strict order
  still blocks it behind B2.

See the ledger (L18 in particular) for exact identities, the memory timelines and
per-phase arithmetic.
