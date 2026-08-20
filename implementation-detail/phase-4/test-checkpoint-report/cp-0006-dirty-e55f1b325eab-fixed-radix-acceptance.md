# CP-0006 — fixed-radix compact acceptance

Status: `RETAIN / PASS`
Date: 2026-08-21
Experiment mode: `acceptance`
Observed campaign wall: `50 seconds`
Configured campaign ceiling: `120 seconds`
Configured per-command ceiling: `60 seconds`
Retained artifact bytes: `523,520`
Transient databases and fixtures deleted: `yes`

The 50-second wall is the terminal runner console observation. The sealed row
schema records the configured 120/60-second ceilings; it does not encode the
aggregate console wall as a row field.

## Identity

| Field | Value |
|---|---|
| Parent checkpoint | `CP-0005` |
| HEAD while built | `d781173a08ab4092eb539c3a0870056e6c6a77ff` |
| Compiled-source diff SHA-256 | `e55f1b325eaba6ac711d45d67e2343d09391cdab5cc8e957a7dbdf844a42c792` |
| Benchmark source SHA-256 | `183a892563439122f0108b1db05d1bf722509e4561bf48603173a35f82cdc70d` |
| Release executable SHA-256 | `7e91b90fecb9b314bfc2706c49184f09ff1e884db34804fc61772aabcf3dbb36` |
| Runner SHA-256 | `965cc07fccc9f8aed8bea342b011d18e66bbf1e2d5680193cec6ea28b8e40c25` |
| Raw JSONL SHA-256 | `b3596ff61b1314bad66f38675bc8acecccaa57d6a8686e30a0e224e91c8f72e1` |
| Python analysis SHA-256 | `d080f0f81346d0ec040801934129da94f04ef1e820b39adb97d733249e4024f5` |
| Ruby analysis SHA-256 | `86cd7018f849bfb605e351c99b47b7d5348dfd295a1e62ed9c8c96d49ead7114` |

## Exact schedule and custody

```text
sampled arms:
  1 MiB:   write
  10 MiB:  write
  100 MiB: write, same-count middle, +1 early, +1 middle

per arm:     1 warmup + 3 measured
capture:     24 rows
roundtrips:   3 rows, one write at each size
total:       27 rows
512 MiB:      0 rows
```

Each arm used one immutable root-local database/authority/expectation master.
Every row used a fresh byte-identical copy, and the runner rehashed all six
masters after the final row. Direct read-only audit confirms one pre-edit hash
triplet and one root/transition/closure tuple per arm. No temporary runner root
or CP-0006 database remains.

All 27 rows report `status=PASS`, exact K64/F64 identity,
`qualification=false`, `promotion=false`, one transaction, one COMMIT dispatch,
one successful return, no COMMIT error, exact timer equations, observed W/D/Q,
and terminal `q_current=0`. Maximum observed logical Q was 2,222,803 bytes.

## Performance

| Operation | Size | Median publication | Spread | Interpretation |
|---|---:|---:|---:|---|
| full write | 1 MiB | 7.191667 ms | 0.453709 ms | measured |
| full write | 10 MiB | 64.032292 ms | 2.079208 ms | measured |
| full write | 100 MiB | 603.327666 ms | 4.355416 ms | 165.747 MiB/s |
| same-count middle | 100 MiB | 8.639167 ms | 0.671625 ms | path-local |
| `+1` early | 100 MiB | 432.939417 ms | 3.411376 ms | suffix-linear |
| `+1` middle | 100 MiB | 432.324667 ms | 6.974376 ms | suffix-linear |

Write wall slopes are 8.903679x from 1 to 10 MiB, 9.422241x from 10 to
100 MiB, and 83.892603x from 1 to 100 MiB. Mapping-byte slopes are
9.606250x, 9.901919x, and 95.120312x respectively.

The nonmedian complete-roundtrip checks were 15.000792 ms, 129.744667 ms,
and 1,246.904708 ms at 1/10/100 MiB. Each used a fresh connection and included
reopen, full scrub, reconstruction, and range verification.

## Exact count-changing work

| Counter | `+1` early | `+1` middle |
|---|---:|---:|
| old references | 5,284 | 5,284 |
| insertion ordinal | 0 | 2,642 |
| source suffix references | 5,284 | 2,642 |
| rebuilt reference occurrences | 5,285 | 2,643 |
| rewritten raw bytes | 104,857,600 | 52,377,184 |
| authenticated objects | 168 | 86 |
| changed leaves | 83 | 42 |
| changed branches | 2 | 2 |
| mapping objects | 86 | 45 |
| canonical mapping bytes | 365,495 | 185,915 |

The publication/full-write ratios are 71.758588% early and 71.656695%
middle. They are retained as nonbinding diagnostics under the prospective
suffix-linear policy, not described as passing the former 5% rejection gate.

The formula-only **100-GiB analytical suffix bound** is not a runtime test or
latency projection. It checks a hypothetical middle insertion at the retained
CDC density: 2,705,409 rebuilt references, 42,273 changed leaves, 673 changed
branches, 42,947 mapping objects, and 186,891,342 canonical mapping bytes.
Runtime allocation of a 100-GiB fixture was zero.

## Decision

```text
CP-0006:  PASS / RETAIN
WP4-M:    COMPLETE
WP4-P:    ELIGIBLE, NOT COMPLETE
K64/F64:  policy-selected input to WP4-P; not compatibility-promoted
DIR256K:  unmeasured fallback input to WP4-P; not compatibility-promoted
Phase 4:  not complete
WP5+:     blocked until WP4-P completes
```

The Python and Ruby analyzers independently return `PASS` with no reasons and
produce semantically identical results after canonical key sorting. The only
runner hardening caveat is that final evidence-file visibility uses exclusive
no-clobber copy rather than rename-atomic publication; the completed CP-0006
file is complete, read-only, hash-matched, and independently parsed, so this
does not invalidate the checkpoint.
