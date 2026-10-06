//! Raw single-cell access: namespace-qualified rows of an exact generation.
use crate::{
    db::integer, layers::Layer, Capture, Cell, Generation, Overlay, OverlayError, OverlayResult,
    Route, CELL_BYTES,
};

pub(crate) fn check(cell: &Cell) -> OverlayResult<()> {
    integer(cell.offset)?;
    if cell.offset % CELL_BYTES as u64 != 0 {
        return Err(OverlayError::Invalid("cell alignment"));
    }
    Ok(())
}
impl Overlay {
    /// Replaces one whole cell of a layer inside the caller's transaction,
    /// stamped with that layer's epoch. A cell with no valid byte leaves no row.
    pub(crate) fn put_cell(
        &self,
        route: Route,
        serial: i64,
        layer: &Layer,
        cell: &Cell,
    ) -> OverlayResult<()> {
        let mut observed = self.payload_work.get();
        observed.cell_copy_bytes = observed
            .cell_copy_bytes
            .saturating_add((CELL_BYTES + crate::MASK_BYTES) as u64);
        self.payload_work.set(observed);
        let window = crate::cells::Window {
            data: cell.data.clone(),
            mask: cell.validity.clone(),
        };
        self.store(
            route.ns,
            serial,
            layer.gen,
            integer(cell.offset)?,
            layer.epoch,
            window.trim(&self.payload_work),
        )
    }
    /// Reads exactly one stored physical cell of a live generation, expanded to
    /// its full window. This is the raw row: layer composition, staleness after
    /// a shrink and base fall-through belong to `source_read`.
    pub fn cell(
        &self,
        route: Route,
        serial: u64,
        generation: Generation,
        offset: u64,
    ) -> OverlayResult<Option<Cell>> {
        let state = self.live(route)?;
        if generation != state.active && Some(generation) != state.captured {
            return Err(OverlayError::Stale);
        }
        self.cell_at(route, serial, generation, offset)
    }
    /// The raw stored cell of a retained capture, including while terminal
    /// close awaits fenced construction/history disposition. Install/release
    /// invalidates the capability; observing bytes never changes its ownership.
    pub fn captured_cell(
        &self,
        capture: Capture,
        serial: u64,
        offset: u64,
    ) -> OverlayResult<Option<Cell>> {
        self.checked_capture(capture)?;
        self.cell_at(capture.route(), serial, capture.generation, offset)
    }
    /// One cell written at an exact live generation of an owned source, such as
    /// the creation generation of a local symlink target.
    pub fn source_cell(
        &self,
        source: crate::BaseSource,
        serial: u64,
        generation: u64,
        offset: u64,
    ) -> OverlayResult<Option<Cell>> {
        let state = self.source_state(source)?;
        let generation = integer(generation)?;
        if generation <= state.installed || generation > state.active.0 {
            return Err(OverlayError::Stale);
        }
        self.cell_at(source.route(), serial, Generation(generation), offset)
    }
    fn cell_at(
        &self,
        route: Route,
        serial: u64,
        generation: Generation,
        offset: u64,
    ) -> OverlayResult<Option<Cell>> {
        let Some(stored) =
            self.stored(route.ns, integer(serial)?, generation.0, integer(offset)?)?
        else {
            return Ok(None);
        };
        let window = stored.expand(&self.payload_work)?;
        Ok(Some(Cell {
            offset,
            data: window.data,
            validity: window.mask,
        }))
    }
}
