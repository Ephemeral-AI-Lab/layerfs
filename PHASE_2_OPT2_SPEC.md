# Phase 2 Optimization 2: append-only packed CAS

Status: implemented and benchmarked; not promoted as a performance optimization.
The packed layout is retained as a qualification candidate only. Pre-sizing
removes its allocation-growth penalty, but the clean throughput lane shows
parity rather than a material win.

Checkpoint: Phase 2 Optimization 1 (`c581bf9`).

## 1. Decision

Keep the frozen FastCDC profile and the existing chunk identity domain.

Implement the next performance experiment as an append-only packed in-memory
CAS candidate. The candidate replaces the current per-chunk owned payload
layout:

```text
BTreeMap<ChunkId, Vec<u8>>
```

with one append-oriented payload area plus the same kind of ID lookup:

```text
BTreeMap<ChunkId, ChunkLocation>
                       |
                       v
               append-only payload area
```

This isolates the cost of per-chunk payload allocation, ownership copies, and
allocator metadata without mixing in a new CDC algorithm, SQLite, filesystem
durability, or a different index implementation.

The first candidate may be benchmark-only and private to `layerfs-core`. It is
not yet the production storage engine and it does not freeze the eventual
durable pack format.

The implementation and A/B evidence are retained as an experiment. The
packed candidate passed differential correctness but was slower than the
current baseline on the measured APFS run, so it must not replace the current
CAS without a follow-up optimization and another measured gate.

## 2. Why this is Opt2

The current warm APFS baseline is:

| Stage | Median |
|---|---:|
| Source read | 14.635 ms |
| FastCDC | 115.288 ms |
| CAS publication | 117.852 ms |
| Manifest finalization | 0.019 ms |
| **Total** | **247.795 ms** |
| **Throughput** | **403.6 MiB/s** |

The measured operation processes 100 MiB into 5,284 unique chunks. The
current CAS stores one owned `Vec<u8>` per unique chunk. The CAS stage is the
second-largest measured stage and is the next narrow shared boundary to test.

The intended data path is:

```text
regular APFS file
        |
        v
bounded FastCDC scanner
        |
        v
borrowed reusable chunk slice
        |
        +--> one BLAKE3 ChunkId computation
        |
        +--> append bytes to packed payload area
        |
        +--> record ChunkId -> {offset, length}
        |
        v
logical chunk-reference manifest
```

The current scanner already reuses its chunk buffer. Opt2 must remove the
CAS-side per-chunk `Vec` allocation. It may still copy each new chunk once
into persistent packed storage; that copy is necessary for an in-memory CAS
to retain immutable bytes. A later direct two-span sink can investigate
removing additional handoff copies, but it is not required for this Opt2
candidate.

## 3. Scope

### 3.1 In scope

- a packed in-memory CAS implementation used for controlled A/B measurement;
- append-only payload ownership for newly inserted chunks;
- ID-to-location lookup;
- authenticated duplicate lookup and reuse;
- bounded chunk lengths and checked offset arithmetic;
- exact reads by chunk ID;
- differential correctness against the existing `InMemoryCas`;
- the existing FastCDC boundaries, chunk IDs, logical-file identity, and edit
  behavior;
- stage timing and fresh-process RSS comparison;
- an evidence report recording whether packing is a real improvement.

### 3.2 Out of scope

- SeqCDC or any CDC profile switch;
- changing FastCDC parameters, masks, GEAR tables, or chunk boundaries;
- changing BLAKE3 domains or canonical object bytes;
- replacing `BTreeMap` with `HashMap` or another index in this experiment;
- SQLite, PostgreSQL, WAL, journaling, transactions, or database schemas;
- durable pack files, crash recovery, pack catalogs, or online repack;
- compression, encryption, or checksums beyond the existing identity
  authentication;
- worker pools, hidden threads, async tasks, or internal parallelism;
- a public storage-provider trait with multiple implementations;
- a production memory-bound claim for retained CAS payloads;
- a new large-file canonical object encoding.

