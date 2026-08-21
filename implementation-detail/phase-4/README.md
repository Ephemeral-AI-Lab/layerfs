# Phase 4 — Durable Storage and WP4-M

Phase 4 persists the frozen Phase 1 canonical objects, Phase 2 CDC/CAS content,
and [Phase 3 COW/root/delta semantics](../phase-3.md). SQLite remains the
accepted local durable engine. Historical append-only work is retained only as
rejected evidence.

## Current status

CP-0006 PASS closes WP4-M without promoting its rows. WP4-P is COMPLETE / PASS:
losers/selectors are deleted, the one compatibility-promoted K64/F64 + DIR256K
profile has production ID
`b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1`
and final TSV/golden-test hashes `6de8c752...a7330` / `727fe668...49701`.
Core 44, selected goldens 2 PASS/1 ignored printer, benchmark 55, parity 14,
full workspace, clippy, deletion, and both independent audit gates pass after
the exact 2,010-entry maximum delta-page corpus fix. WP4 is complete; WP5 is
eligible/pending. Overall Phase 4 is not complete. DIR256K remains explicitly
an unmeasured fallback. No further WP4-M or 512-MiB campaign is required.

Post-promotion CP-0007 retains the exact promoted profile and replaces the
duplicate full pre-COMMIT closure replay for count-changing edits with a
transaction-local construction proof. Retained 100-MiB `+1` medians are
7.868417 ms early and 6.946583 ms middle, with fresh round trips passing. The
mapping remains honestly suffix-linear; WP4-P is not reopened.

CP-0008 measures that suffix curve directly at 1/10/100/500 MiB. At 500 MiB,
publication is 27.140916 ms early and 15.102042 ms middle while exact suffix
references and mapping bytes grow approximately fivefold from 100 MiB. The
current <=50-ms suffix-linear policy still retains K64/F64; an 8–10-ms
scale-independent product SLA would require stopping before WP5 for a
canonical prolly tree.

CP-0009 is now the exact current-binary product-workflow control for research
A/B: 640.109-ms durable 100-MiB submit, 9.737-ms same-open same-count edit,
425.801/433.513-ms warm/fresh logical materialization, 3.008-ms reopen/head,
and a new authenticated returned 1-MiB range at 3.285 ms / 315.337 MiB/s.
Candidate claims must use adjacent balanced pairs against this control rather
than subtracting standalone historical medians.

Canonical-v2 complete validation is now **PASS / FROZEN** for the exact
fresh-store profile. Its two adjacent 100-MiB pairs improved durable full
create by 22.743% and 23.835%; the position-balanced center is 667.652 ms
control versus 512.214 ms v2 (**23.281% faster**). All 29 lifecycle rows,
static tests, clippy/fmt/diff, identity, authority, one-COMMIT, timer, bounded-Q,
publication no-rescan, storage, residue, custody, and terminal-manifest gates
pass. This is the next optimization baseline; CP-0009 remains its historical
v1 control and rollback authority. Automatic nonempty v1-to-v2 migration is
still unsupported, Phase 4 is not otherwise complete, and no later lane is
started by this freeze.

FastCDC contiguous-region kernel v2 is now **PASS / RETAINED** as the exact
Canonical-v2 successor. Its independent confirmation measured 398.555 ms
adjacent control versus 332.028 ms candidate for the durable 100-MiB
capture/publication boundary, or 301.180 MiB/s; 4/4 pairs and both positions
won with exact identities, work, durability, Q, storage, and residue. Serial
safe-Rust exact-boundary CDC tuning is closed. Phase 4 remains active for the
writer-memory policy, authenticated/cold/trusted-hot and incremental
materialization, reopen authority, and count-change locality.

- [Fast-lane amendment](wp4m/fixed-radix-fast-lane-amendment.md)
- [Compact evidence contract](wp4m/fixed-radix-compact-evidence-contract.md)
- [WP4-P promotion ledger](wp4p/promotion.md)
- [Terminal profile report](wp4m/profile-selection-report.md)
- [CP-0006 report](test-checkpoint-report/cp-0006-dirty-e55f1b325eab-fixed-radix-acceptance.md)
- [CP-0006 raw JSONL](test-checkpoint-report/cp-0006-dirty-e55f1b325eab-fixed-radix-acceptance.jsonl)
- [Python analysis](test-checkpoint-report/cp-0006-dirty-e55f1b325eab-fixed-radix-python-analysis.json)
- [Ruby analysis](test-checkpoint-report/cp-0006-dirty-e55f1b325eab-fixed-radix-ruby-analysis.json)
- [CP-0007 count-changing proof report](test-checkpoint-report/cp-0007-dirty-88ffb0bd6a30-count-change-proof.md)
- [CP-0007 preregistration and decision](wp4p/post-promotion-count-change-proof.md)
- [CP-0008 count-change scale report](test-checkpoint-report/cp-0008-dirty-4f1c97f81f7c-count-change-scale.md)
- [CP-0008 scale preregistration](wp4p/count-change-scale-diagnostic.md)
- [CP-0009 current product baseline](test-checkpoint-report/cp-0009-dirty-b073a7e04c7a-current-product-baseline.md)
- [Current baseline manifest](baseline/current-baseline-v1-manifest.tsv)
- [Baseline index](baseline/index.md)
- [Canonical-v2 frozen baseline](baseline/canonical-v2-baseline-v1.md)
- [Canonical-v2 baseline manifest](baseline/canonical-v2-baseline-v1-manifest.tsv)
- [FastCDC v2 baseline](baseline/fastcdc-contiguous-region-kernel-v2-baseline-v1.md)
- [Current benchmark scoreboard](baseline/current-benchmark-scoreboard.md)
- [CP-0010 current grind checkpoint](test-checkpoint-report/cp-0010-dirty-72ed9fee8e6a-fastcdc-v2-phase4-grind.md)

