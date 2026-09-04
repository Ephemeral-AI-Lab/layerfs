# Cloudflare Computer comparison benchmark

**Roadmap:** LayerFS v0.1.2
**Measured:** 2026-09-04
**Status:** Complete comparison report; Cloudflare evidence is informational and is not LayerFS release-admission evidence.

## Executive summary

This report compares the completed Cloudflare Computer real-FUSE campaign with
the LayerFS v0.1.2 SDK-edit campaign. The comparison uses the closest available
persisted timing boundary:

```text
Cloudflare: FUSE operation + host publication
LayerFS:   SDK edit + Commit
```

The systems use different mutation surfaces, so the ratios are comparative
system-path measurements, not a claim that one storage engine is intrinsically
that many times faster under identical internal APIs.

The measured result is nevertheless clear:

- LayerFS remains nearly file-size-independent for small localized edits.
- Cloudflare's current FUSE path becomes increasingly file-size-dependent.
- Cloudflare append, truncate, and zero-extension are the least expensive
  length-changing cases, but still grow with file size.
- Cloudflare middle insert/delete and especially prepend pay for materialized
  range movement and buffered publication.
- At 100 MiB, Cloudflare prepend is **5,827.6 ms** phase sum versus LayerFS
  **7.3 ms** edit plus Commit: approximately **803×** in this matched-tier
  comparison.
- At 200 MiB, Cloudflare prepend reaches **9,774.5 ms** phase sum and a maximum
  successful container cgroup lifetime peak of approximately **948 MiB**.

The complete Cloudflare evidence is in
[benchmark-results/fs-bench-pro/experiments/cloudflare-real-fuse-all4-20260904/REPORT.md](../../../../benchmark-results/fs-bench-pro/experiments/cloudflare-real-fuse-all4-20260904/REPORT.md).
The LayerFS comparator is in
[benchmark-results/fs-bench-pro/sdk-edit-terminal/final-3337728e/report.md](../../../../benchmark-results/fs-bench-pro/sdk-edit-terminal/final-3337728e/report.md).

## 1. Scope and claims

### 1.1 What was measured

The Cloudflare run measured all three requested performance cohorts:

1. `edit_length_preserving`
2. `edit_length_changing`
3. `edit_canonical_chunk_count` as a performance cohort only

Each cohort was exercised at 1, 10, 100, and 200 MiB with three repetitions
per operation:

```text
14 operations × 4 size tiers × 3 repetitions = 168 performance rows
```

The final run produced:

- 168/168 successful performance rows;
- 168/168 independent exact-byte verification proofs;
- 4/4 selected edited Stores restored into fresh real-FUSE containers and
  verified;
- no performance timeout, OOM, swap, or blocked row.

Preparation, cloning, fixture generation, hashing, verification, and cleanup
were excluded from the operation timers.

### 1.2 What is not claimed

This report does not claim:

- that Cloudflare's current FUSE path is its only possible implementation;
- that the third cohort has Cloudflare CDC-count semantics;
- that a local DOFS adapter is equivalent to deployed Cloudflare Durable Object
  storage or crash/power-loss durability;
- that the Cloudflare-to-LayerFS ratios are a controlled comparison of identical
  semantic APIs;
- that lifetime cgroup peaks are exact incremental edit allocations;
- that either system has a constant absolute time for arbitrary file sizes.

The third cohort retains the LayerFS labels `preserve`, `increase`, and
`decrease` only to align the performance matrix. Cloudflare uses fixed windows,
not LayerFS canonical CDC chunking, and this report makes no CDC-count claim.

## 2. Cloudflare benchmark setup

### 2.1 Environment

| Setting | Value |
| --- | --- |
| Host | macOS 26.4.1, ARM64 Apple Silicon |
| Docker | Docker Desktop 29.5.2 |
| Docker VM | Linux 6.12.76-linuxkit, aarch64, 4 vCPUs |
| Docker VM memory | 4,109,398,016 bytes |
| Base image | `rust:1.85.1-bookworm@sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4` |
| Container architecture | `linux/arm64` |
| FUSE | `/dev/fuse`, `CAP_SYS_ADMIN`, `apparmor=unconfined` |
| Mount | Explicit `FUSE_MOUNT=fuse`; kernel mountinfo verified `/workspace` as FUSE |
| Product runtime | Native ARM64 Cloudflare `computerd` plus `fuse-native` |
| Host Store | Local DOFS/WorkspaceFilesystem with public `pullOnce` |
| Cloudflare source | `de87919a4fd37242e960e13b7b3ba802d1eef0a0` |
| LayerFS comparison envelope | Same Docker Desktop/FUSE envelope; different product API |

