# Phase 4 G5-0 terminal report — H11 v9

Disposition: **PASS — qualifying corrected H11 whole-harness authority**.

The sealed v9 gate contains eight fresh rows for the balanced deterministic
1-MiB `N=1/10/100/1,000` schedule. Complete wall from fail-fast lock acquisition
through terminal-verification fsync was `9,254,244,292 ns`, below the frozen
`20,000,000,000 ns` limit. The result root is
`target/phase4-g5-foundation-h11-20260823-v9/`.

## Independent terminal audit

Three fresh read-only lanes independently returned PASS:

- source/correctness traced borrowed process arguments, every whole-harness Q
  owner and drop order, requested-ObjectId authentication, historical tuple
  reconstruction, report fixed-point charging, and the terminal Q marker;
- performance recomputed all eight rows from raw JSON, including identity,
  exact work classes, history/storage equations, timers, protected latency,
  Q/RSS/buffers, and cleanup; and
- custody rehashed all 50 frozen method rows, the executable and inputs, every
  32-file payload entry, every 38-file final-manifest entry, screen/gate
  chronology, terminal verification, and lock inode/token release.

The primary and separately implemented analyzers both report PASS and their
normalized payloads agree exactly.

## Hard results

| Metric | Result |
|---|---:|
| Current-live graph | `58 objects / 1,051,574 canonical bytes / 2,255 mapping bytes` |
| Per-unique-revision storage | `6 objects / 23,030 canonical bytes / 2,255 mapping bytes` |
| SQLite logical/apparent slope | `24,858.9069 bytes/revision` |
| Whole-harness Q high-water | `691,675–705,901 bytes` |
| Whole-harness terminal Q | `0` in every row |
| Maximum RSS | `14,090,240 bytes` (`<20,971,520`) |
| Maximum owned buffer | `1,048,576 bytes` |
| Descriptors/permits/temp/journal/seed/work-root residue | `0` |
| Final manifest | `38/38` byte/hash verified |

All final and selected historical root/transition/output tuples match the
1,001-row frozen oracle. Each non-genesis history edit contributes exactly six
objects, 23,030 canonical bytes, 2,309 mapping-rewrite work bytes, and one
transaction/COMMIT. Read operations remain write-free. First-edit latency has
no material regression under the dual rule: candidate/control `1.07780`, but
the two-sample sum delta is `+546,334 ns`, below `+2,000,000 ns`.

## Custody hashes

- source freeze: `7d3ff760e62e477c3eac083797c7798f9b01d54324a4b1b1b0ff46cfff459d52`
- explicit-source aggregate: `15e3dec66593f2471253f362b69032f62edf537b46ed5874c5a9d16bf8be8b7d`
- tracked diff: `47296e03cfa5256a0b9448589f25e98958918155d683b3d7722999df06b4d775`
- release executable: `83c472b3171290e087c2e647a5ecfbafcaa1613938ce006073dd8c41fdafde6f`
- raw: `1dc9b26c7fe79d39b0ec79ab8b915296e7ac09567d71511d44626edb0d997e53`
- payload manifest: `852fa3ff060761f8a183bad15faa56edd9ebb1dbbb82504f2eda0b25b067e9f6`
- measured terminal: `3809fc78cc286c2df23ac9f25a62c887b1db80dbe44e166ac78525884c510e89`
- final manifest: `c2cc2ba826c2c8eb3fdf2589280149256e1cbe3a535fd5562fd39e715fd4f64d`
- final verification: `3e37657c9f584938e53e4543d79938ef1ed8a0960d59f83af0c1020487615bfd`

## Preserved repair history and limits

V1–v8 remain preserved as failures or superseded diagnostics: v1 analyzer
protocol; v2 incomplete Q/lock authority; v3 clippy; v4 analyzer path; v5
child schema; v6 historical allocation; v7 focused compile; v8 argv ownership
and drop order. V9 neither copies their rows nor relabels their terminals.

The operation log is not execution authority. Physical I/O and controlled-cold
state remain Unavailable. The storage slope is an H11 diagnostic, not a
population claim. History remains append-only; no GC is authorized. Rollback
freshness remains `NotProtected` without external monotonic authority.

G5-1 may begin. G6 is not yet eligible.
