# Phase 2 testing and optimization backlog

This is the proposed backlog for #35, not a Phase 1 terminal or release decision.
The [runtime suppression amendment](phase-1-runtime-suppressions.md) disables
scheduling, not implementation: retain all fourteen scenario definitions, their
fixtures, public operation routes, oracles and historical outcomes. Any later
budget-triggered exclusions in the [persistent suppression ledger](../../../../../benchmark-results/fs-bench-pro/phase1-v013/phase1-runtime-suppressions.json)
join this backlog. Faster code or a new source must not silently re-enable them
in Phase 1. Phase 2 execution needs an explicit scope and measurement contract.

## Evidence to carry forward

Use source-bound measurements from clean source
`6c54f8d74a8f07867c6b658da674603c4be6a7c3` as the starting observations below.
Its SQL history is disabled by default after repair `8278d817`; query counters
and fault instrumentation remain available. The earlier 191 selected performance
passes contained SQL-history allocations inside timed operations and are
**contaminated diagnostics, not eligible optimization baselines**. Preserve them,
both dense500 RSS failures and all other failed or interrupted attempts.

The table reads completed `sample_complete.pure_call_sum_ns` from the
[source-bound slot ledger](../../../../../benchmark-results/fs-bench-pro/phase1-v013/slots.json).
Values are three-seed medians in seconds, except the explicitly single-sample
namespace row. Each listed attempt is a clean-source anchor; its `raw.jsonl`,
`outcome.json`, command, environment and hash manifest live under
`benchmark-results/fs-bench-pro/phase1-v013/attempts/<attempt>/`.
These are observed costs, not final correctness qualification. They also do not
replace the suppression policy's historical decision record when a later clean
observation differs.

| Disabled scenario retained for Phase 2 | Fixed subset | Clean observed product time | Evidence anchor attempt |
| --- | --- | ---: | --- |
| `dedup-history-unrelated-500` | 500 full-tree rewrite/Commit cycles | 390.17 s | `dedup-history-unrelated-500-s3-performance-4a3a69fe1d12` |
| `workspace-dense-rewrite-500` | 100000 files / 500 MiB rewritten | 305.17 s | `workspace-dense-rewrite-500-s2-performance-b8a7d661b72d` |
| `tiny-bulk-create-500` | 100000 files / 500 MiB created | 175.04 s | `tiny-bulk-create-500-s1-performance-4183edd4bd60` |
| `dedup-history-unrelated-100` | 100 full-tree rewrite/Commit cycles | 72.86 s | `dedup-history-unrelated-100-s1-performance-c6e0ed59e326` |
| `directory-content-scan-500` | 100000 files / 500 MiB read | 65.64 s | `directory-content-scan-500-s2-performance-f48a3c3caad0` |
| `workspace-dense-rewrite-100` | 20000 files / 100 MiB rewritten | 54.53 s | `workspace-dense-rewrite-100-s1-performance-5905a279acfc` |
| `namespace-subtree-relocate-delete-500` | Tier-500 subtree mutation | 53.29 s; seed 1 only | `namespace-subtree-relocate-delete-500-s1-performance-8ca2c12dec0f` |
| `tiny-bulk-delete-500` | 100000 files / 500 MiB deleted | 47.17 s | `tiny-bulk-delete-500-s1-performance-69d95a4ef6e2` |
| `tiny-bulk-create-100` | 20000 files / 100 MiB created | 30.09 s | `tiny-bulk-create-100-s3-performance-b7b8228f9d6a` |
| `git-tool-500` | 500 scheduled changes plus Git workflow | 26.53 s | `git-tool-500-s2-performance-171afada1e51` |
| `git-tool-1` | One scheduled change plus Git workflow | 23.19 s | `git-tool-1-s1-performance-e94de148ecb8` |
| `git-tool-10` | 10 scheduled changes plus Git workflow | 23.97 s | `git-tool-10-s2-performance-6daec80a92bb` |
| `git-tool-100` | 100 scheduled changes plus Git workflow | 23.80 s | `git-tool-100-s2-performance-e71653690127` |
| `dedup-history-distributed-500` | 500 distributed SDK edit/Commit cycles | 17.34 s | `dedup-history-distributed-500-s3-performance-a6940f560ae4` |

