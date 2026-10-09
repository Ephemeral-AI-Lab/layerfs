//! Indexed forward metadata discovery before bounded byte composition.
use super::captured_types::CellMetadata;
use crate::{
    db::{integer, unsigned},
    layers::Layer,
    sql, CapturedGap, CapturedRunCursor, CapturedRunReply, CapturedRunStep, CapturedRunWork,
    InodeKind, Overlay, OverlayError, OverlayResult, StatementKind, CELL_BYTES, RUN_BYTES,
};

const CELL: u64 = CELL_BYTES as u64;
const RUN: u64 = RUN_BYTES as u64;

impl Overlay {
    /// Consume at most one metadata row per relevant retained layer (the
    /// existing at-most-four layer invariant). Stale rows return continuation;
    /// their original indexed seek positions survive later windows/gaps.
    pub fn captured_run_step(
        &self,
        mut cursor: CapturedRunCursor,
    ) -> OverlayResult<CapturedRunReply> {
        self.check_captured_reader(cursor.reader)?;
        let reader = cursor.reader;
        let ns = reader.capture().route().ns;
        let serial = integer(cursor.serial)?;
        let layers = self.layers(
            ns,
            serial,
            reader.capture().generation.number(),
            reader.installed,
        )?;
        if let Some(first) = layers.first() {
            if first.kind != InodeKind::File || first.nlink == 0 {
                return Err(OverlayError::Missing);
            }
            if first.size != cursor.size {
                return Err(OverlayError::Invalid("captured run size"));
            }
        }
        let mut generations = [0; 4];
        for (slot, layer) in layers.iter().enumerate() {
            generations[slot] = layer.gen;
        }
        if let Some(count) = cursor.layer_count {
            if count != layers.len() || cursor.generations != generations {
                return Err(OverlayError::Stale);
            }
        } else {
            cursor.layer_count = Some(layers.len());
            cursor.generations = generations;
        }
        let mut work = CapturedRunWork::default();
        if cursor.at == cursor.size {
            return Ok(CapturedRunReply {
                step: CapturedRunStep::End,
                work,
            });
        }
        let (needed, mut end, zero) = gap_bounds(&layers, cursor.at, cursor.size);
        for (slot, layer) in layers.iter().take(needed).enumerate() {
            let probe = &mut cursor.probes[slot];
            if probe.ready || probe.after >= layer.size {
                probe.ready = true;
                continue;
            }
            let rows = self.query(
                StatementKind::Payload,
                sql::CAPTURED_CELL_METADATA,
                &[
                    &ns,
                    &serial,
                    &layer.gen,
                    &integer(probe.after)?,
                    &integer(layer.size)?,
                    &integer(cursor.at - cursor.at % CELL)?,
                ],
                48,
                |row| {
                    let offset = unsigned(row, 0)?;
                    let epoch: i64 = row.get(1)?;
                    let length = unsigned(row, 2)?;
                    let mask: Option<u64> = row
                        .get::<_, Option<i64>>(3)?
                        .map(|v| u64::try_from(v).map_err(|_| rusqlite::Error::InvalidQuery))
                        .transpose()?;
                    // One cell at most, or a dense row of whole cells.
                    if offset % CELL != 0
                        || length == 0
                        || epoch < 0
                        || mask.is_some_and(|bytes| bytes != length.div_ceil(8))
                        || (length > CELL
                            && (mask.is_some()
                                || length % CELL != 0
                                || offset % RUN + length > RUN))
                    {
                        return Err(rusqlite::Error::InvalidQuery);
                    }
                    Ok((CellMetadata { offset, length }, epoch))
                },
            )?;
            match rows.into_iter().next() {
                None => probe.ready = true,
                Some((cell, epoch)) => {
                    work.metadata_rows += 1;
                    // The next exclusive seek never revisits this physical row.
                    probe.after = cell
                        .offset
                        .checked_add(cell.length.div_ceil(CELL) * CELL)
                        .ok_or(OverlayError::Invalid("captured cell boundary"))?;
                    if self.stale(ns, serial, layer, integer(cell.offset)?, epoch)? {
                        work.stale_rows += 1;
                    } else if cell.offset + cell.length > cursor.at {
                        probe.ready = true;
                        probe.cell = Some(cell);
                    }
                }
            }
        }
        if cursor.probes[..needed].iter().any(|probe| !probe.ready) {
            return Ok(CapturedRunReply {
                step: CapturedRunStep::Continue(cursor),
                work,
            });
        }
        // A window ends with the widest row that holds its first byte, so a
        // row is composed once, not once for each of its cells.
        let mut window = None;
        for probe in &cursor.probes[..needed] {
            if let Some(cell) = probe.cell {
                if cell.offset <= cursor.at {
                    let cells = cell.length.div_ceil(CELL) * CELL;
                    window = window.max(Some(cell.offset + cells));
                } else {
                    end = end.min(cell.offset);
                }
            }
        }
        let step = if let Some(row_end) = window {
            let to = cursor.size.min(row_end);
            let mut read = self
                .compose_layers(ns, serial, cursor.at, (to - cursor.at) as u32, layers)?
                .ok_or(OverlayError::Invalid("captured run layers"))?;
            read.base_root = Some(reader.capture().base_root);
            CapturedRunStep::Window {
                read: Box::new(read),
                next: cursor.advance(to),
            }
        } else {
            if end <= cursor.at {
                return Err(OverlayError::Invalid("captured run progress"));
            }
            CapturedRunStep::Gap {
                kind: if zero {
                    CapturedGap::Zero
                } else {
                    CapturedGap::Inherited
                },
                offset: cursor.at,
                length: end - cursor.at,
                next: cursor.advance(end),
            }
        };
        Ok(CapturedRunReply { step, work })
    }
}

fn gap_bounds(layers: &[Layer], at: u64, size: u64) -> (usize, u64, bool) {
    let mut end = size;
    for (depth, layer) in layers.iter().enumerate() {
        if depth != 0 {
            if at >= layer.size {
                return (depth, end, true);
            }
            end = end.min(layer.size);
        }
        if at >= layer.cutoff {
            return (depth + 1, end, true);
        }
        end = end.min(layer.cutoff);
    }
    (layers.len(), end, false)
}
