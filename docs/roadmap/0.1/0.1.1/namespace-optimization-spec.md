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
fixture. Its current warm/uncontrolled-cache 100,000-file median initializes
500 MB in 4.502 seconds, or 111.1 decimal MB/s; Workspace Create is about
15.4 milliseconds and localized Commit about 3.7 milliseconds.

The remaining initialization boundary is measured. Source and canonical
preparation takes about 2.44 seconds, SQLite stepping and bounded commits about
1.54 seconds, and final/root/other work about 0.53 seconds. The current path
also emits about 1.132 million canonical candidates for about 422,000 unique
objects and writes then rereads about 647 MB of temporary object segments.
Sequential execution cannot meet the 2.5-second target.

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
- initialize the 100,000-file / 500-MB tier in at most 2.5 seconds and at least
  200,000,000 logical bytes per second;
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
paired comparison contribution: none
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
immediately before `T0`. An unavailable metric is an evidence error, never a
silent zero.

## Performance targets

### Initialization

| Scenario | Logical bytes | Init target | Minimum throughput | Minimum file rate |
| --- | ---: | ---: | ---: | ---: |
| `namespace-100` | 125 MB | <=625 ms | 200 MB/s | 160 files/s |
| `namespace-1000` | 200 MB | <=1.000 s | 200 MB/s | 1,000 files/s |
| `namespace-10000` | 300 MB | <=1.500 s | 200 MB/s | 6,667 files/s |
| `namespace-100000` | 500 MB | <=2.500 s | 200 MB/s | 40,000 files/s |

Preferred adjacent init-time ratios follow byte growth: 1.60x, 1.50x, and
1.67x. No adjacent ratio may exceed 2.0x in the final candidate.

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
explicit LayerFS-owned buffers: <=10 MiB
preferred incremental peak RSS: <=16 MiB
hard incremental peak RSS: <=32 MiB
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
the fastest correct candidate and still meets the binding 200-MB/s gate. Do
not retune per tier. Low memory may not be purchased with repeated spool scans,
smaller-than-needed transactions, higher CPU, or hidden background work.

## Initialization optimization specification

The current candidate already contains final namespace/inode construction,
parallel root-directory import, the proven-empty Store admission fast path,
bounded 8,191-object / 4-MiB admission, and initialization-only removal of the
unused reference index. Do not reimplement or claim those as new wins.

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
structural stream until global cross-root hard-link resolution completes.
Only then may the inode table, namespace root, Layer, and LayerStack be
constructed and published. Do not add a second content walk or expose a
partial LayerStack.

### Exact authority and SQLite boundary

Deduplicate and byte-compare exact repeated IDs inside the current transaction
batch. Across batches and Stores, the existing `objects` primary key remains
the only global authority. Read and authenticate only actual conflict IDs in
bounded pages. Preserve the exact nonempty-Store fallback. Never add a
temporary database, full ID set, linear spill membership scan, second SQLite
owner, or database worker.

The first direct-pipeline candidate retains the proved cached single-row
SQLite statement and approximately 130 bounded transactions. If and only if
the complete candidate lands between 2.5 and 2.75 seconds and SQLite row step
remains on the measured critical path, one fixed 128-row `INSERT ... ON
CONFLICT DO NOTHING` statement without `RETURNING` may be tested. Use one exact
remainder statement per transaction and perform bounded byte comparison only
for actual preexisting conflicts. Retain it only if SQL execution falls to at
most 0.8 seconds without increasing Store bytes, physical I/O, CPU, RSS, or
transaction size. This is not authorization for a generic bulk API.

### Time and ownership budgets

For the 500-MB tier:

```text
source/canonical producer lane       <=2.10 s
SQLite insertion/commit lane         <=1.50 s
overlapped producer/admission window <=2.25 s
final inode/root/publication tail    <=0.25 s
complete initialization              <=2.50 s
```

The lanes overlap; they are not added. The target explicit ownership is about
8.4 MiB: eight partial slabs about 2 MiB, four queued slabs about 1 MiB, one
admission batch below 4 MiB, aggregate CDC scratch about 0.5 MiB, compact pair
state about 0.25 MiB, and bounded headers/final state. Whole-process RSS is
reported separately; the existing SQLite page cache alone is 32 MiB, so the
10-MiB contract applies to explicit LayerFS-owned buffers, not total RSS.

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
admission batch remains <=4 MiB and <=8,191 objects
candidate unique objects = inserted objects + preexisting reused objects
```

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
200-MB/s claim; continue removing product amplification and report the exact
external bound.

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
  RUN_ID CONTAINER_ID all 3
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
- [ ] The 100,000-file init median is at most 2.5 seconds, at least 200 MB/s,
  and at least 40,000 files/s.
- [ ] Every fresh process and Store starts with an empty metadata intern table;
  no entry survives the initialization operation or comes from setup.
- [ ] Exact metadata interning is bounded to eight entries per producer and an
  all-unique metadata case does not materially regress.
- [ ] The admitted path uses at most eight existing producers, four queued
  256-KiB/512-object slabs, and the calling thread as sole SQLite owner.
- [ ] Object-segment write/read bytes, parent payload rewrites, and parent
  payload-copy bytes are zero; 100,000-file handoffs are at most 2,200.
- [ ] Adjacent initialization ratios are at most 2.0x.
- [ ] The 100,000-file Create median is at most 25 milliseconds, non-Attach
  work at most 10 milliseconds, and Store-wide Create scans equal zero.
- [ ] Localized Commit remains at most 10 milliseconds with touched-path-only
  candidate objects.
- [ ] No new or increased product worker, thread pool, dependency, crate,
  background preloader, public operation, or comparison product is added.
- [ ] Explicit buffers are at most 10 MiB; preferred incremental RSS is at most
  16 MiB and hard incremental RSS at most 32 MiB without an OS memory cap.
- [ ] CPU, copies, spool traffic, transactions, Store size, and read batching
  show no hidden resource trade.
- [ ] Create/cache Store I/O occurs outside cache locks; small-file prefetch is
  bounded and records zero anchor prefetches; actual read-ahead is at most
  8 MiB.
- [ ] Exact real-FUSE reconnect, materialization equality, Docker lifecycle,
  attachment-failure cleanup, focused tests, formatting, warning-denying
  Clippy, `tools/test-fast.sh`, `git diff --check`, and documentation links
  pass.

## Non-goals

No new benchmark family, exact repository distribution, live repository scan,
new directory/extension distribution, new product worker, raised worker
ceiling, persistent background service, larger cache as a latency fix, schema
migration, canonical inlining, physical object packing, packed fixture,
new FUSE/daemon/proxy request or response tag, materialization substitution,
Cloudflare namespace comparison, prepend,
`copy_file_range`, borrowed ranges, sparse/mixed-edit work, release publication,
or silent reinterpretation of namespace-v1 evidence.
