# Phase-4 current benchmark scoreboard

Status: **Phase 4 active — FastCDC v2 retained**

Date: 2026-08-21
Current executable: `454bc2f3deacd8581a3cc352c8b7495215cdc103a85580606246ea12bb25eba8`
Current profile: `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b`

## Headline results

| Metric | Current result | Status |
|---|---:|---|
| Durable 100-MiB full create | **332.028 ms / 301.180 MiB/s** | fresh FastCDC confirmation |
| Writer peak RSS | **89.13 MiB** | too high; next screen |
| Same-open 100-MiB same-count edit | **6.961 ms** | last Canonical-v2 lifecycle evidence |
| Same-open 100-MiB `+1` early / middle | **5.108 / 4.576 ms** | last Canonical-v2 lifecycle evidence |
| One-byte early / middle / late | **6.410 / 6.415 / 6.725 ms** | last Canonical-v2 guards |
| Warm authenticated 100-MiB reconstruction | **338.776 ms / 295.180 MiB/s** | full validation; not incremental |
| Fresh-process authenticated reconstruction | **366.357 ms / 272.958 MiB/s** | OS cache warm-or-unknown |
| Proven cold native materialization | **Unavailable** | benchmark missing |
| Trusted hot read | **Unavailable** | implementation missing |
| Incremental materialization | **Unavailable** | next materialization candidate |
| First edit after reopen | **154.019 ms** | authority work remains |
| Reopen / visible head | **2.088 ms** | retained |
| Authenticated returned 1-MiB range | **2.279 ms / 438.749 MiB/s** | retained |

Only the full-create row is freshly remeasured after FastCDC v2. Other rows
remain the latest Canonical-v2 lifecycle authority until the compact matrix is
refreshed.

## Public `layerfs` comparison

| Comparable metric | This implementation | Public README M3 | Interpretation |
|---|---:|---:|---|
| Durable/cold 100-MiB write | **301.180 MiB/s** | 60.0 MiB/s | different engine/host; ours faster in retained cells |
| Full 100-MiB materialization | **295.180 MiB/s warm authenticated** | 108.5 MiB/s | ours performs full authenticated logical reconstruction |
| First/cold-engine read | 272.958 MiB/s fresh-process | 259.6 MiB/s | neither proves cold OS/device state |
| Trusted hot read | **Unavailable** | 2,921.5 MiB/s | public path uses whole-fixture cache trust |

The public M3 hot profile permits a 128-MiB content cache, 128-MiB SQLite page
cache, and 192-MiB managed-resident envelope. Its `cold` read is cold engine
cache, not cold OS cache. Do not compare its cache-trust read with our complete
reauthentication path as if they were the same operation.

## Test matrix

Legend: `P` primary measured cell, `S` one-sample smoke, `D` diagnostic,
`—` not scheduled.

| Group | Operation | 1 MiB | 10 MiB | 100 MiB | 500 MiB deferred | Primary metric |
|---|---|:---:|:---:|:---:|:---:|---|
| A1 | Durable fresh full create | S | S | **P** | — | wall, MiB/s, COMMIT, RSS |
| A2 | Identical full rewrite / CAS reuse | S | S | **P** | — | created/reused work, wall |
| B1 | Authenticated logical reconstruction | S | S | **P** | — | wall, BLOB/hash phases |
| B2 | Fresh-process reconstruction | S | S | **P** | — | reopen + reconstruction |
| B3 | Proven cold native materialization | S | S | **P** | — | read/write/fsync wall |
| B4 | Trusted hot full read | S | S | **P** | — | wall, MiB/s, cache bytes |
| B5 | 4-KiB random / returned 1-MiB range | S | S | **P** | — | latency, returned bytes |
| C1 | Same-count one-byte early/middle/late | S | S | **P** | — | edit publication latency |
| C2 | `+1/-1` early/middle/late | S | S | **P** | — | suffix work and latency |
| C3 | 100 scattered one-byte edits | — | S | **P** | — | total, median, storage |
| D1 | Receipt-valid no-op materialization | S | S | **P** | — | receipt wall, zero writes |
| D2 | Same-size one-byte incremental update | S | S | **P** | — | changed ranges/bytes, fsync |
| D3 | Same-size 1-MiB replacement | S | S | **P** | — | changed ranges/bytes, fsync |
| D4 | Count-changing incremental update | S | S | **P** | — | shifted suffix, allocation |
| D5 | Invalid receipt / external mutation | S | S | **P** | — | exact full fallback |
| E1 | Reopen / visible head | S | S | **P** | — | process-open boundary |
| E2 | First edit after reopen | S | S | **P** | — | authority + edit lifecycle |
| E3 | Clone/patch/fsync/rename/receipt faults | — | S | **P** | — | atomicity and cleanup |
| F1 | Long history: 10/100/1,000 revisions | — | S | **P** | — | plateau, direct-base plan |
| F2 | 1/2/4 independent active stores | — | S | **P** | — | aggregate RSS and wall |

