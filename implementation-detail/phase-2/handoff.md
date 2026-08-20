# Phase 2 Handoff

Phase 1 establishes the bounded canonical core. Phase 2 starts the large-file
path and must prove streaming behavior, chunk reuse, content-tree locality,
and memory bounds before SQLite-specific work begins.

## 1. Phase 1 delivered

The committed Phase 1 scope is:

```text
canonical paths and names
        ↓
bounded Bytes and Directory objects
        ↓
fixed 9-byte LFSO envelope
        ↓
domain-separated BLAKE3 ObjectId
```

Implemented and tested:

- bounded canonical paths and immediate directory names;
- deterministic unsigned-byte ordering;
- only `Bytes` and `Directory` canonical object kinds;
- checked 9-byte `LFSO` envelope encoding and decoding;
- exact end-of-input validation;
- bounded streaming decode through `Read`;
- canonical encode through `Write`;
- direct object identity hashing through a BLAKE3 write sink;
- identity verification before decoding; and
- malformed, truncated, oversized, ordering, fragmentation, and golden-vector
  regressions.

The direct `Object::id()` path uses the existing canonical encoder and feeds
the encoded slices directly into BLAKE3. It removes the temporary complete
canonical `Vec` when the caller needs only the identity. It does not change
canonical bytes, object IDs, or the on-disk format.

## 2. Evidence and limits

The Phase 1 benchmark artifacts are under `eval/`:

- `eval/phase1-baseline/`
- `eval/phase1-object-id-streaming/`

All 32 Phase 1 benchmark cases returned `correct: true`. The release run was
performed on macOS arm64 with APFS and recorded maximum resident size of about
30.4 MiB for the whole benchmark process.

That RSS result is not a per-operation memory measurement. The benchmark keeps
encoded fixtures alive, runs all cases in one process, and `/usr/bin/time -l`
reports the process high-water mark. The unchanged aggregate RSS after direct
identity hashing is therefore inconclusive for that local optimization. Do not
use it as evidence that the streaming path has no memory benefit, and do not
claim that Phase 1 bounds process-wide concurrent memory.

Phase 2 must measure per-case working memory separately from process RSS:

```text
fresh-process baseline RSS
fresh-process case RSS
case RSS minus baseline RSS
explicit LayerFS buffer accounting
```

RSS is a host observation. The correctness contract must also count live
buffers and enforce a total in-flight budget under concurrency.

## 3. Phase 2 objective

Build and benchmark this path without staging a complete file:

```text
source Read
    → streaming CDC scanner
    → bounded chunk buffer
    → BLAKE3 chunk identity
    → immutable CAS test port
    → bounded content-tree nodes
```

The production candidate remains:

```text
File → bounded immutable content tree → Chunk IDs → CAS
```

`File`, `ContentLeaf`, and `ContentBranch` are candidate logical shapes only.
Do not freeze their canonical encodings until the layout benchmark selects the
shape.

## 4. First implementation order

1. Freeze or verify the controlling CDC profile, including boundary,
   fragmentation, and maximum-memory rules.
2. Implement the streaming CDC scanner with no source-sized staging buffer.
3. Implement chunk identity and immutable CAS semantics behind a small
   in-memory test store.
4. Implement the candidate content-tree builders and readers:
   flat manifest, segmented layout, and fixed-fanout tree.
5. Measure range reads and one-byte edits before selecting the final shape.
6. Add exact rejoin verification and prefix/suffix chunk reuse.
7. Freeze `File`, `ContentLeaf`, and `ContentBranch` only after the benchmark.
8. Keep SQLite out of this first loop; add it after the core behavior and
   counters are proven.

## 5. Phase 2 acceptance gates

Run the core portions of B6, B7, and B8 from `../evaluation.md`:

| Gate | Workload | Must prove |
|---|---|---|
| B6 | Stream a new 100 MiB file | No source-sized duplicate buffer; exact final identity and bytes |
| B7 | One-byte middle edit on 16, 100, and 512 MiB files | Reuse and scan counters show locality; no silent whole-file fallback |
| B8 | Equal-length, prepend, append, truncate, and EOF edits | Exact final bytes and typed failure for failed rejoin |

Every run must record:

- source fingerprint and environment;
- wall and CPU time;
- CDC bytes scanned;
- chunks reused and created;
- bytes hashed and written;
- peak memory and explicit memory-budget status; and
- exact final-byte correctness.

The minimum memory property is:

```text
file size grows
    → per-operation working memory stays bounded
    → total concurrent memory stays under admission budget
```

## 6. Do not carry forward

- Do not represent a large file as one `Object::Bytes` value.
- Do not encode, hash, and store the same complete file through separate
  source-sized buffers.
- Do not load every content-tree node for a bounded range read.
- Do not select a flat manifest merely because it is easiest to implement.
- Do not add SQLite tables or backend-specific types to `layerfs-core`.
- Do not claim a memory or throughput improvement from aggregate RSS alone.
- Do not add a second root identity or version field to the Phase 1 format.

The next useful artifact is a Phase 2 in-memory CDC/CAS benchmark report, not a
SQLite schema.

## 7. Phase 2 closure and Phase 3 handoff

