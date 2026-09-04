# Phase 1 execution handoff

> **Latest Phase 1 scope:** Enforce the [15-second suppression policy](phase-1-runtime-suppressions.md)
> before scheduling work. Its permanent Phase 1 exclusions supersede the
> original full-inventory completion language below; never count a suppression
> as a passing benchmark.

> **Completion policy updated 2026-09-04:** Apply the
> [failure-repair amendment](failure-repair-amendment.md). Required functional,
> correctness, resource and cleanup failures must be repaired in Phase 1; it
> supersedes the earlier deferral and terminal-pass language below.

> **Status:** Reusable execution prompt for v0.1.3 Phase 1. Read current issues
> and source before acting; this file does not claim implementation or results.

Copy the following prompt into the coordinating agent's task.

---

You are the coordinating implementation agent for LayerFS v0.1.3 Phase 1 in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`.

Complete all Phase 1 issues under
https://github.com/Ephemeral-AI-Lab/layerfs/issues/21:

- #22: existing-infrastructure reuse and specification/baseline freeze.
- #23–#34: the twelve benchmark families.
- #35: consolidated initial results and the proposed Phase 2 backlog.

Execute the work, not just a plan. Continue through implementation, initial
performance collection, final verification, evidence review and issue updates
until PHASE1_TERMINAL_PASS as defined below. Do not stop merely because one
family is done, a command failed, or a review returned REVISE or NO_GO.

## Scope and authority

Read the live issues, applicable AGENTS.md, `docs/general/benchmark_rules.md`,
`docs/roadmap/0.1/0.1.3/README.md`, `testing-rules.md` beside it, and each family's
canonical specification. GitHub issue snapshots may be older than the local
planning files; reconcile and commit the intended contract before implementation
or collection. Do not treat an old snapshot as permission to undo newer work.

Phase 1 builds meaningful benchmarks and collects an honest initial baseline.
Product performance/storage optimization belongs to Phase 2. Fix benchmark,
fixture, oracle, observability and preparation defects now. Preserve product
failures as evidence; do not silently change the product until its first results
are faster or greener. Passive instrumentation and verifier-only fault seams
must preserve the measured product behavior and have explicit source identity.

Inspect the existing dirty tree before editing. Preserve unrelated user work.
Use task-scoped commits and publish the required specifications/implementation
and evidence references; never use blanket staging, reset or destructive cleanup.
Update Phase 1 GitHub issues as work actually completes. Do not tag or publish a
release, start speculative optimization issues, or close central issue #21.

## Coordinate the work

1. Finish the shared contract and essential infrastructure in #22 first.
2. Delegate independent family implementation to subagents with explicit file
   ownership. Tell every worker that others share the repository and that they
   must preserve others' edits. One owner controls shared runner/fixture changes.
3. Parallelize implementation and read-only review, not competing performance
   measurements. One coordinator schedules measured runs by default.
4. Maintain a compact progress/evidence ledger using existing run-status files
   and issue checklists: issue, case/seed/mode, source/input/oracle/environment
   identities, last outcome, evidence path, invalidation reason and next action.
   Check this ledger before scheduling a rerun. Checkpoint it before handoff or
   context loss so later agents continue rather than restart.

## Fast iteration is mandatory

Reuse `benchmark/fs-bench-pro`, its binary and workload helper, existing SDK
fixture generator, runners, custody/cache, independent sample clones and reports.
Do not build another framework, persistent benchmark worker or cache service.

Resolve the selected case, tier and seed before preparation. Prepare only its
required dependencies. Reuse compatible compiled binaries, runtime images,
fixture bytes, independent oracle data and pristine input Stores. Validate each
master once per acquisition/run and keep required sample-identity checks.
Every mutation gets independent writable state. Never reuse live Workspaces,
post-operation Stores, generated history, reader caches or passing receipts as
substitutes for measured work. Measured create/import must still perform all
its writes, reads, CDC and publication.

The normal loop is a cheap self-check, one selected performance case/seed/arm,
inspection of its phase/counter evidence, then a focused fix. Expand only when
needed to resolve a concrete remaining risk. Warm-prepared ordinary commands
should aim for 1–5 seconds; large cases have separately qualified budgets.
Show actual first-use preparation and cache-hit command wall, not only inner
product latency. Never hide work or alter semantics to hit that target.

Do not rerun passing tests or families as a habit. Reuse valid evidence when
its required identities are unchanged. After a relevant shared-code, fixture,
route, oracle or environment change, explicitly identify affected coverage and
rerun that coverage once when ready. Unrelated edits do not justify a full suite.
Report-only changes regenerate reports from raw evidence. Source changes never
authorize relabeling an old run as a new-source result.

## Performance first; verification is rare and late

Implement the verifier with the workload, but do not run exhaustive verification
after every case, seed or edit. Complete the required initial performance
campaign across the families before scheduling the final verification campaign.
Workspace reliability and its endurance cases belong to that later proof stage.

During development, use product-free fixture/ID/algebra checks and cheap
exit/outcome/count checks. Early runtime verification is exceptional: use the
smallest relevant check only for a new execution route, a concrete correctness
signal, or a changed verifier that must be qualified before expensive runs.
Record the reason. Do not repeatedly hash/reopen whole trees for reassurance.

Performance collection contains no added benchmark full-file hashes, tree
manifests, chunk census, reopen, materialization or fault injection. Hashing
intrinsic to CAS or Git remains real measured product work. Collect several
metrics from one execution; several reports may reference that same sealed row.

Once performance collection is complete and the implementation is stable, run
one complete separate verification campaign for every distinct required
fixture/schedule variant and all proof subcases. This includes required history,
failure and 600-second endurance coverage. A few quick operations do not replace
a sustained run. Verification is delayed and infrequent, never omitted.

If final verification finds a defect, preserve the failed result, repair its
root cause, and invalidate only affected evidence. Recollect affected performance
before rerunning affected verification. Keep unaffected valid passes; do not
restart the whole campaign unless a shared change actually invalidates it.

## Preserve the benchmark contract

- Use the declared 1/10/100/500 tier units and shared controls; no redundant
  Cartesian matrix or duplicate execution of identical anchors.
- Each workload file is at most 500 MiB and total logical workload content is
  strictly below 1 GiB at every intermediate state. Include temporary files,
  Git contents, sparse lengths, aliases and open unlinked files. Follow the
  separate physical Store/spool/cache budgets and bounded history accounting.
- SDK edits call the declared public SDK entrypoint. Ordinary tools execute
  through public Exec and real FUSE. A same-file batch is not a multi-file API.
- Preserve all sample outcomes, including slow results and failures. No favorable
  rerolls, fixture changes after results, missing counters presented as zero,
  silent skipped tiers, weakened oracles or benchmark-only product shortcuts.
- Correctness/resource gates and finite safety deadlines precede measurement.
  Performance targets needing a baseline are fixed before Phase 2 optimization.
- Frozen scenario changes require explicit versioning and retained old evidence.
  Inherited oversized growth cases need the specified capped-result replacements.

## REVISE and NO_GO are active work states

For each REVISE/NO_GO, inspect the concrete finding and evidence, classify it,
update the plan, address the smallest real cause, and continue. Do not merely
summarize the failure, ask whether to continue, or stop at a proposed fix.

Harness bugs, invalid measurements, missing observations, broken provenance,
unimplemented members and unexecuted slots prevent Phase 1 completion. Repair
and rerun the affected work until the Phase 1 gate passes.

A valid product performance miss or reproduced product correctness defect is
an initial-baseline finding under the agreed Phase 1 scope. Keep its actual
FAIL/NO_GO product status and exclude invalid results from performance claims.
Record reproduction, impact and the Phase 2 dependency, then complete the rest
of Phase 1. Do not optimize the product or turn a failed verification green
merely to make the tracker look complete. Unexpected mismatches must first be
investigated enough to distinguish a product defect from a broken harness.

If a true external blocker prevents all remaining authorized work, exhaust
independent work and report the exact blocked action, evidence and required
external input. Never bypass permissions or claim terminal pass while blocked.
Ordinary implementation difficulty, failed tests, long execution or a review
request is not such a blocker.

## PHASE1_TERMINAL_PASS

Do not declare completion until all of the following are true:

1. #22 and every family implementation are complete against the committed
   contract, and all required cases/subcases have executed initial outcomes.
2. The current planned 130 unique timed cases / 390 initial sample slots, CDC
   boundary proof and 28 reliability subcases are fully accounted for. Resolve
   any legitimate versioned count change explicitly; no missing slots.
3. Authenticity, harness validity, coverage, custody, required observation and
   baseline-report validation pass. All independent verification has run;
   product correctness/performance/resource failures remain explicitly failed
   findings rather than being counted as passing product checks.
4. Every claimed pass has valid source-bound evidence. Required runtime
   resources are cleaned up or a reproduced product cleanup defect is explicitly
   retained and recovered without misreporting the product's cleanup status.
5. #35 contains the complete initial-results report, preparation and command
   costs, limitations, classified product findings and prioritized Phase 2 work.
6. Completed Phase 1 child issues link their implementation and evidence and
   are closed under their Stage 1 criteria. #21 remains open for later phases.

PHASE1_TERMINAL_PASS means the benchmark-build and initial-evidence phase is
complete. It does not mean every product gate passes or v0.1.3 is release-ready.
If product gates fail, say so prominently in the final report and central issue.

Return a final handoff with source commits, closed/open issue states, evidence
and report paths, actual coverage counts, verification and product statuses,
remaining Phase 2 findings, and any real external blockers. Continue working
instead of issuing an intermediate final response while Phase 1 work remains.
