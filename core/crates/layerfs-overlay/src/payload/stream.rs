//! Bounded byte writes and the local layered read plan of one window.
use crate::{
    cells::{Stored, Window},
    db::integer,
    layers::Layer,
    sql, BaseSource, Capture, InodeKind, LocalRead, Overlay, OverlayError, OverlayResult,
    StatementKind, CELL_BYTES, READ_WINDOW, RUN_BYTES,
};
use rusqlite::blob::Blob;

const CELL: u64 = CELL_BYTES as u64;
const RUN: u64 = RUN_BYTES as u64;

impl Overlay {
    /// Writes `bytes` at `offset` into one layer inside the caller's
    /// transaction. Whole cells are written one aligned slot of `RUN_BYTES`
    /// at a time: one read of the row shapes of that slot, then the bytes
    /// replace a live dense row in place or become one new row bound from
    /// the caller's slice. A partly covered cell is one point read and
    /// either an in-place write into a wide row or a merge of at most one
    /// cell and one upsert. Prior fragmentation of the range does not
    /// enlarge the work.
    pub(crate) fn write_cells(
        &self,
        ns: i64,
        serial: i64,
        layer: &Layer,
        offset: u64,
        bytes: &[u8],
    ) -> OverlayResult<()> {
        let end = offset
            .checked_add(bytes.len() as u64)
            .filter(|end| integer(*end).is_ok())
            .ok_or(OverlayError::Invalid("SQLite signed identity/offset"))?;
        let mut observed = self.payload_work.get();
        observed.write_input_bytes = observed
            .write_input_bytes
            .saturating_add(bytes.len() as u64);
        self.payload_work.set(observed);
        // One handle serves every in-place write of this window. A commit
        // needs it closed, so it never outlives this call.
        let mut blob = None;
        let mut at = offset;
        while at < end {
            let cell = at - at % CELL;
            let whole = at == cell && end - at >= CELL;
            let to = if whole {
                let stop = end.min(cell - cell % RUN + RUN);
                stop - stop % CELL
            } else {
                end.min(cell + CELL)
            };
            let source = &bytes[(at - offset) as usize..(to - offset) as usize];
            let mut observed = self.payload_work.get();
            observed.write_cells = observed
                .write_cells
                .saturating_add((to - cell).div_ceil(CELL));
            observed.partial_write_cells = observed
                .partial_write_cells
                .saturating_add(u64::from(!whole));
            self.payload_work.set(observed);
            if whole {
                self.write_whole(ns, serial, layer, at as i64, source, &mut blob)?;
            } else {
                let low = (at - cell) as usize;
                self.write_part(ns, serial, layer, cell as i64, low, source, &mut blob)?;
            }
            at = to;
        }
        match blob {
            Some(open) => open.close().map_err(|cause| self.failed(cause.into())),
            None => Ok(()),
        }
    }
    /// Whole cells `[from, from + source.len())` inside one slot.
    fn write_whole<'a>(
        &'a self,
        ns: i64,
        serial: i64,
        layer: &Layer,
        from: i64,
        source: &[u8],
        blob: &mut Option<Blob<'a>>,
    ) -> OverlayResult<()> {
        let to = from + source.len() as i64;
        let part = |low: i64, high: i64| &source[(low - from) as usize..(high - from) as usize];
        let (mut low, mut high) = (from, to);
        // Rows that start inside the range, and the one that is exactly it.
        let (mut inside, mut same) = (0, None);
        for shape in self.shapes(ns, serial, layer.gen, from, to)? {
            let live = !self.stale(ns, serial, layer, shape.offset, shape.epoch)?;
            if shape.offset < from {
                // Only a wide row reaches a later cell. A stale one is
                // garbage in the way; a live one takes its part in place.
                if live {
                    low = shape.end().min(to);
                    self.write_in_place(blob, shape.row, from - shape.offset, part(from, low))?;
                } else {
                    self.drop_row(shape.row)?;
                }
            } else if live && shape.wide() && shape.end() > to {
                high = shape.offset;
                self.write_in_place(blob, shape.row, 0, part(high, to))?;
            } else {
                inside += 1;
                same = (live && !shape.masked).then_some(shape);
            }
        }
        if low >= high {
            return Ok(());
        }
        if let Some(shape) =
            same.filter(|shape| inside == 1 && shape.offset == low && shape.length == high - low)
        {
            return self.write_in_place(blob, shape.row, 0, part(low, high));
        }
        if inside != 0 {
            self.execute(
                StatementKind::Payload,
                sql::CELLS_DROP,
                &[&ns, &serial, &layer.gen, &low, &high],
                40,
            )?;
        }
        self.execute(
            StatementKind::Payload,
            sql::CELL_PUT,
            &[
                &ns,
                &serial,
                &layer.gen,
                &low,
                &layer.epoch,
                &part(low, high),
                &None::<Vec<u8>>,
            ],
            40 + (high - low) as u64,
        )?;
        Ok(())
    }
    /// Bytes `[low, low + source.len())` of one partly covered cell.
    #[allow(clippy::too_many_arguments)]
    fn write_part<'a>(
        &'a self,
        ns: i64,
        serial: i64,
        layer: &Layer,
        cell: i64,
        low: usize,
        source: &[u8],
        blob: &mut Option<Blob<'a>>,
    ) -> OverlayResult<()> {
        let mut old = None;
        if let Some((shape, stored)) = self.covering(ns, serial, layer.gen, cell)? {
            let live = !self.stale(ns, serial, layer, shape.offset, shape.epoch)?;
            if shape.wide() {
                if live {
                    let at = cell - shape.offset + low as i64;
                    return self.write_in_place(blob, shape.row, at, source);
                }
                // The upsert replaces a row of one cell, not a wider one.
                self.drop_row(shape.row)?;
            } else if live {
                old = stored;
            }
        }
        let mut window = match old {
            Some(old) => old.expand(&self.payload_work)?,
            None => Window::empty(&self.payload_work),
        };
        let high = low + source.len();
        window.data[low..high].copy_from_slice(source);
        window.set(low, high);
        let mut observed = self.payload_work.get();
        observed.cell_copy_bytes = observed.cell_copy_bytes.saturating_add(source.len() as u64);
        self.payload_work.set(observed);
        self.store(
            ns,
            serial,
            layer.gen,
            cell,
            layer.epoch,
            window.trim(&self.payload_work),
        )
    }
    /// Local layers of one window over an owned source, composed in this one
    /// owner job. None means no local row: the file is entirely inherited.
    pub fn source_read(
        &self,
        source: BaseSource,
        serial: u64,
        offset: u64,
        length: u32,
    ) -> OverlayResult<Option<LocalRead>> {
        let state = self.source_state(source)?;
        let key = integer(serial)?;
        if let Some(orphan) = self.orphan(source.route.ns, key)? {
            if self.orphan_layer(source.route.ns, key)?.kind == InodeKind::File {
                return Err(OverlayError::Missing);
            }
            return self.orphan_read(source.route.ns, key, orphan, offset, length);
        }
        self.read_layers(
            source.route.ns,
            serial,
            state.active.0,
            state.installed,
            offset,
            length,
        )
    }
    /// The same composition over the sealed view of a capture, immutable while
    /// the capture is retained.
    pub fn captured_read(
        &self,
        capture: Capture,
        serial: u64,
        offset: u64,
        length: u32,
    ) -> OverlayResult<Option<LocalRead>> {
        self.checked_capture(capture)?;
        let installed = self.state(capture.route())?.installed;
        self.read_layers(
            capture.route().ns,
            serial,
            capture.generation.0,
            installed,
            offset,
            length,
        )
    }
    pub(crate) fn read_layers(
        &self,
        ns: i64,
        serial: u64,
        top: i64,
        installed: i64,
        offset: u64,
        length: u32,
    ) -> OverlayResult<Option<LocalRead>> {
        if length as usize > READ_WINDOW {
            return Err(OverlayError::Invalid("read window"));
        }
        let serial = integer(serial)?;
        integer(offset)?;
        let layers = self.layers(ns, serial, top, installed)?;
        if layers
            .first()
            .is_some_and(|layer| layer.kind != InodeKind::Directory && layer.nlink == 0)
        {
            return Err(OverlayError::Missing);
        }
        self.compose_layers(ns, serial, offset, length, layers)
    }
    pub(crate) fn compose_layers(
        &self,
        ns: i64,
        serial: i64,
        offset: u64,
        length: u32,
        layers: Vec<Layer>,
    ) -> OverlayResult<Option<LocalRead>> {
        if length as usize > READ_WINDOW {
            return Err(OverlayError::Invalid("read window"));
        }
        integer(offset)?;
        let Some(first) = layers.first() else {
            return Ok(None);
        };
        let start = offset.min(first.size);
        let end = offset.saturating_add(u64::from(length)).min(first.size);
        let mut read = LocalRead {
            kind: first.kind,
            size: first.size,
            offset: start,
            data: vec![0; (end - start) as usize],
            inherited: Vec::new(),
            span: None,
            base_root: None,
        };
        if first.kind == InodeKind::Directory || start == end {
            return Ok(Some(read));
        }
        // 0: undecided, 1: decided locally (a written byte or a zero).
        let mut decided = vec![0_u8; read.data.len()];
        let mut observed = self.payload_work.get();
        observed.read_window_zeroed_bytes = observed
            .read_window_zeroed_bytes
            .saturating_add((read.data.len() + decided.len()) as u64);
        self.payload_work.set(observed);
        let mut copied = 0_u64;
        let mut open = decided.len();
        let settle = |decided: &mut [u8], open: &mut usize, from: u64, to: u64| {
            for slot in &mut decided
                [(from.clamp(start, end) - start) as usize..(to.clamp(start, end) - start) as usize]
            {
                *open -= usize::from(*slot == 0);
                *slot = 1;
            }
        };
        for (depth, layer) in layers.iter().enumerate() {
            if depth != 0 {
                // A lower view ends at its own EOF; beyond it the byte is zero.
                settle(&mut decided, &mut open, layer.size, end);
            }
            if open == 0 {
                break;
            }
            let rows = self.query(
                StatementKind::Payload,
                sql::CELL_RANGE,
                &[
                    &ns,
                    &serial,
                    &layer.gen,
                    &((start - start % RUN) as i64),
                    &(end as i64),
                    &(start as i64),
                ],
                48,
                |row| Ok((row.get::<_, i64>(0)?, Stored::decode(row, 1)?)),
            )?;
            for (cell, stored) in rows {
                if self.stale(ns, serial, layer, cell, stored.epoch)? {
                    continue;
                }
                let cell = cell as u64;
                let from = start.max(cell);
                let to = end.min(cell + stored.data.len() as u64);
                for at in from..to {
                    let (slot, index) = ((at - start) as usize, (at - cell) as usize);
                    if decided[slot] == 0 && stored.valid(index) {
                        read.data[slot] = stored.data[index];
                        copied += 1;
                        decided[slot] = 1;
                        open -= 1;
                    }
                }
            }
            // Nothing at or above the cutoff falls through to the lower view.
            settle(&mut decided, &mut open, layer.cutoff, end);
            if open == 0 {
                break;
            }
        }
        if open != 0 {
            read.inherited = vec![0; decided.len().div_ceil(8)];
            let (mut low, mut high) = (usize::MAX, 0);
            for (slot, state) in decided.iter().enumerate() {
                if *state == 0 {
                    read.inherited[slot / 8] |= 1 << (slot % 8);
                    low = low.min(slot);
                    high = slot + 1;
                }
            }
            read.span = Some((start + low as u64, start + high as u64));
        }
        let mut observed = self.payload_work.get();
        observed.read_local_copy_bytes = observed.read_local_copy_bytes.saturating_add(copied);
        observed.read_window_zeroed_bytes = observed
            .read_window_zeroed_bytes
            .saturating_add(read.inherited.len() as u64);
        self.payload_work.set(observed);
        Ok(Some(read))
    }
}
impl Overlay {
    /// Plans of the payload statements a write, shrink and layered read use:
    /// layer rows, one cell, the cells of a window and one staircase step.
    pub fn explain_payload(&self, source: BaseSource) -> OverlayResult<Vec<String>> {
        let state = self.source_state(source)?;
        let ns = source.route.ns;
        let (one, gen, floor) = (1_i64, state.active.0, state.installed);
        let mut plans = Vec::new();
        for (label, statement, params) in [
            (
                "layers",
                sql::LAYERS,
                vec![&ns as &dyn rusqlite::ToSql, &one, &gen, &floor],
            ),
            ("active-layer", sql::LAYER_ACTIVE, vec![&ns, &one, &gen]),
            (
                "lower-layer",
                sql::LAYER_LOWER,
                vec![&ns, &one, &gen, &floor],
            ),
            ("cell", sql::CELL_COVER, vec![&ns, &one, &gen, &one, &one]),
            (
                "shapes",
                sql::CELL_SHAPES,
                vec![&ns, &one, &gen, &one, &one],
            ),
            (
                "cell-window",
                sql::CELL_RANGE,
                vec![&ns, &one, &gen, &one, &one, &one],
            ),
            ("drop-cell", sql::CELL_DROP, vec![&ns, &one, &gen, &one]),
            (
                "drop-cells",
                sql::CELLS_DROP,
                vec![&ns, &one, &gen, &one, &one],
            ),
            ("drop-row", sql::ROW_DROP, vec![&one]),
            ("cut-row", sql::ROW_KEEP, vec![&one, &one]),
            ("move-row", sql::ROW_MOVE, vec![&one, &one, &one]),
            ("row-cell", sql::ROW_SLICE, vec![&one, &one, &one]),
            ("step", sql::STEP_GET, vec![&ns, &one, &gen, &one]),
        ] {
            plans.extend(self.query(
                StatementKind::Explain,
                &format!("EXPLAIN QUERY PLAN {statement}"),
                &params,
                32,
                |row| Ok(format!("{label}: {}", row.get::<_, String>(3)?)),
            )?);
        }
        Ok(plans)
    }
}
