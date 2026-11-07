# Payload

[Architecture](../../README.md) › [Engine](../README.md) › [Overlay](README.md) › Payload

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Changed file bytes: pieces, runs and layers.

| | |
| --- | --- |
| Component | [Overlay](README.md) |
| Source | [`engine/crates/layerfs-overlay/src/payload`](../../../../engine/crates/layerfs-overlay/src/payload) |

A file's bytes in a Workspace come from up to four places, newest first:

```text
 read of [offset, offset+len)  (at most READ_WINDOW = 128 KiB)
   layer: newest local inode row (active gen) ---- fs_data cells
     |                                        \--- fs_data_ref pieces
     |  bytes >= inherited_cutoff not written here: fall through
   layer: next lower local row (captured gen) ---- cells / pieces
     |
   layer: ... (at most four rows read by LAYERS)
     |  beyond a lower layer's own size: zero
   committed base file (immutable) -------------- read by Workspace
                                                   outside SQL
```

`compose_layers` returns one `LocalRead`: the locally decided bytes,
an `inherited` bit mask for bytes the base must supply, and `references`
for bytes a reference piece supplies. Source:
[`stream.rs`](../../../../engine/crates/layerfs-overlay/src/payload/stream.rs),
[`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) (`LocalRead`).

## Pieces

```text
fs_data_ref  (generation, inode, file_offset) -> length, epoch,
                                                source[32], source_offset
file bytes [file_offset, file_offset+length) of this layer
   == committed bytes of content root `source` from source_offset
```

| Rule | Source |
| --- | --- |
| A piece holds no bytes. Pieces of one layer never overlap, live or stale; `file_offset` is cell-aligned. | [`schema.sql`](../../../../engine/crates/layerfs-overlay/sql/schema.sql) |
| Pieces come from `WriteClaims` on a write job: whole cells of a claimed range become one piece or extend the piece before them; ragged edges are stored as cells. The engine trusts the claim. | [`pieces/rows.rs`](../../../../engine/crates/layerfs-overlay/src/payload/pieces/rows.rs), [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| At most `WRITE_CLAIMS` (18) ranges per write job; a claim that reaches before the window is honoured only while the content sequence is still `after`. | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| A layer whose `referenced` flag is 0 issues no piece statement; `pieces_seen` false means no piece row exists in the file. | [`layers.rs`](../../../../engine/crates/layerfs-overlay/src/payload/layers.rs), [`connection.rs`](../../../../engine/crates/layerfs-overlay/src/database/connection.rs) |
| A read window reports at most `READ_REFERENCES` (33) reference ranges. | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |

Folded captures and orphan migration move pieces into the layer above one
bounded step at a time ([`pieces/moves.rs`](../../../../engine/crates/layerfs-overlay/src/payload/pieces/moves.rs)).

## Runs and cells

```text
file offset:  0        4096      8192           32768          65536
              | cell 0 | cell 1  | ...          | next slot ...
              |<------------- one slot (RUN_BYTES = 32 KiB) ------>|
fs_data row:  [ one cell: 1..4096 bytes + optional validity mask ]
          or  [ dense run: whole cells, no mask, inside one slot   ]
```

| Rule | Source |
| --- | --- |
| `cell_offset` is a multiple of 4096 (`CELL_BYTES`). A row is at most 32 768 bytes. | [`schema.sql`](../../../../engine/crates/layerfs-overlay/sql/schema.sql), [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| A one-cell row is trimmed to its last valid byte; `validity` (one bit per byte, `MASK_BYTES` 512) is absent when every stored byte is written, so a written zero differs from a hole. | [`cells.rs`](../../../../engine/crates/layerfs-overlay/src/payload/cells.rs) |
| A wider row is dense, a whole number of cells, and never crosses a slot boundary; rows of one layer never share a cell. | [`runs.rs`](../../../../engine/crates/layerfs-overlay/src/payload/runs.rs) |
| Every row that can hold a byte is found by one indexed range of its slot (`CELL_COVER`, `CELL_RANGE`). | [`statements.rs`](../../../../engine/crates/layerfs-overlay/src/database/statements.rs) |
| Holes cost no rows. | [`tests/payload.rs`](../../../../engine/crates/layerfs-overlay/tests/payload.rs) |
| The 32 768-byte row limit and the slot rule are schema `CHECK`s, so a wider row is a format change; a sequential write stores one dense row per slot. | [`schema.sql`](../../../../engine/crates/layerfs-overlay/sql/schema.sql) |

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
| A layer is one live inode row: its EOF, its fall-through cutoff and the live prefix of its staircase. | [`layers.rs`](../../../../engine/crates/layerfs-overlay/src/payload/layers.rs) |
| A shrink touches one row of at most `RUN_BYTES`, one step row and a binary search; no discarded row is visited. Dominated steps are abandoned by the height change; hidden cells and steps are removed later by maintenance. | [`layers.rs`](../../../../engine/crates/layerfs-overlay/src/payload/layers.rs), [`garbage.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/garbage.rs) |
| The first row of a serial in a generation starts a fresh layer whose cutoff is the lower view's length. | [`inode.rs`](../../../../engine/crates/layerfs-overlay/src/namespace/inode.rs) |

