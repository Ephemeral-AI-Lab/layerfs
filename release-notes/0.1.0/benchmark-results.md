# LayerFS 0.1.0 benchmark

> **Status:** Final benchmark evidence for LayerFS 0.1.0 Developer Preview.

This note records the reportable `fs-bench-pro` comparison between LayerFS
0.1.0 and Cloudflare Computer. It contains only the final seven-pair matched
campaign. Earlier exploratory, pilot, invalidated, and pre-fairness results are
not used.

The benchmark answers this product-level question:

> With the container already prepared, how long does a user-visible public SDK
> lifecycle take through a real FUSE Workspace until the resulting Store state
> is committed and readable from the live local process?

It does not measure container provisioning or claim crash/power-loss
durability. It compares LayerFS with Cloudflare Computer, not C3.

## 1. Results

### 1.1 End-to-end SDK latency

Seven deterministically randomized adjacent pairs were executed. The table
reports the median and interquartile range across those pairs. The speedup is
the median of each pair's `Computer / LayerFS` ratio.

| Public SDK lifecycle | LayerFS median [Q1, Q3] | Computer median [Q1, Q3] | LayerFS speedup |
| --- | ---: | ---: | ---: |
| Cold create 32 MiB | **161.231 ms** [157.358, 165.722] | 1,660.321 ms [1,634.804, 1,672.667] | **10.07×** |
| EDIT16 | **169.133 ms** [158.195, 175.139] | 2,631.062 ms [2,592.369, 2,669.643] | **15.80×** |
| Prepend 10 bytes through temp-copy-rename | **232.394 ms** [230.344, 257.604] | 2,484.210 ms [2,439.458, 2,491.989] | **10.48×** |
| Read 32 MiB | **119.154 ms** [116.034, 126.772] | 780.946 ms [776.400, 791.203] | **6.53×** |
| Registered total | **690.196 ms** [672.505, 707.905] | 7,579.414 ms [7,471.326, 7,598.344] | **10.76×** |

All seven LayerFS samples passed the standalone performance gates. Every valid
sample remains in the distribution.

`EDIT16` contains sixteen deterministic ten-byte overwrites distributed across
the 32 MiB file. Its LayerFS result is approximately **10.57 ms per complete
edit**, including a fresh shell process, FUSE execution, capture, Commit, and
Store visibility.

### 1.2 LayerFS phase breakdown

These are medians from the same final campaign, not separate microbenchmarks.

| Lifecycle | Workspace Create | Fresh-shell/FUSE execution | Commit | Workspace End | Complete |
| --- | ---: | ---: | ---: | ---: | ---: |
| Cold create 32 MiB | 10.539 ms | 87.764 ms | 61.960 ms | 4.134 ms | 161.231 ms |
| Prepend 10 bytes | 12.691 ms | 197.397 ms | 20.238 ms | 4.320 ms | 232.394 ms |
| Read 32 MiB | 11.297 ms | 100.632 ms | 1.298 ms | 3.921 ms | 119.154 ms |

For comparison, Computer's host-side Workspace creation and close are already
fast. Its measured time is concentrated inside public
`Workspace.runtime.exec(sync="wait")`, which includes authority-to-executor
synchronization, FUSE execution, executor-to-authority synchronization, and
authority visibility.

| Computer lifecycle | Workspace Create | `runtime.exec(sync="wait")` | Workspace End | Complete |
| --- | ---: | ---: | ---: | ---: |
| Cold create 32 MiB | 1.627 ms | 1,658.162 ms | 0.392 ms | 1,660.321 ms |
| Prepend 10 bytes | 1.541 ms | 2,482.287 ms | 0.402 ms | 2,484.210 ms |
| Read 32 MiB | 1.576 ms | 779.029 ms | 0.401 ms | 780.946 ms |

The comparison therefore does not depend on making Computer's Workspace
creation artificially slow. The dominant difference is its synchronized
execution path.

### 1.3 Incremental storage

Physical allocation and semantic content are reported separately. Semantic
growth is the primary deduplication signal because SQLite may satisfy new rows
from pages already allocated to the database file.

