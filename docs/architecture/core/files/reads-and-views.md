# Reads and views

[Architecture](../../README.md) › [Core](../README.md) › [Files](README.md) › Reads and views

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Bounded reads and views over file objects.

| | |
| --- | --- |
| Component | [Files](README.md) |
| Source | [`core/crates/layerfs-content/src/file/read.rs`](../../../../core/crates/layerfs-content/src/file/read.rs), [`core/crates/layerfs-content/src/file/view.rs`](../../../../core/crates/layerfs-content/src/file/view.rs), [`core/crates/layerfs-content/src/file/mapping/read.rs`](../../../../core/crates/layerfs-content/src/file/mapping/read.rs), [`core/crates/layerfs-content/src/file/mapping/extents.rs`](../../../../core/crates/layerfs-content/src/file/mapping/extents.rs) |

## Bounded reads

A read asks the caller's `AuthenticatedObjects` provider for canonical bytes
by ID. The provider returns bytes already checked against the ID, in demand
order ([Objects: Access](../objects/access.md)). Content opens no database or
pack.

```text
 read_range(reader, root, start..end, sink)
        |
        v
 read root object once ---> classify
        |                      |
        | WholeFile            | FileState (chunked)
        v                      v
 slice the payload       check the range, then walk the mapping level by level
 write it to sink              |
                               v
                 level wave: demand up to 32 pages in one provider call
                               |
                               v
                 leaf extents -> payload demands, grouped in waves:
                   at most 32 distinct chunks, at most 1 MiB per wave
                               |
                               v
                 write each demanded slice to sink in logical order,
                 release the wave, read the next one
                               |
                               v
                 bytes written == end - start ?  no -> refused
```

| Function | What it does | Source |
| --- | --- | --- |
| `read_all` | Reads the whole file. | [`read.rs`](../../../../core/crates/layerfs-content/src/file/read.rs) |
| `read_all_bounded` | Reads the whole file only if its length is at most `maximum`; else `BoundedCapacityExceeded { what: "read.logical_length" }`. | same |
| `read_range` | Reads `start..end`; an inverted or out-of-file range is `InvalidRange`. An empty range reads only the root. | same |
| `inspect` / `classify` | Reads (or takes) a root and returns `FileContent::WholeFile { logical_len }` or `FileContent::Chunked(FileState)`. | [`bytes.rs`](../../../../core/crates/layerfs-content/src/file/construction/bytes.rs) |

A chunked read checks that the root page it decodes agrees with the file
state's length and extent count before it walks anything, and checks at the end
that it wrote exactly the requested number of bytes
(`InvalidRecord("mapping coverage")`). A range is served exactly or refused;
it is never partly served under `Ok`. Every read returns `ReadCounters`: pages
read, navigation calls, payload IDs, payload calls, largest batches and payload
bytes.

## Views

A view keeps one operation's base open so it is classified once.

| Type | Holds | Used for | Source |
| --- | --- | --- | --- |
| `FileView` | the base root ID, its canonical bytes and its `FileContent` | edits and Commit construction: `logical_len`, `file_state`, `whole_file_bytes`, `read_range`, `extents` | [`view.rs`](../../../../core/crates/layerfs-content/src/file/view.rs) |
| `RangeCursor` | the file state and a caller-owned `PageCache` | several ascending ranges of one file; a page two ranges share is read once | [`mapping/read.rs`](../../../../core/crates/layerfs-content/src/file/mapping/read.rs) |
| `ExtentCursor` | one decoded leaf and one stack entry per level | walking extents in order (`next`) or from an offset (`seek`) without payload reads | [`mapping/extents.rs`](../../../../core/crates/layerfs-content/src/file/mapping/extents.rs) |

```text
 ExtentCursor over a chunked file

 FileState --> branch --> leaf 1 [e1 e2 ... e128] --> leaf 2 [...] --> ...
                            ^
                            current leaf only; a subtree that ends before
                            the sought offset is skipped without reading it

 next() returns Extent { offset, length, chunk, chunk_offset }
```

`FileView::open` reads the root once; `FileView::from_canonical` classifies a
root the caller already read, for callers that read several roots in one
demand. `RangeCursor::read_segment` refuses a range that starts before the
previous one ended.

## Memory windows

| Bound | Value | What it limits | Source |
| --- | --- | --- | --- |
| `READ_NAVIGATION_WAVE` | 32 pages | mapping pages demanded per provider call; at most 32 × 8 KiB of pages | [`mapping/read.rs`](../../../../core/crates/layerfs-content/src/file/mapping/read.rs) |
| `READ_WAVE_OBJECTS` | 32 | distinct chunk payloads per payload wave | same |
| `READ_WAVE_BYTES` | 32 × 32 KiB = 1 MiB | payload bytes per wave; a larger wave is refused (`ObjectLimitExceeded`) | same |
| demands per wave | 128 (`READ_WAVE_OBJECTS * 4`) | slices served from one wave before it is released | same |
| `READ_NAVIGATION_CACHE_PAGES` | 64 | pages a `PageCache` keeps; it empties wholesale when full | same |
| `MAX_NODE_OBJECT_BYTES` | 8,192 | one mapping page | [`types.rs`](../../../../core/crates/layerfs-content/src/file/mapping/types.rs) |
| `MAXIMUM_CHUNK_BYTES` | 32,768 | one payload; larger is refused at decode | [`gear.rs`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs) |
| whole-file root | below the cutoff (at most 1 MiB - 1) | the one object a whole-file read holds | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |

A chunk demanded several times in one wave is read once and borrowed for each
demand. The navigation frontier keeps one small entry per mapping node the
range still has to reach at the next level; the read visits those nodes either
way. No bound grows with file length: a long read is a sequence of bounded
waves, and the sink receives bytes as each wave is served.

## Related

- [Construction](construction.md#mapping): the mapping layout being read.
- [Edits](edits.md): the main user of `FileView`, `RangeCursor` and
  `PageCache`.
- [Objects: Access](../objects/access.md): the `AuthenticatedObjects` provider
  contract.
- [Physical storage: Reads](../physical-storage/reads.md): the Store-side
  provider.
- [Glossary](../../glossary.md).

---

← [Files: Edits](edits.md) · [Up: Files](README.md) · [Core: Filesystem trees](../filesystem-trees/README.md) →
