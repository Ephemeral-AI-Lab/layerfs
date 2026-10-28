//! Bounded byte writes and the local layered read plan of one window.
use crate::{
    cells::{Stored, Window},
    db::integer,
    layers::Layer,
    payload::runs::{slot, Shape},
    sql, BaseSource, Capture, InodeKind, LocalRead, Overlay, OverlayError, OverlayResult,
    StatementKind, CELL_BYTES, READ_WINDOW, RUN_BYTES,
};
use rusqlite::blob::Blob;

const CELL: u64 = CELL_BYTES as u64;
const RUN: u64 = RUN_BYTES as u64;

impl Overlay {
    /// Writes `bytes` at `offset` into one layer inside the caller's
    /// transaction. Whole cells are written one aligned slot of `RUN_BYTES`
    /// at a time: the row shapes of the slot, read once for every
    /// `READ_WINDOW` of slots ahead, then the bytes
    /// replace a live dense row in place or become one new row bound from
    /// the caller's slice. A partly covered cell is one point read and
    /// either an in-place write into a wide row or a merge of at most one
    /// cell and one upsert. Prior fragmentation of the range does not
    /// enlarge the work.
    pub(crate) fn write_cells(
        &self,
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
        // Row shapes of the slots ahead, read once for at most one read
        // window of them, and where that read ends. A slot's write changes
        // rows of its own slot only, and a partly covered first cell is
        // written before the first whole one is reached, so the shapes are
        // the rows each slot's own read would return.
        let (mut ahead, mut ahead_to) = (Vec::new(), 0);
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
                if at >= ahead_to {
                    let window = at - at % RUN + READ_WINDOW as u64;
                    ahead_to = (end - end % CELL).min(window);
                    ahead = self.shapes(serial, layer.gen, at as i64, ahead_to as i64)?;
                }
                self.write_whole(serial, layer, at as i64, source, &ahead, &mut blob)?;
            } else {
                let low = (at - cell) as usize;
                self.write_part(serial, layer, cell as i64, low, source, &mut blob)?;
            }
            at = to;
        }
        match blob {
            Some(open) => open.close().map_err(|cause| self.failed(cause.into())),
            None => Ok(()),
        }
    }
    /// Whole cells `[from, from + source.len())` inside one slot. `ahead`
    /// holds the shapes of the layer's rows from that slot on, as they are
    /// stored now.
    #[allow(clippy::too_many_arguments)]
    fn write_whole<'a>(
        &'a self,
        serial: i64,
        layer: &Layer,
        from: i64,
        source: &[u8],
        ahead: &[Shape],
        blob: &mut Option<Blob<'a>>,
    ) -> OverlayResult<()> {
        let to = from + source.len() as i64;
        let part = |low: i64, high: i64| &source[(low - from) as usize..(high - from) as usize];
        let (mut low, mut high) = (from, to);
        // Rows that start inside the range, and the one that is exactly it.
        let (mut inside, mut same) = (0, None);
        // The rows that occupy a cell of the range: they start in its slot.
        let start = slot(from);
        let occupying =
            |shape: &&Shape| shape.offset >= start && shape.offset < to && shape.end() > from;
        for shape in ahead.iter().filter(occupying).copied() {
            let live = !self.stale(serial, layer, shape.offset, shape.epoch)?;
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
                &[&serial, &layer.gen, &low, &high],
                40,
            )?;
        }
        self.execute(
            StatementKind::Payload,
            sql::CELL_PUT,
            &[
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
        serial: i64,
        layer: &Layer,
        cell: i64,
        low: usize,
        source: &[u8],
        blob: &mut Option<Blob<'a>>,
    ) -> OverlayResult<()> {
        let mut old = None;
        if let Some((shape, stored)) = self.covering(serial, layer.gen, cell)? {
            let live = !self.stale(serial, layer, shape.offset, shape.epoch)?;
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
        if let Some(orphan) = self.orphan(key)? {
            if self.orphan_layer(key)?.kind == InodeKind::File {
                return Err(OverlayError::Missing);
            }
            return self.orphan_read(key, orphan, offset, length);
        }
        self.read_layers(serial, state.active.0, state.installed, offset, length)
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
        let installed = self.state()?.installed;
        self.read_layers(serial, capture.generation.0, installed, offset, length)
    }
    pub(crate) fn read_layers(
        &self,
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
        let layers = self.layers(serial, top, installed)?;
        if layers
            .first()
            .is_some_and(|layer| layer.kind != InodeKind::Directory && layer.nlink == 0)
        {
            return Err(OverlayError::Missing);
        }
        self.compose_layers(serial, offset, length, layers)
    }
    pub(crate) fn compose_layers(
        &self,
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
            references: Vec::new(),
            referenced: Vec::new(),
        };
        if first.kind == InodeKind::Directory || start == end {
            return Ok(Some(read));
        }
        // 0: undecided, 1: decided locally (a written byte or a zero),
        // n + 2: supplied by reference range n.
        let mut decided = vec![0_u8; read.data.len()];
        let mut observed = self.payload_work.get();
        observed.read_window_zeroed_bytes = observed
            .read_window_zeroed_bytes
            .saturating_add((read.data.len() + decided.len()) as u64);
        self.payload_work.set(observed);
        let mut copied = 0_u64;
        let mut open = decided.len();
        let settle = |decided: &mut [u8], open: &mut usize, from: u64, to: u64| {
            let range = &mut decided[(from.clamp(start, end) - start) as usize
                ..(to.clamp(start, end) - start) as usize];
            let free = range.iter().filter(|slot| **slot == 0).count();
            *open -= free;
            if free == range.len() {
                range.fill(1);
            } else {
                // A byte a reference range supplies stays that range's.
                for slot in range.iter_mut().filter(|slot| **slot == 0) {
                    *slot = 1;
                }
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
                if self.stale(serial, layer, cell, stored.epoch)? {
                    continue;
                }
                let cell = cell as u64;
                let from = start.max(cell);
                let to = end.min(cell + stored.data.len() as u64);
                if from >= to {
                    continue;
                }
                let slots = (from - start) as usize..(to - start) as usize;
                // A dense row over bytes no higher layer decided is one copy.
                if stored.validity.is_none() && decided[slots.clone()].iter().all(|slot| *slot == 0)
                {
                    let source = &stored.data[(from - cell) as usize..(to - cell) as usize];
                    read.data[slots.clone()].copy_from_slice(source);
                    decided[slots.clone()].fill(1);
                    copied += slots.len() as u64;
                    open -= slots.len();
                    continue;
                }
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
            if layer.referenced && open != 0 {
                // Cells won inside this layer; its pieces decide what is left.
                let window = (start, end);
                open -= self.compose_pieces(serial, layer, window, &mut decided, &mut read)?;
            }
            // Nothing at or above the cutoff falls through to the lower view.
            settle(&mut decided, &mut open, layer.cutoff, end);
            if open == 0 {
                break;
            }
        }
        if open == decided.len() {
            // Nothing local in the window: every bit is set in one fill.
            read.inherited = vec![u8::MAX; decided.len().div_ceil(8)];
            if decided.len() % 8 != 0 {
                let tail = read.inherited.len() - 1;
                read.inherited[tail] = (1 << (decided.len() % 8)) - 1;
            }
            read.span = Some((start, end));
        } else if open != 0 {
            read.inherited = vec![0; decided.len().div_ceil(8)];
            for (bits, states) in read.inherited.iter_mut().zip(decided.chunks(8)) {
                for (bit, state) in states.iter().enumerate() {
                    *bits |= u8::from(*state == 0) << bit;
                }
            }
            let low = decided.iter().position(|state| *state == 0).unwrap_or(0);
            let high = decided
                .iter()
                .rposition(|state| *state == 0)
                .map_or(0, |slot| slot + 1);
            read.span = Some((start + low as u64, start + high as u64));
        }
        if !read.references.is_empty() {
            read.referenced = vec![0; decided.len().div_ceil(8)];
            for (bits, states) in read.referenced.iter_mut().zip(decided.chunks(8)) {
                for (bit, state) in states.iter().enumerate() {
                    *bits |= u8::from(*state > 1) << bit;
                }
            }
        }
        let mut observed = self.payload_work.get();
        observed.read_local_copy_bytes = observed.read_local_copy_bytes.saturating_add(copied);
        observed.read_window_zeroed_bytes = observed
            .read_window_zeroed_bytes
            .saturating_add((read.inherited.len() + read.referenced.len()) as u64);
        self.payload_work.set(observed);
        Ok(Some(read))
    }
}
impl Overlay {}