The container was not privileged and had no extra CPU, memory, PID, bind, or
volume limit. The `/workspace` path was a real kernel FUSE mount, not a host
directory or an in-process mock.

Host SQLite settings were intentionally aligned with the LayerFS candidate
campaign:

```text
64 KiB pages
journal_mode=MEMORY
synchronous=OFF
temporary storage in memory
32 MiB cache
cache spill disabled
mmap disabled
SQLite threads=0
exclusive locking
```

These settings make the local publication path comparable to the recorded local
LayerFS profile. They do not represent Cloudflare's deployed Durable Object
durability configuration.

### 2.2 Fixture and run isolation

All fixtures were real, fully materialized binary files generated from the same
splitmix64 fixture profile used by the LayerFS campaign. Every row used a fresh
container and an independent writable clone of the pristine host Store. Cached
preparation data was reused only to avoid paying fixture-generation time on
every iteration; it was not used to precompute or shortcut a mutation.

The performance phase did not include:

- fixture construction;
- Store cloning;
- digest generation;
- post-run byte verification;
- Store restore verification;
- container cleanup.

### 2.3 Timing boundaries

Every Cloudflare row records three disjoint timing intervals, each reported as
median with observed minimum and maximum, N=3:

```text
FUSE operation
  native monotonic timer immediately before open
  through mutation, fsync, and close return

Host publication
  one public pullOnce call through return

Phase sum
  FUSE operation + host publication
```

Docker process dispatch and the retained workflow wall time were not substituted
for either operation metric. The phase sum is the closest user-visible
mutation-plus-persistence comparison to LayerFS `edit_commit_ns`, but it is not
an uninterrupted end-to-end clock.

### 2.4 Cloudflare operation implementation

The native ARM64 helper performed all work through `/workspace` on the real FUSE
mount:

- fixed-size overwrite: direct `pwrite`;
- append: `O_APPEND` plus write;
- truncate: `ftruncate`;
- zero extension: `ftruncate`/tail extension;
- insert, delete, prepend, grow, and shrink: a declared 1 MiB-scratch
  in-place range-shift helper that reads and writes through FUSE.

No temporary-file rewrite, shell reconstruction, text search, diff generation,
hidden fallback, or `copy_file_range` was used. The range-shift helper is a
benchmark workload operation, not an upstream Cloudflare semantic edit API.
Cloudflare's current FUSE surface does not expose a generic middle-range
insert/delete/prepend operation.

## 3. Cloudflare operation families

### 3.1 Family 1: `edit_length_preserving`

These operations replace exactly 4 KiB without changing logical file length:

```text
overwrite-head-4k
overwrite-middle-4k
overwrite-tail-4k
```

All values below are median `(minimum–maximum)` milliseconds, N=3. Each cell is
`FUSE / publication / phase sum`.

| Operation | 1 MiB | 10 MiB | 100 MiB | 200 MiB |
| --- | ---: | ---: | ---: | ---: |
| overwrite-head-4k | 4.392 (2.975–4.701) / 44.393 (41.957–57.402) / **48.785 (46.657–60.377)** | 8.678 (8.461–11.843) / 52.078 (47.771–61.606) / **60.756 (56.232–73.449)** | 85.780 (81.451–88.257) / 142.888 (133.028–144.308) / **225.759 (221.284–228.667)** | 144.742 (128.177–145.491) / 229.423 (224.616–262.815) / **374.914 (369.358–390.992)** |
| overwrite-middle-4k | 6.067 (4.780–12.401) / 90.611 (83.634–92.070) / **95.391 (89.701–104.471)** | 9.640 (9.568–10.009) / 89.073 (79.722–89.316) / **98.713 (89.290–99.325)** | 82.853 (81.105–104.870) / 170.809 (167.360–181.396) / **253.662 (248.465–286.266)** | 142.181 (134.132–144.337) / 269.200 (261.416–274.380) / **405.754 (403.332–416.562)** |
| overwrite-tail-4k | 3.735 (2.859–7.541) / 43.506 (41.257–46.106) / **46.364 (44.993–53.647)** | 9.295 (8.472–9.323) / 57.607 (51.696–58.099) / **66.571 (60.991–66.930)** | 80.511 (78.330–90.221) / 144.625 (141.381–150.688) / **231.199 (219.711–234.846)** | 130.638 (127.975–140.536) / 243.874 (221.219–250.219) / **374.512 (361.755–378.193)** |

