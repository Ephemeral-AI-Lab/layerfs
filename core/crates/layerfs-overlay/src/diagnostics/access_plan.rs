//! Operational access plans for the actual bounded namespaced engine queries.
use crate::{
    db::integer, sql, Generation, Lease, Overlay, OverlayError, OverlayResult, Route, StatementKind,
};
impl Overlay {
    /// Exact retained owner observation, including during logical close.
    pub fn lease_exists(&self, route: Route, lease: Lease) -> OverlayResult<bool> {
        self.state(route)?;
        Ok(!self
            .query(
                StatementKind::Lease,
                sql::LEASE_LOOKUP,
                &[
                    &route.ns,
                    &(lease.kind as i64),
                    &integer(lease.owner)?,
                    &integer(lease.resource)?,
                ],
                32,
                |row| row.get::<_, i64>(0),
            )?
            .is_empty())
    }
    /// Plan for the actual exact cell lookup, without loading its data/mask.
    pub fn explain_cell(
        &self,
        route: Route,
        serial: u64,
        generation: Generation,
        offset: u64,
    ) -> OverlayResult<Vec<String>> {
        let state = self.live(route)?;
        if generation != state.active && Some(generation) != state.captured {
            return Err(OverlayError::Stale);
        }
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::CELL_COVER),
            &[
                &route.ns,
                &integer(serial)?,
                &generation.0,
                &crate::payload::runs::slot(integer(offset)?),
                &integer(offset)?,
            ],
            40,
            |row| row.get(3),
        )
    }
    /// Plan for the actual active name seek, including installed-floor exclusion.
    pub fn explain_directory_entry(
        &self,
        route: Route,
        parent: u64,
        name: &[u8],
    ) -> OverlayResult<Vec<String>> {
        let state = self.live(route)?;
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::DIRECTORY_ENTRY_LOOKUP),
            &[
                &route.ns,
                &integer(parent)?,
                &name,
                &state.active.0,
                &state.installed,
            ],
            32 + name.len() as u64,
            |row| row.get(3),
        )
    }
    /// Plan for the exact retained owner key; raw owner/resource numbers alone
    /// do not authorize another namespace.
    pub fn explain_lease(&self, route: Route, lease: Lease) -> OverlayResult<Vec<String>> {
        self.state(route)?;
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::LEASE_LOOKUP),
            &[
                &route.ns,
                &(lease.kind as i64),
                &integer(lease.owner)?,
                &integer(lease.resource)?,
            ],
            32,
            |row| row.get(3),
        )
    }
    /// Plan for the real operation-owned operation record keyset window.
    pub fn explain_operation_record(
        &self,
        route: Route,
        operation: u64,
        kind: u32,
        after: Option<u64>,
    ) -> OverlayResult<Vec<String>> {
        self.operation(route, operation)?;
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::OPERATION_RECORD_PAGE),
            &[
                &route.ns,
                &integer(operation)?,
                &i64::from(kind),
                &after.map(integer).transpose()?.unwrap_or(-1),
            ],
            32,
            |row| row.get(3),
        )
    }
}
