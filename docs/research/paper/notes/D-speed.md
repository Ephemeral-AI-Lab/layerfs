# D. Speed: performance engineering notes for the LayerFS paper

Compiled 2026-11-14, read-only, from the LayerFS repository (`main` v1.x working tree and
`origin/layerfs_v2_0_0` at `8cfb6e2734`) and the benchmark repository. Nothing was built or run.

**Citation legend**
- `LFS:` = `~/Ephemeral-AI-Lab/layerfs/` (main working tree unless the path says `v2:`, which means `git show origin/layerfs_v2_0_0:<path>`).
- `BENCH:` = `~/Ephemeral-AI-Lab/layerfs-benchmark/`.
- `commit <sha>` = commit message body on `origin/layerfs_v2_0_0` or `main` (read with `git show -s`).
- `OVN-*` short names:
  - `OVN-1106` = `BENCH:comparision/ovnfs/README.md` (2026-11-06 tables)
  - `OVN-1107` = `BENCH:comparision/ovnfs/rerun-2026-11-07/report.md`
  - `OVN-V2S` = `BENCH:comparision/ovnfs/rerun-2026-11-07-v2-speed/report.md`
  - `OVN-CS` = `BENCH:comparision/ovnfs/rerun-2026-11-08-commit-speed/report.md`
  - `OVN-COLD` = `BENCH:comparision/ovnfs/rerun-2026-11-08-cold/report.md`
- `ESD` = `BENCH:design-exec-speed.md` (untracked draft design, "Outcome (2026-11-07)" section).

**Eligibility labels.** Unless a row says otherwise, every timing below is a single-sample,
warm, shared-host diagnostic that its own report labels **INELIGIBLE** (not a qualified
performance claim). Three exceptions in kind, not in eligibility:
- `OVN-COLD` uses **3 samples per case, median**, with **verified guest-page-cache eviction**
  before every commit and read. The report still calls these diagnostics: cold applies inside
  the Docker VM only.
- In-process numbers from commit messages are "Linux --release, in-process, median of 3"
  diagnostics with no mount and no route (labelled **IN-PROC** below).
- Counter results (statements, round trips, owner jobs, trials, locate calls) are exact
  counts and are the primary evidence in the #320/#321/#352 work (labelled **COUNT**).

---

## 1. Timeline of speed work

Order is chronological. "Before -> after" uses the source's own numbers and units.

### 1a. v1.x era (October 2026, `main`)

