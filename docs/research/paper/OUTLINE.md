# Paper outline: LayerFS

> **Status:** Research; informative and not a product contract.

Drafted 2026-11-14 from the published layerfs-benchmark reports and the
research notes in this directory. It is a plan for a paper, not a claim that
any result below is qualified. Each evidence row is marked:

- **HAVE**: a published report supports it as stated.
- **PARTIAL**: data exists but has a known weakness (sample count, platform,
  baseline, lost raw evidence).
- **NEED**: not measured yet.

## 1. Positioning

**Counterparts:** mounted agent and versioned filesystems: ovnfs, AgentFS,
overlayfs / Docker image layers, btrfs (and ZFS) snapshots, JuiceFS. Git is
not a counterpart. It is not a live filesystem and is too slow on that path.
It appears only as an offline storage reference point.

**Working title options**

1. *LayerFS: Backup-Grade Deduplication at Tool-Call Latency for Agent Workspaces*
2. *Versioned Workspaces for AI Agents*
3. *Every Tool Call a Commit: A Versioned Filesystem for Coding Agents*

**Thesis (one sentence).** A mounted filesystem can make every agent tool
call a durable, branchable Commit, by doing content-defined chunking, exact
dedup and delta compression inside the Commit path. Commits take tens of
milliseconds, and the history stays within a small factor of an offline-packed
store. Existing agent filesystems store each changed file whole.

**Novelty is the placement, not the codecs.** CDC, content addressing and
delta encoding are known techniques (FastCDC, super-feature resemblance,
zstd prefix deltas). Backup systems and VCSs run them offline over complete
snapshots. LayerFS runs them online, per Commit, over a live overlay. The
paper has to say this plainly. Reviewers will check it.

## 2. Contributions (draft)

1. **Problem and workload.** A characterization of how coding agents mutate a
   filesystem: tool-call granularity, small local edits, saves that write a
   new file and rename it over the old one, branches and forks, and the
   resulting history shapes. *(PARTIAL: replayed real history (DSH, 157
   checkpoints) plus a synthetic edit model; no captured agent syscall trace.)*
2. **Design.** A Workspace model where mount cost doesn't depend on tree
   size (it copies one template overlay database). A Commit captures every
   change published to the Workspace, from any process, without tracking
   which command made it. Its cost follows the size of the change, not the
   tree.
3. **Online storage pipeline.** Canonical objects with domain-separated
   BLAKE3 ids, and GEAR CDC above a 128 KiB cutoff. Exact reuse trusts the
   id (role and length checked) and authenticates every object on read. A delta is kept only when its record is strictly
   smaller than the standalone one, with one trial per object, and each
   delta chain has a work budget. Rename predecessors give saved-by-rename
   files a delta base. Localized edits don't rechunk the unchanged bytes.
4. **Store architecture.** One global database with a single writer and
   short holds. Heavy work happens before the publication lane, and only the
   atomic publish step holds it. Reads never take it.
5. **Evaluation** against ovnfs, AgentFS, overlayfs/Docker, btrfs and JuiceFS
   on storage, Commit latency, edit latency, reads and correctness, with
   ablations.

## 3. Section plan

### 3.1 Introduction (1 page)

- Agents run hundreds of tool calls per task. Users want to undo, branch,
  compare and resume at any of them.
- Today's options: VM/container snapshots (coarse and expensive), overlay
  filesystems (store whole files per version), Git (not a live filesystem,
  and it has to scan the tree).
- Headline numbers (fill from the final campaign; current ovnfs values in
  §5.1): storage per Commit, Commit latency, large-file edits.
- Contributions list from §2.

### 3.2 Background and motivation (1–1.5 pages)

- How agents touch files. Rename-over saves come from git, editors and
  package managers. Most edits are small. Workspaces can be per call or per
  task.
- Why existing designs fall short:
  - Copy-up overlays copy the whole file on its first write.
  - Snapshot filesystems share extents only for in-place updates. Btrfs
    lost on a small-file rewrite history (§5.4).
  - AgentFS (0.6.4, studied 2026-09-01) stores fixed 4 KiB chunk rows in
    SQLite and has no built-in history or snapshots.
