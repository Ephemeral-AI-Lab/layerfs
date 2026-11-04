# Construction

[Architecture](../../README.md) › [Core](../README.md) › [Files](README.md) › Construction

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

From a byte stream to file objects.

| | |
| --- | --- |
| Component | [Files](README.md) |
| Source | [`core/crates/layerfs-content/src/file/construction`](../../../../core/crates/layerfs-content/src/file/construction), [`core/crates/layerfs-content/src/file/mapping`](../../../../core/crates/layerfs-content/src/file/mapping), [`core/crates/layerfs-content/src/contract/policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |

## From a stream to file objects

```text
 construct_bytes(&[u8])   construct_stream(Read)   construct_runs(FileRuns)
          |                        |                        |
          |              buffer up to the cutoff  buffer up to the cutoff
          |                        |                        |
          +------------------------+------------------------+
                                   |
                                   v
              ConstructionPolicy::representation(logical length)
                                   |
        +--------------------------+----------------------------+
        | len == 0                 | 0 < len < cutoff           | len >= cutoff
        v                          v                            v
  empty ExtentLeaf          one WholeFile object       FastCdc --> ExtentBuilder
  + FileState                                          (chunk     (mapping
                                                        objects)   pages)
                                                                    |
                                                                FileState
        |                          |                                |
        +--------------------------+--------------------------------+
                                   |
                                   v
           FinalizedConsumer::accept(object), children before parents
                                   |
                                   v
           ConstructedFile { root, logical_len, counters }
```

Every entry point validates the `ConstructionPolicy` before any work. The
result is the root object ID plus the logical length; nothing else is needed
to read the file back.

| Entry point | Input | Notes | Source |
| --- | --- | --- | --- |
| `construct_bytes` | a byte slice of known length | Picks the representation from `bytes.len()`. | [`bytes.rs`](../../../../core/crates/layerfs-content/src/file/construction/bytes.rs) |
| `construct_bytes_with_predecessor` | slice plus a `PredecessorBase` | Same root; adds advisory chunk predecessors ([Chunking](chunking.md#predecessor-based-chunk-deltas)). | same |
| `construct_stream` | any `Read` | Reads at most the cutoff into a prefix; a short prefix is a small file, otherwise `Cursor(prefix).chain(source)` feeds the chunker with no reread. | same |
| `construct_runs` | `FileRuns`: `Data(n)`, `Zero(n)` or `End` | Same root as streaming the same logical bytes, but full zero chunks and pages are reused. Returns `RunConstruction` with byte, chunk and page counts. | [`runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/runs.rs) |
| `construct_runs_with_predecessor` | `FileRuns` plus a `PredecessorBase` | Same root as `construct_runs`; adds hints. | same |

| Representation | Selected when | Objects emitted | Source |
| --- | --- | --- | --- |
| `Empty` | length 0 | one empty `ExtentLeaf`, then a `FileState` | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| `WholeFile` | `0 < length < cutoff` | one `WholeFile` object | same |
| `Chunked` | `length >= cutoff` | `Chunk`, `ExtentLeaf`, `ExtentBranch` objects, then a `FileState` | same |

The cutoff is `small_file_threshold_bytes`: 131,072 by default, and any power
of two from 131,072 to 1,048,576. The Store records the policy it was created
with; callers derive policy and `ConstructionCapacities` from it
([Physical storage](../physical-storage/README.md)).

Callers in the engine layer: Project Init imports native files with
`construct_stream`
([`import/files.rs`](../../../../engine/crates/layerfs-project/src/import/files.rs)),
and Commit builds captured files with `construct_runs`,
`construct_runs_with_predecessor` or a localized edit
([`captured/owner.rs`](../../../../engine/crates/layerfs-workspace/src/construction/captured/owner.rs)).

## Mapping

A chunked file is a B+ tree of extent pages over chunk objects, opened by one
file-state object.

```text
 FileState  (role FileState)
   logical_len | extent_count | tree_level | profile_id | mapping_root
                                                              |
                                                              v
 ExtentBranch (level >= 1)   [ end bytes | end extents | child ID ] x 2..128
                                   |                         |
                                   v                         v
 ExtentLeaf (level 0)        [ chunk ID | offset | length ] x 64..128
                                   |
                                   v
 Chunk                       LFS4CHK\0 + payload (at most 32 KiB)
```

Byte layouts of each object's value, inside the canonical envelope (`LFSO`,
kind, two 32-bit lengths; see [Identity](../objects/identity.md)). All integers
are big-endian.

```text
 WholeFile value       "LFS5SML\0" | u16 version=1 | payload
                        8            2               1 .. cutoff-1

 Chunk value           "LFS4CHK\0" | payload
                        8            1 .. 32,768

 Extent page value     "LFS4MAP\0" | u16 ver=3 | role | level | flags=0
                        8            2           1      1       1
                       | u16 count | u64 bytes | u64 extents | entries...
                         2           8           8            (header = 31)
   leaf entry   (40)   chunk ID 32 | u32 source_offset | u32 logical_length
   branch entry (48)   u64 cumulative_logical_end | u64 cumulative_extent_end
                       | child ID 32

 FileState value (93)  "LFS4MAP\0" | u16 ver=3 | role 0x0a | flags=0
                       | u64 logical_len | u64 extent_count | u8 tree_level
                       | profile_id 32 | mapping_root 32
```

