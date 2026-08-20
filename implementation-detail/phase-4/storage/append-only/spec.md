# Phase 4B Specification — Append-only Carrier, Disk-backed Index, and One Commit Marker

Status: measured implementation candidate; not a Phase 4A replacement
Rollback status: **rejected and superseded for active implementation** by
[`../../rollback/spec.md`](../../rollback/spec.md).
The requirements and measurements below are retained as historical evidence;
they do not authorize a carrier implementation or promotion.
Controlling baseline: [Phase 4A SQLite BLOB](../sqlite/spec.md)
Qualification platform: macOS on APFS

This document defines the smallest user-authorized Phase 4B candidate. It does
not change Phase 1 canonical objects, Phase 2 CDC, Phase 3 COW or delta
semantics, and it does not replace the Phase 4A reference until the candidate
passes the decision gate in this document.

The design is intentionally narrower than a general storage engine:

```text
one append-only store log
  ├── immutable canonical object frames
  ├── immutable disk index-page frames
  ├── root and delta frames
  └── one durable commit marker per capture
```

The log is the carrier. The index is persisted in the log and accessed through
a bounded page cache. The last valid commit marker is the only durable visible
root. There is no user-visible rollback operation, rollback journal, WAL,
checkpoint file, GC, compaction, repack, or hidden worker.

## 1. Decision and non-goals

### 1.1 Why this candidate exists

Phase 4A is the control implementation. A Phase 4B candidate is justified only
when direct counters attribute a material share of fresh capture time or bytes
to SQLite BLOB page handling, SQLite rollback-journal writes, or SQLite
statement/transaction overhead. A faster design is not justified merely because
its isolated scanner is faster.

The candidate removes the database and its journal from the object hot path. It
does not remove the durability fence: the commit marker is the one publication
record that makes a capture visible after reopen.

### 1.2 In scope

- immutable authenticated canonical-object publication and reuse;
- a disk-backed object locator index with bounded application memory;
- exact bounded object range reads;
- durable root and delta records;
- one atomic capture publication per capture;
- close/reopen recovery after clean and interrupted writes;
- direct counters for CDC, object, index, carrier, sync, and recovery work;
- engine-only APFS benchmark rows and an A/B decision against Phase 4A.

### 1.3 Explicitly out of scope

- a logical rollback API or user-facing version restore operation;
- SQLite, PostgreSQL, WAL, rollback journals, checkpoint records, or a VFS;
- truncating, rewriting, or replacing old carrier/index pages;
- GC, compaction, free-space recycling, repacking, or carrier rotation;
- an in-memory map of all object IDs or all object bytes;
- source-sized ingest buffers, a full-pack staging buffer, or a full-index
  staging buffer;
- background workers, Rayon, async runtimes, hidden queues, retry storms,
  prefetch, or a general connection/thread pool;
- OS projection, FUSE, native materialization, SDK, or network service work.

Unreachable bytes after an interrupted or failed capture are retained. This is
the intentional cost of append-only publication without rollback or
compaction. The store-size consequence must be measured and reported; a later
retention/compaction milestone may address it.

## 2. Contracts that must not change

The carrier and index are physical storage details. They must consume the
existing semantic contracts rather than defining new versions of them.

### 2.1 Canonical objects and identity

- The Phase 1 `LFSO` envelope, exact canonical bytes, object kinds, decode
  limits, and exact-end-of-input rule remain authoritative.
- The object ID remains the typed 32-byte BLAKE3 identity over the canonical
  bytes and existing hash domain.
- A root handle is not silently converted into a new canonical root object.
- Carrier offsets, index page offsets, generation numbers, file paths, record
  sequence numbers, and commit markers never enter object identity.

### 2.2 CDC, content, COW, and delta

- The existing Phase 2 CDC profile and streaming source boundary remain fixed.
- The existing Phase 3 content-tree shape, authenticated references, COW
  mutation behavior, delta ordering, and parent/child meaning remain fixed.
- A small edit may reuse authenticated unchanged objects, but the carrier must
  not claim edit-sized work when the selected content format requires a larger
  suffix or closure traversal.

### 2.3 CAS authority

Installed objects are immutable. The index is a locator accelerator, not an
integrity authority:

