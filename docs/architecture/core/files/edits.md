# Edits

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Changing part of a stored file without rewriting all of it, and the extent-page
mapping those edits split and join, within [Files](README.md).

| | |
| --- | --- |
| Source | [`file/edit/`](../../../../core/crates/layerfs-content/src/file/edit), [`file/mapping/`](../../../../core/crates/layerfs-content/src/file/mapping) |

## file/edit

### Edit model

An edit request is an immutable base root plus an ordered list of
replacements. Each `Edit` replaces `start..end` with `replacement_len` bytes in
**current-result coordinates**: it addresses the result of every earlier edit.

```text
 base (len 100)    0          30   40                      100
                   |-----------|####|-----------------------|
 edit 0            Edit::new(30, 40, 5)     replace 10 bytes by 5
 after edit 0      0          30  35                       95
 edit 1            Edit::insert(60, 8)      60 is in the 95-byte result
 after edit 1      0          30  35        60      68      103
                   |-----------|++|---------|+++++++|-------|
```

| Type | Role | Source |
| --- | --- | --- |
| `Edit` | one replacement: `new`, `overwrite`, `insert`, `delete` | [`input.rs`](../../../../core/crates/layerfs-content/src/file/edit/input.rs) |
| `EditSequence` | replayable ordered list: `base_len`, `final_len`, `len`, `edit_at(i)` | same |
| `EditSource`, `IndexedEditSource` | replacement bytes by index and offset; `read_run_at` may return `FileRun::Zero`, defaulting to `read_at` | same |
| `EditRequest`, `IndexedEditRequest` | base root (or an open `FileView`), edits and source | [`apply.rs`](../../../../core/crates/layerfs-content/src/file/edit/apply.rs) |

| Rule | Refusal |
| --- | --- |
| Edits are never reordered or merged; their boundaries decide the chunk layout | — |
| An edit may not start inside bytes an earlier edit introduced | `InvalidEdit { what: "coordinate before current result" }` |
| An edit may not reach past the base | `InvalidEdit { what: "range beyond base length" }` |
| The base length must equal `EditSequence::base_len` | `InvalidEdit { what: "declared base length" }` |
| A replacement must deliver exactly its declared length | `InvalidEdit { what: "replacement length" }` ([`runs.rs`](../../../../core/crates/layerfs-content/src/file/edit/runs.rs)) |

A whole-file or empty result has the root complete construction would give.
A chunked result keeps the base's untouched extents and chunks only the
replacement, so in general its root differs from fresh construction of the
same bytes ([`edit_model.rs`](../../../../core/crates/layerfs-content/tests/edit_model.rs)).

### Plans

`apply_edits`, `apply_edits_backed` and `apply_indexed_edits_view_backed` all
run one driver, `apply_view`; the result length picks the route.

```text
 open base view (one root read)
 no edits ------------------------------------------------> base root
 compare pass: every replacement equals its base range? ---> base root
   (64 KiB windows, stops at the first difference)
 representation(final_len)
   Empty     --> empty leaf + FileState
   WholeFile --> kept ranges + replacements into one pre-sized object,
                 base hinted as OriginalBase
   Chunked   --> base chunked? yes: stored-node split/concatenate
                               no:  rescan the whole result
```

`Plan` walks the edit list lazily, yielding `Retain { base }` and
`Replace { index, base, len }`; its base cursor only moves forward and a
replaced base range is never read.

```text
 base extents      | e1 | e2 | e3 | e4 | e5 | e6 | e7 |
 edit                        |<-- replaced -->|
 1 split(start)    left  = | e1 | e2 |e3a|
 2 split(removed)  gone  =               |e3b| e4 |e5a|   discarded, unread
                   right =                           |e5b| e6 | e7 |
 3 scan replacement        | n1 | n2 |                   new chunks
 4 concat          | e1 | e2 |e3a| n1 | n2 |e5b| e6 | e7 |
 e1, e2, e6, e7 keep their IDs; e3a, e5b are slices of stored chunks
 pages off the two join paths keep their IDs and are never read
```