The requested Phase 2 data-plane slice is closed on the current worktree. The
implementation is limited to the frozen streaming CDC scanner, the existing
Phase 1 BLAKE3 identity domain, immutable authenticated `InMemoryCas`, the
unencoded logical-file representation, bounded range reads, and bounded edit
rejoin. No production storage trait, SQLite code, or canonical large-file
object encoding was added.

### Evidence artifacts

The final release artifacts are retained at these repository-relative paths:

```text
eval/runs/phase2-layout-selection-baseline-final/environment.json
eval/runs/phase2-layout-selection-baseline-final/results.jsonl
eval/runs/phase2-layout-selection-baseline-final/summary.md

eval/runs/phase2-edits-baseline-final/environment.json
eval/runs/phase2-edits-baseline-final/results.jsonl
eval/runs/phase2-edits-baseline-final/summary.md
```

The layout artifact contains 27/27 correct rows: S1-16, S1-100, and S1-512;
flat manifest, fixed 64-chunk segments, and fixed-fanout-16 tree; and 64 KiB
prefix, middle, and EOF ranges. The edit artifact contains 11/11 correct
rows: B6 on all three single-file sizes, B7 one-byte middle replacement on
all three sizes, and the five B8 edit shapes on S1-100. Both artifacts retain
source fingerprints, host/source metadata, exact correctness, and the
operation counters.

### Counter findings

| Gate | Dataset | CDC bytes scanned | Chunks reused | Chunks created | Finding |
|---|---|---:|---:|---:|---|
| B6 full replace | S1-16 | 16,777,216 | 0 | 851 | Full source scan by design |
| B6 full replace | S1-100 | 104,857,600 | 0 | 5,284 | Full source scan by design |
| B6 full replace | S1-512 | 536,870,912 | 0 | 27,162 | Full source scan by design |
| B7 middle byte | S1-16 | 1,052,829 | 850 | 1 | Bounded edit work; unchanged chunks reused |
| B7 middle byte | S1-100 | 1,060,505 | 5,283 | 1 | Bounded edit work; unchanged chunks reused |
| B7 middle byte | S1-512 | 1,070,912 | 27,161 | 1 | Bounded edit work; unchanged chunks reused |

B8 on S1-100 produced the following exact counter results:

```text
equal-length middle replacement: 1,060,505 scanned; 5,284 reused; 0 created
prepend:                         1,045,499 scanned; 5,283 reused; 1 created
append:                             14,196 scanned; 5,283 reused; 1 created
truncate:                            4,785 scanned; 5,280 reused; 1 created
EOF no-op:                                0 scanned;     0 reused; 0 created
```

B6 is intentionally linear because it captures a new complete source. B7 and
B8 remain bounded by the rejoin probe plus the affected chunk and changed
bytes; they do not silently fall back to a full-file scan. The layout and edit
artifacts are in-memory baselines, not durable-storage, concurrency, peak-RSS,
or final throughput qualifications.

### Layout decision

The benchmark recommends fixed-size 64-chunk segmentation as the working
candidate for the next internal tree/COW experiments: on the tested middle and
EOF ranges it reduced reference inspection to the local segment, while the
flat manifest remained linear in the number of preceding references. The
fixed-fanout tree also provided local reference inspection, but its simple
candidate traversal visited more metadata nodes than segmentation in these
measurements.

This is only a layout-selection recommendation. The candidates are unencoded
in-memory structures; the benchmark did not freeze `File`, `ContentLeaf`, or
`ContentBranch` canonical bytes, add public format/version fields, or select a
final persistent representation. Revisit the choice after the canonical
encoding constraints, COW ancestor updates, and durable range-read costs are
measured.

### Exact verification commands

The closure evidence was produced and checked with:

```text
cargo test -p layerfs-core content --offline
cargo build --release -p layerfs-eval --offline
target/release/layerfs-eval phase2-layout eval/runs/phase2-layout-selection-baseline-final
target/release/layerfs-eval phase2-edits eval/runs/phase2-edits-baseline-final
cargo test --workspace --offline
cargo fmt --all -- --check
cargo clippy --workspace --offline --all-targets -- -D warnings
git diff --check
```

The focused content tests passed 8/8; the workspace passed 32 core tests and 5
evaluator tests; formatting, warnings-denied clippy, and diff checks passed.

### Later work and first Phase 3 task

The following remain later work and are deliberately not Phase 2 closure
claims: final canonical `File`, `ContentLeaf`, and `ContentBranch` encoding;
durable engine/SQLite integration; COW and delta persistence; VFS/SDK layers;
and host/platform projection. B9 scattered capture and B10 repeated
checkpoint orchestration also require those later root and transaction paths.

The exact first Phase 3 task is: implement and test the unencoded immutable
directory/file tree and one in-memory copy-on-write root transition over the
selected segmented working candidate. The first test must mutate one file,
prove the parent root remains readable and unchanged, prove unchanged sibling
subtrees retain their identities, and expose the bounded changed-ancestor
spine needed by the later delta and SQLite phases. This task must preserve the
current Phase 1 identity domains and must not freeze canonical large-file
encodings until that COW/layout evidence is complete.
