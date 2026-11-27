# #818 M2 benchmark (merged tip `7a79aaaea5`)

> **Status:** Measured receipt, one sample per case and arm. M2 = R4, R5, H1, H2, H3, W6 + the W2 review fix + the W6 SQLite guardrail; lane C's H5 core merged, not wired. Init is unchanged by M2: counted at PG local and SQLite in the test profile only, as a no-change check. Comparison base: `receipts/M1/` (`cfc9586d05`). Run by the benchmark subagent; saved by the orchestrator from its report text.

## Identity

| Field | Value |
| --- | --- |
| Source | branch `claude/rt818-rollout-c3dbae`, HEAD `7a79aaaea5` |
| Harness | `tests/fs_call_round_trips.rs` and `tests/support/round_trips.rs`, unchanged since `8a8fcea616` |
| Build | `cargo +1.85.1 test --locked --offline -j 6 -p layerfs-store-service --test fs_call_round_trips --no-run` (test profile; no release build: Init timing rows skipped) |
| Binary | `target/debug/deps/fs_call_round_trips-9eab8e660a198019`, sha256 `5c07e6b569adf436…` |
| PostgreSQL | `lfs818-bench-pg`, postgres:17 (`sha256:ff87138949ad…`), 17.11 aarch64, port 61949, trust, db `rt818_bench`, `synchronous_commit=off`, `pg_stat_statements` (top-level, current db) |
| Delay | netem in the container namespace via the probe helper; after each of the 3 delay commands and at the end `tc qdisc show dev eth0` = `qdisc noqueue 0: root` |
| Calibration (`SELECT 1` median of 200) | local 0.45–0.85 ms; +10 ms: 11.67 (reads), 11.60 (writes), 11.04 (history) |
| Host | macOS 26.4.1 arm64, Docker 8 CPU / 7.75 GiB; 1-min load 8.5–11.5 (M1 4.2–6.5); host free 116 GB at start, 108 GB at end |
| Commands | `run_bounded.py --limit 120 -- /usr/bin/time -l <bin> <test> --exact --test-threads=1 --nocapture`, `FS818_ARM`/`FS818_GROUP`; no case filter; all exit 0 |
| Cache state | probe: each case on the Store open that wrote the setup; Init a new Store per case |
| Receipts | `bench-sqlite.log`, `bench-pg.log`, `bench-pg10.log`, `bench-init-test.log` |

Trips = PG client round trips of all roles; SQLite = statements; "M1 → M2". **Lane expectation** = M1 probe row + the lane pins' measured change from the M1 tip to M2 (keeps the probe's own cache state). Verdict: within 15% of the lane expectation. SPEC §6.3 M2 shown where listed. Latency: (PG +10 − PG local) / trips vs the command's calibration median; pass ≤ 1.10×. Mem = peak footprint MiB, SQLite / PG / PG +10.

### Reads

