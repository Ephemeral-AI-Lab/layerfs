# LayerFS 0.1.1 namespace-v2 benchmark and optimization specification

> **Status:** Namespace-v2 is implemented and has retained provisional
> evidence, but initialization remains below its binding performance gate.
> The bounded direct-admission continuation is specified here and tracked by
> issue #11; no release candidate exists.
>
> **History:** Namespace-v1 used uniform 2,500-byte files. Its source-bound
> evidence remains immutable. Namespace-v2 keeps the same LayerFS-only family,
> scenario IDs, runner, public lifecycle, and registered-total exclusion while
> replacing only the active fixture profile and result-schema version.
>
> **Ownership:** GitHub issues
> [#7](https://github.com/Ephemeral-AI-Lab/layerfs/issues/7),
> [#9](https://github.com/Ephemeral-AI-Lab/layerfs/issues/9), and
> [#10](https://github.com/Ephemeral-AI-Lab/layerfs/issues/10), with the
> focused initialization pipeline in
> [#11](https://github.com/Ephemeral-AI-Lab/layerfs/issues/11). All are
> assigned to `@yifanxuaaa`.

Execution agents use the
[namespace-v2 handoff prompt](namespace-v2-handoff-prompt.md).

## Problem statement

The retained namespace-v1 candidate proves the complete public LayerFS
lifecycle, but its uniform 2,500-byte fixture does not cover a deliberate
small-heavy namespace with large files. Namespace-v2 now supplies that
fixture. Its 4.502-second / 111.1-MB/s 100,000-file result is the pre-direct,
temporary-object-segment baseline. The retained direct-admission implementation
has zero canonical-object-segment traffic; its historical source-sealed
one-sample init-only result is 3.040 seconds / 164.5 MB/s.

The predecessor boundary was measured: source and canonical preparation took
about 2.44 seconds, SQLite stepping and bounded commits about 1.54 seconds, and
final/root/other work about 0.53 seconds. It emitted about 1.132 million
canonical candidates for about 422,000 unique objects and wrote then reread
about 647 MB of temporary object segments. The direct path removes that
sequential object-segment boundary.

The earlier passing selected product evidence uses
`issue9-v3-final-create-100-r001-20260903` for the 100-file row and
`issue11-v3-terminal-all4-composite-r003-20260903` for the other tiers and
runner-owned composite proof. Selected initialization medians are
220.820/269.757/414.729/2,766.280 ms and
566.1/741.4/723.4/180.7 MB/s. Every selected tier-specific binding median
passes; the 100,000-file row remains below the preferred nonbinding
2.5-second/200-MB/s outcome. Both reports use full source seal
`f6a2c969ca9245b0394c91643d6c24a2f56180975fad537c10fb5360358d4170`.
The raw `r003` report retains its 15.223-ms 100-file Create miss and negative
aggregate performance/evidence markers; the supplemental same-seal 100-file
report records a passing 14.742-ms median. This selected view never rewrites
either raw report and is not a release claim.

A newer exact-seal LayerFS-only attempt,
`issue11-v3-layerfs-only-terminal-r001-20260903`, remains an immutable miss at
source seal
`7b211f30c7a0a8a2c74e0dbd39f4bfebf34c7bf44aa6bbe45c455f23672bcb89`.
Its subsequent-sample `namespace-100000` initialization median of 3.019
seconds / 165.6 MB/s / 33,121 files/s passes the authorized 10-percent
tolerance gates of 3.235294118 seconds / 153 MB/s / 30,600 files/s.
Correctness, resources, cleanup, and the composite proof also pass. Its
`namespace-100` Create median remains a binding miss at 17.144 milliseconds
against 15 milliseconds, so the campaign is not terminal and must not be
reconciled with the older source seal.

Initialization must therefore overlap the required source/canonical lane with
the existing SQLite lane through bounded move-only admission. It may not add
workers, retain namespace-sized state, hide work in setup, or change canonical,
Store, SDK, CLI, daemon, proxy, FUSE, or acknowledgement contracts.

## Goal

Admit one namespace-v2 profile through the existing family:

```text
namespace-100
namespace-1000
namespace-10000
namespace-100000
```

The profile must:

- contain more tiny files than small files and more small files than medium or
  large files;
- include an exact 100,000,000-byte large-file case at every tier and two at
  100,000 files;
- retain exactly 100 regular files per data directory;
- use deterministic unique path-derived content;
- prepare each fixture once, outside LayerFS timing, then reuse it across fresh
  product processes and Stores;
- preserve the existing real-FUSE lifecycle, ten-byte localized edit, exact
  reconnect oracle, runner interface, and phase equations;
- initialize the 100,000-file / 500-MB tier in at most 3.235294118 seconds and
  at least 153,000,000 logical bytes per second and 30,600 files per second,
  including the authorized 10-percent release tolerance;
- create its Workspace in at most 25 milliseconds; and
- use no new or increased product worker, background preloader, dependency,
  crate, benchmark family, or comparison product.

## Files to read

Read these completely before editing:

1. `docs/roadmap/0.1/0.1.1/README.md`
2. `docs/roadmap/0.1/0.1.1/baseline-2026-09-02.md`
3. `docs/roadmap/0.1/benchmarking.md`
4. `benchmark/fs-bench-pro/README.md`
5. `benchmark/fs-bench-pro/src/main.rs`
6. `benchmark/fs-bench-pro/run-namespace.sh`
7. `benchmark/fs-bench-pro/workload.rs`
8. `crates/layerfs-content/src/file/cdc/gear.rs`
9. `crates/layerfs-content/src/file/rope/build.rs`
10. `crates/layerfs-content/src/filesystem/apply.rs`
11. `crates/layerfs-content/src/filesystem/change.rs`
12. `crates/layerfs-content/src/tree/directory/edit.rs`
13. `crates/layerfs-layerstack-store/src/layerstack.rs`
14. `crates/layerfs-layerstack-store/src/objects.rs`
15. `crates/layerfs-layerstack-store/src/workspace.rs`
16. `crates/layerfs-workspace/src/cow_tree.rs`
17. `crates/layerfs-workspace/src/lifecycle.rs`
18. `crates/layerfs-workspace/src/projection.rs`
19. `crates/layerfs-sdk/tests/live_fuse.rs`
20. `crates/layerfs-sdk/tests/live_docker.rs`

Treat the worktree as shared and intentionally dirty. Do not reset, restore,
checkout, clean, stage, commit, push, or open a pull request without explicit
authorization.

## Family and history contract

Namespace-v2 is not another benchmark family. It keeps:

```text
runner: benchmark/fs-bench-pro/run-namespace.sh
selectors: namespace-100, namespace-1000, namespace-10000,
           namespace-100000, all
projection: real Linux FUSE
registered_total_ns contribution: none
external comparison contribution: none
```

Historical namespace-v1 evidence remains bound to
`fs-bench-pro-namespace-v1` and its uniform fixture digests. Namespace-v2 uses
a new result-schema and fixture-profile identity so no old result is silently
reinterpreted:

```text
schema: fs-bench-pro-namespace-v3
fixture_profile: synthetic-small-heavy-v2
fixture_digest_profile: namespace-file-digest-tree-v2
edit_contract: content-only-normalized-mtime-v1
```

The active implementation has replaced the v1 generator after retaining both
exact contracts through the bridge proof. No runtime selector, second runner,
or duplicate family is required merely to regenerate historical evidence.

## Scenario matrix

`MB` is decimal. Exact byte counts are authoritative.

| Scenario | Files | Data directories | Logical bytes | 100-MB anchors |
| --- | ---: | ---: | ---: | ---: |
| `namespace-100` | 100 | 1 | 125,000,000 | 1 |
| `namespace-1000` | 1,000 | 10 | 200,000,000 | 1 |
| `namespace-10000` | 10,000 | 100 | 300,000,000 | 1 |
| `namespace-100000` | 100,000 | 1,000 | 500,000,000 | 2 |

Every anchor contains exactly 100,000,000 bytes. Two anchors must occupy
different data directories. The localized ten-byte edit must target a
deterministic non-anchor file whose length is at least ten bytes.
The binding edit restores the normalized mtime so its digest contract remains
exact. After T7, a separate real-FUSE normal overwrite records whether an
ordinary content-changing write changes that fixed mtime; it is discarded
without Commit and its time is cleanup-only evidence.

## Count distribution

Anchors are allocated first. The remaining files use this fixed count mix:

```text
empty:  1 percent
tiny:  79 percent
small: 15 percent
medium: 5 percent
```

Hamilton largest-remainder allocation, tied in the class order above, gives:

| Scenario | Empty | Tiny | Small | Medium | Anchors | Total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `namespace-100` | 1 | 78 | 15 | 5 | 1 | 100 |
| `namespace-1000` | 10 | 789 | 150 | 50 | 1 | 1,000 |
| `namespace-10000` | 100 | 7,899 | 1,500 | 500 | 1 | 10,000 |
| `namespace-100000` | 1,000 | 78,998 | 15,000 | 5,000 | 2 | 100,000 |

This is an intentionally synthetic small-heavy distribution. Reports must not
claim that it exactly reproduces the LayerFS repository, its cache, directory
shape, extensions, links, permissions, or executable share.

## Exact size allocation

Each nonempty, non-anchor path receives one deterministic relative weight:

```text
tiny:   1 through 8
small:  32 through 256
medium: 1,024 through 8,192
```

Within each class, assign sorted role `r` of count `n` a deterministic
midpoint-quantile weight over inclusive bounds `[lower, upper]`:

```text
w(c, r) = lower + floor((2r + 1) * (upper - lower + 1) / (2n))
```

This covers each class range evenly without a random generator. Do not use a
live repository scan, random retries, or mutable external input.

For tier byte budget `B`, anchor bytes `A`, positive non-anchor count `P`, and
weight sum `W`:

```text
R = B - A - P
q_i = R * w_i
base_i = 1 + floor(q_i / W)
remainder_i = q_i mod W
```

Use checked `u128` multiplication. Assign the final
`R - sum(floor(q_i / W))` bytes to descending remainders, tied by relative
path. Empty files remain exactly zero.

The planner must prove before filesystem mutation:

```text
sum(class counts) + anchors = regular_files
sum(final sizes) = logical_bytes
all anchors = 100,000,000 bytes
all nonempty non-anchors >= 1 byte
all empty files = 0 bytes
files per data directory = 100
```

## Path and content generation

Keep the existing flat data-directory topology: one fixture root, the declared
number of data directories, and 100 files per directory. Deterministically
permute planned roles with one versioned SHA-256 sort key before round-robin
assignment so size classes are not clustered. Ties use class then role index.
Do not invent a repository-depth or extension distribution.

Generate unique content from a domain-separated stream keyed by scenario,
relative path, class, and final size. Stream large files through a reusable
buffer of at most 1 MiB; never allocate a `Vec` equal to file size. Anchors are
fully written bytes, not sparse files, hard links, reflinks, clones, or shared
payloads.

The fixture digest is a domain-separated tree over sorted records:

```text
relative path
file type
class
size
content digest
```

Hash content while writing and return only the compact per-file digest record.
The reopen verifier streams file bytes, computes the same per-file digests,
sorts compact records, and reconstructs the same fixture-root digest. It must
also reject missing and extra paths. A verifier lane may return only compact
metadata and a content digest, never the complete file bytes. Verify anchors
serially on the ordered hash path or through an equivalently bounded path so
out-of-order worker results cannot retain one or more 100-MB vectors.

## Efficient fixture preparation

Fixture preparation is excluded from LayerFS timing but remains measured
evidence. It must be `O(files + logical bytes)` with bounded memory:

1. Calculate and validate the complete compact size plan in memory.
2. Create each data directory once.
3. Open every regular file once.
4. Generate, write, and hash every byte once.
5. Perform no post-generation full content reread.
6. Perform no per-file `fsync` or durability barrier.
7. Publish from a unique `.partial` directory by atomic rename only after all
   count, byte, digest, and permission checks pass.
8. Generate each tier once and reuse its immutable fixture for all fresh
   process/Store samples.
9. Generate tiers sequentially in `all` mode to avoid cross-tier resource and
   cache interference.

Default fixture preparation is single-threaded. It may reuse bounded
standard-library setup concurrency only after setup itself is measured as a
blocker and only when its worker ceiling, aggregate buffer bytes, CPU, RSS, and
cache profile are reported separately. It is not a product optimization and
cannot justify a product throughput claim. No new or increased product worker
is allowed.

Required setup evidence:

```text
fixture_plan_ns
fixture_generate_ns
fixture_manifest_ns
fixture_files_per_second
fixture_bytes_per_second
fixture_peak_rss_bytes
fixture_worker_count
fixture_cache_profile
fixture_profile
fixture_digest
```

Fixture generation and its digest pass warm the source filesystem. Report
first-use and warm-cache states separately when both are measured; never pool
them into one median. Reuse of one sealed fixture across samples is a declared
warm/uncontrolled-cache CPU/object-path measurement, not cold-device ingest.

## Timed public lifecycle

The timed lifecycle and equations remain:

```text
T0: immediately before Client::initialize_layerstack
T1: initialization complete
T2: Branch fork complete
T3: real-FUSE Workspace Create complete
T4: fresh-process ten-byte edit complete
T5: Commit complete
T6: Workspace End complete
T7: fresh Store reconnect, real-FUSE Workspace, and exact verification complete
```

Required fields:

```text
layerstack_init_ns = T1 - T0
branch_fork_ns = T2 - T1
workspace_create_ns = T3 - T2
edit_ns = T4 - T3
commit_ns = T5 - T4
workspace_end_ns = T6 - T5
reopen_verify_ns = T7 - T6
complete_product_ns = T7 - T0
```

Store and Client construction, fixture work, container preparation, and report
generation remain excluded and reported as setup. No product work may move
outside `T0..T7` to satisfy a target.

## Metric contract

Retain every namespace-v1 resource and receipt field. Namespace-v2 adds or
requires:

```text
fixture_profile
fixture_digest_profile
empty_files
tiny_files
small_files
medium_files
anchor_files
anchor_bytes
process_baseline_rss_bytes
process_incremental_peak_rss_bytes
product_worker_count_before
product_worker_count_peak
explicit_buffer_peak_bytes
active_worker_output_peak_bytes
completed_result_peak_bytes
spool_pending_peak_bytes
object_id_window_peak_bytes
object_index_peak_bytes
inode_record_peak_bytes
inode_builder_peak_bytes
parent_merge_peak_bytes
admission_batch_peak_bytes
candidate_copy_bytes
canonical_encode_calls
canonical_hash_calls
canonical_bytes_created
spool_write_bytes
spool_read_bytes
spool_complete_scans
spool_segments
candidate_unique_objects
candidate_duplicate_objects
candidate_collision_checks
source_open_calls
source_metadata_calls
source_read_calls
source_read_bytes
single_chunk_files
streaming_files
cdc_scratch_peak_bytes
snapshot_database_calls
snapshot_database_rows
snapshot_database_bytes
snapshot_cache_rows_at_create
snapshot_cache_bytes_at_create
anchor_prefetch_count
maximum_fixture_write_buffer_bytes
maximum_verifier_buffer_bytes
maximum_product_read_ahead_bytes
phase_rchar
phase_wchar
phase_read_bytes
phase_write_bytes
```

`process_peak_rss_bytes` remains the OS whole-process peak. Incremental RSS is
the phase/process peak minus the baseline captured with Store and Client ready,
immediately before `T0`. An unavailable required metric is an evidence error,
never a silent zero. The optional process-global SQLite MEMSTATUS counters are
the documented exception when the system build disables them; DBSTATUS cache
target/use and native process resources remain required.

On macOS and Linux, capture current RSS plus the native lifetime high-water at
T0 and T1 without polling. `initialization incremental peak RSS` is the T1
lifetime high-water minus T0 current RSS only when T1 establishes a new
lifetime maximum; otherwise cumulative high-water data cannot isolate the
phase and the gate is unavailable. Do not substitute endpoint growth for a
peak. Take the SQLite T0 status first, then the T0 process snapshot immediately
before timing; at T1 take the process snapshot immediately after timing, then
SQLite status. Record the configured SQLite connection-cache target and
`SQLITE_DBSTATUS_CACHE_USED` at both boundaries so cache ownership is explicit;
never add or subtract CACHE_USED as resident bytes. The configured target must
not exceed 64 MiB. The retained setting remains 32 MiB because it is the best
measured performance point; 64 MiB is an allowance, not a target. Report
`SQLITE_STATUS_PAGECACHE_OVERFLOW` under that exact name. When the system SQLite
build has MEMSTATUS disabled, label global MEMORY_USED/MALLOC_COUNT counters
unavailable rather than interpreting zero as no allocation.

### Post-timing SQLite diagnostics

After the product timestamp, inspect the completed Store read-only through
SQLite `dbstat` and retain:

```text
sqlite_objects_table_pages
sqlite_objects_table_bytes
sqlite_objects_primary_key_index_pages
sqlite_objects_primary_key_index_bytes
sqlite_page_size_bytes
sqlite_page_count
sqlite_freelist_pages
sqlite_object_rows
sqlite_canonical_object_bytes
sqlite_store_to_canonical_ratio
sqlite_store_to_logical_ratio
```

These are diagnostic report values, not work inside `T0..T7`. Do not run
`ANALYZE`, `VACUUM`, mutate pragmas, or otherwise rewrite the evidence Store.
Bind every database-anatomy row to one exact Store state; initialization-only
and post-Commit Store sizes and row counts must not be mixed.

## Performance targets

### Initialization

| Scenario | Logical bytes | Init target | Minimum throughput | Minimum file rate |
| --- | ---: | ---: | ---: | ---: |
| `namespace-100` | 125 MB | <=416.667 ms | 300 MB/s | 240 files/s |
| `namespace-1000` | 200 MB | <=500 ms | 400 MB/s | 2,000 files/s |
| `namespace-10000` | 300 MB | <=750 ms | 400 MB/s | 13,334 files/s |
| `namespace-100000` | 500 MB | <=3.235294118 s | 153 MB/s | 30,600 files/s |

The retained smaller tiers already exceed the old flat 200-MB/s floor. Their
raised minima prevent a 100, 1,000, or 10,000-file regression from being hidden
behind the 100,000-file fix. Binding adjacent init-time ratios are at most
1.30x and 1.70x through the 10,000-file tier. The 100,000-file result is
independent and is evaluated only against its tolerance-adjusted
3.235294118-second, 153-MB/s, and 30,600-files/s gates. Never delay a faster
10,000-file result to
manufacture a ratio pass. This target applies prospectively; retained rows
keep the target identity in force when they were captured.

Preferred non-binding goals are:

| Scenario | Preferred init | Preferred throughput |
| --- | ---: | ---: |
| `namespace-100` | <=357.143 ms | >=350 MB/s |
| `namespace-1000` | <=400 ms | >=500 MB/s |
| `namespace-10000` | <=600 ms | >=500 MB/s |
| `namespace-100000` | <=2.500 s | >=200 MB/s |

The independent 100,000-file stretch goal remains <=2.000 seconds and >=250
MB/s. Preferred and stretch outcomes are reported separately from binding
status.

### Workspace Create and localized Commit

| Scenario | Create target | Commit target |
| --- | ---: | ---: |
| `namespace-100` | <=15 ms | <=10 ms |
| `namespace-1000` | <=18 ms | <=10 ms |
| `namespace-10000` | <=22 ms | <=10 ms |
| `namespace-100000` | <=25 ms | <=10 ms |

At 100,000 files, non-Attach Create work must be at most 10 milliseconds and
Create must perform zero Store-wide small-object scans. Commit must remain
localized to the touched non-anchor path.

### Stretch verification and complete-product targets

| Scenario | Reopen verification | Complete product |
| --- | ---: | ---: |
| `namespace-100` | <=0.60 s | <=1.30 s |
| `namespace-1000` | <=1.00 s | <=2.10 s |
| `namespace-10000` | <=1.80 s | <=3.40 s |
| `namespace-100000` | <=7.00 s | <=10.00 s |

These are stretch targets and do not authorize weakening exact verification.
Initialization and Create fixes are assessed independently from reopen.

## CPU, memory, and worker targets

Do not impose an artificial OS RSS limit during performance measurement.
Instead, enforce explicit ownership bounds and report whole-process and
incremental RSS separately.

```text
new product workers: 0
product worker ceiling increase: 0
initialization CPU: <=14.07 total CPU-seconds
modeled LayerFS named-buffer sum: <=10 MiB
configured SQLite connection-cache target: <=64 MiB (retained setting 32 MiB)
initialization whole-process incremental peak RSS: <=128 MiB
complete-lifecycle whole-process incremental peak RSS: <=256 MiB
swap: 0
OOM: false
```

The explicit peak is one aggregate process-wide equation, not a per-buffer or
per-worker limit:

```text
explicit_buffer_peak =
    active worker output
  + completed results
  + spool pending bytes
  + object-ID and exact-dedup windows
  + compact inode records and inode builder
  + parent/final-structure state
  + admission batch
  + reusable CDC/read scratch
```

Measure 6-, 8-, and 10-MiB explicit aggregate-buffer candidates at 10,000 and
100,000 files when spooling or backpressure is introduced. Select one budget
for every tier: the smallest candidate whose throughput is within 5 percent of
the fastest correct candidate and still meets that tier's binding throughput
gate. Do not retune per tier. Low memory may not be purchased with repeated spool scans,
smaller-than-needed transactions, higher CPU, or hidden background work.

## Initialization optimization specification

The retained direct candidate already contains deferred final namespace/inode
construction, parallel root-directory import, the proven-empty Store admission
fast path, bounded 8,191-object / less-than-4-MiB admission, and
initialization-only removal of the unused reference index. Do not reimplement
or claim those as new wins.

Direct admission is an eligible-shape optimization, not the universal
initialization path. It currently requires a proven-empty Store, a nonempty
source root containing only top-level directories, no detected hard link, and
the direct structural limits. Root-level regular files or symlinks, any hard
link, and a nonempty Store select the canonical final-state fallback. Reports
must state this boundary and must not extrapolate direct-path worker, memory,
spool, or throughput counters to the fallback.

The benchmark has at most 1,000 top-level data directories. The compact pair
stream also rejects more than 1,000 task blocks, and the current direct
selection does not yet prove an earlier preflight for that boundary. Before
release, either route more than 1,000 top-level tasks to the existing fallback
before admitting any object or prove the larger shape with focused coverage.

### Retained and rejected evidence

The retained warm/uncontrolled-cache 100,000-file result is:

```text
source/canonical preparation       about 2.44 s
SQLite row step                    about 1.15 s
SQLite bounded commits             about 0.38 s
final inode/root                   about 0.13 s
other admission/publication        about 0.40 s
complete initialization            about 4.50 s / 111.1 MB/s
whole-process CPU                  about 15.9 CPU-s
whole-process peak RSS             about 99 MiB
```

It performs about 100,000 source opens, 101,001 metadata observations, 210,687
source reads, 1.132 million canonical puts, 708,845 duplicate-candidate
comparisons, 423,200 SQLite row submissions, and 130 transactions. It writes
and rereads about 647 MB of object segments.

A zero-capacity direct-stream experiment removed object-segment I/O but did
not remove duplicate construction or smooth producer/admission scheduling.
Eight producers reached about 3.806 seconds / 131 MB/s; ten reached about
3.762 seconds / 133 MB/s while adding about one second of system CPU. At eight
producers the consumer was idle for about 2.07 seconds while producers
accumulated about 8.56 blocked seconds. That rendezvous implementation is
rejected; eight producers are the maximum retained candidate because ten buys
only about 1.2 percent wall time.

Exact eight-entry portable-metadata interning reduced canonical puts from
about 1.132 million to 439,000 and pending duplicates from 708,845 to 15,845,
with 99,000 exact hits and 2,000 misses. It reduced user CPU but was not a
standalone wall-time win while segment preparation and SQLite remained
sequential. It is retained only as a prerequisite of the corrected pipeline.

Do not repeat native direct-tiny, fitting-directory-leaf, filename-sort,
path-reuse, open/fstat, reusable-CDC, generic hot-ID, ten-or-more-producer,
temporary-SQLite-authority, giant-transaction, or multi-row-`RETURNING`
experiments. Their retained evidence is neutral, slower, more memory-hungry,
or incompatible with the resource goal.

### Cold bounded metadata interning

Every fresh process and Store begins with an empty initialization-local table.
Each producer retains at most eight exact entries:

```text
(InodeKind, normalized mode, mtime seconds, mtime nanoseconds)
  -> canonical metadata-root ObjectId
```

The first miss invokes the unchanged canonical builder and emits the complete
graph. A hit reuses only that deterministic root ID during the same
initialization. A difference in any key field is a miss. The table is destroyed
with the operation and is never persistent, shared across samples, or filled
during setup. This is bounded common-result interning inside a cold operation,
not warm-cache evidence.

An all-unique metadata case must remain bounded to eight entries per producer
and have no material regression. Do not add a complete object-ID set, full
namespace manifest, or generic namespace-sized cache.

### Coarse move-only direct admission

Use eight existing import producers and the calling thread as the sole SQLite
owner. Each producer fills an owned slab under both limits:

```text
payload bytes <= 256 KiB
objects <= 512
```

Move full slabs through one standard-library synchronous channel that holds at
most four slabs. Moving a `CanonicalObject` moves its payload `Vec`; no parent
payload copy or second canonical allocation is allowed. The caller carries one
exact-dedup admission batch across all producer, slab, task, and directory
boundaries under the existing limits:

```text
payload bytes < 4 MiB
objects <= 8,191
```

The path must record slab sends, queue occupancy and bytes, producer blocked
time, consumer idle time, active threads, context switches, and payload-copy
bytes. At 100,000 files, target at most 2,200 slab handoffs instead of the
rejected stream's roughly 6,891 handoffs.

Path-independent canonical content may be admitted while import continues.
Path-dependent inode and directory structure remains in the existing compact
structural stream until producer completion and confirmation that no hard link
was detected. Only then may the inode table, namespace root, Layer, and
LayerStack be completed and published. Object-only transactions may commit
before that point, but no Layer or LayerStack may become visible before the
final publication transaction. Do not describe the whole import as one atomic
SQLite transaction, add a second content walk, or expose a partial LayerStack.

### Exact authority and SQLite boundary

Deduplicate and byte-compare exact repeated IDs inside the current transaction
batch. Across batches and Stores, the existing `objects` primary key remains
the only global authority. Read and authenticate only actual conflict IDs in
bounded pages. Preserve the exact nonempty-Store fallback. Never add a
temporary database, full ID set, linear spill membership scan, second SQLite
owner, or database worker.

The retained 100,000-file Store is write-bound inside SQLite. Its rowid
`objects` table has a separate `sqlite_autoindex_objects_1` primary-key index.
For each unique object, VDBE executes a `NoConflict` primary-key probe,
`IdxInsert` into that index, and `Insert` into the table. Bounded transaction
commit writes dirty pages through the pager to `pwrite`; canonical payload
`pread` is not a sampled hotspot.

Actual cross-transaction conflict work is only about 82 calls, 1,148 rows, 97
KiB, and 9--10 milliseconds, versus about 1.15 seconds of SQLite row stepping
and 0.38 seconds of commit. Do not add an initialization object-read cache,
payload prefetch, collision-read cache, database reader worker, or larger
read-ahead unless new evidence makes object-payload reads a critical path.
A `WITHOUT ROWID` table could remove the separate primary-key index, but it is
a Store-schema change and is not authorized by v0.1.1 or issue #11.

The first direct-pipeline candidate retains the proved cached single-row
SQLite statement and approximately 130 bounded transactions. If and only if
the complete candidate lands between 2.5 and 2.75 seconds and SQLite row step
remains on the measured critical path, one fixed 128-row `INSERT ... ON
CONFLICT DO NOTHING` statement without `RETURNING` may be tested. Use one exact
remainder statement per transaction and perform bounded byte comparison only
for actual preexisting conflicts. Retain it only if SQL execution falls to at
most 0.8 seconds without increasing Store bytes, physical I/O, CPU, RSS, or
transaction size. This is not authorization for a generic bulk API.
When this conditional A/B is implemented, its new result-schema identity must
add `sqlite_object_insert_execute_calls`. The row count remains about 423,200;
the fixed-128 candidate must reduce insert executions to at most about 3,441.
Do not reinterpret historical namespace-v3 rows as having that field.

### Time and ownership budgets

For the 500-MB tier:

```text
source/canonical producer lane       <=2.10 s
SQLite insertion/commit lane         <=1.50 s
overlapped producer/admission window <=2.25 s
final inode/root/publication tail    <=0.25 s
complete initialization              <=2.50 s
```

The lanes overlap; they are not added. The target modeled named-buffer sum is
about 8.4 MiB: eight partial slabs about 2 MiB, four queued slabs about 1 MiB, one
admission batch below 4 MiB, aggregate CDC scratch about 0.5 MiB, compact pair
state about 0.25 MiB, and bounded headers/final state. Whole-process RSS is
reported separately; the existing SQLite page cache alone is 32 MiB, so the
10-MiB contract applies to this named-buffer equation, not total LayerFS heap
or total RSS. Deferred-tree and allocator-owned state is not comprehensively
captured by the named-buffer counter; native process high-water is the
authoritative aggregate memory measurement.
Never add or subtract CACHE_USED from RSS. The 128/256-MiB limits apply to the
native whole-process lifetime-HWM deltas; the SQLite target and explicit
LayerFS ownership remain separately binding.

Do not introduce physical object packing, a new schema, a packed fixture,
canonical inlining, or a bulk initialization API under this specification.
Those require a separate incompatible-contract decision.

Required ownership invariants:

```text
object segment writes = 0
object segment reads = 0
parent payload spool rewrite = 0
parent payload copy bytes = 0
slab payload <=256 KiB and slab objects <=512
queued slabs <=4
slab handoffs at 100,000 files <=2,200
spool linear membership rescans = 0
admission batch remains <4 MiB and <=8,191 objects
candidate unique objects = inserted objects + preexisting reused objects
```

Reports derive logical path movement without relabelling it as physical I/O:

```text
logical_path_movement_bytes =
    source_read_bytes
  + object_segment_write_bytes
  + object_segment_read_bytes
  + store_growth_bytes

logical_path_movement_ratio =
    logical_path_movement_bytes / logical_bytes
```

The pre-direct retained 100,000-file path was about 4.91x. The current direct
path reports its exact source-plus-Store value per row. These logical values
explain amplification but never replace the exact segment-zero and
parent-copy-zero gates or claim physical-I/O equivalence.

Duplicate objects within a segment, across segments, across a batch boundary,
and already durable must have exact count/byte receipts. Same-ID/different-byte
collisions fail before publication.

## Physical-I/O feasibility control

Before interpreting the 2.5-second target, retain first-use and warm controls
for:

```text
source sequential and many-file reads
temporary segment sequential write/read
SQLite 4-MiB batch admission
combined source -> segment -> SQLite path
```

A safe direct path still reads 500 MB of source and writes the unchanged
canonical rows and SQLite pages, but object-segment write/read bytes must be
zero. The small existing compact structural stream remains measured separately.
The control records effective aggregate I/O, CPU, page-cache state, and
`/proc/self/io` or the platform equivalent. A host ceiling does not authorize
extra product workers, tmpfs substitution, hidden setup work, or a false
153-MB/s binding claim; continue reporting the exact external bound.

## Workspace Create optimization specification

The proved root cause is the first SnapshotReader lookup calling an eager
Store-wide small-object cache scan before FUSE Attach. Direct pre-cache evidence
measured 100,000-file Create at about 13-16 milliseconds; the eager cache raises
it to about 234 milliseconds.

1. Record passive Create subphases and Store/cache rows and bytes without
   changing the public API.
2. Make the Snapshot cache demand-filled. A cache miss reads and caches only
   requested authenticated objects; it never starts `WHERE length(bytes)` or
   another Store-wide scan.
3. Bootstrap only the pinned snapshot row, namespace root, root inode-table
   path and record, root portable metadata, and spool/runtime state.
4. Reuse the decoded namespace root and batch root metadata values only if
   post-fix counters show those secondary reads are material.
5. Preserve Create evidence instead of erasing pre-Attach work during metric
   reset.
6. If exact reads regress, retain the Create fix and add bounded reachable
   per-directory prefetch inside the read phase. Prefetch only small reachable
   files, at most 128 IDs per Store batch, at most 512 KiB per directory, and
   at most 8 MiB per SnapshotReader. Stream anchors and count zero anchor
   payload IDs during enumeration.
7. Inspect and update a cache batch under one lock/read/insert sequence: gather
   misses while locked, release the cache lock before SQLite, then re-lock to
   insert. Never hold the cache mutex across Store I/O or clone/check it once
   per ID.
8. Keep the existing protocol maximum unchanged but cap actual large-file
   request/read-ahead buffers at 8 MiB or less. This specification authorizes
   no new FUSE, daemon, or proxy request/response tag.

The retained proxy cache uses at most four per-node two-MiB read-ahead entries
inside that eight-MiB aggregate ceiling and does not cache a response with no
unread tail. This replaces the measured single 16-MiB entry whose cross-file
eviction fetched 6.43 GB to serve a 300-MB exact verification. The retained
10,000-file row fetches and serves exactly 300 MB with zero unused bytes; the
100,000-file row fetches and serves exactly 500 MB with zero unused bytes.
Remaining exact-verifier time is a reported non-binding stretch miss, not
authorization for bulk protocol operations, prefetch, more workers, or a
weaker oracle.

## Experiment and retention protocol

For every hypothesis:

1. Write the hypothesis, expected counters, and one failing focused check.
2. Build one source variant with one changed variable.
3. Run focused canonical/resource checks.
4. Run three fresh-process samples at 100, 1,000, and 10,000 files.
5. Run 100,000 only after smaller tiers pass; the final claim requires three
   valid 100,000-file samples.
6. Retain exact successes, failures, CPU, RSS, workers, copies, spool traffic,
   SQLite calls/rows/transactions, Store growth, and canonical outcomes.
7. Retain a change only when it improves outside noise, no tier regresses more
   than 5 percent, CPU/memory/workers do not increase materially, and every
   correctness gate passes.
8. Revert a rejected isolated experiment, record why, and continue with the
   next ranked hypothesis. Do not retry away valid slow results.

The current required implementation order is:

```text
reconcile the retained source and evidence seal
-> restore exact cold initialization-local metadata interning
-> prove cached/uncached canonical equality and all-unique bounded behavior
-> add eight-producer, four-slab move-only direct admission
-> run one 10,000-file screen
-> run three fresh-process 100,000-file samples
-> test fixed-128 no-RETURNING SQL only if SQLite remains the critical lane
-> final four-tier proof
```

## Correctness and cleanup gates

- Canonical object bytes, IDs, roots, CDC profile, five-table Store schema, and
  public SDK/CLI/daemon/proxy/FUSE contracts remain unchanged.
- Every fixture passes exact file/class/directory/byte equations and digest
  determinism.
- Every successful sample reconnects to a fresh Store/Client, uses real FUSE,
  and rejects missing, extra, resized, or changed files.
- Fixture generation and exact verification stream anchors with at most 1 MiB
  scratch and retain no complete file `Vec`.
- Directory enumeration prefetches zero anchor payloads; actual product
  read-ahead is at most 8 MiB without changing the protocol maximum.
- Empty files, anchors, the localized edit, modes, mtimes, hard links,
  symlinks, nested/empty-directory fallbacks, and source growth/truncation
  safety have focused coverage where affected.
- Candidate, inserted, reused, transaction, copy, spool, CPU, memory, and
  worker evidence remains truthful.
- A runner-owned source-sealed composite receipt embeds the exact command,
  activation environment, exit status, output, and output digest for focused quality, large-spill/reconnect,
  materialization/FUSE equality, managed Docker, attachment failure, exact
  reconnect, and cleanup census. External self-authored proof manifests are
  rejected.
- No mount, container, process, output reader, spool, temporary candidate,
  Workspace, execution, or Branch lease leaks.
- A two-root parallel fixture with one 100-MB anchor forces a local spill and
  nonempty pending tail; its serial and parallel roots and complete canonical
  object maps are equal.
- Duplicate objects within/across segments and batches, empty/nonempty Stores,
  same-ID/same-byte reuse, and same-ID/different-byte collision have focused
  proofs and exact receipts.
- Source growth/truncation, segment flush, admission, final structure, Layer,
  and LayerStack publication failures leak no temporary segment or visible
  partial LayerStack.

Direct-admission failure atomicity is logical, not a claim that independently
committed SQLite pages vanish physically. Cleanup deletes every admitted
object and leaves zero object/Layer/LayerStack rows; SQLite may retain those
pages on its freelist for reuse by the next successful initialization. The
failure proof records Store bytes, page count, and freelist count before and
after cleanup. It does not run `VACUUM`, truncate the Store, or hide cleanup
I/O inside a performance result.

## Runnable checks

Focused and harness checks:

```bash
benchmark/fs-bench-pro/run-namespace.sh --self-check
bash -n benchmark/fs-bench-pro/run-namespace.sh
cargo test -p layerfs-content
cargo test -p layerfs-layerstack-store
cargo test -p layerfs-workspace
cargo test -p layerfs-sdk --test live_fuse
```

One tier and full matrix:

```bash
benchmark/fs-bench-pro/run-namespace.sh \
  RUN_ID CONTAINER_ID namespace-10000 3

benchmark/fs-bench-pro/run-namespace.sh \
  RUN_ID CONTAINER_ID all 4
```

Quality gates:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
tools/test-fast.sh
git diff --check
```

## Acceptance criteria

- [ ] Namespace-v2 uses the same four `namespace-*` IDs, `all`, runner, real
  FUSE lifecycle, and registered-total exclusion; no `repo-shape-*` family or
  selector exists.
- [ ] Exact tier file, directory, class, anchor, and logical-byte equations
  match this specification.
- [ ] Each fixture is generated and hashed in one content pass, prepared once,
  atomically sealed, and reused across fresh product samples.
- [ ] Generator/oracle scratch is at most 1 MiB, no complete file is retained,
  and both 100-MB anchors can be verified without multiplying memory by worker
  count.
- [ ] Historical namespace-v1 identities and evidence remain interpretable and
  are never relabeled as namespace-v2.
- [ ] With source metadata/content and the LayerStack-derived inode seed held
  equal, reference and optimized builders produce the same final reachable
  canonical root and bytes; independent public initializations are not
  claimed to share roots across distinct LayerStack identities.
- [ ] Prospectively, the 100,000-file init median is at most 3.235294118
  seconds, at least 153 MB/s, and at least 30,600 files/s under the authorized
  10-percent release tolerance; 200 MB/s / 2.5
  seconds remains preferred and 250 MB/s / 2.0 seconds remains stretch.
- [ ] Every fresh process and Store starts with an empty metadata intern table;
  no entry survives the initialization operation or comes from setup.
- [ ] Exact metadata interning is bounded to eight entries per producer and an
  all-unique metadata case does not materially regress.
- [ ] The admitted path uses at most eight existing producers, four queued
  256-KiB/512-object slabs, and the calling thread as sole SQLite owner.
- [ ] Direct-path reports state that the measured path requires a proven-empty
  Store, a nonempty all-directory source root, no hard link, and direct
  structural eligibility; fallback performance is never relabeled as direct.
- [ ] More than 1,000 top-level task blocks either select the canonical
  fallback before any direct admission or pass focused correctness, failure,
  cleanup, and resource coverage proving the larger direct shape.
- [ ] Object-segment write/read bytes, parent payload rewrites, and parent
  payload-copy bytes are zero; 100,000-file handoffs are at most 2,200.
- [ ] Read-only post-timing `dbstat` records table/index pages and bytes, rows,
  canonical bytes, page/free-list state, and amplification for one exact Store.
- [ ] No database-read cache, payload prefetch, reader worker, or larger
  read-ahead is added without new evidence that payload reads are material.
- [ ] A fixed-128 SQL experiment, if admitted, uses a new result schema,
  reports insert executions separately from submitted rows, and reduces the
  former to at most about 3,441 without a resource regression.
- [ ] Every tier meets its 300/400/400/153-MB/s throughput floor and absolute
  initialization target; adjacent ratios through 10,000 files are at most
  1.30x and 1.70x, while 100,000 files remains independent.
- [ ] The 100,000-file Create median is at most 25 milliseconds, non-Attach
  work at most 10 milliseconds, and Store-wide Create scans equal zero.
- [ ] Localized Commit remains at most 10 milliseconds with touched-path-only
  candidate objects.
- [ ] No new or increased product worker, thread pool, dependency, crate,
  background preloader, public operation, or comparison product is added.
- [ ] Initialization CPU is at most 14.07 CPU-seconds; the modeled named-buffer
  sum is at most 10 MiB; the configured SQLite target is at most 64 MiB and remains 32
  MiB absent measured need; initialization and complete-lifecycle incremental
  HWM are at most 128 and 256 MiB without an OS memory cap.
- [ ] CPU, copies, spool traffic, transactions, Store size, and read batching
  show no hidden resource trade.
- [ ] Create/cache Store I/O occurs outside cache locks; small-file prefetch is
  bounded and records zero anchor prefetches; actual read-ahead is at most
  8 MiB.
- [ ] The runner-owned composite receipt proves exact real-FUSE reconnect,
  materialization equality, Docker lifecycle, attachment-failure cleanup,
  focused tests, formatting, warning-denying
  Clippy, `tools/test-fast.sh`, `git diff --check`, and documentation links
  pass.

## Non-goals

No new benchmark family, exact repository distribution, live repository scan,
new directory/extension distribution, new product worker, raised worker
ceiling, persistent background service, larger cache as a latency fix, schema
migration, canonical inlining, physical object packing, packed fixture,
new FUSE/daemon/proxy request or response tag, materialization substitution,
external namespace comparison, prepend,
universal Workspace regular-file editing, sparse/mixed-edit work, release publication,
or silent reinterpretation of namespace-v1 evidence.