The purpose of keeping the index and algorithm fixed is attribution. If the
candidate changes CDC, lookup structure, and physical persistence at once, a
result cannot tell us why it became faster or slower.

## 4. Current and candidate representations

### 4.1 Current representation

The current Phase 2 candidate is conceptually:

```rust
BTreeMap<ChunkId, Vec<u8>>
```

For every new chunk it performs:

1. one `ChunkId` calculation through `put_chunk`;
2. an ID lookup;
3. an owned allocation for the chunk payload;
4. a copy from the reusable CDC buffer into that allocation; and
5. a map insertion containing the owned payload.

Opt1 already removed the duplicate ID hash inside `put_chunk`. Opt2 must not
reintroduce a second hash while changing storage ownership.

### 4.2 Candidate representation

The minimal candidate is:

```rust
struct ChunkLocation {
    offset: u64,
    length: u32,
}

struct PackedInMemoryCas {
    payload: Vec<u8>,
    index: BTreeMap<ChunkId, ChunkLocation>,
    stored_bytes: u64,
}
```

The exact Rust visibility and names are implementation details. The semantic
shape is the requirement:

- `payload` owns bytes in append order;
- `index` owns only fixed-size location metadata plus the chunk ID;
- `length` is sufficient for the current 32 KiB maximum chunk size;
- `offset` uses a wide checked type;
- `stored_bytes` counts logical payload bytes, not allocator capacity;
- no chunk payload is separately allocated after append.

The first implementation should retain `BTreeMap` so the experiment changes
payload layout but not lookup complexity. A separate index experiment may be
valuable later, but combining it with packing would make the result
ambiguous.

### 4.3 Capacity-growth rule

The first candidate may use a normal append-only `Vec<u8>`. Its capacity
growth must be observed because a growing vector can temporarily copy the
existing payload into a larger allocation.

If capacity growth materially dominates the result or creates an avoidable
RSS spike, the smallest follow-up is a fixed-size segmented payload area:

```text
Segment 0: [bytes ...]
Segment 1: [bytes ...]
Segment 2: [bytes ...]
```

That follow-up is not part of the initial Opt2 acceptance. Do not add a
general segmented-arena abstraction before the single packed candidate has
been measured.

## 5. Required operations

The packed candidate must preserve the semantic behavior of `InMemoryCas`.

### 5.1 Insert by bytes

`put_chunk(bytes)` must:

1. reject a chunk larger than `MAXIMUM_CHUNK_BYTES`;
2. compute the `ChunkId` exactly once;
3. look up the ID in the index;
4. if present, read the indexed bytes and authenticate/compare them;
5. return `Reused` without appending on an equal authenticated incumbent;
6. reject an unequal or corrupted incumbent with `IdentityMismatch`;
7. check offset, length, and payload-size arithmetic before mutation;
8. append the bytes exactly once for a new chunk;
9. insert the location only after the append is known to be complete; and
10. increment `stored_bytes` only after the index update succeeds.

The operation must not leave a visible index entry pointing outside the
payload area. If a fallible step fails, state updates must be all-or-none as
far as the selected storage representation permits. A failed append must not
be reported as an inserted object.

### 5.2 Insert with an externally supplied ID

The existing authenticated `put(id, bytes)` behavior remains:

```text
hash(bytes) == id
    -> packed insertion/reuse

hash(bytes) != id
    -> IdentityMismatch
```

This path may hash once to authenticate the caller-supplied ID. It must not
use an unchecked location or bypass incumbent validation.

### 5.3 Read by ID

`get(id)` must:

1. find the location in the index;
2. validate that `offset + length` is within the payload area using checked
   arithmetic;
3. return exactly the indexed byte range;
4. authenticate the returned bytes against `id`; and
5. return `MissingObject` or `IdentityMismatch` using the existing error
   vocabulary.

The read path must not copy the entire packed payload. Returning a borrowed
slice is sufficient for the in-memory benchmark candidate.

### 5.4 Duplicate insertion

Repeated insertion of the same chunk must not append duplicate payload bytes:

```text
first put(chunk)  -> Inserted
second put(chunk) -> Reused
stored_bytes      -> unchanged by second put
pack length       -> unchanged by second put
```

