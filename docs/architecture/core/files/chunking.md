# Chunking

[Architecture](../../README.md) › [Core](../README.md) › [Files](README.md) › Chunking

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Content-defined chunk boundaries and chunk deltas.

| | |
| --- | --- |
| Component | [Files](README.md) |
| Source | [`core/crates/layerfs-content/src/file/cdc`](../../../../core/crates/layerfs-content/src/file/cdc), [`core/crates/layerfs-content/src/file/mapping/align.rs`](../../../../core/crates/layerfs-content/src/file/mapping/align.rs), [`core/crates/layerfs-content/src/file/mapping/predecessor.rs`](../../../../core/crates/layerfs-content/src/file/mapping/predecessor.rs) |

## Chunk boundaries

A file at or above the construction cutoff is cut into chunks by a frozen
two-byte rolling GEAR scanner, `FastCdc`
([`gear.rs`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs)).
The scanner does not hash the first 8 KiB of a chunk. After that it reads the
input in byte pairs `(a, b)` and tests the rolling value twice per pair.

```text
 one chunk, by its length so far

 0               8 KiB              16 KiB                       32 KiB
 |---- copy -----|-- strict masks ----|------- looser masks -------|
   no hashing      fewer cuts            more cuts                   forced
                                                                     cut

 per byte pair (a, b), from 8 KiB on:

   h = (h << 2) + (GEAR[a] << 1)
   h & SHIFTED_mask == 0 ? --yes--> cut: chunk ends before a
   h = h + GEAR[b]
   h & mask == 0 ?         --yes--> cut: chunk ends after a
   no cut                  -------> next pair (forced cut at 32 KiB)

 after any cut: emit chunk, h = PROFILE_SEED (0), start the next chunk
 at end of input: emit what is left (it may be shorter than 8 KiB)
```

| Step | What happens | Source |
| --- | --- | --- |
| Copy | The first `MINIMUM_CHUNK_BYTES` of each chunk are copied with no hash work. | [`Scanner::consume`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs) |
| Strict region | Below `TARGET_CHUNK_BYTES`, `SMALL_MASK` / `SHIFTED_SMALL_MASK` apply; they have more bits set, so a cut is less likely. | [`scan_region`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs) |
| Looser region | From the target to the maximum, `LARGE_MASK` / `SHIFTED_LARGE_MASK` apply. | same |
| Forced cut | A chunk that reaches `MAXIMUM_CHUNK_BYTES` is emitted. | same |
| Odd byte | A byte left at the end of one `Read` is kept as `pending` and paired with the next read's first byte, so read sizes never move a boundary. | [`process_pending_pair`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs) |
| End | `finish` emits the remainder, including a pending byte. | [`Scanner::finish`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs) |

The rolling value shifts two bits per pair, so a byte stops affecting it 64
bytes later. The byte that decides a cut is not part of the chunk (`a` for the
first test, `b` for the second). The rolling GEAR value only picks boundaries;
a chunk's identity is its BLAKE3 object ID ([Identity](../objects/identity.md)).

`cut_candidates` reports every position where the scanner could end a chunk,
for callers that want boundary hints. A candidate proves nothing; the caller
checks it by identity.

### Zero runs

```text
 zero run of length L (from FileRun::Zero)

   head   = min(L, 32 KiB)             scanned normally
   middle = (L - head) / 32 KiB        n identical full chunks,
                                       one callback with repeat count n
   tail   = rest                       scanned normally
```

