# Phase 4 G5-1 Foundation terminal handoff

Status: **REVISE** at checkpoint `d58c5a1307253dfc221fe50de996c183deb9458a` because H11 omitted benchmark-owned allocations from hard logical Q. G4 remains **PASS / CLOSED**; sealed v12 remains **REVISE** under its original relative-only rule; the G4 stage disposition remains `PASS_WITH_USER_APPROVED_SUB_1MS_MICRO_VARIANCE_POLICY`. Nothing here relabels sealed G4 evidence.

G5-1 reconciled the final G4 baseline, completed primary-source Xet research, froze the prospective G5 materiality rule, prepared lane contracts, and preserved two H11 history/reopen attempts. V2 produced useful diagnostics but failed final whole-harness Q authority. G5-1 made no production/profile change and stops before G5-2.

## Post-terminal planning addendum — 2026-08-22

Later user discussion explicitly proposed a weaker, opt-in
`TrustedLocalDev` threat model and a bounded warm-projection service. That new
assumption does not relabel this terminal result: `RETAIN_FULL_REOPEN_AUTHENTICATION`
remains correct under the original adversarial freshness model.

The audited prospective execution plan is now
[Phase 4 G5 implementation and verification](../../../implementation-detail/phase-4/g5/implementation-verification-plan.md).
It preserves the verified control, removes only eager complete-closure scrub
from the trusted candidate, keeps fetched/new/incumbent CAS identity checks,
and separates canonical COMMIT from bounded derived projection.

The [fast-iteration contract](benchmark-contracts/g5-fast-iteration-contract.md)
now explicitly permits a complex state/fault matrix while requiring shared
preparation, long-lived stateful children, checkpoint verification, compact
timing sidecars, a `<20 s` complete screen, and a `<=120 s` complete gate.
Operations are divided into mandatory mechanism verification, compact
protected sentinels, G5-3 closure, and later optimization lanes. No untested
operation receives an implicit performance claim.

The corrected H11 whole-harness Q/custody attempt is complete at v9. G5-0 is
PASS with eight fresh rows, exact terminal Q zero, two agreeing analyzers, and
38/38 final artifacts reverified. G5-1 `TrustedLocalDev` is now the active
milestone; G5-2, G5-3, G6, and production integration remain closed.

## Terminal decisions

| Lane | Disposition | Reason |
|---|---|---|
| G5-A reopen authority | `RETAIN_FULL_REOPEN_AUTHENTICATION` | No non-replayable mutation authority or protection domain exists; Xet immutable-gap summaries do not detect unreported native-file mutation. |
| G5-B count-changing mapping | `RETAIN_K64_F64` | Final Canonical-v2 controls replace stale v1 numbers; exact Xet 3–9 grouping cannot meet the 105% live-byte gate, and every candidate remains suffix-linear under adversarial cuts. |
| G5-C history/concurrency/GC | `H11_REVISE_EXACT_BLOCKER` | V2's analyzer-level identity/work/storage result is useful, but its 73,033-byte Q omits the expected-manifest/vector, reachability sets, history timings, and report output; literal terminal zero is not authoritative. |
| G5-D cold/SQLite/locality | `RETAIN_CURRENT_SQLITE_PROFILE` | H11 supplies logical/apparent/allocated and SQL/BLOB evidence, but cold state and byte-level physical I/O remain unavailable. |

The single G5-2 starting action is: **freeze the smallest corrected H11 whole-harness Q and custody protocol before any broader G5-C concurrency/GC gate**. This is a selected next action only; G5-2 work did not begin.

## Package map

| Area | Document |
|---|---|
| Final G4 authority | [G4 final reconciliation](baseline/g4-final-reconciliation.md) |
| G5-A | [Reopen authority](reopen-authority/report.md) |
| Xet and G5-B | [Xet transfer analysis](count-changing-mapping/xet-transfer-analysis.md), [candidate comparison](count-changing-mapping/candidate-comparison.md), [proposed G5-B0 shadow](count-changing-mapping/proposed-g5-b0-shadow-contract.md) |
| G5-C/H11 | [H11 result](concurrency-history/h11-result.md), [resource/history model](concurrency-history/resource-history-model.md) |
| G5-D | [Cold/SQLite attribution](cold-sqlite/attribution-and-candidate-order.md) |
| Endurance contracts | [Longitudinal matrix](history-endurance/longitudinal-workload-matrix.md), [history scaling](history-endurance/history-scaling-contract.md) |
| Benchmark contracts | [Fast iteration](benchmark-contracts/g5-fast-iteration-contract.md), [H11 preregistration link](benchmark-contracts/h11-preregistration-link.md) |
| Custody | [Handoff manifest](custody/g5-handoff-manifest.tsv) |
| Decisions | [Lane dispositions](decision/lane-dispositions.md), [final synthesis](decision/final-synthesis.md) |

## Prospective G5 latency materiality

A protected latency regression is product-material only when both `candidate/control > 1.05` and candidate mean minus control mean is at least `1,000,000 ns`. For fixed two-sample comparisons the exact predicates are:

```text
candidate_sum * 100 > control_sum * 105
AND candidate_sum - control_sum >= 2,000,000 ns
```

Every raw value, sum, mean, ratio, delta, and branch remains reportable. Identity, topology, errors, work, authority, durability, cleanup, Q/RSS/buffers, storage, custody, chronology, analyzer agreement, and observability labels remain hard.

## Boundaries

No G5-A broker, persistent G5-B tree, concurrency/GC implementation, SQLite/profile change, G6/WP5, VFS/SDK/application integration, production change, or commit was made. Xet xorbs, shards, flat recipes, global indexes, remote concurrency, and compression remain outside the local LayerFS core.