Even with only a 4 KiB overwrite, the Cloudflare FUSE operation grows from
single-digit milliseconds at 1 MiB to roughly 80–145 ms at 100/200 MiB. That
indicates a file-size-dependent buffered-write path in addition to any range
shift cost.

### 3.2 Family 2: `edit_length_changing`

This family covers all requested positional and size-changing operations:

```text
insert-middle-4k
delete-middle-4k
append-tail-4k
prepend-head-4k
replace-grow-middle-2k-to-4k
replace-shrink-middle-4k-to-2k
truncate-tail-4k
zero-extend-tail-4k
```

| Operation | 1 MiB | 10 MiB | 100 MiB | 200 MiB |
| --- | ---: | ---: | ---: | ---: |
| insert-middle-4k | 9.740 / 53.084 / **63.756 (56.511–67.702)** | 43.600 / 304.308 / **349.092 (336.164–363.940)** | 268.561 / 2,772.330 / **3,040.892 (3,032.097–3,109.958)** | 430.627 / 5,521.092 / **5,958.433 (5,204.819–5,989.485)** |
| delete-middle-4k | 6.407 / 95.613 / **101.811 (85.362–114.920)** | 27.768 / 329.347 / **357.116 (339.445–369.409)** | 259.890 / 2,804.492 / **3,062.722 (3,016.570–3,129.605)** | 420.392 / 5,265.091 / **5,685.482 (5,027.023–5,963.658)** |
| append-tail-4k | 4.476 / 21.168 / **25.333 (17.851–25.644)** | 8.126 / 23.722 / **33.096 (28.798–43.631)** | 85.165 / 110.953 / **191.776 (181.381–196.722)** | 142.218 / 208.587 / **350.804 (336.380–370.883)** |
| prepend-head-4k | 12.165 / 87.945 / **105.171 (87.851–105.594)** | 45.726 / 549.647 / **595.145 (590.931–624.711)** | 392.549 / 5,435.082 / **5,827.631 (5,674.993–5,831.980)** | 570.930 / 9,050.102 / **9,774.453 (9,585.887–11,430.965)** |
| replace-grow-middle-2k-to-4k | 5.742 / 88.830 / **94.304 (85.253–99.255)** | 37.296 / 335.611 / **369.870 (355.718–393.324)** | 272.716 / 2,788.414 / **3,037.769 (3,001.932–3,109.517)** | 464.825 / 4,796.444 / **5,245.146 (5,150.024–6,007.918)** |
| replace-shrink-middle-4k-to-2k | 9.472 / 85.070 / **95.841 (90.657–103.201)** | 31.031 / 339.152 / **370.183 (362.151–379.755)** | 274.965 / 2,762.116 / **3,035.953 (2,997.428–3,129.064)** | 377.980 / 4,705.526 / **5,081.391 (5,079.268–5,782.973)** |
| truncate-tail-4k | 5.734 / 40.875 / **46.610 (46.303–64.716)** | 8.964 / 43.425 / **52.420 (52.265–74.436)** | 86.867 / 143.654 / **230.521 (225.039–243.506)** | 140.450 / 250.196 / **388.857 (387.442–390.646)** |
| zero-extend-tail-4k | 5.795 / 13.833 / **19.628 (16.687–28.033)** | 9.869 / 25.379 / **35.248 (29.185–47.362)** | 83.997 / 111.900 / **195.897 (187.710–208.928)** | 136.967 / 209.179 / **346.146 (334.368–358.395)** |

