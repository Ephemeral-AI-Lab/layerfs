# M. LayerFS vs Modal's sandbox filesystem (ImageFS + overlayfs, and ImageFS-blk)

Compiled 2026-11-14, read-only. Nothing built, run or edited in any repository.

**Identities and sources**
- LayerFS: `origin/layerfs_v2_0_0` @ `8cfb6e2734`, read with `git show` (paths below are on that ref
  unless marked). `v2doc:` = `docs/architecture/...` on that ref. Verified notes reused from
  `docs/research/paper/notes/B-engine.md` (§1–3, 9, 10), `D-speed.md` (§2–4), `E-evaluation.md`
  (§1–2, 8), `A-website.md` (§2.4).
- Benchmarks: `LFB` = `~/Ephemeral-AI-Lab/layerfs-benchmark`. Build identities differ by
  table (2.0 builds `b9c4adcb`, `833d04cf`; v1.0.0-era `e5c7aaae05` for fio/smallfile three-arm).
  All LayerFS timings are on Docker Desktop (arm64 Mac, 8 vCPU, ~8 GiB), shared host, and labelled
  INELIGIBLE/diagnostic by their own reports except the ovnfs cold rerun (3 samples, verified
  eviction, guest page cache only).
- Modal (all are **the post's/docs' claims**, not reproduced by us):
  - **[P]** "Building fast filesystems for VMs", Nitin Maddi and Amit Prasad, 2026-10-16,
    https://modal.com/blog/vm-sandbox-filesystem
  - **[V]** "VM Sandboxes: Full computers for agents", Amit Prasad, 2026-10-01,
    https://modal.com/blog/vm-sandboxes-agent-computers
  - **[S]** Sandbox snapshots docs, https://modal.com/docs/guide/sandbox-snapshots
  - **[D]** Sandboxes guide (runtimes, limits), https://modal.com/docs/guide/sandboxes and
    https://modal.com/docs/guide/vm-sandboxes
  - **[J]** "Fast, lazy container loading in Modal.com", Jonathon Belotti, 2024-09-08,
    https://modal.com/blog/jono-containers-talk
  - **[L]** "How Modal speeds up container launches in the cloud", Yiren Lu, 2024-08-16,
    https://modal.com/blog/speeding-up-container-launches
  - **[M]** "Scaling to 1 million concurrent sandboxes in seconds", Colin Weld and Connor Adams,
    2026-07-16, https://modal.com/blog/scaling-to-1-million-concurrent-sandboxes-in-seconds

**Most important framing finding.** The owner describes Modal's design as "imagefs (FUSE) +
overlayfs". That is Modal's **container (gVisor) design and its abandoned first VM design**. The post
[P] is about *replacing* it for VM Sandboxes: Modal measured FUSE/virtio-fs + overlayfs as too slow
for metadata-heavy agent workloads and moved to **ImageFS-blk**, a host daemon serving a lazily
populated, synthesized ext4 image over virtio-blk, with block-level copy-on-write into a per-VM sparse
file. A fair paper comparison must address both: (a) FUSE lower + kernel overlayfs upper (the
owner's framing, still Modal's container path), and (b) ImageFS-blk (what Modal ships for VMs).

---

## 1. Modal design summary

### 1.1 ImageFS (2022–, containers; the "FUSE + overlayfs" design)
- **Lower layer.** ImageFS, introduced April 2022: a distributed, lazy-loading, read-only filesystem
  served by a Rust FUSE server. Only minimal metadata is fetched up front [P]. The image is an index of
  files and metadata of about 5 MB that points to file data; the index loads in 1–100 ms and is
  FUSE-mounted in about 2 ms [J].
- **Content addressing and caching.** File contents are content-addressed [P][J]; [L] describes
  hashing each file's contents and storing it by that hash, i.e. **whole-file** granularity (no
  chunking mentioned). Tiers: memory, local SSD, AZ cache server, regional CDN, blob storage [J][P].
  [J] gives indicative tier latencies (memory ns, SSD ~100 µs, AZ cache ~1 ms, CDN ~100 ms, blob
  ~200 ms). Files shared between images are loaded once (e.g. >8,700 common files between two ML
  images) [J]. Background prefetching keeps reads on local disk or memory [P].
