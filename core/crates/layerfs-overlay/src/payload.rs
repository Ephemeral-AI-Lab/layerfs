//! Binary bounded cells, with namespace-qualified access and frozen generations.
use crate::{
    db::integer, sql, Capture, Cell, Generation, Overlay, OverlayError, OverlayResult, Route,
    StatementKind, CELL_BYTES, MASK_BYTES,
};

pub(crate) fn check(cell: &Cell) -> OverlayResult<()> {
    integer(cell.offset)?;
    if cell.offset % CELL_BYTES as u64 != 0 {
        return Err(OverlayError::Invalid("cell alignment"));
    }
    Ok(())
}
impl Overlay {
    pub(crate) fn put_cell(
        &self,
        route: Route,
        serial: i64,
        gen: Generation,
        cell: &Cell,
    ) -> OverlayResult<()> {
        self.execute(StatementKind::Payload,
            "INSERT INTO payload(ns,serial,gen,cell_offset,data,validity) VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(ns,serial,gen,cell_offset) DO UPDATE SET data=excluded.data,validity=excluded.validity",
            &[&route.ns,&serial,&gen.0,&integer(cell.offset)?,&cell.data.as_slice(),&cell.validity.as_slice()],
            (32+CELL_BYTES+MASK_BYTES) as u64)?;
        Ok(())
    }
    /// Reads exactly one retained physical cell. No raw rowid crosses this boundary.
    /// Composition of invalid bytes with immutable base belongs to Workspace.
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
    /// Exact immutable capture bytes, including while terminal close awaits
    /// fenced construction/history disposition. Install/release invalidates the
    /// capability; observing bytes never changes its ownership.
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
    /// the creation generation of a local symlink target. Byte composition over
    /// several generations belongs to the payload read contract.
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
        let serial = integer(serial)?;
        let key = integer(offset)?;
        Ok(self
            .query(
                StatementKind::Payload,
                sql::CELL_LOOKUP,
                &[&route.ns, &serial, &generation.0, &key],
                32,
                |r| {
                    let data: Vec<u8> = r.get(0)?;
                    let validity: Vec<u8> = r.get(1)?;
                    Ok(Cell {
                        offset,
                        data: data
                            .into_boxed_slice()
                            .try_into()
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        validity: validity
                            .into_boxed_slice()
                            .try_into()
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    })
                },
            )?
            .pop())
    }
}