Append, truncate, and zero extension avoid moving an existing suffix and remain
the least expensive Cloudflare length-changing operations. Middle insert/delete
and replacement operations become multi-second at 100/200 MiB. Prepend is the
worst case because it shifts the entire existing file while also exercising the
buffered publication path.

### 3.3 Family 3: `edit_canonical_chunk_count` performance cohort

This family is retained for matrix alignment only. It does not claim that
Cloudflare preserves, increases, or decreases LayerFS CDC chunk counts.

| Operation | 1 MiB | 10 MiB | 100 MiB | 200 MiB |
| --- | ---: | ---: | ---: | ---: |
| overwrite-fixed-64k-chunk-count-preserve | 3.511 / 51.915 / **55.971 (49.252–72.870)** | 10.515 / 53.832 / **61.864 (59.974–76.163)** | 87.407 / 140.230 / **229.651 (225.708–236.362)** | 140.597 / 229.350 / **369.947 (364.562–391.406)** |
| overwrite-fixed-64k-chunk-count-increase | 3.084 / 52.370 / **56.253 (48.628–57.118)** | 9.260 / 51.474 / **60.734 (57.985–62.065)** | 89.305 / 131.082 / **222.463 (218.781–242.198)** | 152.575 / 239.768 / **385.629 (377.576–392.344)** |
| overwrite-fixed-64k-chunk-count-decrease | 3.285 / 47.799 / **52.416 (46.825–53.773)** | 8.827 / 57.153 / **65.979 (60.669–76.319)** | 92.607 / 147.129 / **239.736 (213.413–240.478)** | 133.806 / 241.644 / **372.493 (371.905–378.091)** |

The third cohort follows the same file-size trend as the other Cloudflare
overwrite cases.

## 4. LayerFS comparison data

The LayerFS comparator is the final candidate arm at revision
`3337728e9846a200d7a5cc08d076de18f1d5436c`:

- three SDK-only families;
- 56 registered operations;
- 1, 10, 100, and 500 MiB siblings;
- five repetitions per candidate row;
- 280 candidate performance rows;
- 56 aggregate verifier receipts and 112 source-arm subproofs.

LayerFS reports separate `edit_call_ns`, `commit_call_ns`, and
`edit_commit_ns`. For comparison with Cloudflare, this report uses
`edit_commit_ns` (SDK edit plus Commit) against the Cloudflare phase sum.
Only 1/10/100 MiB are matched exactly; LayerFS's 500 MiB tier is included as a
directional endpoint because the final Cloudflare matrix stops at 200 MiB.

### 4.1 Matched persisted comparison

Each cell is `Cloudflare phase sum / LayerFS edit+Commit = Cloudflare ratio`.
Values are medians in milliseconds.

#### Same-count edits

| Operation | 1 MiB | 10 MiB | 100 MiB |
| --- | ---: | ---: | ---: |
| overwrite-head-4k | 48.785 / 5.402 = **9.0×** | 60.756 / 5.985 = **10.2×** | 225.759 / 6.928 = **32.6×** |
| overwrite-middle-4k | 95.391 / 5.409 = **17.6×** | 98.713 / 5.115 = **19.3×** | 253.662 / 8.238 = **30.8×** |
| overwrite-tail-4k | 46.364 / 3.770 = **12.3×** | 66.571 / 4.341 = **15.3×** | 231.199 / 6.899 = **33.5×** |

#### Length-changing edits

