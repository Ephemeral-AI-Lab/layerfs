# Files

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Turning bytes into file objects and back: representation by length,
content-defined chunking, complete-file construction and bounded reads.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-content` |
| Source | [`file/`](../../../../core/crates/layerfs-content/src/file), [`contract/policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| Uses | [Objects](../objects/README.md) |
| Used by | [Filesystem trees](../filesystem-trees/README.md), [Compare and merge](../compare-and-merge/README.md), [Engine: Project Init](../../engine/project-init/README.md), [Engine: Commit](../../engine/commit/README.md), [Engine: Store calls](../../engine/store-calls/README.md) |
| Flows | [A Commit](../../flows/commit.md), [Project Init](../../flows/init.md) |

## Picture

```text
      bytes, Read stream or                 base root + ordered edits
      data/zero runs                        + replacement source
            |                                        |
            v                                        v
 +-----------------------------+      +-----------------------------+
 | Construction                |      | Edits (edits.md)            |
 |  pick representation by len |      |  no-op -> base root         |
 |  whole file / empty / chunk |      |  split / scan / concatenate |
 +-----------------------------+      +-----------------------------+
            |        |                       |            |
            |        v                       v            |
            |   +---------------------------------+       |
            |   | FastCdc: boundaries, zero runs, |       |
            |   | advisory predecessors (hints)   |       |
            |   +---------------------------------+       |
            v                 v                           v
 +-------------------------------------------------------------------+
 | ExtentBuilder (mapping): chunks, extent pages, file state         |
 |  --> caller's FinalizedConsumer, children before parents          |
 +-------------------------------------------------------------------+
 | Reads: caller's AuthenticatedObjects --> classify root -->        |
 |  navigation waves (32 pages) --> payload waves (32 chunks, 1 MiB) |
 |  --> sink, in logical order                                       |
 +-------------------------------------------------------------------+
```

The module opens no database, pack or file
([`lib.rs`](../../../../core/crates/layerfs-content/src/lib.rs)).

## What it does

### Representation

