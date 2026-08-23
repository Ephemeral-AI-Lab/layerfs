# H11 diagnostic storage/history model

H11 retains every root. It distinguishes the current reachable graph, the union reachable from retained roots, and objects unreachable from the current root but still reachable from history. “Unreachable” never means collectible while a retained root pins the object.

Status: storage/reachability numbers below are internally consistent diagnostics, but H11 is `REVISE` because the reachability sets and other harness allocations were omitted from logical Q. The table must not be cited as a complete resource PASS.

## Observed checkpoint state after the measured N+1 edit

| N | Stored/retained objects | Stored canonical bytes | Stored mapping bytes | Current-live objects/bytes/mapping | Current-unreachable objects | Logical/apparent store | Allocated samples |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 63 | 1,074,439 | 4,345 | 58 / 1,051,574 / 2,255 | 5 | 1,130,528 / 1,130,528 | 1,134,592 / 1,134,592 |
| 10 | 117 | 1,281,709 | 24,640 | invariant | 59 | 1,359,904 / 1,359,904 | 1,363,968 / 1,363,968 |
| 100 | 657 | 3,354,409 | 227,590 | invariant | 599 | 3,592,224 / 3,592,224 | 3,596,288 / 3,596,288 |
| 1,000 | 6,057 | 24,081,409 | 2,257,090 | invariant | 5,999 | 25,964,576 / 25,964,576 | 25,993,216 / 26,091,520 |

Retained-unreachable objects were exactly zero at every checkpoint: the stored set equals the union reachable from retained roots. Current-live shape is independent of history count.

## Slopes from N=1 to N=1,000

| Metric | Observed per added revision | Frozen ceiling |
|---|---:|---:|
| Stored objects | 6.0 | 16 |
| Stored canonical bytes | 23,030.0 | 65,536 |
| Stored mapping bytes | 2,255.0 | 8,192 |
| Logical/apparent store bytes | 24,858.907 | 131,072 allocated ceiling used as conservative comparison |
| Worst-case allocated store bytes | 24,981.910 | 131,072 |

The unique-change cost is one new raw object plus the changed K64/F64 spine and publication mappings: six objects and 23,030 canonical bytes per revision. It is not revisions multiplied by the 1-MiB file. SQLite page/envelope overhead raises logical growth to about 24.9 KiB/revision.

This model is append-only retention, not GC proof. Before a full G5-C gate can add branch/revert, released-root reachability, crash-safe mark/sweep, index deletion ordering, restart, cancellation, capacity, ambiguous publication, and concurrency, a corrected H11 must first charge its own expectations, traversal state, timing/report buffers, return traversal high-water, and prove zero only after every charge drops.
