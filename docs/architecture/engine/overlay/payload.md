# Payload

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How the [Overlay](README.md) stores a Workspace's changed file bytes as cells, dense runs, shrink layers and reference pieces, and how reads compose them.

| | |
| --- | --- |
| Source | [`layerfs-overlay/src/payload`](../../../../engine/crates/layerfs-overlay/src/payload), [`schema.sql`](../../../../engine/crates/layerfs-overlay/sql/schema.sql) |

## Composition

```text
 read of [offset, offset+len)          len <= READ_WINDOW (128 KiB)
   layer: newest local inode row (active gen) ---- fs_data cells
     |                                        \--- fs_data_ref pieces
     |  bytes >= inherited_cutoff not written here: fall through
   layer: next lower local row (captured gen) ---- cells / pieces
     |  ... at most four rows (LAYERS ... LIMIT 4)
     |  beyond a lower layer's own size: zero
   committed base file (immutable) -------------- read by Workspace,
                                                   outside SQL
```

| Result field | Meaning | Source |
| --- | --- | --- |
| local bytes | Bytes a layer decided | [`stream.rs`](../../../../engine/crates/layerfs-overlay/src/payload/stream.rs) `compose_layers` |
| `inherited` mask | Bytes the base must supply | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) `LocalRead` |
| `references` | Ranges a reference piece supplies, at most `READ_REFERENCES` (33) | same |

`compose_layers` does not walk a range twice once it is decided, and folds a
dense row's range at once instead of testing byte by byte.

## Runs and cells

```text
file offset:  0        4096      8192           32768          65536
              | cell 0 | cell 1  | ...          | next slot ...
              |<---------- one slot (RUN_BYTES = 32 KiB) ------>|
fs_data row:  [ one cell: 1..4096 bytes + optional validity mask ]
          or  [ dense run: whole cells, no mask, inside one slot   ]
```