The 500-MiB column is explicitly out of scope. CP-0008 remains the historical
scale evidence; no new 500-MiB fixture, smoke, diagnostic, fallback, or
acceptance cell may run without a later explicit authorization.

## Phase-4 grind execution phases

| Grind phase | Scope | Time budget | Exit condition |
|---|---|---:|---|
| **G0 — freeze** | checkpoint FastCDC v2, CP-0010, scoreboard, exact control | no timing | clean retained baseline |
| **G1 — writer memory** | one `cache_spill=2000` A/B screen; select or reject explicit policy | `<20 s` screen | memory policy closed |
| **G2 — materialization research** | decompose SQLite/hash/output wall; freeze receipt and mutation authority | `<20 s` diagnostic + static tests | exactly one candidate selected |
| **G3 — incremental prototype** | no-op, same-size one-byte, 1-MiB replacement, invalid-receipt/fault fallback | `<20 s` screen | retain/revert same-size mechanism |
| **G4 — materialization acceptance** | compact 1/10/100 matrix; native cold, trusted hot, incremental, fallbacks | `<=120 s` total | materialization baseline frozen |
| **G5 — remaining core lanes** | reopen authority, count-change locality, optional sub-300-ms create work | separate one-variable screens | each lane closed or explicitly deferred |
| **G6 — Phase-4 closure** | final scoreboard, manifests, limitations, WP5 handoff | no new candidate | Phase 4 PASS or explicit blockers |

Within a grind phase, measured candidates remain serial and one-variable. A
phase groups decisions; it does not authorize stacking its experiments into
one implementation. Do not run G4 when G3 fails, and do not run any 500-MiB
cell.

## Incremental materialization gates

| Workload | Hard correctness gate | First performance signal |
|---|---|---|
| No-op | receipt/root exact; zero destination writes | materially below full reconstruction |
| One-byte same-size | exact bytes/metadata; atomic publication | changed-range work; no full BLOB replay |
| 1-MiB replacement | exact bytes/metadata; bounded Q/RSS | time tracks changed bytes, not file size |
| `+1/-1` | exact suffix and root; honest allocation | report suffix scaling; no constant-time claim |
| Invalid receipt | exact typed fallback; no trust laundering | full fallback within protected regression bound |
| Crash/fault | old or new destination only; cleanup exact | no performance gate |

Performance targets remain planning targets until prospectively frozen. A
same-size incremental candidate should first demonstrate at least a clear
multi-fold improvement over full reconstruction; do not reject a correct first
prototype merely for missing a speculative single-digit-millisecond target.

## Current decision

```text
RETAIN FastCDC v2
CURRENT G0 — freeze/checkpoint
NEXT G1 — writer-memory screen
THEN G2 — materialization decomposition/authority
THEN G3 — same-size incremental materialization
DEFER create concurrency until a materially sub-300-ms target is selected
KEEP count-change locality and reopen authority as separate lanes
```

Detailed evidence and qualifications:
[CP-0010](../test-checkpoint-report/cp-0010-dirty-72ed9fee8e6a-fastcdc-v2-phase4-grind.md).
