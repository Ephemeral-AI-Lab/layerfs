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

Run the core portions of B6, B7, and B8 from `eval.md`:

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