| Rule | Value | Source |
| --- | --- | --- |
| Entries per non-root page | `MIN_ENTRIES` 64 to `MAX_ENTRIES` 128 | [`types.rs`](../../../../core/crates/layerfs-content/src/file/mapping/types.rs) |
| Root leaf | 0 to 128 entries | same |
| Root branch | at least 2 children | same |
| Branch levels | at most `MAX_LEVEL` 31 | same |
| Canonical page size | at most `MAX_NODE_OBJECT_BYTES` 8,192 | same |
| Extent slice | nonempty, `source_offset + length <= 32,768` | `ExtentSlice::new` |
| Adjacent slices | two contiguous slices of one payload are merged, never stored side by side | `ExtentNode::validate`, [`concat.rs`](../../../../core/crates/layerfs-content/src/file/edit/concat.rs) |
| Branch summaries | strictly increasing cumulative ends that add up to the page totals | `ExtentNode::validate` |
| Mapping profile | `profile_id()` hashes the page bounds, the grammar and the CDC profile; a file state with another profile is refused | [`codec.rs`](../../../../core/crates/layerfs-content/src/file/mapping/codec.rs) |

Encoders validate before writing and decoders revalidate what they read, so a
decoded page is canonical. A page is checked in its own context: a short page is
accepted as a root and refused as a non-root (`NonCanonicalPagePartition`).

### The streaming builder

`ExtentBuilder` keeps one list of unfinished entries per level. When a level
holds more than `stream_flush_entries` (128 + 64) entries it emits a full page
of 128 and pushes that page's summary one level up. At the end, `finish`
emits what is left, splitting an over-full level in half, and returns the root
summary.

```text
 level 2   [ s ]                         <- open, unfinished
 level 1   [ s s s s ... ]               <- open, unfinished
 level 0   [ e e e e e e e e e ... ]     <- open; at 193 entries emit
             \_____ 128 _____/               one full leaf, push its
                                             summary to level 1
```

The builder holds only these open boundary pages, so its memory depends on
tree height and page size, not on file length. Complete construction and edits
share it, so equal extent sequences give equal partitions and equal roots.

Zero runs use `push_repeated_chunk`
([`repeated.rs`](../../../../core/crates/layerfs-content/src/file/mapping/repeated.rs)):
the zero chunk is accepted once, and complete identical pages are emitted once
and referenced repeatedly. Work depends on boundary pages and tree height, not
on the repeat count.

## Complete-file construction

| Step | What happens | Source |
| --- | --- | --- |
| 1. Validate | `policy.validated()` refuses an unsupported cutoff or depth (`UnsupportedPolicy`). | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| 2. Probe | A stream or run source is read up to the cutoff. | [`bytes.rs`](../../../../core/crates/layerfs-content/src/file/construction/bytes.rs), [`runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/runs.rs) |
| 3a. Whole file | `encode_whole_file` builds the value and envelope, checks `whole_file_raw_limit` and `whole_file_canonical_limit`, and emits one object. | `encode_whole_file` |
| 3b. Chunked | `FastCdc` cuts chunks; each chunk object is emitted, then appended to the builder as one extent. | [`build.rs`](../../../../core/crates/layerfs-content/src/file/mapping/build.rs) |
| 4. Check | The bytes scanned must equal the builder's logical length (`InvalidRecord("chunk accounting")`); a run source must account for every data and zero byte. | same, `runs.rs` |
| 5. Root | `emit_file_state` emits the `FileState` last and returns its ID. | `build.rs` |

Each object is handed to the caller's `FinalizedConsumer` as soon as its bytes
can no longer change, children before parents, with its direct references
listed. Content opens no database or file: what happens to emitted objects is
the consumer's business ([Physical storage: Saves](../physical-storage/saves.md)).

Construction is deterministic: the same bytes under the same policy give the
same root, whether they came as a slice, a stream or runs. Edited chunked files
are the exception; see [Edits](edits.md#edit-model).

A whole-file payload peaks at about twice its size during complete
construction, because the value and the canonical envelope are separate
allocations (stated in `encode_whole_file`). The edit route writes straight into
one pre-sized object (`begin_whole_file_object`).

## Related

- [Chunking](chunking.md): how chunk boundaries are chosen.
- [Edits](edits.md): changing an existing file with the same builder.
- [Reads and views](reads-and-views.md): reading the objects back.
- [Objects: Identity](../objects/identity.md) and
  [Objects: Kinds](../objects/kinds.md): envelope, ID and roles.
- [Physical storage: Saves](../physical-storage/saves.md): where emitted
  objects go.
- [Glossary](../../glossary.md).

---

← [Files: Chunking](chunking.md) · [Up: Files](README.md) · [Files: Edits](edits.md) →