1. authenticate the canonical bytes before trusting a newly supplied object;
2. use the disk index to find an incumbent locator;
3. read and authenticate the incumbent canonical bytes before returning or
   reusing them, unless a bounded verified-locator cache has a valid entry for
   the exact immutable carrier identity and byte range;
4. reuse only an authenticated equal occupant;
5. return a typed unequal, malformed, missing, replaced, inaccessible, or
   generic I/O error as appropriate;
6. never overwrite an incumbent in place.

The verified-locator cache is optional, bounded, and observable. It may reduce
repeat hashing within a stable open handle, but it must never become an
unbounded object cache or turn a matching index key into implicit trust.

## 3. Lessons from the previous LayerFS failure

The previous LayerFS experiment is useful precisely because it separated a
fast scanner from a slow complete durable path. The evidence is retained in
the older repository and historical documentation, not copied as production
implementation:

- historical benchmark: `/Users/yifanxu/Ephemeral-AI-Lab/ephemeral-sandbox-docs/ephemelra-sanbdox-v2.1/layefs/l1.5/benchmark.md`;
- complete-path evidence: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/tests/evidence/c3-qualification-evidence.json`;
- representative rows: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/tests/evidence/c3-complete-prng-8m.jsonl` and `c3-complete-repeated-8m.jsonl`;
- the earlier Rust/SQLite versus Node/SQLite study:
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-sqlite-techstack-experiment`.

### 3.1 Measured complete-path failure

Representative 8 MiB PRNG medians from the old complete path were:

| Candidate | Wall time | Objects | Direct CAS reads | CAS bytes read | CAS bytes written | Read amplification |
|---|---:|---:|---:|---:|---:|---:|
| NF | 691.379 ms | 433 | 35,027 | 86.065 MiB | 8.465 MiB | 10.260× |
| OF | 645.519 ms | 433 | 35,027 | 86.065 MiB | 8.465 MiB | 10.260× |
| OS | 893.361 ms | 645 | 53,500 | 87.252 MiB | 8.502 MiB | 10.401× |

Repeated-content medians were much lower: NF 169.863 ms, OF 122.799 ms, and
OS 113.641 ms. This contrast shows that authenticated reuse was comparatively
cheap while first publication and metadata work were expensive.

The old 64 MiB scanner anchor reported approximately 327.985 MiB/s for OF and
347.459 MiB/s for OS. Those rows did not prove the complete durable path: the
evidence recorded no file or directory sync calls and excluded the full pack,
CAS admission, closure validation, COW/root work, and publication authority.
They must not be used as the Phase 4B ingest result.

### 3.2 Root causes

The old complete path amplified work in several places:

1. Per-object filesystem metadata operations multiplied open/stat/read/close
   and locator/catalog work.
2. Carrier and locator validation was repeated across layers instead of being
   represented by direct immutable offsets.
3. The benchmark mixed the fast streaming scanner with complete-C3 work such as
   admission, index construction, closure validation, COW trees, and root
   publication.
4. The OS candidate produced 645 objects instead of OF's 433, so every
   object-level operation was multiplied before the scanner advantage mattered.
5. The old durable evidence did not pay the durability sync cost, so it could
   not predict a crash-safe result.

### 3.3 Required prevention in this design

| Previous failure | Phase 4B prevention | Direct evidence required |
|---|---|---|
| Per-object path metadata | One store file, one append stream, direct offset/length locators, no per-object files | file-open/stat/close counters; carrier write calls |
| Repeated locator/catalog work | One disk index lookup followed by one bounded carrier read | index probes, page-cache hits/misses, carrier reads |
| Scanner-only speed claim | Time CDC, object publication, index, roots/deltas, marker sync, and reopen together | phase counters sum to wall/CPU time |
| Object-count amplification | Use the frozen CDC profile and report created/reused counts before comparing layouts | chunk/object counts and reuse ratio |
| Missing durability evidence | Append the marker and perform one measured durability sync before success | marker-sync count, sync bytes/time, reopen result |
| Memory hidden in staging | Keep carrier/index writes streaming with fixed buffers and bounded pages | logical cache high-water, RSS/PSS, temp bytes |

### 3.4 Earlier Rust + SQLite versus Node + SQLite result

The earlier `layerfs-sqlite-techstack-experiment` reported the useful
engineering signal that a Rust + SQLite path could reach roughly 300 MiB/s
where the corresponding Node + SQLite path was roughly 100 MiB/s under that
experiment's conditions. That result is not contradicted by the old LayerFS
failure:

- the SQLite study compared language/runtime and binding overhead for a
  controlled SQLite-shaped operation;
- the old LayerFS failure was dominated by the physical layout's per-object
  filesystem and validation work on the complete path;
- changing Rust versus Node cannot remove a metadata-amplifying storage design;
- Phase 4B must compare equal correctness, durability, workload, cache state,
  and operation boundaries before attributing a gain to the carrier.

The earlier result is background motivation, not an acceptance number for this
design.

### 3.5 Current Phase 4A baseline and WAL lesson

The current Phase 4A diagnostic run provides a second useful boundary. With the
fixed DELETE/FULL/FILE/mmap=0 profile, the observed medians were approximately:

| Row | DELETE/FULL | Main observed cost |
|---|---:|---|
| P4-I1 fresh ingest | 879.296 ms | publication ~720 ms; commit ~152 ms |
| P4-I2 repeated ingest | 830.913 ms | publication ~829 ms; commit ~1.5 ms |
| P4-R1 reopened full read | 497.043 ms | durable read/reconstruction path |
| P4-R2 reopened ranges | 4.269 ms | bounded range path |

A temporary WAL/FULL experiment did not improve the fresh ingest row:

| Row | WAL/FULL diagnostic | Difference from DELETE/FULL |
|---|---:|---:|
| P4-I1 fresh ingest | 1,027.187 ms | ~16.8% slower |
| P4-I2 repeated ingest | 820.681 ms | roughly similar |
| P4-R1 reopened full read | 412.770 ms | workload/cache-sensitive |
| P4-R2 reopened ranges | 3.663 ms | workload/cache-sensitive |

These numbers are diagnostic rather than a clean-source gate: the recorded
benchmark artifacts used an older source commit (`cb80edb950ac538e122d6496e8ecedbff4d53a95`)
and a dirty tree. The WAL result nevertheless establishes the design rule for
this candidate: changing SQLite journal mode is not the optimization target for
fresh durable ingest. The append-only design should remove page/journal churn,
retain one real durability sync, and prove the improvement with the phase
counters above.

## 4. Physical format

### 4.1 One append-only log

The first implementation uses one regular file, for example `store.log`, on
the qualified APFS volume. It is opened once per engine handle and is appended
to by one synchronous writer. The logical components are record types in this
one file rather than independently renamed files:

```text
store.log
  ├── ObjectFrame
  ├── IndexPageFrame
  ├── RootFrame
  ├── DeltaFrame
  └── CommitMarker
