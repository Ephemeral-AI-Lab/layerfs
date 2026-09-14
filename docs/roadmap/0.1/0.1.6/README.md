# v0.1.6: sandbox-local snapshot experiment (worktree view)

> **Current replacement experiment, 2026-09-15:** the owner-directed
> [sandbox-local snapshot specification and implementation plan](sandbox-local-snapshot-spec-and-plan.md)
> starts from v0.1.5, with local mutable ownership, no pausing/quiescing,
> one Commit worker, one sample per case/arm, and hard v0.1.5 comparison gates
> for create-500, then bulk-create-500, then 25,000 files.

This directory on the `codex/v016-sandbox-local-experiment` branch contains only
the frozen planning documents for the scoped experiment; the broader historical
v0.1.6 overlay documents live on `main` and are not part of this execution.

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
`6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`. The experiment does not
automatically merge main, close #149/#150, or release v0.1.6.
