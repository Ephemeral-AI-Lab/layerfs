# GC capacity model: a 50 GB Store (spec section 13)

> Deferred 50 GB / §13 capacity model for `layerfs-project#41`. Adds no contract; [`LayerFS-GC-spec.md`](LayerFS-GC-spec.md) wins, `§N` cites it.
> **[M]** = measured on the cited artifact; **[E]** = extrapolated from the stated assumptions. Nothing marked [E] is a measurement.
> **T-G61** measured a real **10.0 GB** Store (`sqlite_compact_capacity_measure.json`): three `none` runs at 8.88% dead, three `rebuild` runs at 10.00% dead and one WAL/FULL rebuild. **T-G66** then measured the **50 GB** arm on the same host (`sqlite_compact_capacity_measure_50gb.json`): a **49.999 GB** pack-byte Store (384,850 packs, 347,490 live whole-file objects, 33,850 dead = 8.876%, built in 761.2 s), **one** `none` run and **one** `rebuild` run at 10.000% dead. One run per arm is what §13 asks of the 50 GB arm, not a three-run median; the 10 GB three-run medians stay the repeated-run basis. **The 50 GB numbers do not scale linearly from the 10 GB ones**: the 10 GB Store and its rebuild artifacts fitted the host's page cache, the 50 GB one does not, so its `rebuild` is **85.5 min** and its real temp peak on disk is **143.1 GB above the pre-rebuild baseline** (**~2.9×** the Store) — not the [E] ~20 min and 1.9×. **T-G72** replaced the receipt's `new + visited` peak with a documented bound over the run's temp artifacts (pre-truncate staged copy + new file + rollback journal + visited): **147.609 GB** for the same 50 GB `rebuild`, 3.2% above the sampled peak and no longer under-reporting the journal. The 30% dead, 50 GB-live/35 GB-live, each-removal-type and PostgreSQL `ACCESS EXCLUSIVE` arms remain [E].

## 1. Measured basis

| Quantity | Tiny fixture (T12.3a) [M] | Capacity fixture (T-G61), 10 GB [M] | 50 GB arm (T-G66) [M] |
|---|---|---|---|
| Store size | 20 live roots, 20 dead packs | **9.987 GB pack bytes** (76,870 packs; 69,399 live 131,071-byte whole-file objects, 6,770 dead); 10.035 GB physical | **49.999 GB pack bytes** (384,850 packs; 347,490 live 131,071-byte whole-file objects, 33,850 dead); **50.235 GB physical** at the `none` seal, 50.863 GB at the `rebuild` seal |
| Profile | `Disposable` (WAL/OFF) | `Disposable` (WAL/OFF); one `Durable` (WAL/FULL) control | `Disposable` (WAL/OFF); no `Durable` arm (owner profile disabled) |
| `none` wall | n/a | 148.519 / 149.704 / **149.318 s** median; range 148.519–149.704 | **1,633.585 s** (one run) |
| `rebuild` wall | 26.738 / 28.201 / 28.681 ms; median 28.201 ms | 246.371 / 255.548 / 243.306 s; median **246.371 s**; range 243.306–255.548 | **5,131.835 s** (one run) |
| `Durable` rebuild | n/a | **284.078 s** (one run) | not run (owner profile) |
| `deadBytesBound` | 1,100 B | **887,601,160 B** at 8.88% (`none`); **1,011,104,896 B** at 10.00% (`rebuild`) | **4,438,005,800 B** at 8.876% (`none`); **5,062,342,096 B** at 10.000% (`rebuild`) |
| `physicalShrink` | 20,480 B | **1,016.5–1,019.0 MB** (mean 1.017 GB); `none` 0 | **5,099,651,072 B** (5.100 GB); `none` 0 |
| `peakTempBytes` (receipt) | **758,240 B** (T-G72 bound; old formula 258,048 B) | **29,481,103,648 B** (29.481 GB bound; old formula 9.144 GB) | **147,608,626,376 B** (147.609 GB bound; old formula 45.784 GB) |
| Peak on disk, sampled | n/a | not sampled | **143.1 GB above the pre-rebuild baseline** (§3) |
| New file | n/a | 9,139,773,440 B (9.140 GB) | 45,763,203,072 B (45.763 GB) |
| Fixture build | n/a | 150 s | **761.2 s** (65.7 MB/s over 49.999 GB) |
| Host | linux-x86_64, rustc 1.85.1 | 88 vCPU, 135 GB RAM, ext4, 4.0 TB free | 88 vCPU, 125 GB visible RAM, ext4 (`/dev/sda2`), 4.0 TB free; same rustc 1.85.1 |