```

Keeping the components in one carrier avoids the false atomicity of renaming a
carrier and index separately. A split-file implementation is not eligible for
the first Phase 4B gate unless it proves equivalent group visibility and
recovery behavior.

Every frame has a versioned physical header, a checked payload length, a frame
kind, a generation/sequence field, and a checksum sufficient to distinguish a
complete frame from a torn or malformed tail. Physical framing is private to
the engine. It must not be confused with or substituted for the Phase 1
canonical object envelope.

The current candidate fixes the following private encoding: a 72-byte
big-endian header (`L4AO`, version 1, kind, checked payload length, generation,
previous-valid-frame offset, physical frame offset, and BLAKE3 checksum), an
8-byte-aligned frame extent, and a 64 MiB maximum payload. The checksum covers
the first 40 header bytes and the payload. The physical frame offset binds a
header to its location; the previous-valid-frame offset binds the recovered
frame sequence.

Frames are aligned to a fixed storage boundary. A crash may leave an incomplete
tail frame. Recovery does not truncate. It scans complete candidates in order,
validates the checksum and predecessor link, and counts complete valid frames
after the last marker as unreachable residue. If a malformed, checksum-invalid,
or wrong-predecessor candidate occurs after an already valid marker, recovery
retains that previous marker, classifies the remaining suffix as residue, and
stops; it does not skip corrupt bytes to discover a later marker. A malformed or
integrity failure before the first valid marker, or a permission, generic I/O,
short-read, no-space, or checked-overflow failure at any point, is returned as
its typed error. The next append never overwrites residue. A later writer may
only proceed after a cleanly parseable suffix. An unrecoverable torn or corrupt
suffix therefore poisons the reopened writer handle while retaining the
previous marker for inspection. Salvage/truncation is outside this candidate.

### 4.2 Object frames

An `ObjectFrame` stores:

```text
object_id
object_kind
canonical_length
canonical_object_bytes
frame_integrity
```

The canonical bytes are copied exactly once from the core-owned representation
into the append write path. The locator recorded in the index contains the
frame offset and the exact canonical byte range needed for a read. An object
frame is never edited after append. A range read first streams the complete
canonical object through identity and semantic validation, then reads only the
requested exact range into the caller's output buffer. Authentication bytes and
returned range bytes are counted separately.

The implementation must not stage all frames for a capture. At most the
current bounded canonical object, the fixed write buffer, and bounded index
working pages may be resident in application memory.

### 4.3 Disk-backed index

The candidate uses an immutable fixed 256-bucket root followed by immutable
collision pages, all stored as `IndexPageFrame` records in the same log. The
bucket is the first byte of the existing 32-byte object ID. Each page contains
one object ID, its object-frame offset, kind, canonical length, and the next
page offset. A capture appends one page for each new object and one 2 KiB
`IndexRootFrame` containing the 256 bucket heads; old pages are never edited.

This is intentionally simpler than the proposal's copy-on-write B-tree. Object
IDs are immutable and lookups already have a full 32-byte key, so a B-tree's
split/parent-version machinery would add write amplification and recovery
states without reducing the required object authentication. The trade-off is a
collision-chain lookup: the implementation bounds traversal at
`MAX_INDEX_VISITS` and reports the chain cost. A later measured workload may
replace this shape, but it must prove lower index I/O without adding a full
catalog map or a second publication protocol.

The complete index is never loaded into memory. Lookups read the selected root
and collision pages with offset reads. The only resident index state is the
fixed 256-entry root held by the active capture and a 32-page LRU cache of
decoded immutable collision pages. The root is constant-size; the cache has
fixed capacity and reports hits, misses, and evictions. There is no capture-wide
index batch, sort, spool, or full-index negative map in this candidate.

### 4.4 Roots and deltas

`RootFrame` and `DeltaFrame` contain the authenticated semantic references
needed to reopen Phase 3 state:

| Frame | Required information |
|---|---|
| RootFrame | root identity/handle, parent root, referenced authenticated tree object, exact frame bounds |
| DeltaFrame | delta identity, parent root, child root, ordered delta payload, exact frame bounds |

The current implementation authenticates the exact existing Phase 3
`RootRecord` and `DeltaRecord` fields; it does not serialize Rust memory layout
and does not enter object identity. It does not invent a new logical root
encoding. Full semantic-root persistence therefore remains bounded by the
existing Phase 3 persistence boundary and is an explicit qualification item,
not a claim made by this carrier candidate.

## 5. Commit-marker publication protocol

### 5.1 Visibility rule

The last valid `CommitMarker` is the durable visible head. No root or delta is
visible merely because its frame exists in the log. Unmarked object/index/root/
delta frames are durable residue and may be reused only after normal
authentication; they do not advance the visible root.

The marker is a publication record, not a logical rollback feature, backup
checkpoint, SQLite journal, or periodic checkpoint. There is exactly one
marker appended per successful capture attempt that reaches the durable
publication point.

### 5.2 Marker contents

A marker must bind, at minimum:

```text
format/profile identifier
generation
previous_marker_offset
parent_root_id
child_root_id
delta_id and delta_frame_offset
index_root_offset
visible_log_end_offset
capture record range and byte counts
authenticated capture digest
marker checksum
```

The marker must not contain a guessed root or an unverified index pointer. All
referenced frames are validated before the marker is appended.

The current benchmark is deliberately narrower than this contract: it is a
scanner/admission diagnostic whose committed root is an empty directory plus a
fixed delta. It is not a full logical-content workload and must not be used in
a Phase 4A comparison until the committed root references the complete source
object graph and reopen verifies that closure.

For a live capture, the candidate maintains a domain-separated BLAKE3 digest
incrementally over every successfully appended gap, frame header, payload part,
and padding byte in the capture range. Marker publication finalizes that
bounded state; it does not reread the capture. Reopen independently recomputes
and authenticates the committed range, so the optimization does not remove
recovery evidence.

### 5.3 One-capture sequence

The operation is one logical capture and one caller-thread transaction:

```text
acquire the single writer authority
  -> load and authenticate the current visible marker/root
  -> validate the requested parent root
  -> stream source through the existing CDC/content path
  -> authenticate each new object
  -> index lookup; authenticate and reuse equal occupants
  -> append new ObjectFrames sequentially
  -> append changed IndexPageFrames and the new index root
  -> append authenticated DeltaFrame
  -> append authenticated RootFrame
  -> append exactly one CommitMarker
  -> flush userspace buffers
  -> perform exactly one measured `File::sync_all` durability sync
  -> return the new visible root