| Storage measurement | LayerFS | Computer | LayerFS reduction |
| --- | ---: | ---: | ---: |
| Seeded 32 MiB Store | 39.0625 MiB | **33.1562 MiB** | LayerFS is **17.81% larger initially** |
| EDIT16 physical allocation growth | **0.2500 MiB** | 9.0000 MiB | **97.22% less** |
| EDIT16 semantic content growth | **0.2250 MiB** | 8.0000 MiB | **97.19% less** |
| Prepend physical allocation growth | **0.0000 MiB** | 47.0000 MiB | **100% allocation growth avoided** |
| Prepend semantic content growth | **0.0256 MiB** | 32.0000 MiB | **99.92% less** |

The zero physical growth for LayerFS prepend does not mean that no data was
stored. LayerFS stored 26,805 canonical bytes, but existing allocated SQLite
pages absorbed them. The semantic row is the accurate representation-level
comparison.

## 2. Environment setup and Git versions

### 2.1 Run identity

| Field | Value |
| --- | --- |
| Campaign | `fair-current-matched-7pair-rerun-20260901` |
| Started | `2026-08-31T21:35:46Z` (`2026-09-01 05:35:46` Asia/Shanghai) |
| Pairs | 7 |
| Pair-order seed | `2767635618933594294` |
| LayerFS HEAD | `07b1fc2a` — `perf: complete fs-bench-plus optimization` |
| Exact LayerFS source seal | `3d516065ce2806519c4694ceb1544592514202b7262015223487d97e580de82e` |
| Ending LayerFS source seal | `3d516065ce2806519c4694ceb1544592514202b7262015223487d97e580de82e` |
| Computer product commit | [`de87919a4fd37242e960e13b7b3ba802d1eef0a0`](https://github.com/cloudflare/computer/commit/de87919a4fd37242e960e13b7b3ba802d1eef0a0) |
| Local Computer benchmark-harness HEAD | `64c462b` — `bench: account for spawned computerd tasks` |
| Computer product patches | None |
| Fixture | 33,554,432 bytes |
| Fixture SHA-256 | `3d2fadd86ea3d8c52f8f3255bec470f2da7e31b7ed809cc0e97e1e9dc894cd8c` |

The LayerFS worktree contained the 0.1.0 product and benchmark changes without
a final commit representing the entire tree. Consequently, `07b1fc2a` alone is
not the complete source identity. Reproduction requires the recorded source
seal and captured working-tree patch. The starting and ending seals are equal,
proving that the benchmark dependency closure did not change during the
campaign.

### 2.2 Host and toolchain

| Component | Value |
| --- | --- |
| Host | MacBook Pro `Mac15,10` |
| Processor | Apple M3 Max, 14 cores: 10 performance and 4 efficiency |
| Host memory | 36 GB |
| Host architecture | `arm64` |
| macOS | 26.4.1, build `25E253` |
| Darwin kernel | 25.4.0, `RELEASE_ARM64_T6031` |
| Git | 2.47.1 |
| Rust | `rustc 1.96.0 (ac68faa20 2026-05-25)` |
| Cargo | `cargo 1.96.0 (30a34c682 2026-05-25)` |
| Node.js | 22.22.0 |
| Docker Desktop | 4.76.0 (`228118`) |
| Docker Engine | 29.5.2, API 1.54, Linux `arm64` |
| containerd | 2.2.4 |
| runc | 1.3.5 |

### 2.3 Candidate images and container policy

| Candidate | Image identity |
| --- | --- |
| LayerFS | `sha256:a9f98b70333d9a254f2d113b0b1f7325f4d89388ac95dddb2837b205878175fb` |
| Computer | `sha256:24b11d3dd5d7c5a130722a969265e66a36df8f8c24ac210b7ab111750a31252c` |

Both candidates ran on the same Docker Desktop host with:

- Linux `arm64` containers.
- A real `/dev/fuse` device.
- `CAP_SYS_ADMIN`, required for the FUSE mount.
- A 512-process PID limit.
- No explicit per-container CPU or memory quota; both shared the same Docker
  Desktop allocation.
- No host bind mounts.
- Only a daemon port published to host `127.0.0.1`.
- Container creation, daemon readiness, and fixture copying completed before
  the measured interval.

The Computer runtime image was a thin derived image over the pinned upstream
product image. It contains the sealed shared workload helper, while the
Computer product files remain unpatched.

LayerFS used one fresh prepared daemon container per paired arm. Computer used
four separately prepared `computerd` containers so cold create, EDIT16,
prepend, and read could not share executor filesystem state. Container startup
and readiness were not timed for either candidate.

### 2.4 Store and acknowledgement profile

Both authority Stores ran natively on the macOS host and used the same SQLite
profile:

```text
journal_mode=MEMORY
synchronous=OFF
temp_store=MEMORY
cache_size=-32768
cache_spill=OFF
mmap_size=0
threads=0
no WAL checkpoint
no database fsync
no directory fsync
```

The acknowledgement boundary was:

> The transaction has committed and the resulting state is readable from the
> live local process.

This profile deliberately provides no crash- or power-loss-durability claim.
The benchmark does not compare WAL/FULL durability, checkpoint latency,
disaster recovery, or remote replication.

### 2.5 Workload and public operation boundary

Every measured command launched one fresh `/bin/sh -c` process. There was no
persistent execution shell and no prewarmed shell pool.

LayerFS used its public SDK lifecycle:

```text
Workspace Create
fresh-process Exec
Output completion
Workspace Commit and Store-visible Branch head
Workspace End
```

Computer used its public `Workspace.runtime.exec(sync="wait")` lifecycle,
including its normal push/FUSE/pull synchronization behavior.

The registered cases were:

- **Cold create 32 MiB:** create a new 32 MiB file from the sealed fixture.
- **EDIT16:** sixteen deterministic ten-byte overwrites distributed across a
  seeded 32 MiB file, with a Commit after every edit.
- **Prepend:** write a ten-byte prefix to a temporary file, copy the original
  32 MiB payload after it, and rename the temporary file over the original.
- **Read:** sequentially read the seeded 32 MiB file.

Each registered row used an isolated fresh authority Store. Correctness was
verified by size and SHA-256, followed by fresh-process authority reopening and
executor reopening. Pair order was deterministic but randomized between
LayerFS-first and Computer-first.

The measured LayerFS equation was:

```text
T0 = before Workspace Create
T1 = Create returns
T2 = fresh-process Exec and Output return
T3 = Commit returns and the Store-visible head is verified
T4 = Workspace End returns

complete_lifecycle = T4 - T0
```

The following work was intentionally outside the timer:

- Docker image construction.
- Container creation and readiness.
- Fixture generation and copying.
- Direct construction of each isolated seeded authority Store.
- Post-run correctness/reopen proofs.
- Report generation.

## 3. Analysis

### 3.1 The primary latency advantage is architectural

LayerFS 0.1.0 is organized around one local canonical Store:

```text
SDK → local daemon/FUSE Workspace → canonical Store → Commit
```

Cloudflare Computer retains an authority/executor synchronization boundary:

```text
SDK → authority SQLite → push/sync → computerd/FUSE
    → command → pull/sync → authority-visible result
```

Workspace state is committed directly into that Store. The released
architecture has no cross-store synchronization boundary, so its public SDK
lifecycle contains only local Workspace, FUSE, canonical-content, and Store
publication work.

The comparison has a precise scope: it establishes the public SDK latency of
both products for a prepared local container. Remote replication, multi-host
synchronization, and crash-durable acknowledgement require separate benchmark
profiles.

Computer Workspace creation is faster than LayerFS Workspace creation, but its
synchronized `runtime.exec` dominates the complete lifecycle. Even the read
case, which has no changed content to commit, spends approximately 779 ms in
Computer's synchronized execution versus approximately 101 ms in LayerFS's
fresh-shell/FUSE execution. This demonstrates that the speed gap is not solely
a deduplication result.

### 3.2 Cold-create speed comes from streaming capture and bounded admission

Cold create cannot benefit from preexisting payload deduplication. LayerFS
still finishes the entire lifecycle in 161.231 ms.

During the application write, LayerFS live-captures the sequential file and
constructs its canonical CDC/rope representation. Commit therefore admits an
already constructed candidate instead of rereading, rechunking, and
retransferring the complete file.

A representative cold-create receipt contains:

- One live-captured 32 MiB file.
- 1,747 candidate objects.
- 1,744 inserted objects.
- 33,661,702 inserted canonical bytes.
- 13 bounded admission transactions.
- At most 127 objects and approximately 2.5 MiB in one admission transaction.

The cold result therefore measures actual insertion of the new content. It is
not a zero-copy or already-present-file shortcut.

### 3.3 EDIT16 benefits from dirty-range tracking and local canonical reuse

LayerFS records mutated inodes and byte ranges while FUSE writes occur. Commit
can rebuild the affected extent/rope frontier and its ancestor nodes without a
full filesystem scan or a complete 32 MiB file rewrite.

Cloudflare Computer's pinned DOFS implementation uses fixed 512 KiB chunks. A
small overwrite replaces the affected fixed chunk; sixteen edits consequently
produce 8 MiB of new semantic payload in this workload.

LayerFS produces only 0.225 MiB of semantic growth for all sixteen edits. That
includes changed payload fragments, rope/extent nodes, inode/tree nodes, and
Commit metadata. This representation advantage supports both the 97.19%
storage reduction and the 15.80× public-lifecycle speedup.

### 3.4 Prepend exposes fixed-boundary versus content-defined chunking

Prepending bytes shifts every subsequent fixed byte offset. In a fixed 512 KiB
chunk representation, the new boundaries no longer align with the original
file, causing the full 32 MiB payload to be stored again.

LayerFS content-defined chunking resynchronizes with the old content shortly
after the inserted prefix. A representative LayerFS candidate contains:

- 1,747 total candidate objects.
- 1,730 reused objects.
- 17 inserted objects.
- 33,635,130 reused candidate bytes.
- 26,805 inserted canonical bytes.
- **99.9204% candidate-byte reuse.**

The 0.1.0 prepend workload physically copies the original 32 MiB file through
FUSE into a temporary file before rename. Range-reference or
`copy_file_range` behavior is outside this release benchmark, so the measured
232.394 ms includes the full userspace copy.

### 3.5 Sequential read benefits from bounded read planning and readahead

LayerFS builds a read plan, fetches canonical payloads in bounded batches, and
serves sequential FUSE reads through readahead. A representative 32 MiB receipt
records:

- 256 kernel read requests.
- 254 read-ahead hits.
- 2 read-ahead misses/fetches.
- 2 large host response frames.
- An `UpToDate` Commit requiring no candidate construction.

The read row is an end-to-end public-operation measurement, not a pure storage
engine microbenchmark. It includes fresh shell startup, Workspace lifecycle,
FUSE transport, and reading the 32 MiB payload.

### 3.6 Interpretation

The supported conclusion is:

> On this Apple M3 Max host, with prepared local containers, real FUSE, public
> SDK operations, fresh command processes, isolated Stores, and matched
> live-process SQLite acknowledgement, LayerFS 0.1.0 is 10.76× faster overall
> than pinned Cloudflare Computer. LayerFS uses 97.19% less semantic content for
> EDIT16 and 99.92% less for prepend, while its initial seeded Store is 17.81%
> larger.

The result should not be generalized to a different durability profile,
remote topology, newer Computer product commit, or range-reference workload
without rerunning the campaign.

## Evidence and reproduction

- [Final matched report](../../benchmark-results/fs-bench-pro/paired/fair-current-matched-7pair-rerun-20260901/report.md)
- [Deterministic pair schedule](../../benchmark-results/fs-bench-pro/paired/fair-current-matched-7pair-rerun-20260901/schedule.tsv)
- [Captured LayerFS working-tree patch](../../benchmark-results/fs-bench-pro/paired/fair-current-matched-7pair-rerun-20260901/environment/working-tree.patch)
- [Starting source seal](../../benchmark-results/fs-bench-pro/paired/fair-current-matched-7pair-rerun-20260901/environment/layerfs-source-seal.sha256)
- [Ending source seal](../../benchmark-results/fs-bench-pro/paired/fair-current-matched-7pair-rerun-20260901/environment/layerfs-ending-source-seal.sha256)
- [Representative LayerFS raw receipts](../../benchmark-results/fs-bench-pro/paired/fair-current-matched-7pair-rerun-20260901/pair-001/layerfs/raw.jsonl)
- [Representative Computer raw summary](../../benchmark-results/fs-bench-pro/paired/fair-current-matched-7pair-rerun-20260901/pair-001/computer/summary.json)
- [Benchmark harness and reproduction instructions](../../benchmark/fs-bench-pro/README.md)

The final audit verified:

- Seven complete pairs and seven LayerFS hard-gate passes.
- Identical fixture and workload identities.
- Matching SQLite acknowledgement profiles.
- Real FUSE mounts and zero host bind mounts.
- Fresh-shell execution.
- Isolated Store census for every registered row.
- Computer correctness and fresh-process reopen proofs.
- Matching starting and ending LayerFS source seals.
- Deterministic report regeneration.
- `git diff --check`.
