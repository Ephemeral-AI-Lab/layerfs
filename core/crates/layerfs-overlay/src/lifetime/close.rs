//! Logical close and exact last-owner eligibility, without namespace sweeps.
use crate::{Capture, Overlay, OverlayError, OverlayResult, Route, StatementKind};
pub(crate) const CLOSE_KEY: i64 = i64::MAX;

/// Bounded close/cleanup observation for a minted route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupState {
    Live,
    Held,
    Queued,
    Gone,
}
impl Overlay {
    /// Revokes new mutations/acquisitions and queues only eligible terminal work.
    /// Existing reply attempts/releases and exact captures retain their custody.
    pub fn close(&self, route: Route) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            self.live(route)?;
            self.execute(
                StatementKind::Workspace,
                "UPDATE workspace SET lifecycle=1 WHERE ns=?1",
                &[&route.ns],
                8,
            )?;
            self.queue_closed(route)
        })
    }
    /// Releases an exact closed capture after its caller has fenced/resolved all
    /// construction/history work. Unknown disposition must keep the capture.
    /// This local release does not discard a history stage or infer publication.
    pub fn release_closed_capture(&self, capture: Capture) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            let state = self.state(capture.route())?;
            self.checked_capture(capture)?;
            if !state.closed {
                return Err(OverlayError::Stale);
            }
            self.execute(
                StatementKind::Capture,
                "UPDATE workspace SET captured=NULL,captured_revision=NULL WHERE ns=?1",
                &[&capture.route().ns],
                8,
            )?;
            self.wake_orphan_sources(capture.route.ns, capture.generation.0)?;
            self.queue_closed(capture.route())
        })
    }
    /// One maintained point observation; no owner/payload COUNT or scan.
    pub fn cleanup_state(&self, route: Route) -> OverlayResult<CleanupState> {
        self.check_route(route)?;
        let rows=self.query(StatementKind::Workspace,
            "SELECT incarnation,lifecycle,EXISTS(SELECT 1 FROM reclaim WHERE ns=?1 AND queue_key=?2) FROM workspace WHERE ns=?1",
            &[&route.ns,&CLOSE_KEY],16,|row|Ok((row.get::<_,Vec<u8>>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?)))?;
        let Some((incarnation, closed, queued)) = rows.first() else {
            return Ok(CleanupState::Gone);
        };
        if incarnation.as_slice() != route.incarnation {
            return Err(OverlayError::Stale);
        }
        Ok(if *closed == 0 {
            CleanupState::Live
        } else if *queued != 0 {
            CleanupState::Queued
        } else {
            CleanupState::Held
        })
    }
    pub(crate) fn queue_closed(&self, route: Route) -> OverlayResult<()> {
        let state = self.state(route)?;
        if !state.closed || state.captured.is_some() || state.base_readers != 0 {
            return Ok(());
        }
        let held=self.query(StatementKind::Reclaim,
            "SELECT EXISTS(SELECT 1 FROM lease WHERE ns=?1) OR EXISTS(SELECT 1 FROM request WHERE ns=?1) OR EXISTS(SELECT 1 FROM native_mount WHERE ns=?1)",
            &[&route.ns],8,|row|row.get::<_,i64>(0))?[0]!=0;
        if !held {
            self.execute(StatementKind::Reclaim,
                "INSERT INTO reclaim(ns,queue_key,target,cursor) VALUES(?1,?2,?2,0) ON CONFLICT(ns,queue_key) DO NOTHING",
                &[&route.ns,&CLOSE_KEY],16)?;
            self.closed_ready.set(true);
        }
        Ok(())
    }
}
