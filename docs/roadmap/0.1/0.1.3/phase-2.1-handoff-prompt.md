# Phase 2.1 implementation handoff prompt

```text
Implement and drive LayerFS Phase 2.1 issue #40 to one of its two defined terminal outcomes. Continue through implementation failures, performance misses and verification failures by diagnosing the measured cause and replanning. Do not stop at a plan, partial refactor, compile success, one favorable timing, or the first failed experiment.

AUTHORITATIVE SCOPE

- GitHub issue: https://github.com/Ephemeral-AI-Lab/layerfs/issues/40
- Local specification: docs/roadmap/0.1/0.1.3/phase-2.1-shared-construction-staging-spec.md
- Dependencies: #40 is a prerequisite of #38 and #39 and a child of #21.
- Read the complete specification before editing. Its latest explicit user scope overrides conflicting older planning language.
- Read docs/roadmap/0.1/0.1.1/architecture_shift.md and namespace-optimization-spec.md for the proved v0.1.1 mechanisms and rejected experiments.
- Read docs/roadmap/0.1/0.1.3/phase-1-verification-withdrawal.md before designing verification. Its failure record is a constraint: do not recreate the withdrawn exhaustive suite.
- The user's 2026-09-05 instruction explicitly authorizes the selective Phase 2.1 verification companion below. It does not restart Phase 1 verification; it supersedes the old “no further verification run” boundary only for these new, bounded Phase 2.1 checks.
- Inspect task 01a06b53-f424-7080-98a6-c294549114e0 and the isolated experiment checkout /Users/yifanxu/Ephemeral-AI-Lab/layerfs-workspace-commit-engine. Reuse selected code only after pinning its source and reconciling it with the current functional repairs.
- Read /Users/yifanxu/Ephemeral-AI-Lab/layerfs-bulk-create-feasibility/investigations/bulk-create/no-rollback-streaming-amendment.md for the authorized recovery limits.

WORKTREE AND OWNERSHIP

Create or use one dedicated Codex worktree on a codex/ branch from the actual current v0.1.3 implementation state. Record the base, experiment revision and uncommitted prototype dependencies before editing. Preserve all unrelated work; never reset, clean, restore or overwrite another agent's changes. Local implementation commits are authorized. Do not merge, push, open a pull request or publish a release unless the user separately authorizes it.

Use subagents for bounded, nonoverlapping review/implementation responsibilities inside #40 only: canonical tree/content construction, Store schema/admission/staging, Workspace lifecycle/concurrency, and namespace performance/resource audit. Assign file ownership and remind every worker that others share the codebase. Serialize builds and performance measurements; do not run competing resource-sensitive commands. The primary agent owns integration, measurements, issue updates and terminal truth.

REQUIRED OUTCOME

Deliver the namespace construction/admission refactor, exactly the three-column workspace_stages foundation, short publication ownership, selective automatic verification, and namespace/input-shape results. Do not implement, run, qualify or plan the #38/#39 family fixes in this task. Use at most eight CPU cores in aggregate across participating owner processes and reduce matched total CPU work rather than buying wall time with more cores.

The namespace target is 100,000 files  / 500,000,000 logical bytes in no more than 2.2 seconds median, with median product CPU no more than 11 CPU-seconds and no more than 90% of the matched baseline median. Preserve the other resource, workload and timing boundaries in #40.

FIRST DECISION: APPROACH A OR B

Pin one matched baseline and run at most two bounded, counter-driven namespace experiments. Test whether approximately 0.566 seconds of complete wall and the required CPU reduction plausibly come from removable work under the current canonical and Store contracts. No broad worker, queue, cache or SQL parameter sweep.

Approach A — optimization is plausible:
1. Optimize the existing namespace initializer first, one evidenced mechanism at a time.
2. Require the predicted work counter and total CPU to fall.
3. Stabilize the useful mechanism, then extract it into the shared components.
4. Re-run the thin initializer adapter and exercise namespace input-shape controls plus the minimal Workspace-staging lifecycle. Do not run CAS/CDC benchmark families.
5. Claim the target only after all three prescribed matched samples pass.

Approach B — optimization is not plausible:
1. Stop specialized namespace tuning after the bounded evidence shows required work/limits dominate or the two concrete hypotheses fail.
2. Record the 2.2-second target as MISS with the measured constraint and retained attempts.
3. Still extract the proved v0.1.1 mechanisms and implement the minimal staging foundation. Publish narrow reuse boundaries without integrating #38/#39 callers.
4. Preserve the retained initializer performance/CPU profile; do not invent a benchmark-specific route or begin later family work.
5. Never relabel the refactor as a 2.2-second performance PASS.

Record the chosen approach and evidence in issue #40. A failed Approach A experiment may revise the hypothesis or move to Approach B; it is not terminal until the common deliverables below pass.

IMPLEMENTATION CHECKPOINTS

1. Inventory every caller before moving a shared function. Keep existing wrappers compiling while each caller transfers.
2. Reuse/extract checked object insertion. Preserve exact conflict authentication; caller owns transaction and publication policy.
3. Consolidate the exact eight-entry, operation-scoped metadata-result cache beside the canonical builder. No persistent/global cache.
4. Reuse owned bounded slabs and carried admission batches. Never flush merely because a file, directory, task or worker ends.
5. Integrate the sorted affected-page directory/inode updater. Preserve unchanged child identities and sparse-edit locality. Establish same-seed canonical parity at split/balance boundaries before replacing initializer construction.
6. Broaden native discovery using namespace-focused root-file-plus-directory and single-large-directory controls so they reach the efficient path without changing fixtures. Harmonize live production workers at no more than eight; the test-only ten-worker helper is not a runtime optimization.
7. Add schema v5 and only workspace_stages(workspace_id, branch_id, root_id). Add no status, generation, timestamp, conflict, receipt, lock or per-file tables.
8. Implement stage -> conditional Commit/Branch publication -> stage deletion. Insert a stage only after complete selected-root validation. No-op also stages and retires without creating history.
9. Remove lifetime Store ownership around construction/worker waits. Preserve bounded transactional serialization and fairness. Remove lifetime Branch exclusivity only with Workspace isolation; continuing sessions install the exact returned Commit/root rather than rereading the latest Branch head.
10. Preserve Commit-and-continue for history/endurance. Skip continuing-view installation only on an explicit commit-and-close path. Report publication success plus later cleanup failure accurately.
11. Delete duplicated or superseded code only after its supported input domain transfers. Keep fallback behavior until then. Do not add a new crate, universal executor, public bulk API, backend plugin system or operation-family dispatch.

SELECTIVE AUTOMATIC VERIFICATION

Create exactly one executable companion: benchmark/fs-bench-pro/verify-selected.py. It lives beside the existing family performance scripts and reuses workspace-runner.py, run-namespace.sh, workspace_verify.rs, dedup_verify.rs and existing independent family oracles. Do not create one verification script per family or copy oracle logic into the wrapper.

The script accepts exactly one explicit family/case/seed/source/input selection. It rejects --all, ranges, implicit expansion and missing identities. It produces one immutable verification.json with source/harness/product/environment/input identities, checks, sampled paths/ranges, reused proof identities, omissions, cleanup, monotonic wall, evidence path and distinct PASS/FAIL/TIMEOUT/INCOMPLETE status.

Every verification invocation must finish in strictly less than one minute. Enforce a hard 59-second end-to-end limit beginning before setup/authentication and ending after cleanup and receipt publication. Reserve time for cleanup. At or beyond 59 seconds the invocation cannot PASS; retain TIMEOUT/INCOMPLETE evidence and return nonzero. Do not automatically retry unchanged work.

“Parallel to the benchmark family script” means a separate companion entry point, not simultaneous execution. Never run verification concurrently with performance because it would contaminate CPU/I/O measurements.

Invoke the companion automatically:
- after the first successful selected benchmark on a changed construction/staging/publication route;
- after correcting an actual verification failure; and
- once per materially distinct selected route on the final stable candidate.

Reuse an exact identity-matched verification PASS. Do not rerun it after repeated performance samples or unrelated changes. Verification failure triggers diagnosis and the smallest corrected check, not a larger suite. Fix functional failures before scaling out performance work. Use bounded representative content/range checks and existing structural/CAS/CDC receipts; do not perform per-sample full-file verification or replay every history Commit.

FAST ITERATION

For each hypothesis:
1. State the predicted counter and affected phase.
2. Make the smallest change in the shared cause.
3. Run the smallest relevant build/check.
4. Run one representative benchmark sample only when needed.
5. Run verify-selected.py only when its invalidation rules require it.
6. Record the result and continue, revise or select Approach B.

Never rerun repeatedly passing tests, fixtures, performance samples or verification. Reuse prepared inputs and source-compatible evidence. Keep preparation, verification and reporting outside product timing while still reporting their real wall. Do not run a full workspace test suite during ordinary iteration. Broaden checks once on the stable affected path or after a concrete failure requires it.

Final stable-candidate verification remains at most 60 seconds per family and 600 seconds total. Select at most one final companion invocation per family. A timeout or omitted check remains explicit and cannot PASS.

ISSUE #40 UPDATES

Update issue #40 only at meaningful checkpoints:
- pinned implementation/experiment/evidence identities;
- Approach A/B decision and its evidence;
- each shared component integrated through its real callers;
- staging/schema/ownership path working;
- first source-bound namespace and transferred-family measurements;
- terminal outcome and short deferred applicability note for later user discussion.

Do not post routine command logs. Every update distinguishes measured fact, hypothesis, failure and deferred work. Link immutable evidence paths or commits. Preserve all failed and superseded attempts under their actual identities.

TERMINAL OUTCOMES

APPROACH_A_TERMINAL_PASS requires:
- the complete shared refactor and staging/ownership/verification deliverables;
- three matched namespace-100000 samples with median <=2.2 seconds;
- CPU median <=11 CPU-s and <=90% of matched baseline median, candidate maximum <=matched baseline maximum;
- resource, canonical, staging, publication, cleanup and bounded verification checks passing;
- namespace input-shape controls and the minimal staging lifecycle showing the intended path changes; and
- a short deferred applicability note, without executing or planning #38/#39.

APPROACH_B_REFACTOR_TERMINAL_PASS requires:
- a source-bound report explaining why the 2.2-second/lower-CPU target is not plausible under the bounded experiments and current contracts;
- the complete shared refactor and staging/ownership/verification deliverables;
- retained namespace performance/CPU without an unexplained reproducible regression;
- namespace input-shape controls and the minimal staging lifecycle exercising the shared path with truthful results; and
- a short deferred applicability note, without executing or planning #38/#39. The namespace target remains MISS and no family is called passed from code reuse alone.

Do not stop until one terminal definition is actually satisfied. Compilation alone, one sample, an unresolved correctness/resource failure, a missing selective verifier, or a plan for later is not terminal. If one implementation attempt fails, preserve it, diagnose it and replan. Ask the user only for an actual external decision or unavailable resource that cannot be resolved from the repository and existing authorization.

At terminal, update issue #40 with the chosen outcome, every measured namespace sample, CPU/memory/work counters, verification receipts, source identities, remaining risks and narrow reusable interfaces. Stop and return to the user. Do not execute, plan or close #38/#39; they will be discussed separately after #40. Do not claim release readiness.
```
