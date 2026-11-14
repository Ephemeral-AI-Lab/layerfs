# E. Evaluation notes for the LayerFS paper

Compiled 2026-11-14 from published reports (read-only). Nothing was rebuilt or rerun.
Abbreviations: `LFB` = `~/Ephemeral-AI-Lab/layerfs-benchmark`, `LFS` = `~/Ephemeral-AI-Lab/layerfs`.
MB = 10^6 B, MiB = 2^20 B. "INELIGIBLE" = the reports' own label for diagnostic timings (no enforced cold state, shared host).

**Universal caveats for the paper**
- Every speed number in the repository is labeled diagnostic. The only timings with an enforced cold boundary are the ovnfs cold rerun (§1–2): 3 samples, every eviction verified at 0 resident bytes, guest page cache only.
- Storage numbers are exact byte counts and do not depend on cache state. Most rows are one sample. Repeats show page-level noise of 0 to 32 KiB (fs-storage-dsh D10 repeat +28,672 B; micro M03 repeat 0 / +8,192 B).
- Host for almost everything: Apple M3 Max (14 cores, 36 GiB), macOS 26.4, Docker Desktop VM (linuxkit 6.12.76 arm64, 8 vCPU, about 7.75–8 GiB), shared with other agents. 1-minute load was 4–16. store-transfer ran on an Apple M2 host. Nothing ran on native x86-64 Linux.
- Store profile: SQLite Disposable (WAL, synchronous=OFF). PostgreSQL rows ran with synchronous_commit=off. `LAYERFS_CONSTRUCTION_WORKERS=1`.
- Build identities differ by table: v2 storage at `7691ac20`; six-suite speed and correctness at `76a32a9a`; ovnfs comparisons at `b9c4adcb` (cold), `751b6b83` and `b4fa173e`; juicefs at `833d04cf`; Cloudflare at `98ebfdfb`; 157-snapshot result at `e5c7aaae05` (v1.0.0). Do not mix them in one claim without saying so.

---

## 1. Storage vs ovnfs

**Source:** `LFB/comparision/ovnfs/rerun-2026-11-08-cold/report.md` (directory name spelled "comparision" in the repo).
**Systems:** LayerFS `layerfs_v2_0_0` @ `b9c4adcb04f3445d1488698d4956f2010798a6d9` (bundle built from `bd21785a1f`, same tree) vs ovnfs `stencil-hq/ovnfs` @ `5d54cb0aec474c9aad90f43214ae4ba2bfab1d89`.
**Samples:** 3 per case (storage is deterministic up to page noise). 78/78 edit runs PASS, every head matched its oracle.
**Label:** storage numbers are exact bytes. LayerFS = SQLite file + WAL; ovnfs = `du` of its state folder (allocated). The L10 growth line uses allocated bytes for both.
**Workload:** S1–S4 use the fs-storage-history edit model over the DSH checkpoint-157 tree (9,404 files + 11 symlinks, 65,020,822 B), 5 operations per commit. Large-file cases use incompressible seeded data.

### 1a. Histories (65 MB source tree)

| Case | Shape | States | Raw (MB) | Distinct files (MB) | ovnfs stored (MB) | LayerFS stored (MB) | Raw÷stored ovnfs | Raw÷stored LayerFS | Added/commit ovnfs (KB) | Added/commit LayerFS (KB) |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| S1 | 1 branch × 40 commits | 41 | 2,665.7 | 65.6 | 26.37 | 23.97 | 101× | 111× | 32.2 | 5.1 |
| S2 | 5 branches × 3 commits | 16 | 1,040.3 | 65.0 | 26.09 | 23.88 | 40× | 44× | 66.6 | 5.5 |
| S3 | fork chain 4 deep × 3 | 13 | 845.1 | 65.0 | 25.59 | 23.81 | 33× | 35× | 41.6 | 6.5 |
| S4 | 5 branches × same commit | 6 | 390.1 | 64.7 | 25.62 | 23.78 | 15× | 16× | 105.7 | 4.9 |

- ovnfs ends 7.6–10.0% larger in total (the base tree dominates). Per commit, ovnfs adds 6–22× more.
- In LayerFS's own accounting, delta+zstd shrinks each commit's canonical bytes 11–28× (S1: 2,356 KB → 109 KB over 40 commits). About two-thirds of what remains is metadata, and the SQLite Store grows 1.5–2.6× beyond that.
- **History of this comparison:** the 2026-11-06 run against the older ovnfs `cdb3fb6`, which had no packing or compression, gave ovnfs 3.82–4.35× larger (`LFB/comparision/ovnfs/README.md`). That gap closed when ovnfs `5d54cb0` added packing, compression and deltas. Cite 7.6–10%, not 3.8–4.4×.

### 1b. Large file, small edits (incompressible)

| Case | File | Edit per commit | Commits | Raw (MB) | ovnfs stored (MB) | LayerFS stored (MB) | Stored÷raw ovnfs | Stored÷raw LayerFS | Added/commit ovnfs | Added/commit LayerFS |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| L100 | 100 MiB | overwrite 100 B | 10 | 1,153.4 | 1,154.7 | 106.3 | 100% | 9% | 104.9 MB | 5.3 KB |
| L10 | 10 MiB | overwrite 100 B | 20 | 220.2 | 221.5 | 10.9 | 101% | 5% | 10.5 MB | 5.5 KB |
| I10 | 10 MiB | insert 100 B (shifts rest) | 10 | 115.3 | 116.6 | 10.9 | 101% | 9% | 10.5 MB | 5.3 KB |
| A5 | 5 MiB | append 16 KiB | 20 | 113.5 | 18.7 | 5.9 | 16% | 5% | 613 KB | 18.4 KB |
| L500 | 500 MiB | overwrite 100 B | 5 | 3,145.7 | 3,146.9 | 530.4 | 100% | 17% | 524.3 MB | 7.4 KB |

- Ratio of stored bytes: 3.2× (A5) to 20.3× (L10), about 3–20×. LayerFS stores a 100 B overwrite as 153 B of payload at 10, 100 and 500 MiB; the rest is metadata (L100 over 10 Commits: 41,872 of 43,402 B new stored bytes are metadata; L500: 5.9 KB stored and 7.4 KB Store growth per Commit, 28 ms cold Commit).
- ovnfs keeps a full copy per changed version: no reflinks on ext4, and its delta chain stops working for large files.
- Earlier warm single-sample run (`largefile-2026-11-07/report.md`, LayerFS `751b6b83`): L100 ovnfs 1,154,654,208 B vs LayerFS 106,303,488 B. Per-commit ratios: 24,158× (L100), 2,727× (L10), 2,496× (I10), 36× (A5).

