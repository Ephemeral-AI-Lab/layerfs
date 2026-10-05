//! Per-generation payload layers and the shrink staircase of one inode.
use crate::{
    cells::Stored, db::unsigned, sql, InodeKind, Overlay, OverlayError, OverlayResult,
    StatementKind, CELL_BYTES,
};

const CELL: u64 = CELL_BYTES as u64;

/// One live inode row as a payload layer: its EOF, its fall-through cutoff to
/// the lower view, and the live prefix of its shrink staircase.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Layer {
    pub gen: i64,
    pub kind: InodeKind,
    pub size: u64,
    pub cutoff: u64,
    pub epoch: i64,
    pub height: i64,
}
impl Overlay {
    /// Rows of one inode, newest first, within `(installed, top]`.
    pub(crate) fn layers(
        &self,
        ns: i64,
        serial: i64,
        top: i64,
        installed: i64,
    ) -> OverlayResult<Vec<Layer>> {
        self.query(
            StatementKind::Inode,
            sql::LAYERS,
            &[&ns, &serial, &top, &installed],
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
                })
            },
        )
    }
    fn step(&self, ns: i64, serial: i64, gen: i64, depth: i64) -> OverlayResult<(i64, i64)> {
        self.query(
            StatementKind::Payload,
            sql::STEP_GET,
            &[&ns, &serial, &gen, &depth],
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
        ns: i64,
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
            let (boundary, epoch) = self.step(ns, serial, layer.gen, middle)?;
            if boundary <= cell {
                newest = epoch;
                low = middle + 1;
            } else {
                high = middle - 1;
            }
        }
        Ok(stamp < newest)
    }
    pub(crate) fn stored(
        &self,
        ns: i64,
        serial: i64,
        gen: i64,
        cell: i64,
    ) -> OverlayResult<Option<Stored>> {
        Ok(self
            .query(
                StatementKind::Payload,
                sql::CELL_LOOKUP,
                &[&ns, &serial, &gen, &cell],
                32,
                |row| Stored::decode(row, 0),
            )?
            .pop())
    }
    pub(crate) fn store(
        &self,
        ns: i64,
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
                    &[&ns, &serial, &gen, &cell, &epoch, &data, &validity],
                    bound as u64,
                )?;
            }
            None => {
                self.execute(
                    StatementKind::Payload,
                    sql::CELL_DROP,
                    &[&ns, &serial, &gen, &cell],
                    32,
                )?;
            }
        }
        Ok(())
    }
    /// Shrinks one layer to `to` inside the caller's transaction: the boundary
    /// cell loses its bytes at or above `to`, later cells become stale through
    /// one new staircase step, and the lower view is cut off. Work is one cell,
    /// one step row and a binary search; no discarded cell is visited.
    pub(crate) fn shrink(
        &self,
        ns: i64,
        serial: i64,
        layer: &mut Layer,
        to: u64,
    ) -> OverlayResult<()> {
        let inside = to % CELL;
        if inside != 0 {
            let cell = (to - inside) as i64;
            if let Some(stored) = self.stored(ns, serial, layer.gen, cell)? {
                let kept = if self.stale(ns, serial, layer, cell, stored.epoch)? {
                    None
                } else {
                    let mut window = stored.expand()?;
                    window.cut(inside as usize);
                    window.trim()
                };
                self.store(ns, serial, layer.gen, cell, stored.epoch, kept)?;
            }
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
            if self.step(ns, serial, layer.gen, middle)?.0 >= boundary {
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
            &[&ns, &serial, &layer.gen, &first, &boundary, &layer.epoch],
            48,
        )?;
        layer.cutoff = layer.cutoff.min(to);
        self.schedule_shrink(ns, serial, layer.gen, boundary, layer.height)?;
        Ok(())
    }
}
