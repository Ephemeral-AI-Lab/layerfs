# Phase-4 current benchmark scoreboard

Status: **Phase 4 active — G0–G3 complete; G4 STAGE TERMINAL PASS under the
user-approved 1-ms absolute-regression materiality rule; v12 remains SEALED
TERMINAL REVISE under its frozen relative-only contract; stop before G5**

Date: 2026-08-22
Current executable: `e72988fc25e96f608d0d405e157ea8e837029595ace916f066932082a736db33`
Current normalized ledger: `dc563d339401b0e7cdf84b20f1a8da20c99b5f0da849c700e86dceaa9de546b1`

## Headline results

| Metric | Current result | Status |
|---|---:|---|
| Durable 100-MiB full create | **279.463 ms / 357.829 MiB/s** | G4-v12 adjacent gate PASS |
| Writer peak RSS | **12.48 MiB** | G1 PASS; 86.005% lower |
| SQLite cache snapshot maximum | **8.35 MiB** | G1 PASS; 89.944% lower |
| G4 campaign whole-child peak RSS | **19.625 MiB / 20,578,304 B** | G4-v12 RSS gate PASS |
| Same-open 100-MiB same-count edit | **8.043 ms** | G4-v12 exact <=5% adjacent gate PASS |
| Same-open 100-MiB `+1` early / middle | **5.108 / 4.576 ms** | last Canonical-v2 lifecycle evidence |
| One-byte early / middle / late | **6.410 / 6.415 / 6.725 ms** | last Canonical-v2 guards |
| Warm optimized authenticated 100-MiB reconstruction | **237.214 ms / 421.560 MiB/s** | G4-v12 R1 PASS; closure derived, not computed |
| Fresh-process authenticated reconstruction | **237.381 ms / 421.263 MiB/s** | G4-v12 PASS; OS cache warm-or-unknown |
| First/full native materialization, warm source | **307.652 ms / 325.042 MiB/s** | G4-v12 M0 durability gate PASS |
| Proven controlled-host-buffer-cold materialization | **Unavailable** | exclusive-host purge preconditions unavailable |
| Same-open protected-seed 100-MiB full read | **10.058 ms / 9,942.582 MiB/s** | G4-v12 PASS; byte delivery only; digest pass 83.018 ms |
| 100-MiB one-byte incremental materialization | **4.104 ms** | G4-v12 adjacent gate PASS |
| First edit after reopen | **154.019 ms** | authority work remains |
| Reopen / visible head | **3.583 ms** | G4-v12 exact <=5% adjacent gate PASS |
| Authenticated returned 1-MiB range | **2.046 ms / 488.823 MiB/s** | G4-v12 exact <=5% adjacent gate PASS |

G4-v12 is one prospective 30-record / 50-arm / 76-child acceptance campaign,
not a median campaign. Its complete wall is 91.262292709 seconds. The primary
and independent normalized ledgers agree at
`dc563d339401b0e7cdf84b20f1a8da20c99b5f0da849c700e86dceaa9de546b1`, and
the 271-entry measured payload manifest verifies. The balanced fixed
two-sample estimator retains the original exact
`candidate_sum <= control_sum * 1.05` equation for every protected route.
Sequence 17 (100-MiB clone/no-op) failed at **+8.5353%**, sequence 20 (1-MiB
count change) at **+6.7999%**, and sequence 26 (1-MiB pre-publication fault)
at **+14.3604%**. Their semantic/work counters match. The sealed v12 result
therefore remains `REVISE`; its old gate is not relabeled or recomputed.

The controlling [G4 stage terminal](../experiments/g4-materialization-acceptance/G4-STAGE-TERMINAL-v1.json)
does not relabel the old gate. For two samples per role, a product-material
regression requires both exact conditions: candidate sum times 100 greater
than control sum times 105, and candidate sum minus control sum at least
2,000,000 ns. The three delta numerators are only 452,458, 571,043, and
199,208 ns. All hard absolute and semantic/work/Q/cleanup/durability/resource/
custody gates remain mandatory and passed.

All other v12 gates passed: R1, M0, protected-seed read, whole-child RSS,
direct/static <=1-MiB buffer evidence, checked Q with zero terminal balance,
old-or-new durability including lost-acknowledgement handling, bucket limits,
source/operand custody, fsynced terminal verification, residue cleanup, and
owner-bound lock release attestation. Static closure is 166 passed, 1
intentionally ignored, and 0 failed. Cleanup is accepted only for the frozen
benchmark-private mode-0700/no-malicious-same-UID model; it is not a
categorical race-free production claim. The G3-v13 control remains separately
sealed at `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
G4 does not authorize VFS/SDK/product integration.

## Public `layerfs` comparison

| Comparable metric | This implementation | Public README M3 | Interpretation |
|---|---:|---:|---|
| Durable/cold 100-MiB write | **357.829 MiB/s** | 60.0 MiB/s | different engine/host; our v12 row includes durable publication |
| Full 100-MiB materialization | **421.560 MiB/s warm authenticated** | 108.5 MiB/s | ours performs full authenticated logical reconstruction |
| First/cold-engine read | 421.263 MiB/s fresh-process | 259.6 MiB/s | neither proves cold OS/device state |
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
| **G4 — materialization acceptance — STAGE TERMINAL PASS** | v12 source/static/resource/custody closure passed; three old-gate relative failures are non-material under the controlling 1-ms absolute-regression rule | `91.262292709 s` of `<=120 s` | accepted benchmark-private baseline; old gate did not pass |
| **G5 — remaining core lanes — NOT STARTED BY THIS TASK** | concurrent premature planning is foreign and excluded; no implementation or measurement was authorized here | no G4 execution authority | stop before G5; do not include foreign planning in G4 custody |
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
PRESERVE G4-v6 — measured numeric PASS, historical terminal REVISE
PRESERVE G4-v9 — internally strong MEASURED_PROTOCOL_REVISE
PRESERVE G4-v10 — aborted invalid execution; no evidence reused
PRESERVE G4-v11 — sealed REVISE
PRESERVE G4-v12 — sealed TERMINAL REVISE; sequences 17/20/26 exceed <=5%
PASS/CLOSE G4 under the controlling user-approved 1-ms absolute-regression rule
RETAIN G4-STAGE-TERMINAL-v1.json as the controlling audited stage terminal
DO NOT create or run v13; never reclassify v8-v12
DO NOT CLAIM the frozen v12 relative-only gate passed
STOP G4 optimization — no descriptor refactor and no unchanged-source rerun
STOP BEFORE G5 — this task authorizes no G5 implementation/measurement
EXCLUDE concurrent premature G5 planning as foreign shared-tree work
DEFER create concurrency until a materially sub-300-ms target is selected
KEEP count-change locality and reopen authority as separate lanes
```

Detailed evidence and qualifications:
[G4 baseline](g4-materialization-acceptance-baseline-v1.md),
[G4 report](../experiments/g4-materialization-acceptance/G4-REPORT.md), and
[CP-0011](../test-checkpoint-report/cp-0011-dirty-3e167cdcdc26-sqlite-writer-memory.md).