| Operation | 1 MiB | 10 MiB | 100 MiB |
| --- | ---: | ---: | ---: |
| insert-middle-4k | 63.756 / 4.842 = **13.2×** | 349.092 / 5.292 = **66.0×** | 3,040.892 / 7.752 = **392.3×** |
| delete-middle-4k | 101.811 / 10.283 = **9.9×** | 357.116 / 5.643 = **63.3×** | 3,062.722 / 9.191 = **333.2×** |
| append-tail-4k | 25.333 / 6.003 = **4.2×** | 33.096 / 6.515 = **5.1×** | 191.776 / 7.569 = **25.3×** |
| prepend-head-4k | 105.171 / 4.680 = **22.5×** | 595.145 / 4.883 = **121.9×** | 5,827.631 / 7.257 = **803.0×** |
| replace-grow-middle-2k-to-4k | 94.304 / 4.200 = **22.5×** | 369.870 / 7.610 = **48.6×** | 3,037.769 / 9.015 = **337.0×** |
| replace-shrink-middle-4k-to-2k | 95.841 / 3.930 = **24.4×** | 370.183 / 5.485 = **67.5×** | 3,035.953 / 7.893 = **384.6×** |
| truncate-tail-4k | 46.610 / 4.994 = **9.3×** | 52.420 / 4.246 = **12.3×** | 230.521 / 5.837 = **39.5×** |
| zero-extend-tail-4k | 19.628 / 5.767 = **3.4×** | 35.248 / 7.100 = **5.0×** | 195.897 / 7.115 = **27.5×** |

#### Fixed-64 KiB performance cohort

| Operation | 1 MiB | 10 MiB | 100 MiB |
| --- | ---: | ---: | ---: |
| preserve | 55.971 / 6.715 = **8.3×** | 61.864 / 6.198 = **10.0×** | 229.651 / 8.060 = **28.5×** |
| increase | 56.253 / 5.069 = **11.1×** | 60.734 / 6.272 = **9.7×** | 222.463 / 6.932 = **32.1×** |
| decrease | 52.416 / 5.040 = **10.4×** | 65.979 / 5.141 = **12.8×** | 239.736 / 10.433 = **23.0×** |

### 4.2 Direct mutation versus publication

The phase-sum comparison includes persistence. The direct mutation boundary
also shows the architectural difference:

| Case | Cloudflare FUSE operation | LayerFS SDK edit | Cloudflare direct-operation ratio |
| --- | ---: | ---: | ---: |
| 100 MiB overwrite-head-4k | 85.8 ms | 2.7 ms | approximately **31×** |
| 100 MiB overwrite-middle-4k | 82.9 ms | 2.9 ms | approximately **29×** |
| 100 MiB overwrite-tail-4k | 80.5 ms | 2.0 ms | approximately **41×** |
| 100 MiB prepend-head-4k | 392.5 ms | 3.0 ms | approximately **132×** |

This is not solely a storage-write comparison: Cloudflare's timer includes
FUSE open, mutation, `fsync`, and close, while LayerFS's timer is a semantic
SDK range-edit call. The gap combines transport/API overhead with the much more
important difference in how the edit is represented.

### 4.3 Non-matched 200 MiB versus 500 MiB endpoint

The final Cloudflare matrix includes 200 MiB; the LayerFS campaign includes
500 MiB. This is not a formal same-tier ratio, but it demonstrates the
direction:

```text
Cloudflare 200 MiB prepend phase sum: 9,774.5 ms
LayerFS    500 MiB prepend edit+Commit:   14.3 ms
```

The numerical quotient is approximately 684×, but it must not be presented as
a same-size benchmark ratio.

## 5. Architecture breakdown

### 5.1 Two different mutation pipelines

```text
LayerFS semantic edit path

Client::edit_workspace_file_range
            │
            ▼
   persistent PieceTree
   ┌───────────────────────────┐
   │ Base(original range refs) │
   │ Inline(new bytes)          │
   │ Zero(logical zeros)        │
   │ Spool(immutable byte span) │
   └───────────────────────────┘
            │
            ▼
   localized candidate builder
            │
            ▼
   FileMutationBatch
   (changed payload + affected mapping nodes)
            │
            ▼
   new authenticated root reusing old objects
```

```text
Cloudflare current real-FUSE path

native workload helper
            │
            ▼
   kernel FUSE /workspace
            │
            ▼
   Cloudflare FUSE adapter
   hydrate existing buffered file
            │
            ▼
   direct write or bounded range shift
   (prepend/insert/delete moves existing suffix bytes)
            │
            ▼
   fsync + close
            │
            ▼
   host pullOnce publication
   synchronize/persist buffered file state
```

### 5.2 Prepend representation

For a 100 MiB file and a 4 KiB prepend, the logical operation is:

