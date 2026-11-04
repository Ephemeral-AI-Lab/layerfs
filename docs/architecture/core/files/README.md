# Files

[Architecture](../../README.md) › [Core](../README.md) › Files

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Complete-file construction, content-defined chunking, localized edits and bounded logical reads.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-content` |
| Source | [`core/crates/layerfs-content/src/file`](../../../../core/crates/layerfs-content/src/file), [`core/crates/layerfs-content/src/contract/policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| Uses | [Objects](../objects/README.md) |
| Used by | [Filesystem trees](../filesystem-trees/README.md), [Compare and merge](../compare-and-merge/README.md), [Engine: Project Init](../../engine/project-init/README.md), [Engine: Workspace](../../engine/workspace/README.md) |
| Flows | [A Commit](../../flows/commit.md), [Project Init](../../flows/init.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Chunking](chunking.md) | Content-defined chunk boundaries and chunk deltas. |
| [Construction](construction.md) | From a byte stream to file objects. |
| [Edits](edits.md) | Changing part of a file without rewriting all of it. |
| [Reads and views](reads-and-views.md) | Bounded reads and views over file objects. |

## The picture

```text
            bytes, stream or               base root + ordered edits
            data/zero runs                 + replacement bytes
                  |                                  |
                  v                                  v
 +--------------------------------+   +-------------------------------+
 | Construction                   |   | Edits                         |
 |  pick representation by length |   |  compare: no-op -> base root  |
 |  whole file / empty / chunked  |   |  split / scan / concatenate   |
 +--------------------------------+   +-------------------------------+
        |              |                  |            |
        |              v                  v            |
        |   +------------------------------------+     |
        |   | Chunking                           |     |
        |   |  FastCdc boundaries, zero runs,    |     |
        |   |  advisory predecessors (hints)     |     |
        |   +------------------------------------+     |
        |              |                               |
        v              v                               v
 +-------------------------------------------------------------------+
 | ExtentBuilder: chunk objects, extent pages, file state            |
 | --> FinalizedConsumer (caller), children before parents           |
 +-------------------------------------------------------------------+
                                   :
                       stored by the caller (Storage)
                                   :
 +-------------------------------------------------------------------+
 | Reads and views                                                   |
 |  AuthenticatedObjects (caller) --> classify root --> level waves  |
 |  --> payload waves --> sink, in logical order                     |
 +-------------------------------------------------------------------+
```

The crate opens no database, pack or file. Objects go out through the
caller's `FinalizedConsumer`; bytes come in through the caller's
`AuthenticatedObjects` provider ([`lib.rs`](../../../../core/crates/layerfs-content/src/lib.rs)).

## What it does

