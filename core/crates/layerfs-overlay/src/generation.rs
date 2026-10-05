//! Fixed captured membership and publication/reply-attempt ordering primitives.
use crate::{
    db::integer, inode, sql, Capture, Generation, Inode, Overlay, OverlayError, OverlayResult,
    Publication, Route, StatementKind,
};

impl Overlay {
    /// Records a send attempt, including a lost reply. It never removes published
    /// inode/name/byte state and never claims kernel delivery.
    pub fn reply_attempted(&self, publication: Publication) -> OverlayResult<()> {
        self.atomic(|| {
            self.live(publication.route)?;
            let changed = self.execute(
                StatementKind::Frontier,
                "DELETE FROM request WHERE ns=?1 AND revision=?2",
                &[&publication.route.ns, &publication.revision],
                16,
            )?;
            if changed != 1 {
                return Err(OverlayError::Stale);
            }
            Ok(())
        })
    }
    /// Seals existing rows without copying them. The daemon parks capture until
    /// earlier reply attempts settle; this method does not block a SQL owner.
    pub fn capture(&self, route: Route) -> OverlayResult<Capture> {
        self.atomic(|| {
            let state=self.live(route)?;
            if state.captured.is_some() {return Err(OverlayError::CaptureInFlight);}
            if !self.query(StatementKind::Frontier,
                "SELECT revision FROM request WHERE ns=?1 ORDER BY revision LIMIT 1",
                &[&route.ns],8,|r|r.get::<_,i64>(0))?.is_empty() {
                return Err(OverlayError::ReplyAttemptsPending);
            }
            let next=state.active.0.checked_add(1).ok_or(OverlayError::Invalid("generation exhausted"))?;
            self.execute(StatementKind::Capture,
                "UPDATE workspace SET captured=active,active=?2,dirty_inodes=0,dirty_names=0 WHERE ns=?1",
                &[&route.ns,&next],16)?;
            Ok(Capture {route,generation:state.active,revision:state.revision,base_root:state.base_root})
        })
    }
    /// Captured inode page with a fixed indexed domain and bounded reply. Later
    /// active inserts cannot change its membership or extend EOF.
    pub fn captured_inodes(&self, capture: Capture, after: u64) -> OverlayResult<Vec<Inode>> {
        self.checked_capture(capture)?;
        self.query(
            StatementKind::Capture,
            sql::INODE_CAPTURE,
            &[&capture.route.ns, &capture.generation.0, &integer(after)?],
            24,
            inode::decode,
        )
    }
    pub(crate) fn checked_capture(&self, capture: Capture) -> OverlayResult<()> {
        let state = self.live(capture.route)?;
        if state.captured != Some(capture.generation) {
            return Err(OverlayError::Stale);
        }
        Ok(())
    }
    /// Observes the exact query strategy for a fixed captured domain. Runtime
    /// statement counters are available separately through diagnostics().
    pub fn explain_capture(&self, capture: Capture) -> OverlayResult<Vec<String>> {
        self.checked_capture(capture)?;
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::INODE_CAPTURE),
            &[&capture.route.ns, &capture.generation.0, &0_i64],
            24,
            |r| r.get(3),
        )
    }
    /// Observes the ordinary point lookup strategy under this route's live view.
    pub fn explain_inode(&self, route: Route, serial: u64) -> OverlayResult<Vec<String>> {
        let state = self.live(route)?;
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::INODE_LOOKUP),
            &[&route.ns, &integer(serial)?, &state.active.0],
            24,
            |r| r.get(3),
        )
    }
    /// Returns the active generation for a caller's bounded cell/read job.
    pub fn active_generation(&self, route: Route) -> OverlayResult<Generation> {
        Ok(self.live(route)?.active)
    }
}
