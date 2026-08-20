# CP-0005 — fixed-radix acceptance attempt 1

Status: `REVISE`
Date: 2026-08-21
Experiment mode: `acceptance`
Total experiment wall: `92 seconds`
Retained artifact bytes: `522,126`
Transient databases and fixtures deleted: `yes`

## Identity

| Field | Value |
|---|---|
| Parent checkpoint | `CP-0004` |
| HEAD while built | `d781173a08ab4092eb539c3a0870056e6c6a77ff` |
| Compiled-source diff SHA-256 | `eeca9d9b70188d3d6d2100248bc61b86803c4872e330fafe94cc466c7675918a` |
| Benchmark source SHA-256 | `d64c1719db401b6c179d45eb377d3ff7ac998cfc38172ee01fd3d7f981e7181c` |
| Release executable SHA-256 | `5a55046582bacc84525e796fb511465f63ff101443c259968587cfe63e4900ec` |
| Runner SHA-256 | `f3a767f97bce24c59e0264df2cfc36376de6049b9af9fa61d0a4e898911ad462` |
| Raw JSONL SHA-256 | `942f48cf4cd4d8f5cdb98a7ca966d14e6d5a4690dd7b9e58cf807d8d7ca52ff1` |
| Python analysis SHA-256 | `81205443b973bc82bb44f44d220cad11e0ecabb1cb35ecb052d88181a0a48da3` |
| Ruby analysis SHA-256 | `a9170ea2d362db425669b1e0a8493679c155d3dbc6a7a755260abf1d32f6ac7c` |

## Contract result

The compact runner completed the exact 27-row schedule in 92 seconds:

```text
24 capture rows = 6 arms * (1 warmup + 3 measured)
3 complete-roundtrip checks = one write at 1/10/100 MiB
512-MiB rows = 0
temporary SQLite state retained = 0 bytes
```

All returned rows passed identity, CDC, root/transition/closure, one
transaction/one COMMIT, timer, suffix-accounting, W/D, and terminal-Q checks.
Both independent analyzers nevertheless return `FAIL` for the same single
orchestration cause:

```text
100-MiB edit-same: unstable pre-edit database/authority custody
100-MiB +1 early:  unstable pre-edit database/authority custody
100-MiB +1 middle: unstable pre-edit database/authority custody
```

The runner prepared each sample independently. The logical expectations,
roots, transitions, and closure digests are stable, but each preparation
creates a new store/authority identity, so the database and authority byte
hashes differ. Causal edit samples therefore were not copied from one
byte-identical prepared base. This is a benchmark-orchestration defect, not an
engine identity, publication, resource, or algorithm failure.

## Directional measurements only

These medians are retained to diagnose the attempt but are not accepted WP4-M
performance evidence:

| Operation | Size | Median publication |
|---|---:|---:|
| full write | 1 MiB | 7.527875 ms |
| full write | 10 MiB | 68.334125 ms |
| full write | 100 MiB | 618.917000 ms |
| same-count middle edit | 100 MiB | 15.185083 ms |
| `+1` early | 100 MiB | 431.455791 ms |
| `+1` middle | 100 MiB | 438.025708 ms |

The nonmedian 100-MiB complete-roundtrip check was 1,299.848542 ms. The
former 5% `+1` diagnostic remains nonbinding under the prospective amendment:
`69.711414%` early and `70.772932%` middle. Count-changing work remains honest
suffix-linear work; no logarithmic claim is made.

## Decision

Decision: `REVISE`

Prepare one master database plus its inseparable authority and expectation
sidecars per arm, copy those exact bytes for every sample, verify the copy
hashes, and rerun once as CP-0006. Do not weaken the analyzers or relabel this
attempt. WP4-M is not closed and WP4-P is not eligible from CP-0005.
