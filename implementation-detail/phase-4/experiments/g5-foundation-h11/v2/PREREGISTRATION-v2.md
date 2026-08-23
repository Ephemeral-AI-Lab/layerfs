# H11 retained-control preregistration v2

Status: **FROZEN BEFORE ANY V2 MEASURED ROW**. V1 is preserved at `target/phase4-g5-foundation-h11-20260822-v1/` with `REVISE`. V2 changes only the two invalid analyzer comparisons below; it reuses the exact v1 binary, fixture, 1,001-row oracle, operation log, and balanced eight-row schedule, all rebound by `method/METHOD-MANIFEST-v2.tsv`.

## Exact repair

1. The first post-reopen edit at N=1 follows a genesis transition and authenticates only the current graph. N=10/100/1,000 follow non-genesis replace transitions and authenticate the exact current and parent graphs. Therefore first-edit SQL/query/row/BLOB/authentication parity is required within the N=1 pair and independently across all six N=10/100/1,000 rows. First-edit latency materiality uses N=10 control versus N=1,000 candidate. The N=1 genesis measurement remains raw evidence but is not mislabeled as the same mechanism.
2. Logical and apparent SQLite image bytes remain exact within each pair. Filesystem allocated bytes are reported raw, must have pair spread `<=1,048,576`, and use `max(N=1000) - min(N=1)` for the hard `<=131,072 bytes/revision` slope. Allocation-block identity between independently created files is not asserted.

All other v1 requirements remain unchanged: exact roots/digests/history; current-live invariance; hard work parity for reopen/head/range/reconstruction/materialization; exact transactions/COMMITs; retained reachability; object/canonical/mapping/storage slopes; RSS `<=20,971,520`; individual buffer `<=1,048,576`; Q/residue/leaks zero; exact primary/independent agreement; fail-fast global lock; and complete wall `<=20,000,000,000 ns` through terminal verification.

Latency materiality remains exact:

```text
candidate_sum * 100 > control_sum * 105
AND candidate_sum - control_sum >= 2,000,000 ns
```

It applies only to latency. No semantic, work, resource, storage, custody, cleanup, chronology, authority, durability, reconciliation, or observability gate receives an exception. There is no outcome-rescue rerun after this v2 row.