| Rule | Source |
| --- | --- |
| `cell_offset` is a multiple of `CELL_BYTES` (4096); a row is 1 to 32 768 bytes. Both are schema `CHECK`s, so a wider row is a format change | [`schema.sql`](../../../../engine/crates/layerfs-overlay/sql/schema.sql), [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| A one-cell row is trimmed to its last valid byte; `validity` (one bit per byte, `MASK_BYTES` 512) is absent when every stored byte was written, so a written zero differs from a hole | [`cells.rs`](../../../../engine/crates/layerfs-overlay/src/payload/cells.rs) |
| A wider row is dense, a whole number of cells, and never crosses a slot boundary; rows of one layer never share a cell; a sequential write stores one dense row per slot | [`runs.rs`](../../../../engine/crates/layerfs-overlay/src/payload/runs.rs) |
| Every row that can hold a byte is found by one indexed range of its slot (`CELL_COVER`, `CELL_RANGE`) | [`statements.rs`](../../../../engine/crates/layerfs-overlay/src/database/statements.rs) |
| Holes cost no rows | [`tests/payload.rs`](../../../../engine/crates/layerfs-overlay/tests/payload.rs) |

## Layers

```text
fs_inode row (serial, gen): size, inherited_cutoff, epoch, height
fs_truncate rows (serial, gen, depth 1..height): (boundary, epoch)

shrink to `to`:  cut the one row holding `to` (and the piece across it)
                 boundary = `to` rounded up to a cell; epoch + 1
                 depth d = first live step with boundary' >= boundary
                 put step (d, boundary, epoch); height = d
                 inherited_cutoff = min(cutoff, to)
                 queue STALE, STEPS (and STALE_PIECES) cleanup
cell stamped e is stale  <=>  e < epoch of the newest step whose
                              boundary <= cell   (binary search)
```

| Rule | Source |
| --- | --- |
| A layer is one live inode row: its EOF, its fall-through cutoff and the live prefix of its staircase | [`layers.rs`](../../../../engine/crates/layerfs-overlay/src/payload/layers.rs) |
| A shrink touches one row of at most `RUN_BYTES`, one step row and a binary search; it visits no discarded row. Hidden cells and steps go later through maintenance | [`layers.rs`](../../../../engine/crates/layerfs-overlay/src/payload/layers.rs), [`garbage.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/garbage.rs) |
| The first row of a serial in a generation starts a fresh layer whose cutoff is the lower view's length | [`inode.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/inode.rs) |

## Pieces

```text
fs_data_ref  (generation, inode, file_offset) -> length, epoch,
                                                source[32], source_offset
file bytes [file_offset, file_offset+length) of this layer
   == committed bytes of content root `source` from source_offset
```

| Rule | Source |
| --- | --- |
| A piece holds no bytes. Pieces of one layer never overlap, live or stale; `file_offset` is cell-aligned | [`schema.sql`](../../../../engine/crates/layerfs-overlay/sql/schema.sql) |
| Pieces come from a write job's `WriteClaims`: whole cells of a claimed range become one piece or extend the piece before it; ragged edges are stored as cells. The engine trusts the claim | [`pieces/rows.rs`](../../../../engine/crates/layerfs-overlay/src/payload/pieces/rows.rs) |
| At most `WRITE_CLAIMS` (`WRITE_WINDOW / 8192 + 2` = 34) ranges per write job | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| A layer whose `referenced` flag is 0 issues no piece statement; the `pieces_seen` hint false means no piece row exists in the file | [`layers.rs`](../../../../engine/crates/layerfs-overlay/src/payload/layers.rs), [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) |
| Folds and orphan migration move pieces into the layer above, one bounded step at a time | [`pieces/moves.rs`](../../../../engine/crates/layerfs-overlay/src/payload/pieces/moves.rs) |

## Digests

| Rule | Source |
| --- | --- |
| A regular file this Workspace creates is followed while every byte it gains is an append at its end; the BLAKE3 state (about 2 KiB) is kept in open state | [`digest.rs`](../../../../engine/crates/layerfs-overlay/src/payload/digest.rs) |
| A write elsewhere, a cell change, a size that differs from the followed length, a second link or a removal ends the following for good | same |
| When the last descriptor closes, `fs_digest` gets `(inode, sequence, length, digest)`; a metadata-only change at the same size moves the pin; any other row write leaves it stale | same, [`file_owners.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/file_owners.rs) |
| `captured_digests` returns a digest only for a captured row it still describes and a file with no open descriptor; at most `PAGE_ROWS` serials per call | [`digest.rs`](../../../../engine/crates/layerfs-overlay/src/payload/digest.rs) |

## Streams

| Call | One job | Source |
| --- | --- | --- |
| write (inside `apply`) | One `PayloadWrite` of at most `WRITE_WINDOW` (256 KiB): whole cells one slot at a time; a partial cell is one point read and one upsert | [`stream.rs`](../../../../engine/crates/layerfs-overlay/src/payload/stream.rs) |
| `source_read` | Current view over an owned `BaseSource`; an orphan reads its own composition | same |
| `read_file` | A descriptor read window (`FileRead`), valid after the descriptor closes | [`orphan.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/orphan.rs) |
| `captured_read` | Sealed view of a capture | [`stream.rs`](../../../../engine/crates/layerfs-overlay/src/payload/stream.rs) |
| `captured_run_step` | One forward unit of a captured file: `Continue`, `Gap` (`Inherited` or `Zero`), `Window`, `Local`, `Reference` or `End` | [`captured_runs.rs`](../../../../engine/crates/layerfs-overlay/src/payload/captured_runs.rs), [`captured_types.rs`](../../../../engine/crates/layerfs-overlay/src/payload/captured_types.rs) |

A larger write or read is several jobs; the windows bound one job, not a
file. A captured `Window` is at most `READ_WINDOW`: while the cursor's bytes
lie in consecutive live dense rows of the newest layer, they are read by one
`CELL_RANGE` statement and copied once (`dense_window`); any other shape is
composed by `compose_layers`. A shapes-only cursor reports such a run as
`Local`, up to 64 rows (`SHAPE_ROWS`). `PayloadWork` counts copied and zeroed
bytes per engine ([`diagnostics/payload.rs`](../../../../engine/crates/layerfs-overlay/src/diagnostics/payload.rs)).

## Related

- [Overlay](README.md): the tables, the job transaction and the Workspace lock.
- [Lifetimes](lifetimes.md): stale cells, steps and pieces, and orphan reads.
- [Workspace: Operations](../workspace/operations.md): base reads and recognition of reference pieces.
- [Commit: Construction](../commit/construction.md): the consumer of captured runs, digests and pieces.
