# Phase 1 planning evidence (2026-09-17)

> **Status:** The four read-only planning reports behind
> [`phase1-implementation-plan-20260917.md`](../../component-decoupling/phase1-implementation-plan-20260917.md)
> and [`phase1-terminal-handoff-20260917.md`](../../component-decoupling/phase1-terminal-handoff-20260917.md),
> for #178's Phase 1. Static source reading only: no builds, no tests, no
> measurements, no product changes. Append-only.

| Report | Scope | Items |
| --- | --- | --- |
| `plan-nav-validate.md` | C1 tree navigation + validation | P1-1 (width 256 — the 4,096 ceiling is unreachable inside the 4 MiB lease), P1-2 (per-op session, `RefCell` in `StoreProvider` not `Store`), P1-3, P1-4 (+ V1 vehicle) |
| `plan-edit-assembly.md` | chunked edit path + assembly | P1-6, P1-7, P1-8, P1-9, P1-14 (+ V2 vehicles: `edit_memory_probe`, `edit_timing_c1` delete/shrink fixtures), P1-12 |
| `plan-ordering.md` | ordering subsystem + accounting | P1-5, P1-10, P1-11 (+ the `read_waves` double-count ruling input), P1-13, P1-15, P1-16 |
| `test-evaluation.md` | the test strategy for all 16 | 34-test parity guard list, pinned-behaviour changes (2 pre-authorized, 3 avoided by design), 15 new discriminating tests, prerequisite vehicles V1–V3, the anchor-honesty sequencing risk |

Trees read: `625ad7b57` (clean). Before anchors: the Phase 0 receipts at
[`../phase0-baseline-20260917T221759Z/`](../phase0-baseline-20260917T221759Z/).
The main agent adjudicated the planning findings before entering them into the
plan — including the `read_waves` double-count at `objects.rs:107/:124`
(confirmed in source and ruled a bug, folded into prerequisite C1) and the
P1-1 scratch arithmetic (4,096 × 8,280 B > the 4 MiB lease; width 256 chosen).
