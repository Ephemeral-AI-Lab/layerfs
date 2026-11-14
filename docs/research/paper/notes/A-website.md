# A. layerfs.ai website: claims, numbers, inconsistencies

Read on 2026-11-14 (site header shows v1.1.0). Method: fetched every page linked
from the site navigation, `llms.txt` and `llms-full.txt` (raw HTML converted to
text, so card/SVG text was captured too), plus the home-page demo script and the
GitHub v1.1.0 release notes linked from the header. Read-only; nothing signed in
or submitted. Quotes are under 15 words; everything else is paraphrase.

Labels used below:
- **ILLUS**: the site itself says illustrative (core-ideas cards: "Their numbers
  and timings illustrate each idea; they are not measurements").
- **MEAS**: a benchmark/run page with method, host, commit and receipts path.
- **MEAS?**: presented as measured but no receipt, method or page backs it.
- **SIM**: client-side random simulation (home-page demo), not labeled as such.
- **DOC**: a design/contract statement (behaviour, limit), not a measurement.

---

## 1. Page inventory

| URL | What it is | Paper value |
| --- | --- | --- |
| https://layerfs.ai/ | Hero ("Ephemeral Workspaces. Durable Shared History."), the 10 idea cards (6 core + 4 supporting), video `/video/layerfs-v1.0.0-release.mp4`, and an "Agent tool calls and their history" demo with a fake `get_storage` panel | Taglines, idea framing; demo is SIM |
| https://layerfs.ai/docs/ | Two install prompts (DSH recommended; SDK only), six doc sections | SDK-only prompt states the per-call protocol in 6 steps |
| https://layerfs.ai/docs/core-ideas/ | The 10 ideas with "How it works" text under each card | Core mechanism descriptions; chunking/delta parameters |
| https://layerfs.ai/docs/concepts/ | Project/Store, Branch, Commit/Layer, Sandbox/Workspace, Merge/conflicts, History; diagrams + replayable examples | Precise semantics (refs, publish overwrites, conflict table, .lww) |
| https://layerfs.ai/docs/sdk/ | Three packages, two routes (Store in-process vs Workspace via Docker), platform table, migration from v1.0.0 | Architecture split for the paper |
| https://layerfs.ai/docs/sdk/agent/ | `@layerfs/agent`: 32 calls in 6 groups, each with a `layerfs` CLI twin | API table |
| https://layerfs.ai/docs/sdk/integration/ | `@layerfs/runtime`, `@layerfs/backend` (incl. `get_storage`, `list_commit_storage`) | Observability API, worker route |
| https://layerfs.ai/docs/platform/ | Supported platforms, Durability, Concurrency model, Limits, What isn't saved | Limits table; durability disclaimer |
| https://layerfs.ai/docs/help/ | Error codes, FAQ, glossary | FAQ answers contradict core idea 2 (see §3) |
| https://layerfs.ai/docs/compare/ | vs git worktree, copying, overlayfs, Btrfs/ZFS, Cloudflare Computer; "when not to use" | Competitor claims with partial evidence |
| https://layerfs.ai/docs/integrations/ | DSH (preview), Pi (planned) | – |
| https://layerfs.ai/docs/integrations/dsh/ | dsh-layerfs plugin; **two real multi-agent runs (Wayfare, Quarry)** with commits/branches/storage numbers | Best end-to-end evidence on the site |
| https://layerfs.ai/architecture/ | "Coming soon." (empty) | none |
| https://layerfs.ai/blog/ | "Coming soon." (empty) | none |
| https://layerfs.ai/plugins/ | 7 listed plugins (1 official, 6 third-party) | Third-party adoption; one third-party timing claim |
| https://layerfs.ai/benchmarks/ | Summary of 9 suites + "How every suite is measured" rules | Headline numbers |
| https://layerfs.ai/benchmarks/fs-bench/ | vs Cloudflare Computer, 12 scenarios | MEAS |
| https://layerfs.ai/benchmarks/dsh-dedup/ | 157 snapshots of a real repo vs Btrfs and Git | MEAS; strongest storage evidence |
| https://layerfs.ai/benchmarks/fs-bench-pro/ | Legacy suite, v0.1.4–v0.1.6, retired reference impl.; v1.0.0 pending | Not usable for current-system claims |
| https://layerfs.ai/benchmarks/pjdfstest/ | POSIX conformance vs kernel overlayfs | MEAS; documents format limits |
| https://layerfs.ai/benchmarks/fsx/ | Data integrity, 3.06 M ops | MEAS |
| https://layerfs.ai/benchmarks/fsstress/ | Concurrency + Commit/remount capture proof | MEAS |
| https://layerfs.ai/benchmarks/smallfile/ | Small-file rates vs libfuse passthrough_ll and kernel overlayfs | MEAS |
| https://layerfs.ai/benchmarks/mdtest/ | Metadata rates, 1 and 4 MPI procs | MEAS |
| https://layerfs.ai/benchmarks/fio/ | Data I/O bandwidth/IOPS/latency | MEAS |
| https://layerfs.ai/llms.txt, /llms-full.txt | Machine-readable summary and full text | llms.txt still says v1.0.0 (see §3) |
| GitHub release v1.1.0 (header link) | Release notes | States no Linux host ran v1.1.0; no new perf claim |

No separate releases page or blog posts exist on the site itself; "Releases v1.1.0" links to GitHub.

---

## 2. Claims and numbers

### 2.1 Positioning and taglines (home, llms.txt)

| Claim / number | Wording (short) | URL | Label |
| --- | --- | --- | --- |
| Product | "the filesystem for AI coding agents"; MIT, open source | / , llms.txt | DOC |
| Hero | "Ephemeral Workspaces. Durable Shared History." | / | DOC (conflicts with durability disclaimer, §3) |
| Tagline | "Ephemerality: scaling multi-agent development beyond imagination" | / | marketing |
| Three pillars on hero | Ephemeral workspaces · Durable results · Massive parallelism | / | marketing |
| Per-agent lifecycle | each agent mounts a complete writable Workspace over shared history, works, commits, goes away | / | DOC |
| Status | llms.txt: "developer preview (v1.0.0)"; header: v1.1.0 | llms.txt, / | inconsistent |

### 2.2 The ten ideas (home cards + /docs/core-ideas/)

All card numbers are ILLUS per the core-ideas page preamble.

| # | Idea | Claims | Numbers | Label |
| --- | --- | --- | --- | --- |
| 1 | Ephemeral Computing | Compute exists only while a command runs; file calls run in-process against the Store, each write is one atomic op and one Commit; each Bash call mounts a fresh Workspace, runs, commits, unmounts; mount copies/installs nothing; LayerFS never auto-commits, times out or unmounts, so a Workspace can also last a whole task | "Agents spend ~80% thinking, ~10% in Bash." | ILLUS, no source given for 80/10 |
| 2 | Decentralized Agents | No server; a project's whole history is one SQLite file on a shared volume; each Sandbox daemon and the SDK open it directly; one writer at a time via short publish step (check head, merge, publish); reads never wait | Card: "Agents on different computers share one serverless Store" ; "Each agent runs on its own computer" | ILLUS; contradicted by the body text and FAQ (§3) |
| 3 | Multiagent Coordination | Many Branches per project, many agents per Branch; no locks; each tool call is a Commit that merges onto the head it meets; different files land, same file refused as Conflict; in a Workspace, conflict markers are written and the Workspace follows the new head; blame names the Commit (tool call) behind each line | – | DOC |
| 4 | Recursive Filesystem | A fork is one record and copies no files; fork from main, any Branch, Commit or Layer, at any depth; a losing fork costs only bytes it changed; "A filesystem for RSI agents and recursive exploration for RL training" | Card scores (0.62 … ★0.91) | ILLUS |
| 5 | Ultra Storage Optimization | CAS (object named by hash); files < 128 KiB kept whole, larger ones content-defined-chunked into ~8–32 KiB chunks; deltas against last version; zstd packing; `get_storage` reports dedupe = logical ÷ canonical and compression = canonical ÷ stored | "10 copies compress into 0.31"; "10 copies → 1.05 dedup → 0.31 compression wtf???" | ILLUS (card literally labeled "illustrative · get_storage") |
| 6 | Instant Bootstrap | Init captures source, .git, deps, caches, symlinks and every ignored file as main @ L0; mount binds whole tree, no scan/copy/install; only new file is the Workspace's overlay DB; content loads on demand (first read pays real I/O); installs are just committed changes | "≈5 min before the first run" (clone + install, typical); "mount 0.05 s"; "measured mounts: 5–20 ms"; "~5 min to install vs 0.05 s to mount" | ILLUS card; "measured mounts" is MEAS? with no receipt anywhere on the site |
| 7 | Auto Checkpoints | Every file call is a Commit; each Bash call commits when the orchestrator calls `workspace.commit`; no-change → UpToDate, no Commit; every Commit is a ref to read/diff/fork; files only, not processes/memory/sockets/output | – | DOC |
| 8 | Append-only Objects | Objects never rewritten; Commit id from tree, parents and message; sealed packs never updated or deleted; branches share objects; readers never wait for the writer; WAL + synchronous=OFF so a host crash can lose recent work; no GC yet, Store only grows | – | DOC |
| 9 | Conflict Rules (.lww) | Root `/.lww` gitignore-style patterns (`%include`); for matched paths changed on both sides, newer mtime wins (ours on tie); Workspace Commit reads the head's rules, `branch.merge(a,b)` reads a's; decisions recorded on the Commit; for lockfiles, build output, caches | card times 10:01/10:03 | DOC |
| 10 | Big File, Small Edit | Files are extent trees over chunks; overlay keeps only written bytes; Commit rebuilds only edited extents and their root path; cost ~ size of change + tree depth; no file size limit; mid-file insert/delete rewrites the tail (no POSIX insert) | Card: 2 GiB file, write 4 KiB + 1 KiB → "1 chunk + 3 tree nodes new" | ILLUS |

### 2.3 Concepts and API semantics (/docs/concepts/, /docs/sdk/*, /docs/platform/)

| Claim | Number / detail | URL | Label |
| --- | --- | --- | --- |
| "Seven words cover LayerFS" | project, Store, Commit, Branch, Layer, Workspace, Sandbox | /docs/concepts/ | DOC |
| Project names | 1–63 of a-z 0-9 `_ - .` | concepts, platform | DOC |
| Store contents | one SQLite file: chunks, trees, Commits, Layers, Branches; no second DB, nothing outside it | concepts | DOC |
| Store policy at Init | whole-file threshold 131072 B; delta depths 16 (whole file) / 8 (chunk) / 8 (metadata); `LAYERFS_STORE_POLICY` | concepts | DOC |
| Refs | "main", Branch name (head at call time), Commit id, Layer id; a call resolves each live ref once (one consistent snapshot per call) | concepts | DOC |
| main | read-only stack of Layers; changes only by `branch.publish`; cannot be mounted | concepts, sdk/agent | DOC |
| Publish semantics | publish **overwrites** main: skips Layers published since fork and lists them (`overwroteCount`, first 50); to keep them, merge main in first | concepts | DOC (important caveat for "many agents" story) |
| Commit message | ≤ 4096 B UTF-8; part of Commit identity; a message alone never makes a Commit | concepts | DOC |
| Sandbox | Docker container + one LayerFS daemon; bound to one project and one image; numbered 1,2,…; **up to 16 Workspaces**; closes itself 2 min after last unmount; `sandbox.create` is the only starter; no placement | concepts, platform | DOC |
| Workspace | `{sandbox, path}` at `/workspaces/N`; whole tree incl. ignored files; exec runs `bash -c`; any process seeing the mount works through ordinary file calls; several calls can share one Workspace | concepts | DOC |
| Unmount | refused as Dirty with uncommitted changes unless `discard = true`; deletes overlay; history stays | concepts | DOC |
| Merge rules table | one side changed → that side; `.lww` match → newer mtime; both text → line merge with markers; one non-text → ours unmarked; one side deleted → kept text in marker block | concepts | DOC |
| Conflict atomicity | a conflict never half-lands; nothing committed; nothing retried | concepts | DOC |
| Branch merge | `branch.merge(a,b,name,msg)` creates a new Branch; fast-forward if one side holds the other; else 3-way vs merge base, one Commit with two parents | concepts | DOC |
| History calls | list_commits, diff_commits, diff_branches, diff_files, blame, publish answer; pages 50 default, up to 1000 | concepts | DOC |
| Agent SDK size | "32 calls in six groups" (LayerFS 3, project 2, branch 6, fs 14, workspace 4, sandbox 3); every call also a `layerfs` CLI command printing JSON | /docs/sdk/agent/ | DOC (count verified) |
| `LayerFS.init` | default timeout 300 s (300000 ms); no unlimited option | sdk/agent, platform | DOC |
| `fs.read` window | 2000 lines and 48 KiB per call | sdk/agent, platform | DOC |
| exec output | returned whole at any size; no timeout of its own | platform | DOC |
| Concurrency | reads never wait; heavy work side by side, only check-head/merge/publish is serial; sandbox create/close take project lock, mount takes Sandbox lock, exec/status/commit/unmount take neither; lock waited up to **60 s** then Busy; one process can make up to **six** concurrent calls to one Sandbox | platform | DOC |
| Rationale for one writer | tool calls separated by model thinking time, so simultaneous Commits are rare | platform | DOC (design argument, no measurement) |
| Size caps | no cap on file, Workspace or Commit; disk/filesystem are the limits | platform | DOC |
| Durability | one profile, Disposable: WAL + synchronous=OFF; LayerFS process crash loses nothing reported committed; OS crash / power loss can lose recent history or damage the Store; durable profile exists but off | platform | DOC |
| Platform | every Store opener shares one Linux kernel; Store on local ext4/xfs/btrfs; else `Unsupported`; macOS/Windows via a Linux worker (container / WSL2) reached with `LAYERFS_WORKER_ENDPOINT` | platform, sdk | DOC |
| Store versioning | older Store format refused, not migrated | platform | DOC |
| Not saved | processes, memory, sockets; exec stdout/stderr; uncommitted changes; files outside the Workspace (/tmp, HOME); `fs.cp` gives new timestamps; daemon logs capped at two 10 MB files | platform | DOC |
| Format limits (from benchmarks) | only regular files, dirs, symlinks (no FIFO/socket/device); portable modes 0o777 files / 0o1777 dirs, no set-id; only mtime stored (atime/ctime reported as mtime), mount noatime; no xattrs, fallocate, ioctl, copy_file_range, locks, lseek (as listed on the fsx page), RENAME_EXCHANGE; no hard link to symlink | /benchmarks/pjdfstest/, /fsx/, /fsstress/ | DOC + MEAS |
| Kernel caching | writeback caching off; 60 s entry/attr timeout | /benchmarks/mdtest/, /fio/ | DOC |

### 2.4 Benchmarks (all MEAS unless noted)

Common setup stated on every v1.0.0 page: "core at main e5c7aaae05, through the JS SDK", arm64 Mac, Docker Desktop linux/arm64, 8 CPUs, 8.3 GB, one sample per case, Disposable Store, 1-min host load between ~6 and ~20. Rules page: one sample, setup cloned not measured, warm cache never credits a measured phase, speed beside storage, FAIL stays FAIL.

| Suite | Headline numbers | Caveats the page itself states | URL |
| --- | --- | --- | --- |
| fs-bench vs Cloudflare Computer | LayerFS 5.20 s vs 8.88 s summed over 12 scenarios, **1.71×**, faster in 10 of 12. Per row: create 1000 files 413 ms vs 1.40 s (3.38×); stat 1.07 s vs 1.68 s; rm 476 ms vs 1.24 s; mkdir tree 835 ms vs 1.41 s; find 960 ms vs 1.50 s; write 64 MiB 160 vs 127 ms (Computer 1.26× faster); copy 64 MiB 598 vs 352 ms (Computer 1.70× faster); read 64 MiB 178 vs 179 ms (tie); pure read 7.4 vs 78 ms (10.56×, warm); pure copy 209 vs 224; overwrite 111 vs 169 (warm); git init+commit 100 files 181 vs 535 ms (2.95×) | One sample; rows moved up to ~2× between runs; Computer privileged with larger max_read and auto_cache, LayerFS unprivileged with writeback off (not equalised); warm rows; setup/mount/Commit/unmount outside timers; Docker Desktop on Mac | /benchmarks/fs-bench/ |
| dsh-dedup (storage) | DeepSeek harness repo, 157 snapshots (every 100th of 15,632 commits), 4.94 GB of trees, 75,929 distinct files / 892 MB. LayerFS **100.1 MB** (49.3× smaller than trees); Btrfs+zstd 1,994.6 MB (**19.9×** LayerFS); Btrfs+zstd+bees 2,068.9 MB (20.7×, incl. 33.6 MB bees state); Btrfs uncompressed 3,209.1 MB (32.1×); Git pack+index **55.1 MB** (smaller than LayerFS). Strides: 10 → 53.1 MB (Git 41.8), 3 → 69.5 MB (Git 48.2), 1 → 100.1 MB. Payload/metadata/other at stride 1: 67.5 / 5.5 / 27.1 MB. Stride 1 keeps 9× the trees of stride 10 for 1.6× payload. bees merged 373 MB but sharing metadata grew 414 MB. ~2/3 of distinct files < 8 KiB. Delta chains capped 16/8/8 vs Git depth 50 | One sample; Btrfs numbers from a separate September run (not paired); both sides use "a changed file is rewritten whole" import rule; v1.0.0 Stores 5–18% larger than an earlier prototype (open item) | /benchmarks/dsh-dedup/ |
| dsh-dedup (Commit time) | Init 155–201 ms; Σ Commit 97.9 s for 156 Commits at stride 1; mean **628 ms**, slowest 2.8 s (thousands of changed files); stride 10 mean 1402 ms; stride 3 mean 867 ms; all 156 Commits forked, mounted, read back | Commit time is client wall time around `sandbox.commit`; Commit reads what the step just wrote (warm by own writes) | /benchmarks/dsh-dedup/ |
| pjdfstest | 238 files, 8,798 assertions; **0 unexpected** failures; keeps 2,296 of 3,003 reference passes (**76%**); 707 LayerFS-only failures = 275 direct + 432 knock-on; direct ones in 4 classes: FIFO/socket 214, set-id/sticky 37, atime 16, ctime 6; 0 hangs; 141 fail only on reference | Runs as uid 501 (suite expects root); knock-on label heuristic | /benchmarks/pjdfstest/ |
| fsx | 14/14 PASS (7 variants × 2 seeds), **3.06 M ops**, 0 miscompares, files 256 KiB–64 MiB, mmap and O_DIRECT; follow-up Commit 457 ms, 166.7 MB logical / 15.3 MB newly stored; fixes v0.1.6 issue #361 (writes > 1 MiB, mmap writes) | fallocate etc. off by design; warm caches; not native Linux | /benchmarks/fsx/ |
| fsstress | 4 × 10,000 concurrent ops; Commit → fork → mount → compare: **3,755/3,755 entries identical**; Commit **4.85 s** for 5.21 GB logical, **1.28 MB newly stored**; 42 of 61 op kinds alike vs kernel; 19 differ (xattrs, fallocate, RENAME_EXCHANGE, FIEMAP, link-to-symlink) | Arms do different work after refusals; one sample | /benchmarks/fsstress/ |
| smallfile | 4 threads × 25,000 × 4 KiB = 100k files; 9/9 PASS; LayerFS ÷ passthrough_ll: create 2.03×, read 3.02×, append 1.71×, stat 10.71×, chmod 3.38×, rename 1.75×, delete 1.27×, mkdir 4.21×, rmdir 3.27×; one Commit of 100k files **10.4 s** (104 µs/file, 410 MB). Kernel overlayfs far ahead (e.g. create 204,225 vs 6,693 files/s) | passthrough_ll uses a mutex-guarded inode list that slows with count; 1.0 s cache timeout vs LayerFS 60 s; passthrough_hp not measured; LayerFS read ran after Commit (committed base); cache warm/unknown | /benchmarks/smallfile/ |
| mdtest | 9/9 exit 0 (baseline finished 7/9, 2 hit 115 s ceiling); 1 proc create/rename/remove **0.86–1.28×** of baseline (trails in 4/10 phases by ≤14%); 4 procs **1.21–4.22×**; stat/open 2.94–784× attributed to cache timeout setting, "not daemon speed"; Commit of 20k files 2.6 s (130 µs/file); vs kernel: write phases 0.03–0.30×, stat ≥ kernel in 8/10 | Same baseline caveats; empty files | /benchmarks/mdtest/ |
| fio | 9/9 PASS (baseline failed rand-write-direct EINVAL); vs baseline: seq-write **0.31×** (514 vs 1,669 MB/s), seq-read 0.81×, rand-write 0.40×, randrw 70/30 2.23×, rand-read-direct 1.49×; sync-write 6.91× shown but not counted (Disposable fsync returns immediately); Commit 24.8 s, 1.68 GB logical / 1.15 GB newly stored | Streaming writes trail; recording cost not separated; one 20 s sample | /benchmarks/fio/ |
| fs-bench-pro | Totals over v0.1.4–v0.1.6: 20 families, 432 case results, 472/490 proofs; v0.1.6: 36 cases, 25 PASS / 3 EXCEPTION / 8 N/A, 36/36 proofs; v0.1.7 none; v1.0.0 pending (#332) | Measured on the **retired reference implementation**; suite frozen and broken; not comparable across releases | /benchmarks/fs-bench-pro/ |

### 2.5 End-to-end multi-agent runs (/docs/integrations/dsh/)

MEAS-style (date, model, counts, "Full record" link) but no host/commit identity on the page.

| Run | Date / setup | Time | History | Checks | Storage |
| --- | --- | --- | --- | --- | --- |
| Wayfare (trip planner, Node 24, no deps) | 2026-10-20; 1 main agent + 7 worker subagents on deepseek-flash; empty project → published, verified main | **56 min, 8 agents** | 147 Commits, 31 Branches, 13 Layers | 334/334 tests, A01–A12 | 1.21 MB on disk for 91.5 MB of snapshots, "27× dedupe" |
| Quarry (SQL DB in Rust, every build's `target/` committed) | same | **4 h 16 min, 8 agents** | 913 Commits, 56 Branches, 9 Layers | 891 tests, 800/800 sqllogictest, Q01–Q12 | **8.90 GB on disk for 4.93 TB of snapshots**, "166× dedupe", less than one copy of main (13.5 GB) |

Also: screenshot caption "110 Commits on a dozen Branches, main at L10" (History tab). dsh scenario suite: 10 scenarios (5 SDK, 5 CLI), run takes ~15–20 min. The dsh preset gives agents bash only (no read/write/edit tools), "one Workspace per agent, only the main agent publishes".

### 2.6 Plugins (/plugins/)

7 listings: LayerFS × DSH (official, v0.1.0), Codex parallel verify, Cursor, Herdr, Magpie, Pi, Mainline (third-party, preview). Third-party claims: Herdr "a new one in ~0.2 s, restored after restart"; Magpie "forks copy 0 bytes, checkpoints store only changes". No evidence linked on the page.

### 2.7 API calls named on the site

- `@layerfs/agent`: `LayerFS.open/init/close`; `project.list_branches/list_sandboxes`; `branch.fork/list_commits/diff_commits/diff_branches/merge/publish`; `fs.read/list/stat/grep/glob/diff_files/blame/write/edit/apply/mkdir/rm/cp/mv`; `workspace.mount/exec/commit/unmount`; `sandbox.create/list_workspaces/close`.
- `@layerfs/backend`: `store.project.get_history/get_storage/get_comparable/list_overwritten`, `store.branch.list_commit_storage`, `store.fs.get_storage/list_revisions`, `daemon.workspace.status/list_changes/get_storage`, `daemon.process.start/list/...`, `daemon.sandbox.get_log`.
- `@layerfs/runtime`: `status`, `project.list/start_init/get_init/get_state/remove`, `worker.start/restart`, `sandbox.reclaim/list_leftovers/clean_leftovers/list/sweep`, `volume.list/remove`, `events.subscribe`.
- Benchmark pages describe runs with **v1.0.0 names** (`project.init`, `sandbox.mount`, `sandbox.exec`, `sandbox.commit`), which no longer exist in the documented v1.1.0 SDK.

---

## 3. Inconsistencies and errors

Ordered by how much they matter for a paper.

1. **"Agents on different computers" vs "one Linux kernel".** Core idea 2's card says agents on different computers share one serverless Store and each agent runs on its own computer. The same page's body, /docs/platform/, /docs/compare/ ("Agents on different machines sharing one Store. Not today") and the FAQ ("Can agents on different machines share one Store? Not today") all say every opener must share one Linux kernel. The "decentralized/serverless across machines" claim is not true of the current system; "serverless" holds only in the sense of "no DB server, one SQLite file".
2. **"Durable Shared History" / "Durable results" vs no durability.** Hero and pillar text promise durability; /docs/platform/ says durability is not claimed (WAL, synchronous=OFF; OS crash or power loss can lose recent history or damage the Store). Core idea 8 also says "Immutable isn't durable". Paper should say "persistent, immutable history", not "durable".
3. **Mount time numbers are unsupported and self-inconsistent.** Card: "mount 0.05 s" (50 ms) next to "measured mounts: 5–20 ms"; 50 ms is outside 5–20 ms. No benchmark page measures mount latency; every benchmark page puts mount outside its timers. "~5 min" install time is labeled "typical" with no source. The headline bootstrap claim of the paper therefore has no published measurement on the site.
4. **Core idea 5 card text "wtf???"** literally on the home page, core-ideas page and llms-full.txt. The card's own animation is also inconsistent: commits show 12–15 chunks with "11 matched + 1 new" (12 ≠ 13/14/15), and one new chunk per commit across 10 copies would give roughly 1.7 copies stored, not ≈1.05. The 0.31 total (≈3.4× compression) also understates every measured result (dsh-dedup 49×, Wayfare 76× on-disk vs snapshots, Quarry ~550×).
5. **80/10 compute split.** Card says "~80% thinking, ~10% in Bash" (remaining 10% unexplained; owner's figure is 80/20). No source, trace or measurement on the site.
6. **Benchmarks labeled "LayerFS v1.0.0" were run at main `e5c7aaae05`.** Locally, `e5c7aaae05` is an ancestor of tag v1.0.0 (`2aaf2552ce`) by 202 commits, including Rust changes in core/engine (~131 files, +5.2k/−4.8k lines, e.g. #373 numbered Sandboxes, idle close). The pages state the commit honestly, but "v1.0.0" is a label, not the tagged build; the v1.1.0 release notes say the results "were measured on 1.0.0 source". The site header advertises v1.1.0, which has no measurements of its own.
7. **All performance numbers are from Docker Desktop on an arm64 Mac**, at 1-min load 6–20 on 8 CPUs; fsx/fs-bench pages say "Not a native Linux qualification". Meanwhile the v1.1.0 release notes say no Linux host ran v1.1.0 (Linux FUSE mount tests and real-Store JS tests waived), although Linux is the only native platform.
8. **Version labels disagree.** llms.txt: "developer preview (v1.0.0)"; header: v1.1.0; dsh page badge "LayerFS JS SDK 1.0.0" while its install prompt pins v1.1.0; benchmark pages "v1.0.0"; home video `layerfs-v1.0.0-release.mp4`.
9. **Benchmark pages use the retired v1.0.0 API names** (`sandbox.mount/exec/commit`, `project.init`, `@layerfs/sdk`), while the docs describe `workspace.*`, `LayerFS.init`, `@layerfs/agent`.
10. **Data paths point to a moved directory.** Benchmark pages cite `benchmark/data/...` "in the LayerFS repository" and fs-bench-pro says the suite "now lives in benchmark/fs-bench-pro"; per the repository, benchmarks moved to the separate layerfs-benchmark repo (v1.1.0 release notes say so too).
11. **fs-bench "faster in 10 of 12"** counts read 64 MiB, which the same page calls "a tie, not a win" (178 vs 179 ms); pure copy 1.07× is well within the stated ~2× run-to-run movement. Honest count is 9 wins, 1 tie, 2 losses, with several within noise.
12. **dsh-dedup vs Btrfs is not paired**: Btrfs numbers come from a September run, and the shared import rule rewrites changed files whole, which removes Btrfs reflink/extent sharing by construction. The compare page repeats "19.9 times the space" without these caveats. ZFS is named in the compare page heading but never measured.
13. **"Every tool call is a commit"** (core idea 3 card) vs reads make no Commit, no-change calls answer UpToDate, and a Bash call commits only when the orchestrator calls `workspace.commit` (core idea 7 body, concepts).
14. **Sandbox per agent vs Workspaces per Sandbox.** Core idea 2 body says "One Sandbox per agent"; concepts say one Sandbox holds up to 16 Workspaces of any Branches and the dsh preset rule is "one Workspace per agent".
15. **pjdfstest "3 deliberate format limits"** in the verdict vs "in four places" in the section heading (atime and ctime are one limit, "One stored time"). Minor wording.
16. **fio intro** says the job set has "an fsync after every write"; only the `sync-write` job does (`seq-write` uses end_fsync). The "fsync returns at once (p50 under 1 µs)" claim is not shown in the latency table (sync-write p50 62 µs is write completion latency).
17. **mdtest sentence is garbled**: "with a directory each, LayerFS does (11,597 … against 4,143 for one)": the verb is missing (probably "scales").
18. **dsh page vs its own install prompt**: step-by-step uses `DSH_HOME=$HOME/dsh-layerfs` and port 3090; the prompt says no DSH_HOME override and port 3080.
19. **Integrations page says Pi is "planned"**, while /plugins/ lists "LayerFS for Pi" (preview, v0.1.0, third-party).
20. **"Dedupe" in the dsh runs is ambiguous**: Wayfare 91.5 MB / 1.21 MB = 75.6× on-disk reduction but the page says 27× dedupe; Quarry 4.93 TB / 8.90 GB ≈ 554× but says 166×. Consistent only if "dedupe" means logical ÷ canonical (before compression), which the page does not say.
21. **Home "Try it live" demo is a random simulation.** The bundled script (`OrchestrationDemo.*.js`) generates agents, commands and storage with `Math.random` and adds a fixed 48 MB of "logical" per publish; its `get_storage` panel (dedupe 1.5×, compression 3.32×, stored 9.4 MB, logical 48.0 MB) carries no "illustrative" label, unlike the core-idea 5 card.
22. **Architecture and Blog pages are empty** ("Coming soon."), though linked from every page header; llms.txt describes Architecture as how Store, Workspaces, FUSE and daemon fit together.

---

## 4. Competitor claims and evidence

| Competitor | Site claim | Evidence on site |
| --- | --- | --- |
| git worktree | Tracked files only; each new worktree reinstalls/rebuilds; history only what you commit; merging by hand | Argument only. Comparison card "~5 min vs 0.05 s" is ILLUS with no measurement |
| "Git / other AgentFS" | "Drops ignored files; reinstalls" (core idea 6 card) | None. No AgentFS product is named or cited. Unclear which systems this covers; risky to repeat in a paper |
| Copying the project (cp -r, rsync, fresh volume) | Reads/writes every file before start; space grows per agent | Argument only |
| overlayfs | Cheap writable layer but no history, branches or merge | Argument only for the feature claim; kernel overlayfs is used as the speed ceiling in 5 suites (LayerFS far slower on writes, as expected for FUSE) |
| Btrfs (and ZFS) snapshots | Blocks shared only while they stay put; full rewrites are new data; cannot merge | dsh-dedup: 19.9× more space (Btrfs+zstd), with the unpaired-run and rewrite-whole caveats; ZFS not measured |
| Git pack | Smaller archive (55.1 vs 100.1 MB) but offline, deep chains (50), not a filesystem | dsh-dedup, MEAS; the site concedes Git wins on size |
| libfuse passthrough_ll | Like-for-like FUSE baseline; LayerFS faster in 9/9 smallfile ops, mixed in fio | MEAS, with stated baseline weaknesses (mutex inode list, 1 s cache timeout, passthrough_hp not run) |
| Cloudflare Computer | 1.71× less total time on Computer's own 12-scenario script | MEAS, one sample, not equalised (privileged/auto_cache vs writeback-off sandbox), mount/commit untimed |
| Sandboxes (E2B, Modal, Daytona, Firecracker, etc.) | Not named anywhere. Only implicit: "a sandbox held for the whole task sits idle" (core idea 1) | None |
| "ovnfs" | Not mentioned anywhere on the site (likely means overlayfs) | – |

---

## 5. Suggested paper wording

Safe, supported by the site:
- "LayerFS keeps a project's entire committed state in one SQLite file that every process opens directly; there is no database server. Today all openers must share one Linux kernel and a local ext4, xfs or btrfs filesystem."
- "File tool calls run in the agent's own process against the Store, each write one atomic Commit; each Bash call mounts a Workspace over a Branch head, runs, commits and unmounts. A mount binds the committed tree without scanning, copying or installing; its only new file is a per-Workspace overlay database."
- "Init captures the whole tree, including `.git`, dependencies, caches and every ignored file, so a mounted Workspace is ready to run without reinstalling."
- "A fork is a single record: it copies no files, and a fork of a fork costs the same as the first."
- "Storage combines content addressing, content-defined chunking (files ≥128 KiB, chunks of about 8–32 KiB), bounded delta chains (16/8/8) and zstd packing. On 157 snapshots of a real repository (4.94 GB of trees) the Store held every snapshot as a mountable Commit in 100.1 MB, 49× smaller than the trees; Btrfs with zstd used 1,994.6 MB on the same trees under the same rewrite-whole import rule, and a Git pack 55.1 MB."
- "In two end-to-end runs with eight agents on one project, the Store held 913 Commits on 56 Branches (4.93 TB of logical snapshots) in 8.90 GB, and 147 Commits on 31 Branches in 1.21 MB."
- "Commits on one Branch from many agents merge three ways onto the head they meet; a same-file clash is refused atomically as Conflict; publication to main is serialised through one short lane while reads never wait."
- "Correctness: 3.06 M fsx operations with 0 miscompares; an fsstress Commit of 5.21 GB re-mounted identical in all 3,755 entries; no unexpected pjdfstest failures beyond three deliberate portable-format limits."

Avoid or qualify:
- "Agents on different computers" / "decentralized across machines" (not supported today).
- "Durable" history (say persistent, immutable, crash-unsafe under the Disposable profile).
- "5–20 ms mounts" or "0.05 s vs 5 min" until a mount-latency measurement with receipts exists; the site has none.
- "80% thinking, 10% Bash" without a cited trace study; decide 80/10 vs 80/20 and source it.
- "v1.0.0 results": say "measured at commit e5c7aaae05 (pre-v1.0.0 main) on Docker Desktop, arm64 macOS, single sample under load 6–20".
- "1.71× faster than Cloudflare Computer": add one sample, configurations not equalised, mount/commit untimed, one tie counted as a win.
- "Other AgentFS drop ignored files": name and cite systems or drop.
- The "10 copies → 0.31" card ratio: illustrative and internally inconsistent; use measured ratios instead.
