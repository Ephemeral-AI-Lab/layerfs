# v0.1.6: sandbox-local snapshots and the retained-history roadmap

> **Released, 2026-09-16:** `v0.1.6` is published as a source-only Developer
> Preview — [release record](../../../../release-notes/0.1.6/README.md),
> [release and downloads](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.6),
> [acceptance](../../../../release-notes/0.1.6/acceptance.md) and
> [every measured selection](../../../../release-notes/0.1.6/benchmark-closeout.md).
> Everything below is the planning and evidence trail behind that release.

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
| [#152 final report](evidence/issue152-final-report.md) | [#152](https://github.com/Ephemeral-AI-Lab/layerfs/issues/152): the 8-group campaign's per-family table (196 collected cells, candidate vs v0.1.5 comparator), its three candidate identities, and every `FAIL`/`NOT_RUN` row — the authoritative result set for the sandbox-local line |
| [#152 reliability fix report](evidence/issue152-reliability-fix-report.md) | Outcome of the six `workspace_reliability` failures handed over by [#152](https://github.com/Ephemeral-AI-Lab/layerfs/issues/152): per-case fix, identity chain, 27/27 proofs, non-passing lines |
| [#154 rolling evidence](evidence/issue154/README.md) | Which of the `#154` files is the current seed-1 matrix and which are superseded phase reports, with the identity each was taken on |
| [Retained-history report](evidence/issue153-retained-history-report.md) | [#153](https://github.com/Ephemeral-AI-Lab/layerfs/issues/153): the 157-commit `deepseek-full` and `stride-3`/`stride-10` profiles re-run on the v0.1.6 candidate with a paired v0.1.5 control |

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
- **B3** (`local-snapshot-create-25000-onebyte-v1`, one 25k lifecycle with three
  Commits) is collected and verified on both arms. Both 25k absolute gates pass —
  transient backing 25,000 B against the 32 MiB ceiling, and the candidate's
  peak-sum memory *below* the control's (130.2 MB vs 134.6 MB) — and both separate
  verifications PASS with per-Commit C1/C2/C3 canonical checks, a sampled native
  reopen and cleanup. Five time/CPU lines miss the strict +15% allowance on the
  final pair (+1.7% to +36% over control) while an earlier pair on the same
  products passed all of them, so those lines sit inside the host-state spread
  rather than inside a product regression; all are within the owner's one-worker
  tolerance. Reaching B3 required three harness repairs (root-mode expectation,
  per-Commit evidence collision, missing bounded native recipe), all on the
  verification path and none in either product — ledger L19.

See the ledger (L18–L20 in particular) for exact identities, the memory timelines
and per-phase arithmetic.

**Repository checks.** GitHub Actions is disabled for this repository by owner
decision (ledger L21) and `.github/workflows/ci.yml` is removed. The former CI steps
plus the benchmark harness tests now run from `tools/preflight.sh`, which is the
pre-push gate; "CI green" claims in earlier ledger entries are historical.

**Next campaign.** The full existing suite is tracked in
[#152](https://github.com/Ephemeral-AI-Lab/layerfs/issues/152): 8 family groups in the
same phase order the v0.1.5 finalization campaign used, one sample per case per arm, a
cell accepted when it is < 50 % worse than its v0.1.5 comparator or under 10 ms absolute
difference, and verification defects fixed fast-path in the harness (the three classes
L19 records) without touching either product. The B1/B2/B3 receipts above are reused by
citation rather than re-collected.

**Status after L20.** The owner accepted the measured one-worker cost, and the
three gates are recorded as accepted: B1 with one informational container-CPU
line over its sub-gate, B2 with its Commit/CPU-sum deltas inside the accepted
tolerance and its sandbox-memory line recorded as not measurable on this harness,
and B3 with both absolute 25k gates and both separate verifications passing. The
adoption recommendation is to take this direction for v0.1.6 at one construction
worker, with the limitations listed in L20 attached — sandbox *process* memory is
not emitted by the frozen harness, the time comparison is cache-stance and
host-load sensitive beyond the 15% allowance, dirty shared-mmap visibility stays
unsolved, and the breadth families were not run. This is not a release decision:
no merge, issue closure or tag follows from it.

## #152 follow-up: closed

The #152 campaign closed with every registered selection terminal and handed
over one work item: the six `workspace_reliability` fault-injection proofs. It is
**complete** (commit `ac729dfeb`, ledger
[`L30`](evidence/issue151-experiment-ledger.md)): five were instrumentation still
pointing at the pre-v0.1.6 host-owned routes, and
`workspace-final-publication-failure-retry` was a real divergence — the sandbox
route refused the retry the materialized route performs, so recovery from a
failed final publication was Discard-only there. The sandbox route now re-drives
a retained publication attempt, and **27/27 verification-supported
`workspace_reliability` proofs PASS** on the frozen candidate. Report:
[`issue152-reliability-fix-report.md`](evidence/issue152-reliability-fix-report.md);
the original work specification stays in
[`issue152-reliability-fix-handoff.md`](issue152-reliability-fix-handoff.md).

Two rows of that campaign's result table stay non-passing by owner decision and
are **not** the v0.1.6 registered benchmark requirement: the six single-worker
material regressions and the waived 2.7 s cold `namespace-100000` Init are
accepted as recorded, and the inherited v0.1.5-era `historical_access` artifact
(11 performance cases + 11 proofs) remains `NOT_RUN` because its sealed v2 Store
(`store_sha256 f323de0e…`) was removed before the campaign. The six
`historical_access` additions that v0.1.6 actually registers are a separate,
new implementation with their own sealed producers; they are measured, verified
and reported in [#154](https://github.com/Ephemeral-AI-Lab/layerfs/issues/154).