```

The marker is appended before the sync so one sync covers all preceding data
and the publication decision. No object, index page, root, or delta receives an
individual sync. The in-process visible head is updated only after flush and
`sync_all` succeed. A successful capture is returned only after that sync
succeeds. The current macOS/APFS profile uses Rust's portable `File::sync_all`;
it does not claim the stronger, platform-specific `F_FULLFSYNC` contract.

If flush or the final sync reports an error, return a typed durability/I/O
failure. The caller must treat visibility as ambiguous until reopen confirms
the last valid marker. The handle is poisoned after either failure, the old
in-process head remains visible, and the engine must not silently retry or
create a second marker on that handle. Reopen may expose the new marker when
the sync outcome was ambiguous; it must authenticate the marker chain before
doing so.

### 5.4 Failure and reopen behavior

| Failure point | Visible root after reopen | Allowed residue |
|---|---|---|
| Parent validation fails | Previous root | None from this attempt, unless prior caller work already appended residue |
| During object/index append | Previous root | Complete and partial unmarked frames |
| After root/delta append, before marker | Previous root | Complete root/delta and object/index frames, all unreachable until separately authenticated and referenced |
| Marker is torn or checksum-invalid | Previous valid marker | Torn marker tail and any later unmarked bytes |
| Marker is valid and durable | Child root | All marker-referenced frames must validate; otherwise return corruption and do not expose the child |
| Disk full before marker | Previous root | Typed no-space residue; no rollback/truncate |
| Sync failure after marker append | Reopen decides from marker validity | Operation returns typed durability ambiguity; handle is poisoned; no retry storm |

On reopen, the engine scans aligned frames, validates frame lengths/checksums,
and selects the newest valid marker whose parent chain and referenced root,
delta, index root, and object records authenticate. Frames after that marker
are not visible. Reopen must not infer a root from the last bytes in the file,
from an unmarked root frame, or from file length alone.

The first implementation may leave residue in place. It must never claim that
failed captures were rolled back physically.

## 6. Memory, I/O, and resource bounds

### 6.1 Required bounded state

The engine may hold:

- the existing bounded CDC ring and current canonical object buffer;
- one fixed carrier write buffer;
- a fixed-capacity index page cache;
- a fixed-capacity verified-locator cache, if enabled;
- one bounded index-update batch;
- fixed metadata, statement-free handles, and counters.

It may not hold:

- the source file, full logical file, full capture, full carrier, or full index;
- one decoded copy of every object in a capture;
- an unbounded page cache or object cache;
- a memory map whose populated pages replace the declared memory bound;
- a hidden temporary buffer that grows with source size.

If index updates exceed the batch bound, use a file-backed spool with explicit
temporary-storage counters and cleanup custody. A bounded logical-memory result
does not imply a bounded APFS page cache or RSS/PSS; report those separately.

### 6.2 Fast-path I/O rules

The baseline implementation shall:

1. open the log once and use sequential buffered appends;
2. avoid per-object open/stat/close, directory lookup, locator file creation,
   and catalog marker creation;
3. batch object and index writes per capture;
4. use direct bounded offset reads for index pages and object ranges;
5. avoid a full-object read on a range request;
6. perform one durability sync per capture, never one per object;
7. count every append write, indexed read, carrier read, and sync directly;
8. keep the writer synchronous and caller-thread based.

No optimization may bypass object authentication, parent validation, marker
ordering, or exact range bounds.

## 7. Typed errors and authority rules

The engine preserves distinct errors for at least:

- parent-root conflict or stale parent;
- missing, malformed, unequal, replaced, or inaccessible object;
- invalid index/page/root/delta/marker integrity;
- invalid range, short read, generic read/write, and no-space failure;
- writer busy/lock conflict with bounded waiting;
- durability sync failure and durability ambiguity;
- checked size, offset, count, and frame-length overflow.

An index key match is not a successful reuse by itself. A marker is not valid
until every referenced frame and semantic object has passed the required
authentication. A failure after append retains custody of residue and does not
delete by stale path or truncate based on an unverified offset.

The store has one writer authority. The candidate uses the already-locked
`fs2` 0.4.3 dependency for a narrow nonblocking advisory file lock because the
Rust standard library has no portable advisory-lock API. A second
engine/process receives `CarrierBusy`; there are no retries or hidden workers.
The current public open is writer-capable and therefore takes that exclusive
lock even for inspection. A separate bounded read-only open path is deferred:
P4B-C1 (one writer with bounded readers) is explicitly NOT QUALIFIED and is not
a concurrency-throughput claim. Readers must not observe an index root or root
frame that is not reachable from the last valid marker.

## 8. Direct instrumentation

Every capture and read benchmark records phase-level counters. At minimum:

```text
source_read_calls, source_bytes
cdc_ns, cdc_bytes, chunk_count
canonical_encode_ns, object_hash_ns
index_lookup_calls, index_page_reads, index_page_cache_hits, index_page_cache_misses
index_lookup_ns
verified_reuse_cache_hits, verified_reuse_cache_misses
object_created, object_reused, object_bytes_authenticated
object_auth_ns, carrier_append_ns, carrier_bytes_written
object_frame_bytes_written, index_frame_bytes_written
index_page_append_calls, index_bytes_written
root_bytes_written, delta_bytes_written, marker_bytes_written
carrier_flush_calls, carrier_flush_failures
carrier_flush_ns, marker_sync_calls, marker_sync_successes, marker_sync_failures
marker_sync_ns
carrier_range_reads, carrier_bytes_read
reopen_frame_scan_ns, reopen_marker_candidates, reopen_residue_bytes
logical_cache_high_water, temporary_storage_high_water, RSS/PSS or unavailable
wall_ns, cpu_ns, correct, reopened_correct
```

The report must show the accounting relationship:

```text
total log growth = object bytes + index bytes + root bytes + delta bytes
                   + marker bytes + alignment/padding bytes
