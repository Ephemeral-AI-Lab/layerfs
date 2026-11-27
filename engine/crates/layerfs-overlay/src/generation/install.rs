//! Known install: advance the base to a captured generation after history
//! success, keeping later active mutations and the effective live view.
use crate::{Capture, Overlay, OverlayError, OverlayResult, StatementKind};

impl Overlay {
    /// Advances a capture after known history success. The trusted caller must
    /// supply the root constructed/saved/published from this exact capture.
    /// Later active rows and pending replies remain untouched. Physical retire
    /// work is queued; last-owner eligibility/automatic deletion is S6 work.
    pub fn install(&self, capture: Capture, root: [u8; 32]) -> OverlayResult<()> {
        self.install_merged(capture, root, &[])
    }
    /// [`Self::install`] of a root other than the one constructed from the
    /// capture (a merge). `changed` lists, sorted, the serials whose value
    /// differs between the two roots. A row [`Self::keep_live_row`] carried
    /// for one of them would let its bytes fall through to the new base's
    /// different ones, so it is deleted with the install: its only change, a
    /// live mtime, is never committed. Every other active row of a changed
    /// serial, or under a changed directory, was computed against the old
    /// base: the caller refuses the install while one exists and holds
    /// later mutations until it is done (`layerfs-store-service`
    /// `store/live.rs`). Rows of unchanged serials stay valid over the new
    /// base, which holds the same value for them.
    pub fn install_merged(
        &self,
        capture: Capture,
        root: [u8; 32],
        changed: &[u64],
    ) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            if !self.install_ready(capture)? {
                return Err(OverlayError::BaseSourcesPending);
            }
            let active = self.state()?.active;
            let stale: Vec<i64> = self
                .open
                .borrow()
                .kept_live
                .rows
                .keys()
                .copied()
                .filter(|serial| {
                    u64::try_from(*serial)
                        .is_ok_and(|serial| changed.binary_search(&serial).is_ok())
                })
                .collect();
            for serial in &stale {
                self.execute(
                    StatementKind::Inode,
                    "DELETE FROM fs_inode WHERE inode=?1 AND generation=?2",
                    &[serial, &active.0],
                    24,
                )?;
                self.open.borrow_mut().kept_live.remove(*serial);
            }
            self.change_state(|state| {
                if !stale.is_empty() {
                    state.revision = state
                        .revision
                        .checked_add(1)
                        .ok_or(OverlayError::Invalid("revision exhausted"))?;
                }
                state.base_root = root;
                state.installed = capture.generation.0;
                state.captured = None;
                state.captured_revision = None;
                Ok(())
            })?;
            self.enqueue(crate::reclaim::RETIRE, 0, capture.generation.0)?;
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
}