| Step | Source |
| --- | --- |
| Split cuts one stored extent; the payload ID stays, offset and length move | [`split.rs`](../../../../core/crates/layerfs-content/src/file/edit/split.rs), [`tree.rs`](../../../../core/crates/layerfs-content/src/file/edit/tree.rs) |
| The removed middle is discarded before it is encoded | `tree::discard` in [`apply.rs`](../../../../core/crates/layerfs-content/src/file/edit/apply.rs) |
| The replacement goes through the construction scanner and builder; each new chunk takes an `Aligner` hint | [`chunk_runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/chunk_runs.rs) |
| Concatenation rebuilds pages on the join boundaries only; contiguous slices of one payload coalesce | [`tree.rs`](../../../../core/crates/layerfs-content/src/file/edit/tree.rs), [`concat.rs`](../../../../core/crates/layerfs-content/src/file/edit/concat.rs) |
| Drafts no longer reachable are released after each edit; only pages the final root reaches are emitted, children first | [`objects.rs`](../../../../core/crates/layerfs-content/src/file/edit/objects.rs), [`finish.rs`](../../../../core/crates/layerfs-content/src/file/edit/finish.rs) |

### Runs and state profiles

A zero run in a replacement goes through the repeated-chunk path, and the
compare pass checks it against stored ranges without reading payloads where it
can ([`zero.rs`](../../../../core/crates/layerfs-content/src/file/edit/zero.rs)).
One request applies any number of edits in one pass; the unfinished frontier
lives in one of two profiles
([`state.rs`](../../../../core/crates/layerfs-content/src/file/edit/state.rs)):

| Profile | Entry point | Drafts live | Bound |
| --- | --- | --- | --- |
| Memory | `apply_edits` | decoded in process memory | `EDIT_DEFERRED_LIMIT` (8 MiB − 1), else `BoundedCapacityExceeded { what: "edit.deferred_nodes" }` ([`objects.rs`](../../../../core/crates/layerfs-content/src/file/edit/objects.rs)) |
| Backed | `apply_edits_backed`, `apply_indexed_edits_view_backed` | raw records in a caller `IndexedEditBacking` | each record ≤ 64 KiB; `get_many` asks ≤ 64 keys in one 64 KiB envelope; no total ceiling ([`backing.rs`](../../../../core/crates/layerfs-content/src/file/edit/backing.rs)) |

The backing is plain keyed records with guarded batches
(`EditRecordChange` with an expected pre-state; `Applied` or `NotApplied`);
Content defines no database type. Commit supplies the backing
([`captured/owner.rs`](../../../../engine/crates/layerfs-workspace/src/construction/captured/owner.rs)).
A failure is returned once and the editor becomes terminal.

### Cost

| Work | Grows with | Source |
| --- | --- | --- |
| Mapping pages read | paths from the root to each edit boundary | [`tree.rs`](../../../../core/crates/layerfs-content/src/file/edit/tree.rs) |
| Payloads read | same-length replaced ranges, read by the compare pass up to the first difference; a split reads none | [`compare.rs`](../../../../core/crates/layerfs-content/src/file/edit/compare.rs) |
| Payloads written | replacement bytes | [`chunk_runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/chunk_runs.rs) |
| Pages emitted | changed pages on the join paths | [`finish.rs`](../../../../core/crates/layerfs-content/src/file/edit/finish.rs) |
| A page two passes need | one read (shared `PageCache`) | [`apply.rs`](../../../../core/crates/layerfs-content/src/file/edit/apply.rs) |

[`edit_localized.rs`](../../../../core/crates/layerfs-content/tests/edit_localized.rs)
records one overwrite of three extents: 3 pages read on an 8 MB file, 4 on a
24 MB file, and the same 3 payloads in both. Routes whose cost follows the
result instead: a whole-file result reads every kept byte (at most the
cutoff); a whole-file base with a chunked result rescans the whole result.

## file/mapping

A chunked file is a B+tree of extent pages over chunk objects, opened by one
file state. All integers are big-endian.

```text
 FileState value (93 B)   "LFS4MAP\0" | u16 ver=3 | role 0x0a | flags=0
                          | u64 logical_len | u64 extent_count
                          | u8 tree_level | profile_id 32 | mapping_root 32
 Extent page value        "LFS4MAP\0" | u16 ver=3 | role | level | flags=0
                          | u16 count | u64 bytes | u64 extents  (header 31)
   leaf entry (40 B)      chunk ID 32 | u32 source_offset | u32 length
   branch entry (48 B)    u64 cumulative_logical_end
                          | u64 cumulative_extent_end | child ID 32
 Chunk value              "LFS4CHK\0" | payload (1 .. 32,768)
```

| Rule | Value | Source |
| --- | --- | --- |
| Entries per non-root page | `MIN_ENTRIES` 64 to `MAX_ENTRIES` 128 | [`types.rs`](../../../../core/crates/layerfs-content/src/file/mapping/types.rs) |
| Root leaf / root branch | 0 to 128 entries / at least 2 children | same |
| Levels | at most `MAX_LEVEL` 31, else `MappingDepthExceeded` | same |
| Page size | at most 8,192 canonical bytes, else `ObjectLimitExceeded` | same |
| Partition | a short non-root page is `NonCanonicalPagePartition`; a page is checked in its own context | `ExtentNode::validate`, `decode_node_with_context` |
| Slices | nonempty, `source_offset + length ≤ 32,768`; contiguous slices of one payload are merged | `ExtentSlice::new`, `ExtentNode::validate` |
| Branch summaries | strictly increasing cumulative ends adding up to page totals | `ExtentNode::validate` |
| Profile | `profile_id()` hashes page bounds, grammar and the CDC profile | [`codec.rs`](../../../../core/crates/layerfs-content/src/file/mapping/codec.rs) |

**Streaming builder.** `ExtentBuilder` keeps one list of unfinished entries
per level. When a level holds more than `stream_flush_entries` (192) it emits
a full page of 128 and pushes that page's summary up; `finish` emits the
rest, splitting an over-full level in half, and returns the root summary.
Complete construction and edits share it, so equal extent sequences give
equal partitions
([`build.rs`](../../../../core/crates/layerfs-content/src/file/mapping/build.rs)).
`push_repeated_chunk` accepts a zero chunk once and emits complete identical
pages once, referenced repeatedly
([`repeated.rs`](../../../../core/crates/layerfs-content/src/file/mapping/repeated.rs)).

## Related

- [Files](README.md): representations, chunking, construction and reads.
- [Objects](../objects/README.md): envelope, roles and advisory predecessors.
- [Physical storage: Encoding](../physical-storage/encoding.md): extent pages
  stored as COPY/INSERT programs against their predecessor page.
- [Engine: Commit](../../engine/commit/README.md): the caller that turns
  captured Workspace changes into edits.
