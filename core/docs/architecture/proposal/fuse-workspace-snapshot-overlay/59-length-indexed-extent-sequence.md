# Length-indexed extent sequence for one file

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Dated 2026-09-25. Describes the implementation landed on branch
> `codex/issue245-range-cow-plan` for issue #245 packages A–C, written against
> the working tree whose production LOC is 56,035. It supersedes the
> absolute-offset piece-page description in
> [02-overlay-snapshot.md](02-overlay-snapshot.md) for the file index only; the
> namespace, binding and custody rules there are unchanged. No performance,
> capacity or release claim follows from this document.

## 1. Why the format changed

The earlier file index keyed every extent record by its **absolute logical
start** in a fixed two-level page tree. A structural insertion moves every
suffix extent, so those keys had to be rewritten for every suffix page, and each
callback rebuilt the whole page set from a materialized `Vec<Piece>` capped at
1,024 extents.

The current index derives an extent's logical start from its **position in the
sequence**. A leaf packs extent records; a branch packs child references, each
carrying the logical byte length of its child subtree. Inserting bytes rewrites
those lengths on one ancestor path and the leaves the replaced interval touches;
every other page is shared with the previous root.

## 2. Page format

`PageData` keeps its 4,096-byte frame, `LFSWMTA1` magic, incarnation stamp, page
identity, level byte and trailing SHA-256 seal. Two header fields that were
reserved and zero now declare what the body is:

| Field | Offset | Value |
| --- | --- | --- |
| format | 49 | `0` keyed cells, `1` length-indexed pieces |
| kind | 54 | `0` cells page, `1` pieces page |

A reader refuses a page whose declared format does not match the reader it is
asked for, so an extent page written under the previous absolute-offset format
is **refused**, never reinterpreted. There is no in-place migration: a retained
private root from the older format must be migrated or refused, and this
implementation refuses it.

A **pieces leaf** (`level == 0`) holds packed 32-byte records:

| Bytes | Meaning |
| --- | --- |
| 0 | kind: `0` base, `1` local, `2` zero |
| 1..4 | length, 24-bit big-endian, `1 ..= MAX_EXTENT` |
| 4..12 | source byte offset inside the extent's own origin |
| 12..20 | private payload identity (`Local` only) |
| 20..28 | payload custody page (`Local` only) |
| 28..32 | zero |

A **pieces branch** (`level > 0`) holds 16-byte child references: an 8-byte page
identity and an 8-byte subtree logical length. Every non-root branch keeps at
least two children, so the tree is canonical and its height is exactly what the
extent count requires.

`MAX_EXTENT` is `2^24 - 1`. A larger extent is stored as several adjacent parts,
which is a page boundary and never a new logical byte. The declared level
ceiling is 7, and every arithmetic step is checked; a file that cannot be
represented inside those bounds fails with `Capacity`.

## 3. Path-copied replacement

`metadata_pieces::replace` performs one accepted edit:

1. Walk the old tree in order, accumulating the logical start of each subtree
   from its own child lengths (and its canonical read offset from the extents).
2. Share every subtree the replaced interval does not touch, by reference. A
   subtree that ends exactly where the interval begins is carried the same way;
   the replacement is merged at the boundary above it.
3. In the leaves the interval touches, retain the bytes before it, drop the
   overlap, and retain the bytes after it. An extent that straddles a boundary is
   split; its retained part keeps reading its own origin from one byte past what
   the interval consumed.
4. Merge adjacent compatible extents while the new leaves are written.
5. Pack the folded leaves and every branch level above them; the result is a new
   root that shares all untouched pages with the old one.

`Fold` charges each published extent before the page that names it is written,
validating length, shape, canonical offsets inside the recorded base length and
the running edit count. A refused splice publishes no root and leaves the
previous sequence exactly as it was.

## 4. Bounded cursor

`metadata_pieces::Cursor` holds one page path and one leaf's records. A seek
subtracts subtree lengths on the way down; `next` steps through the leaf and
walks to the following leaf through the same path. Reads and one Commit walk
therefore visit `O(H + touched leaves)` index pages and never materialize a
file's whole extent list. The cursor checks the operation deadline between page
reads.

## 5. Complexity

| Path | Work per accepted callback |
| --- | --- |
| Local write index | `O(H + K)` extents folded plus the retained bytes; only touched leaves and their ancestors are written |
| One read at an offset | `O(H + touched leaves)` |
| Commit walk of one file | one ordered pass, bounded edit buffer and bounded cursor |

`H ≈ log_F P` for `P` extents, fanout `F` (a branch page holds 248 children, a
leaf 124 records) and `K` extents touched by one callback. The logical file
ceiling remains 4 GiB.

## 6. What this does not yet do

- Commit lowering streams the sequence through the cursor, but the replacement
  extents of one call are still materialized before the splice. A complete-file
  construction is therefore bounded by the file, not by a frame.
- The Branch request, server save and C1 builder still enforce the 256-edit,
  8 MiB replay and 1,024-piece ceilings. Removing them requires the coordinated
  streaming transport of #245 package D, not a local limit change.
- No continuously writable generation proof (package E) and no frozen
  performance selection (package F) exists.
