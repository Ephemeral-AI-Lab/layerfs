//! The captured run step of a file whose layers hold reference pieces.
use crate::{
    layers::Layer, payload::captured_runs::gap_bounds, CapturedGap, CapturedRunCursor,
    CapturedRunReply, CapturedRunStep, CapturedRunWork, Overlay, OverlayError, OverlayResult,
    CELL_BYTES,
};

const CELL: u64 = CELL_BYTES as u64;
fn reply(step: CapturedRunStep, work: CapturedRunWork) -> CapturedRunReply {
    CapturedRunReply {
        step,
        work,
        referenced: true,
    }
}

impl Overlay {
    /// One unit of the captured run cursor for a file with a layer that holds
    /// pieces. The layers decide the cursor's byte in order, each by its cell
    /// first and then by its piece, as the layered read does:
    ///
    /// - a cell over the byte gives the composed window of that row, which
    ///   reports any piece beneath its unwritten bytes as reference ranges;
    /// - a live piece over the byte gives `Reference`, up to the piece's end,
    ///   the next row of this or a newer layer, and the bounds of the gap;
    /// - otherwise the next layer decides, and after the last one the unit is
    ///   a gap that ends before the next cell or piece of any layer asked.
    ///
    /// Pieces are sought by key at the cursor's byte: at most two rows of a
    /// layer are read, whatever the length of a piece, and the cursor keeps
    /// no piece state. A stale piece only bounds the unit.
    pub(crate) fn captured_piece_step(
        &self,
        serial: i64,
        layers: Vec<Layer>,
        mut cursor: CapturedRunCursor,
        mut work: CapturedRunWork,
    ) -> OverlayResult<CapturedRunReply> {
        let at = cursor.at;
        let (needed, mut end, zero) = gap_bounds(&layers, at, cursor.size);
        for (slot, layer) in layers.iter().take(needed).enumerate() {
            self.probe_cell(serial, layer, &mut cursor.probes[slot], at, &mut work)?;
            if !cursor.probes[slot].ready {
                return Ok(reply(CapturedRunStep::Continue(cursor), work));
            }
            if let Some(cell) = cursor.probes[slot].cell {
                if cell.offset <= at {
                    if slot == 0 {
                        let local = self.local_shape(serial, &layers, cursor, &mut work)?;
                        if let Some(next) = local {
                            let step = CapturedRunStep::Local {
                                offset: at,
                                length: next.at - at,
                                next,
                            };
                            return Ok(reply(step, work));
                        }
                    }
                    let to = cursor
                        .size
                        .min(cell.offset + cell.length.div_ceil(CELL) * CELL);
                    let mut read = self
                        .compose_layers(serial, at, (to - at) as u32, layers)?
                        .ok_or(OverlayError::Invalid("captured run layers"))?;
                    read.base_root = Some(cursor.reader.capture().base_root);
                    let step = CapturedRunStep::Window {
                        read: Box::new(read),
                        next: cursor.advance(to),
                    };
                    return Ok(reply(step, work));
                }
                end = end.min(cell.offset);
            }
            if !layer.referenced {
                continue;
            }
            let place = crate::db::integer(at)?;
            if let Some(piece) = self.piece_cover(layer.gen, serial, place)? {
                work.metadata_rows += 1;
                if piece.end() as u64 > at {
                    if !self.stale(serial, layer, piece.offset, piece.epoch)? {
                        let to = end.min(piece.end() as u64);
                        if to <= at {
                            return Err(OverlayError::Invalid("captured run progress"));
                        }
                        let step = CapturedRunStep::Reference {
                            offset: at,
                            length: to - at,
                            root: piece.source,
                            source_offset: piece.source_offset as u64 + (at - piece.offset as u64),
                            next: cursor.advance(to),
                        };
                        return Ok(reply(step, work));
                    }
                    work.stale_rows += 1;
                }
            }
            // No piece of this layer starts between the byte and this row.
            if let Some(piece) = self.piece_next(layer.gen, serial, place + 1)? {
                work.metadata_rows += 1;
                end = end.min(piece.offset as u64);
            }
        }
        if end <= at {
            return Err(OverlayError::Invalid("captured run progress"));
        }
        let step = CapturedRunStep::Gap {
            kind: if zero {
                CapturedGap::Zero
            } else {
                CapturedGap::Inherited
            },
            offset: at,
            length: end - at,
            next: cursor.advance(end),
        };
        Ok(reply(step, work))
    }
}
