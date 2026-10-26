//! Fixed captured membership and publication/reply-attempt ordering primitives.
use crate::{
    db::integer, inode, sql, Capture, Inode, Overlay, OverlayError, OverlayResult, Publication,
    StatementKind,
};

impl Overlay {
    /// Advances a capture after known history success. The trusted caller must
    /// supply the root constructed/saved/published from this exact capture.
    /// Later active rows and pending replies remain untouched. Physical retire
    /// work is queued; last-owner eligibility/automatic deletion is S6 work.
    pub fn install(&self, capture: Capture, root: [u8; 32]) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            if !self.install_ready(capture)? {
                return Err(OverlayError::BaseSourcesPending);
            }
            self.change_state(|state| {
                state.base_root = root;
                state.installed = capture.generation.0;
                state.captured = None;
                state.captured_revision = None;
                Ok(())
            })?;
            self.enqueue(crate::maintenance::RETIRE, 0, capture.generation.0)?;
            self.wake_orphan_sources(capture.generation.0)?;
            Ok(())
        })
    }
    /// Readiness before the one known-install attempt. The daemon fences later
    /// source acquisitions and parks the original job until exact releases.
    pub fn install_ready(&self, capture: Capture) -> OverlayResult<bool> {
        self.checked_capture(capture)?;
        Ok(self.state()?.base_readers == 0)
    }
    /// Final captured name bindings, indexed by their sealed generation. The
    /// resume key is (parent,binary name); no OFFSET or active-domain filtering.
    pub fn captured_directory_entries(
        &self,
        capture: Capture,
        after: Option<(u64, &[u8])>,
    ) -> OverlayResult<Vec<crate::DirectoryEntry>> {
        self.checked_capture(capture)?;
        let (parent, name) = after.unwrap_or((0, &[]));
        if name.len() > 255 {
            return Err(OverlayError::Invalid("captured name cursor"));
        }
        self.query(
            StatementKind::Capture,
            sql::DIRECTORY_ENTRY_CAPTURE,
            &[&capture.generation.0, &integer(parent)?, &name],
            24 + name.len() as u64,
            entry,
        )
    }
    /// Readiness observation before a capture attempt. Waiting callers retain
    /// their original operation and wake on reply-attempt progress, without retry.
    pub fn capture_ready(&self) -> OverlayResult<bool> {
        let state = self.live()?;
        if state.captured.is_some() {
            return Err(OverlayError::CaptureInFlight);
        }
        if state.consolidating.is_some() {
            return Ok(false);
        }
        // The last reply attempt of the namespace owes this waiter a turn.
        Ok(!self.tickets.watch())
    }
    /// Records a send attempt, including a lost reply, on the owner's turn.
    /// It never removes published inode/name/byte state and never claims
    /// kernel delivery. A replying thread records its attempt through
    /// `reply_tickets` instead; its last attempt wakes capture readiness.
    pub fn reply_attempted(&self, publication: Publication) -> OverlayResult<()> {
        self.state()?;
        self.tickets.attempted(publication)?;
        Ok(())
    }
    /// Seals existing rows without copying them. The daemon parks capture until
    /// earlier reply attempts settle; this method does not block a SQL owner.
    pub fn capture(&self) -> OverlayResult<Capture> {
        self.atomic_cleanup(|| {
            let state = self.live()?;
            if state.captured.is_some() {
                return Err(OverlayError::CaptureInFlight);
            }
            if state.consolidating.is_some() {
                return Err(OverlayError::Consolidating);
            }
            if self.tickets.pending() {
                return Err(OverlayError::ReplyAttemptsPending);
            }
            let next = state
                .active
                .0
                .checked_add(1)
                .ok_or(OverlayError::Invalid("generation exhausted"))?;
            self.change_state(|current| {
                current.captured = Some(state.active);
                current.captured_revision = Some(state.revision);
                current.active = crate::Generation(next);
                current.dirty_inodes = 0;
                current.dirty_directory_entries = 0;
                Ok(())
            })?;
            // Kept rows are frozen with the generation; construction of this
            // capture carries them again.
            let kept: Vec<i64> = self.open.borrow().kept_live.rows.keys().copied().collect();
            for serial in kept {
                self.open.borrow_mut().kept_live.remove(serial);
            }
            Ok(Capture {
                engine: self.identity,
                generation: state.active,
                revision: state.revision,
                base_root: state.base_root,
            })
        })
    }
    /// Captured inode page with a fixed indexed domain and bounded reply. Later
    /// active inserts cannot change its membership or extend EOF.
    pub fn captured_inodes(&self, capture: Capture, after: u64) -> OverlayResult<Vec<Inode>> {
        self.checked_capture(capture)?;
        self.query(
            StatementKind::Capture,
            sql::INODE_CAPTURE,
            &[&capture.generation.0, &integer(after)?],
            24,
            inode::decode,
        )
    }
    /// Changed inodes of the active generation after `after`, from the same
    /// indexed window a capture reads. A diagnostic observation: membership
    /// may change between windows.
    pub fn active_inodes(&self, after: u64) -> OverlayResult<Vec<Inode>> {
        let state = self.live()?;
        self.query(
            StatementKind::Capture,
            sql::INODE_CAPTURE,
            &[&state.active.0, &integer(after)?],
            24,
            inode::decode,
        )
    }
    /// Changed name bindings of the active generation after `(parent, name)`,
    /// as [`Self::active_inodes`] observes inodes.
    pub fn active_directory_entries(
        &self,
        after: Option<(u64, &[u8])>,
    ) -> OverlayResult<Vec<crate::DirectoryEntry>> {
        let state = self.live()?;
        let (parent, name) = after.unwrap_or((0, &[]));
        if name.len() > 255 {
            return Err(OverlayError::Invalid("active name cursor"));
        }
        self.query(
            StatementKind::Capture,
            sql::DIRECTORY_ENTRY_CAPTURE,
            &[&state.active.0, &integer(parent)?, &name],
            24 + name.len() as u64,
            entry,
        )
    }
    pub(crate) fn checked_capture(&self, capture: Capture) -> OverlayResult<()> {
        let state = self.state()?;
        if capture.engine != self.identity
            || state.captured != Some(capture.generation)
            || state.captured_revision != Some(capture.revision)
            || state.base_root != capture.base_root
        {
            return Err(OverlayError::Stale);
        }
        Ok(())
    }
}

/// One captured or active name binding row.
fn entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<crate::DirectoryEntry> {
    Ok(crate::DirectoryEntry {
        inherited: row.get(3)?,
        parent: crate::db::unsigned(row, 0)?,
        name: row.get(1)?,
        serial: row
            .get::<_, Option<i64>>(2)?
            .map(|v| u64::try_from(v).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(2, v)))
            .transpose()?,
    })
}