## Core contracts

### Algorithm

- [Algorithm specification](algorithm/spec.md)
- [Tests and benchmark specification](algorithm/tests-and-benchmarks.md)
- [Complexity analysis](algorithm/complexity-analysis.md)

### Mapping

- [Logical persistence mapping](mapping/logical-persistence.md)
- [Mapping research handoff](mapping/research-handoff.md)

### SQLite storage

- [SQLite durable-engine specification](storage/sqlite/spec.md)
- [SQLite implementation plan](storage/sqlite/implementation-plan.md)
- [Visible-head migration and publication](storage/sqlite/visible-head.md)

## Historical storage alternative

The append-only carrier was explored, rejected, and deleted from the active
implementation. Moving its reports does not reopen it.

- [Decision record](storage/append-only/decision.md)
- [Rejected specification](storage/append-only/spec.md)
- [Acceptance ledger](storage/append-only/acceptance-ledger.md)
- [First-implementation findings](storage/append-only/first-implementation-findings.md)

## Rollback to SQLite

- [Rollback specification](rollback/spec.md)
- [Rollback implementation plan](rollback/implementation-plan.md)
- [Execution handoff](rollback/execution-handoff.md)
- [Deletion record](rollback/deletion-record.md)

## WP4-M

The [rolling progress ledger](wp4m/progress.md) is the quickest status entry.
The [implementation handoff](wp4m/implementation-handoff.md) describes the
original WP4-M boundary.

### Checkpoints

- [Baseline checkpoint 1](wp4m/checkpoints/baseline-1.md)
- [Optimization checkpoint 2](wp4m/checkpoints/checkpoint-2.md)

### Milestones M0–M4

- [M0 — measurement truth](wp4m/milestones/m0.md)
- [M1 — borrowed SQLite reads](wp4m/milestones/m1.md)
- [M1b — residual borrowed rows](wp4m/milestones/m1b.md)
- [M2 — bounded reconstruction fetches](wp4m/milestones/m2.md)
- [M3 — borrowed encoding and ObjectId reuse](wp4m/milestones/m3.md)
- [M4 — receipt-backed changed-spine validation](wp4m/milestones/m4.md)

### M4.5

M4.5 has multiple reports and therefore one folder:

- [Specification](wp4m/milestones/m4-5/spec.md)
- [Terminal report](wp4m/milestones/m4-5/report.md)
- [M4.5-0 baseline freeze](wp4m/milestones/m4-5/0-baseline-freeze.md)
- [M4.5-1 authority witness](wp4m/milestones/m4-5/1-authority-witness.md)
- [M4.5-2 shadow proof](wp4m/milestones/m4-5/2-shadow-proof.md)
- [M4.5-3 COMMIT reconciliation](wp4m/milestones/m4-5/3-commit-reconciliation.md)
- [M4.5-4 accounting](wp4m/milestones/m4-5/4-accounting.md)
- [M4.5-5 release comparison](wp4m/milestones/m4-5/5-release-comparison.md)
- [Repair benchmark](wp4m/milestones/m4-5/repair-benchmark.md)
- [V3 follow-up](wp4m/milestones/m4-5/v3-follow-up.md)
- [V3 terminal benchmark](wp4m/milestones/m4-5/v3-terminal-benchmark.md)
- [Independent audit](wp4m/milestones/m4-5/independent-audit.md)

## F-series

### Planning

- [Read after M4.5](wp4m/f-series/planning/read-after-m4-5.md)
- [Full-create plan](wp4m/f-series/planning/full-create-plan.md)
- [Finalization handoff](wp4m/f-series/planning/finalization-handoff.md)
- [Retained 100-MiB lifecycle](wp4m/f-series/planning/retained-100-mib-lifecycle.md)

### Results

- [F0 — checkpoint freeze](wp4m/f-series/f0.md)
- [F1 — COMMIT and I/O observability](wp4m/f-series/f1.md)
- [F2 — retained construction proof](wp4m/f-series/f2/report.md)
- [F2-v1 audit addendum](wp4m/f-series/f2/v1-audit-addendum.md)
- [F3 — terminal insertion-grouping and causal-diagnostic report](wp4m/f-series/f3/report.md)
- [Cursor Git research note](../../research/cursor-git-at-any-scale.md)
- [F4-A — accepted F2-v3 residual attribution](wp4m/f-series/f4/report.md)

Historical optimization status: F2-v3 is retained. F3 is terminal `NO-GO`;
no F3-v4 is authorized. F4-A is terminal `NO-GO`: no isolated removable
mechanism passes the 33-ms/4-of-5 gate. These results do not reopen WP4-M or
authorize F5/F6.
