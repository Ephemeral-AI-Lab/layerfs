//! Rows of several cells: their shapes, in-place writes, splits and moves.
use crate::{
    cells::{Stored, Window},
    db::integer,
    layers::Layer,
    sql, Overlay, OverlayError, OverlayResult, StatementKind, CELL_BYTES, RUN_BYTES,
};
use rusqlite::blob::Blob;

const CELL: i64 = CELL_BYTES as i64;
const RUN: i64 = RUN_BYTES as i64;

/// One stored row without its bytes. A row of at most one cell may carry a
/// validity mask and end inside its cell. A wider row is dense, a whole
/// number of cells, and lies inside one aligned slot of `RUN_BYTES`. Rows of
/// one layer never share a cell, live or stale.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shape {
    pub row: i64,
    pub offset: i64,
    pub epoch: i64,
    pub length: i64,
    pub masked: bool,
}
impl Shape {
    pub fn decode(row: &rusqlite::Row<'_>, masked: bool) -> rusqlite::Result<Self> {
        let shape = Self {
            row: row.get(0)?,
            offset: row.get(1)?,
            epoch: row.get(2)?,
            length: row.get(3)?,
            masked,
        };
        if shape.offset < 0
            || shape.offset % CELL != 0
            || shape.length < 1
            || (shape.wide()
                && (masked || shape.length % CELL != 0 || shape.offset % RUN + shape.length > RUN))
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(shape)
    }
    pub fn wide(&self) -> bool {
        self.length > CELL
    }
    /// Exclusive end of the cells this row occupies in its layer.
    pub fn end(&self) -> i64 {
        self.offset + self.length.max(CELL)
    }
}
/// First cell of the aligned slot that holds every row able to reach `cell`.
pub(crate) fn slot(cell: i64) -> i64 {
    cell - cell % RUN
}
impl Overlay {
    /// The row that occupies `cell` in one layer: the last row at or before
    /// it in its slot, when it reaches the cell. A row of one cell comes
    /// with its bytes; a wider row is never loaded.
    pub(crate) fn covering(
        &self,
        ns: i64,
        serial: i64,
        gen: i64,
        cell: i64,
    ) -> OverlayResult<Option<(Shape, Option<Stored>)>> {
        let found = self
            .query(
                StatementKind::Payload,
                sql::CELL_COVER,
                &[&ns, &serial, &gen, &slot(cell), &cell],
                40,
                |row| {
                    let validity: Option<Vec<u8>> = row.get(5)?;
                    let shape = Shape::decode(row, validity.is_some())?;
                    let data: Option<Vec<u8>> = row.get(4)?;
                    Ok((
                        shape,
                        data.map(|data| Stored {
                            epoch: shape.epoch,
                            data,
                            validity,
                        }),
                    ))
                },
            )?
            .pop();
        Ok(found.filter(|(shape, _)| shape.end() > cell))
    }
    /// Shapes of the rows of one layer that occupy a cell of `[from, to)`,
    /// in order. The range lies inside one slot, so at most one slot of rows
    /// is visited and no bytes are loaded.
    pub(crate) fn shapes(
        &self,
        ns: i64,
        serial: i64,
        gen: i64,
        from: i64,
        to: i64,
    ) -> OverlayResult<Vec<Shape>> {
        let mut shapes = self.query(
            StatementKind::Payload,
            sql::CELL_SHAPES,
            &[&ns, &serial, &gen, &slot(from), &to],
            40,
            |row| Shape::decode(row, row.get::<_, Option<i64>>(4)?.is_some()),
        )?;
        shapes.retain(|shape| shape.end() > from);
        Ok(shapes)
    }
    pub(crate) fn drop_row(&self, row: i64) -> OverlayResult<()> {
        self.execute(StatementKind::Payload, sql::ROW_DROP, &[&row], 8)?;
        Ok(())
    }
    /// `length` stored bytes of one row from byte `from` of its data.
    pub(crate) fn row_bytes(&self, row: i64, from: i64, length: i64) -> OverlayResult<Vec<u8>> {
        self.query(
            StatementKind::Payload,
            sql::ROW_SLICE,
            &[&row, &(from + 1), &length],
            24,
            |row| row.get(0),
        )?
        .pop()
        .ok_or(OverlayError::Invalid("stored row"))
    }
    /// A new dense row at cell `at` that takes over `length` bytes of a
    /// dense row from that cell on, with the row's stamp. The caller cuts
    /// or drops the row in the same job.
    pub(crate) fn part(
        &self,
        (ns, serial, gen): (i64, i64, i64),
        shape: &Shape,
        at: i64,
        length: i64,
    ) -> OverlayResult<()> {
        let data = self.row_bytes(shape.row, at - shape.offset, length)?;
        self.execute(
            StatementKind::Payload,
            sql::CELL_PUT,
            &[
                &ns,
                &serial,
                &gen,
                &at,
                &shape.epoch,
                &data,
                &None::<Vec<u8>>,
            ],
            40 + data.len() as u64,
        )?;
        Ok(())
    }
    /// Overwrites bytes of one live dense row where they are stored. The row
    /// keeps its length, stamp and index entries and no trigger runs. This is
    /// not a statement: it is refused unless an earlier statement of the same
    /// job has passed admission and begun the transaction.
    pub(crate) fn write_in_place<'a>(
        &'a self,
        blob: &mut Option<Blob<'a>>,
        row: i64,
        at: i64,
        bytes: &[u8],
    ) -> OverlayResult<()> {
        self.available()?;
        if !self.writing() {
            return Err(OverlayError::Invalid("in-place write before admission"));
        }
        let start = std::time::Instant::now();
        let written = match blob {
            Some(open) => open.reopen(row),
            None => self
                .connection
                .blob_open(rusqlite::MAIN_DB, "payload", "data", row, false)
                .map(|open| *blob = Some(open)),
        }
        .and_then(|()| match blob {
            Some(open) => open.write_at(bytes, at as usize),
            None => Err(rusqlite::Error::InvalidQuery),
        });
        let mut observed = self.payload_work.get();
        observed.in_place_writes = observed.in_place_writes.saturating_add(1);
        observed.in_place_bytes = observed.in_place_bytes.saturating_add(bytes.len() as u64);
        observed.in_place_ns = observed
            .in_place_ns
            .saturating_add(start.elapsed().as_nanos().min(u64::MAX as u128) as u64);
        self.payload_work.set(observed);
        written.map_err(|cause| self.failed(cause.into()))
    }
    /// Removes cells `[from, to)` from one live dense row. What it holds
    /// below and above stays, as rows of their own with the same stamp.
    pub(crate) fn carve(
        &self,
        layer: (i64, i64, i64),
        shape: &Shape,
        from: i64,
        to: i64,
    ) -> OverlayResult<()> {
        let end = shape.offset + shape.length;
        if to < end {
            self.part(layer, shape, to, end - to)?;
        }
        if from > shape.offset {
            self.keep(shape, from - shape.offset)
        } else {
            self.drop_row(shape.row)
        }
    }
    /// Cuts one dense row to its first `length` bytes.
    pub(crate) fn keep(&self, shape: &Shape, length: i64) -> OverlayResult<()> {
        self.execute(
            StatementKind::Payload,
            sql::ROW_KEEP,
            &[&shape.row, &length],
            16,
        )?;
        Ok(())
    }
    /// Moves the lower row that starts at `cell` into the upper layer and
    /// removes it from the lower one, atomically with the caller's cursor
    /// advance. The row itself moves when the upper layer has no live row
    /// over its cells and every stored byte is effective. Otherwise its
    /// effective bytes are merged below the upper bytes one cell at a time.
    /// One call handles one row: at most `RUN_BYTES`.
    pub(crate) fn transfer_row(
        &self,
        ns: i64,
        serial: i64,
        lower: &Layer,
        upper: &Layer,
        cell: i64,
    ) -> OverlayResult<u64> {
        let Some((old, bytes)) = self.covering(ns, serial, lower.gen, cell)? else {
            return Ok(0);
        };
        if self.stale(ns, serial, lower, old.offset, old.epoch)? {
            self.drop_row(old.row)?;
            return Ok(0);
        }
        let limit = integer(lower.size.min(upper.cutoff))?;
        let above = self.shapes(ns, serial, upper.gen, old.offset, old.end())?;
        let mut live = false;
        for shape in &above {
            live = live || !self.stale(ns, serial, upper, shape.offset, shape.epoch)?;
        }
        if !live && old.offset + old.length <= limit {
            for shape in &above {
                self.drop_row(shape.row)?;
            }
            self.execute(
                StatementKind::Payload,
                sql::ROW_MOVE,
                &[&old.row, &upper.gen, &upper.epoch],
                24,
            )?;
            return Ok(old.length as u64);
        }
        let stored = match bytes {
            Some(stored) => stored,
            None => Stored {
                epoch: old.epoch,
                data: self.row_bytes(old.row, 0, old.length)?,
                validity: None,
            },
        };
        let mut copied = 0;
        for (index, data) in stored.data.chunks(CELL_BYTES).enumerate() {
            let at = old.offset + index as i64 * CELL;
            let end = (limit - at).clamp(0, data.len() as i64) as usize;
            copied += self.merge_below(ns, serial, upper, at, &stored, index * CELL_BYTES, end)?;
        }
        self.drop_row(old.row)?;
        Ok(copied)
    }
    /// Copies the valid bytes `[from, from + end)` of a lower row into the
    /// positions of upper cell `cell` that hold no upper byte. Upper shrink
    /// and write stamps are read now, never saved in a work item.
    #[allow(clippy::too_many_arguments)]
    fn merge_below(
        &self,
        ns: i64,
        serial: i64,
        upper: &Layer,
        cell: i64,
        old: &Stored,
        from: usize,
        end: usize,
    ) -> OverlayResult<u64> {
        let mut window = match self.covering(ns, serial, upper.gen, cell)? {
            None => Window::empty(&self.payload_work),
            Some((shape, current)) => {
                if self.stale(ns, serial, upper, shape.offset, shape.epoch)? {
                    // The upsert replaces a row of one cell, not a wider one.
                    if shape.wide() {
                        self.drop_row(shape.row)?;
                    }
                    Window::empty(&self.payload_work)
                } else if let Some(current) = current {
                    current.expand(&self.payload_work)?
                } else {
                    // A live wide row decides every byte of its cells.
                    return Ok(0);
                }
            }
        };
        let mut copied = 0_u64;
        for at in 0..end {
            if old.valid(from + at) && window.mask[at / 8] & (1 << (at % 8)) == 0 {
                window.data[at] = old.data[from + at];
                copied += 1;
                window.set(at, at + 1);
            }
        }
        let mut observed = self.payload_work.get();
        observed.cell_copy_bytes = observed.cell_copy_bytes.saturating_add(copied);
        self.payload_work.set(observed);
        self.store(
            ns,
            serial,
            upper.gen,
            cell,
            upper.epoch,
            window.trim(&self.payload_work),
        )?;
        Ok(end as u64)
    }
}
