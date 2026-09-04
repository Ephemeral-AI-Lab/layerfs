> **Superseded: Phase 1 verification has been withdrawn by the user.** See [closure decision](phase-1-verification-withdrawal.md) and [final performance report](phase-1-final-results.md). Earlier instructions and pending verification queues below are historical.

# Phase 2 testing and optimization backlog

This is the proposed backlog for #35, not a Phase 1 terminal or release decision.
The [runtime suppression amendment](phase-1-runtime-suppressions.md) disables
scheduling, not implementation: retain all fifteen suppressed scenario definitions, their
fixtures, public operation routes, oracles and historical outcomes. Any later
budget-triggered exclusions in the [persistent suppression ledger](../../../../benchmark-results/fs-bench-pro/phase1-v013/phase1-runtime-suppressions.json)
join this backlog. Faster code or a new source must not silently re-enable them
in Phase 1. Phase 2 execution needs an explicit scope and measurement contract.

## Current qualified evidence and Phase 2 entry

Checkpoint 2026-09-05: **370 active performance samples are independently qualified**,
with actual producing identities retained: 306 at `7948df2d`, 30 at `30d13dee`,
33 at `e0922904`, and the corrected CDC deletion seed at `e24a3b34`. The [read-only pin qualification](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/readonly-pin-performance-checkpoint/checkpoint.json)
and [rename-cache qualification](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/rename-cache-performance-checkpoint/checkpoint.json)
join the original qualified report without pooling timings from different sources.
The corrections were mandatory functional repairs, not an optional optimization
campaign. Their superseded timings remain historical observations.

All **29 targeted proofs** are qualified: 28 reliability subcases plus the CDC
boundary aggregate. The retained sustained proof supplies 600.003326232 seconds
of active work and 123983 cycles at its original VM4 source/environment; it is not
a VM8 timing and must not be repeated merely to refresh this backlog. Routine fast
verification is still being collected. **Global PHASE1_TERMINAL_PASS has not yet
been established**, and #35 and central #21 remain open.

The following current examples are three-seed medians from the two qualification
checkpoints' `incremental-performance-rows.json` and `family-medians.json` files.
Product time is cumulative declared public-operation time; preparation and complete
sample wall are separate. Store delta is physical file-length growth, not live
canonical bytes or a deduplication ratio. These numbers are observations from the
unoptimized, functionally repaired paths, not prescribed improvement factors.

| Selected active case | Actual source | Product s | Preparation s | Complete sample s | Store delta MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| `agent-episodes-500` | `e0922904` | 10.045371 | 2.589075 | 13.081430 | 12.7500 |
| `namespace-subtree-relocate-delete-100` | `e0922904` | 8.247964 | 3.673712 | 12.386639 | 8.0000 |
| `workspace-fixed-move-500` | `e0922904` | 0.188365 | 3.955823 | 4.526450 | 0.0625 |
| `dedup-history-metadata-500` | `30d13dee` | 3.768286 | 2.367553 | 14.414506 | 0.1250 |
| `directory-content-scan-10` | `30d13dee` | 1.803143 | 2.415040 | 4.682609 | 0.0000 |
| `payload-random-read-500` | `30d13dee` | 2.772245 | 3.198419 | 6.448937 | 0.0000 |

Fixed-move's preparation outweighs its product time, while metadata-history's
complete wall includes costs beyond its declared product sum. Investigate these
separate scopes before choosing a change; the table alone does not identify a
cache, transport, allocation or storage root cause. Keep qualified preparation
reuse and product work distinct in any Phase 2 comparison.

## Historical suppressed-case evidence to carry forward

Use source-bound measurements from clean source
`6c54f8d74a8f07867c6b658da674603c4be6a7c3` as historical starting observations below, not as current repaired-source
or VM8 timing claims. Its SQL history is disabled by default after repair `8278d817`; query counters
and fault instrumentation remain available. The earlier 191 selected performance
passes contained SQL-history allocations inside timed operations and are
**contaminated diagnostics, not eligible optimization baselines**. Preserve them,
both dense500 RSS failures and all other failed or interrupted attempts.