- **Motivating figure:** bytes added per Commit vs edit size, ovnfs vs
  LayerFS (from the cold rerun).

### 3.3 Design (3–4 pages)

1. **Model:** Project, Branch, Commit, Workspace, Sandbox. Execution is
   separate from filesystem semantics: the daemon supervises no commands.
2. **Workspace overlay:** one SQLite database per mounted Workspace, copied
   from a template at mount. Open-file state lives in daemon memory. One
   lock per Workspace, held for one short atomic job. Terminal unmount
   deletes the database.
3. **Commit:** capture the frontier, build canonical objects, save, then
   publish in the lane (head check, merge, publish as one step). Show why
   the cost follows the change.
4. **Canonical objects:** the envelope and identity. Same bytes don't always
   give the same root, so comparison falls back to `same_file_bytes`. Discuss
   this tradeoff openly; Git doesn't have it.
5. **Chunking:** GEAR 8/16/32 KiB with normalization. Localized edits keep
   untouched extents. All-zero subtrees are reused without scanning their
   length.
6. **Delta selection:** candidates come from the advisory predecessor, the
   rename predecessor, or the similarity index (8 folded hashes, 704 KiB).
   One trial, kept only if strictly smaller. Depth caps of 16/8/8 and
   cutoff-scaled chain budgets (4T canonical, 2T encoded).
7. **Inode metadata:** value pooling plus a COPY/INSERT delta grammar.
8. **Store:** packs stored as database rows, a single writer, short holds.
   SQLite and PostgreSQL backends.

Figures: system architecture; the Commit pipeline (adapt the handbook's
flow); one object stored three ways (FULL, STORED, PREFIX).

### 3.4 Implementation (0.5 page)

Rust: core, engine and API crates, with production LOC from
`tools/production_loc.py`. FUSE on Linux (fuser 0.18 with authorized
patches), NFS on macOS. Node SDK.

### 3.5 Evaluation (4–5 pages)

The questions it should answer:

- **Q1.** How much storage does history cost, compared with agent
  filesystems?
- **Q2.** How fast are edits and Commits?
- **Q3.** What does LayerFS give up: reads, import, new data?
- **Q4.** How much does each mechanism contribute (ablations)?
- **Q5.** Does it stay correct (POSIX suites, stress, history verification)?
- **Q6.** How does it scale: tree size, history length, branch count,
  concurrent Workspaces?

### 3.6 Related work (1 page)

- **Dedup and chunking:** LBFS, Venti, FastCDC, Data Domain, resemblance
  detection (Shilane et al., Finesse), restic/borg/casync.
- **Versioned filesystems:** Elephant, ext3cow, WAFL/ZFS/btrfs snapshots,
  NILFS.
- **Union filesystems and container layers:** overlayfs, AUFS, Docker,
  lazy image pulling (Slacker, Nydus, stargz).
- **Database-backed filesystems:** Inversion, TableFS, JuiceFS, AgentFS.
- **Agent sandboxes:** E2B, Modal, Daytona, Cloudflare Computer, ovnfs,
  Morph/Fly-style VM snapshots.
- **VCS:** Git, Mercurial, Pijul.

### 3.7 Discussion and limits (0.5 page)