- **FUSE tuning.** Read-only, so the kernel may cache FUSE replies aggressively [P]. Read-ahead raised
  to 32 MB (default 128 KB); max request size ~1 MB, keeping the page cache rather than direct I/O;
  FUSE throughput rose from 800 MB/s to about 2.5 GiB/s; a 512 MiB file loads in ~200 ms from disk
  cache and ~300 ms from network [J]. (A 1 GB read-ahead caused production latency problems [J].)
- **Writes.** Kernel OverlayFS: read-only ImageFS lower + writable upper; first write to a file copies
  it up [P]. The post states the upper is XFS ("our lower is a FUSE filesystem and upper is XFS") and
  that reflinks did not work in this arrangement, so copy-up is whole-file; in degenerate cases this
  "incurs gigabytes of overhead to write individual bytes" [P]. Where the upper lives (local disk vs
  tmpfs) is not stated. How gVisor's own overlay is configured is not described in [P].
- **Cold start.** Sub-second container cold start attributed to ImageFS [P]. At scale: median sandbox
  start (create request to user code) under 0.5 s; scheduling tens of ms; ~1 M sandboxes created in
  under a minute [M].

### 1.2 First VM attempt: virtio-fs over ImageFS + OverlayFS (abandoned)
- Host `virtiofsd` served the guest from the same ImageFS/OverlayFS stack; lazy loading and caching
  were kept [P].
- Performance was poor: recursive `chown` from ~4 s to over a minute; `pnpm`/`bun install` ~3× slower;
  `ripgrep` degraded sharply [P]. One guest `chown` = 3 guest-to-host round trips (getxattr, getattr,
  setattr), 6 host syscalls, ~99 µs [P].
- Diagnosis [P]: virtio-fs makes the host the authority for metadata, so every metadata op crosses the
  VM boundary although the host never needs the VM's root filesystem.
- Microbenchmarks, 100,000 calls each [P] (columns: virtio-fs, gVisor, ImageFS used directly by the
  host): stat 3.23 / 2.12 / 1.45 s; link 9.75 / 3.91 / 2.57 s; chown 9.87 / 2.31 / 1.58 s; hardlink
  tree 12.61 / 4.61 / 2.93 s; cp -R 32.90 / 17.75 / 6.48 s.

### 1.3 ImageFS-blk (current VM Sandboxes)
- **Lower.** For a requested `modal.Image`, Modal fetches minimal metadata, writes ext4 inode records
  itself (permissions, sizes, extent maps) and leaves the rest of the ext4 image sparse; blocks are
  supplied only when the guest reads them [P]. `mkfs.ext4 -d` was rejected because it needs all
  content up front [P].
- **Read path.** A host daemon (ImageFS-blk) serves virtio-blk requests, maps block ranges to ImageFS
  file byte ranges, fetches through the same cache hierarchy; dirty blocks and ext4 metadata come from
  the sparse file [P]. The guest kernel's ext4 and page cache do everything else.
- **Writes / copy-on-write.** Written blocks go to a per-VM sparse file and are marked in a dirty
  bitmap; COW is **block-level**, bounding the worst case to copying one block [P]. Block size not
  stated. ext4 journals in the guest and, per a figure caption, commits its journal and ships
  accumulated changes to the host in one batch about every 5 s [P].
- **Metadata.** A guest `chown` takes ~2.1 µs, all in the guest [P].
- **Snapshots.** Only: snapshots consult the dirty bitmap "to figure out what changed" [P]. No format,
  timing or capture details.