One full maximum chunk of zeros clears any earlier nonzero state; after that the
scanner state repeats every 32,768 bytes, so the interior of a hole is `n`
identical 32 KiB chunks handed over as one repeat count
([`zeros.rs`](../../../../core/crates/layerfs-content/src/file/cdc/zeros.rs)).
The mapping builder then reuses whole identical pages
([Construction: Mapping](construction.md#mapping)). The partition equals the
one byte-by-byte scanning of the same zeros gives.

### Frozen profile

| Parameter | Value | Source |
| --- | --- | --- |
| `MINIMUM_CHUNK_BYTES` | 8,192 | [`gear.rs`](../../../../core/crates/layerfs-content/src/file/cdc/gear.rs) |
| `TARGET_CHUNK_BYTES` | 16,384 (switches masks; not an output size) | same |
| `MAXIMUM_CHUNK_BYTES` | 32,768 | same |
| `NORMALIZATION_SHIFT` | 2 | same |
| `PROFILE_SEED` | 0 | same |
| `SMALL_MASK` / `SHIFTED_SMALL_MASK` | `0x0000_d903_0353_7000` / `0x0001_b206_06a6_e000` | same |
| `LARGE_MASK` / `SHIFTED_LARGE_MASK` | `0x0000_d901_0353_0000` / `0x0001_b202_06a6_0000` | same |
| `GEAR` | 256 fixed 64-bit values | same |
| `profile_id()` | BLAKE3 of all of the above, under `layerfs/fastcdc-profile/v1` | same |

The CDC `profile_id()` is hashed into the mapping `profile_id()`
([`codec.rs`](../../../../core/crates/layerfs-content/src/file/mapping/codec.rs)),
which every file state records. Changing any parameter changes every chunk
partition and every chunked file root, and an existing file state then fails to
decode with `InvalidRecord("file state profile")`.

## Chunk sizes

| Chunk | Payload size | Why |
| --- | --- | --- |
| Ordinary | more than 8 KiB, at most 32 KiB | 8 KiB is copied before the first test can cut. |
| Last chunk of a file or replacement | 1 byte to 32 KiB | `finish` emits whatever remains. |
| Interior chunk of a zero run | exactly 32 KiB | The repeated chunk is `MAXIMUM_CHUNK_BYTES` of zeros. |
| Any chunk | at most `MAXIMUM_CHUNK_BYTES` | `encode_chunk_object` and `decode_chunk_payload` refuse more with `ObjectLimitExceeded`. |

The scanner owns one chunk buffer and one read buffer, each
`MAXIMUM_CHUNK_BYTES`; it never holds the whole input. Each chunk becomes one
canonical chunk object: `LFS4CHK\0` followed by the payload
([layout](construction.md#mapping)).

## Predecessor-based chunk deltas

Content does not encode deltas. It attaches **advisory predecessors** to new
chunk objects: stored payloads the new chunk probably resembles. Storage
decides whether to store a delta against one
([Physical storage: Encoding](../physical-storage/encoding.md)). A hint never
changes a chunk's bytes, its ID or the mapping partition.

```text
 stored earlier version   | p1 | p2   | p3 | p4 | p5 |
                                 \       \
                                  \       \  matched by byte range
                                   v       v
 new chunks being built   | n1 | n2 | n3   | n4 | ...

 n2 carries "p2" as its advisory predecessor, n3 carries "p3", ...
 Storage may then store n2 as a delta against p2.
```

| Route | Base | How a new chunk is matched | Bound | Source |
| --- | --- | --- | --- | --- |
| `construct_bytes_with_predecessor` | a caller-supplied `PredecessorBase` | `PredecessorCursor`: the base extent overlapping the chunk's own byte range, same offsets | stops after `PREDECESSOR_DESCRIPTOR_LIMIT` (4,096) base extents | [`predecessor.rs`](../../../../core/crates/layerfs-content/src/file/mapping/predecessor.rs) |
| `construct_runs_with_predecessor` | the file this one replaced, such as a rename target | `Aligner`, re-anchored at every new chunk equal to an old payload | `WINDOW` (64) old extents ahead, at most 128 held | [`align.rs`](../../../../core/crates/layerfs-content/src/file/mapping/align.rs), [`runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/runs.rs) |
| Edit with a chunked result | the edited base | `Aligner`, anchored at each edit's start | same | [`apply.rs`](../../../../core/crates/layerfs-content/src/file/edit/apply.rs) |
| Edit with a whole-file result | the edited base root | the whole base, provenance `OriginalBase` | one entry | same |

For each new chunk the `Aligner` shifts the chunk's range back into base
coordinates and offers the old payload that shares the most bytes with it. A
chunk equal to an old payload gets no hint and resets the shift there. With no
overlap it offers the successor of the nearest unchanged neighbour. Every chunk
appended past the old end of the file therefore gets the same hint, the old
last chunk; Storage keeps trying it while it wins, as text resembling the tail
does, and skips it after a run of losses, as incompressible bytes produce
([Encoding](../physical-storage/encoding.md), step 3a). A base that
is a whole-file object offers chunks nothing: matching across the
representation change is not implemented. A base costs one root read and then
mapping pages, never a payload read. One object carries at most
`MAXIMUM_ADVISORY_PREDECESSORS` (4) entries
([`object/predecessor.rs`](../../../../core/crates/layerfs-content/src/object/predecessor.rs)).

## Related

- [Construction](construction.md): where chunks go once cut.
- [Edits](edits.md): how an edit scans only its replacement.
- [Objects: Identity](../objects/identity.md) and
  [Objects: Kinds](../objects/kinds.md): the chunk object's ID and role.
- [Physical storage: Encoding](../physical-storage/encoding.md): what Storage
  does with an advisory predecessor.
- [Glossary](../../glossary.md).

---

← [Core: Files](README.md) · [Up: Files](README.md) · [Files: Construction](construction.md) →
