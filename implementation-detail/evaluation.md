# LayerFS Mini Evaluation

Status: evaluation contract for the LayerFS Rust + SQLite restart.

This document defines the first benchmark and correctness evaluation for the
four-operation DeltaGit workflow:

```text
open → materialize → modify ordinary directory → capture or discard
```

The evaluation is based on the useful measurement rules in the existing
[M3 benchmark document](https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/benchmarks/m3-improvements.md): separate cold and warm behavior, count database transactions and statements, measure bounded edits, measure storage growth, and verify exact final bytes.

Historical numbers from the previous TypeScript/SQLite implementation are
reference context only. They are not Rust acceptance targets.

## 1. Evaluation goals

The first evaluation must answer these questions:

1. Can LayerFS materialize a complete root correctly?
2. Is warm no-op materialization materially cheaper than cold materialization?
3. Are sequential and random reads fast without reconstructing an entire file?
4. Does a small edit to a large file avoid a full-file CDC and hash pass?
5. Does capture write only changed objects and metadata?
6. Does SQLite transaction overhead dominate the operation?
7. Does memory remain bounded as file size and edit count grow?
8. Are Rust and TypeScript being compared under equivalent conditions?

## 1.1 Phase 1 closure baseline

Phase 1 has a separate microbenchmark gate before it is marked complete. This
gate measures only the canonical core and prevents us from confusing bounded
object performance with the later large-file system benchmark.

Run it after the Phase 1 source and tests are stable:

```text
cargo build --release -p layerfs-eval
/usr/bin/time -l -o eval/phase1-<commit>/time.txt \
  target/release/layerfs-eval phase1 eval/phase1-<commit>
```

The command produces `environment.json`, `results.jsonl`, and `summary.md`.
The `time.txt` file is the external macOS maximum-resident-size observation.
If the host cannot provide a reliable RSS observation, retain the artifact and
record the value as `unavailable`; never substitute zero.

The Phase 1 baseline cases are:

| Case family | Inputs | Operations | Required evidence |
|---|---|---|---|
| Bytes | 1 KiB, 1 MiB, 8 MiB | encode to `Vec`, encode to `Write`, decode from slice, decode from `Read`, hash from slice, hash from `Read`, `Object::id` | Median timing, input/output bytes, correct result |
| Directory | 16, 256, and 4,096 children | encode, streaming decode, streaming hash | Median timing, encoded size, correct result |
| Paths | Short nested path and 256-component near-maximum path | canonical validation | Median timing, path length, correct result |

Each case has one warm-up and five measured iterations. The benchmark must
exercise the actual public Phase 1 entry points; it must not benchmark only
fixture construction or a private mock codec.

Phase 1 may close only when:

- every case returns the expected object or identity;
- the benchmark artifacts record the source commit and dirty-tree state;
- per-case timings and input/output sizes are retained;
- peak memory is recorded by the external macOS measurement or explicitly
  marked unavailable; and
- the report states that this is a bounded canonical-object baseline, not a
  large-file, CDC, CAS, SQLite, or concurrency qualification.

This gate does not set a throughput target. It establishes the source- and
environment-specific baseline used to detect regressions in Phase 1. Phase 2
then adds the first architectural performance gate for CDC, CAS, content-tree
locality, range reads, and one-byte edit scaling.

## 2. Fixed datasets

All datasets must be deterministic. The generator seed, content pattern,
paths, file sizes, and edit offsets must be recorded in the result artifact.

### 2.1 Single-file dataset

```text
S1-16       one file, 16 MiB
S1-100      one file, 100 MiB
S1-512      one file, 512 MiB
```

The file content must be reproducible and must contain enough repeated and
non-repeated regions to exercise CDC boundaries. The same content is used for
Rust, TypeScript, and any future backend comparison.

### 2.2 Mixed-tree dataset

```text
S2-tree     10,000 deterministic files, approximately 100 MiB total
```

The tree must include nested directories, small files, medium files, empty
directories where supported, and a small number of larger files. The exact
distribution is fixed by the dataset manifest and not regenerated differently
for each run.

## 3. Required benchmark table