## 6. Layer ownership

Opt2 does not move durable storage into `layerfs-core`.

```text
layerfs-core
├── FastCDC and chunk identity
├── logical content behavior
├── immutable in-memory CAS candidates for tests/evaluation
└── no filesystem or SQLite assumptions

layerfs-engine (later)
├── durable pack carrier
├── pack sealing and publication
├── pack/index lifecycle
├── storage-engine selection
└── SQLite/Postgres/file-backed implementations
```

The packed in-memory candidate belongs beside the existing in-memory CAS
implementation only because it is a controlled core-level experiment. It
must not define a durable pack header, physical locator format, database
schema, or platform capability contract.

The eventual engine-level direct-pack path should preserve the same semantic
shape:

```text
borrowed chunk
    -> authenticate ID
    -> append to private pack carrier
    -> retain fixed-size locator/index metadata
    -> seal/admit through the storage engine
```

That future path must still obey immutable publication and exact incumbent
authentication. Packing is not permission to overwrite an incumbent or skip
identity validation.

## 7. Compatibility requirements

Opt2 must produce the same logical result as the current baseline for the same
source bytes.

The following must remain unchanged:

- FastCDC profile and fragmentation-independent boundaries;
- 5,284 chunks for the current deterministic 100 MiB fixture;
- every chunk ID;
- ordered chunk references;
- logical-file root/manifest identity;
- source fingerprint;
- `Inserted` versus `Reused` semantics;
- missing-object and identity-mismatch errors;
- one-byte edit rejoin behavior;
- range-read bytes;
- maximum chunk size;
- no source-sized staging buffer in the scanner.

The packed physical layout is not part of the logical identity. A packed CAS
and the current CAS must be interchangeable for correctness tests at the
logical content boundary.

## 8. Implementation sequence

### Step 1 — Add the smallest packed CAS candidate

Add the packed representation and its direct unit tests in the existing CAS
owner. Reuse the current `ChunkId`, `PutOutcome`, `CoreError`, and chunk-size
limits. Do not add a new public trait or dependency.

### Step 2 — Add a differential test seam

Run the same source through both CAS implementations and compare:

- emitted chunk IDs;
- logical-file chunk references;
- final root/manifest ID;
- stored logical bytes;
- duplicate/reuse outcomes; and
- range-read results.

The evaluator may select the candidate through a narrow test/benchmark choice.
Do not add runtime automatic selection or production configuration.

### Step 3 — Add stage timing for the candidate

Use the existing Phase 2 ingest timing shape:

```text
source read
CDC
CAS publication
manifest
```

Keep the source file prepared and synced before the timed region. Run the
current and packed candidates against the same APFS source file, in the same
release profile and with the same process/environment collection.

### Step 4 — Measure memory separately from throughput

Record both:

- logical storage: packed payload bytes, index bytes, and logical chunk count;
- host observation: fresh-process RSS high-water mark minus an empty evaluator
  baseline.

Do not claim that packed CAS is production-memory-bounded merely because it
uses fewer allocations. The in-memory candidate still retains all unique
payload bytes by design.

### Step 5 — Decide whether to retain the candidate

Retain the implementation only if it produces a repeatable improvement under
the acceptance gates below. If it does not, record the negative result and
remove the speculative candidate rather than carrying an unmeasured backend
forward.

## 9. Correctness tests

The minimum direct tests are:

| Test | Required assertion |
|---|---|
| Empty CAS | New CAS has zero objects, zero payload bytes, and zero index entries |
| Single insert | ID, bytes, location, count, and stored bytes are correct |
| Duplicate insert | Returns `Reused`; payload length and stored bytes do not grow |
| Multiple inserts | Locations are non-overlapping and monotonically appended |
| Boundary-sized chunks | 0, minimum, target, maximum, and maximum+1 behavior is exact |
| Corrupt incumbent | Returns `IdentityMismatch`; does not silently reuse bytes |
| Wrong supplied ID | Returns `IdentityMismatch`; does not mutate packed state |
| Missing read | Returns `MissingObject` |
| Indexed range | Read returns exactly the stored payload slice |
| Fragmented source | Same boundaries and IDs as one-shot input |
| Full 100 MiB differential | Same 5,284 IDs and logical root as current CAS |
| One-byte edit | Same reuse/create counters and final bytes as current CAS |
| Range materialization | Same prefix, middle, and EOF bytes as current CAS |