## Digests

| Rule | Source |
| --- | --- |
| A regular file this Workspace creates is followed while every byte it gains is an append at its end; the BLAKE3 state (about 2 KiB) is kept in open state. | [`digest.rs`](../../../../engine/crates/layerfs-overlay/src/payload/digest.rs) |
| A write elsewhere, a cell change, a size that differs from the followed length, a second link or a removal ends the following for good. | [`digest.rs`](../../../../engine/crates/layerfs-overlay/src/payload/digest.rs) |
| When the last descriptor closes, `fs_digest` gets `(inode, sequence, length, digest)`; a metadata-only change at the same size moves the pin; any other row write leaves it stale. | [`digest.rs`](../../../../engine/crates/layerfs-overlay/src/payload/digest.rs), [`file_owners.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/file_owners.rs) |
| `captured_digests` returns a digest only for a captured row it still describes and only when the file has no open descriptor; at most `PAGE_ROWS` serials per call. | [`digest.rs`](../../../../engine/crates/layerfs-overlay/src/payload/digest.rs) |

## Streams

| Call | Job | Source |
| --- | --- | --- |
| `write_cells` (inside `apply`) | One `PayloadWrite` of at most `WRITE_WINDOW` (256 KiB): whole cells one slot at a time, row shapes read once per read window of slots ahead; a partial cell is one point read and one upsert | [`stream.rs`](../../../../engine/crates/layerfs-overlay/src/payload/stream.rs) |
| `source_read` | Current view over an owned `BaseSource`; orphans read their own composition | [`stream.rs`](../../../../engine/crates/layerfs-overlay/src/payload/stream.rs) |
| `read_file` | A descriptor read window (`FileRead`), independent of descriptor close | [`orphan.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/orphan.rs) |
| `captured_read` | Sealed view of a capture | [`stream.rs`](../../../../engine/crates/layerfs-overlay/src/payload/stream.rs) |
| `captured_run_step` | One forward metadata unit of a captured file: `Continue`, `Gap` (`Inherited` or `Zero`), `Window`, `Local`, `Reference` or `End` | [`captured_runs.rs`](../../../../engine/crates/layerfs-overlay/src/payload/captured_runs.rs), [`captured_types.rs`](../../../../engine/crates/layerfs-overlay/src/payload/captured_types.rs) |

A larger write or read is several jobs; the windows bound one job, not a
file. `PayloadWork` counts copies and zeroed bytes per engine
([`diagnostics/payload.rs`](../../../../engine/crates/layerfs-overlay/src/diagnostics/payload.rs)).

A captured `Window` is at most `READ_WINDOW` (owner approval, 2026-11-07;
it was one row). When the cursor's byte lies in a live dense row of the
newest layer, the step follows that layer's next rows while they are live,
dense, start where the last one ended and fit `READ_WINDOW`: one metadata
row each, as before. Those bytes are the newest layer's whatever lower
layers hold, so they are read by one `CELL_RANGE` statement and copied
once from SQLite into the window, with no decision map and no zeroing
(`dense_window`). Any other shape is composed by `compose_layers` over the
widest row that holds the byte. A shapes-only cursor reports the same dense
run as `Local`, up to 64 rows. In `compose_layers`, a range already decided
is not walked again and the dense-row check folds its range instead of
testing byte by byte.

## Related

- [Namespace](namespace.md): the inode rows that are layers.
- [Lifetimes and reclamation](lifetimes-and-reclamation.md): stale cells, steps and pieces.
- [Workspace: Operations](../workspace/operations.md): base reads and composition outside SQL.
- [Commit: Construction](../commit/construction.md): consumer of captured runs, digests and pieces.
- [Glossary](../../glossary.md).

---

← [Overlay: Namespace](namespace.md) · [Up: Overlay](README.md) · [Overlay: Open state](open-state.md) →