| ID | Scenario | Fixed workload | Required measurements | Expected property |
|---|---|---|---|---|
| B0 | Correctness seed | Create deterministic roots for S1 and S2 | Root identity, object count, canonical bytes | Same input produces the same authenticated result |
| B1 | Cold materialize | Materialize S2 into an empty destination | Wall time, CPU time, bytes read/written, objects read, files created, peak memory | Complete tree is materialized without source-sized staging |
| B2 | Warm no-op materialize | Materialize the same root again into the matching destination | Wall time, files rewritten, bytes rewritten, object reads | Unchanged files are not rewritten |
| B3 | Warm incremental materialize | Parent root to child root with 3 changed paths | Changed paths, files rewritten, bytes read/written, wall time | Work follows changed paths and affected ancestors |
| B4 | Sequential read | Read a 100 MiB file from offset 0 to EOF in fixed 1 MiB reads | MiB/s, latency, bytes read, object reads, SQLite transactions/statements | Stable streaming throughput |
| B5 | Random range read | 100 deterministic 64 KiB ranges from a 100 MiB file | p50/p95 latency, bytes read, object reads, transactions/statements | Work follows requested ranges plus required metadata |
| B6 | New-file write | Stream a new 100 MiB file into LayerFS | MiB/s, CDC bytes scanned, chunks/objects created, bytes written, transaction time | No source-sized buffer or whole-file duplicate pass |
| B7 | Small-edit scaling | One-byte middle replacement on 16, 100, and 512 MiB files | Capture time, CDC bytes scanned, chunks reused/created, bytes written, memory | Bounded edit work is approximately independent of total file size |
| B8 | Edit-shape coverage | Equal-length replacement, prepend, append, truncate, and EOF edit on S1-100 | Capture time, bytes scanned, chunks changed, final bytes | All edit shapes are exact and do not silently use an unbounded fallback |
| B9 | Scattered capture | 50 edits for the quick loop; 500 edits for the checkpoint run on S1-100 | Total time, time/edit, storage growth, chunks reused/created, final bytes | Repeated edits do not rebuild the complete file per edit |
| B10 | Repeated checkpoints | 32 sequential small-edit checkpoints from one base | Cumulative storage, deduplication ratio, root/delta count | Unchanged payload is not duplicated per checkpoint |

The labels are descriptive. Do not reuse the historical `A6` label for both
random reads and scattered edits.

## 4. Minimum development run

The quick run must be small enough to execute after normal code changes:

```text
datasets:
  S1-100
  S2-tree

cases:
  B1 cold materialize
  B2 warm no-op materialize
  B3 incremental materialize
  B4 sequential read
  B5 100 random ranges
  B6 streamed 100 MiB write
  B7 three one-byte edits
  B8 all five edit shapes
  B9 fifty scattered edits

runs:
  one warm-up
  five measured iterations
```

The checkpoint run adds S1-16 and S1-512 to B7, runs 500 scattered edits in
B9, and executes B10.

## 5. Measurements

Every case records one JSON object per iteration with at least these fields:

```text
case
backend
language
dataset
seed
file_size_bytes
operation
iteration
elapsed_ns
cpu_time_ns
bytes_read
bytes_written
cdc_bytes_scanned
chunks_reused
chunks_created
objects_reused
objects_created
sqlite_transactions
sqlite_statements
peak_memory_bytes
temporary_storage_peak_bytes
correct
```

Materialization cases additionally record:

```text
files_created
files_replaced
files_deleted
files_rewritten
changed_paths
```

Capture cases additionally record:

```text
parent_root
new_root
delta_bytes
changed_paths
```

The benchmark must distinguish these quantities:

- logical input bytes;
- bytes read from the engine;
- bytes written to the engine;
- CDC bytes scanned;
- newly created chunk/object bytes; and
- total database file growth.

SQLite transaction and statement counts are required because a fast result
with excessive database round trips may not scale. Counts must come from the
engine boundary, not an estimate based on elapsed time.

## 6. Correctness gates

Performance results are invalid if the correctness gate fails.

### 6.1 Materialization

For B1 through B3:

```text
materialized directory == expected directory tree
```

The comparison must check paths, entry kinds, file lengths, file bytes, and
relevant metadata defined by the LayerFS format.

### 6.2 Reads

For B4 and B5, every returned byte must equal the expected source byte at the
requested offset and length. A short read is a failure, not a successful
partial result.

### 6.3 Capture

For B6 through B10:

```text
materialize(captured_root) == expected_final_directory
```

The captured root must retain the correct parent relationship. A failed
capture must not advance the workspace head or expose a partial root.

### 6.4 Small-edit proof

For B7, record the scaling of:

```text
file size → capture time
file size → CDC bytes scanned
file size → bytes written
```

The first implementation does not receive a numerical pass threshold before a
baseline exists. It must, however, expose the counters needed to identify a
full-file fallback. If a one-byte edit scans or rewrites the complete file,
the result must be marked as bounded-edit failure even if the final bytes are
correct.

### 6.5 Phase 2 in-memory baseline artifacts

The Phase 2 data-plane baseline uses the existing `layerfs-eval` artifact
conventions. Build it in release mode and retain the generated
`environment.json`, `results.jsonl`, and `summary.md` together:

```text
cargo build --release -p layerfs-eval --offline
target/release/layerfs-eval phase2-layout eval/runs/<layout-run>
target/release/layerfs-eval phase2-edits eval/runs/<edit-run>
target/release/layerfs-eval phase2-ingest-breakdown eval/runs/<breakdown-run>
```

`phase2-layout` constructs one authenticated CDC/CAS fixture for each of
S1-16, S1-100, and S1-512, then compares three unencoded in-memory reference
layouts: a flat manifest, fixed 64-chunk segments, and a fixed-fanout-16 tree
with 64-chunk leaves. It runs deterministic 64 KiB prefix, middle, and EOF
ranges with one warm-up and three measured iterations. Each row records exact
correctness, source fingerprint and metadata, elapsed samples, and metadata
nodes, chunk references, chunks read, and delivered bytes. This is a
layout-selection baseline only; it does not freeze `File`, `ContentLeaf`, or
`ContentBranch` encodings and is not a final performance claim.

`phase2-edits` records B6 full replacement on all three single-file sizes, B7
one-byte middle replacement on all three sizes, and B8 equal-length middle
replacement, prepend, append, truncate, and EOF on S1-100. Each operation is
verified against a deterministic BLAKE3 fingerprint. The result rows retain
CDC bytes scanned, reused and created chunks, bytes hashed, bytes delivered,
CAS bytes stored, range and final size, source metadata, and correctness. A
bounded B7 or B8 result must be judged from these counters; correct bytes alone
do not qualify an unbounded full-file fallback. These are cold in-memory
baselines, not durable-storage, concurrency, or final performance claims.

`phase2-ingest-breakdown` runs only S1-100 and partitions the same B6
`full_replace` operation into source read/generation, CDC scanner work, full
in-memory CAS publication, and logical-file manifest finalization. Its
`component_total_ns` is the additive sum of those four stages; the separate
`outer_elapsed_ns` is retained as a timer cross-check. The CAS row includes
BLAKE3 identity hashing, authenticated lookup, byte copy, and insertion/reuse
in the current `InMemoryCas` implementation. It is a diagnostic decomposition
of the in-memory baseline, not a durable SQLite or filesystem benchmark.

## 7. Cold and warm protocol

The benchmark must use these labels precisely:

- `cold`: operating-system page-cache state was actually controlled;
- `warm`: the same data was accessed previously without clearing the cache;
- `reopened`: the process or database was reopened, but cache state was not
  controlled.

Reopening SQLite alone is not evidence of a cold run. If cache control is not
available on the host, the result must be reported as `reopened` or `unknown`,
not as `cold`.

## 8. Reproducibility

Each run records:

```text
git_commit
dirty_tree
rustc_version
sqlite_version
compiler_profile
operating_system
filesystem
cpu_model
memory_size
storage_device
journal_mode
synchronous_mode
cdc_profile
dataset_manifest_hash
benchmark_command
timestamp
```

The result is not accepted as a comparison artifact if source, dataset, SQLite
mode, storage device, or benchmark command is unknown.

## 9. Rust versus TypeScript comparison

Comparisons must use the same:

- dataset manifest and edit offsets;
- SQLite database schema and logical indexes;
- journal mode and synchronous setting;
- storage device and filesystem;
- cold/warm procedure;
- operation boundaries;
- correctness oracle; and
- number of warm-up and measured iterations.

Report at least:

```text
median elapsed time
p95 elapsed time
throughput
SQLite transactions
SQLite statements
CDC bytes scanned
engine bytes read/written
peak memory
```

Do not compare a Rust release build against a TypeScript development build.
Do not compare different SQLite durability settings. Do not attribute a
database improvement to Rust without separating core CDC/CAS time from engine
transaction time.

## 10. Reporting

The benchmark should produce:

```text
eval/
└── runs/
    └── <timestamp>-<git-commit>/
        ├── environment.json
        ├── dataset.json
        ├── results.jsonl
        ├── correctness.json
        └── summary.md
```

`summary.md` must include:

- the exact command;
- the source fingerprint;
- the dataset manifest hash;
- median and p95 per case;
- the relevant work counters;
- correctness status;
- comparison conditions; and
- any unavailable observation.

Unavailable memory, cache, or filesystem observations must be marked
`unavailable`; they must not be recorded as zero.

## 11. Initial interpretation rules

The first benchmark run is a baseline, not a pass/fail performance claim.

The first hard gates are:

- exact materialization;
- exact range reads;
- exact captured bytes;
- immutable parent roots;
- no partial root publication;
- no unchanged-file rewrites on B2;
- no hidden full-file fallback on B7; and
- bounded, recorded memory and storage observations.

After the baseline, define performance targets from the measured bottleneck.
Do not import historical goals such as sub-10-millisecond edits or GB/s reads
without evidence that the Rust/SQLite composition can achieve them on the
specified hardware.