### 1c. 500 MiB create / append / remove / cut (single commit)

| Case | What the commit records | Stored by commit: ovnfs | Stored by commit: LayerFS |
|---|---|--:|--:|
| NEW500 | brand-new 500 MiB file | 524.3 MB | 530.3 MB |
| ADD500 | append 500 MiB to a 500 MiB file | 1,048.6 MB | 530.2 MB |
| RM500 | remove a 500 MiB file | 24.6 KB | 0 |
| DEL500 | cut last 500 MiB of a 1000 MiB file | 524.3 MB | 8.2 KB |

Exact bytes from the warm run (`rerun-2026-11-08-commit-speed/report.md`, same builds): NEW500 524,316,672 vs 530,235,392; ADD500 1,048,604,672 vs 530,190,336; RM500 24,576 vs 0; DEL500 524,316,672 vs 8,192. L500 over 5 commits: 2,621,501,440 vs 32,768.

## 2. Speed vs ovnfs (cold commit)

**Source, samples and builds:** as §1 (`rerun-2026-11-08-cold/report.md`): median of 3, range in parentheses, ms. Times include each system's control route: the LayerFS CLI to its worker, `docker exec` for ovnfs.

**Cold method:**
- Before every commit, both sides flush and evict with `posix_fadvise(DONTNEED)` and confirm 0 resident bytes with `fincore`: 1,014 evictions, all at 0 bytes.
- Not controlled: daemon in-memory caches, VM metadata caches, and the macOS cache beneath the VM disk.
- The file-operation exec is NOT preceded by an eviction, so the exec column is not labeled cold.

| Case | Exec (file op): ovnfs | Exec: LayerFS | Commit cold: ovnfs | Commit cold: LayerFS | LayerFS speedup exec / commit |
|---|--:|--:|--:|--:|--:|
| S1 | 105.6 (103.3–137.9) | 82.3 (63.9–93.8) | 83.8 (80.1–103.8) | 41.9 (34.7–50.6) | 1.3× / 2.0× |
| S2 | 94.7 (93.3–95.3) | 56.3 (54.9–62.1) | 77.5 (72.5–79.4) | 35.8 (35.6–39.1) | 1.7× / 2.2× |
| S3 | 91.4 (89.8–95.5) | 57.2 (54.8–58.0) | 73.7 (73.0–75.8) | 39.1 (38.4–39.4) | 1.6× / 1.9× |
| S4 | 92.7 (91.0–101.1) | 60.7 (58.1–63.6) | 70.6 (69.3–77.2) | 46.0 (45.9–46.3) | 1.5× / 1.5× |
| L100 | 123.8 (118.6–129.6) | 46.1 (44.9–48.0) | 221.1 (181.9–250.9) | 24.5 (22.0–24.8) | 2.7× / 9.0× |
| L10 | 103.2 (92.5–105.5) | 47.4 (45.8–48.0) | 190.9 (173.2–202.3) | 24.4 (23.0–24.4) | 2.2× / 7.8× |
| I10 | 112.8 (107.6–113.6) | 73.7 (72.6–78.8) | 197.9 (182.9–200.2) | 25.5 (22.6–27.3) | 1.5× / 7.8× |
| A5 | 95.3 (90.0–103.0) | 48.5 (45.9–51.2) | 145.2 (133.8–147.9) | 19.5 (19.1–22.8) | 2.0× / 7.4× |
| L500 | 217.2 (206.5–217.8) | 44.5 (44.1–55.6) | 603.9 (583.9–624.7) | 28.1 (25.8–38.9) | 4.9× / 21× |
| NEW500 | 1,756 (1,524–2,974) | 2,669 (2,436–4,364) | 997 (960–1,931) | 1,359 (1,329–2,780) | ovnfs faster 1.5× / 1.4× |
| ADD500 | 1,816 (1,742–1,853) | 1,874 (1,863–3,955) | 1,401 (1,170–1,726) | 1,541 (1,488–3,240) | ovnfs 1.03× / 1.1× |
| RM500 | 85 (80–113) | 48 (48–48) | 68 (60–78) | 19 (18–27) | 1.8× / 3.6× |
| DEL500 | 972 (718–1,437) | 47 (43–66) | 593 (569–648) | 51 (50–66) | 21× / 11.7× |