- Read throughput (if still behind ovnfs).
- No GC protocol yet (spec in PR #681).
- Linux-only direct Store; the macOS and Windows worker routes.
- Durable profile disabled; Disposable/WAL/OFF only.

## 4. Evidence map

| Claim | Status | Source |
| --- | --- | --- |
| Per-Commit storage is 6–22× below ovnfs on branch histories | HAVE (3 samples, cold) | layerfs-benchmark `comparision/ovnfs/rerun-2026-11-08-cold/report.md` |
| Large-file small edits keep 5–17% of raw, against about 100% for ovnfs | HAVE | same |
| Commit 1.5–2.2× faster (histories) and 7.4–21× faster (large files) than ovnfs | HAVE (diagnostic; the routes differ) | same |
| ovnfs reads 1.3–1.5× (tree) and about 6× (500 MiB file) faster | HAVE; a weakness | same |
| Overall reduction breakdown: dedup × delta+zstd ÷ Store overhead | HAVE | `fs-storage-dsh`, `-micro`, `-history` reports (7691ac20) |
| Real history, 157 states: raw ÷ stored 11.7× (D10), 27.5× (D03) | HAVE | `fs-storage-dsh` |
| Within 1.15–1.27× of a Git pack (offline reference) | HAVE | `fs-storage-dsh` |
| Branches cost metadata only (H03: 50 identical branches add about 270 KB) | HAVE | `fs-storage-history` |
| Projects share content within one Store (H05 28.3 MB vs H06 471.4 MB) | HAVE | `fs-storage-history` |
| Btrfs 10.8–11.2× larger on the 157-checkpoint rewrite history | PARTIAL: September candidate, raw runs lost | `docs/research/layerfs-vs-btrfs-157/REPORT.md` |
| Mount about 17 ms, Commit about 15 ms | PARTIAL: one sample, INELIGIBLE | `juicefs-bench` 2026-11-09 |
| POSIX correctness: pjdfstest 36/36, fsx 9/9 | HAVE | 2026-11-06 reports |
| Stress correctness | **FAIL**: fsstress 2 PASS, 7 FAIL (`76a32a9a`) | `fsstress` 2026-11-06 |
| Mount cost independent of tree size | NEED: sweep over tree size | — |
| Commit cost follows the change, not the tree | NEED: sweep with a fixed change over growing trees | — |
| Mechanism ablations | NEED | — |
| AgentFS, overlayfs/Docker, btrfs (current), JuiceFS storage and speed on the same workloads | NEED | — |
| Agent workload characterization from captured traces | NEED | — |
| Concurrent-Workspace scaling | NEED | — |

## 5. Evaluation plan

### 5.1 Baselines and configuration

| System | Configuration | Why |
| --- | --- | --- |
| LayerFS | SQLite Disposable; PostgreSQL as a second row | Subject |
| ovnfs | On ext4 **and** on btrfs/XFS (reflink copy-up and clones) | Closest agent filesystem; ext4 alone invites a fairness complaint |
| AgentFS | Its default SQLite database, per-session delta | SQLite-backed agent filesystem; no history, so compare edit speed and bytes per version where it can hold them |
| overlayfs + `docker commit` | One layer per Commit | What container sandboxes use today |
| btrfs + zstd snapshots | One snapshot per Commit | Snapshot filesystem; strong on in-place large-file edits |
| ZFS snapshots | Optional | Second snapshot filesystem |
| JuiceFS (PG metadata, local object store) | One clone per Commit, if it supports that | Database-backed filesystem |
| Git | `git add -A && git commit`, then `git gc` | Offline storage reference only |

Platform: native Linux on dedicated hardware (not Docker Desktop on macOS).
Use the same route into every system (a direct shell inside the machine) so
CLI or `docker exec` overhead doesn't differ between arms. Report it
separately.

### 5.2 Workloads

| Id | Workload | Exists? |
| --- | --- | --- |
| W1 | DSH real history, 157 states (D10, D03, D01) | Yes: `fs-storage-dsh` |
| W2 | Edit-model histories S1–S4 and H01–H07 | Yes: `fs-storage-history`, ovnfs harness |
| W3 | Large files: L10, L100, L500, I10, A5, NEW500, ADD500, RM500, DEL500 | Yes: ovnfs large-file harness |
| W4 | Single-mechanism micro cases M01–M08 | Yes: `fs-storage-micro` |
| W5 | Agent session traces: real agent runs recorded at syscall level, then replayed | **New** |
| W6 | Package-manager churn: `npm install`, `pip install`, a build with `node_modules` and `target/` included | **New**; tests the "Git ignore rules don't filter LayerFS state" claim |
| W7 | Scale sweep: trees of 10³–10⁶ files; history of 10–10⁴ Commits; 1–100 branches | **New** |
| W8 | N concurrent Workspaces on one Store, each committing per call | **New** |

### 5.3 Metrics

- **Storage:** allocated bytes at quiesced points, bytes added per Commit,
  and the breakdown (exact dedup, delta+zstd, Store overhead).
- **Speed:** mount, edit exec, Commit, cold full-tree read, cold large-file
  read, random 4 KiB reads at depth in a delta chain, import.
- **Cost:** RSS of the daemon and Store process, CPU seconds per Commit.
- **Correctness:** pjdfstest, fsx, fsstress, xfstests generic subset if
  feasible, and oracle verification of every Branch head.

### 5.4 Ablations (LayerFS only)

| Ablation | Switch | Expected effect |
| --- | --- | --- |
| No CDC | Whole-file objects at any size | Large-file edits store full copies |
| No delta | Depth caps 0 | Small-edit Commits grow to the size of changed objects |
| No similarity index | Disable index proposals | M05 new-path edits lose delta bases |
| No rename predecessor | Disable `fs_predecessor` | M04 rename-over edits lose delta bases |
| No inode metadata delta | Metadata depth 0 | Metadata dominates small Commits (about ⅔ today) |
| Cutoff sweep | 128 KiB … 1 MiB | Tradeoff between storage and read amplification |
| Depth sweep | 0, 4, 8, 16, 32 | Storage vs read latency at the chain tip |

Each needs a policy switch that exists only for the experiment. Check
whether Store policy already allows it (depth 0–50 is accepted, cutoff is
configurable), and build anything else on an experiment branch, not `main`.

### 5.5 Measurement rules for the paper

The repository's measurement policy (one sample, no reruns) is for
regression work. A paper needs repeated samples with variance. Before the
campaign:

- Get an explicit owner ruling that the paper campaign may take N ≥ 5
  samples per cell and report median plus a 95% interval.
- Keep the cold-cache method from the ovnfs rerun (`fsync` +
  `posix_fadvise(DONTNEED)` per file, `fincore` check to 0 bytes) and
  extend it to all baselines.
- Pin every binary and image by digest, as the current reports do.

## 6. Gaps to close, in order

1. **fsstress failures** (7 of 9 rows at `76a32a9a`). A storage paper with an
   open stress failure won't survive review. Fix it, or show it's a harness
   problem.
2. **Cold read throughput** (158 MB/s vs 943 MB/s for one 500 MiB file).
   Profile it: chunk size, read-ahead, FUSE request size, decode path. Either
   close the gap or explain it as a measured tradeoff.
3. **Native Linux testbed** with the same route into every system.
4. **Baselines:** AgentFS, overlayfs/Docker, btrfs (rerun; the September raw
   data is lost), ovnfs on a reflink filesystem, JuiceFS.
5. **Scale sweeps** for mount and Commit independence (W7).
6. **Ablations** (§5.4).
7. **Agent traces** (W5), even a small set. This is what makes it an
   agent paper rather than a generic versioned-filesystem paper.
8. **Import speed** (6–8× slower than ovnfs): fix it or scope it out as a
   one-time cost.

## 7. Venue and timeline

- **Near term: HotStorage** (5 pages). Thesis, design summary, the ovnfs
  results, Git as a reference point, and gaps 1–2 addressed or disclosed.
  The current evidence is close to enough.
- **Full paper: FAST or USENIX ATC** (12 pages), after gaps 1–7. EuroSys
  is also possible. FAST's deployed-systems track fits if there's real
  usage to report (for example, dsh users).
- Release a technical report on arXiv alongside the workshop submission to
  establish priority.

## 8. Reviewer questions to prepare for

- "Isn't this just FastCDC plus zstd?" Answer with the placement: online,
  per Commit, inside a live overlay, at tool-call latency. Ablations show
  each mechanism's share.
- "Why not btrfs or ZFS snapshots?" They share extents for in-place updates
  but not for rewritten files or rename-over saves. Show W1 and W6.
- "Why not Git?" It isn't a live filesystem, it has to scan the tree, and
  its Commit cost grows with the tree. Show the W7 sweep.
- "Same bytes, different roots?" Explain why localized edits produce
  different partitions, and what `same_file_bytes` costs.
- "Disposable profile means no durability?" State the profile, what it
  guarantees, and that Durable is disabled for now.
- "Single writer?" Concurrent Commits are expected to be rare. Show W8's
  uncontended and contended Commit latency.
- "Garbage collection?" State it as future work (spec in PR #681).