Test-only corruption may mutate the private packed representation directly.
Production visibility must remain narrow; do not expose raw pack authority just
to make tests easier.

The tests must also exercise checked offset/length validation. A malformed
location must fail closed rather than panic, wrap, or return bytes outside the
payload area.

## 10. Performance evaluation

### 10.1 Required comparison rows

| Row | Pipeline | Purpose |
|---|---|---|
| A | APFS file → FastCDC → current `InMemoryCas` | Existing baseline |
| B | APFS file → FastCDC → packed in-memory CAS | Opt2 candidate |
| C | APFS file → FastCDC → packed CAS with repeated input | Dedup/reuse behavior |

Row C is a correctness and reuse row, not a replacement for the unique-input
performance row.

Do not include SeqCDC in the Opt2 decision. CDC comparison is a separate
question and changing both the chunker and CAS layout would destroy attribution.

### 10.2 Measurement protocol

For each row:

1. use the same prepared regular APFS source file;
2. use the same release build and compiler settings;
3. run in a fresh evaluator process when collecting RSS;
4. perform at least five timed warm runs after one untimed correctness warm-up;
5. report the median, minimum, maximum, and individual runs;
6. report source, CDC, CAS, and manifest stages separately;
7. record chunk count, stored bytes, index entries, and correctness;
8. record CPU time when available; and
9. retain the exact source fingerprint and source commit metadata.

The baseline and candidate must be measured in the same session window when
possible. APFS cache state must be stated; this is a warm-file comparison, not
a claim about cold storage behavior.

### 10.3 Promotion gates

Opt2 is accepted as a useful optimization only if all of the following pass:

1. every correctness row is true;
2. the candidate produces exactly the baseline chunk count, IDs, and root;
3. median total time improves by at least 5% over 247.795 ms, or is at most
   235.405 ms;
4. median CAS time improves by at least 10% over 117.852 ms, or is at most
   106.067 ms;
5. no candidate run shows a source-sized duplicate payload population;
6. candidate RSS does not increase by more than 10% without a documented
   capacity-growth explanation; and
7. no error, reuse, range-read, or small-edit regression appears.

These are Opt2 promotion gates, not product-level throughput promises. The
500 MiB/s stretch target remains 200 ms per 100 MiB. The 800 MiB/s objective is
not an Opt2 acceptance gate.

### 10.4 Evidence artifact

Write the result under a new run directory, for example:

```text
eval/runs/phase2-opt2-packed-cas-s1-100/
├── environment.json
├── results.jsonl
└── summary.md
```

The summary must include:

- baseline and candidate descriptions;
- exact source and build metadata;
- stage timing table;
- throughput and CPU table;
- chunk and reuse counters;
- logical payload, packed payload, index, and capacity bytes;
- RSS observations and their limitations;
- correctness/differential result;
- whether each promotion gate passed; and
- a decision: retain, revise with one measured follow-up, or reject.

## 11. Expected result and next decision

The expected improvement comes from eliminating thousands of persistent
per-chunk payload allocations and their map values, not from making BLAKE3 or
FastCDC asymptotically different.

If Opt2 passes, the next work is a narrowly scoped direct borrowed-chunk to
packed-sink experiment or an engine-owned durable pack carrier. If it fails,
the result should identify whether the cost is in CDC, the B-tree lookup, pack
append/capacity growth, or benchmark orchestration before another code change.

Do not proceed directly to SQLite based on a packed in-memory result. SQLite
will add its own page-cache, transaction, BLOB, journal, and synchronization
costs and requires a separate storage-engine benchmark.

Do not switch to SeqCDC as a response to a failed pack experiment. The prior
evidence shows that scanner-only SeqCDC gains can be lost when chunk count and
downstream CAS work increase.