The namespace seed-2 attempt `namespace-subtree-relocate-delete-500-s2-performance-fa8300eb5d36`
was stopped following the scope change after real work had started; its derived
status is executed partial / user-policy-stop, with cleanup PASS. Its sealed legacy
not-run label remains preserved alongside that correction. It is not a completed
second timing sample or an observed product correctness failure. All four Git cases remain disabled;
completed historical performance does not establish their independent Git proof.

## Investigation order and bounded changes

1. **Large file populations and whole-tree work.** Inspect the named Exec and
   Commit receipts before selecting a change. On clean dense500 seed 2, Exec
   contributed 199.69 s and Commit 112.23 s; on tiny-create500 seed 1 they
   contributed 88.67 s and 85.77 s. Namespace500 seed 1 contributed 28.50 s Exec
   and 24.77 s Commit. These observations justify investigating both public
   workload and Commit paths; they do not prove a particular cache, traversal,
   allocation or storage bottleneck. Use existing visit, byte, spool, capture and
   admission counters to choose a small attributable change. Preserve namespace
   identity, hard links, bounded planning and the current resource caps.
2. **Repeated history commits.** Unrelated500 seed 3 accumulated 317.11 s Exec
   and 73.04 s Commit; distributed500 seed 3 accumulated 1.11 s SDK edits and
   16.21 s Commit. Investigate those different cost distributions independently.
   Reuse the frozen history schedules and verify every retained snapshot and
   deduplication invariant. Do not substitute a shorter history or only its
   final tree. Reduced repeated work is a hypothesis until counters and a
   controlled comparison establish it.
3. **Read traversal and Git.** Content-scan500 seed 2 spent 65.47 s in Exec and
   about 0.002 s in its clean Commit. Git10 seed 2 spent 21.17 s in Exec and
   2.74 s in Commit. Exec includes the actual Linux workload/process path;
   these aggregate timings alone do not separate Git computation, filesystem
   requests or transport overhead. Diagnose a representative selected case
   before proposing changes, preserving Git commands, immutable native reference
   identity, precommit custody and independent semantic/full-tree verification.
4. **Preparation and storage follow-up.** Reuse qualified canonical inputs and
   the existing bounded spill index, cache custody, import and resource reports.
   Preparation is separate from product time. Any proposal to improve temporary
   indexes, object admission, layout or storage amplification needs measured
   evidence and its own bounded comparison; this backlog does not prescribe an
   architecture rewrite or reopen already repaired preparation by default.

Freeze any optimization target after reviewing a clean selected baseline. The
Phase 1 fifteen-second scheduling limit is not an established Phase 2 performance
SLO, and this backlog imposes no universal three-to-five-second target. Keep
performance collection separate from exhaustive verification. After a change,
run the smallest relevant regression and selected integration, then recollect
only source-dependent evidence invalidated by the change. Reuse the existing
runner, fixture generators, public routes, instrumentation and proof machinery.

## Repair guards and later qualification

Carry the [functional repair record](functional-repair-status.md) forward as
regression obligations: bounded namespace/content-frontier planning, compact
contiguous spool representation, fragmented directory transport, uncached-parent
unlink visibility, sustained truncate/write generations and bounded spill lookup.
Their original failures remain failures at their producing sources. The d6 rebase
copy reduction was insufficient to resolve the RSS failure by itself; SQL-history
retention was the subsequently demonstrated shared cause. The clean dense500
seed-2 anchor above completed with product/cleanup PASS, while full independent
verification for this now-suppressed subset remains outside current Phase 1
coverage. Do not compare a failed attempt's duration with a successful attempt
as a performance improvement.

At Phase 2 entry, consult the final Phase 1 report for the actual surviving
proof/source mappings and any newly discovered failures; the earlier dated
checkpoints in the repair record are historical. This document does not claim
that current verification has completed. Restore suppressed coverage under the
new phase contract, retain original seeds and repetitions, and run its independent
correctness/resource/cleanup gates before making qualification claims.

A later release candidate still needs the prescribed inherited acceptance refresh,
release checks and any affected platform/configuration coverage. The five capped
inherited cases remain a separate inventory; their evidence does not replace the
fourteen omitted workloads. Phase 1 suppression neither authorizes release nor
certifies those omitted cases. Keep central issue #21 open; this is #35 backlog
input, not release approval.