```text
LayerFS:

  [Inline: 4 KiB] [Base: original 100 MiB]
   └── new piece ──┘ └──── unchanged reference ────┘

  No 100 MiB suffix copy is required.

Cloudflare FUSE helper:

  [4 KiB new bytes] [old byte 0 ... old byte 100 MiB]
                         ▲
                         └── every old byte must move right

  The helper bounds its scratch buffer, but the data movement remains
  proportional to the shifted suffix. The buffered writer/publication path
  additionally processes the existing file representation.
```

### 5.3 Why LayerFS edit time is small

`WorkspaceFileRangeEdit` updates the live edit representation. It does not need
to read the entire old file to prepend, insert, delete, or replace a small
range. A balanced implicit treap stores piece lengths and subtree aggregates;
split/merge changes only the paths around the edit.

The 100 MiB LayerFS prepend row is concrete evidence of this behavior:

```text
edit median                 2.967 ms
commit median               4.289 ms
combined median             7.257 ms
commit CDC bytes scanned    4,096 bytes
candidate object bytes      12,018 bytes
final piece count           2
```

The old payload remains addressable by its original authenticated object
references. Replacement bytes are the only new file payload in this case.

### 5.4 Why Cloudflare edit time grows

Cloudflare's FUSE operation is a byte-stream mutation, not a semantic range
edit. For a prepend or middle insert, the existing suffix must be shifted. For
an overwrite that does not shift a suffix, the current buffered FUSE writer
still hydrates existing chunks on first modification. Therefore even a fixed
4 KiB overwrite exhibits a file-size-dependent operation time.

The FUSE transport adds additional fixed work that LayerFS's semantic SDK
boundary does not have:

- kernel-to-daemon FUSE request handling;
- file open and close;
- explicit `fsync`;
- buffered writer state transitions;
- range-shift read/write requests for positional edits.

### 5.5 Why LayerFS Commit is small

For a single localized file edit, LayerFS:

1. identifies the changed inode;
2. retains monotonic base spans;
3. streams only maximal non-base replacement runs into
   `FileMutationBatch`;
4. creates affected rope/mapping nodes;
5. upserts the changed inode record;
6. publishes a new root that still references all untouched objects.

The commit is therefore a small structural publication, not a new full-file
serialization. CDC work is restricted to the replacement and the necessary
local boundary context. The resulting commit time is not mathematically
constant for every possible workload, but it is independent of the full file
payload for the measured single-edit cases.

### 5.6 Why Cloudflare publication dominates

Cloudflare's `host_publication_ns` is a synchronization/publication interval,
not an isolated disk `fsync`. The current pinned DOFS writer hydrates existing
file chunks into a full-file buffer when a file is first modified. The
publication path then has to process and synchronize that buffered state.

For prepend:

| Tier | FUSE operation | Host publication | Publication share of phase sum |
| ---: | ---: | ---: | ---: |
| 1 MiB | 12.2 ms | 87.9 ms | approximately 84% |
| 10 MiB | 45.7 ms | 549.6 ms | approximately 92% |
| 100 MiB | 392.5 ms | 5,435.1 ms | approximately 93% |
| 200 MiB | 570.9 ms | 9,050.1 ms | approximately 93% |

This is why the publication comparison is much more dramatic than the direct
FUSE comparison. The small logical edit has been amplified into a file-sized
buffered publication.

## 6. Complexity and space analysis

Let:

```text
N       original logical file size
Δ       replacement bytes supplied by the caller
P       live LayerFS piece count
C       original canonical extent count
R       number of changed replacement runs
S       scratch-buffer size
```

### 6.1 LayerFS localized edit

For the universal edit engine, use `E` for the number of lowered edits, `Pj`
for the piece count before edit `j`, `P` for the final normalized piece count,
`Fj` for pieces synchronously split/released/visited by edit `j`, `B` for
supplied replacement bytes, `Bi` for live inline bytes, `S` for physical spool
bytes, `R` for maximal replacement runs at Commit, and `C` for the base
canonical extent count.