## 12. Completion checklist

- [x] Candidate uses the existing FastCDC profile and chunk IDs.
- [x] Candidate keeps the existing `BTreeMap` lookup for attribution.
- [x] New chunks append to one packed payload area.
- [x] Duplicate chunks do not append again.
- [x] Every location is checked and authenticated on read.
- [x] Checked arithmetic and fail-closed malformed-location behavior exist.
- [x] Current CAS and packed CAS produce identical logical results.
- [ ] Fragmentation and small-edit differential tests pass through the packed content path (deferred; current A/B is full-ingest differential).
- [x] Five-run APFS comparison is recorded.
- [ ] RSS and logical storage are reported separately per engine (combined-process RSS was captured; per-engine isolation is deferred).
- [x] All measured correctness and timing gates are explicitly reported below.
- [x] The result is recorded before moving to direct sink or SQLite work.

## 13. Measured implementation result

Run artifact: `eval/runs/phase2-opt2-packed-cas-s1-100/`.

Environment: macOS 26.4.1, arm64 Mac15,10, APFS Data volume, release build,
warm source-file comparison. The source was 100 MiB and produced 5,284 unique
chunks. The A/B command performed one untimed warmup and five measured runs per
engine. Every measured row reported `correct=true` and
`differential_correct=true`.

| Engine | Median total | Median throughput | Median CDC | Median CAS |
|---|---:|---:|---:|---:|
| `InMemoryCas` | 169.912 ms | 588.5 MiB/s | 88.896 ms | 71.951 ms |
| `PackedInMemoryCas` | 180.462 ms | 554.1 MiB/s | 91.010 ms | 80.356 ms |

The packed candidate was 6.21% slower by median total time and 11.68% slower
in the CAS stage. Therefore the Opt2 performance promotion gate failed even
though the correctness gate passed. The result points to the initial packed
`Vec<u8>` append/capacity and location-management path as a follow-up target;
it does not justify changing CDC or moving to SQLite.

The combined evaluator process reported a maximum RSS of 760,053,760 bytes
under `/usr/bin/time -l`. That observation covers both engines, six warm/measured
pairs, allocator retention, and verification, so it is not a per-engine RSS
claim. A separate fresh-process RSS run is required before making a memory
promotion decision.

## 14. Corrected shared-path and clean-lane findings

The first result above was produced by the earlier evaluator-local packed loop.
It is retained as historical evidence, but it is not the final comparison
because the candidate did not use the same `LogicalFile` full-ingest path as the
baseline. The corrected comparison routes both arms through the same content
algorithm and differs only at the CAS publication callback. The public API was
then narrowed again: normal content operations remain concrete over
`InMemoryCas`; packed qualification uses only concrete full-replace wrappers.
No public storage-provider trait was added.

### Clean throughput lane

Discovery run artifact: `eval/runs/phase2-opt2-clean-s1-100/`.

This run uses one outer timer around untimed `full_replace`, one warmup, and five
measured runs per engine. It does not call `Instant` for every source read or
chunk, so it is the throughput result. The 100 MiB source was created and
synced on the local APFS volume. Every row passed correctness and differential
checks.

| Engine | Median outer | Median throughput | Result |
|---|---:|---:|---|
| `InMemoryCas` | 224.876 ms | 444.7 MiB/s | baseline |
| `PackedInMemoryCas` (pre-sized) | 222.752 ms | 448.9 MiB/s | 0.94% faster |

The packed candidate produced the same 5,284 chunk references, counters, stored
logical bytes, and reconstructed BLAKE3 output. Its payload was pre-sized to
104,857,600 bytes and recorded zero payload reallocations and zero estimated
growth-copy bytes.

The non-pre-sized corrected shared-path run is in
`eval/runs/phase2-opt2-shared-s1-100/`: packed median was 241.743 ms versus
230.739 ms for the baseline, or 4.77% slower. Pre-sizing therefore removes the
observed growth cost, but it does not establish a material packed-CAS win.

