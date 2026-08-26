# Stage 2 Docker/Linux FUSE — final source-bound result

Status: `PASS_OPTIMIZED`.

The admitted product path is the native Linux route `layerfs-fuse -> MountedWorkspace -> Engine/Core -> Store`. Stage 1.2 remains skipped. The campaign uses no benchmark shim, backing tree, SDK/evaluator bypass, `.bench` recognition, network scenario, tracing asymmetry, emulation, storage-control shortcut, or weakened gate.

## Frozen identity

- Product source commit: `bd1cd225e152a630a10520806ecca65593c71a6b`
- Product source tree: `211bdec5dd38ac281c9ec3d08d0ca9d659ad3dea`
- ARM64 image: `sha256:731f86a01661eb8dfd37910ee70509f4212d2cf1d2c7418d4d1b9b961f8e3139`
- Executable SHA-256: `fbe800c136a973430a3bf47dbb28ed861348e1f7fa135aead2558b40bcad7258`
- Executable BLAKE3: `cc6a76415e42fbef97f569310db6d805ffd08d301a0136198b87e052d8ffc1f3`
- Upstream `fs-bench.sh` SHA-256: `0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef`
- Authoritative CPU envelope: exact Docker `--cpus 1` CFS quota (`cpu.max = 100000 100000`), with no cpuset substitution.

The full host workspace closure, corrected full Linux workspace test closure, Stage 2 Linux clippy, and uncached exact-current image build pass. The local image manifest is nonempty and binds the image ID, platform, configuration, and nine rootfs layer identities.

## Correctness and lifecycle

- Exact 100 MiB checkpoint: 224,115,542 ns, below the controlling 400 ms gate.
- Same-daemon sequential read: 1,392.71 MiB/s; restart read: 793.10 MiB/s.
- Same-daemon and restart bytes, random ranges, mmap, hard links, portable metadata, shrink/re-extension zeroes, unlink-open semantics, repeated orphan fsync, and namespace absence all pass.
- Dirty graceful `SIGTERM`, pre-/post-ack forced death, restart, and mounted splice/remount oracles pass with exact independent reopen bytes.
- The high-entropy 100 MiB SQL-miss path passes with 5,440 created objects, three exact reuses, bounded Q/memory, zero OOM, and zero residue.
- Terminal mounted ownership is root-only (`lookup_refs=1`, `live_nodes=1`, `inode_mappings=1`), with handles, pending/dirty nodes, dirty ranges, directory changes, logical workspace bytes, spool live/dead/physical, operation Q, and Store connections all zero.

The retained checkpoint-local `strict_zero_cfs_event=false` result is a nonblocking diagnostic. The controlling equations are checkpoint latency at most 400 ms and authoritative population `throttled_usec / wall <= 5%`; both pass.

## Readiness and authoritative performance

The readiness-specific validator applies the actual `REPS=1 WARMUP=0 RANDOMIZE_TARGETS=1` contract and reports `READY` for both controls. The n=3 verifier’s readiness `REVISE` receipts are preserved as inapplicable diagnostics.

Both authoritative populations use the exact 12-scenario filter with `REPS=3 WARMUP=1 RANDOMIZE_TARGETS=1`, contain 24 unique rows, and have zero ANSI-stripped FAIL markers and zero network scenarios.

| Control | SL | Rsum | G | Spread | Result |
|---|---:|---:|---:|---:|---|
| `/var/tmp` overlay | 2.920 s | 2.049 | 3.171 | 1.010 | `PASS_OPTIMIZED` |
| `/tmp` tmpfs | 3.120 s | 2.277 | 3.821 | 1.009 | `PASS_OPTIMIZED` |

Every Cloudflare per-row ratio cap passes.

## Resource, publication, and evidence equations

- Authoritative CFS throttling ratios are 1.239% (`/var/tmp`) and 1.182% (`/tmp`), both below 5%.
- Cgroup memory peaks are 284,471,296 and 283,856,896 bytes; daemon RSS high-water values are 11,538,432 and 11,567,104 bytes. OOM and OOM-kill deltas are zero.
- FD high-water is 11 against a limit of 74; thread high-water is 6. Store connection HWM is 1 and terminal is 0.
- All 12 per-scenario daemon CPU/wall pairs are exact. The three prior substring collisions (`write`, `copy`, and `read`) are superseded by singleton two-row external captures, and the synthesizer independently recomputes their actual scenario sets.
- Each fresh authoritative Store has exactly one genesis publication and zero campaign checkpoints, with one started/committed transaction and zero rollbacks.
- Original separate benchmark stderr was unrecoverable because the commands merged `2>&1` through `tee`. Only smoke, two readiness commands, and two authoritative command shapes were rerun on the identical source/image/environment for stream custody; all benchmark and verifier stderr files are genuinely empty. These recaptures do not replace accepted timings.
- All task-owned containers, volumes, processes, mounts, Store journals, spools, receipts, and scratch residue are absent after cleanup. The immutable image is intentionally retained.

Candidate 012 and all failed/intermediate candidate 013 diagnostics remain preserved and superseded. `SHA256SUMS` is generated only after every other retained artifact is final.