- **Numbers (post's claims; no hardware, VM size, run count or variance given [P]).**
  - 100k-call microbenchmarks, ImageFS-blk vs virtio-fs: stat 0.90 vs 3.23 s; link 0.80 vs 9.75 s;
    chown 0.21 vs 9.87 s; hardlink tree 1.29 vs 12.61 s; cp -R 5.74 vs 32.90 s.
  - Workflows (virtio-fs + ImageFS / ImageFS / ImageFS-blk): `bun install` 12.5 / 3.1 / 1.07 s;
    ripgrep on the Linux repo warm 6.03 / 3.77 / 0.18 s, **cold 24.3 / 4.47 / 8.53 s** (ImageFS-blk
    slower than ImageFS cold, unexplained); SQLite speedtest1 543.4 / 289.8 / 43.2 s; pgbench
    read-only 98.2k / 137.1k / 230.1k TPS; read-write 8.3k / 11.9k / 18.8k TPS.
  - Cold start: sub-second retained with prefetching, no full image download [P][V].

### 1.4 Isolation, snapshots and limits (docs)
- **Isolation.** gVisor is the default runtime; VM runtime gives each Sandbox its own Linux kernel
  [D][V]. VMM is Cloud Hypervisor plus Modal components for host filesystems, lazy image loading,
  memory bursting and snapshotting [V]. GPU Sandboxes are gVisor only [D].
- **Snapshots [S].**
  - `snapshot_filesystem()`: copy of the root filesystem, excluding mounted Volumes; stored as a Modal
    Image, **a diff from the base image, storing only modified files** (i.e. whole-file granularity as
    documented). Restore = new Sandbox from that Image; many Sandboxes may fork from one snapshot.
  - `snapshot_directory(path)`: one directory as an Image, mountable into a running Sandbox.
  - `_experimental_snapshot()` (alpha): **memory + filesystem**; processes resume in the clone. Limits:
    snapshotting terminates the Sandbox; TCP connections close; not during an `exec`; exec'd
    background processes not properly restored; no GPUs; same instance type required; 7-day expiry.
  - Filesystem/directory snapshots expire after 30 days by default (`ttl`, `None` = never); no API to
    list snapshots; delete via Image deletion. No latency or size figures; restores use the same
    infrastructure as fast cold starts.
- **Limits [D].** Default Sandbox lifetime 5 min, max 24 h (docs recommend snapshot + restore for
  longer); VM runtime targets Docker-in-Sandbox, FUSE mounts, nested cgroups. `/var/lib/docker` is
  included in filesystem snapshots.
- **Not stated anywhere read:** chunk size, hash function, cache sizes/eviction, upper-layer device,
  durability guarantees, snapshot capture time, branch/merge/diff/history APIs.

---

## 2. LayerFS overlay summary (2.0, `8cfb6e2734`)

- **Lower = an immutable committed root in a versioned Store**, not an image. Mount binds the
  Branch's root with one snapshot read, one root decode, a copy of a schema-only overlay template and
  one overlay open; "no tree is scanned or copied" (v2doc `engine/overlay/README.md:62-64,173`;
  `engine/workspace/README.md:144`; source `layerfs-store-service/src/store/bind.rs`,
  `layerfs-overlay/src/database/startup.rs`).
- **Lazy loading.** `BaseView` reads only the objects a request needs (inode-table pages, directory
  pages, file mapping; `plan_read` ≤ 128 KiB); every fetched object is re-hashed to its BLAKE3
  `ObjectId` (authenticated); one 64 MiB `CanonicalCache` per daemon keyed by object id, shared across
  Workspaces and Branches; sequential chunked reads fetch range + 8 MiB; one demand ≤ 4,096 ids /
  32 MiB (v2doc `engine/workspace/README.md:70-87,145,161`). Content unit: whole file below 128 KiB,
  FastCDC chunks 8/16/32 KiB above, with extent B+trees (C-core-storage notes; `LFB/comparision/ovnfs/rerun-2026-11-07/design.md`).
  There is **no** CDN/blob tier: the Store is one SQLite file (one kernel, local ext4/xfs/btrfs) or a
  PostgreSQL schema over a connection string (v2doc `engine/store/README.md:142,160`;
  `engine/store/postgresql.md`).
- **Upper = one SQLite file per Workspace** (`<n>.sqlite`, journal MEMORY, synchronous OFF, EXCLUSIVE,
  mmap 0, 4 MiB pager), deleted at unmount; no fsync ever (`layerfs-overlay/src/database/profile.rs:56-62`;
  v2doc `engine/overlay/README.md:92,170`). Generation-versioned `fs_inode`/`fs_entry` rows
  (NULL inode = whiteout); open-file and kernel-reference state in daemon memory, uncapped
  (`overlay/README.md:127-145,181`).
- **Copy-up granularity: none.** Payload is 4 KiB cells / dense runs ≤ 32 KiB with a validity mask;
  a write "stores only its own bytes and reads no base payload; uncovered bytes stay inherited"
  (v2doc `engine/workspace/operations.md` "Files"; `engine/overlay/payload.md`). Kernel writes may be
  recognized against the file's own committed chunks and stored as **reference pieces** (no bytes)
  (`layerfs-workspace/src/operations/file/recognize.rs`). Truncation is a one-row "staircase".
- **Metadata-only ops.** chmod/mtime/size = a new `fs_inode` row for that serial; rename = `fs_entry`
  rows (+ a `predecessor` used as delta base at Commit); no data copied (v2doc
  `engine/workspace/operations.md:34-41,66-72`). mtime-only change keeps the base row at Commit.
- **Commit.** Capture flips generations (copies no rows); one producer constructs only changed
  inodes; localized edits split extents and rebuild only join-path pages; unchanged subtrees keep
  their IDs; then Save (dedup, zstd, deltas) and a short publication lane; install rebinds without a
  tree walk (v2doc `engine/commit/README.md:46-59`; `engine/commit/construction.md`; B-engine §3).
  Captures every published change from any process; not a process fence (mmap dirty pages, running
  descendants).
- **Fork/history.** Fork = one `INSERT_BRANCH` row; Commits, Branches, lineage, three-way merge with
  marker blocks, `.lww` rules (B-engine §7–8).
- **Kernel route (Linux).** fuser 0.18.0 (3 authorized patches); negotiated `FUSE_ASYNC_READ |
  FUSE_BIG_WRITES (+MAX_PAGES)`, `max_write = max_readahead = 128 KiB`, `max_background = 1`,
  congestion threshold 1 (`layerfs-fuse/src/mount/profile.rs:33-46`); 2 receive loops
  (v2doc `kernel-routes/fuse.md:46`); writeback cache never selected; `FOPEN_KEEP_CACHE` on; 60 s
  entry/attr TTL; FSYNC answers ENOSYS once (v2doc `kernel-routes/README.md:130-153`). Unsupported:
  xattrs, locks, fallocate, copy_file_range, IOCTL, LSEEK, READDIRPLUS; FIFOs/sockets/devices refused
  (`kernel-routes/README.md:96`).
- **Isolation.** Sandbox backends docker / namespaces (relies on an enclosing VM/container) / macOS
  native (none) (B-engine §10.9). LayerFS is a filesystem + Store, not a VM platform.

---

## 3. Comparison table

Legend for "Bench": **Y** = a measurement exists (cited), **P** = proxy only (different system or
build), **N** = none. "Modal" values are the post's/docs' claims.

| # | Row | Modal (FUSE+overlayfs = "ImageFS"; VM = "ImageFS-blk") | LayerFS 2.0 | Winner and why | Bench |
|---|---|---|---|---|---|
| 1 | Lower layer | Read-only image: whole-file content-addressed ImageFS behind FUSE [P][L]; for VMs a synthesized sparse ext4 image over virtio-blk [P] | Immutable committed root of a Branch in a versioned Store; any Commit of any Branch is mountable (`overlay/README.md:173`) | **LayerFS** for agent state: the lower is the agent's latest committed project state, not a build-time image. **Modal** for "boot an arbitrary OS image" | N |
| 2 | Lazy-load unit and authentication | Whole file [L]; FUSE requests ~1 MB, read-ahead 32 MB [J]; ImageFS-blk maps blocks to file byte ranges [P]; authentication not described | Objects: inode/dir pages, whole files <128 KiB, CDC chunks 8/16/32 KiB; each object re-hashed to its BLAKE3 id; 8 MiB daemon read-ahead (`workspace/README.md:80-87`) | **Split.** LayerFS finer-grained and authenticated; Modal far larger kernel read windows and a distributed cache hierarchy | P (cold reads §row 11) |
| 3 | Where writes go | overlayfs upper on XFS (location unstated) [P]; ImageFS-blk: per-VM sparse file + dirty bitmap, guest ext4 page cache in front [P] | Per-Workspace SQLite file, written synchronously by the daemon for every `write(2)` (writeback off) | **Modal** on speed (kernel page cache absorbs writes). **LayerFS** on exact capture (every acknowledged write is in the overlay) | Y/P (row 12) |
| 4 | Copy-up granularity | overlayfs: whole file, no reflinks, "gigabytes of overhead to write individual bytes" in degenerate cases [P]; ImageFS-blk: one block [P] | No copy-up: 4 KiB cells, base bytes stay inherited; recognized chunks stored as reference pieces (`operations.md`, `payload.md`) | **LayerFS ≫ FUSE+overlayfs**; **≈ ImageFS-blk** (both sub-file; LayerFS additionally avoids storing re-written identical chunks) | P: ovnfs (user-space AUFS-style overlay, whole-file copy-up): 100 B edit in 500 MiB, exec 44.5 vs 217.2 ms (E-evaluation §2) |
| 5 | Metadata-only ops (chmod, rename, mtime) | overlayfs: chmod/chown of a lower file triggers copy-up (full data unless `metacopy=on`; Modal does not say which); virtio-fs chown ~99 µs; ImageFS-blk chown ~2.1 µs in-guest; 100k chown 0.21 s [P] | One `fs_inode`/`fs_entry` row, no data; but each op is a FUSE round trip + SQLite job. smallfile chmod 17,275 files/s (≈5.8 s per 100k) vs kernel overlayfs 973,438/s on fresh upper files (v1.0.0 build, `LFB/.archive/data/smallfile/.../three-arm`) | **Structurally LayerFS beats classic overlayfs copy-up on lower files; in speed, ImageFS-blk and kernel overlayfs are ~1–2 orders of magnitude faster per op** (different hosts; not comparable as a ratio) | P |
| 6 | Snapshot / Commit: what is captured | `snapshot_filesystem`: root FS minus Volumes, as an Image diff of modified files [S]; memory snapshot (alpha) also captures processes [S] | Published frontier of the Workspace from any process; not memory, processes or unflushed mmap pages (`commit/README.md:58`) | **Modal** captures more (memory/process state, alpha); **LayerFS** captures exactly filesystem state at a defined frontier | N |
| 7 | Snapshot / Commit: cost model | Not published. Docs say diff of modified files [S] → proportional to bytes of modified *files* (inference); ImageFS-blk dirty bitmap could make it ∝ dirty blocks [P], but not stated | ∝ changed paths + changed bytes; unchanged subtrees and extent pages reused; capture copies no rows. Measured cold: 100 B edit in 10–500 MiB → 19.5–28.1 ms; 5-op code edits 35.8–46.0 ms; new 500 MiB 1,359 ms (D-speed §2; E-evaluation §2) | **LayerFS, much better vs whole-file diffs**; vs a block-dirty-bitmap snapshot the advantage is smaller and unmeasured | P (ovnfs: L500 commit 603.9 vs 28.1 ms cold, 21×; NEW500 ovnfs faster 997 vs 1,359 ms) |
| 8 | Snapshot output, dedup, delta | A Modal Image (`im-...`), whole-file diff; dedup only via whole-file content addressing (inference from [L]); no delta; 30-day default TTL [S] | Immutable content-addressed Commit; CDC chunk dedup, zstd, prefix deltas, delta-encoded metadata; no TTL | **LayerFS, much better**: e.g. 100 MiB file × 10 small edits stored 106.3 MB vs 1,154.7 MB on a whole-file system (ovnfs) | P (ovnfs, E-evaluation §1b) |
| 9 | Fork / branch | New Sandbox(es) from a snapshot Image; memory-snapshot clone [S] | Fork = one row insert; mount 13–21 ms; branch open (fork+mount) 31–41 ms (INELIGIBLE, D-speed §2) | **LayerFS** for fork cost and depth; **Modal** for forking a *running process* (alpha) | Y (LayerFS only) |
| 10 | History | No list API; track Image ids yourself; expiry; no diff/merge/lineage [S] | Commits, Branches, lineage, diff, three-way merge, blame, `.lww` | **LayerFS, decisively** (a feature Modal does not offer) | N (feature) |
| 11 | Sharing across sandboxes/machines | Global: memory → SSD → AZ cache → regional CDN → blob [J]; images shared across fleet; ~1 M concurrent sandboxes [M] | One daemon cache per Store binding shared by its Workspaces; SQLite Store needs one kernel; PostgreSQL Store reachable over a connection string; no CDN/blob tiering | **Modal, decisively** at fleet scale | N |
| 12 | Read path | Kernel page cache; FUSE with 32 MB read-ahead, ~2.5 GiB/s [J]; ImageFS-blk: native guest ext4 + page cache; warm ripgrep 0.18 s, cold 8.53 s [P] | FUSE, `max_readahead` 128 KiB, `max_background` 1; KEEP_CACHE so warm hits are page-cache speed (fio rand-read p50 1.0 µs, all arms ~1 µs). Cold 500 MiB file 158 MB/s vs 943 MB/s (ovnfs FUSE); cold 65 MB tree 1.3–1.5× slower than ovnfs | **Modal** on cold/sequential reads; **tie** on page-cache hits | Y (LayerFS cold reads, E-evaluation §2b; fio three-arm v1.0.0) |
| 13 | Write path | overlayfs upper / guest ext4: buffered in page cache | Writeback off; every write a FUSE request + SQLite job. fio (v1.0.0 build) seq-write 514 vs 3,258 MB/s kernel overlayfs (0.158×), 4 KiB rand-write 0.013×; smallfile create 6,693 vs 204,225 files/s | **Kernel overlayfs / ImageFS-blk by 6–75×** on write-heavy steady state | Y (three-arm, v1.0.0-era build, warm, one sample) |
| 14 | Durability | Guest ext4 journal batched to host ~every 5 s [P]; snapshot storage durability not stated | Store Disposable (WAL, sync OFF; crash can lose/damage it); overlay never synced; FSYNC → ENOSYS | **Modal likely** (unstated); LayerFS explicitly disclaims durability | N |
| 15 | Cold start | Container median <0.5 s at scale [M]; VM sub-second claimed [P][V]; image index FUSE-mounted ~2 ms [J] | Workspace mount 13.2–21.5 ms on a running Sandbox; Sandbox create 273–414 ms steady, first 3.4–5.3 s on a fresh Docker machine (INELIGIBLE, D-speed §2) | **Apples-to-oranges**: LayerFS mount (FS only) is fast; whole-sandbox start favors Modal's platform | P |
| 16 | Correctness / POSIX | VM: real guest ext4 — full POSIX, xattrs, Docker, FUSE inside [D][V] | No xattrs, FIFOs, sockets, devices, set-id, atime/ctime, fallocate, locks, copy_file_range, RENAME_EXCHANGE; pjdfstest keeps 76% of reference passes, 0 unexpected; fsx 3.06 M ops 0 miscompares; fsstress commit→fork→compare 3,755/3,755 identical (A-website §2.4, v1.0.0-era) | **Modal**, clearly (cannot host `/var/lib/docker` on LayerFS) | Y (LayerFS only) |
| 17 | Isolation | gVisor or Cloud Hypervisor VM [V][D] | Not a VM platform; docker/namespaces backends rely on enclosing container/VM | **Not comparable**; LayerFS could sit inside a Modal-style VM | N |

---

## 4. Verdict

**Where "much better" holds (with evidence or clear structure):**
1. **Snapshot/commit cost proportional to change**, not to modified-file size. LayerFS commits a
   100 B edit to a 500 MiB file in 19.5–28.1 ms cold and stores 3–8 KB, against 0.6 s and a full
   copy on a whole-file-copy-up overlay (ovnfs proxy). Modal's documented snapshot is a whole-file
   diff Image, so a SQLite DB, `.git` pack or build artifact touched by one byte would be captured
   whole (inference from [S]; Modal publishes no snapshot timing).
2. **No copy-up.** Against FUSE lower + overlayfs upper this is a real, structural win ([P] itself
   names the gigabytes-per-byte degenerate case). Against ImageFS-blk (block COW) it is roughly a tie.
3. **Storage across many snapshots**: CDC dedup + deltas (3–20× less than whole-file on large-file
   edits; 7.6–10% less on code histories, where the base tree dominates).
4. **Fork, history, merge.** Fork is a row; history is first-class with lineage and merge. Modal has
   no equivalent (snapshot ids, TTL, no list/diff/merge).
5. **Metadata-only ops do not copy data** (vs overlayfs without `metacopy`).

**Where it does not hold (LayerFS is worse, sometimes by orders of magnitude):**
1. **Steady-state write throughput and per-op latency**: kernel overlayfs is 6–75× faster on writes
   and creates (fio, smallfile three-arm), because LayerFS has writeback off and publishes every
   write into SQLite synchronously. ImageFS-blk runs metadata ops entirely in-guest (~2.1 µs chown).
2. **Cold sequential reads**: 158 MB/s vs ~940 MB/s for another FUSE overlay; Modal reports
   ~2.5 GiB/s FUSE reads with 32 MB read-ahead. LayerFS's 128 KiB read-ahead and `max_background=1`
   are the prime (unmeasured) suspects.
3. **POSIX coverage**: no xattrs/FIFOs/sockets/locks/fallocate; Docker-in-sandbox and some databases
   cannot run on a Workspace. Modal VMs run real ext4.
4. **Distribution and fleet scale**: no CDN/blob tiers; SQLite Store is single-kernel.
5. **Process state**: Modal offers (alpha) memory snapshots; LayerFS captures files only.
6. **Durability**: LayerFS disclaims it by design.

**Apples-to-oranges.** Modal's post is about a sandbox *platform* (VMM, scheduler, global image
cache) and optimizes package installs, ripgrep and databases; LayerFS is a *versioned filesystem*
optimizing many small commits, forks and merges. Modal's snapshot cost is unpublished, and LayerFS
has no measurement on Modal's workloads (bun install, ripgrep on Linux, speedtest1, pgbench), nor
on native Linux. Every existing "vs overlay" number is either a FUSE proxy (ovnfs), a v1.0.0-era
build, or warm one-sample Docker Desktop data. **Honest claim:** "much better for capture, storage,
fork and history of agent state; substantially slower on raw I/O than kernel overlayfs or a block
device, and narrower in POSIX coverage." The owner's unqualified "much better" does not hold for the
filesystem as a whole, and the target Modal now ships (ImageFS-blk) is not FUSE + overlayfs.