| Case | M1 | SPEC M2 | Lane exp. | M2 trips | SQLite stmts | SQLite ms | PG local ms | PG +10 ms | added/trip (×cal) | Mem | Verdict |
|---|---:|---:|---:|---:|---|---|---|---:|---|---|---|
| stat.depth3 | 28 | 18 | 20 | 21 | 36→35 | 0.58→0.93 | 13.2→19.1 | 263 | 11.64 (1.00) | 14.7/16.0/17.2 | within (+5%); group's first call, 5 prepares (M1 8); +17% vs SPEC 18 |
| stat.depth7 | 17 | ~30 | 13 | 13 | 31→30 | 0.45→0.66 | 11.5→10.6 | 164 | 11.83 (1.01) | same | exact |
| stat.root | 8 | 4 | 4 | 4 | 22→21 | 0.26→0.45 | 4.2→3.7 | 50 | 11.62 (1.00) | same | exact |
| read.small_depth3 | 10 | 6 | 6 | 6 | 24→23 | 0.40→0.63 | 5.7→8.8 | 75 | 11.10 (0.95) | same | exact |
| read.2000_lines | 11 | — | 7 | 7 | 25→24 | 0.51→0.78 | 5.7→8.9 | 90 | 11.54 (0.99) | same | exact |
| read.lines_100_200 | 9 | — | 5 | 5 | 23→22 | 0.36→0.51 | 4.5→5.1 | 71 | 13.20 (**1.13**) | same | exact; latency +9 ms over band on 5 trips |
| list.dir_100 | 12 | 8 | 8 | 8 | 26→25 | 0.68→0.93 | 6.0→6.3 | 97 | 11.28 (0.97) | same | exact |
| list.root | 8 | — | 4 | 4 | 22→21 | 0.26→0.39 | 3.7→3.0 | 48 | 11.33 (0.97) | same | exact |
| glob.dir_100 | 9 | — | 5 | 5 | 23→22 | 0.46→0.69 | 4.8→5.9 | 65 | 11.92 (1.02) | same | exact |
| glob.tree | 13 | — | 9 | 9 | 27→26 | 0.52→0.84 | 6.5→15.1 | 111 | 10.70 (0.92) | same | exact |
| grep.one_file_dir | 9 | — | 5 | 5 | 23→22 | 0.31→0.53 | 5.0→9.8 | 65 | 10.98 (0.94) | same | exact |
| grep.tree_1k | 67 | 64 | 63 | 63 | 114→113 | 8.63→12.53 | 45.3→161.8 | 797 | 10.08 (0.86) | same | exact; local time ×3.6 at equal trips (load) |
| diff_files.h10_vs_h99 | 87 | ~55 | 33 | 35 | 194→78 | 3.30→1.44 | 42.1→19.9 | 423 | 11.52 (0.99) | same | within (+6%); 4 prepares; pins warm 23 / cold 49 |

### History (all at +10 ms, one command)

| Case | M1 | SPEC M2 | Lane exp. | M2 trips | SQLite stmts | SQLite ms | PG local ms | PG +10 ms | added/trip (×cal) | Mem | Verdict |
|---|---:|---:|---:|---:|---|---|---|---:|---|---|---|
| blame.h10 | 281 | — (M3 ~17) | 72 | 70 | 719→132 | 13.1→6.0 | 148.8→101.9 | 805 | 10.04 (0.91) | 14.5/16.8/18.1 | within (−3%); first call of group, 8 prepares; pins warm 33 / cold 71 |
| blame.h100 | 2,428 | ~900 | 459 | 459 | 7,658→1,094 | 119.6→51.2 | 1261→552 | 5411 | 10.59 (0.96) | same | exact; probe ≈ pin cold (491) less first-use costs (versions 11–100 uncached; pin warm 254 repeats the identical call) |
| revision.h100 | 3,887 | ~1,000 | 254 | 256 | 12,658→769 | 203.9→46.5 | 2278→328 | 3084 | 10.77 (0.98) | same | within (+1%); ≈ pin warm 253 |
| blame.h20 | 462 | — | ~57 (interp.) | 60 | 1,342→167 | 23.7→9.6 | 286.0→169.5 | 718 | 9.15 (0.83) | same | unpinned shape |
| revision.h10 | 462 | — | 40 | 41 | 1,314→106 | 23.7→5.5 | 307.8→92.9 | 493 | 9.75 (0.88) | same | within (+2.5%); pin cold 82 |
| revision.h20 | 882 | — | ~103 (interp.) | 70 | 2,602→188 | 46.1→9.8 | 572.1→113.2 | 846 | 10.47 (0.95) | same | unpinned; better than interpolation |

Projection before running: blame.h100 6.8 s, revision.h100 3.8 s at +10 ms, so both ran directly (whole group one command, 22.4 s, declared exception ≤ 25 s; measured calls 11.4 s). All +10 ms trip counts equal the local run's.

### Writes