The table reads completed `sample_complete.pure_call_sum_ns` from the
[source-bound slot ledger](../../../../benchmark-results/fs-bench-pro/phase1-v013/slots.json).
For the original fourteen exclusions, values are three-seed medians in seconds,
except the explicitly single-sample namespace row. The fifteenth row is a later
source-30d partial failure at the budget boundary, not a completed timing median. Each listed attempt is a clean-source anchor; its `raw.jsonl`,
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
| `directory-content-scan-100` | 20000 files / 100 MiB read; source `30d13dee` | FAIL at 15.000758416 s observed cumulative product time; incomplete seed 1 | `directory-content-scan-100-s1-performance-3d28647a3175` |

The new directory100 failure is recorded separately in
[the suppression validation](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/readonly-pin-performance-checkpoint/suppression-validation.json).
Seeds 2/3 were not run after the persistent trigger. All three historical passing
timings remain retained but are excluded from current admission. The fifteen
listed IDs are disabled across all later Phase 1 sources, not deleted; restore
them only under an explicit Phase 2 contract. A failed partial duration is not a
throughput sample or a baseline for a claimed speedup.

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
SLO. A **three-to-five-second bulk-operation aspiration** may become an optional,
separately scoped Phase 2 experiment after its exact workload, product-versus-wall
metric, resource caps and baseline are frozen. It is not a universal target or a
new Phase 1 gate, and it does not justify changing fixtures or weakening checks.
Keep performance collection separate from exhaustive verification. After a change,
run the smallest relevant regression and selected integration, then recollect
only source-dependent evidence invalidated by the change. Reuse the existing
runner, fixture generators, public routes, instrumentation and proof machinery.

## Deferred exhaustive routine verification

The [authorized fast-verification amendment](phase-1-fast-verification-amendment.md)
accepts qualified fast proofs for routine Phase 1 coverage. **302 active routine
exhaustive counterparts are deferred to Phase 2**, in addition to the separately
suppressed workloads above. The original amendment checkpoint listed 305; the
later directory100 suppression removes its three seed slots from active coverage.
There are 350 active routine verification slots: 48 already have retained full
proofs, and 302 use the amended fast profile as their Phase 1 completion route.

Retain the exact case/seed/input/source inventory and each accepted proof's actual
assurance: `fast_iteration_verified` is not `fully_verified`. The later exhaustive
campaign restores all required current Store/FUSE body reads, canonical object
census and full history object-union/storage checks that fast receipts explicitly
defer. Preserve all history roots/topology and dedup transcript obligations; a
final-tree-only check is insufficient. Existing full proofs and qualified canonical
references remain reusable where their precise identities and assumptions match.
Do not rerun the 29 already-qualified targeted proofs solely because routine
verification used fast mode; rerun only a gate invalidated by a later change.

## Repair guards and later qualification

Carry the [functional repair record](functional-repair-status.md) forward as
regression obligations: bounded namespace/content-frontier planning, compact
contiguous spool representation, fragmented directory transport, uncached-parent
unlink visibility, sustained truncate/write generations and bounded spill lookup.
Also retain the owner-drop lease/mount cleanup, deferred-error cache rollback,
owned process-group descendant reaping, acknowledged read-only pins, and overwritten
hard-link attribute invalidation regressions. Their corrected targeted proofs are
qualified, including the [final hard-link recovery](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/hardlink-final-checkpoint/checkpoint.json).
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
inherited replacements already have 25 qualified Phase 1 performance samples and
five full proofs, recorded in the [capped completion receipt](../../../../benchmark-results/fs-bench-pro/phase1-v013/qualification/capped-final-checkpoint/family-completion.json).
They remain distinct from the five oversized original definitions and the 58 other
inherited definitions; this is not 58 newly collected release passes. Their evidence
does not replace the fifteen suppressed workloads or the 302 deferred exhaustive
routine checks. Phase 1 suppression neither authorizes release nor
certifies those omitted cases. Keep central issue #21 open; this is #35 backlog
input, not release approval.
