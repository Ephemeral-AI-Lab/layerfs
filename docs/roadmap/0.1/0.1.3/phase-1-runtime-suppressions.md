# Phase 1 runtime suppressions — 2026-09-04

The user explicitly changed Phase 1 scope: suppress the slow case/subset
combinations listed below and permanently suppress any further combination
whose measured product time exceeds 15 seconds. This supersedes earlier
requirements to execute every original case in Phase 1. It is a scope amendment,
not evidence that the suppressed cases pass or that their performance is fixed.

Suppression disables Phase 1 scheduling only. Keep every benchmark definition,
implementation and recorded outcome intact. The suppressed combinations remain
in the Phase 2 inventory for testing and optimization; this is not deletion.

## Time limit and persistence

- Limit: **15,000,000,000 ns per performance sample**, using the same named
  product-time sum reported as `pure_call_sum_ns` in the preceding discussion.
  Import cases use initialization time; Workspace/history cases accumulate
  all declared product phases for that sample. The limit does not reset per
  phase, Exec or Commit, and a family batch is not one sample.
- Suppress the **exact scenario ID / subset**, across all seeds, repetitions
  and future Phase 1 source revisions. Other tiers remain enabled. One sample
  exceeding the limit is sufficient; do not wait for a three-sample median.
- Enforce the cumulative budget through the existing supervisor so a remaining
  long phase does not run to its old multi-minute deadline. Preserve the
  timeout/partial outcome and finish necessary stop/recovery/cleanup. Do not
  kill unrelated work or abandon owned resources to meet the time limit.
- Check suppression before preparing or launching a sample. When it trips,
  persist the decision immediately and skip remaining repetitions and future
  performance or associated verification runs for that combination in Phase 1.
  It stays suppressed until an explicit user change; faster code, a new source
  or a fresh process must not automatically enable it again.
- Preparation, outer CLI validation, standalone proof-only recipes and cleanup
  are separate from this measured performance metric. Independent verification
  for still-enabled cases remains required and separate. This amendment is not
  permission to weaken their correctness or resource checks.
- Keep every original outcome, source identity and suppression reason. Record
  `suppressed_phase1_time_budget` separately from PASS, FAIL and unimplemented
  work. Do not substitute a skip for a successful operation or erase a known
  correctness defect. Report the original inventory, suppressed inventory and
  remaining active coverage separately.

## Initial suppression list

These are all fourteen combinations in the preceding discussion, including
the four with earlier-source results awaiting recollection. Suppression is
immediate; no new timing confirmation is needed.

| Exact scenario ID | Subset | Previously observed median |
| --- | --- | ---: |
| `dedup-history-unrelated-500` | 500 full-tree rewrite/Commit cycles | 390.17 s |
| `workspace-dense-rewrite-500` | 100,000 files / 500 MiB rewritten | 305.17 s |
| `tiny-bulk-create-500` | 100,000 files / 500 MiB created | 175.04 s |
| `dedup-history-unrelated-100` | 100 full-tree rewrite/Commit cycles | 72.86 s |
| `directory-content-scan-500` | 100,000 files / 500 MiB read | 65.64 s |
| `workspace-dense-rewrite-100` | 20,000 files / 100 MiB rewritten | 54.53 s |
| `namespace-subtree-relocate-delete-500` | Subtree mutation, tier 500; earlier source | 50.87 s |
| `tiny-bulk-delete-500` | 100,000 files / 500 MiB deleted | 47.17 s |
| `tiny-bulk-create-100` | 20,000 files / 100 MiB created | 30.09 s |
| `git-tool-500` | 500 scheduled changes and Git workflow; earlier source | 26.23 s |
| `git-tool-1` | One scheduled change and Git workflow | 23.09 s |
| `git-tool-100` | 100 scheduled changes and Git workflow; earlier source | 22.87 s |
| `git-tool-10` | 10 scheduled changes and Git workflow; earlier source | 22.68 s |
| `dedup-history-distributed-500` | 500 distributed SDK edit/Commit cycles | 17.34 s |

All four Git performance cases are consequently suppressed for Phase 1. Keep
their implementation and historical evidence; classify the family as wired
with runtime-suppressed execution, not as a passing four-case family.

The original new-performance inventory remains 130 cases / 390 samples.
This initial list suppresses 14 cases / 42 prescribed samples, leaving
**116 active cases / 348 prescribed samples**, before any additional budget
triggers. The five capped inherited cases remain separately accounted and
are subject to the same per-sample performance limit.

## Execution handoff

The coordinating Phase 1 task owns the minimal runner/report implementation,
its source-bound evidence policy and updates to #21, #22, affected family
issues and #35. Reuse existing selection, supervision, ledgers and reporting;
do not create another runner. Reflect the amended active inventory in terminal
criteria while keeping all suppressions visible. All still-enabled correctness,
resource, cleanup and evidence gates must pass. Suppression does not authorize
release qualification for the omitted coverage.

Do not launch suppressed work already queued in the current campaign. An
already-running sample should be stopped through its normal supervisor and
cleaned up when controllable; any result completed before the instruction
arrived remains historical evidence and must not cause further repetitions.
Do not spend Phase 1 optimizing suppressed cases merely to re-enable them.
