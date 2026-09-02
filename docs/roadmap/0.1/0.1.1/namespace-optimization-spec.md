# LayerFS 0.1.1 namespace-v2 benchmark and optimization specification

> **Status:** Proposed replacement admission contract. It is not implemented,
> registered, or release evidence yet.
>
> **History:** Namespace-v1 used uniform 2,500-byte files. Its source-bound
> evidence remains immutable. Namespace-v2 keeps the same LayerFS-only family,
> scenario IDs, runner, public lifecycle, and registered-total exclusion while
> replacing only the active fixture profile and result-schema version.
>
> **Ownership:** GitHub issues
> [#7](https://github.com/Ephemeral-AI-Lab/layerfs/issues/7),
> [#9](https://github.com/Ephemeral-AI-Lab/layerfs/issues/9), and
> [#10](https://github.com/Ephemeral-AI-Lab/layerfs/issues/10), assigned to
> `@yifanxuaaa`.

Execution agents use the
[namespace-v2 handoff prompt](namespace-v2-handoff-prompt.md).

## Problem statement

The retained namespace-v1 candidate proves the complete public LayerFS
lifecycle, but its uniform 2,500-byte fixture does not cover a realistic
small-heavy namespace with large files. Its 100,000-file evidence also shows
two independent performance problems:

- initialization processes 250 MB in 6.799 seconds, or 36.77 decimal MB/s;
- Workspace Create grows to about 234 milliseconds even though real-FUSE
  Attach remains about 15 milliseconds.

The initialization path must become a bounded streaming pipeline that performs
each required scan, canonical construction, hash, and admission once. Workspace
Create must demand-load only the pinned root instead of scanning the Store.
Neither improvement may add or increase product workers, hide work in setup,
raise memory bounds to buy latency, or change canonical, Store, SDK, CLI,
daemon, proxy, FUSE, or acknowledgement contracts.

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
schema: fs-bench-pro-namespace-v2
fixture_profile: synthetic-small-heavy-v1
fixture_digest_profile: namespace-file-digest-tree-v1
```

The active implementation may replace the v1 generator after one bridge proof
retains both exact contracts. No runtime selector, second runner, or duplicate
family is required merely to regenerate historical evidence.

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

Measure one variable at a time in this order:

1. **Seal spilled worker-local transfers.** A 100-MB anchor forces a local
   candidate spill. Before timing, make transfer explicitly fallible and seal
   pending spill bytes, close/flush the writer, and preserve a readable
   segment. Prove a nonempty pending tail, serial/parallel canonical equality,
   fresh reopen, and cleanup. Do not perform a second reachability traversal.
2. **Known single-chunk construction.** Directly emit byte-identical payload,
   extent-leaf, and file-state objects for known nonempty inputs below the
   8,192-byte CDC minimum, including portable mode and mtime. Retain the
   generic path at empty, unknown, changing, or larger inputs. For a native
   tiny file, open once, read the exact observed length, perform one trailing
   read to detect growth, and fail on early EOF.
3. **Fitting initial-directory leaf.** Sort once and encode one final leaf and
   state when every entry fits. Fall back at the first overflow. Prove complete
   canonical-object equality at empty, one, 100, largest-fitting, first
   overflow, and the 1,000-entry fixture root. The root may use the existing
   fallback; do not claim a direct multi-page builder.
4. **Composite sealed segments.** Replace all-result retention and worker-spool
   to parent-spool copying with small sealed-segment descriptors. Admission
   consumes each segment directly once and releases it. Memory-backed segments
   move owned vectors; file-backed segments remain file-backed. Global
   hard-link validation completes before any parallel structural object can
   become durable.
5. **Exact cross-segment dedup and collision authority.** Deduplicate inside
   each bounded batch. On a proven-empty Store, use the database uniqueness
   authority while identifying actual inserted IDs and checking skipped IDs
   for byte equality. Preserve the existing exact nonempty-Store reuse path or
   an equally exact disk-backed authority. Never use a complete in-memory set
   or linear spill-file membership scan.
6. **Compact inode stream.** Encode finalized inode records early and retain
   compact `(InodeId, record ObjectId)` pairs. Keep mutable records only while
   hard-link counts are unresolved. Consume compact segment insertions without
   materializing another complete mutation vector. Preserve insertion order
   and prove every canonical inode-table object, not only logical lookup.
7. **Move-only admission.** Transfer owned canonical bytes through one bounded
   producer/admission path; do not clone them into local, parent, spool, and
   SQLite owners.
8. **Reusable CDC scratch.** Reuse bounded scratch per existing importer worker
   for non-tiny inputs, including anchors. Reset it exactly across success and
   callback failure; include aggregate scratch in the 10-MiB equation.
9. **Bounded ID/order state.** Spill large order and ID collections
   sequentially with a small page. Allow at most one required complete spool
   read per phase and no linear membership rescans.
10. **SQLite A/Bs.** Test bounded batch ordering, statement reuse, and current
   multi-row versus cached single-row execution one at a time. Preserve the
   4-MiB/8,191-object bounds, collision checks, visibility-last publication,
   schema, and reconnect behavior.

Do not introduce physical object packing, a new schema, a packed fixture,
canonical inlining, or a bulk initialization API under this specification.
Those require a separate incompatible-contract decision.

Required ownership invariants:

```text
worker segment writes <= one complete segment pass
admission segment reads <= one complete required pass
parent payload spool rewrite = 0
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

A safe composite path still reads 500 MB of source, writes and reads sealed
segments, and writes SQLite pages. The control records effective aggregate I/O,
CPU, page-cache state, and `/proc/self/io` or the platform equivalent. A host
ceiling does not authorize extra product workers, tmpfs substitution, hidden
setup work, or a false 200-MB/s claim; continue removing product amplification
and report the exact external bound.

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

The required implementation order is:

```text
large spilled-local correctness proof
-> freeze and implement namespace-v2 fixture/oracle
-> revised same-fixture baseline
-> native single-chunk path
-> fitting-directory path
-> composite sealed segments
-> compact inode stream and exact dedup authority
-> reusable CDC scratch
-> isolated SQLite A/Bs
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