Summary ranges (report's own): histories exec 1.3–1.7×, commit 1.5–2.2×; large files exec 1.5–4.9×, commit 7.4–21×; ovnfs faster on new 500 MiB data.

Caveats:
- For small histories, the commit gap is comparable to the route cost. An ovnfs commit takes 8–12 ms inside its container (`rerun-2026-11-07/report.md`), and a no-op route costs about 50–100 ms for `docker exec` and 26–54 ms for the LayerFS CLI.
- Flushing before the timer moves write-back out of the commit. This helps ovnfs on DEL500 and L500 (cold faster than warm).

### 2b. Cold reads (separate read cases, 3 reads each, full cold boundary)

The boundary is a machine or daemon restart plus eviction, as described in §2.

| Case | What is read | ovnfs ms | LayerFS ms | ovnfs MB/s | LayerFS MB/s | ovnfs faster |
|---|---|--:|--:|--:|--:|--:|
| RT0 | 65 MB tree, 9,404 files, as imported | 3,532 (3,527–3,677) | 5,314 (5,211–5,413) | 18.4 | 12.2 | 1.5× |
| RT40 | same tree after S1's 40 commits | 3,964 (3,894–4,078) | 5,322 (5,230–5,425) | 16.4 | 12.2 | 1.3× |
| RF0 | one 500 MiB file, as imported | 556 (541–656) | 3,325 (3,304–3,358) | 943 | 158 | 6.0× |
| RF5 | that file after 5 commits | 579 (544–583) | 3,311 (3,278–3,340) | 906 | 158 | 5.7× |

- LayerFS reads are flat across history (RT40 vs RT0 and RF5 vs RF0 differ by less than 1%), so the slow large-file read is not caused by delta chains.
- Import is warm only. ovnfs imports 467–680 ms and LayerFS 2,239–3,111 ms on S1–S4, with different routes (LayerFS ships the data from the Mac).

## 3. Storage breakdown (LayerFS v2, `7691ac20`)

**Common for all 3a–3c:**
- LayerFS `7691ac20b0a8c8b741dd2b701e7aa77b63aaa8a3` = `layerfs_v2_0_0` @ `8b701f42f0` + PRs #92, #91, #90. Those PRs were pending owner merge when measured, so this is the "final identity candidate".
- Docker machine, SQLite and PostgreSQL 17 Stores, one sample per row. Storage is exact bytes; wall times INELIGIBLE.
- Policy: CDC GEAR 8/16/32 KiB for files of 128 KiB and up; zstd prefix deltas, depth caps 16/8/8; zstd-3.

**Definitions (from the reports):**
- Raw = Σ logical bytes of every committed snapshot.
- Stored = net Store bytes (SQLite file+WAL; PostgreSQL schema bytes).
- Exact dedup = raw ÷ Σ new_canonical.
- Delta+zstd = Σ new_canonical ÷ Σ new_stored.
- Store overhead = stored ÷ Σ new_stored.
- Raw÷stored = dedup × delta+zstd ÷ overhead.

### 3a. Real repository history (fs-storage-dsh)

Source: `LFB/result/2026-11-06/fs-storage-dsh/7691ac20b0a8c8b741dd2b701e7aa77b63aaa8a3/report.md`. 4/4 rows PASS, verified at state 157.
- D10 replays every 10th of the 157 DSH checkpoints (17 states); D03 replays every 3rd (53 states).
- Each state is written as in-place rewrites of changed files, then committed.

| Case | DB | Raw (MB) | Stored (MB) | Raw÷stored | Exact dedup | Delta+zstd | Store overhead |
|---|---|--:|--:|--:|--:|--:|--:|
| D10 | SQLite | 561.0 | 47.9 | 11.71× | 1.48× | 9.26× | 1.17× |
| D10 | PostgreSQL | 561.0 | 53.3 | 10.53× | 1.48× | 9.26× | 1.30× |
| D03 | SQLite | 1,676.8 | 61.0 | 27.48× | 2.84× | 11.59× | 1.20× |
| D03 | PostgreSQL | 1,676.8 | 68.4 | 24.51× | 2.84× | 11.59× | 1.34× |

Exact bytes:
- D10 SQLite: net 47,927,296; Σ new_canonical 380,287,990; Σ new_stored 41,052,735 (metadata 821,887).
- D03 SQLite: net 61,014,016; Σ new_canonical 590,109,407; Σ new_stored 50,908,808 (metadata 1,640,102).
- Against the Phase 0 reference `b4fa173e40`: net fell 16–24%.

### 3b. Single-mechanism micro cases (fs-storage-micro)

Source: `LFB/result/2026-11-06/fs-storage-micro/7691ac20b0a8c8b741dd2b701e7aa77b63aaa8a3/report.md`. 16/16 rows PASS, plus M03 repeats.

| Case | Workload | DB | Raw (MB) | Stored (MB) | Raw÷stored | Exact dedup | Delta+zstd | Overhead |
|---|---|---|--:|--:|--:|--:|--:|--:|
| M01 | 10,000 random files 0–4 KiB, 1 commit | SQLite | 20.6 | 22.6 | 0.91× | 0.94× | 1.05× | 1.09× |
| | | PG | 20.6 | 24.9 | 0.83× | 0.94× | 1.05× | 1.20× |
| M02 | 50 MB tree + 10 copies | SQLite | 600.0 | 35.4 | 16.97× | 11.90× | 1.45× | 1.01× |
| | | PG | 600.0 | 37.0 | 16.21× | 11.90× | 1.45× | 1.06× |
| M03 | 100 text files, 20 commits of 1%-line in-place edits | SQLite | 85.7 | 2.9 | 29.27× | 1.00× | 32.12× | 1.09× |
| | | PG | 85.7 | 3.4 | 25.52× | 1.00× | 32.12× | 1.25× |
| M04 | same, write-temp + rename-over | SQLite | 85.7 | 3.1 | 28.02× | 1.00× | 32.04× | 1.14× |
| | | PG | 85.7 | 3.5 | 24.45× | 1.00× | 32.04× | 1.31× |
| M05 | same, new path + unlink | SQLite | 85.7 | 3.4 | 25.15× | 1.00× | 30.07× | 1.19× |
| | | PG | 85.7 | 4.0 | 21.49× | 1.00× | 30.07× | 1.39× |
| M06 | 64 MiB random file, 5 small edits (insert/overwrite/append/delete) | SQLite | 404.8 | 68.9 | 5.87× | 5.90× | 1.00× | 1.01× |
| | | PG | 404.8 | 71.7 | 5.64× | 5.90× | 1.00× | 1.05× |
| M07 | 64 MiB random + zeros + text | SQLite | 402.7 | 93.8 | 4.29× | 2.99× | 1.45× | 1.01× |
| | | PG | 402.7 | 97.7 | 4.12× | 2.99× | 1.45× | 1.05× |
| M08 | 33.5 MB SQLite DB, 10 × 1% row updates + VACUUM | SQLite | 402.5 | 29.3 | 13.73× | 3.21× | 4.44× | 1.04× |
| | | PG | 402.5 | 31.4 | 12.81× | 3.21× | 4.44× | 1.11× |

Notable:
- M01 costs 2,261 B per tiny file on SQLite (182.5 B of overhead beyond the packs).
- In M02, the second commit (500 MB of copies) stored 15,938 B of new data, all metadata, with 0 payload.

### 3c. History shapes (fs-storage-history)

Source: `LFB/result/2026-11-06/fs-storage-history/7691ac20b0a8c8b741dd2b701e7aa77b63aaa8a3/report.md`. 14/14 rows PASS, every Branch head verified.
- The base tree (65,020,822 B) is imported with `project init`, which the Commit list does not cover. **The report therefore gives no exact-dedup, delta+zstd or overhead split for H01–H07.** Only raw÷stored and the Commit-pack totals below are available.
- Each Commit applies 5 operations (70% edit, 10% new, 5% delete, 10% rename, 5% append).

| Case | Shape | Commits | Raw (MB) | Stored SQLite (MB) | Raw÷stored SQLite | Raw÷stored PG | Σ new_stored (B) | of which metadata (B) | Net÷Git pack |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|
| H01 | 1 branch × 500 commits | 500 | 32,477.4 | 26.5 | 1,225.9× | 1,117.4× | 1,495,019 | 978,708 | 1.160 |
| H02 | 50 branches × 5 | 250 | 16,319.9 | 25.0 | 653.8× | 598.6× | 606,622 | 358,021 | 1.132 |
| H03 | 50 identical branches | 50 | 3,315.8 | 23.8 | 139.5× | 127.9× | 155,997 | 155,647 | 1.114 |
| H04 | 10 nested forks | 50 | 3,315.3 | 23.8 | 139.1× | 128.8× | 141,707 | 92,040 | 1.110 |
| H05 | 20 projects, ONE Store | 100 | 7,801.8 | 28.3 | 275.5× | 257.5× | 252,721 | 148,733 | 1.309 |
| H06 | 20 projects, 20 Stores | 100 | 7,801.8 | 471.4 | 16.55× | 15.30× | 252,777 | 148,789 | 1.104 |
| H07 | combined (4 projects, 20 branches) | 200 | 13,262.8 | 25.4 | 522.4× | 479.6× | 543,266 | 339,027 | 1.158 |

Per-unit costs:
- H01 SQLite: growth 5,939 B per Commit. Of that, new_stored is 2,990 B/Commit (1,033 B payload and 1,957 B metadata); the rest is Store pages.
- Per-branch growth on SQLite: H03 identical branches 5,407 B (own new_stored 3,120 B, almost all metadata); H02 27,607 B; H04 27,853 B; H07 60,621 B.
- Fitted cost per operation kind (H01, H02, H04, H05, H07; a least-squares diagnostic): edit 414 B, new file 1,360 B, delete 1,111 B, rename 464 B, append 526 B of new_stored.

**H05 vs H06, cross-project sharing:**

| Backend | H05 net (one Store) | Σ H06 net (20 Stores) | Σ H06 ÷ H05 | H05 Init | Σ H06 Init | Init gain |
|---|--:|--:|--:|--:|--:|--:|
| SQLite | 28,315,648 | 471,351,296 | 16.65× | 27,758,592 | 470,867,968 | 16.96× |
| PostgreSQL | 30,294,016 | 509,919,232 | 16.83× | 29,564,928 | 508,297,216 | 17.19× |

- The first project's Init cost 23,568,384 B on SQLite; each later Init into the same Store cost 217–225 KB (mean 220,537 B).
- All 20 projects share one base tree, so the ideal ratio is about 20×. 16.65× is what was achieved; the remainder is per-project metadata and Init records.

## 4. Git pack reference

Git pack + idx is built with `git repack -a -d -f -F --window=10 --depth=50 --threads=2` (git 2.39.5 in v2 reports; git 2.47.1 for dsh-dedup v1.0.0). Git keeps no mountable live view, no per-file executable metadata model beyond Git's, and no Workspace overlay. It is a compression reference, not a peer system.

| Workload | Report / build | LayerFS net (B) | Git pack+idx (B) | LayerFS ÷ Git | Σ new_stored ÷ Git |
|---|---|--:|--:|--:|--:|
| D10, 17 states | fs-storage-dsh `7691ac20`, SQLite | 47,927,296 | 41,805,317 | 1.146 | 0.982 |
| D10 | same, PostgreSQL | 53,272,576 | 41,805,317 | 1.274 | 0.982 |
| D03, 53 states | same, SQLite | 61,014,016 | 48,166,223 | 1.267 | 1.057 |
| D03 | same, PostgreSQL | 68,419,584 | 48,166,223 | 1.420 | 1.057 |
| 157 states (stride 1) | dsh-dedup v1.0.0 `e5c7aaae05`, SQLite, Store allocated | 100,085,760 | 55,068,665 (51,985,325 pack + 3,083,340 idx) | 1.8175 | — (payload 67,488,833 + metadata 5,507,608 + other 27,089,319) |
| H01–H07 | fs-storage-history `7691ac20`, SQLite | (§3c) | 21.3–22.8 MB (H06 Σ 427.1 MB) | 1.104–1.309 | — |
| M01–M08 | fs-storage-micro `7691ac20`, SQLite | (§3b) | (report table) | 1.011–1.245 except M05 0.113 and M08 2.254 | — |

- M05 is new-path edits: Git cannot delta across paths with this repack and stores whole files (30.3 MB), while LayerFS uses its similarity ring.
- M08 is SQLite page churn: Git deltas byte-level better (13.0 MB vs 29.3 MB).
- The other side references (Σ tar|zstd-3, whole-file unique, approximate CDC oracle) are in each report. Example, D03 SQLite: net is 0.141× of Σ tar|zstd-3 and 0.105× of whole-file unique bytes.

## 5. Btrfs reference: "157 snapshots in 100 MB, 19.9× smaller than Btrfs + zstd"

**Source of the README claim:** `LFS/README.md` lines 94, 128 and 175. The line 128 link points to layerfs-benchmark `data/dsh-dedup/README.md`. Locally the data lives at `LFB/.archive/data/dsh-dedup/README.md`, with `v1.0.0/benchmark-storage.csv` and `v1.0.0/btrfs-reference.csv`. (The README's "4.94 GB of trees" is 4,936,693,030 logical bytes.)

| System | Retained bytes | ÷ LayerFS v1.0.0 | Verified | Source / date |
|---|--:|--:|---|---|
| LayerFS v1.0.0, stride 1 (157 states) | 100,085,760 (Store allocated) | 1.00× | 156/156 Commits + Init | dsh-dedup v1.0.0, `main` `e5c7aaae05`, 2026-10-19, 1 sample |
| Btrfs + zstd:3, one read-only snapshot per checkpoint | 1,994,596,352 | 19.93× | 157/157 | Btrfs runs 2026-09-09 |
| Btrfs + zstd:3 + bees (incl. 33.6 MB state) | 2,068,860,928 | 20.67× | 157/157 | same |
| Btrfs, no compression | 3,209,113,600 | 32.06× | 157/157 | same |
| Btrfs + zstd + duperemove (interrupted, 55/157 verified) | 2,434,428,928 (incl. 406.8 MB hashfile) | — not citable | 55/157 | same |
| LayerFS September research candidate (`0cdd2f3533`, producer `d4f26f0d16`) | 184,598,528 | (Btrfs zstd = 10.81×) | 157/157 | `LFS/docs/research/layerfs-vs-btrfs-157/REPORT.md` |

The LayerFS v1.0.0 Store breaks down as payload 67,488,833, metadata 5,507,608 and other 27,089,319 B. Logical per Store byte is 49.32×. Σ Commit time was 97.9 s, Init 155 ms.

Btrfs region breakdown (REPORT §2.2):

| Region | zstd baseline | bees |
|---|--:|--:|
| Data | 700.8 MB | 327.3 MB (−53.3%) |
| Metadata, incl. DUP copies and inline data | 1,293.8 MB | 1,708.0 MB |
| Filesystem Used | 1,994.6 MB | 2,035.3 MB |

Deduplication cut the data region but grew metadata by 414.2 MB.

**Caveats to state in the paper**
- The 19.9× pairs two separate campaigns, not one paired run. LayerFS v1.0.0 ran 2026-10-19 through the JS SDK; Btrfs ran 2026-09-09 in a 2-CPU/2-GiB privileged container on an 8 GiB loop device (`btrfs-progs` 6.2, metadata DUP).
- The Btrfs raw runs and `evidence.json` are lost; the report text was recovered from a session log ("restored 2026-10-20"). Its numbers are copied into `btrfs-reference.csv`.
- The importer rewrites changed files in full, which is unfavorable to Btrfs extent CoW. The report notes a 4 KiB change to a 512 MiB file cost Btrfs only about 36 KiB.
- dsh-dedup v1.0.0 has an open storage regression: +17.5% at stride 1 versus the 2026-10-07 in-process result of 85,172,224 B, above the old approved ceiling. The owner accepted it.
- **The current v2 build (`7691ac20`) has no stride-1 (157-state) result.** fs-storage-dsh ran only D10 and D03 (§3a). So 100.1 MB / 19.9× is a v1.0.0 number.
- Not equal durability or redundancy: Btrfs counts DUP metadata; LayerFS is the Disposable profile.

## 6. Correctness suites

All from the 2026-11-06 six-suite campaign at LayerFS `76a32a9a10e7b7b1379699ece52d45e233da5a50` ("pg-r2" frozen head, gate-05 sealed release artifacts).
- 9 configurations: docker/{sqlite, postgresql, pg-colocated}, native macOS/{sqlite, pg} (NFS mount), lima/{sqlite, pg}, apple-container/{sqlite, pg}.
- One sample per row. "Execution PASS" means the CLI lifecycle succeeded and the tool accepted its own result. Independent committed-content verification is SKIPPED in every row.
- Each report's own terminal label is "Diagnostic terminal: INCOMPLETE" because of final infrastructure cleanup, not because of test outcomes.

| Suite | Report | Cases | Rows | Outcome | Tool detail |
|---|---|---|--:|---|---|
| pjdfstest (unprivileged subset v2) | `LFB/result/2026-11-06/pjdfstest/76a32a9a…/pgr2-six/report.md` | P01 open/04.t (4 assertions), P02 rename/18.t (6), P03 chmod/02.t (5), P04 symlink/00.t (14) | 36 | 36 PASS | 29 assertions × 9 configs, 0 failed, 0 skipped. **A small subset; not full pjdfstest.** |
| fsx | `…/fsx/76a32a9a…/pgr2-six/report.md` | X01: 1,000 slots, seed 1, file ≤8 MiB, op ≤64 KiB | 9 | 9 PASS | 997 effective ops (539 reads, 237 writes, 221 truncates), 3 skipped slots, identical in every config |
| fsstress v2 | `…/fsstress/76a32a9a…/pgr2-six/report.md` | S01: 2 procs × 512 slots, seed 1 | 9 | **2 PASS (native sqlite/pg), 7 FAIL** | Native: 1,024 slots, 921 effective, 42 expected errno, 61 skipped, 0 unexpected. All Linux-guest rows (docker ×3, lima ×2, apple-container ×2): "exec: workload exit 1" |
| fsstress v2, fixed product | `…/fsstress/b9ad9edd7ea3a9314625dee53466aa59a7ea69c7/report.md` | S01 | 8 | **8/8 PASS**, cleanup COMPLETE | 1,024 slots, 0 unexpected errors. Native 921 ok / 42 expected / 61 skip; Linux 932 / 40 / 52 |
| fsstress v2, original | `…/fsstress/b919f50eaf9cce79671f8501a580279d23cc5f3c/report.md` | S01 | 8 | 2 PASS, 1 FAIL, 5 withdrawn | Docker/SQLite failed: six hardlinks to symlink inodes returned EPERM (the product allowed hardlinks to regular files only) |

**Reason for the 7 failures at 76a32a9a:**
- The report gives only "workload exit 1".
- Inference, not stated in the report: `76a32a9a` does not contain `b9ad9edd` ("Support Workspace hard links to symlink inodes", PR #580). I checked with `git merge-base --is-ancestor`: not an ancestor.
- The failure pattern matches b919f50e: Linux rows fail, while native passes under the declared Darwin dangling-symlink exception.
- For the paper: cite b9ad9edd 8/8 as the fix, and disclose that the pg-r2 build lacks it.

**mdtest** (`…/mdtest/76a32a9a…/pgr2-six/report.md`, 512 empty files, create/stat/remove):
- 4 PASS (native only), 14 NOT_RUN: Linux mdtest/Open MPI were missing from the offline guest image. A tooling gap, not a product failure.
- Native SQLite: 1,300 create/s, 293,412 stat/s, 1,827 remove/s.

**Other verification:**
- backup-verify `7691ac20`: PASS on MinIO, 51/51 checks.
- store-transfer `d8dddd50`: PASS on every route, 16 refusals and 45 interruption cases. 256/512 MiB Stores moved at 192–584 MiB/s with 22–50 MB caller RSS. Different host (Apple M2).

## 7. Per-operation lifecycle costs (CLI call wall time, ms, INELIGIBLE)

### 7a. Single-run reports named in the brief

| Report | LayerFS | Row | Create (Sandbox) | Mount | Exec | Commit | Unmount | Cleanup / close | End-to-end |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|
| juicefs-bench `LFB/result/2026-11-09/juicefs-bench/833d04cf…/report.md` | `833d04cf44e8e747929d50ab1170a48260d9381c` | docker/sqlite | 3,398.0 | 17.2 | 35,229.9 | 14.9 (UpToDate) | 186.8 | 61.7 | 38,927.3 |
| fs-bench-cloudflare `LFB/result/2026-11-08/fs-bench-cloudflare-computer/98ebfdfb…/report.md` | `98ebfdfb96fcb88a1c7275c4323e1a387349a219` | Docker SQLite | 5,345.0 (first Sandbox; includes bundle copy) | — | 6,384.9 | 21.2 (UpToDate) | — | 562.8 | 12,392.9 |
| same | same | Docker PostgreSQL | 273.4 | — | 6,036.4 | 89.6 | — | 137.9 | 6,729.1 |
| same | same | native SQLite (NFS) | 940.4 | — | 24,555.2 | 184.6 | — | 141.3 | 25,925.2 |
| same | same | native PostgreSQL | 57.1 | — | 23,711.7 | 145.4 | — | 84.4 | 24,094.4 |

- Every Cloudflare and juicefs Commit was UpToDate, because each tool deletes its files before the Commit. These Commit times measure a no-change Commit.
- Cloudflare's Lima and Apple Container rows were NOT_RUN at owner request (4 of 8 rows ran).
- Sandbox create on a fresh Docker machine at this build took 4.2–5.3 s, versus 0.3–0.4 s on `6d820c756b`. These were the first Sandbox on the machine; later creates were not measured.

Cloudflare scenario sums:

| Row | Sum of 12 scenarios (ms) |
|---|--:|
| Docker SQLite | 4,393.5 |
| Docker PostgreSQL | 4,071.3 |
| native SQLite | 20,670.4 |
| native PostgreSQL | 19,875.9 |
| host APFS baseline, no LayerFS | 14,757.5 |
| plain container, no LayerFS (diagnostic) | 2,132.7 |

LayerFS cost is about 2.3 s over the plain container on Docker. Native macOS is dominated by one process launch per file on an NFS mount.

### 7b. Aggregated over the six-suite campaign (my aggregation, more samples)

- Method: `scripts/lifecycle.py` parses every row table in pjdfstest, fsx, fsstress, fio, smallfile, mdtest and fs-bench-quick reports at `76a32a9a`: 315 rows, 294 PASS used. One sample per row; rows differ in workload, so Commit varies with data written (max 0.75–0.98 s on large-data cases).
- Times are CLI calls including process launch and routing.
- Docker Close (about 260 ms) includes Sandbox teardown.

| Machine | DB | PASS rows | Create | Mount | Commit | Unmount | Close | Store release |
|---|---|--:|--:|--:|--:|--:|--:|--:|
| docker | sqlite | 32 | 390.6 (285.4–903.4) | 18.8 (13.2–31.5) | 33.5 (11.3–771.7) | 16.9 (11.9–49.5) | 259.7 (192.4–511.0) | 18.4 (13.7–34.2) |
| docker | postgresql | 32 | 393.5 (264.7–1,022.9) | 28.1 (20.1–52.1) | 90.8 (13.1–963.2) | 16.4 (12.2–34.3) | 278.1 (208.3–563.0) | 8.5 (6.7–13.5) |
| docker | pg-colocated | 32 | 401.1 (269.7–827.0) | 22.5 (15.3–38.4) | 53.5 (11.0–906.8) | 16.9 (11.8–35.5) | 268.5 (202.6–549.4) | 8.7 (6.7–18.6) |
| native (macOS, NFS) | sqlite | 35 | 43.7 (26.3–648.5) | 37.7 (32.0–55.7) | 106.5 (62.1–903.0) | 31.0 (26.4–120.0) | 64.5 (51.3–101.1) | 11.6 (8.8–16.2) |
| native | postgresql | 35 | 46.5 (40.0–61.0) | 43.3 (36.5–66.7) | 135.2 (56.0–977.2) | 27.6 (23.4–143.4) | 60.8 (49.3–77.0) | 8.5 (6.4–35.2) |
| lima | sqlite | 32 | 694.1 (664.1–4,224.8) | 18.0 (13.4–26.7) | 29.8 (10.3–750.9) | 16.3 (11.5–51.0) | 95.9 (88.1–983.3) | 14.7 (10.4–44.5) |
| lima | postgresql | 32 | 699.7 (673.7–785.0) | 26.6 (19.1–39.5) | 76.0 (12.4–955.5) | 15.5 (11.9–45.0) | 93.6 (87.0–122.2) | 8.9 (6.4–36.1) |
| apple-container | sqlite | 32 | 60.3 (43.5–3,384.5) | 17.1 (13.6–39.8) | 47.4 (9.8–780.6) | 14.1 (9.7–44.0) | 37.5 (33.2–1,321.9) | 14.9 (10.6–26.0) |
| apple-container | postgresql | 32 | 67.9 (45.7–94.8) | 27.3 (19.2–50.4) | 91.7 (12.2–935.3) | 14.3 (9.5–42.8) | 36.9 (33.1–61.9) | 8.6 (6.5–12.7) |

Values are median (min–max), ms. pjdfstest P01 (tiny workload) on docker/sqlite, a clean single example: create 381, mount 19.2, exec 171, commit 16.0, unmount 16.8, close 273, release 18.7; end-to-end 897 ms.

**Buffered I/O tool numbers, same campaign, SQLite, one sample, own-write warm:**

fio (64 MiB sequential at 1 MiB blocks; 8 MiB random at 4 KiB blocks), MiB/s:

| Machine | Seq write | Seq read | Rand write | Rand read |
|---|--:|--:|--:|--:|
| docker | 383 | 1,143 | 53.7 | 18.8 |
| native | 413 | 2,000 | 42.1 | 444 |
| lima | 438 | 1,016 | 46.8 | 54.1 |
| apple-container | 438 | 1,391 | 44.4 | 58.4 |

smallfile (2 workers × 256 × 4 KiB), files/s:

| Machine | Create | Read | Stat | Rename | Delete |
|---|--:|--:|--:|--:|--:|
| docker | 5,539 | 7,320 | 11,579 | 3,460 | 6,374 |
| native | 1,863 | 6,102 | 14,945 | 1,786 | 3,799 |
| lima | 6,553 | 9,412 | 25,417 | 4,144 | 9,338 |
| apple-container | 5,763 | 8,167 | 17,427 | 4,294 | 7,654 |

## 8. JuiceFS bench: LayerFS vs JuiceFS/PG16

**Source:** `LFB/result/2026-11-09/juicefs-bench/833d04cf44e8e747929d50ab1170a48260d9381c/report.md`.
**LayerFS:** `833d04cf`, docker/SQLite Disposable.
**JuiceFS:** 1.4.2 (`46af7afa`) FUSE mount with PostgreSQL 16.15 metadata (fsync on, synchronous_commit on), local-file object storage, compression none, writeback disabled. Ran once later on the same shared VM, with identical binary, client image and command.
**Samples:** one each, at different times. **Label:** INELIGIBLE; correctness UNVERIFIED for both; admission_eligible=false.
**Workload:** `juicefs bench . -p 4 --block-size 1 --big-file-size 1024 --small-file-size 128 --small-file-count 100`, i.e. 4 workers, a 1 GiB file each, 100 × 128 KiB files each, no fsync, SKIP_DROP_CACHES.

| Phase | Unit | JuiceFS / PG16 | LayerFS / SQLite | LayerFS ÷ JuiceFS |
|---|---|--:|--:|--:|
| Write big file | MiB/s | 1,155.51 | 350.77 | 0.30× |
| Read big file | MiB/s | 1,486.61 | 386.27 | 0.26× |
| Write small file | files/s | 242.90 | 2,137.30 | 8.80× |
| Read small file | files/s | 4,182.10 | 15,089.50 | 3.61× |
| Stat file | files/s | 17,141.60 | 44,630.80 | 2.60× |

Caveats:
- Durability is not matched: JuiceFS/PG16 runs with fsync and synchronous_commit on, LayerFS runs Disposable. This is likely a large part of JuiceFS's small-file write cost.
- The tool reads and stats its own recent writes (warm).
- An earlier 25 s-budget LayerFS campaign (`jfs-20261109`) FAILED (exit -9 at 25,008 ms). It is retained; the owner authorized a 60 s budget for the reported run.

---

# Part B. Premise evidence: model time vs tool time, and tool-call mix

**Bottom line:**
- Locally recorded agent runs support "most wall time is in the model". Model time is 64–69% of active wall time in aggregate, with a per-session median of 84–85%.
- They support only a weaker form of "few tool calls need Bash".
  - The agents studied issue most calls *as* shell commands: dsh's main tool is `exec_command`, and Claude Code's is `Bash`.
  - But about 53% of those shell commands are plain file read/search (`cat`, `sed`, `rg`, `ls`, …) and 2–4% are file mutations (`mv`, `cp`, `rm`, `mkdir`, `apply_patch`).
  - Counting dedicated file tools too, about 62–63% of file+shell calls are expressible as filesystem operations. About 32–34% need process execution (build/test/run, infrastructure, git writes, other programs). Git reads are another 3–7%.
  - Per user prompt, the median is 2–3 shell calls, but the mean is 13–22 (heavy tail).

### B1. Data sources examined

| Candidate | What it is | Usable? |
|---|---|---|
| `~/.dsh/sessions/**/session*.jsonl.zstd` (981 files, about 370 MB compressed) | dsh (DeepSeek Harness) agent event logs with ms timestamps for step/start, assistant/message, tool/call, tool/result, step/end, plus run_code sub-call dispatch events | **Yes, analyzed.** 409 mock-model sessions (`layerfs-mock`) and 89 without a model call were excluded; 483 real-model sessions remain |
| `~/.claude/projects/**/*.jsonl` (1,612 files, 6.8 GB) | Claude Code transcripts on this machine, with ISO timestamps for assistant records, tool_use blocks and tool_result records | **Yes, analyzed** (not in the brief's list, but it is local agent run data) |
| `~/.dsh/sessions-plain`, `~/.dsh/dsh-runtimes` | runtime dependency manifests | No timing data |
| `~/Ephemeral-AI-Lab/deepseek-history-data` | the 157-checkpoint Git corpus and build logs | No agent timing (it is the storage corpus) |
| `~/Ephemeral-AI-Lab/dsh-layerfs-history` | plugin code repository | No run data found |
| LAYERFS_TRACE files under benchmark/result folders, `~/lfs-history` | — | None found (`~/lfs-history` absent) |
| `LFS/core/docs/issues/352/STATS.md` (#352 replay, ranges 0–60) | Replay of deepseek-harness **git history** as planned multi-agent steps (`SPEC.md`: "The numbers come from dsh's git history") | **Not premise evidence:** no LLM involved, and the call mix is set by the replay planner. See B5 |
| `~/.codex/sessions` (26 jsonl) | Codex logs | Not analyzed (small) |

**Privacy:** both scripts print only aggregate counts and timings. Shell commands are reduced to a bucket of their first program word. No prompts, file contents, paths or command text were emitted.

### B2. Method

**dsh** (`scripts/premise2.py`, output `scripts/premise2_out.json`):
- Within each agent step, model time = assistant/message.time − step/start.time (request + reasoning + generation).
- Tool time = union of [tool/call, tool/result] intervals in the step.
- The remainder is about 0.01%.
- Time between turns (waiting for the human) is excluded.
- Calls made inside the `run_code` code-mode tool (tool/ptc-dispatch events) are expanded into their real tool names; run_code itself is not counted as a call in the "effective" mix.

**Claude Code** (`scripts/ccode.py`, output `scripts/ccode_out.json`):
- Within each transcript, each gap between consecutive user/assistant records is attributed to what ends it: an assistant record means model time; a tool_result means tool time; a human prompt or meta record means idle (excluded).
- Any gap over 900 s is also counted as idle.
- Subagent transcripts are separate files; each is its own timeline, so concurrent agents add.

**Shell bucketing** (`scripts/shellcls.py`, first program word after `cd`/env prefixes):

| Bucket | First program word |
|---|---|
| read/search | cat sed head tail rg grep ls find wc awk jq diff echo … |
| git-read | git show/diff/log/status/… |
| edit/fs-mutate | apply_patch mv cp rm mkdir touch tee perl … |
| build/test/run | cargo npm node make python pytest go timeout … |
| ops/infra | docker gh curl limactl psql layerfs sleep kill ps … |
| git-write | other git subcommands |
| other | anything else |

Pipelines are bucketed by their first command only, so `rg x \| wc` counts as read/search.

### B3. Results

| Metric | dsh (483 sessions, 6 model families) | Claude Code (1,612 transcripts) |
|---|--:|--:|
| Scope | 2,274 user turns, 49,757 steps, 250.6 h inside steps | 14,641 prompts, 1,043 h active (2,401 h idle excluded) |
| **Model share of active wall time (aggregate)** | **64.3%** (tool 35.7%) | **68.9%** (tool 31.1%) |
| Model share, per session/file (p10 / median / p90) | 0.47 / **0.85** / 0.99 (n=360 sessions with >60 s) | 0.50 / **0.84** / 0.98 (n=1,579) |
| Model time per step (p10 / median / p90) | 3.4 / 7.5 / 22.6 s | — |
| Gap from tool result to next tool call (p10 / median / p90) | 0.001 / 6.5 / 20.0 s (p10 ≈ 0: parallel calls in one step) | 2.3 / 5.6 / 20.1 s |
| Shell call duration (median / p90) | 0.10 / 7.8 s | 0.57 / 11.2 s |
| File-tool call duration (median / p90) | 0.024 / 0.036 s | 0.027 / 3.9 s |
| Effective file+shell calls | 61,001 (shell 49,661; file tools 11,340) | 211,613 (Bash-family 189,249; file tools 22,364) |
| Shell: read/search | 26,044 (52.4% of shell) | 101,170 (53.5% of Bash) |
| Shell: git-read | 1,951 (3.9%) | 13,737 (7.3%) |
| Shell: edit/fs-mutate | 1,241 (2.5%) | 7,497 (4.0%) |
| Shell: build/test/run | 12,402 (25.0%) | 34,747 (18.4%) |
| Shell: ops/infra | 2,151 (4.3%) | 6,571 (3.5%) |
| Shell: git-write | 899 (1.8%) | 4,304 (2.3%) |
| Shell: other / unknown | 2,924 (5.9%) | 20,958 (11.1%) |
| Shell: stdin/job polling (write_stdin, job_*; BashOutput/Monitor) | 2,049 (4.1%) | (265 Monitor, in Bash-family total) |
| **File-expressible share of file+shell calls** (file tools + read/search + edit) | **63.3%** | **61.9%** |
| Git-read share | 3.2% | 6.5% |
| **Needs process execution** (build + ops + git-write + other + polling) | **33.5%** | **31.6%** |
| Shell calls per user turn/prompt (median / mean) | 3 / 21.8 | 2 / 12.9 |
| Tool calls per turn/prompt (median / mean) | 4 / 24.1 | 4 / 18.0 |
| Turns/prompts with tools but zero shell | 83 / 2,268 | 1,464 / 14,641 (3,360 had no tools) |

Per model family (dsh), model share of in-step time:

| Family | Sessions | Model share | Note |
|---|--:|--:|---|
| DeepSeek V4/V4.1 flash | 374 | 59.2% | per-session median 0.81 |
| GLM 5.3 | 62 | 83.0% | |
| OpenAI Codex models | 29 | 96.0% | |
| xAI/other | 18 | — | too small, 0.09 h |

### B4. Caveats on the premise numbers

- **Sample bias.** One developer's machine. Much of the work is LayerFS development and benchmarking, which is build-, Docker- and test-heavy and long-running, so it inflates tool time and the build/ops buckets. Claude Code transcripts include this research workflow itself.
- **Coarse bucketing.** "Needs execution" is approximate. `python3 -c` file edits count as build/test/run. `echo > file` counts as read/search. Interactive polling is counted as shell.
- **Interfaces bias the call mix.** Under dsh's Codex-style preset the model must use `exec_command` even to read files. Claude Code exposes Read/Edit/Grep, yet the model still chose Bash for 89% of file+shell calls. The premise should therefore say "most shell calls do file work that a filesystem API can serve", not "agents rarely call Bash".
- **Clock boundaries.** Model time includes queueing at the provider, network and rate limiting. dsh tool time includes harness dispatch. In Claude Code, permission prompts can land in tool or model gaps; gaps over 900 s are excluded as idle.
- **Concurrency.** Concurrent subagents are counted per agent timeline, not by wall-clock union.

### B5. The #352 replay is not premise evidence; what it shows

- Source: `LFS/core/docs/issues/352/STATS.md`, cumulative ranges 0–60.
- It replays git history as planned agent steps: 10,566 steps produce 167,777 SDK calls.
  - fs: 83,158 calls (49.6%), of which fs.write is 54,699.
  - sandbox: 65,120 calls, including exec 30,709, mount and unmount 11,464 each, commit 11,269.
  - branch: 18,976; project: 462; operator: 61.
- 71,392 Commits; 1,997,973,499 B new stored. "Time" is tooling wall time, never operation latency, and no model is involved.
- What it can support: LayerFS sustained a long multi-agent history in which about half the SDK calls went through the direct fs route (no Sandbox). The fs/sandbox ratio was set by the replay planner, not by agents.

### B6. What would need to be recorded for a cleaner premise measurement

1. Per tool call: start/end monotonic timestamps, tool name, an exit/result class, and **a structured intent class**. The class should come from the harness, not regex, e.g. pure read, file write/patch, search, build, test, run server, VCS read/write, network.
2. Per model call: request-sent, first-token and last-token times, plus provider queueing if exposed, to separate network/queue from generation.
3. Turn boundaries with human-wait time, and subagent parent/child links so wall-clock union can be computed.
4. Per Bash call: whether it touched only files under the workspace (from a FUSE/LayerFS trace, e.g. LAYERFS_TRACE with step ids), so "could have been a file API call" is measured, not inferred from the first word.
5. A task-level label (e.g. SWE-bench-style task id), so Bash calls per *task* can be reported, not per prompt.
6. Runs from more than one user and codebase, with dedicated file tools available, to remove the single-developer and tool-preset bias.

### Scripts (all in `notes/scripts/`)

| File | Role |
|---|---|
| `lifecycle.py` | §7b aggregation over the benchmark report tables |
| `premise2.py` | dsh analysis → `premise2_out.json` (`premise.py` is an earlier draft) |
| `ccode.py` | Claude Code analysis → `ccode_out.json` (about 7 s with 8 processes) |
| `shellcls.py` | shared shell-bucket classifier |
| `schema_probe.py`, `probe2.py`, `probe3.py` | structure-only schema probes (print keys and enum tags only) |

Run with `python3 -I <script>` (for `lifecycle.py`, pass the layerfs-benchmark path as its argument).
