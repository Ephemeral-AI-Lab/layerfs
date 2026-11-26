# #818 M1 benchmark (merged tip `cfc9586d05`)

> **Status:** Measured receipt, one sample per case and arm. M1 = R1, R2, R3; W1, W2, W3, W5; I1, I2a, I2b. Comparison base: Phase 0 probe logs (`receipts/P0/`, equal to `056e920166` on every `fs.*` row). Run by a benchmark subagent; saved by the orchestrator from its report text.

## Identity

| Field | Value |
| --- | --- |
| Source | branch `claude/rt818-rollout-c3dbae`, HEAD `cfc9586d05` |
| Harness | `tests/fs_call_round_trips.rs` (blob `8b52013d39`), `tests/support/round_trips.rs` (blob `5bf8a0f512`), unchanged since `8a8fcea616` |
| Build | `cargo +1.85.1 test --locked --offline -j 6 -p layerfs-store-service --test fs_call_round_trips --no-run` (test profile) and with `--release` |
| Binaries | test `fs_call_round_trips-9eab8e660a198019` sha256 `37ffea395b869d2f…`; release `fs_call_round_trips-5ae2897f0c20b069` sha256 `db76b87011816739…`; diagnosis `sdk_history_round_trips-1431b882328fa2ee` sha256 `20e71c302806df4e…` |
| PostgreSQL | container `lfs818-bench-pg`, `postgres:17` (`sha256:ff87138949ad…`), 17.11 aarch64, port 61949, trust, db `rt818_bench`, `synchronous_commit=off`, `pg_stat_statements` (track=all; harness counts `toplevel` of the current db) |
| Delay | netem in the container namespace through the probe helper (`FS818_NETEM_CONTAINER=lfs818-bench-pg`); after each of the 10 delay commands `tc qdisc show dev eth0` = `qdisc noqueue 0: root` |
| Calibration (`SELECT 1` median of 200) | local 0.26–0.37 ms; +10: 12.68 (reads), 13.24 (writes), 13.61 (history), 11.51–11.69 (Init); +2: 3.42 (blame), 3.02 (revision.h100), 3.04 (revision.h10/h20) |
| Host | macOS 26.4.1 arm64; Docker 8 CPU / 7.75 GiB; 1-min load 4.2–6.5; host free 238 GB before release build |
| Commands | `run_bounded.py --limit 120 -- /usr/bin/time -l <bin> <test> --exact --test-threads=1 --nocapture` with `FS818_ARM/GROUP/CASES`; all exit 0 |
| Cache state | probe: each `fs.*` case on the Store open that wrote the setup (≈ pin `warm`); Init a new Store per case |
| Receipts | `bench-sqlite.log`, `bench-pg.log`, `bench-pg10.log`, `bench-pg2.log` (test profile); `bench-init-release.log` (release, all arms); `diag-sdk_history_round_trips.log` |

Trips = PG client round trips, all roles; SQLite = client statements; "P0 → M1". Expectation = SPEC §6.3 where listed, else the lanes' measured item deltas applied to P0. Latency: added = (+delay ms − PG local ms) per trip vs that command's calibration median; pass ≤1.10×. Mem = command peak footprint MiB, SQLite/PG/PG+delay.

### Reads (test profile)

| Case | P0 | Expect. | M1 | SQLite stmts | SQLite ms | PG local ms | PG +10 ms | added/trip (×cal) | Mem | Verdict |
|---|---:|---:|---:|---|---|---|---|---|---|---|
| stat.depth3 | 50 | 22 SPEC | **28** | 95→36 | 1.75→0.58 | 22.0→13.2 | 677→401 | 13.86 (1.09) | 14.9/14.1/14.9 | **miss +27%**: group's first call, control `[0,28,20,0]` 8 prepares (P0 3); setup writes no longer use the control reader (W2). Within cold target 31 (P0 cold 59 − probe 50 = +9) |
| stat.depth7 | 73 | ~34 SPEC | **17** | 152→31 | 3.55→0.45 | 32.2→11.5 | 1029→234 | 13.09 (1.03) | same | better by 50% (R3: walk ~0 per component when cached; SPEC midpoint conservative) |
| stat.root | 18 | 8 SPEC | 8 | 39→22 | 0.43→0.26 | 9.6→4.2 | 256→107 | 12.79 (1.01) | same | exact |
| read.small_depth3 | 38 | 10 SPEC | 10 | 83→24 | 1.74→0.40 | 17.9→5.7 | 514→145 | 13.89 (1.10) | same | exact |
| read.2000_lines | 32 | 11 | 11 | 68→25 | 1.47→0.51 | 15.1→5.7 | 422→155 | 13.60 (1.07) | same | exact |
| read.lines_100_200 | 30 | 9 | 9 | 66→23 | 1.26→0.36 | 13.3→4.5 | 407→115 | 12.30 (0.97) | same | exact |
| list.dir_100 | 33 | 12 SPEC | 12 | 69→26 | 1.55→0.68 | 15.2→6.0 | 436→156 | 12.53 (0.99) | same | exact |
| list.root | 18 | 8 | 8 | 39→22 | 0.46→0.26 | 6.6→3.7 | 243→110 | 13.33 (1.05) | same | exact |
| glob.dir_100 | 30 | 9 | 9 | 66→23 | 1.14→0.46 | 13.4→4.8 | 422→115 | 12.20 (0.96) | same | exact |
| glob.tree | 23 | 13 | 13 | 44→27 | 0.79→0.52 | 8.9→6.5 | 321→185 | 13.70 (1.08) | same | exact |
| grep.one_file_dir | 23 | 9 | 9 | 49→23 | 0.70→0.31 | 10.2→5.0 | 320→119 | 12.72 (1.00) | same | exact |
| grep.tree_1k | 78 | 68 SPEC | 67 | 131→114 | 8.69→8.63 | 45.0→45.3 | 1120→912 | 12.93 (1.02) | same | within (−1.5%) |
| diff_files.h10_vs_h99 | 88 | 82 (R1 −4, R2 −2) | 87 | 198→194 | 3.66→3.30 | 39.7→42.1 | 1287→1176 | 13.04 (1.03) | same | within (+6%): 2 prepares + 3 object reads |

