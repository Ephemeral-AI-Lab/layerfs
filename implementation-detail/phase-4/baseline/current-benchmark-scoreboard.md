# Phase-4 current benchmark scoreboard

Status: **Phase 4 active — G0–G3 complete; G3 PASS / G4 READY — v13
STATICALLY CLOSED AND TERMINALLY SEALED; G4 planning-only and UNSTARTED**

Date: 2026-08-22
Current executable: `42e3ddeb15df298c978b14639690e366fbb26ee55851524d42c6c3e9c0e8bd55`
Current profile: `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b`

## Headline results

| Metric | Current result | Status |
|---|---:|---|
| Durable 100-MiB full create | **308.884 ms / 323.746 MiB/s** | fresh G1 retained policy |
| Writer peak RSS | **12.48 MiB** | G1 PASS; 86.005% lower |
| SQLite cache snapshot maximum | **8.35 MiB** | G1 PASS; 89.944% lower |
| Same-open 100-MiB same-count edit | **6.961 ms** | last Canonical-v2 lifecycle evidence |
| Same-open 100-MiB `+1` early / middle | **5.108 / 4.576 ms** | last Canonical-v2 lifecycle evidence |
| One-byte early / middle / late | **6.410 / 6.415 / 6.725 ms** | last Canonical-v2 guards |
| Warm authenticated 100-MiB reconstruction | **338.776 ms / 295.180 MiB/s** | full validation; not incremental |
| Fresh-process authenticated reconstruction | **366.357 ms / 272.958 MiB/s** | OS cache warm-or-unknown |
| Proven cold native materialization | **Unavailable** | benchmark missing |
| Trusted hot read | **Unavailable** | implementation missing |
| 100-MiB one-byte incremental materialization | **3.414166 ms** | once-only v13 mechanism screen; not a median or acceptance result |
| First edit after reopen | **154.019 ms** | authority work remains |
| Reopen / visible head | **2.088 ms** | retained |
| Authenticated returned 1-MiB range | **2.279 ms / 438.749 MiB/s** | retained |

Only the full-create row is freshly remeasured after the G1 runtime policy.
The incremental row is the once-only, benchmark-private G3-v13 mechanism
screen, not a median, proven cold-I/O result, production integration, or G4
acceptance baseline. Other rows remain the latest Canonical-v2 lifecycle
authority until the compact matrix is refreshed. The v13 mechanism executable
is `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`;
its [sealed terminal](../../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-v13.json)
is separate from the accepted product executable above.

## Public `layerfs` comparison

| Comparable metric | This implementation | Public README M3 | Interpretation |
|---|---:|---:|---|
| Durable/cold 100-MiB write | **323.746 MiB/s** | 60.0 MiB/s | different engine/host; ours faster in retained cells |
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
| **G0 — freeze — COMPLETE** | checkpoint FastCDC v2, CP-0010, scoreboard, exact control | no timing | clean retained baseline |
| **G1 — writer memory — COMPLETE** | retained `cache_spill=2000`; 89.944% cache and 86.005% RSS reduction | `6.864 s` screen | memory policy closed |
| **G2 — materialization research — COMPLETE** | decomposition selected destination-authority-gated incremental materialization | `<20 s` diagnostic + static tests | selected one G3 mechanism |
| **G3 — incremental prototype — COMPLETE** | protected seed/clone/patch, exact fallback, faults, authority and evidence closure | `<20 s` operation-sum screen | v13 statically closed and terminally sealed |
| **G4 — materialization acceptance — READY / UNSTARTED** | compact 1/10/100 matrix; native cold, trusted hot, incremental, fallbacks | `<=120 s` total | materialization baseline frozen |
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
RETAIN FastCDC v2 + SQLite cache_spill=2000
COMPLETE G0 — freeze/checkpoint
COMPLETE G1 — writer-memory policy
COMPLETE G2 — materialization decomposition/authority
COMPLETE G3 — v13 protected-seed incremental mechanism
READY G4 — planning only; measured execution UNSTARTED
DEFER create concurrency until a materially sub-300-ms target is selected
KEEP count-change locality and reopen authority as separate lanes
```

Detailed evidence and qualifications:
[CP-0011](../test-checkpoint-report/cp-0011-dirty-3e167cdcdc26-sqlite-writer-memory.md).
