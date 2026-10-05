//! Bounded byte writes and the local layered read plan of one window.
use crate::{
    cells::{Stored, Window},
    db::integer,
    layers::Layer,
    sql, BaseSource, Capture, InodeKind, LocalRead, Overlay, OverlayError, OverlayResult,
    StatementKind, CELL_BYTES, READ_WINDOW,
};

const CELL: u64 = CELL_BYTES as u64;

impl Overlay {
    /// Writes `bytes` at `offset` into one layer inside the caller's
    /// transaction. Each intersecting cell is handled alone: a fully covered
    /// cell is one upsert with no read, a partly covered one is one point read,
    /// an in-memory merge of at most one cell, and one upsert. Prior
    /// fragmentation of the range does not change the work.
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
        let mut cell = offset - offset % CELL;
        while cell < end {
            let from = offset.max(cell);
            let to = end.min(cell + CELL);
            let source = &bytes[(from - offset) as usize..(to - offset) as usize];
            let (low, high) = ((from - cell) as usize, (to - cell) as usize);
            let key = cell as i64;
            let stored = if low == 0 && high == CELL_BYTES {
                Some((source.to_vec(), None))
            } else {
                let mut window = match self.stored(ns, serial, layer.gen, key)? {
                    Some(old) if !self.stale(ns, serial, layer, key, old.epoch)? => old.expand()?,
                    _ => Window::empty(),
                };
                window.data[low..high].copy_from_slice(source);
                window.set(low, high);
                window.trim()
            };
            self.store(ns, serial, layer.gen, key, layer.epoch, stored)?;
            cell += CELL;
        }
        Ok(())
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
                    &((start - start % CELL) as i64),
                    &(end as i64),
                ],
                40,
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
            ("cell", sql::CELL_LOOKUP, vec![&ns, &one, &gen, &one]),
            (
                "cell-window",
                sql::CELL_RANGE,
                vec![&ns, &one, &gen, &one, &one],
            ),
            ("drop-cell", sql::CELL_DROP, vec![&ns, &one, &gen, &one]),
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
