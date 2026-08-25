# Stage 1.1M Verified terminal closure

Disposition: `REVISE_NO_AUTHORIZED_OWNER`; `terminal_pass=false`.

Verified correctness is closed on current product source. Verified performance
is not a terminal PASS and is not relabeled as one.

## Current-source closure

The frozen release evaluator was built from clean commit
`0403ea7166b332c5ddcb7b6cf04f60a0610fd5db` with `cargo build --release
--locked -p layerfs-eval`. Its executable is 3,902,000 bytes, SHA-256
`347746fc4ec7e78654a1b041bbe97f2ec8945bb286e537df43de270d71a44d53`,
and BLAKE3
`1cb5b6b208d2a24ac94ffb43e4db30317c4082a300504c62c1f268119d06038b`.

The exact 47-row Stage 1.1 campaign is retained at
`target/layerfs-stage1-apple-edge-20260825-attempt-015`. The evaluator's
environment file records a command template; `regression-receipt.json` appends
the exact concrete build/prepare/readiness/run argv. It passed 47/47
rows, 51/51 edit/sub-edit operations, 34/34 durable transitions, 51/51
physical oracles, 34/34 canonical transition oracles, four save bursts and
eight selected-history checks. It observed three patch routes, twelve shift
routes, zero FullFallback, zero rematerialization, zero BUSY/LOCKED, and exact
bytes, metadata, history and refresh results. The fixture's exact hard-link
topology is empty (`payload.bin` has `nlink=1`; no multi-link files), so focused
workspace tests—not these 47 rows—remain the nonzero hard-link proof.

| Resource/equation | Observed | Disposition |
|---|---:|---|
| Complete wall = row wall + outside-row wall | `13,430,358,958 = 9,077,248,419 + 4,353,110,539 ns` | PASS |
| Product RSS | `28,770,304 B` | PASS |
| Largest buffer | `1,048,576 B` | PASS |
| Q high/terminal | `8,388,607 / 0 B` | PASS |
| Store connections high/terminal | `2 / 0` | PASS |
| FD baseline/terminal | `5 / 5` | PASS |
| Owned temp/sidecar residue | `0 / 0` | PASS |
| Phase fetched/auth/role | `74,236 / 74,236 / 74,236` | PASS |
| Publication tx/COMMIT/rollback | `34 / 34 / 0` | PASS |

The first parallel workspace test exposed a test-lifetime violation: the test
kept a TrustedLocalDev Store open while forcing a Verified reopen/scrub. The
small repair closes Trusted first. The aborted downstream pass also exposed
SDK assertions that still used `8 MiB` after the strict product Q cap became
`8 MiB - 1`; those assertions now use the exported product constant. Both
failures are preserved in `closure.json`, and the invalidated serial scopes,
all not-yet-reached scopes, workspace doctests, formatting and all-target
clippy pass.

## Frozen Verified performance result

| Size | p50 | p50 MiB/s | p95 | p95 MiB/s | Control p50/p95 | Disposition |
|---:|---:|---:|---:|---:|---:|---|
| 0 MiB | `24.071333 ms` | N/A | `24.648250 ms` | N/A | `33.493458 / 33.748916 ms` | measured |
| 24 MiB | `62.191459 ms` | `385.905` | `65.981500 ms` | `363.738` | `70.361208 / 70.787375 ms` | FAIL |
| 96 MiB | `179.337500 ms` | `535.304` | `183.878333 ms` | `522.084` | `189.636458 / 193.538916 ms` | PASS |

The fitted intercept is `23.142779 ms` and fitted sustained bandwidth is
`614.617 MiB/s`. The exact misses are `8.858126 ms` at 24 MiB p50,
`7.314500 ms` at 24 MiB p95, and `3.142779 ms` at the fixed-cost gate.

M7 removed exactly the redundant fresh-construction install-parent barrier.
Its direct owner saving is `4.330125 ms` at 24 MiB and `4.384750 ms` at
96 MiB; immediate live refresh and hard-link two-sync ordering remain intact.
All remaining >3 ms Apple sync owners protect distinct required mutations.
The Engine audit found no duplicate identity-authentication pass, and the
guarded-read route has a generous ceiling of only `2.612297 ms` at 24 MiB.

The M1 attempt-004 p95 excess of `0.489333 ms` remains labeled
`NONMATERIAL_MICROVARIANCE`; it remains a numerical miss. The accepted
attempt-014, `f3dd4a3`, all M2 failures, the M7 miss and the LTO regression
remain preserved.

## Boundary

Verified remains the default trust class. No additional Verified performance
optimization is authorized under the current 3 ms owner floor and frozen
trust/durability rules. Further work requires an explicit authority expansion
and must be reported as a separate product class, never as Verified.