```

It must also report write amplification against logical canonical bytes and
read amplification against requested range bytes. Counters are collected at the
engine boundary; they are not reconstructed from elapsed time.

## 9. Performance experiment and optimization order

### 9.1 Required A/B rows

Run Phase 4A and Phase 4B against the same deterministic datasets, release
profile, APFS volume, CDC profile, correctness oracle, and cache protocol:

| Row | Workload |
|---|---|
| P4B-I1 | fresh 100 MiB full ingest, new objects |
| P4B-I2 | repeated 100 MiB ingest with authenticated reuse |
| P4B-R1 | full read after close/reopen |
| P4B-R2 | random exact bounded ranges after close/reopen |
| P4B-E1 | one-byte edits at 16, 100, and 512 MiB logical sizes |
| P4B-E2 | edit at beginning, middle, and end |
| P4B-C1 | one writer with bounded readers |
| P4B-X1 | crash/fault injection at every publication boundary |

Each timing row has one warm-up and at least three measured iterations, with
median and spread. `reopened` means process/database reopen without claiming a
cold OS cache. A scanner-only row is diagnostic and cannot qualify P4B.

### 9.2 Optimize in this order

Measure each change against the same source fingerprint. The expected highest
value order is:

1. remove per-object filesystem metadata and replace it with sequential carrier
   appends;
2. batch the object and index writes so there is one append phase per capture;
3. eliminate redundant index probes and repeated page reads with the bounded
   page cache;
4. tune index page size and write-buffer size using the APFS rows;
5. use the bounded verified-locator cache only if its hit rate and correctness
   benefit outweigh its memory cost;
6. reduce index page-version/write amplification by batching touched paths;
7. only then inspect CDC/hash/encoding costs, because carrier speed cannot fix a
   core algorithm that rescans the full source unnecessarily;
8. only add a disk-resident negative filter or a different index shape if
   counters show index misses dominate and the change wins without an
   unbounded memory or false-negative risk.

Do not start with mmap, threads, async I/O, prefetch, compression, compaction,
or a larger cache. They change several variables at once or hide the storage
cost the experiment is meant to measure.

### 9.3 Provisional selection gate

The candidate can replace Phase 4A for a subsequent milestone only when all of
the following are true on a clean source fingerprint:

1. all correctness, immutability, crash/reopen, exact-range, and typed-error
   tests pass;
2. fresh durable ingest has a material median improvement over Phase 4A on the
   target 100 MiB row; the proposed review threshold is at least 20% lower wall
   time, subject to confirmation on the measured host;
3. repeated ingest, reopen reads, and range reads do not regress by more than
   the review noise budget, proposed as 10% until the benchmark has a recorded
   spread;
4. the direct counters show the improvement came from removed database/
   metadata/journal work rather than omitted durability or correctness work;
5. logical memory remains within the declared cache bound and temporary residue
   is reported;
6. the candidate does not introduce hidden workers, retries, full-index memory,
   or a new canonical/data-format contract.

If the candidate does not pass, keep Phase 4A as the only implementation and
record the measured reason. Do not leave two unqualified production paths.

## 10. Correctness and recovery tests

The smallest load-bearing test set must include:

### Format and object tests

- exact canonical bytes and IDs match Phase 1;
- duplicate equal objects reuse the authenticated locator;
- unequal IDs/bytes, malformed frames, tampered frames, and wrong lengths
  fail with typed errors;
- carrier offsets and index locators round-trip after close/reopen;
- object reads and ranges are exact at zero, middle, end, empty, and bounds.

### Index tests

- empty index and one-entry index;
- page split, multi-level root, collision/adjacent keys, and repeated updates;
- index page cache eviction still returns correct bytes;
- index corruption is detected before a root is exposed;
- no test loads the entire index merely to make lookup convenient.

### Publication tests

- parent mismatch writes no marker and leaves the old root visible;
- failure after object append leaves the old root visible after reopen;
- failure after index, delta, and root append leaves the old root visible;
- torn marker and invalid marker checksum are ignored;
- a valid marker exposes exactly its authenticated child root and delta;
- disk-full and sync faults return typed errors without a retry storm;
- unreachable residue is measurable and is not falsely reported as rollback.

### Resource and performance tests

- source ingest remains streaming with a fixed logical memory bound;
- no per-object file open/stat/close or per-object sync occurs;
- one successful capture has exactly one marker and one durability sync;
- direct counters have nonzero work for nontrivial rows;
- wall, CPU, CDC, object, index, carrier, marker, and reopen costs reconcile;
- clean and reopened handles produce identical roots, deltas, bytes, and errors.

Fault injection must occur at the actual append, frame validation, marker
append, and sync boundaries. Timing sleeps are not evidence of crash safety.

## 11. Implementation order if Phase 4B opens

1. Freeze the engine-private frame and marker encoding, including checked sizes,
   alignment, checksums, and the reopen scanner.
2. Implement a carrier-only append/read path with authenticated object tests.
3. Add the disk-backed immutable index and direct lookup/range counters.
4. Add root/delta frames and the one-marker publication protocol.
5. Add close/reopen and fault-injection tests before performance tuning.
6. Run P4B rows against the unchanged Phase 4A baseline.
7. Tune one bounded variable at a time: page size, write buffer, batch bound,
   then verified-locator cache.
8. Record the A/B decision. Keep only the selected path for the next milestone.

This user-authorized candidate is open for measurement, but it is not a
production replacement. The Phase 4A decision record must still explicitly
select Phase 4B before a later milestone removes or demotes the reference
implementation.