### History (test profile)

| Case | P0 | Expect. | M1 | SQLite stmts | SQLite ms | PG local ms | PG +delay ms | added/trip (×cal) | Mem | Verdict |
|---|---:|---:|---:|---|---|---|---|---|---|---|
| blame.h10 | 267 | 263 | 281 | 714→719 | 12.5→13.1 | 98.9→148.8 | +10 3598→3960; +2 1235→1215 | +10 13.56 (1.00); +2 3.79 (**1.11**) | 14.9/13.5/13.8 | within (+6.8%): first call of group, 9 prepares (P0 4) + 11 object reads; pins at tip cold 283 / warm 242 |
| blame.h100 | 2,325 | 2,319 | 2,428 | 7,575→7,658 | 111.0→119.6 | 1044→1261 | +10 NOT_RUN (proj. 34.3 s); +2 10015→11008 | +2 4.01 (**1.18**) | 14.9/13.5/14.4 | within (+4.7%) but above P0 (+4.4%): control stmts 2,121→2,228 (object reads; SQLite +83). Pins at tip unchanged (PG warm 2,223 / cold 2,464; SQLite 7,343 / 7,699): code saves the predicted 6; probe's starting cache now near cold since setup writes skip the control reader (inferred) |
| revision.h100 | 3,890 | 3,886 | 3,887 | 12,658→12,658 | 195.4→203.9 | 1849→2278 | +10 NOT_RUN (proj. 55.2 s); +2 alone, first in command, 4,113 trips (P0 3,999): 17578→14871 | +2 3.06 (1.01) | 14.9/13.5/15.0 | exact |
| blame.h20 | 466 | 462 | 462 | 1,348→1,342 | 22.2→23.7 | 215.9→286.0 | +2 1905→2194 | +2 4.13 (**1.21**) | 14.9/13.5/14.4 | exact |
| revision.h10 | 468 | 462 | 462 | 1,314→1,314 | 22.7→23.7 | 221.3→307.8 | +10 (filtered, 464 trips) 6809→6277; +2 (502, first) 2222→1815 | +10 12.86 (0.95); +2 3.00 (0.99) | 14.9/13.5/13.8 | exact |
| revision.h20 | 888 | 882 | 882 | 2,602→2,602 | 44.2→46.1 | 405.5→572.1 | +2 (907) 4169→3152 | +2 2.84 (0.94) | 14.9/13.5/14.3 | exact |

Over budget at +10 ms (D14), as at P0: blame.h100 and revision.h100, run at +2 ms in three commands (blame cases 21.8 s, revision.h100 alone 22.6 s, revision.h10/h20 12.4 s); labelled. PG local history times are 20–40% above P0 at equal trips; the same 3,886-trip pin read 1,786–2,412 ms across P0/R1/R3, so this is run-to-run spread.

### Writes (test profile)

Lane deltas: control reader 10→0 (W1 −6, W2 −4); writer −1 (W3); rm.dir Workspace reader 107→8 (W5).

