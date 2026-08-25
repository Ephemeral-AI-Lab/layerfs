# Stage 1.1 terminal audit closure — attempt-003

Disposition: `PASS_PRIMARY_TRUSTED_CLASS`, separately labeled; Verified remains
`REVISE_NO_AUTHORIZED_OWNER` with `terminal_pass=false`.

The corrected Trusted population contains exactly 3 warmups and 9 measured
rows at 0/24/96 MiB. All 12 rows use schema
`layerfs-stage1m-attribution-row-v2`, identify `TrustedLocalDev`, and satisfy
both the independent row-wall and product-operation timer equations.

| Size | p50 | p95 | p50 MiB/s | p95 MiB/s | Primary |
|---:|---:|---:|---:|---:|---|
| 0 MiB | `22.912958 ms` | `26.752542 ms` | N/A | N/A | report |
| 24 MiB | `38.150500 ms` | `41.080458 ms` | `629.087` | `584.219` | PASS |
| 96 MiB | `84.157708 ms` | `96.167333 ms` | `1140.715` | `998.260` | PASS |

The fitted sustained rate is `1564.972 MiB/s`. The fitted intercept is
`22.814764 ms`, so the fixed `<20 ms` target still misses by `2.814764 ms`.
The `0.098194 ms` zero residual keeps the model valid; it is not used to turn
the fixed-cost miss into PASS.

Across all rows, `26,016 fetched = 26,016 role decodes`, with zero fetched-row
identity authentication and zero identity-authentication wall. Q peaks at
`8,388,607 B` and returns to zero; RSS peaks at `15,564,800 B`; scratch/total
connections peak at `1/2` and terminate at zero; FD closes at `4`; residue and
network use are zero. Maximum Trusted row CPU is `108,275,000 ns`. The Stage
1.1 edge schema does not expose operation CPU, so regression CPU is honestly
Unavailable rather than zero.

The current-source Stage 1.1 regression is attempt-020 at commit `36d05d8`:
`47/47` rows, `51/51` edit/sub-edit operations and `34/34` durable transitions,
with exact bytes, metadata, history, refresh routes, `34` transactions/COMMITs,
zero rollback/BUSY/LOCKED/rematerialization, Q terminal zero, connections
terminal zero, FD `5 -> 5`, and zero owned residue. Its frozen single-file
fixture contains no hard-link group (`nlink=1`); hard-link ordering is proven
by the passing workspace tests, not fabricated as a campaign observation.

Attempts 016–019 and the first failed closure remain append-only diagnostic
evidence. Stage 1.2 and Docker/FUSE were not started or resequenced.