Phase durations, 50 GB `rebuild` (one run, WAL/OFF): **mark 784.251 s**, **build 4,254.371 s**, **verify 85.557 s**, **install 7.311 s** (sum 5,131.490 s of the 5,131.835 s wall). `none`: mark 1,633.579 s and nothing else.
Sources: [`sqlite_compact_capacity_measure.json`](../../../../engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_capacity_measure.json) (10 GB) and [`sqlite_compact_capacity_measure_50gb.json`](../../../../engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_capacity_measure_50gb.json) (50 GB).

Caches [M]: the 10 GB Store fitted in the host's page cache and no cache drop ran between its runs; the 50 GB arm did not fit. Its `none` mark ran immediately after the fixture build (50 GB of fresh writes still in the page cache) at **35.9 s per live GB**; the `rebuild` mark, after that read pass, ran at **17.2 s per live GB** — the 16.4 s/GB of the 10 GB arm. The mark is still single-threaded payload/identity hashing (~62 MB/s effective); the 2.1× spread is cache state, not the object count. No cache drop ran between the two 50 GB arms either.

Per-GB rates measured on this host (GB = 10^9 B):

| Step | 10 GB [M] | 50 GB [M] | Basis |
|---|---|---|---|
| Mark (`none`) | 14.9 s per stored GB / 16.4 per live GB | **35.9 s per live GB** (32.7 per stored GB) | 1,633.579 s over 45.561 GB live; cold page cache, right after the fixture build |
| Mark (`rebuild`) | 16.4 s per live GB | **17.2 s per live GB** | 784.251 s over 45.561 GB live; warm after the `none` read pass |
| Build (`VACUUM INTO` + prune + `VACUUM`) | 10.3 s per new GB | **93.0 s per new GB** | 4,254.371 s over 45.763 GB new; the phase moves ~141 GB (staged copy 50.85 GB, 44.6 GB rollback journal, target rewrite 45.76 GB) at ~33 MB/s |
| Verify | 0.034 s per new GB (0.31 s total) | **1.87 s per new GB** (85.557 s) | at 50 GB it reads the payload back; the 10 GB read was cache-only |
| Install | ~1.9 s fixed | **7.311 s** | rename, fsync, settle |

The mark dominates at 10 GB; at 50 GB the build phase dominates by 5.4× — the opposite. Do not extrapolate either phase linearly from the 10 GB arm: the 10 GB run was cache-optimistic, and the 50 GB build rate depends on free page cache and device write throughput rather than on CPU. Use the 50 GB [M] rates above for a 50 GB Store and re-measure with §7's harness on the target host.

## 2. The 50 GB / 9% Store and the rule

`rebuild iff total > 0 AND dead * 10 >= total` (§3); `dead` = bytes in packs with no live object after the metadata walk (§6.4). GB = 10^9 B.

| Dead share | Old | Dead | Live | `dead*10 >= total` | Path | Measured? |
|---|---|---|---|---|---|---|
| 9% | 50.0 GB | 4.5 GB | 45.5 GB | 45 >= 50: **no** | `none` | **50 GB [M]**: 49.999 GB pack bytes, 8.876% dead → `none`, 0 B freed |
| 10% | 50.0 GB | 5.0 GB | 45.0 GB | 50 >= 50: **yes** | `rebuild` | **50 GB [M]**: 50.623 GB pack bytes, 10.000% dead → `rebuild`, 5.100 GB freed |
| 30% | 50.0 GB | 15.0 GB | 35.0 GB | 150 >= 50: **yes** | `rebuild` | [E] only; §13's 50 GB total/35 GB live and 50 GB-live arms remain deferred |

At **9% the measured path is `none`**: mark only, no file, generation unchanged (7 → 7), **0 bytes freed**; the report still gives the dead bound (4.438 GB measured at 50 GB; 0.888 GB at 10 GB).
The measured 50 GB rebuild copied a 45.763 GB new file to free 5.100 GB — **8.97 B copied per B freed** [M], against 9.04 at 10 GB [M] and the frozen 10% floor's ~9.0 [E]. At 30% it is ~2.3 [E].

## 3. Temp-space model

