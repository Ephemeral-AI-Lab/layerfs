//! Per-generation payload layers and the shrink staircase of one inode.
use crate::{
    db::unsigned, sql, InodeKind, Overlay, OverlayError, OverlayResult, StatementKind, CELL_BYTES,
    RUN_BYTES,
};

const CELL: u64 = CELL_BYTES as u64;

/// One live inode row as a payload layer: its EOF, its fall-through cutoff to
/// the lower view, and the live prefix of its shrink staircase.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Layer {
    pub gen: i64,
    pub nlink: u64,
    pub kind: InodeKind,
    pub size: u64,
    pub cutoff: u64,
    pub epoch: i64,
    pub height: i64,
    /// Whether this layer has ever held a reference piece. A layer without
    /// one issues no piece statement.
    pub referenced: bool,
    /// Content sequence of the row as it was read: the revision that last
    /// wrote it. Negative for a row the running job inserts.
    pub sequence: i64,
}
impl Overlay {
    /// Rows of one inode, newest first, within `(installed, top]`.
    pub(crate) fn layers(
        &self,
        serial: i64,
        top: i64,
        installed: i64,
    ) -> OverlayResult<Vec<Layer>> {
        self.query(
            StatementKind::Inode,
            sql::LAYERS,
            &[&serial, &top, &installed],
            32,
            |row| {
                Ok(Layer {
                    gen: row.get(0)?,
                    kind: match row.get::<_, i64>(1)? {
                        1 => InodeKind::File,
                        2 => InodeKind::Directory,
                        3 => InodeKind::Symlink,
                        _ => return Err(rusqlite::Error::InvalidQuery),
                    },
                    size: unsigned(row, 2)?,
                    cutoff: unsigned(row, 3)?,
                    epoch: row.get(4)?,
                    height: row.get(5)?,
                    nlink: unsigned(row, 6)?,
                    referenced: row.get(7)?,
                    sequence: row.get(8)?,
                })
            },
        )
    }
    fn step(&self, serial: i64, gen: i64, depth: i64) -> OverlayResult<(i64, i64)> {
        self.query(
            StatementKind::Payload,
            sql::STEP_GET,
            &[&serial, &gen, &depth],
            32,
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .pop()
        .ok_or(OverlayError::Invalid("shrink staircase"))
    }
    /// Whether a cell stamped `stamp` was hidden by a later shrink of its layer.
    /// A cell stamped with the layer's current epoch needs no lookup; otherwise
    /// a binary search of the live staircase finds the newest shrink whose
    /// boundary is at or below the cell.
    pub(crate) fn stale(
        &self,
        serial: i64,
        layer: &Layer,
        cell: i64,
        stamp: i64,
    ) -> OverlayResult<bool> {
        if stamp == layer.epoch {
            return Ok(false);
        }
        let (mut low, mut high, mut newest) = (1, layer.height, 0);
        while low <= high {
            let middle = low + (high - low) / 2;
            let (boundary, epoch) = self.step(serial, layer.gen, middle)?;
            if boundary <= cell {
                newest = epoch;
                low = middle + 1;
            } else {
                high = middle - 1;
            }
        }
        Ok(stamp < newest)
    }
    pub(crate) fn store(
        &self,
        serial: i64,
        gen: i64,
        cell: i64,
        epoch: i64,
        stored: Option<(Vec<u8>, Option<Vec<u8>>)>,
    ) -> OverlayResult<()> {
        match stored {
            Some((data, validity)) => {
                let bound = 40 + data.len() + validity.as_ref().map_or(0, Vec::len);
                self.execute(
                    StatementKind::Payload,
                    sql::CELL_PUT,
                    &[&serial, &gen, &cell, &epoch, &data, &validity],
                    bound as u64,
                )?;
            }
            None => {
                self.execute(
                    StatementKind::Payload,
                    sql::CELL_DROP,
                    &[&serial, &gen, &cell],
                    32,
                )?;
            }
        }
        Ok(())
    }
    /// Shrinks one layer to `to` inside the caller's transaction: the row
    /// that holds the boundary loses its bytes at or above `to`, later rows
    /// become stale through one new staircase step, and the lower view is
    /// cut off. Work is one row of at most `RUN_BYTES`, one step row and a
    /// binary search; no discarded row is visited. A row is afterwards wholly
    /// below the boundary or wholly stale.
    pub(crate) fn shrink(&self, serial: i64, layer: &mut Layer, to: u64) -> OverlayResult<()> {
        let inside = to % CELL;
        // No row crosses a slot boundary: nothing straddles one.
        if to % RUN_BYTES as u64 != 0 {
            let cell = (to - inside) as i64;
            let found = self
                .covering(serial, layer.gen, cell)?
                .filter(|(shape, _)| inside != 0 || shape.offset < cell);
            if let Some((shape, stored)) = found {
                let stale = self.stale(serial, layer, shape.offset, shape.epoch)?;
                match stored.filter(|_| shape.masked && !stale) {
                    Some(stored) => {
                        let mut window = stored.expand(&self.payload_work)?;
                        window.cut(inside as usize);
                        let kept = window.trim(&self.payload_work);
                        self.store(serial, layer.gen, cell, stored.epoch, kept)?;
                    }
                    // A stale wide row stays wholly stale under the new step.
                    None if stale && !shape.wide() => self.drop_row(shape.row)?,
                    None if stale => {}
                    None if to as i64 - shape.offset >= shape.length => {}
                    // A dense row keeps its whole cells below the boundary
                    // cell and that cell's bytes below `to`, with its stamp.
                    None if shape.offset < cell => {
                        if inside != 0 {
                            let layer = (serial, layer.gen);
                            self.part(layer, &shape, cell, inside as i64)?;
                        }
                        self.keep(&shape, cell - shape.offset)?;
                    }
                    None => self.keep(&shape, inside as i64)?,
                }
            }
        }
        if layer.referenced {
            // The piece that crosses the new size ends at it. It keeps its
            // stamp: it starts below the boundary, so it is not stale.
            self.cut_piece(serial, layer.gen, to)?;
        }
        let boundary = to.div_ceil(CELL).checked_mul(CELL).map(i64::try_from);
        let Some(Ok(boundary)) = boundary else {
            return Err(OverlayError::Invalid("SQLite signed identity/offset"));
        };
        // First live step at or above the new boundary: it and every later one
        // are dominated. They are abandoned by the height change, not deleted.
        let (mut low, mut high, mut first) = (1, layer.height, layer.height + 1);
        while low <= high {
            let middle = low + (high - low) / 2;
            if self.step(serial, layer.gen, middle)?.0 >= boundary {
                first = middle;
                high = middle - 1;
            } else {
                low = middle + 1;
            }
        }
        layer.epoch = layer
            .epoch
            .checked_add(1)
            .ok_or(OverlayError::Invalid("shrink epoch exhausted"))?;
        layer.height = first;
        self.execute(
            StatementKind::Payload,
            sql::STEP_PUT,
            &[&serial, &layer.gen, &first, &boundary, &layer.epoch],
            48,
        )?;
        layer.cutoff = layer.cutoff.min(to);
        let cleaned = (layer.gen, layer.referenced);
        self.schedule_shrink(serial, cleaned, boundary, layer.height)?;
        Ok(())
    }
}