---

## 5. Proposed experiments (fair, same host, native Linux x86-64 and arm64, no Docker Desktop)

Arms throughout: **(A) LayerFS** 2.0 Workspace (SQLite Disposable, `LAYERFS_CONSTRUCTION_WORKERS=1`);
**(B) FUSE-lower + kernel overlayfs**: a read-only lazy FUSE image (e.g. stargz/SOCI snapshotter or
squashfuse over a content-addressed store) as lower, XFS upper without reflink, snapshot = tar of the
upper (+ whiteouts) into a whole-file content-addressed store (models Modal [P][S]);
**(B') same with `metacopy=on`**; **(C) block analog of ImageFS-blk**: ext4 on a qcow2/dm-snapshot
overlay over a base image, snapshot = dirty-block extraction; **(D) native ext4** (ceiling, no
snapshot). Enforce identical declared cache states (cold = evict + restart) per AGENTS rules.

1. **Per-tool-call agent replay.** Replay recorded agent traces (e.g. dsh/#352 Bash steps or a
   SWE-style trajectory set) with a snapshot after each mutating call. Metrics: added tool wall per
   call (p50/p99) vs D; snapshot latency; bytes stored per snapshot and cumulative; restore/fork-to-
   first-exec latency. Tests the core claim end to end.
2. **Copy-up stress.** Single-byte / 100 B edits and appends to 10 MiB–1 GiB files, a SQLite DB under
   a small update workload, and a `git commit` in a large repo (pack/index rewrites), snapshot after
   each step. Metrics: exec time, snapshot time, stored bytes, peak upper size. Expect A ≈ C ≫ B.
3. **Metadata-heavy and install workloads.** Modal's own set: 100k-call stat/link/chown/hardlink/cp -R
   microbenchmarks, `bun install`/`pnpm install`, ripgrep on the Linux tree (warm and cold), SQLite
   speedtest1, pgbench. Metrics: wall, ops/s, CPU; plus one snapshot afterwards (time, bytes). Shows
   the raw-I/O cost of A honestly.
4. **Fork fan-out.** From one snapshot/Commit, start N = 1, 8, 64 writable views and run the same
   step. Metrics: time to first exec, per-view memory, storage after N divergent edits, cache hit
   ratio (shared object cache in A vs page cache sharing in B/C).
5. **History scaling.** 1,000 sequential snapshots on a 65 MB code tree (5-op edits) and on a 1 GiB
   tree with build artifacts. Metrics: snapshot latency vs snapshot index (should stay flat), total
   storage, read latency of the head and of an old snapshot (delta chain cost), time to list/diff.
6. **Read-path ablation (diagnostic, LayerFS only).** Vary `max_readahead` (128 KiB → 1/32 MiB),
   `max_background` (1 → 12), and writeback cache on/off, on cold 500 MiB and 65 MB-tree reads and
   fio seq/rand write, recording whether capture exactness and commit cost change. Separates design
   cost from tuning cost before claiming anything against Modal.

Also run pjdfstest and xfstests `generic` on all arms to quantify POSIX coverage.

---

## 6. Paper-ready subsection (~600 words)

### Comparison with an image-FUSE + overlayfs sandbox filesystem

Agent sandboxes commonly assemble the root filesystem from a lazily loaded, read-only image and a
writable copy-on-write layer. Modal's design is a representative, well-documented example. Its
ImageFS serves container images through a FUSE server backed by a whole-file content-addressed cache
spanning memory, local SSD, zonal caches, a CDN and blob storage, and places a kernel overlayfs upper
on XFS above it [Belotti 2024; Lu 2024; Maddi and Prasad 2026]. For VM sandboxes Modal reports that
exposing this stack over virtio-fs made metadata-heavy work several times slower, and replaced it
with ImageFS-blk: the host synthesizes a sparse ext4 image, serves its blocks lazily over virtio-blk
and records guest writes in a per-VM sparse file with a dirty bitmap, so the guest kernel owns all
metadata [Maddi and Prasad 2026]. Snapshots are exposed as images holding the modified files, with an
alpha memory snapshot that also captures processes [Modal docs].

LayerFS differs in what the lower layer is and in what a snapshot produces. Its lower layer is the
committed root of a Branch in a versioned, content-addressed Store; mounting binds that root without
scanning or copying the tree, and objects are fetched on demand and verified against their BLAKE3
identities. Writes go to a per-Workspace SQLite overlay in 4 KiB cells; base bytes are never copied
up, and chunks recognized as unchanged are recorded as references. A Commit captures every published
change, constructs only changed inodes and pages, and stores the result with chunk-level
deduplication and delta compression. Forking is a single row insert, and history, diff and three-way
merge are first-class.

These choices win where agents spend their filesystem time between model turns: capturing small
changes to large state. In our diagnostic measurements a 100 B edit to a 500 MiB file commits in
20–28 ms and adds 3–8 KB, whereas a whole-file copy-up overlay takes about 0.6 s and stores a full
copy; across ten such edits to a 100 MiB file LayerFS stores 106 MB against 1.15 GB. Modal's own post
identifies whole-file copy-up as a degenerate case of the overlayfs design; its block-level COW
removes that cost for writes, but its documented snapshot unit remains the modified file, and it
offers no branch, merge or history operations.

The same choices lose on raw I/O. Every LayerFS write is a FUSE request published into SQLite before
it returns, and the mount negotiates 128 KiB read-ahead with one background request. Against kernel
overlayfs on the same machine, LayerFS achieved 0.16x sequential-write and 0.013x 4 KiB random-write
bandwidth and created small files 30x more slowly; cold sequential reads reach about 158 MB/s. Page
cache hits run at kernel speed in all systems. LayerFS also omits xattrs, special files, locks and
fallocate, so workloads such as Docker-in-sandbox, which Modal's VM runtime explicitly targets,
cannot run on a Workspace. Modal additionally distributes images across a fleet, while a LayerFS
SQLite Store is confined to one kernel.

The comparison is therefore not a single ordering. LayerFS is much better at the operations that
make sandbox state versioned (commit cost proportional to change, sub-file copy-on-write, dedup
across snapshots, cheap forks and history) and worse at steady-state throughput, POSIX coverage and
fleet-scale distribution. The two are also partly complementary: a LayerFS Workspace could run inside
a Modal-style VM, with the block device serving the OS image and LayerFS serving the project.
All LayerFS numbers above are single-host diagnostics on an arm64 Docker Desktop VM, and Modal's
figures are taken from its publications without reproduction; Section X proposes a same-host
evaluation that replays agent traces against FUSE-lower + overlayfs and block-level baselines.
