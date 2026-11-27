# #818 M3 benchmark (merged tip `5af53d4667`) and final comparison

> **Status:** Measured receipt, one sample per case and arm. M3 = H4, H5 (with lane S per-object storage reads), H6; I3a, I3b (parts A and C), I3c (not I3d); W4 (W7 not built); review fixes. Comparison bases: M2 (`receipts/M2/`) for the milestone, `056e920166` (SPEC §2) for the final table. Run by the benchmark subagent; saved by the orchestrator from its report text. R6 is measured separately after M3.

## Identity

| Field | Value |
| --- | --- |
| Source | branch `claude/rt818-rollout-c3dbae`, HEAD `5af53d4667` |
| Harness | `tests/fs_call_round_trips.rs` unchanged; `tests/support/round_trips.rs` blob `a88f9abd09` (+2 lines since M2: `opened.prepare_sdk_writes()` at the probe's open, as `StoreService::open_postgres` does since W4; nothing on SQLite) |
| Build | `cargo +1.85.1 test --locked --offline -j 6 -p layerfs-store-service --test fs_call_round_trips --no-run`, and `--release` |
| Binaries | test `fs_call_round_trips-9eab8e660a198019` sha256 `698b33db584cdebc…`; release `fs_call_round_trips-5ae2897f0c20b069` sha256 `e75a74a4cc33ccf1…` |
| PostgreSQL | `lfs818-bench-pg`, postgres:17 (`sha256:ff87138949ad…`), 17.11 aarch64, port 61949, trust, db `rt818_bench`, `synchronous_commit=off`, `pg_stat_statements` (top-level, current db) |
| Delay | netem in the container namespace via the probe helper; after each of the 7 delay commands and at the end `tc qdisc show dev eth0` = `qdisc noqueue 0: root` |
| Calibration (`SELECT 1` median of 200) | local 0.30–0.38 ms; +10 ms: 13.80 (reads), 11.54 (writes), 11.02 (history), 11.01–11.06 (Init) |
| Host | macOS 26.4.1 arm64, Docker 8 CPU / 7.75 GiB; 1-min load 6.0–8.0; host free 273 GB |
| Commands | `run_bounded.py --limit 120 -- /usr/bin/time -l <bin> <test> --exact --test-threads=1 --nocapture`; no case filter; all exit 0 |
| Cache state | probe: each `fs.*` case on the Store open that wrote the setup; Init a new Store per case (release) |
| Receipts | `bench-sqlite.log`, `bench-pg.log`, `bench-pg10.log` (test profile); `bench-init-release.log` (release, all three arms) |

Rule (RECORD): probe ≈ warm, so the warm pin is the target; the cold pin is the cold target; the lanes' measured pins replace SPEC where they differ. Verdict: within 15% of the expectation, otherwise the cause from the per-role counts. Latency: (PG +10 − PG local) / trips vs the command's calibration median, pass ≤ 1.10×. Mem = command peak footprint MiB, SQLite / PG / PG +10.

## Milestone results

### Reads (test profile; no M3 read item; expectation = M2 probe)

| Case | M2 | SPEC §4 / §3 | M3 trips | SQLite stmts | SQLite ms | PG local ms | PG +10 ms | added/trip (×cal) | Mem | Verdict |
|---|---:|---|---:|---:|---:|---:|---:|---|---|---|
| stat.depth3 | 21 | 18 / ≤19 | 21 | 35 | 0.70 | 13.8 | 288 | 13.05 (0.95) | 14.8/17.2/19.7 | exact vs M2; group's first call (5 prepares); +17% vs SPEC, within cold target 28 |
| stat.depth7 | 13 | ~30 / ≤39 | 13 | 30 | 0.52 | 9.3 | 150 | 10.80 (0.78) | same | exact |
| stat.root | 4 | 4 / ≤4 | 4 | 21 | 0.28 | 3.0 | 47 | 11.00 (0.80) | same | exact |
| read.small_depth3 | 6 | 6 / ≤19 | 6 | 23 | 0.40 | 4.4 | 69 | 10.83 (0.79) | same | exact |
| read.2000_lines | 7 | 7 / ≤14 | 7 | 24 | 0.58 | 5.5 | 100 | 13.57 (0.98) | same | exact |
| read.lines_100_200 | 5 | 5 / ≤14 | 5 | 22 | 0.35 | 3.0 | 59 | 11.15 (0.81) | same | exact |
| list.dir_100 | 8 | 8 / ≤14 | 8 | 25 | 0.55 | 3.6 | 114 | 13.84 (1.00) | same | exact |
| list.root | 4 | 4 / ≤4 | 4 | 21 | 0.27 | 2.5 | 60 | 14.30 (1.04) | same | exact |
| glob.dir_100 | 5 | 5 / ≤14 | 5 | 22 | 0.44 | 3.0 | 76 | 14.58 (1.06) | same | exact |
| glob.tree | 9 | 9 | 9 | 26 | 0.60 | 8.4 | 101 | 10.31 (0.75) | same | exact |
| grep.one_file_dir | 5 | 5 / ≤9 | 5 | 22 | 0.41 | 4.9 | 57 | 10.32 (0.75) | same | exact |
| grep.tree_1k | 63 | 64 / ≤64 | 63 | 113 | 8.81 | 40.1 | 928 | 14.09 (1.02) | same | exact; goal met |

The reads command's calibration (13.80 ms) sits above its per-trip cost (10.3–14.6), so small-trip rows show 0.75–0.81: under the band, not a failure.

### History (test profile; all at +10 ms in one command, 15.8 s)

| Case | M2 | Pins M3 warm / cold | SPEC §4 / §3 | M3 trips | SQLite stmts | SQLite ms | PG local ms | PG +10 ms | added/trip (×cal) | Mem | Verdict |
|---|---:|---|---|---:|---:|---:|---:|---:|---|---|---|
| diff_files.h10_vs_h99 | 35 | 22 / 48 | ~12 / ≤15 | 35 | 78 | 1.13 | 18.5 | 401 | 10.92 (0.79) | 14.8/17.2/19.7 | within cold target (5 prepares). SPEC miss: per-side Commit row ×4, markers, Branch rows; follow-up recorded, not built |
| blame.h10 | 70 | 12 / 38 | ~17 / ≤18 | 37 | 132 | 3.99 | 19.1 | 416 | 10.73 (0.97) | 15.3/19.0/20.9 | at cold pin (group's first call, 9 prepares); warm pin meets SPEC |
| blame.h100 | 459 | 22 / 65 | ~29 / ≤38 | 51 | 1,094 | 35.9 | 50.6 | 632 | 11.39 (1.03) | same | between pins; warm pin meets SPEC; probe +34% over §3 target, within cold target |
| revision.h100 | 256 | 48 / 92 | ~30 / ≤38 | 52 | 769 | 33.7 | 46.5 | 831 | 15.09 (**1.37**) | same | within 15% of warm (+8%); SPEC miss: Layer-chain rows not windowed (recorded, not built) |
| blame.h20 | 60 | — | — | 15 | 167 | 7.53 | 14.4 | 252 | 15.81 (**1.43**) | same | unpinned |
| revision.h10 | 41 | — / 52 | ≤18 | 23 | 106 | 3.80 | 13.9 | 344 | 14.37 (**1.30**) | same | better than cold pin |
| revision.h20 | 70 | — | — | 32 | 188 | 7.39 | 19.1 | 520 | 15.66 (**1.42**) | same | unpinned |

SQLite history statements unchanged from M2 (H4–H6 are PostgreSQL only).

### Writes (test profile)

| Case | M2 | Lane exp. | SPEC §4 / §3 | M3 trips | SQLite stmts | SQLite ms | PG local ms | PG +10 ms | added/trip (×cal) | Mem | Verdict |
|---|---:|---:|---|---:|---:|---:|---:|---:|---|---|---|
| write.new_file | 56 | 40 (pins 33 / 80) | ~40 / ≤40 | 42 | 60 | 3.88 | 27.5 | 551 | 12.47 (1.08) | 15.1/18.5/19.3 | within (+5%); first create on this open |
| write.overwrite | 13 | 13 | ~12 / ≤12 | 13 | 37 | 3.00 | 12.4 | 216 | 15.70 (**1.36**) | same | exact; +1 vs SPEC (warm pin 11 meets) |
| edit.2000_lines | 12 | 12 | ~12 | 12 | 35 | 2.89 | 11.1 | 187 | 14.65 (**1.27**) | same | exact |
| mkdir | 34 | 34 | ~26 / ≤26 | 34 | 52 | 3.19 | 17.8 | 401 | 11.27 (0.98) | same | exact vs lane; **SPEC miss +31%** |
| cp.file | 32 | 32 | ~25 / ≤26 | 32 | 47 | 3.44 | 18.3 | 369 | 10.97 (0.95) | same | exact; SPEC miss +28% |
| mv.file | 29 | 29 | ~23 / ≤26 | 29 | 43 | 2.44 | 18.6 | 333 | 10.85 (0.94) | same | exact; SPEC miss +26% |
| rm.file | 26 | 26 | ~21 / ≤26 | 26 | 41 | 2.99 | 15.8 | 298 | 10.86 (0.94) | same | exact; §3 goal met, §4 +24% |
| apply.10_writes | 35 | 35 | ~27 | 35 | 57 | 5.89 | 22.9 | 407 | 10.99 (0.95) | same | exact; SPEC miss +30% |
| rm.dir_100_recursive | 41 | 41 | ~36 / ≤50 | 41 | 63 | 18.78 | 33.9 | 476 | 10.79 (0.93) | same | exact; §3 goal met |

Cause of the SPEC write misses: the writer carries 24–28 trips on the non-fused route (head, bindings, validate, lane, finish 7, publish 9). SPEC §4's write savings assumed W4/W6/W7 on every call; measured, W4 helps only the first create per open, W6 0–3, W7 not built (upper bound 1). Taking the fused route for create/mkdir/cp/mv/rm (the SDK staging limit) is an owner deferral. SQLite write statements unchanged since M1.

### Init (release, D15)

| Case | P0 release trips | Pin M3 | SPEC M3/§4 | M3 PG trips (server top-level) | SQLite stmts | SQLite ms | PG local ms | PG +10 ms | added/trip (×cal) | Mem | Verdict |
|---|---:|---:|---|---:|---:|---:|---:|---:|---|---|---|
| nested_1k | 229 | 93 | ~140 | 93 (238) | 288 | 31.7 | 263.8 | 1420 | 12.43 (**1.12**) | 11.8/15.1/16.1 | exact; better than SPEC |
| flat_1k | 2,246 | 100 | ~150 | 100 (254) | 360 | 36.8 | 297.3 | 1386 | 10.89 (0.99) | 11.9/15.3/17.5 | exact; better than SPEC |
| nested_10k | 10,952 | 209 | ~210 / ≤250 | 209 (531) | 1,834 | 267.7 | 5897.7 | 8417 | 12.05 (1.09) | 23.3/28.0/30.5 | exact; goal met |
| flat_10k | 31,042 | 241 | ~240 / ≤250 | 241 (592) | 2,497 | 351.2 | 6000.6 | 9007 | 12.48 (**1.13**) | 22.7/29.5/29.5 | exact; goal met |

`logical == walked` everywhere; canonical bytes 137,268 / 137,359 / 1,379,418 / 1,380,691 as at baseline. SQLite 10k statements 1,834 / 2,497 are 3 and 5 below the exact pins (1,837 / 2,502; the probe does not freeze source times; not diagnosed). Commands at +10 ms: 11.4 / 11.2 / 24.5 / 23.3 s (source write, two netem calls, three calibrations).

## Final comparison (SPEC §4 format; baseline `056e920166`, M3 measured)

Baseline = SPEC §2 receipts (`receipts/056e920166/`, test profile), except ¹ (cases appended at P0: baseline from `receipts/P0/`, no +10 ms baseline). "PG gain" = baseline ÷ M3; "PG ÷ SQLite" = M3 PG time ÷ M3 SQLite time.

**Reads, history, writes (test profile)**

| Case | SQLite ms | SQLite stmts | PG trips | PG local | PG +10 ms | PG gain, local / +10 | PG ÷ SQLite after, local / +10 |
|---|---|---|---|---|---|---|---|
| `stat.depth3` | 2.7 → 0.7 | 95 → 35 | 50 → 21 | 37.0 → 13.8 ms | 628 → 288 ms | 2.7× / 2.2× | 20× / 411× |
| `stat.depth7` | 5.3 → 0.5 | 152 → 30 | 73 → 13 | 56.4 → 9.3 ms | 918 → 150 ms | 6.0× / 6.1× | 18× / 288× |
| `stat.root` | 0.8 → 0.3 | 39 → 21 | 18 → 4 | 12.7 → 3.0 ms | 214 → 47 ms | 4.3× / 4.6× | 11× / 168× |
| `read.small_depth3` | 2.7 → 0.4 | 83 → 23 | 38 → 6 | 33.6 → 4.4 ms | 471 → 69 ms | 7.6× / 6.8× | 11× / 174× |
| `read.2000_lines` | 2.1 → 0.6 | 68 → 24 | 32 → 7 | 23.1 → 5.5 ms | 396 → 100 ms | 4.2× / 3.9× | 9.5× / 173× |
| `read.lines_100_200` | 1.8 → 0.3 | 66 → 22 | 30 → 5 | 19.6 → 3.0 ms | 358 → 59 ms | 6.6× / 6.1× | 8.5× / 168× |
| `list.dir_100` | 2.5 → 0.6 | 69 → 25 | 33 → 8 | 28.6 → 3.6 ms | 393 → 114 ms | 7.9× / 3.4× | 6.6× / 208× |
| `list.root` | 0.8 → 0.3 | 39 → 21 | 18 → 4 | 13.8 → 2.5 ms | 211 → 60 ms | 5.6× / 3.5× | 9.1× / 221× |
| `glob.dir_100` | 2.0 → 0.4 | 66 → 22 | 30 → 5 | 22.9 → 3.0 ms | 380 → 76 ms | 7.6× / 5.0× | 6.8× / 172× |
| `glob.tree` | 1.1 → 0.6 | 44 → 26 | 23 → 9 | 17.1 → 8.4 ms | 294 → 101 ms | 2.0× / 2.9× | 14× / 169× |
| `grep.one_file_dir` | 1.4 → 0.4 | 49 → 22 | 23 → 5 | 17.3 → 4.9 ms | 289 → 57 ms | 3.5× / 5.1× | 12× / 138× |
| `grep.tree_1k` | 13.0 → 8.8 | 131 → 113 | 78 → 63 | 79.4 → 40.1 ms | 999 → 928 ms | 2.0× / 1.1× | 4.5× / 105× |
| `diff_files.h10_vs_h99` | 5.2 → 1.1 | 198 → 78 | 88 → 35 | 64.2 → 18.5 ms | 1.15 s → 401 ms | 3.5× / 2.9× | 16× / 355× |
| `blame.h10` | 17.7 → 4.0 | 714 → 132 | 267 → 37 | 203 → 19.1 ms | 3.42 s → 416 ms | 11× / 8.2× | 4.8× / 104× |
| `blame.h100` | 173 → 35.9 | 7,575 → 1,094 | 2,325 → 51 | 1.83 s → 50.6 ms | 31.20 s → 632 ms | 36× / 49× | 1.4× / 18× |
| `revision.h100` | 286 → 33.7 | 12,658 → 769 | 3,890 → 52 | 3.26 s → 46.5 ms | 50.86 s → 831 ms | 70× / 61× | 1.4× / 25× |
| `blame.h20` ¹ | 22.2 → 7.5 | 1,348 → 167 | 466 → 15 | 216 → 14.4 ms | not run → 252 ms | 15× / — | 1.9× / 33× |
| `revision.h10` ¹ | 22.7 → 3.8 | 1,314 → 106 | 468 → 23 | 221 → 13.9 ms | not run → 344 ms | 16× / — | 3.7× / 91× |
| `revision.h20` ¹ | 44.2 → 7.4 | 2,602 → 188 | 888 → 32 | 406 → 19.1 ms | not run → 520 ms | 21× / — | 2.6× / 70× |
| `write.new_file` | 5.0 → 3.9 | 77 → 60 | 66 → 42 | 48.6 → 27.5 ms | 809 → 551 ms | 1.8× / 1.5× | 7.1× / 142× |
| `write.overwrite` | 3.9 → 3.0 | 54 → 37 | 24 → 13 | 22.4 → 12.4 ms | 298 → 216 ms | 1.8× / 1.4× | 4.1× / 72× |
| `edit.2000_lines` | 4.3 → 2.9 | 52 → 35 | 23 → 12 | 22.8 → 11.1 ms | 284 → 187 ms | 2.1× / 1.5× | 3.8× / 65× |
| `mkdir` | 4.5 → 3.2 | 69 → 52 | 45 → 34 | 30.0 → 17.8 ms | 548 → 401 ms | 1.7× / 1.4× | 5.6× / 126× |
| `cp.file` | 5.0 → 3.4 | 64 → 47 | 44 → 32 | 29.3 → 18.3 ms | 548 → 369 ms | 1.6× / 1.5× | 5.3× / 107× |
| `mv.file` | 4.4 → 2.4 | 60 → 43 | 40 → 29 | 23.0 → 18.6 ms | 487 → 333 ms | 1.2× / 1.5× | 7.6× / 137× |
| `rm.file` | 4.6 → 3.0 | 58 → 41 | 37 → 26 | 22.6 → 15.8 ms | 453 → 298 ms | 1.4× / 1.5× | 5.3× / 100× |
| `apply.10_writes` | 7.9 → 5.9 | 74 → 57 | 46 → 35 | 37.7 → 22.9 ms | 562 → 407 ms | 1.6× / 1.4× | 3.9× / 69× |
| `rm.dir_100_recursive` | 18.2 → 18.8 | 179 → 63 | 151 → 41 | 106 → 33.9 ms | 1.87 s → 476 ms | 3.1× / 3.9× | 1.8× / 25× |

**Init (release; baseline SQLite and PG local from the P0 release re-take, D15; baseline +10 ms from `056e920166`, test profile, measured for 1k and projected for 10k)**

| Case | SQLite ms | SQLite stmts | PG trips | PG local | PG +10 ms | PG gain, local / +10 | PG ÷ SQLite after, local / +10 |
|---|---|---|---|---|---|---|---|
| 1k files, 10 dirs | 50.5 → 31.7 | 308 → 288 | 229 → 93 | 410 → 264 ms | 3.10 s → 1.42 s | 1.6× / 2.2× | 8.3× / 45× |
| 1k files, one dir | 52.7 → 36.8 | 2,284 → 360 | 2,246 → 100 | 1.02 s → 297 ms | 27.04 s → 1.39 s | 3.4× / 20× | 8.1× / 38× |
| 10k files, 100 dirs | 492 → 268 | 12,169 → 1,834 | 10,952 → 209 | 9.15 s → 5.90 s | ≈141 s (proj.) → 8.42 s | 1.6× / ≈17× | 22× / 31× |
| 10k files, one dir | 510 → 351 | 31,882 → 2,497 | 31,042 → 241 | 16.20 s → 6.00 s | ≈386 s (proj.) → 9.01 s | 2.7× / ≈43× | 17× / 26× |

Against SPEC §4's projections: reads match or beat them; history beats the baseline massively but sits above the projection (blame h100 632 ms vs ~473 ms, revision 831 vs ~519 ms; 51 / 52 trips vs ~29 / ~30); writes stay about 1.4–1.8× faster (SPEC "about 2×"); Init trips meet or beat SPEC, but PG local Init time does not follow trips (10k 5.9 / 6.0 s vs ~0.74 / ~0.68 s projected: server foreign-key work, 30,418 nested calls, unchanged from baseline), so +10 ms Init 10k is 8.4 / 9.0 s vs ~3.2 / ~3.5 s.

## SPEC §3 exit check at this identity

| Goal | Result |
|---|---|
| Single-path reads ≤ 4 + 5·d | met on every row except probe `stat.depth3` 21 > 19 (group's first call, 5 prepares; warm pin 5; within cold target) |
| grep 1k ≤ 64 | met (63) |
| blame/revision ≤ 18 at V = 10, ≤ 38 at V = 100 | blame met on warm pins (12, 22); probe 37 and 51 (colder starts). Revision **not met**: h100 52 (warm pin 48), h10 23 (Layer-chain row window: recorded, not built) |
| diff_files ≤ 15 | **not met**: 35 (pins 22/48; follow-up recorded) |
| Writes: overwrite ≤ 12, new file ≤ 40, mkdir/cp/mv/rm ≤ 26 | overwrite 13 probe (warm pin 11 met); new file 42 probe (warm pin 33; cold 80 within cold target 86); mkdir 34, cp 32, mv 29 **not met**; rm 26 met |
| Recursive rm ≤ 50 | met (41) |
| Init ≤ 250 per 10k | met (209 / 241) |
| +10 ms added time within ±10% on every row | **not met**: 7 rows over (flags) |
| Every row fits the command budget | met for the measured calls; commands ≤ 25 s (overhead-dominated) |
| SQLite statement changes only by dedupe and I2b | met |
| Same outcomes | met: every outcome kind equals baseline; `logical == walked`; canonical bytes equal |

## Flags

- **Latency band exceeded (+10 ms):** revision.h100 1.37, blame.h20 1.43, revision.h10 1.30, revision.h20 1.42, overwrite 1.36, edit 1.27, Init nested_1k 1.12, Init flat_10k 1.13. Client statements equal server top-level calls in every flagged row: no uncounted statement; the excess is time without statements. The same history and write rows were 0.96–1.08 at M2 (new at M3). The flagged rows are the later cases of their commands; one unproven inference for history is that larger batched replies take more than one network round trip each. Attribution needs a per-statement timing trace.
- **PG Init local time is server-bound:** 10k nested 5.9 s on 209 trips (30,418 nested FK calls, unchanged from baseline).
- **SQLite Init:** flat_10k 510 → 351 ms (I2b fix confirmed). Probe 10k statements 3 and 5 below the exact pins (source times not frozen); not diagnosed.
- **SQLite rm.dir_100:** 18.8 ms vs P0 12.5 ms and baseline 18.2 ms; within the baseline's spread; statements −65%.
- **Memory:** 14.8–21.9 MiB per `fs.*` command (M2 14.5–18.9); history +10 ms 20.9 MiB (M2 18.1); Init 11.8–17.5 MiB at 1k, 22.7–30.5 MiB at 10k (M1 release 11.8–12.8, 23.4–28.8).