**SQLite (measured).** 50 GB arm: old physical file 50.863 GB; new `.compact` 45.763 GB; `.compact-visited` 20,365,312 B. The rebuild's temp artifacts are the *whole* staged copy, a rollback journal and SQLite's rewrite temp database — not the new file plus visited. `VACUUM INTO` first writes the whole sealed Store to `<store>.compact` (the old page count, 50.863 GB); the prune's `DELETE ... WHERE NOT IN visited` grows `<store>.compact-journal` to **44.6 GB** (44,630,503,928 B observed); the closing `VACUUM` then writes the vacuumed Store to SQLite's unlinked temp database (≈45.763 GB, in the process temp directory) while the staged file is rewritten in place and the rollback journal records the pages it overwrites. A 20 s disk sampler saw the run's real temp peak at **143.1 GB above the pre-rebuild baseline** (sample max 315.9 GB used against 172.8 GB before the rebuild) = staged 50.85 + rewrite temp 45.76 + journal 44.6–45.8 GB; **~2.9×** the 50 GB pack-byte Store [M].
The old receipt computed `target_bytes + visited_bytes` (`Built::peak_temp_bytes` in `engine/crates/layerfs-persistence/src/backend/sqlite/compact.rs`) *after* the staged file was truncated: 45.784 GB = new + visited, under-reporting the rollback journal and the staged/temp copies (the T-G66 finding).
**T-G72 bounds the whole run instead.** `Built::peak_temp_bytes` is `staged_bytes + journal_bound + target_bytes + visited_bytes`: the staged extent measured right after `VACUUM INTO`, the rollback-journal upper bound (one record per staged page, `page_size + 8` B, plus an 8192 B header allowance), the new file (which also bounds the rewrite's temp database) and the visited scratch. The 50 GB arm is 50,862,854,144 + 50,962,203,848 + 45,763,203,072 + 20,365,312 = **147,608,626,376 B (147.609 GB) = 2.90× the old file**, 4.5 GB (3.2%) above the sampled peak: the bound charges the journal the whole staged page count instead of the pages the transactions actually touch. The total footprint is old + peak = **~198 GB (3.9×)** at 50 GB; the measured sample was old + 143.1 ≈ 194 GB (3.8×) [M]. Upstream issue: [Ephemeral-AI-Lab/layerfs#695](https://github.com/Ephemeral-AI-Lab/layerfs/issues/695).
Visited is metadata-scale: 20,365,312 B for 347,490 live objects = **58.6 B per visited object** [M] (10 GB: 57 B).
`physicalShrink` (5.100 GB) exceeds `deadBytesBound` (5.062 GB) by ~0.7%, as at 10 GB (0.6%): shrink is file-level and includes free pages; do not equate the two.

**PostgreSQL.** Peak = old tables (50 GB, dead included) + scratch schema `lfs_gc_<hex>` (LIKE copy of live `pack`/`object_location`) + index/PK + `ANALYZE` + WAL "up to the live bytes" (§8 "Space"); mark temp tables are session-temp and small.
Extrapolated [E; SQLite measured: the T-G72 receipt bound is 2.90× the old file and old + peak ~3.9×, so the SQLite rows below are receipt-bound estimates and every [E] row stays a projection]:

| Backend | Case | Old | New file / scratch | Visited / index | Journal / WAL | Peak temp | Total footprint |
|---|---|---|---|---|---|---|---|
| SQLite | 9% `none` | 50.0 | 0 | <=0.1 GB | 0 | **<=0.2 GB** | ~50.2 GB |
| SQLite | 10% `rebuild` | 50.0 | ~45.8 new + ~50.9 staged | ~0.02 GB | ~44.6 GB prune / ~45.8 GB rewrite | **~147.6 GB bound** (sampled 143.1) | **~198 GB (3.9× bound; 3.8× sampled)** |
| SQLite | 30% `rebuild` | 50.0 | ~35.2 new + ~50.9 staged | ~0.02–0.1 GB | ~35 GB prune/rewrite [E] | ~137 GB bound [E] (actual ~120 [E]) | **~187 GB [E] (3.7×)** |
| PG | 9% `none` | 50.0 | 0 | 0 | mark only | **~50 GB** | ~50 GB |
| PG | 10% `rebuild` | 50.0 | ~45.0 | ~4–6 GB | <=45 GB | **~99–101 GB, <=146 GB** | same |
| PG | 30% `rebuild` | 50.0 | ~35.0 | ~3–5 GB | <=35 GB | **~88–90 GB, <=125 GB** | same |

**Headroom [M].** Keep **>=150 GB free** for a 50 GB `rebuild` at ~10% dead (measured 143.1 GB sampled peak, ~2.9× the Store), >=115 GB at <=30% dead [E]; `none` needs a few GB. The T-G61 [E] guidance of 95–110 GB came from the receipt field and is **too low**.
Size `pg_wal` separately: a full `pg_wal` stops the server (§8).

## 4. Time model

Measured 50 GB [M] on this host, beside the 10 GB medians. Linearity is **not** assumed: the 50 GB column is measured, not scaled.

| Dead | Path | Measured old | 10 GB wall (median) [M] | 50 GB wall [M] | s/GB live at 50 GB [M] | Freed at 50 GB |
|---|---|---|---|---|---|---|
| 9% | `none` | 49.999 GB | 149.318 s | **1,633.585 s (~27.2 min)** | 35.9 | 0 |
| 10% | `rebuild` | 50.623 GB | 246.371 s | **5,131.835 s (~85.5 min)** | 112.6 | 5.100 GB |
| 10% | `rebuild`, Durable | - | 284.078 s | not run | - | - |
| 30% | `rebuild` | - | - | **~65–70 min [E]** (mark 17.2 s/GB + build 93 s/GB over ~35.8 GB new) | ~110 [E] | ~15.0 GB |

The oldest [E] table said 1.8–2.0 min for a 50 GB rebuild; the linear T-G61 [E] said ~20.3 min; the measured 50 GB rebuild is **85.5 min**, 4.2× the linear projection. Two measured causes: (a) the build phase is **9× slower per new GB** than at 10 GB (93.0 against 10.3 s/GB) because the rebuild's ~146 GB working set (old + staged + journal) no longer fits the 125 GB page cache, so the copy is real device I/O; (b) the mark has a 2.1× cold/warm spread (35.9 against 17.2 s/GB live). Plan **~27 min** for a 50 GB `none` and **~85 min** for a 50 GB `rebuild` on a host like this one [M]; re-measure with §7's harness on the target host.
PostgreSQL is not measured by AC-57; its server-side `INSERT ... SELECT` copy, index build and `ANALYZE` extrapolate to **4–10 min**, `ACCESS EXCLUSIVE` only at the swap, **~2 s** [E].

## 5. Operation window, mounts and commits

- **SQLite.** `writer_admission` is held checkpoint→rename, so the exclusive lane is the whole §7 run: **27.2 min for a 50 GB `none`** and **85.5 min for a 50 GB `rebuild`** [M]; the 10 GB values are 2.5 min each [M]. A live mount = `MountsActive`; a mount during compaction = `BusyOp` (AC-07/AC-49); a Commit racing install = `StaleStore` or lands on `G+1`, retry never lost. `none` is mark-only but still holds the lane for its mark.
- **PostgreSQL.** The session advisory key spans M–C; mounts get `BusyOp`, a post-mark commit makes the swap `Conflict` with no writes (§8). `ACCESS EXCLUSIVE` is the swap only (~2 s) [E]; the `G+1` Store is usable during C. Plan 25 minutes quiet for SQLite at 50 GB [M for the phases, E for the quiet-window policy], mostly online for PG [E].
- **Monitoring.** `store.status` gives `op {id,name,phase,started,uncertain}`, `state` busy/unresolved and phases — SQLite `mark/build/verify/install`, PG `mark/build/swap/cleanup` (§11). The capacity runs' T10.3 phase sums are in the fixtures (`none` = all mark; `rebuild` = mark/build/verify/install).
  `compact_metrics` has runs, operations, refusals, settles, retries, status, slo, phase durations and last §11 gauges: process-local, no CLI/SDK renderer, no PG metrics.
  Don't loop a direct reader: it makes the checkpoint `Busy` (§7; [`gc-runbook.md`](gc-runbook.md) §1).

## 6. Risks

- **Disk full (both).** SQLite `SQLITE_FULL`/`ENOSPC`/`EDQUOT`/`EFBIG` → `Capacity`, artifact deleted, old Store unchanged apart from checkpoint (§7 step 8; AC-55). The measured 50 GB run needed **143.1 GB** of sampled temp peak (~2.9× the 50 GB Store); the T-G72 receipt bound is **147.609 GB** (2.90× the old file), so keep **>=155 GB free** and re-check before a 30% run (>=140 GB there [E]).
- **Peak temp.** `peakTempBytes` now bounds the run's temp artifacts (T-G72): pre-truncate staged copy + new file + rollback-journal bound + visited. The 50 GB receipt is 147.609 GB (2.90× the old file; sampled 143.1 GB); total footprint = old + peak ≈ 198 GB (3.9×). The journal term is conservative: the bound charges one record per staged page while the run journaled 44.6 GB [M].
- **WAL.** SQLite needs `wal_checkpoint(TRUNCATE) busy == 0` and never unlinks `-wal`/`-shm` (a live reader is `Busy`); PG WAL ≤ live bytes + standby lag.
- **PG standby.** `synchronous_commit ≥ remote_write` with `synchronous_standby_names` needs a streaming sync/quorum standby; missing → `Busy` no writes; one dying after the check blocks COMMIT and `SET LOCAL` cannot bound it (§8; AC-60 deferred).
- **Installed-but-uncertain / races.** Rename/fsync failure or lost COMMIT keeps the new Store at `G+1`, settles `unknown`; never delete the artifact; PG: prove the backend ended (`pid`, `backend_start`, `usename`), then read the incarnation.
  `MountsActive`/`BusyOp`/`StaleStore` are expected: quiesce, retry, never kill an owner. The 20,480 B vs 1,100 B ratio and 28.2 ms are tiny-fixture effects; the 10 GB and 50 GB numbers above are the capacity evidence.

## 7. Runbook thresholds (50 GB Store)

1. **Eligibility:** only `dead*10 >= total` (>=10% dead, §3); below it `none`, no file, 0 freed — but still a full mark (**27 min at 50 GB** [M]). Record the dead bound and re-check later.
2. **Economics/space [M]:** at 10% dead the measured rebuild copies **8.97 B per B freed** [M] (10 GB: 9.04 [M]) and needs **~148 GB free** (147.609 GB receipt bound, 2.90×) [M]; prefer **>=20–30% dead** (30% ~2.3 B per B [E], ~137 GB bound [E]).
3. **Window/quiesce [M]:** SQLite **~27 min** quiet for a 50 GB `none` and **~85 min** for a 50 GB `rebuild`; the 10 GB values are 2.5 min each. No full doctor; expect `BusyOp`. A `Busy` owner: wait, do not terminate (runbook §2).
4. **Recovery gate:** if `store.status.state` is `unresolved`, rerun `store.compact` to adopt the op; never hand-delete artifacts or `gc_intent`/`gc_dead` (runbook §2–§3).
5. **Backup/alerts/failures:** `backup.create` before the first run ([`gc-rollout.md`](gc-rollout.md) §3; §9); alert on `Capacity`, `unresolved`, `retries`, nonzero `residue`, PG WAL/standby lag. `Capacity` → free space and rerun; uncertain → keep the new Store, settle `unknown`.
6. **Re-measure per host.** `CAPACITY_MEASURE=1 UPDATE_CAPACITY=1 cargo test -p layerfs-persistence --release --test sqlite_compact_capacity -- --ignored --nocapture` with a TMPDIR of **>=3× the target** (the receipt bound is 2.90×; the measured temp peak was 143.1 GB). The 50 GB arm used `CAPACITY_GROUPS=3510 CAPACITY_DEAD_OBJECTS=33850 CAPACITY_RUNS=1 CAPACITY_KEEP=1 CAPACITY_OUT=<...>_50gb.json`; the mark is CPU-bound and the copy I/O-bound, so a host with faster SHA-256 and more page cache can be several times quicker.

## Sources

- [`LayerFS-GC-spec.md`](LayerFS-GC-spec.md): §3 threshold, §6.4 dead set, §7 SQLite rebuild/cost/Capacity, §8 PG phases/space/standby, §11 report and `peakTempBytes`, §13 deferred 50 GB arms and estimates.
- [`gc-t12.3a-sqlite-matrix-report.md`](gc-t12.3a-sqlite-matrix-report.md) (AC-57) and `sqlite_compact_measure.json` (the tiny [M] numbers; its `peakTempBytes` is the T-G72 bound, 758,240 B); `sqlite_compact_capacity_measure.json` and `sqlite_compact_capacity.rs#capacity_measure` (the 10 GB [M] numbers); `sqlite_compact_capacity_measure_50gb.json` (the 50 GB [M] numbers, one run per arm). The two capacity JSONs still record the T-G66 old-formula `peakTempBytes` (45.784 GB at 50 GB); the T-G72 bounds in this page are arithmetic over their measured page counts and sizes, not a re-measurement.
- [`gc-t9-postgresql-rebuild.md`](gc-t9-postgresql-rebuild.md) (M/A/B/C, scratch schema, WAL/standby gaps) and [`gc-implementation-report.md`](gc-implementation-report.md) (status/metrics projection).
- [`gc-runbook.md`](gc-runbook.md) and [`gc-rollout.md`](gc-rollout.md) (safe cleanup, rehearsal).