| Stage | Time | Additional working space | Explanation |
| --- | --- | --- | --- |
| One SDK edit `j` | `O(path depth + log Pj + Fj + Bj)` | `O(piece allocation + Bi)` | Persistent piece-tree split/merge plus the supplied replacement. |
| All live edits | `O(E log E + ΣFj + total supplied bytes)` when `Pj = O(E)` | bounded by piece/spool policy | Repeated edits are normalized without a full-file byte copy. |
| Normalization | `O(P)` | `O(P)` metadata | Final piece sequence is visited once. |
| Content Commit | `O(P + R log C + final Inline + final Spool + logical Zero + prune + candidate-reachability work)` | bounded deferred object overlay | Only replacement runs and affected canonical boundaries are materialized. |
| Live read | `O(log P + V + Q + canonical Base-run traversal)` | caller-sized output | Reads traverse references and return only requested bytes. |

For an owner-side prepend, `P=2`, `R=1`, and `Bi=Δ`; the content work is
approximately `O(log C + Δ)` with zero old-payload reads. The implementation
enforces maximum edits, piece count, inline bytes, spool bytes, logical zeros,
and deferred object-overlay limits, so the localized benchmark path has no
allocation proportional to the full file size merely because the file is large.
Physical spool usage is `Theta(S)` for bytes supplied since cleanup. Full
materialization is `Theta(projected entries + projected bytes)` only for a
projection or verification consumer; it is not part of the localized Commit
algorithm.

### 6.2 Cloudflare current FUSE path

| Stage | Time | Working space | Explanation |
| --- | --- | --- | --- |
| Existing-file hydration | approximately `O(N)` in the current buffered path | approximately `O(N)` product buffer | Existing chunks are hydrated before the first modifying write. |
| Overwrite | approximately `O(N)` current path plus FUSE overhead | approximately `O(N)` buffered state | The logical delta is small, but the buffered file representation is not. |
| Insert/delete/prepend range shift | approximately `O(N - offset + Δ)` | helper scratch `O(S)` plus product buffer | The helper bounds scratch memory, but it still moves the suffix. |
| Host publication | file-size-dependent; observed strongly increasing with `N` | file-size-dependent | `pullOnce` synchronizes/persists the buffered file state. |

The 1 MiB scratch helper prevents the benchmark helper itself from allocating a
100 MiB temporary array, but it does not make the operation file-size-agnostic.
The product writer's buffered state remains the dominant memory and publication
concern.

### 6.3 `copy_file_range` is not the explanation

Neither the current LayerFS SDK path nor the Cloudflare real-FUSE run used
`copy_file_range`. LayerFS's improvement is not a faster copy syscall; it avoids
copying unchanged bytes. Replacing the Cloudflare helper's read/write loop with
`copy_file_range` could reduce some transport or CPU overhead, but a prepend
would still move a suffix proportional to `N` and would not provide the LayerFS
structural invariant.

## 7. RSS and memory issue

### 7.1 Cloudflare observations

The Cloudflare supervisor recorded container cgroup lifetime peaks. These are
conservative whole-container lifetime values including setup and fixture
population, not exact incremental edit-phase allocations:

| File tier | Maximum successful Cloudflare cgroup lifetime peak |
| ---: | ---: |
| 1 MiB | 43.063 MiB |
| 10 MiB | 105.883 MiB |
| 100 MiB | 535.258 MiB |
| 200 MiB | 948.453 MiB |

No successful row observed swap, OOM, or container termination. That does not
make the footprint acceptable for arbitrary file sizes. Extrapolating the
observed pattern to very large files would create an unacceptable risk of
memory amplification.

The source-level explanation is the current buffered FUSE/DOFS write path:
existing chunks are hydrated into a file-sized buffer on first modification.
The relevant pinned source is documented in the Cloudflare report:

- [Cloudflare FUSE adapter](https://github.com/cloudflare/computer/blob/de87919a4fd37242e960e13b7b3ba802d1eef0a0/packages/computerd/src/fuse/driver.ts)
- [Cloudflare DOFS write-buffer implementation](https://github.com/cloudflare/computer/blob/de87919a4fd37242e960e13b7b3ba802d1eef0a0/packages/dofs/src/fs/writeFile.ts)

### 7.2 LayerFS observations

LayerFS candidate native worker lifetime RSS remained approximately 7–10 MiB
across the 1–500 MiB SDK campaign for these single-edit cases. For example,
the 100 MiB prepend candidate workers had lifetime RSS peaks around 7.6–8.0
MiB, while the candidate's changed payload and object overlay remained small.

The measurement scopes are not identical: Cloudflare's headline values are
container cgroup lifetime peaks, while LayerFS's report retains native worker
RSS and separate cgroup observations. Therefore this report does not claim a
formal memory ratio. It does conclude that LayerFS's reference-preserving
edit/commit architecture avoids an allocation proportional to the file payload
in the measured path, whereas Cloudflare's current buffered FUSE path exhibits
a strongly increasing lifetime footprint.

### 7.3 Required follow-up for a Cloudflare-equivalent design

To remove the RSS and publication amplification, a Cloudflare-compatible
implementation would need to represent a file as persistent ranges, extents, or
chunks and publish only changed ranges and mapping nodes. A bounded scratch
buffer alone is insufficient: it limits the helper's temporary allocation but
does not eliminate file-sized hydration or file-sized publication.

## 8. Correctness and evidence handling

The Cloudflare performance and verification phases were intentionally separate:

```text
performance rows
    → all 168 rows complete
    → independent byte verification of all 168 rows
    → restore verification for one Store per tier
```

No digest, verification scan, or Store restore entered a performance timer.
Every successful row used exact expected-byte equations and an independent
post-run oracle. The one failed parser-only development attempt and the earlier
500 MiB FUSE-driver-limit pilot are retained separately and are not pooled into
the conclusive 168-row matrix.

LayerFS's comparator likewise comes from the final SDK-only campaign and not
from the superseded POSIX/temp-copy families. This matters because the old
POSIX families mixed operation mechanisms and could not support a clean
semantic comparison.

## 9. Final conclusions

1. **LayerFS is faster before Commit because it performs a semantic range edit.**
   It updates a small piece-tree structure and stores the new bytes, while
   Cloudflare's current FUSE path must express a positional edit as byte-stream
   reads/writes and pays FUSE and buffered-writer overhead.

2. **LayerFS wins decisively at Commit because it publishes references.**
   Unchanged payload objects are reused; only changed payload and structural
   nodes are written. Cloudflare publication processes a buffered file-sized
   state, so a 4 KiB logical mutation can become a multi-second publication.

3. **Prepend is the strongest separator.** At 100 MiB the matched persisted
   ratio is approximately 803×; at 200 MiB Cloudflare's phase sum is 9.77 s.

4. **The Cloudflare result is not a correctness artifact.** The complete matrix
   passed independent exact-byte verification and selected Store restore
   verification. The difference is explained by the mutation and persistence
   representations, not by skipping verification or using `copy_file_range`.

5. **The RSS trend is a real design concern even though the recorded peaks are
   lifetime bounds.** Cloudflare's 100/200 MiB tiers reached 535/948 MiB
   cgroup lifetime peaks; LayerFS's measured localized path remained in the
   single-digit-to-low-teens MiB worker range.

6. **The universal LayerFS edit invariant is structural, not OS-specific.** It
   does not rely on `copy_file_range`, reflink, clone, `fallocate`, or another
   driver primitive. The same semantic representation can be lowered by
   different projection adapters while preserving the size-independent edit
   behavior.

## 10. Reproducibility and evidence

Cloudflare evidence directory:

```text
benchmark-results/fs-bench-pro/experiments/cloudflare-real-fuse-all4-20260904/
```

It contains the plan, runner, native helper, Dockerfile, environment receipt,
performance JSONL, verification JSONL, restore verification, setup/cleanup
receipts, container logs, summary, and SHA-256 manifest.

LayerFS comparator directory:

```text
benchmark-results/fs-bench-pro/sdk-edit-terminal/final-3337728e/
```

The Cloudflare report's evidence was sealed with `SHA256SUMS`; the LayerFS
campaign retains its own source, harness, and evidence manifests. The two
campaigns should be read together with the API-boundary caveat in Sections 1,
4, and 5: this is a comparison of complete measured paths, not a claim that
the two products exposed the same internal operation.