| Date | Commit | Change | Mechanism | Before -> after (label) | Source |
| --- | --- | --- | --- | --- | --- |
| 2026-10-13 | `56a6424e1` (#320 T1 batch 1) | Keep a bound Workspace's Storage producer between Commits | Each Commit had rebuilt its producer handle and re-read policy row, content-hint table and value-group catalogue | Tier-1 case, 100 Commits: Store transactions 33,211 -> 9,395; catalogue queries 13,793 -> 620; sorting statements 13,693 -> 421; full-scan steps 553,786 -> 4,900; per-Commit 36.7 ms -> 29.0 ms (COUNT; time INELIGIBLE) | `LFS:core/docs/issues/320/PROGRESS.md` §"T1 batch 1" |
| 2026-10-13 | `a6343e5f9` + `63a1ab005` (#320 "Design B") | Inode values stored as Store rows instead of pooled value-group packs; sorted merge reads only children a change reaches | Commit cost had grown with branch length: every Commit read every inode leaf of a one-level branch page and paid every leaf's value groups; first Save rebuilt a pooled-value index from history | Host in-process, hot shape P=2,000, Commits 2–50 mean: pack reads 2,198 -> 15, statements 6,706 -> 125, 58 ms -> 10 ms; tier P=800: 196 ms -> 18 ms. Mounted (lead's runs, quoted): median Commit 13–16 ms from 0 to 950 earlier Commits (was 18–139 ms); Store bytes/Commit 11,866 -> 7,232 (hot) (COUNT exact; ms single host samples, "not a qualified result") | `LFS:core/docs/issues/320/LONG-BRANCH.md` §7.2, §7.5 |
| 2026-10-13 | prototype `perf-proto-overlay` (not merged) | Gate experiment: one overlay DB per Workspace | Single overlay owner thread/connection serialized all Workspaces' file ops (each create+append+read = 13 kernel requests, 12 jobs, 8 write transactions); daemon capped near 7,000 file ops/s | µs per file (create+append+read), 4 writers, CPUs unlimited 1,752 -> 972 (1.80x); 2 CPUs 2,330 -> 1,627 (1.43x); 2,000-file Commit beside three writers 3.2 s -> 0.7 s. Pre-agreed 2x gate **not met**; owner went ahead anyway (INELIGIBLE, single runs) | `LFS:core/docs/issues/321/PLAN.md` §1–2 |
| 2026-10-13/14 | #321 Steps A–E, merged `9d3f44e4d` (2026-10-14) | One overlay SQLite file per Workspace (copied from a template at mount, deleted at unmount); one mutex+condvar per Workspace replaces owner thread, class queues, admission credits and completion tracking; open/directory/kernel-reference state in memory; fewer statements (Step D) | Removes cross-Workspace serialization and per-job queueing; each job is one short atomic hold under the Workspace lock | Final identity vs #319 baseline (different day): tier-1 Commit 3,646.6 -> 1,708.2 ms (-53.2%), command 7,478.2 -> 6,712.3 ms (-10.2%); four Workspaces Commit 67,434.5 -> 13,389.4 ms (-80.1%), command -26.0%; branch development Commit -74.3%; statements -12% to -61% in every case, owner jobs fall in every mounted case. **Unfavourable, recorded:** dense rewrite 500 Commit 8,292.1 -> 12,026.4 ms (+45.0%); local snapshot 25,000 command +19.2%; 64 MiB cells command +30–67 ms. (All INELIGIBLE, one sample; COUNT for statements) | `LFS:core/docs/issues/320/PROGRESS.md` §"Step E — re-qualify once" (lines ~1777–2008) |
| 2026-10-14 | #321 Step B (part of the above) | Receive workers per mount: 2 kept | Declared experiment: 1 worker faster for same-Workspace writers (678 vs 854 µs/file) but 28% slower on cold reads | Kept 2 workers (INELIGIBLE) | same, §"Worker count per mount" |
| 2026-10-14 | `0d357b191` (fuser patch, owner-authorized) | fuser receive loops allocate `max_write + 4096` (128 KiB + 4 KiB) instead of 16 MiB + 4 KiB | glibc raised its mmap threshold after the first 16 MiB free, so every later per-mount buffer was zeroed from the arena and never returned (~32 MiB resident per mounted Workspace) | Lifetime peak (sampler VmHWM): dense rewrite 154 -> 104 MB (150,488 -> 101,556 KiB, -32.5%); four Workspaces 214,220 -> 168,480 KiB (-21.4%). Memory, not time (INELIGIBLE) | `LFS:core/docs/issues/320/PROGRESS.md` §"fuser receive buffers sized by max_write"; `v2:docs/architecture/engine/kernel-routes/fuse.md` (receive buffer row) |
| 2026-10-18 | `728d8d7661` (#352 F-018) | SDK reads stop queueing behind a Commit's publication lane | Reads waited on a lock they did not need (the single-writer lane) | Range-8 replay: `fs.read` p95 96.1 -> 9.7 ms, `fs.stat` p95 46.6 -> 7.1, all reads p95 90.4 -> 24.9 ms (INELIGIBLE replay diagnostic) | `LFS:core/docs/issues/352/OPTIMIZATIONS.md` (row 007.a1 D6 F-018) |
| 2026-10-18 | `bdd4befdcc` (#352 F-022) | A mount finds its Branch by name, not by listing every Branch | Mount cost grew with Branch count | `oneshot_mount_exec.vm_steps` exponent 0.971 -> 0 (1,923/2,331 -> 1,140/1,140 VM steps) (COUNT) | same, row 009.a1 |
| 2026-10-19 | `dd361543e4` (#364 item 1) | Merge records stay resident (≤ 8 MiB), a Save keeps resolved objects (≤ 4 MiB), key windows read the primary key | `branch.merge` did repeated object reads and a materialized key window | Recorded-merge replay on r015: 2,056 -> 436 ms; statements 106,809 -> 9,957; VM steps 8.58 M -> 0.62 M; full-scan steps 13,332 -> 0; 42/42 merges identical (COUNT; time INELIGIBLE) | same, row 015.a4 |
| 2026-10-20 | F-057 merge units (design `e5c7aaae05`) | `branch.merge` takes pattern-group units | Functional/merge-semantics feature; **not a speed change**. Listed because the task named it | — | `LFS:core/docs/issues/352/F057_MERGE_UNITS.md` |
| 2026-10-20 | F-063 | Rename-over records the replaced file as chunk predecessor | **Storage** fix (git `index.lock` rename got no delta base; ~85% of `.git` index bytes saved); owner accepted only if added time < 5 ms per call | storage, not speed | `LFS:core/docs/issues/352/fixes/063-git-index-rename-predecessor.md` |
| 2026-10-20 | F-073 | Written-away-and-back file no longer counts as changed in diffs/merges; one of its three commits bounds the publication lane | Correctness, plus a lane bound | not a measured speed change | `LFS:core/docs/issues/352/fixes/073-r058-written-back-content-flag.md` |

### 1b. v2.0 era (November 2026, `origin/layerfs_v2_0_0`)

PostgreSQL round-trip work (layerfs-project #61–#97; merged as `b4fa173e40` "pg-perf" and `499078be5c` "pg-r2-reads", 2026-11-06):

| Date | Commit | Change | Mechanism | Before -> after (label) | Source |
| --- | --- | --- | --- | --- | --- |
| 2026-11-06 | `3868d1a288` (#67) | One wave for similar small files; validate the candidate before the lane | Each of 1,000 similar 23-byte files sealed its own one-object pack; validation ran inside the publication lane | `commit.similar_files_1000`: PG 12,241 -> 226 round trips; SQLite 11,211 -> 3,213 statements, 1,040 -> 41 transactions; PG lane-held round trips for an unmoved Commit 22–38 -> ~10 (COUNT) | commit `3868d1a288` |
| 2026-11-06 | `317896baad` (#68) | Stop reading the just-published tree back during Commit | Locator cache kept winners; Save keeps its metadata pack bodies (≤ 2 MiB) and pooled values (≤ 512 KiB) for the after-save reader | PG round trips per Commit: files_1000 225 -> 164; c04 455 -> 146 (COUNT) | commit `317896baad` |
| 2026-11-06 | `7a625d855d` (#69) | Digest window and overlay page 64 -> 1,024 rows | Fewer `file_roots`/digest calls per captured page | files_1000 PG 132 -> 72; C04 SQLite statements 2,751 -> 1,196 (COUNT). **Suspected** cause of a write-path regression in fs-bench-quick pg71 (below), not isolated | commit `7a625d855d` |
| 2026-11-06 | `40aa23f6fb` (#70) | Batch committed reads; 2 MiB sequential base read-ahead; sibling file lengths remembered | Fewer, larger statements per read | `read.sequential_random_64mib` PG 1,616 -> 302 round trips, SQLite 1,138 -> 505; `stat_files_1000` 1,034 -> 47 (COUNT) | commit `40aa23f6fb` |
| 2026-11-06 | `ddd1bc83cd` (#61) | PG publication waves up to 16 MiB (SQLite stays 4 MiB) | Fewer transactions per Save | `commit.random_64mib` PG 218 -> 115 round trips (COUNT) | commit `ddd1bc83cd` |
| 2026-11-06 | `b9093b61d2` (#97) | Read-ahead window 8 MiB fetched in one demand; 16 MiB per-reader cap; packs served in cohorts that fit the 2 MiB pack cache | Sequential reads of chunked committed content prefetched by the daemon | 64 MiB sequential: PG 301 -> 69 statements; SQLite 505 -> 314 (COUNT) | commit `b9093b61d2` |
| 2026-11-06 | `910f686c5a` (#97 fix) | Read-ahead streams tracked in the shared cache, not per request client | Each FUSE READ got its own client, so read-ahead **never engaged on a mount** before this fix | Mounted `cat` of 64 MiB: SQLite 1,138 -> 314 statements, 541 -> 20 Store demands (COUNT) | commit `910f686c5a` |
| 2026-11-06 | pg-perf combined, fs-bench-quick pg71 | All of the above on PostgreSQL | — | Docker PG mount 116 -> 53 ms, create 566 -> 386 ms (median of 18 cells); Commit statements fell 1.5–6x; **regression to check**: write-heavy Exec SQLite x1.33, 64 MiB Commit SQLite x1.35 (e.g. C06 Docker SQLite Commit 245 -> 332 ms) (INELIGIBLE, one sample per row) | `BENCH:result/2026-11-06/fs-bench-quick/ddd1bc83cda03d223c9dde6be86b08024db0c5e5/pg71-final/report.md` |

Exec and import speed (PR #610, merged `c421d9dbaf`, 2026-11-07):

| Date | Commit | Change | Mechanism | Before -> after (label) | Source |
| --- | --- | --- | --- | --- | --- |
| 2026-11-07 | `455111f924` | Restore image file mtimes when unpacking a namespaces Sandbox root | The unpacker dropped tar mtimes, so Python rejected every cached `.pyc` and (non-root over a read-only lower layer) recompiled imports in every process | Python imports in Sandbox 63.8 -> 26.6 ms; `exec` of that import command, CLI wall 115 -> 72 ms (20-round medians, INELIGIBLE); S1–S4 edit step 99–114 -> 59–71 ms (OVN-V2S, INELIGIBLE) | `ESD` "Outcome"; commit `455111f924`; `OVN-V2S` |
| 2026-11-07 | `f6916c64a5` | `LAYERFS_TRACE_STEP` per-step exec timing | Instrumentation; found that the helper's first cgroup move after idle costs ~20 ms of the 32 ms worker round trip | — | `ESD` |
| 2026-11-07 | `7f41d7aa30` (**dropped by owner**) | `clone3` into the command's cgroup | Removed the cheap second move, not the expensive first one | `exec true` CLI wall 44.8 -> 43.6 ms (no gain) | `ESD` |
| 2026-11-07 | `90a2856fce` | Table-driven base64 and JSON codec for worker frames | Old base64 decoder searched the alphabet per character | 8 MiB on host: unbase64 265 -> 3.9 ms, parse 283 -> 13 ms; 500 MiB import 27.8 s -> 9.9 s (`ESD` probe) / 32 s -> 9.7 s (commit msg) (INELIGIBLE) | commit `90a2856fce`; `ESD` |
| 2026-11-07 | `6ae6810b41` | Raw binary attachments in worker frames (protocol 2 -> 3) | No base64 (a third of the value) for byte payloads | (measured together with the next row) | commit `6ae6810b41` |
| 2026-11-07 | `26a9a49591` | Up to 4 `project.stage` parts in flight | Overlaps CLI read/framing, Docker Desktop port-forward transfer and worker writes | 500 MiB import 5.6, 5.6, 5.7 s (INELIGIBLE) | `ESD` |
| 2026-11-07 | `4c0f0fa494` | Init constructor-to-owner queue 4 -> 32 batches | Storing thread idled 623 ms waiting; constructor blocked 1.6 s in send | In-process Init 2,465 -> ~1,780 ms, owner wait 623 -> 10 ms; 500 MiB import 4.85, 4.78 s (INELIGIBLE) | commit `4c0f0fa494`; `ESD` |
| 2026-11-07 | PR #610 overall (`c421d9dbaf`) | — | — | S1–S4 65 MB/9,415-file import 5.2–7.8 s -> 2.2–3.1 s (2.3–2.5x); 500 MiB import 32,052 -> 4,986 ms (6.4x); traced split ~2.9 s upload + 1.8 s worker Store build (INELIGIBLE, one sample, loads 4–23 differ between arms) | `OVN-V2S` |

Commit speed (PR #612 "commit-fast", merged `b9c4adcb04`, 2026-11-08). In-process numbers are
Linux release `commit_captured`, no mount, median of 3 (IN-PROC):

| Date | Commit | Change | Mechanism | Before -> after (IN-PROC) | Source |
| --- | --- | --- | --- | --- | --- |
| 2026-11-07 | `34b909e0f5` | Build a Commit on one producer thread, store it on the calling thread | Captured reads, BLAKE3 and CDC now overlap pack hashing and SQLite; bounded queue 32 msgs × ≤256 KiB; still one construction producer (rule §3.8 amended with owner approval) | NEW500 4.71 -> 2.48 s; ADD500 7.86 -> 5.25 s; 3 small files 2.5 -> 2.7 ms; 1000 × 1 KiB 108 -> 112 ms | commit `34b909e0f5`; `v2:docs/architecture/engine/commit/construction.md` §"One producer" |
| 2026-11-07 | `82d22cdced` | Stop trying a chunk delta base whose trials keep losing (`LosingBases` memo, backoff after 8 losses) | Every appended chunk past the old EOF was offered the old last chunk as delta base | ADD500 trials 26,966 -> 22; commit 7,877 -> 5,493 ms; stored bytes identical | commit `82d22cdced` |
| 2026-11-07 | `5bf440eb08` | Locate a page-heavy wave's references a cache's worth at a time | 27,188 ids demanded against a 4,096-entry locator cache: linear eviction probes and one-by-one re-locates | ADD500 locate calls 53,500 -> 144; finish stage 1,374 -> 222 ms; commit 6,461 -> 5,161 ms | commit `5bf440eb08` |
| 2026-11-07 | `66b9a73c67` | Back off a delta base whose wins save < 1/8 | Log-append deltas saved ~1% each at 0.56 s per 100 MiB | TXT 100 MiB log append commit 1,540 -> 977 ms (-37%) for **+1.04% stored bytes** (explicit speed/storage trade, rule §3.9) | commit `66b9a73c67` |
| 2026-11-07 | `2a4ac592f8` | Hash each sealed pack once (SHA-256) | Pack bodies were hashed at seal and again in persistence validation (~0.30 s per 500 MiB pass) | 500 MiB one Save 2,758 -> 2,487 ms; 10,000 small files 323 -> 287 ms | commit `2a4ac592f8` |
| 2026-11-07 | `616cf70722` | Read a captured file's dense overlay rows in whole 128 KiB read windows | Commit read new-file bytes one 32 KiB row per owner job with zeroed windows and byte-wise decision maps | NEW500 overlay read 1,356 -> 211 ms; owner jobs 16,071 -> 4,071; zeroed bytes 1,048,576,000 -> 0 | commit `616cf70722` |
| 2026-11-07 | `2486436bc8` | Checkpoint a Disposable Store's WAL off the commit path (`wal_autocheckpoint=0` + background PASSIVE checkpoint thread, 4 MiB wake gate) | COMMIT copied the WAL into the main file every 1,000 pages | 500 MiB Save: COMMIT 471 -> 208 ms, Save 2,373 -> 2,283 ms (15 samples, host load 10–20); four rejected alternatives recorded (MEMORY journal, rollback journal, no-thread WAL, big page cache) | commit `2486436bc8` |
| 2026-11-07 | `8b9d7aba34` | Reclaim the released captured generation after the answer | 16,015 maintenance steps (~0.4–0.8 s for NEW500) ran before the Commit answered | NEW500 release 807 -> 0.2 ms (431 ms of steps now after the answer) | commit `8b9d7aba34` |
| 2026-11-08 | PR #612 overall (`b9c4adcb04`) | — | — | Mounted, through CLI: ADD500 commit 6,873 -> 1,404 ms (median of 3, 1,377–1,441); NEW500 3,928 -> 1,820 ms (median of 3, 1,207–2,943); edits on 500 MiB 15–19 ms (warm, INELIGIBLE) | `OVN-V2S` -> `OVN-CS` |

Later (2026-11-10, 2026-11-14): `58c548b3e0` reuses runtime artifacts and prepared guest images
(targets the slow first Sandbox create; commit says "No release-speed claim"); `d2f1429c05`
reads each NFS RPC record with one socket read (socket reads per request 2.00 -> ~0.9–1.0, count
only, no timing). Neither has a measured time effect.

---

## 2. Current per-operation costs (latest reports)

All through the public CLI on macOS -> Docker Desktop worker (Sandboxes as namespaces inside one
worker container), SQLite Disposable, `LAYERFS_CONSTRUCTION_WORKERS=1`, unless stated.

| Operation | Value | Build / date | Label | Source |
| --- | --- | --- | --- | --- |
| Mount (Workspace) | 13.2–21.5 ms across cells (Docker SQLite); 33.7–43.8 ms native macOS (NFS) | `76a32a9a10`, 2026-11-06 | INELIGIBLE, one sample/row | `BENCH:result/2026-11-06/fs-bench-quick/76a32a9a10e7b7b1379699ece52d45e233da5a50/pgr2-final/report.md` |
| Mount | 17.2 ms | `833d04cf44`, 2026-11-09 | INELIGIBLE | `BENCH:result/2026-11-09/juicefs-bench/833d04cf44e8e747929d50ab1170a48260d9381c/report.md` |
| Mount (in-process prototype era, v1) | 5.8 ms; unmount 3.9 ms | 2026-10-13 | INELIGIBLE | `LFS:core/docs/issues/321/PLAN.md` §1 |
| Exec route overhead, `workspace exec true` | 22–37 ms (no-op control, CLI wall) | `751b6b83`, 2026-11-07 | INELIGIBLE | `OVN-1107` |
| Exec route, no-op | 26–42 ms | `b9c4adcb`, 2026-11-08 | INELIGIBLE | `OVN-CS` S1–S4 table |
| Exec route breakdown | CLI wall 44.8 ms; worker round trip 32 ms, of which helper's first cgroup move ~20 ms, helper start 2.6 ms, bash spawn 1.2 ms, bash 1.7 ms, exec record 0.5 ms; CLI waits 2.8 ms for worker `ready` | `f6916c64a5`, 2026-11-07 | 20-round medians, INELIGIBLE | `ESD` |
| Small Commit (one or two tiny files) | C13: Commit 12.3 ms, second Commit 12.6 ms | `76a32a9a10` (before commit-fast) | INELIGIBLE | pgr2-final C13 |
| UpToDate Commit | 11.4–14.2 ms (C14, C17, C18); 14.9 ms (juicefs); 21.2 ms (Cloudflare) | 2026-11-06 / 11-09 / 11-08 | INELIGIBLE | pgr2-final; juicefs; `BENCH:result/2026-11-08/fs-bench-cloudflare-computer/98ebfdfb96fcb88a1c7275c4323e1a387349a219/report.md` |
| Commit, small code edits (5 ops) | warm medians 20–33 ms (S1–S4); cold medians 35.8–46.0 ms | `b9c4adcb` | warm INELIGIBLE; cold = 3-sample median, guest cache evicted | `OVN-CS`; `OVN-COLD` |
| Commit, 100 B edit in 10–500 MiB file | cold 19.5–28.1 ms; warm 15–19 ms (L500) | `b9c4adcb` | cold 3-sample median / warm one sample | `OVN-COLD`; `OVN-CS` |
| Commit, 1,000 new files | 89.2 ms (C01) | `76a32a9a10` | INELIGIBLE | pgr2-final |
| Commit, 64 MiB zeros | 352.6 ms (C06) | `76a32a9a10` | INELIGIBLE | pgr2-final |
| Commit, new 500 MiB file | 1,820 ms warm (1,207–2,943); 1,359 ms cold (1,329–2,780) | `b9c4adcb` | 3 samples | `OVN-CS`; `OVN-COLD` |
| Commit, append 500 MiB | 1,404 ms warm; 1,541 ms cold | `b9c4adcb` | 3 samples | same |
| Commit in-process (no route) | EDIT100 9.4 ms; 3 small files 2.7 ms; 1,000 × 1 KiB 112 ms | commit-fast branch | IN-PROC | commits `66b9a73c67`, `34b909e0f5` |
| Commit on a long branch (v1) | median 13–16 ms from 0 to 950 earlier Commits | `a6343e5f9`+`63a1ab005` | lead's mounted runs, quoted | `LFS:core/docs/issues/320/LONG-BRANCH.md` §7.5 |
| Unmount | 10.4–24.3 ms (Docker SQLite cells); 186.8 ms after the 4 × 1 GiB juicefs workload | 2026-11-06; 2026-11-09 | INELIGIBLE | pgr2-final; juicefs |
| Sandbox create, steady state | 299–414 ms (Docker SQLite, old per-container route, 11-06); 273.4 ms (Docker PG row on an already-started machine, 11-08); native 26–43 ms | — | INELIGIBLE | pgr2-final; Cloudflare 11-08 |
| Sandbox create, first on a fresh Docker machine (namespaces route) | 3,398 ms (11-09); 4,247.8 / 5,345.0 ms (11-08, the latter includes the first bundle copy); "about 10x slower" than the old build's 325–420 ms | `98ebfdfb96`, `833d04cf44` | INELIGIBLE | juicefs; Cloudflare 11-08 diagnostic table |
| Sandbox close | 208–269 ms Docker SQLite | 2026-11-06 | INELIGIBLE | pgr2-final |
| Import 65 MB / 9,415 files (Mac -> worker) | 2,191–3,157 ms | `b9c4adcb` | INELIGIBLE | `OVN-CS` |
| Import 500 MiB file | 4,200–8,318 ms (one sample per case) | `b9c4adcb` | INELIGIBLE | `OVN-CS` |
| Branch open (fork + mount) | 31–41 ms | `b9c4adcb` | INELIGIBLE, route-bound | `OVN-CS` |
| Exec, small file edit step (Python, 5 ops) | warm 58–76 ms; cold-commit runs 56.3–82.3 ms | `b9c4adcb` | INELIGIBLE / 3-sample median | `OVN-CS`; `OVN-COLD` |
| Per-file FUSE cost, one Workspace (v1) | create/append/read 242.5 / 120.9 / 189.6 µs per file (Step B, 1 Workspace) | #321 Step B | INELIGIBLE | `LFS:core/docs/issues/320/PROGRESS.md` §"Memory and threads" |
| Throughput (juicefs bench, 4 workers, own-write warm) | write big 350.77 MiB/s, read big 386.27 MiB/s, small write 2,137.3 files/s, small read 15,089.5 files/s, stat 44,630.8 files/s (JuiceFS/PG16: 1,155.51, 1,486.61, 242.9, 4,182.1, 17,141.6) | `833d04cf44` | INELIGIBLE; different persistence | juicefs report |
| Cloudflare fs-bench, Docker SQLite | sum of 12 scenarios 4,393.5 ms vs plain container 2,132.7 ms (same day, new build 4,437.8 ms): ~2.3 s LayerFS overhead; create 1000 files 257.7 ms vs 32.9 plain; rm 1000 files 458.4 vs 30.1 | `98ebfdfb96` | INELIGIBLE | Cloudflare 11-08 |

---

## 3. Remaining slow paths

### 3.1 Cold large-file sequential read: ~158 MB/s (ovnfs 906–943 MB/s)
- Measured: RF0 500 MiB cold read 3,325 ms (3,304–3,358) = 158 MB/s; ovnfs 556 ms = 943 MB/s;
  6.0x. RF5 (after 5 overwrite Commits) 3,311 ms, under 1% different, so **reading through
  history/delta chains is not the cause** (`OVN-COLD` "Read cases", 3 samples, verified eviction).
- Warm reads of the same file are about as slow: L500 read 3,058 ms (`OVN-CS`), 3,094 / 4,027 ms
  (`OVN-V2S`). Cold ≈ warm suggests (inference) the limit is the per-request service path, not
  disk I/O.
- **No root-cause analysis exists in either repository** (searched both). Candidate causes from
  source configuration (`v2:docs/architecture/engine/kernel-routes/fuse.md`, negotiated values),
  all **suspected, not measured**:
  - `max_readahead` = `max_write` = `READ_WINDOW` = 128 KiB, so the kernel asks for at most
    128 KiB per READ: ~4,000 requests for 500 MiB, i.e. ~0.83 ms per request at 158 MB/s.
  - `max_background` = 1 and `congestion_threshold` = 1: at most one asynchronous (read-ahead)
    request outstanding, so sequential reads are effectively serial round trips through the daemon.
  - 2 receive loops per mount (`RECEIVE_LOOPS` = 2).
  - Per-request daemon work: chunk lookup, pack read from SQLite, zstd decode, and BLAKE3
    re-hash of each decoded record ("every decoded record is re-hashed on read",
    `BENCH:comparision/ovnfs/ovnfs-issue-draft.md`). The daemon's 8 MiB read-ahead
    (`b9093b61d2`, `910f686c5a`) cut Store demands 541 -> 20 for 64 MiB but does not change the
    kernel's request size or concurrency.
  - Parallel readers do better: juicefs bench (4 workers) read big file at 386.27 MiB/s, own-write
    warm, INELIGIBLE.

### 3.2 Cold many-file tree read: 1.3–1.5x slower than ovnfs
- RT0 9,404 files / 65.0 MB: 5,314 ms vs 3,532 ms (1.5x); RT40 5,322 vs 3,964 ms (1.3x)
  (`OVN-COLD`). LayerFS is flat across history (RT40 vs RT0 < 1%); ovnfs slows 12%.
- Known cause (OVN-1107 warm analysis): the read walks 9,415 files one at a time and "LayerFS
  fetches each file's content in its own Store read (no prefetch across a directory)": about
  0.4 ms per file vs ~0.2 ms ovnfs and 0.04 ms plain ext4.
- Related open items: READDIRPLUS was WIP and did not compile in #359
  (`LFS:core/docs/issues/352/COMMAND_COST_RCA_20261018.md` §1); #321 left "remove repeated
  attribute requests" unchecked because the kernel issues them and removing them needs the
  daemon to own permission checks (#321 owner decisions, memory note; `LFS:core/docs/issues/321/CHECKLIST.md`).
- Per small file, one Workspace was ~16x slower than native in the #321 baseline (752 µs vs 42 µs
  per create+append+read; `LFS:core/docs/issues/321/PLAN.md` §1), "a database per Workspace does
  not change that".

### 3.3 Import: 5.6–8.0x slower than ovnfs (65 MB tree); ~4x on 500 MiB (ovnfs copy-in excluded)
- S1–S4: 2,191–3,157 ms vs ovnfs 353–393 ms (`OVN-CS`). Routes differ: LayerFS ships bytes from
  the Mac over the CLI -> worker loopback (Docker Desktop port forward); ovnfs imports a copy already
  in its container (`docker cp` 1.8–2.9 s not counted, `OVN-1106`).
- 500 MiB traced split: ~2.9 s upload + ~1.8 s worker Store build (`OVN-V2S`). Upload aggregate
  ~180–250 MB/s with 4 parts in flight; worker work per 8 MiB part ~3.5 ms, "so the limit looks like
  Docker Desktop's port forward" (`ESD`).
- Store-build side (in-process profile before the queue change): SQLite publish ~856 ms (COMMIT 481,
  ~260 of it WAL auto-checkpoint), SHA-256 of each pack twice ~600 ms, zstd probe 118 ms; storing
  thread 77% of a core, chunking thread 38% (`ESD`). Since then, double hashing (`2a4ac592f8`) and
  the in-commit checkpoint (`2486436bc8`) were removed; parallel compression or skipping the zstd
  probe for incompressible data are proposed, not built (`ESD`).

### 3.4 Brand-new large data (NEW500) and large writes
- Commit: warm 1,820 ms vs ovnfs 841 ms (`OVN-CS`); cold 1,359 vs 997 ms; write exec 2,669 vs
  1,756 ms (`OVN-COLD`). ADD500 is now about even (commit 1,541 vs 1,401 cold).
- Structural cause (source-derived): with kernel writeback off every `write(2)` is published into the
  per-Workspace overlay SQLite (32 KiB dense rows, `WRITE_WINDOW` 256 KiB) before it returns; the
  Commit then re-reads those rows, chunks (FastCDC), BLAKE3-hashes, zstd-probes, packs, SHA-256s and
  inserts into the Store. New data is therefore written twice (overlay, then Store); ovnfs writes
  the file to an upper directory and hashes it at snapshot.
- In-process, after commit-fast: NEW500 ~2.48 s with overlay read 211 ms (`34b909e0f5`,
  `616cf70722`). The remaining time is CDC/hash/compress/SQLite on one producer + one Save thread.
- juicefs bench big-file write 350.77 MiB/s vs JuiceFS 1,155.51 MiB/s (INELIGIBLE, different durability).
- Recorded regressions not resolved: pg71 write-heavy Exec x1.33 and 64 MiB Commit x1.35 on SQLite
  after `ddd1bc83cd` (suspect `7a625d855d`, not isolated); #321 dense rewrite 500 Commit +45%
  (8,292.1 -> 12,026.4 ms, "located, not attributed"); a ~500 MB Store write is bimodal on the shared
  VM because another disk writer throttles buffered writes (#321 Store-write stall follow-up,
  `LFS:core/docs/issues/320/PROGRESS.md` §"Store-write stall cause").

### 3.5 Fixed per-call costs
- Exec: ~20 ms of the ~32 ms worker round trip is the helper's first cgroup move after idle
  (kernel slow path); only a helper that never moves (or a cgroup mount with `favordynmods`) would
  remove it; owner chose not to pursue (`ESD`).
- First Sandbox create on a fresh Docker machine 3.4–5.3 s (namespaces route); later creates were
  not measured (Cloudflare 11-08). `58c548b3e0` targets artifact reuse; no timing claimed.
- Native macOS (NFS loopback): per-file loops 2.8–8x Docker (e.g. `stat 1000 files` 7,766 ms vs
  956 ms) because each loop step launches a macOS process on an NFS mount (Cloudflare 11-08).

---

## 4. Structural design choices that matter for speed

1. **One global Store, one writer, short holds** (owner decision 2026-10-18,
   `LFS:core/docs/architecture/83-store-concurrency-model.md`). Construction, encoding,
   compression and object saves run before the publication lane; the lane holds only head check,
   merge-if-moved and publish. Work that grows with tree size/history/Branch count inside the lane
   is a defect (F-025). Reads never take the writer (F-018 fixed a violation: read p95 96.1 -> 9.7 ms).
   Uncontended cost is the product cost; replay reports uncontended and contended series separately.
   Validation moved before the lane in `3868d1a288` (PG lane round trips 22–38 -> ~10).
2. **One construction producer per Commit** (`LAYERFS_CONSTRUCTION_WORKERS=1` in every measured
   run, root AGENTS §3.8). Since `34b909e0f5` the producer runs on its own thread feeding the
   calling thread's `!Send` Save through a bounded queue (32 messages, ≤ 256 KiB/512 objects per batch;
   8 MiB in flight by default), which nearly halved NEW500 in process. `LAYERFS_CONSTRUCTION_WORKERS`
   selects nothing in engine source; Project Init alone keeps a parallel profile
   (`v2:docs/architecture/engine/commit/construction.md`).
3. **Per-Workspace overlay isolation** (#321): one SQLite file per mounted Workspace (MEMORY journal,
   synchronous OFF, EXCLUSIVE locking, mmap off), one mutex+condvar per Workspace, open state in
   memory, unmount deletes the file; no shared memory budget (`LFS:core/docs/architecture/21-daemon-owner.md`
   "Issue 321 final model"). Removed the ~7,000 file-ops/s single-owner ceiling.
4. **Kernel writeback off, `FOPEN_KEEP_CACHE` on, 60 s entry/attr TTL**: every `write(2)` is published
   before it returns, which makes capture exact (acknowledged writes are in the overlay) but puts all
   data through SQLite at write time (`v2:docs/architecture/engine/kernel-routes/README.md`).
5. **Small negotiated windows**: `max_write` = `max_readahead` = 128 KiB, `max_background` = 1; receive
   buffers sized to `max_write + 4096` (memory fix). These bound per-request memory and keep requests
   short under the Workspace lock, and are the prime suspects for the read ceiling (§3.1).
6. **Disposable Store profile** (WAL, `synchronous=OFF`, background PASSIVE checkpoint since
   `2486436bc8`); Durable is NOT_RUN by owner direction. No fsync on Workspace backing.
7. **Bounded batching everywhere instead of input-sized structures**: 1,024-row pages and digest
   windows, 8 MiB read-ahead with 16 MiB per-reader cap, 2 MiB pack cache, 4 MiB SQLite / 16 MiB PG
   publication waves, delta-trial backoff memo ≤ 4,096 bases.
8. **Deferred maintenance**: reclamation after the Commit's answer (`8b9d7aba34`), checkpoint off the
   commit path.
9. **No retry, no busy handler**: queues and the Linux directory lock replace `Busy`; a lost reply is
   not replayed. Removes retry storms but means waits are visible as latency.
10. **Content model**: FastCDC (8/16/32 KiB) chunks, BLAKE3 identities, extent B-tree of ≤128-entry pages,
    deltas; a 100 B edit to a 500 MiB file commits in 15–28 ms and stores 3–8 KB, while ovnfs (whole-file
    copy-up) takes 0.6–1.1 s and stores the whole file (`OVN-CS`, `OVN-COLD`).

---

## 5. Methodology: strengths and weaknesses

**Strengths**
- Cold eviction protocol in `OVN-COLD`: per-file `fsync` then `posix_fadvise(DONTNEED)` from a
  throwaway read-only helper container on the system's own volume, with util-linux `fincore`
  before/after; a boundary counts only at 0 resident bytes. 1,014 boundaries all reached 0 bytes
  (76.4 GB dropped in edit runs, 9.9 GB in read cases); eviction time (0.19–3.26 s) outside timers.
  Read cases restart the whole LayerFS machine (or ovnfs daemon) before each read.
- Every head verified against a host oracle (78 edit runs, 24 cold reads; SHA-256 of large files).
- Pinned identities: commit SHAs, CLI/daemon/bundle SHA-256s, bundle seal, image digests, harness
  commits, toolchain 1.85.1 (e.g. Cloudflare 11-08 and juicefs 11-09 identity tables).
- Exact counters as primary evidence (statements, round trips, owner jobs, delta trials, locate calls),
  times secondary; one sample per case/arm, no best-of; failed and superseded attempts retained
  (`OVN-1106` "Superseded attempts", juicefs retained 25 s FAIL).
- Speed and storage measured together and trade-offs stated (`66b9a73c67`: -37% commit time for
  +1.04% bytes; `2486436bc8` lists four rejected options with numbers).
- Route diagnostics separate route from system work (ovnfs in-container `snap --wait` 8.4–12.2 ms vs
  63–66 ms via `docker exec`, `OVN-1107`).
- Owner rule: time differences under 5 ms per call are not regressions (absolute floor), counters unaffected.

**Weaknesses**
- Docker Desktop on macOS (Apple Virtualization, 8 vCPU, 7.75–8 GiB) shared with other agents' builds
  and campaigns; host 1-minute load 4–29 across reports. One sample per row nearly everywhere.
- Cold covers only the guest page cache: daemon in-memory caches stay warm between edit and commit;
  VM filesystem metadata caches and the macOS cache under the VM disk image are not controlled; program
  files not evicted (`OVN-COLD` "Not controlled"). Flushing before the timer moves write-back out of
  commit timings (ovnfs DEL500 0.59 s cold vs 1.14 s warm).
- Different routes: LayerFS CLI -> worker (Docker Desktop port forward, ~180–250 MB/s cap on import)
  vs `docker exec` into ovnfs (50–100 ms per call). Small-op ratios are route-bound; import compares
  Mac-to-worker against in-container copies.
- Arms often not contemporaneous: `OVN-CS` reuses ovnfs runs from 2026-11-07 at different loads
  (ovnfs L500 at load 14–15 vs LayerFS 7–8); `OVN-V2S` "before" arm at load 21–23.
- Different persistence: LayerFS Disposable vs ovnfs unsynced ext4; JuiceFS PG16 with fsync on.
- Many tool workloads delete their data before Commit (Cloudflare, juicefs: Commit = UpToDate), so
  they don't exercise Commit or storage.
- Storage measured differently (SQLite file + WAL vs `du` allocated).
- No public CLI protocol proves a cold live Workspace before Commit (fs-bench-quick reports).

---

## 6. Paper-ready performance engineering summary (~700 words)

**Performance engineering.** LayerFS's latency budget is dominated by three paths: the FUSE request
path during command execution, Commit (construct, save, publish), and bulk import. We optimized each by
first instrumenting it with exact counters (SQL statements, Store round trips, overlay jobs, delta
trials) and then removing work rather than adding parallelism, under a standing rule that every
measured operation uses one construction producer.

*Isolating Workspaces.* The first daemon design served all mounted Workspaces through one overlay
database owned by a single thread with class queues and admission credits. With four concurrent
writers each spent 2.0 s of 3.45 s queued, and the daemon saturated near 7,000 file operations per
second. We gave each Workspace its own SQLite overlay file, copied from a pre-built template at mount
and deleted at unmount, guarded by one mutex held only for a short atomic job. A prototype showed
1.4–1.8x lower per-file latency with four writers and cut a 2,000-file Commit beside three writers
from 3.2 s to 0.7 s; the merged design cut statements by 12–61% across our case set and the
four-Workspace Commit time by 80%.

*Keeping Commit flat in history length.* Commit cost initially grew with branch length because each
Commit re-read every inode leaf under an edited branch page and rebuilt a value index from history.
Storing inode values as Store rows and reading only the children a change reaches reduced a Commit at
2,000 prior Commits from 2,198 pack reads to 15, giving a flat 13–16 ms median mounted Commit across
950 Commits.

*Short holds on the single writer.* All committed state lives in one SQLite (or PostgreSQL) Store with
one writer. Rather than shard, we keep the publication lane to head check, merge-if-moved and publish;
construction, compression and validation happen before it, and reads never take it. Removing a read's
dependency on the lane dropped p95 read latency in a multi-agent replay from 96 ms to 10 ms.

*Large-file Commit.* Content-defined chunking and an extent tree make small edits cheap: a 100-byte
edit to a 500 MiB file commits in 15–28 ms and stores 3–8 KB. Large new data was slow: appending
500 MiB committed in 6.9 s. Profiling found seven independent costs, each removed with its own
measurement: (i) construction and the Save ran on one thread, now a producer thread feeds the Save
through a bounded queue (in-process NEW500 4.7 s to 2.5 s); (ii) every appended chunk tried a delta
against the old last chunk and lost (26,966 trials to 22 via a backoff memo); (iii) a page-heavy final
wave thrashed a 4,096-entry locator cache (53,500 locate calls to 144); (iv) each pack was SHA-256
hashed twice; (v) the overlay read new-file bytes one 32 KiB row per job with zeroed buffers (1.36 s
to 0.21 s); (vi) SQLite WAL checkpoints ran inside COMMIT (471 to 208 ms, now on a background thread);
(vii) overlay reclamation ran before the answer (0.8 s, now after). End to end, a 500 MiB append now
commits in 1.40 s (ovnfs 1.35 s) while storing only the new bytes. One change trades 1% storage for
37% less Commit time on log appends; we report both.

*Import and exec.* Import from macOS into the Linux worker was codec-bound: base64 framing of 8 MiB
parts took 16 MB/s. Table-driven codecs, raw binary attachments, four parts in flight and a deeper
constructor queue cut a 500 MiB import from 32.1 s to 5.0 s and a 65 MB, 9,415-file tree from
5.2–7.8 s to 2.2–3.1 s. For exec, the Sandbox image unpacker dropped file mtimes, invalidating every
cached Python bytecode file; restoring them cut the edit step from ~105 ms to ~62 ms.

*Remaining slow paths.* Three are open. (1) Cold sequential reads of a large file reach only 158 MB/s
against ovnfs's 943 MB/s; warm reads are equally slow and history depth has no effect, pointing at the
request path. We negotiate 128 KiB reads with one outstanding background request, which serializes
read-ahead through the daemon; we have not yet profiled this. (2) Cold whole-tree reads of 9,400 small
files are 1.3–1.5x slower, because each file's content is a separate Store read with no
directory-level prefetch. (3) Brand-new bulk data is written twice, first into the overlay at
`write(2)` time (kernel writeback is off so every acknowledged write is captured exactly) and again
into the Store at Commit; creating and committing a new 500 MiB file takes 2.7 s and 1.4 s cold against
ovnfs's 1.8 s and 1.0 s. Import remains 6–8x slower than ovnfs, partly because our route crosses the
Docker Desktop port forward (~200 MB/s) while ovnfs imports from inside its container.

*Methodology.* All numbers come from one sample per case on a shared Apple-silicon host running Docker
Desktop, so we treat timings as diagnostics and counters as evidence. For cold claims we evict each
system's files with `fsync` + `posix_fadvise(DONTNEED)` and accept a boundary only when `fincore`
reports zero resident bytes (1,014 of 1,014 did). Every head is checked against an oracle, and binaries,
bundles and images are pinned by hash. Daemon caches, VM metadata caches and the host cache under the VM
remain uncontrolled, and the two systems are reached through different routes.