A file is one root object ID plus a logical length. The policy picks the
representation from the length
([`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs)).

| Length | Representation | Root object |
| --- | --- | --- |
| 0 | `Empty` | a `FileState` over an empty extent leaf |
| 1 to cutoff − 1 | `WholeFile` | one `WholeFile` object: `LFS5SML\0`, version 1, the bytes |
| cutoff and above | `Chunked` | a `FileState` over a B+tree of extent pages over `Chunk` objects ([Edits: Mapping](edits.md#filemapping)) |

The default cutoff is 131,072 bytes; a Store may record any power of two up to
1 MiB.

### Chunking

A chunked file is cut by `FastCdc`, a frozen two-byte rolling GEAR scanner
([`cdc/gear.rs`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs)).

```text
 one chunk, by its length so far
 0               8 KiB              16 KiB                       32 KiB
 |---- copy -----|-- strict masks ----|------- looser masks -------|
   no hashing      fewer cuts            more cuts            forced cut
 per byte pair (a, b):  h = (h << 2) + (GEAR[a] << 1); test SHIFTED mask
                        h = h + GEAR[b];               test mask
 after a cut: emit, h = 0; at end of input: emit the rest (may be < 8 KiB)
```

| Rule | Detail | Source |
| --- | --- | --- |
| Profile | min 8,192, target 16,384 (switches masks), max 32,768; shift 2, seed 0; fixed masks and 256 GEAR values; `profile_id()` hashes them all under `layerfs/fastcdc-profile/v1` | [`gear.rs`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs) |
| Read sizes never move a boundary | An odd trailing byte of one `Read` is paired with the next read's first byte | `process_pending_pair` in [`gear.rs`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs) |
| Bounded memory | One chunk buffer and one read buffer of 32 KiB each; never the whole input | [`gear.rs`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs) |
| Zero runs | A run of length L: the first 32 KiB is scanned, the interior is `n` identical 32 KiB chunks handed over as one repeat count, the rest is scanned; the partition equals byte-by-byte scanning | [`cdc/zeros.rs`](../../../../core/crates/layerfs-content/src/file/cdc/zeros.rs) |
| Frozen | The CDC profile ID is hashed into the mapping profile every file state records; a change fails old file states with `InvalidRecord("file state profile")` | [`mapping/codec.rs`](../../../../core/crates/layerfs-content/src/file/mapping/codec.rs) |
| Hints | `cut_candidates` reports every possible boundary; a candidate proves nothing | [`gear.rs`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs) |

The rolling value only picks boundaries; a chunk's identity is its object ID.

**Chunk delta hints.** Content never encodes a delta. It attaches advisory
predecessors to new chunks so Physical storage may store a PREFIX record
([Physical storage: Encoding](../physical-storage/encoding.md)).

| Route | Base | Matching | Bound | Source |
| --- | --- | --- | --- | --- |
| `construct_bytes_with_predecessor` | a caller `PredecessorBase` | the base extent overlapping the chunk's own range | 4,096 base extents | [`mapping/predecessor.rs`](../../../../core/crates/layerfs-content/src/file/mapping/predecessor.rs) |
| `construct_runs_with_predecessor`, chunked edits | the replaced file (for example a rename target) or the edited base | `Aligner`: re-anchored at every chunk equal to an old payload; otherwise the old payload sharing most bytes, else the successor of the nearest unchanged neighbour | 64 old extents ahead, 128 held | [`mapping/align.rs`](../../../../core/crates/layerfs-content/src/file/mapping/align.rs) |
| Whole-file edit result | the edited base root | `OriginalBase` | one | [`edit/apply.rs`](../../../../core/crates/layerfs-content/src/file/edit/apply.rs) |

Chunks appended past the old end all get the old last chunk as hint; Storage
keeps trying it while it saves at least 1/8 of the FULL record. A whole-file
base offers chunks nothing. A base costs one root read and mapping pages,
never a payload read.

### Construction

| Entry point | Input | Notes | Source |
| --- | --- | --- | --- |
| `construct_bytes` | a slice of known length | representation from `bytes.len()` | [`construction/bytes.rs`](../../../../core/crates/layerfs-content/src/file/construction/bytes.rs) |
| `construct_stream` | any `Read` | buffers at most the cutoff; a longer prefix is chained back into the chunker without a reread | same |
| `construct_runs` | `FileRuns`: `Data(n)`, `Zero(n)`, `End` | same root as streaming the same bytes; full zero chunks and identical pages are reused; returns byte, chunk and page counts | [`construction/runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/runs.rs) |
| `*_with_predecessor` | plus a base | same root, plus advisory hints | same files |

```text
 validate policy --> probe up to cutoff --> representation(len)
   Empty:     empty ExtentLeaf, then FileState
   WholeFile: encode_whole_file, one object
   Chunked:   FastCdc --> each Chunk --> ExtentBuilder --> FileState last
 check: bytes scanned == builder length   else InvalidRecord("chunk accounting")
 --> ConstructedFile { root, logical_len, counters }
```

Engine callers: Project Init imports with `construct_stream`
([`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs));
Commit builds captured files with `construct_runs`, its predecessor form or
an edit ([`captured/owner.rs`](../../../../engine/crates/layerfs-workspace/src/construction/captured/owner.rs)).
A whole-file payload peaks at about twice its size during complete
construction (value and envelope are separate allocations); the edit route
writes into one pre-sized object.

### Reads and views

```text
 read_range(reader, root, start..end, sink)
   read root once --> WholeFile: slice the payload
                  --> FileState: check range, root page agrees with state
   navigation wave: up to 32 pages per provider call, level by level
   payload waves:   at most 32 distinct chunks and 1 MiB each
   write slices in logical order, release the wave, next wave
   bytes written == end - start ?   else InvalidRecord("mapping coverage")
```

| Function or type | What it does | Source |
| --- | --- | --- |
| `read_all`, `read_all_bounded(max)` | whole file; the bounded form refuses a longer file with `BoundedCapacityExceeded { what: "read.logical_length" }` | [`read.rs`](../../../../core/crates/layerfs-content/src/file/read.rs) |
| `read_range` | exactly `start..end` or refused, never partly served under `Ok`; returns `ReadCounters` | same |
| `inspect`, `classify` | `FileContent::WholeFile { logical_len }` or `Chunked(FileState)` | [`construction/bytes.rs`](../../../../core/crates/layerfs-content/src/file/construction/bytes.rs) |
| `FileView` | one base root read and classified once per operation (`open`, or `from_canonical` for a root already read) | [`view.rs`](../../../../core/crates/layerfs-content/src/file/view.rs) |
| `RangeCursor` + `PageCache` | several ascending ranges of one file; a shared page is read once; a range starting before the previous end is refused | [`mapping/read.rs`](../../../../core/crates/layerfs-content/src/file/mapping/read.rs) |
| `ExtentCursor` | extents in order (`next`) or from an offset (`seek`), one decoded leaf and one stack entry per level, no payload reads | [`mapping/extents.rs`](../../../../core/crates/layerfs-content/src/file/mapping/extents.rs) |

| Read window | Value | Source |
| --- | --- | --- |
| `READ_NAVIGATION_WAVE` | 32 pages per provider call | [`mapping/read.rs`](../../../../core/crates/layerfs-content/src/file/mapping/read.rs) |
| `READ_WAVE_OBJECTS`, `READ_WAVE_BYTES` | 32 chunks, 32 × 32 KiB = 1 MiB per payload wave; demands served per wave at most 128 | same |
| `READ_NAVIGATION_CACHE_PAGES` | 64 pages per `PageCache`, emptied wholesale when full | same |

A chunk demanded several times in one wave is read once. No bound grows with
file length: a long read is a sequence of bounded waves.

## Invariants

| Rule | Enforced by |
| --- | --- |
| Equal bytes under one policy give one root, whether they came as a slice, a stream or runs | one builder for every route: [`mapping/build.rs`](../../../../core/crates/layerfs-content/src/file/mapping/build.rs), [`chunk_runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/chunk_runs.rs) |
| A chunked edit result keeps the base's untouched extents, so its root depends on edit boundaries | [`edit/tree.rs`](../../../../core/crates/layerfs-content/src/file/edit/tree.rs) |
| Construction memory depends on tree height and page size, not file length | `ExtentBuilder` in [`mapping/build.rs`](../../../../core/crates/layerfs-content/src/file/mapping/build.rs) |
| Chunk and mapping profiles are frozen and recorded in every file state | `profile_id()` in [`gear.rs`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs), [`mapping/codec.rs`](../../../../core/crates/layerfs-content/src/file/mapping/codec.rs) |
| An object is emitted only when its bytes can no longer change, children before parents | [`mapping/build.rs`](../../../../core/crates/layerfs-content/src/file/mapping/build.rs), [`edit/finish.rs`](../../../../core/crates/layerfs-content/src/file/edit/finish.rs) |
| Advisory predecessors never change bytes, IDs or partitions | [`object/predecessor.rs`](../../../../core/crates/layerfs-content/src/object/predecessor.rs), [`construction/runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/runs.rs) |
| Reads stay inside declared windows and are served exactly or refused | [`mapping/read.rs`](../../../../core/crates/layerfs-content/src/file/mapping/read.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| File length | no product cap; `u64`, overflow is `LengthOverflow` | [`mapping/build.rs`](../../../../core/crates/layerfs-content/src/file/mapping/build.rs) |
| Whole-file payload | at most cutoff − 1, else `BoundedCapacityExceeded { what: "construction.whole_file" }` | [`construction/bytes.rs`](../../../../core/crates/layerfs-content/src/file/construction/bytes.rs) |
| Chunk payload | at most 32,768, else `ObjectLimitExceeded` | [`mapping/codec.rs`](../../../../core/crates/layerfs-content/src/file/mapping/codec.rs) |
| Mapping depth and pages | see [Edits: Mapping](edits.md#filemapping) | [`mapping/types.rs`](../../../../core/crates/layerfs-content/src/file/mapping/types.rs) |
| Policy out of range | `UnsupportedPolicy` before any work | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| Bad read range | `InvalidRange { start, end, length }` | [`read.rs`](../../../../core/crates/layerfs-content/src/file/read.rs) |
| Input stream fails | `Io`, returned once; objects already accepted stay with the consumer | [`gear.rs`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs) |
| Run source breaks its contract | `InvalidRecord("file run window")` or `("file run accounting")` | [`construction/runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/runs.rs) |
| Provider returns wrong count or role | `BatchCardinality`, `WrongLogicalRole` | [`mapping/read.rs`](../../../../core/crates/layerfs-content/src/file/mapping/read.rs) |

## Demonstration

In-memory stores from `tests/support` or `examples/support`; no database,
Docker, root, feature or platform gate. Each test command gets a wall timeout
of at most 120 s.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`file_complete.rs`](../../../../core/crates/layerfs-content/tests/file_complete.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test file_complete` | any | Representation boundaries (0, 1, cutoff − 1, cutoff), the frozen chunk partition, slice and stream agreeing, an `Io` failure after early chunks |
| [`file_read.rs`](../../../../core/crates/layerfs-content/tests/file_read.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test file_read` | any | Ranges across pages, invalid ranges, one provider call per navigation level, wave ceilings, refused non-canonical pages |
| [`zero_cdc.rs`](../../../../core/crates/layerfs-content/tests/zero_cdc.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test zero_cdc` | any | A zero stream is cut into full 32 KiB chunks: the period the zero-run path relies on |
| [`edit_timing_c1.rs`](../../../../core/crates/layerfs-content/examples/edit_timing_c1.rs) | `cargo +1.85.1 run --release --locked -p layerfs-content --example edit_timing_c1 -- --case shrink` | any | One edit on a 3.3 MB chunked base: elapsed time, root and objects demanded and written (diagnostic, not a measurement) |

## Pages

| Page | What it covers |
| --- | --- |
| [Edits](edits.md) | `file/edit` (localized edits, plans, runs, backed state, cost) and `file/mapping` (extent-page layout and the streaming builder). |
