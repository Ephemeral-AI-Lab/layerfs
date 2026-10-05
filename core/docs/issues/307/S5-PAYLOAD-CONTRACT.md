# S5 payload contract: cells, layers, cutoff and holes

> **Status:** Selected algorithm contract for the S5 implementation, recorded
> before code. It resolves R1 for the overlay. It is a derivation, not evidence:
> the paired plans, runtime profiles and adversary proofs are S5 exit work.

## 1. Model

A regular file's local payload is a stack of **layers**, one per live generation
in which the inode has a row. Each layer owns its inode row values and the cells
written in that generation.

```text
 view(g, x) for byte x of the file as seen from generation g
   x >= size(g)                      -> beyond EOF
   cell(g) holds x, valid, not stale -> that byte          (written here)
   x >= cutoff(g)                    -> 0                  (hole / cut off)
   otherwise                         -> view(lower(g), x), 0 beyond its EOF
 lowest layer falls through to the immutable base file, 0 beyond its EOF
```

| Per layer | Meaning |
| --- | --- |
| `size` | Logical EOF of this layer's view |
| `cutoff` | Bytes at or above it never fall through to the lower view. Starts at the lower view's length when the layer is created; every shrink to `T` sets `min(cutoff, T)`. Invariant `cutoff <= size` |
| `epoch` | Shrinks performed in this layer |
| `height` | Entries of this layer's shrink staircase |
| cells | `(serial, gen, cell_offset)`, 4096-byte aligned; `data` holds only the bytes up to the last valid one; `validity` is absent when every stored byte is valid, else one bit per stored byte; each cell is stamped with the layer `epoch` at its last write |

The cutoff is relative to the layer immediately below, not to the base. Known
install replaces the layers at and below the sealed generation with a base whose
content is exactly their view, so the layers above stay correct without being
rewritten. A capture seals a layer: later writes create a new layer whose cells
hold only the newly written bytes. Nothing is copied up from a lower layer or
from the base by a write.

## 2. Write

For a request `[off, off + n)`, `n <= 128 KiB`, every intersecting cell is
handled independently:

- fully covered cell: one upsert of the supplied bytes, no read;
- partly covered cell: one point read of this layer's cell, merge at most 4096
  bytes and 512 mask bytes in memory, one upsert. A stale cell (§3) is treated as
  empty.

Then the inode row takes `size = max(size, off + n)` and the mtime, in the same
transaction. Cells touched: at most `ceil((n + 4095) / 4096)`, 33 for the
window. Bytes copied: at most `n` plus two edge cells. The cost does not depend
on how the range was written before: 65,536 alternating one-byte writes leave at
most 33 cell rows under a 128 KiB window, and overwriting that window reads at
most the two edge cells. There is no fragment list to walk. A written zero is a
valid byte; an unwritten byte is not.

A write beyond EOF creates no cell for the gap. Because `cutoff <= size`, the
gap is at or above the cutoff and reads as zero: a hole costs no rows.

## 3. Shrink and regrow without resurrecting bytes

Shrinking to `T` must hide this layer's cells at or above `T` and the lower
view at or above `T`, and a later regrow must not bring either back. Deleting
the cells would pause the request for work proportional to discarded data, which
the product contract forbids.

The lower view is handled by `cutoff = min(cutoff, T)`.

This layer's own cells are handled by staleness. Let `b` be `T` rounded up to a
cell boundary. The boundary cell below `b` is fixed eagerly: its bytes at or
above `T` are dropped, one cell. Cells at or above `b` become stale when their
stamp is older than the shrink. That needs, for a cell at offset `c`, the newest
shrink whose boundary is at or below `c`:

```text
 Z(c) = max { epoch_i : b_i <= c }      a cell is stale iff its stamp < Z(c)
```

`Z` is a non-decreasing step function of `c`. Its steps are the shrinks not
dominated by a later shrink at a lower boundary, in increasing boundary and
increasing epoch order: a staircase. A sequence of regrow-then-shrink at
increasing boundaries makes it arbitrarily tall, so a single number cannot
represent it, and dropping a step early would resurrect the bytes it hides.

The staircase is stored as rows `(serial, gen, depth) -> (boundary, epoch)` with
the live prefix `1..height` named by the inode row.

- **Shrink:** binary-search the live prefix for the first step with boundary
  `>= b`, set `height` to just below it, write the new step at `height + 1`,
  increment `epoch`. Dominated steps are abandoned by the height change, not
  deleted; later pushes overwrite them. O(log height) point reads, one step
  row, one inode row, at most one cell.
- **Staleness test:** a cell stamped with the current `epoch` is fresh with no
  lookup. Otherwise binary-search the live prefix for the last step with
  boundary `<= c`: O(log height) point reads.

Zero truncate is the same operation with `b = 0`: height becomes 1.

Worst case per request is logarithmic in the number of earlier shrinks of that
file in that generation, and constant for a file that was never shrunk. No
request does work proportional to discarded bytes, cells or earlier writes.
Amortized cost is the same; cumulative cost is linear in operations. Abandoned
step rows and stale cells are garbage that only bounded reclamation removes
(S6); until then they cost space, not correctness or foreground work.

## 4. Read

One owner job composes the local layers for a window of at most 128 KiB and
returns the bytes it resolved plus a bitmap of bytes that must come from the
base. Per layer it runs one indexed range read of that layer's cells inside the
window and, only for cells older than the layer's epoch, the staleness test.
Layers are visited newest first until every byte is resolved or the lowest
local layer is passed. Depth equals the live generations holding a row for the
inode, two with one capture in flight. Bounding that depth across repeated
failed captures is S6.

Base bytes are then fetched outside the owner as **one** range covering the
needed bytes, whatever their fragmentation, through the existing bounded base
read plan. Alternating valid and inherited bytes cost one base range, not one
demand per gap.

## 5. Space and amplification to report

A cell row stores its bytes trimmed to the last valid one, so a 100-byte file
costs one ~130-byte row rather than a fixed 4.6 KiB cell. A full 4096-byte cell
exceeds SQLite's local payload for 4096-byte pages and spills to one overflow
page, so dense data costs about 1.1 pages per cell plus its index entry. A
one-byte append rewrites its cell's stored bytes, at most 4096. These figures
are format arithmetic; S5 evidence reports the measured page counts.

## 6. Holes and canonical content

The overlay represents a hole as the absence of cells above the cutoff. The
canonical file model has no hole object: a hole is logically zero bytes and its
canonical identity is that of the zero-filled file. The owning content contract
therefore has to construct a zero run without reading or hashing its length.
The extent tree's branch summaries are cumulative within a node, so subtrees
over a uniform run are identical objects. Hole-aware construction is required
to emit the repeated zero-chunk extents and their repeated pages by reuse, in
work logarithmic in the run length plus one chunk of alignment at each end,
producing the same canonical root that streaming the zeros would. That
cluster-one change (P4) is implemented and proved in `layerfs-content`, not by
an overlay shortcut, and is required before S5 is accepted.

## 7. What this contract does not decide

Live reclamation of stale cells, abandoned steps and removed inodes; orphan
custody; failed-capture composition and its depth bound; physical reservations
(all S6). Backed deferred editing for heavily fragmented Commit input (P3) and
the Commit-side enumeration of data runs (S10). Kernel page-cache, mmap and
reply-buffer custody (S8).