A file is named by one root object ID and has a logical length. The
`ConstructionPolicy` picks one of three representations from the length
([`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs)):

| Length | Representation | Root object |
| --- | --- | --- |
| 0 | `Empty` | a `FileState` over an empty extent leaf |
| 1 to cutoff - 1 | `WholeFile` | one `WholeFile` object holding all bytes |
| cutoff and above | `Chunked` | a `FileState` over a B+ tree of extent pages over `Chunk` objects |

The default cutoff is 131,072 bytes. A chunked file's chunks are cut by a
frozen content-defined chunker, 8 KiB to 32 KiB each
([Chunking](chunking.md)). Construction streams: it holds a bounded prefix,
one chunk and the open edge of the extent tree
([Construction](construction.md)). A read classifies the root and returns
bytes in logical order through bounded waves
([Reads and views](reads-and-views.md)).

An edit takes a stored root and an ordered list of replacements. If every
replacement equals the bytes it replaces, the result is the base root. For a
chunked file, the edit splits the stored tree at each boundary, chunks only the
replacement, and joins the parts; untouched subtrees keep their IDs and are not
read ([Edits](edits.md)). New chunks carry advisory predecessors so Storage can
store them as deltas ([Physical storage](../physical-storage/README.md)).

## Invariants

| Rule | Where it is enforced |
| --- | --- |
| Equal bytes under one policy give one root, whatever the input form (slice, stream or runs). | One builder for every route: [`build.rs`](../../../../core/crates/layerfs-content/src/file/mapping/build.rs), [`chunk_runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/chunk_runs.rs); checked by [`file_complete.rs`](../../../../core/crates/layerfs-content/tests/file_complete.rs) |
| A chunked edit result keeps the base's untouched extents, so its root depends on the edit boundaries. | [`tree.rs`](../../../../core/crates/layerfs-content/src/file/edit/tree.rs); see [Edits](edits.md#edit-model) |
| Edit cost follows the edit boundaries and replacement size, not the file size. | Stored-node split/concat: [`tree.rs`](../../../../core/crates/layerfs-content/src/file/edit/tree.rs); checked by [`edit_localized.rs`](../../../../core/crates/layerfs-content/tests/edit_localized.rs) |
| Reads stay inside declared windows: 32 pages per navigation call, 32 chunks and 1 MiB per payload wave. | [`mapping/read.rs`](../../../../core/crates/layerfs-content/src/file/mapping/read.rs) |
| Construction memory depends on tree height and page size, not on file length. | `ExtentBuilder::flush_streaming` in [`build.rs`](../../../../core/crates/layerfs-content/src/file/mapping/build.rs) |
| The chunk and mapping profiles are frozen; their digests are recorded in every file state. | `profile_id()` in [`gear.rs`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs) and [`codec.rs`](../../../../core/crates/layerfs-content/src/file/mapping/codec.rs) |
| A page or file state is canonical when decoded; a non-canonical one is refused. | `ExtentNode::validate`, `decode_node_with_context`, `decode_file_state` |
| An object is emitted only when its bytes can no longer change, children before parents. | [`build.rs`](../../../../core/crates/layerfs-content/src/file/mapping/build.rs), [`finish.rs`](../../../../core/crates/layerfs-content/src/file/edit/finish.rs) |
| Advisory predecessors never change bytes, IDs or partitions. | [`object/predecessor.rs`](../../../../core/crates/layerfs-content/src/object/predecessor.rs), [`construction/runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/runs.rs) |
| A read is served exactly or refused, never partly served under `Ok`. | `read_range` in [`mapping/read.rs`](../../../../core/crates/layerfs-content/src/file/mapping/read.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| File length | no product cap; `u64` length, overflow is `LengthOverflow` | [`build.rs`](../../../../core/crates/layerfs-content/src/file/mapping/build.rs) |
| Mapping depth | `MAX_LEVEL` 31, else `MappingDepthExceeded` | [`types.rs`](../../../../core/crates/layerfs-content/src/file/mapping/types.rs) |
| Cutoff | power of two, 131,072 to 1,048,576, else `UnsupportedPolicy` | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| Delta depth per role | 0 to `MAXIMUM_DELTA_MAX_DEPTH` 50 (defaults 16 whole-file, 8 chunk), else `UnsupportedPolicy` | same |
| Whole-file payload | at most cutoff - 1, else `BoundedCapacityExceeded { what: "construction.whole_file" }` | [`bytes.rs`](../../../../core/crates/layerfs-content/src/file/construction/bytes.rs) |
| Chunk payload | at most 32,768, else `ObjectLimitExceeded` | [`mapping/codec.rs`](../../../../core/crates/layerfs-content/src/file/mapping/codec.rs) |
| Mapping page | 64 to 128 entries (non-root), 8,192 bytes, else `NonCanonicalPagePartition` / `ObjectLimitExceeded` | [`types.rs`](../../../../core/crates/layerfs-content/src/file/mapping/types.rs) |
| Edits per request | no count cap | [`input.rs`](../../../../core/crates/layerfs-content/src/file/edit/input.rs) |
| Memory-profile edit frontier | `EDIT_DEFERRED_LIMIT` (8 MiB - 1) charge, else `BoundedCapacityExceeded { what: "edit.deferred_nodes" }`; the backed profile has no total ceiling | [`objects.rs`](../../../../core/crates/layerfs-content/src/file/edit/objects.rs) |
| Bad edit list | `InvalidEdit { what }` | [`input.rs`](../../../../core/crates/layerfs-content/src/file/edit/input.rs), [`apply.rs`](../../../../core/crates/layerfs-content/src/file/edit/apply.rs) |
| Bad read range | `InvalidRange { start, end, length }` | [`read.rs`](../../../../core/crates/layerfs-content/src/file/read.rs) |
| Read longer than the caller allows | `BoundedCapacityExceeded { what: "read.logical_length" }` | same |
| Input stream fails | `Io`, returned once; objects already accepted stay with the consumer; no retry | [`gear.rs`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs); checked by `late_input_failure_after_early_output_is_returned_once` |
| Run source breaks its contract | `InvalidRecord("file run window")` or `InvalidRecord("file run accounting")` | [`construction/runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/runs.rs) |
| Provider returns wrong count or role | `BatchCardinality`, `WrongLogicalRole` | [`mapping/read.rs`](../../../../core/crates/layerfs-content/src/file/mapping/read.rs) |

## Demonstration

All four use in-memory stores from the crate's `tests/support` or
`examples/support`; they need no database, Docker, root or feature flag, and
have no platform `cfg` or `#[ignore]`. The commands were not run while writing
this page.

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`file_complete.rs`](../../../../core/crates/layerfs-content/tests/file_complete.rs) | `layerfs-content` | Representation boundaries (0, 1, cutoff - 1, cutoff), the frozen chunk partition of a reference input, chunk size window, slice and stream agreeing, bounded source requests, and an `Io` failure after early chunks. |
| [`file_read.rs`](../../../../core/crates/layerfs-content/tests/file_read.rs) | `layerfs-content` | Ranges across extents and pages, invalid ranges, one read per repeated payload, one provider call per navigation level, wave ceilings, and refusal of an overstated file state or a non-canonical page. |
| [`edit_localized.rs`](../../../../core/crates/layerfs-content/tests/edit_localized.rs) | `layerfs-content` | A recording provider counts demanded and emitted IDs: an edit reads only its boundary paths, the counts do not grow with file size, a pure deletion never walks the rightmost path. |
| [`edit_timing_c1.rs`](../../../../core/crates/layerfs-content/examples/edit_timing_c1.rs) | `layerfs-content` | Builds a 3.3 MB chunked base, applies one edit (`overwrite` by default, or `delete`, `shrink`, `split`), and prints elapsed time, the edited root and objects demanded and written. |

