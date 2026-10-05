//! Fixed captured membership and publication/reply-attempt ordering primitives.
use crate::{
    db::integer, inode, sql, Capture, Generation, Inode, Overlay, OverlayError, OverlayResult,
    Publication, Route, StatementKind,
};

impl Overlay {
    /// Query plans/VM program for the two exact known-install statements.
    /// Static opcodes are strategy evidence; diagnostics() observes actual work.
    pub fn explain_install(&self, capture: Capture, root: [u8; 32]) -> OverlayResult<Vec<String>> {
        self.checked_capture(capture)?;
        let mut plan = self.query(StatementKind::Explain,
            "EXPLAIN QUERY PLAN UPDATE workspace SET base_root=?2,installed=?3,captured=NULL WHERE ns=?1",
            &[&capture.route.ns,&root.as_slice(),&capture.generation.0],48,|row|row.get(3))?;
        let vm = self.query(
            StatementKind::Explain,
            "EXPLAIN INSERT INTO reclaim(ns,queue_key,target,cursor) VALUES(?1,?2,?2,0)",
            &[&capture.route.ns, &capture.generation.0],
            16,
            |row| {
                Ok(format!(
                    "{} {} {} {} {}",
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?
                ))
            },
        )?;
        plan.extend(vm);
        Ok(plan)
    }
    /// Advances a capture after known history success. The trusted caller must
    /// supply the root constructed/saved/published from this exact capture.
    /// Later active rows and pending replies remain untouched. Physical retire
    /// work is queued; last-owner eligibility/automatic deletion is S6 work.
    pub fn install(&self, capture: Capture, root: [u8; 32]) -> OverlayResult<()> {
        self.atomic(|| {
            self.checked_capture(capture)?;
            self.execute(
                StatementKind::Capture,
                "UPDATE workspace SET base_root=?2,installed=?3,captured=NULL WHERE ns=?1",
                &[&capture.route.ns, &root.as_slice(), &capture.generation.0],
                48,
            )?;
            self.execute(
                StatementKind::Capture,
                "INSERT INTO reclaim(ns,queue_key,target,cursor) VALUES(?1,?2,?2,0)",
                &[&capture.route.ns, &capture.generation.0],
                16,
            )?;
            Ok(())
        })
    }
    /// Final captured name bindings, indexed by their sealed generation. The
    /// resume key is (parent,binary name); no OFFSET or active-domain filtering.
    pub fn captured_dentries(
        &self,
        capture: Capture,
        after: Option<(u64, &[u8])>,
    ) -> OverlayResult<Vec<crate::Dentry>> {
        self.checked_capture(capture)?;
        let (parent, name) = after.unwrap_or((0, &[]));
        self.query(
            StatementKind::Capture,
            sql::DENTRY_CAPTURE,
            &[
                &capture.route.ns,
                &capture.generation.0,
                &integer(parent)?,
                &name,
            ],
            24 + name.len() as u64,
            |row| {
                Ok(crate::Dentry {
                    parent: crate::db::unsigned(row, 0)?,
                    name: row.get(1)?,
                    serial: row
                        .get::<_, Option<i64>>(2)?
                        .map(|v| {
                            u64::try_from(v)
                                .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(2, v))
                        })
                        .transpose()?,
                })
            },
        )
    }
    /// Plan for the exact generation-selective name cursor.
    pub fn explain_dentry_capture(&self, capture: Capture) -> OverlayResult<Vec<String>> {
        self.checked_capture(capture)?;
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::DENTRY_CAPTURE),
            &[
                &capture.route.ns,
                &capture.generation.0,
                &0_i64,
                &&[] as &dyn rusqlite::ToSql,
            ],
            24,
            |row| row.get(3),
        )
    }
    /// Readiness observation before a capture attempt. Waiting callers retain
    /// their original operation and wake on reply-attempt progress, without retry.
    pub fn capture_ready(&self, route: Route) -> OverlayResult<bool> {
        let state = self.live(route)?;
        if state.captured.is_some() {
            return Err(OverlayError::CaptureInFlight);
        }
        Ok(self
            .query(
                StatementKind::Frontier,
                "SELECT revision FROM request WHERE ns=?1 ORDER BY revision LIMIT 1",
                &[&route.ns],
                8,
                |row| row.get::<_, i64>(0),
            )?
            .is_empty())
    }
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
            &[
                &route.ns,
                &integer(serial)?,
                &state.active.0,
                &state.installed,
            ],
            32,
            |r| r.get(3),
        )
    }
    /// Returns the active generation for a caller's bounded cell/read job.
    pub fn active_generation(&self, route: Route) -> OverlayResult<Generation> {
        Ok(self.live(route)?.active)
    }
}
