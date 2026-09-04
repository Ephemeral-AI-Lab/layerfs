# Phase 1 completion amendment — 2026-09-04

The user clarified that functional failures must be fixed in Phase 1. This
amendment supersedes earlier completion and deferral language in the handoff,
testing rules, execution contract and issue snapshots. It changes the completion
policy, not frozen workload definitions, measurement protocols or historical
evidence identities.

Phase 1 includes the minimum shared-root-cause product repairs needed for the
declared workloads to complete correctly. Unexpected operation errors, capacity
failures within those workloads, correctness/integrity defects, cleanup defects,
and failed mandatory resource or safety-deadline gates block completion. Classify
and fix harness defects too. Expected errors pass only under their exact frozen
oracle; unsupported guarantees outside the declared contract do not become new
features through this amendment.

Keep affected family issues and #35 open until the required corrected execution
and independent verification pass. `PHASE1_TERMINAL_PASS` cannot coexist with an
unresolved failure of a required functional, correctness, resource or cleanup
gate. A complete inventory of failing baseline outcomes is progress, not terminal
pass. Central issue #21 remains open for later phases and release qualification.

Preserve every original failure, reproduction and source identity. Give product
repairs their own source-bound evidence; report original baseline and corrected
candidate separately. Reuse unaffected evidence only with explicit compatibility
and invalidation reasoning, never by relabeling an old run as a new-source run.
A completion-policy edit alone does not change workload semantics or invalidate
a valid timing; preserve its original contract identity.

Fix the shared cause, run the smallest affected diagnostic, and recollect only
invalidated coverage. Concrete correctness failures justify focused early
verification. Keep the complete verification campaign separate and late, after
performance collection is complete and the implementation is stable; retain
unaffected passes instead of repeatedly rerunning them. REVISE and NO_GO require
replanning and continued repair until the required gates pass.

Do not shrink fixtures, omit cases, weaken oracles, raise declared budgets or
timeouts, or add benchmark-only product bypasses to obtain a pass. Required
repairs may improve performance incidentally. Optional latency and storage
optimization of correct operations remains Phase 2; slow results that satisfy
all mandatory gates remain honest baseline findings.

The first known repair dependencies are the structural namespace final-delta
limit, deferred piece allocation limit, and proxy directory entry count limit
documented in `findings/`. Track each shared cause and its affected operations;
do not create a separate product fix for every failing sample.