```text
 edit_localized: what is counted

 Recorder provider <-- demanded IDs -- apply_edits -- emitted --> consumer
        |                                                          |
        v                                                          v
 pages + payloads read                              pages + chunks written
 assert: only root-to-boundary paths                assert: only changed pages
```

```sh
cargo +1.85.1 test --locked -p layerfs-content --test file_complete
cargo +1.85.1 test --locked -p layerfs-content --test file_read
cargo +1.85.1 test --locked -p layerfs-content --test edit_localized
cargo +1.85.1 run --release --locked -p layerfs-content --example edit_timing_c1
cargo +1.85.1 run --release --locked -p layerfs-content --example edit_timing_c1 -- --case shrink
```

Give each test command a wall timeout of at most 120 seconds (repository rule).
The example prints a wall time for diagnosis only; it is not a measurement
receipt. Its header names a v0.1.6 reference counterpart that was retired with
the reference implementation.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`docs/general/cas_cdc_deltaencoding_handbook.md`](../../../general/cas_cdc_deltaencoding_handbook.md) — CDC and localized edits covered in [Chunking](chunking.md) and [Edits](edits.md); delta encoding stays in the handbook and [Physical storage](../physical-storage/README.md).
- [`core/docs/architecture/03-files.md`](../../../../core/docs/architecture/03-files.md) — covered in [Construction](construction.md), [Edits](edits.md) and [Reads and views](reads-and-views.md); historical only for its `layerfs-server` parts.
- [`core/docs/architecture/24-file-lengths.md`](../../../../core/docs/architecture/24-file-lengths.md) — Store-side length lookup, not this component; historical only here.
- [`core/docs/architecture/31-payload-streams.md`](../../../../core/docs/architecture/31-payload-streams.md) — zero runs covered in [Chunking](chunking.md#zero-runs) and [Construction](construction.md); its overlay parts belong to [Engine: Overlay](../../engine/overlay/README.md).
- [`core/docs/architecture/45-immutable-payload-segments.md`](../../../../core/docs/architecture/45-immutable-payload-segments.md) — withdrawn; historical only.
- [`core/docs/architecture/54-run-aware-localized-file-edits.md`](../../../../core/docs/architecture/54-run-aware-localized-file-edits.md) — covered in [Edits: Runs and batches](edits.md#runs-and-batches).

---

← [Objects: Access](../objects/access.md) · [Up: Core](../README.md) · [Files: Chunking](chunking.md) →
