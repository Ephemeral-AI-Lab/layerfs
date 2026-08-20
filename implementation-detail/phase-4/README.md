# Phase 4 — Durable Storage and WP4-M

Phase 4 persists the frozen Phase 1 canonical objects, Phase 2 CDC/CAS content,
and [Phase 3 COW/root/delta semantics](../phase-3.md). SQLite remains the
accepted local durable engine. Historical append-only work is retained only as
rejected evidence.

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
- [Read after F3](wp4m/f-series/f3/read-after.md)

Current status: F2-v3 is retained. F3 is terminal `NO-GO`; no F3-v4 is
authorized. This document reorganization does not create or authorize F4.