| Case | P0 | Expect. | M1 | SQLite stmts | SQLite ms | PG local ms | PG +10 ms | added/trip (×cal) | Mem | Verdict |
|---|---:|---:|---:|---|---|---|---|---|---|---|
| write.new_file | 66 | 57 SPEC (lanes 55) | 56 | 77→60 | 3.67→4.52 | 30.9→35.5 | 909→810 | 13.82 (1.04) | 15.7/14.5/14.4 | within (−2%) |
| write.overwrite | 24 | 15 SPEC (lanes 13) | 13 | 54→37 | 2.93→3.55 | 14.4→12.7 | 318→188 | 13.46 (1.02) | same | within (−13%), equals lane sum |
| edit.2000_lines | 23 | 12 | 12 | 52→35 | 2.75→3.21 | 14.7→12.7 | 315→172 | 13.25 (1.00) | same | exact |
| mkdir | 45 | 34 | 34 | 69→52 | 3.36→3.91 | 19.1→21.9 | 615→550 | 15.54 (**1.17**) | same | trips exact; latency flag: +78 ms (≈6 delays), server top-level 67 vs cp 66 (cp 0.99×); not explained by counts |
| cp.file | 44 | 33 | 33 | 64→47 | 3.24→4.21 | 20.3→20.6 | 594→453 | 13.10 (0.99) | same | exact |
| mv.file | 40 | 29 | 29 | 60→43 | 2.84→2.98 | 23.4→19.6 | 532→417 | 13.72 (1.04) | same | exact |
| rm.file | 37 | 26 | 26 | 58→41 | 2.84→3.23 | 16.8→19.5 | 490→350 | 12.73 (0.96) | same | exact |
| apply.10_writes | 46 | 35 | 35 | 74→57 | 5.25→6.04 | 24.4→29.0 | 613→497 | 13.37 (1.01) | same | exact |
| rm.dir_100_recursive | 151 | ~42 SPEC (lanes 41) | 41 | 179→63 | 12.54→12.66 | 73.7→36.0 | 2142→638 | 14.67 (**1.11**) | same | within (−2%) |

SQLite write times +0.1–1.0 ms (under the 5 ms floor).

### Init (release, D15)

| Case | P0 | Expect. | M1 (server top-level) | SQLite stmts | SQLite ms | PG local ms | PG +10 ms | added/trip (×cal) | Mem | Verdict |
|---|---:|---:|---:|---|---|---|---|---|---|---|
| nested_1k | 229 | ~205 (I1) | 209 (343) | 308→288 | 50.5→36.7 | 409.5→336.4 | 2944 (P0 test-profile 3414) | 12.47 (1.07) | 12.0/11.8/12.0 | within (+2%) |
| flat_1k | 2,246 | ~226 SPEC | 228 (379) | 2,284→360 | 52.7→36.7 | 1016.3→354.6 | 3462 (P0 over budget; +2 9457) | 13.63 (**1.18**) | 11.9/12.8/12.2 | within (+0.9%); latency flag +483 ms |
| nested_10k | 10,952 | ~606 SPEC | 624 (1,138) | 12,169→1,837 | 491.5→271.9 | 9152.9→5953.3 | 14243 (P0 NOT_RUN ≈141 s) | 13.28 (**1.15**) | 23.9/27.1/24.8 | within (+3%); excess 1.1 s, below the one-sample spread of the 6 s local term |
| flat_10k | 31,042 | ~706 SPEC | 726 (1,343) | 31,882→2,502 | 510.0→**563.8** | 16202.2→6429.1 | 15162 (P0 NOT_RUN ≈386 s) | 12.03 (1.04) | 23.4/25.3/28.8 | within (+2.8%); SQLite time +10.5% |

All Init rows now run at +10 ms (projection local + trips × 11.65 ms: 3.8 / 3.0 / 13.2 / 14.9 s). Commands took 16.3 / 14.1 / 32.8 / 30.6 s; the extra is source write, two docker+apk netem calls and three 200× `SELECT 1` calibrations (≈7 s at +10 ms). `logical == walked` in every Init; canonical bytes equal P0. PG local no longer follows trips (nested_10k 5.95 s on 624 trips).

## Answers and outcomes

Every outcome kind equals P0 on every arm (ok / Text / Committed); no refusals. Setup counts moved, as M1 expects (not measured cases): setup Init 286→266 PG trips (SQLite 369→349); the 100 setup writes 2,137→1,032 PG trips (SQLite 5,587→3,890).

## Flags

- 15% misses: stat.depth3 +27% (within its cold target); stat.depth7 −50% (better). 22 of 32 rows exact.
- The probe's first history calls (blame.h10, blame.h100) read more than at P0, although the pins at the tip show the predicted savings: the probe's starting state is colder since W1/W2.
- Latency band exceeded: +10 ms mkdir 1.17, rm.dir 1.11, Init flat_1k 1.18, nested_10k 1.15; +2 ms blame.h10 1.11, blame.h100 1.18, blame.h20 1.21. Client statements equal server top-level calls in every flagged row; the excess is time without statements. P0's spread was up to 1.18 at +2 ms and at most 1.09 at +10 ms. Attributing it needs a per-statement trace.
- SQLite init.flat_10k 510.0→563.8 ms (+53.8 ms) with statements −92%; not diagnosed here (sent to lane I).
- Memory: 13.5–15.7 MiB per fs command (test profile); Init 11.8–12.8 MiB at 1k, 23.4–28.8 MiB at 10k (release). No P0 memory receipt to compare.
