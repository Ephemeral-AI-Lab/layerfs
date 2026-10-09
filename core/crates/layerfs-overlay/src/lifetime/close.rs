//! Logical close and exact last-owner eligibility, without namespace sweeps.
use crate::{Capture, Overlay, OverlayError, OverlayResult, Route, StatementKind};
pub(crate) const CLOSE_KEY: i64 = i64::MAX;
/// Whether any owner still holds one namespace: each table that holds
/// custody is asked by the namespace prefix of its own key. A descriptor is
/// its `file_handle` or `native_directory` row and a lookup reference its
/// `lookup_owner` or `native_lookup` row; `lease` holds the owners that have
/// no row of their own.
pub(crate) const HELD: &str = "SELECT EXISTS(SELECT 1 FROM lease WHERE ns=?1)
    OR EXISTS(SELECT 1 FROM file_handle WHERE ns=?1)
    OR EXISTS(SELECT 1 FROM lookup_owner WHERE ns=?1)
    OR EXISTS(SELECT 1 FROM native_lookup WHERE ns=?1)
    OR EXISTS(SELECT 1 FROM native_mount WHERE ns=?1)
    OR EXISTS(SELECT 1 FROM native_directory WHERE ns=?1)";
const OBSERVE_CLEANUP: &str = "SELECT w.incarnation,w.lifecycle,
    EXISTS(SELECT 1 FROM reclaim WHERE ns=?1 AND queue_key=?2),
    (SELECT seq FROM sqlite_sequence WHERE name='workspace')
    FROM (SELECT ?1 AS ns) AS selected LEFT JOIN workspace w ON w.ns=selected.ns";

/// Bounded close/cleanup observation for a minted route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupState {
    Live,
    Held,
    Queued,
    Gone,
}
impl Overlay {
    /// Read-only terminal observation without minting a mutable Route. A missing
    /// row is Gone within this engine's allocated namespace domain; this does
    /// not attest that a caller's incarnation previously owned the absent row.
    /// Existing rows require exact incarnation agreement. Namespace identifiers
    /// are never reused, including after their physical reclamation.
    pub fn observe_cleanup(
        &self,
        namespace: i64,
        incarnation: [u8; 32],
    ) -> OverlayResult<CleanupState> {
        self.available()?;
        if namespace <= 0 || incarnation == [0; 32] {
            return Err(OverlayError::Stale);
        }
        let row = self.query(
            StatementKind::Workspace,
            OBSERVE_CLEANUP,
            &[&namespace, &CLOSE_KEY],
            64,
            |row| {
                Ok((
                    row.get::<_, Option<Vec<u8>>>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            },
        )?;
        let (stored, closed, queued, allocated) = row.first().ok_or(OverlayError::Stale)?;
        if allocated.is_none_or(|last| namespace > last) {
            return Err(OverlayError::Stale);
        }
        let Some(stored) = stored else {
            return Ok(CleanupState::Gone);
        };
        if stored.as_slice() != incarnation {
            return Err(OverlayError::Stale);
        }
        Ok(match (closed, queued) {
            (Some(0), _) => CleanupState::Live,
            (Some(_), 0) => CleanupState::Held,
            (Some(_), _) => CleanupState::Queued,
            (None, _) => return Err(OverlayError::Stale),
        })
    }
    /// Exact plan of the read-only namespace observation. The only
    /// sqlite_sequence entry in this schema belongs to workspace.
    pub fn explain_cleanup_observation(&self) -> OverlayResult<Vec<String>> {
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {OBSERVE_CLEANUP}"),
            &[&1_i64, &CLOSE_KEY],
            512,
            |row| row.get(3),
        )
    }
    /// Plan of the owner test of one closed namespace: one keyed probe of
    /// each table that holds custody.
    pub fn explain_close_held(&self) -> OverlayResult<Vec<String>> {
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {HELD}"),
            &[&1_i64],
            512,
            |row| row.get(3),
        )
    }
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
        self.queue_closed_at(route, &state)
    }
    /// The same decision over the Workspace row this job already read. The
    /// job must not have changed that row's lifecycle, capture or base
    /// readers since.
    pub(crate) fn queue_closed_at(
        &self,
        route: Route,
        state: &crate::WorkspaceState,
    ) -> OverlayResult<()> {
        if !state.closed || state.captured.is_some() || state.base_readers != 0 {
            return Ok(());
        }
        // A pending reply holds the namespace; its last attempt owes the
        // owner turn that comes back here.
        let held = self.tickets.watch(route.ns)
            || self.query(StatementKind::Reclaim, HELD, &[&route.ns], 8, |row| {
                row.get::<_, i64>(0)
            })?[0]
                != 0;
        if !held {
            self.execute(StatementKind::Reclaim,
                "INSERT INTO reclaim(ns,queue_key,target,cursor) VALUES(?1,?2,?2,0) ON CONFLICT(ns,queue_key) DO NOTHING",
                &[&route.ns,&CLOSE_KEY],16)?;
            self.closed_ready.set(true);
        }
        Ok(())
    }
}
