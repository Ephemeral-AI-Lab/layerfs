# Continuation prompts: D, pooling coverage and final acceptance

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Current continuation of [#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168)
and [#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169), after pooling was
implemented and the reference oracle was added. This routing document supersedes
the earlier E-first assignment and any older copy embedded in GitHub. Full
acceptance criteria in the [original handoff](stages-3-4-handoff.md) remain.

## Dispatch these three assignments

1. **[D: correct the oracle and finish stored-tree edits](stages-3-4-continue-d.md).**
   Start here in a single-agent run. Align the compared edit coordinates before
   diagnosing batch-normalized as a product mismatch, then implement D fully.
2. **[Pooling: boundary coverage and independent qualification](stages-3-4-continue-pooling.md).**
   Pooling now exists. Finish coverage and valid component evidence without
   waiting for D. Do not rebuild E or shrink real limits.
3. **[Final acceptance: integrate, qualify and review](stages-3-4-continue-acceptance.md).**
   Run after the two assignments deliver their required code/proofs. Preparation
   may start earlier; combined edit qualification waits for exact correctness.

```text
current implementation
        |
        +--> D: identical inputs -> stored-tree algorithm -> exact proof --+
        |                                                                 |
        +--> pooling: real boundaries -> independent evidence -------------+--> final acceptance
```

These documents are prompts, not dispatched agents or results. If separate workers
are explicitly assigned, D owns C1 file algorithms/oracle and pooling owns C2 pool
coverage/implementation. They are not alone: preserve each other's changes. One
integration owner handles shared API/policy/schema/manifests, shared report edits
and Git commits. Never stage another worker's unfinished changes. Resource-sensitive
builds/measurements obey the lock and must not overlap.

## Shared instructions for every assignment

Work in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`.
Read repository/core AGENTS.md, your assigned prompt, the
[completion report](stages-3-4-completion-report.md), the
[original handoff](stages-3-4-handoff.md), [file plan](stages-3-4-file-plan.md)
and relevant [completion details](stages-3-4-completion-handoff.md).
Use the [reviewer contract](stages-3-4-reviewer-handoff.md) for acceptance/limits.
Later owner directions and this routing order supersede stale scheduling statements,
not correctness/resource requirements.

Planning source: `2b2dbc028`. Resolve the actual start commit/tree and relevant
dirty/untracked inputs. Carry reviewed uncommitted instructions into another
checkout if needed; clean main may not contain this prompt. Preserve earlier
reports/receipts and unrelated work. The reference is
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`; never link its implementation into
candidate production or retain it as a runtime fallback.

No retry, error-driven fallback, fsync/fdatasync/sync_all/sync_data, WAL,
third-party patches/forks/vendoring, hidden payload staging or successful-version
rollback. Keep C1/C2 independent of Workspace/FUSE/daemon/history. Preserve
publication visibility, exact authentication and same-save regression fixes.
Use the existing injected timer, with real standalone/integrated bodies, bounded
reports, honest read/wait attribution and identical disabled behavior.

Production files <=999 physical lines; lib.rs/mod.rs <=200 with declarations/
reexports/direct delegation only. Tests/fixtures/examples stay external. No fake
clock/hash injection/fault hook/test-only public API in production. Existing real
public APIs may be exercised directly. Do not add frameworks or new dependencies
when existing code/stdlib/pins suffice.

Before any benchmark work, follow repository benchmark rules and create the
required committed versioned case addendum: exact operations, identities, cache/
index state, sample/worker rules, acknowledgement, gates and budgets. Reuse setup/
builds honestly; never reuse warm measured work. One sample per case/arm unless
explicitly authorized otherwise; fresh append-only evidence, no best-of or raised
limits. Stage 6 does not absorb #168/#169's required evidence.

No CI or aggregate preflight: tools/preflight.sh is retired and must not be run,
restored or replaced with an aggregate wrapper. Run explicit affected-workspace
checks, retain failing cases and report exact commands/counts/gaps. Per commit,
use the same audited production counter against first parent and final committed
tree, include SQL, exclude tests/docs/tools and report signed change plus migration
subtotals. Original file-plan ranges are final-size guidance, not new LOC quotas.

#166/#167 remain owner-closed; their residual inventory stays in #174. Address a
residual when it affects the current assignment's own claim; do not restart that
closed scope or borrow overstated timer attribution as new evidence.

## Delivery rules

Finish your assigned behavior/proof, not merely a chosen number of test targets.
Record a task-specific additive report/evidence directory; do not concurrently
rewrite a shared completion report. Final acceptance combines them. If an external
run limit interrupts execution, leave source identity, completed work, exact
remaining case/files and next action, then resume the same assignment. A run
limit is not a technical blocker and does not change completion criteria.
