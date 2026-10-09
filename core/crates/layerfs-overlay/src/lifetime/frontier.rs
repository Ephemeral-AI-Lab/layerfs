//! Bounded exact custody observations; observing never replays or releases work.
use crate::{
    db::integer, sql, Capture, Generation, Overlay, OverlayError, OverlayResult, Publication,
    Route, StatementKind,
};

impl Overlay {
    /// Observes the retained capture after a lost completion, including during
    /// logical close. Later active revisions cannot change this frozen identity.
    /// None means the routed Workspace exists without a capture; Stale means
    /// the routed incarnation is absent. No upstream history outcome is inferred.
    pub fn retained_capture(&self, route: Route) -> OverlayResult<Option<Capture>> {
        self.check_route(route)?;
        self.query(
            StatementKind::Capture,
            sql::RETAINED_CAPTURE,
            &[&route.ns, &route.incarnation.as_slice()],
            40,
            |row| {
                let Some(generation) = row.get::<_, Option<i64>>(0)? else {
                    return Ok(None);
                };
                let root: Vec<u8> = row.get(2)?;
                Ok(Some(Capture {
                    route,
                    generation: Generation(generation),
                    revision: row.get(1)?,
                    base_root: root.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?,
                }))
            },
        )?
        .pop()
        .ok_or(OverlayError::Stale)
    }
    /// Fixed keyset page of still-owned reply-send-attempt tickets, read from
    /// the engine's memory. A lost completion never removes these records. The caller must perform/fence the
    /// actual reply attempt before releasing a ticket; observation is no such fence.
    pub fn pending_publications(
        &self,
        route: Route,
        after: u64,
    ) -> OverlayResult<Vec<Publication>> {
        self.state(route)?;
        Ok(self.tickets.page(route, integer(after)?, 64))
    }
    /// Actual retained-capture point-query plan; paired runtime work is recorded
    /// in the Capture family, without loading changed membership or payload.
    pub fn explain_retained_capture(&self, route: Route) -> OverlayResult<Vec<String>> {
        self.state(route)?;
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::RETAINED_CAPTURE),
            &[&route.ns, &route.incarnation.as_slice()],
            40,
            |row| row.get(3),
        )
    }
}
