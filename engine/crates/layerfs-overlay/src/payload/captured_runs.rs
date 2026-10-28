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
/// Rows one shapes-only unit may join; a fixed bound on its statements.
const SHAPE_ROWS: usize = 64;

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
        let serial = integer(cursor.serial)?;
        let layers = self.layers(
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
            return Ok(reply(CapturedRunStep::End, work));
        }
        if layers.iter().any(|layer| layer.referenced) {
            // A layer holds pieces: the layers decide in order.
            return self.captured_piece_step(serial, layers, cursor, work);
        }
        let (needed, mut end, zero) = gap_bounds(&layers, cursor.at, cursor.size);
        for (slot, layer) in layers.iter().take(needed).enumerate() {
            let probe = &mut cursor.probes[slot];
            self.probe_cell(serial, layer, probe, cursor.at, &mut work)?;
        }
        if cursor.probes[..needed].iter().any(|probe| !probe.ready) {
            return Ok(reply(CapturedRunStep::Continue(cursor), work));
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
        let step = if let Some(next) = self.local_shape(serial, &layers, cursor, &mut work)? {
            CapturedRunStep::Local {
                offset: cursor.at,
                length: next.at - cursor.at,
                next,
            }
        } else if let Some(row_end) = window {
            let to = cursor.size.min(row_end);
            let mut read = self
                .compose_layers(serial, cursor.at, (to - cursor.at) as u32, layers)?
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
        Ok(reply(step, work))
    }
}

impl Overlay {
    /// Makes one layer's probe ready for byte `at`, or consumes one stale
    /// row on the way: at most one metadata row. A ready probe holds the
    /// first live row of the layer that ends beyond the byte, or none.
    pub(crate) fn probe_cell(
        &self,
        serial: i64,
        layer: &Layer,
        probe: &mut super::captured_types::Probe,
        at: u64,
        work: &mut CapturedRunWork,
    ) -> OverlayResult<()> {
        if probe.ready || probe.after >= layer.size {
            probe.ready = true;
            return Ok(());
        }
        let floor = at - at % CELL;
        match self.next_cell(serial, layer, probe.after, floor)? {
            None => probe.ready = true,
            Some((cell, epoch)) => {
                work.metadata_rows += 1;
                // The next exclusive seek never revisits this physical row.
                probe.after = cell
                    .offset
                    .checked_add(cell.length.div_ceil(CELL) * CELL)
                    .ok_or(OverlayError::Invalid("captured cell boundary"))?;
                if self.stale(serial, layer, integer(cell.offset)?, epoch)? {
                    work.stale_rows += 1;
                } else if cell.offset + cell.length > at {
                    probe.ready = true;
                    probe.cell = Some(cell);
                }
            }
        }
        Ok(())
    }
    /// The first physical row of one layer at or after `after` that ends
    /// beyond `floor`, with its epoch. One indexed statement.
    fn next_cell(
        &self,
        serial: i64,
        layer: &Layer,
        after: u64,
        floor: u64,
    ) -> OverlayResult<Option<(CellMetadata, i64)>> {
        let rows = self.query(
            StatementKind::Payload,
            sql::CAPTURED_CELL_METADATA,
            &[
                &serial,
                &layer.gen,
                &integer(after)?,
                &integer(layer.size)?,
                &integer(floor)?,
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
                        && (mask.is_some() || length % CELL != 0 || offset % RUN + length > RUN))
                {
                    return Err(rusqlite::Error::InvalidQuery);
                }
                let dense = mask.is_none();
                Ok((
                    CellMetadata {
                        offset,
                        length,
                        dense,
                    },
                    epoch,
                ))
            },
        )?;
        Ok(rows.into_iter().next())
    }
    /// For a shapes-only cursor whose first byte lies in a live dense row of
    /// the newest layer: the cursor after that row and the dense live rows
    /// that follow it without a gap, at most `SHAPE_ROWS` physical rows. The
    /// newest layer decides those bytes whatever lower layers hold, so no
    /// payload is composed. None leaves the cursor to the ordinary window.
    pub(crate) fn local_shape(
        &self,
        serial: i64,
        layers: &[Layer],
        mut cursor: CapturedRunCursor,
        work: &mut CapturedRunWork,
    ) -> OverlayResult<Option<CapturedRunCursor>> {
        let (Some(layer), true) = (layers.first(), cursor.shapes) else {
            return Ok(None);
        };
        let Some(first) = cursor.probes[0]
            .cell
            .filter(|cell| cell.dense && cell.offset <= cursor.at)
        else {
            return Ok(None);
        };
        let mut to = cursor.size.min(first.offset + first.length);
        let mut rows = 1;
        // A row that stops inside a cell leaves that cell's tail undecided.
        while rows < SHAPE_ROWS && to < cursor.size && to % CELL == 0 {
            let probe = &mut cursor.probes[0];
            if probe.after >= layer.size {
                break;
            }
            let Some((cell, epoch)) = self.next_cell(serial, layer, probe.after, to)? else {
                probe.cell = None;
                probe.ready = true;
                break;
            };
            rows += 1;
            work.metadata_rows += 1;
            probe.after = cell
                .offset
                .checked_add(cell.length.div_ceil(CELL) * CELL)
                .ok_or(OverlayError::Invalid("captured cell boundary"))?;
            if self.stale(serial, layer, integer(cell.offset)?, epoch)? {
                work.stale_rows += 1;
                continue;
            }
            probe.cell = Some(cell);
            probe.ready = true;
            if cell.offset != to || !cell.dense {
                break;
            }
            to = cursor.size.min(cell.offset + cell.length);
        }
        Ok(Some(cursor.advance(to)))
    }
}

/// The reply of a unit of a file without pieces.
pub(crate) fn reply(step: CapturedRunStep, work: CapturedRunWork) -> CapturedRunReply {
    CapturedRunReply {
        step,
        work,
        referenced: false,
    }
}
/// The layers that can decide byte `at`, the end of the gap that starts
/// there when none does, and whether that gap is zero.
pub(crate) fn gap_bounds(layers: &[Layer], at: u64, size: u64) -> (usize, u64, bool) {
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