| Case | M1 | SPEC M2 | Lane exp. | M2 trips | SQLite stmts | SQLite ms | PG local ms | PG +10 ms | added/trip (×cal) | Mem | Verdict |
|---|---:|---:|---:|---:|---|---|---|---:|---|---|---|
| write.new_file | 56 | ~55 | 56 | 56 | 60→60 | 4.52→6.99 | 35.5→49.8 | 714 | 11.86 (1.02) | 16.5/15.9/17.8 | exact |
| write.overwrite | 13 | ~12 | 13 | 13 | 37→37 | 3.55→4.81 | 12.7→15.9 | 179 | 12.54 (1.08) | same | exact (+8% vs SPEC) |
| edit.2000_lines | 12 | — | 12 | 12 | 35→35 | 3.21→4.60 | 12.7→19.4 | 160 | 11.72 (1.01) | same | exact |
| mkdir | 34 | — | 34 | 34 | 52→52 | 3.91→5.82 | 21.9→35.5 | 414 | 11.12 (0.96) | same | exact; M1 latency flag not repeated |
| cp.file | 33 | — | 30–33 | 32 | 47→47 | 4.21→5.67 | 20.6→31.1 | 419 | 12.11 (1.04) | same | within; Workspace reader 6→5 (W6) |
| mv.file | 29 | — | 29 | 29 | 43→43 | 2.98→4.64 | 19.6→22.7 | 387 | 12.55 (1.08) | same | exact |
| rm.file | 26 | — | 26 | 26 | 41→41 | 3.23→4.43 | 19.5→22.7 | 350 | 12.60 (1.09) | same | exact |
| apply.10_writes | 35 | — | 35 | 35 | 57→57 | 6.04→8.21 | 29.0→46.2 | 449 | 11.52 (0.99) | same | exact |
| rm.dir_100_recursive | 41 | — | 41 | 41 | 63→63 | 12.66→17.34 | 36.0→53.9 | 545 | 11.98 (1.03) | same | exact |

SQLite write statements equal M1 on every case (W6 guardrail). SQLite write times +1.3–4.7 ms at equal statements (load; under the 5 ms floor).

### Init (test profile, no-change check)

| Case | M1 PG trips (server) | M2 PG trips (server) | SQLite stmts | SQLite ms (test) | PG local ms (test) | Mem SQLite/PG | Verdict |
|---|---|---|---|---:|---:|---|---|
| nested_1k | 209 (343) | 209 (343) | 288 → 288 | 51.6 | 455.2 | 11.3/13.0 | unchanged |
| flat_1k | 228 (379) | 228 (379) | 360 → 360 | 60.0 | 696.6 | 11.0/13.1 | unchanged |
| nested_10k | 624 (1,138) | 624 (1,138) | 1,837 → 1,837 | 448.5 | 11,044.0 | 22.8/25.0 | unchanged |
| flat_10k | 726 (1,343) | 726 (1,343) | 2,502 → 2,502 | **941.2** | 10,563.4 | 21.8/24.4 | unchanged counts; SQLite time flag (I2b; fixed on lane I's branch `cef20dfd63`, not yet merged) |

`logical == walked` everywhere; canonical bytes as at P0 and M1.

## Answers and outcomes

Every outcome kind equals M1 and P0 (ok / Text / Committed); no refusals. Setup counts equal M1.

## Flags

- **15% rule:** all 28 `fs.*` rows within 15% of the lane expectation (22 exact). Against SPEC §6.3 only stat.depth3 (+17%: first call, 5 prepares) and overwrite (+8%) are above; history far better than SPEC.
- **Probe vs pins:** blame.h100 (459) sits near the pin's cold row, revision.h100 (256) at its warm row — the probe's case order, not code. At M3 judge blame h100 against the pins or the probe's cold-side row explicitly.
- **Latency band:** read.lines_100_200 1.13 (5 trips, +9 ms); M1's mkdir/rm.dir flags did not repeat.
- **SQLite Init flat_10k time:** 941 ms test profile vs P0 499 ms; I2b's windowed placement scan, already fixed on lane I's branch (release 494 → 299 ms there).
- **PG Init 10k local** 11.0 s / 10.6 s (test profile, load 9) on 624 / 726 trips: not trip-bound.
- **Load** 8.5–11.5; counts are primary.
- **Memory:** 14.5–18.9 MiB per `fs.*` command (M1 13.5–15.7); history +10 ms 13.8 → 18.1 MiB (H3's per-call cache, 3 MiB bound plus memo); writes also +1.5–3 MiB with no code change, so part is noise.