A final clean rerun on the same source/build shape is in
`eval/runs/phase2-opt2-clean-final-s1-100/`:

| Engine | Median outer | Median throughput | Result |
|---|---:|---:|---|
| `InMemoryCas` | 237.890 ms | 420.4 MiB/s | baseline |
| `PackedInMemoryCas` (pre-sized) | 237.674 ms | 420.7 MiB/s | 0.09% faster |

The 420–449 MiB/s spread between these two clean sessions shows ordinary host
and cache variance. The packed arm stayed within 0.09–0.94% of the paired
baseline, far below the 5% promotion gate; the result is parity, not a speed
claim.

### Index A/B

A temporary packed-only `HashMap<ChunkId, ChunkLocation>` arm was compiled and
measured against the unchanged `BTreeMap` baseline. The run is in
`eval/runs/phase2-opt2-presized-hash-s1-100/`; it measured 226.991 ms for the
HashMap packed arm. The prior pre-sized BTreeMap packed run measured 223.907 ms
in its own paired session. Because the sessions differ and the difference is
small, this is not evidence for a HashMap promotion. The experiment was
reverted, and the packed candidate keeps the BTreeMap index so Opt2 remains
attributable to payload layout.

### Correctness coverage

The core suite now has a packed full-replace differential test that checks:

- identical chunk references and edit counters against `InMemoryCas`;
- identical reconstructed bytes;
- duplicate full-ingest returns `Reused` for every existing chunk;
- duplicate full-ingest does not increase packed payload length; and
- the existing CAS tests continue to cover corrupt locations, wrong IDs,
  missing objects, bounds, and exact indexed reads.

The packed content path still does not claim generic range/edit API parity;
those APIs remain owned by the concrete Phase 1 `InMemoryCas` path until a
durable engine contract requires them.

### Memory interpretation

`/usr/bin/time -l` reported 234,553,344 bytes maximum RSS for the clean A/B
process. That process ran both engines, warmups, measured rows, verification,
and allocator cleanup in one process, so it is a combined observation rather
than a per-engine memory number. It is not evidence that packed CAS is
memory-bounded: both candidates retain the full unique 100 MiB payload. Fresh
one-engine processes are still required for a per-engine RSS comparison.

### Decision

Do not promote packed CAS as the next speed optimization. Keep the small
implementation and tests as a qualification candidate because they prove the
packed representation is semantically viable and pre-sizing avoids its growth
penalty. The next performance work should measure the content handoff and CDC
cost with a clean, non-retaining sink or direct aggregate counters before
changing FastCDC or adding a durable storage engine. The preserved evidence
does not support switching to SeqCDC or claiming 800 MiB/s.

## 15. Research handoff for the next experiment

The parallel review of the previous LayerFS implementation found that its
complete content path has more work than this Phase 2 core benchmark:

```text
source window -> CDC ring -> FastCDC boundary
    -> logical chunk hash/ID
    -> canonical physical chunk encode/hash
    -> object sink
    -> reference spool
    -> canonical file object/hash
    -> tree/version/closure/CAS publication/handoff
```

The previous evidence has no complete-operation 800 MiB/s result. It reports
approximately 330.7 MiB/s for an isolated 64 MiB scanner anchor, 12.4 MiB/s
for a complete 8 MiB PRNG storage path, and 64.8 MiB/s for a complete repeated
content path. A historical two-hash model was approximately 520 MiB/s, but it
excluded source, sink, pack, CAS, and lifecycle work. Therefore 800 MiB/s is an
aspiration, not a current acceptance target.

The smallest next experiment is a test-only sink comparison at the existing
object-sink boundary in the previous implementation: keep CDC, logical and
physical hashing, canonical encoding, and the reference spool identical, but
discard object payloads immediately after the sink receives them. Compare the
exact chunk lengths, logical IDs, physical IDs, file ID, counters, and timing.
This will tell us whether sink/storage retention is material before changing
the frozen FastCDC profile or designing SQLite/pack persistence. It is not yet
implemented in this empty rebuild because that sink seam belongs to the later
engine lifecycle, not to the current Phase 2 `layerfs-core` API.
