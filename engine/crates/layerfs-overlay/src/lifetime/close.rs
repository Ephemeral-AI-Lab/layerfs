//! Logical terminal close; physical disposal belongs to the Workspace file owner.
use crate::{Capture, Overlay, OverlayError, OverlayResult};

/// Whether a Workspace is serving, retained by a consumer, ready for checked
/// connection close, or physically absent from its owner. Only the daemon's
/// file owner can establish Gone after connection close and file deletion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupState {
    Live,
    Held,
    Queued,
    Gone,
}
impl Overlay {
    fn terminal_state(&self, state: &crate::WorkspaceState) -> OverlayResult<CleanupState> {
        if !state.closed {
            return Ok(CleanupState::Live);
        }
        let open = self.open.borrow();
        let kernel = open.mount.is_some_and(|(_, revoked)| !revoked);
        let files = open.files.any_request((0, i64::MIN)..=(0, i64::MAX));
        let readers = open.reads.len() != 0;
        let lookups = open.lookups.len() != 0;
        let pins = open.operations.len() != 0 || open.captured.len() != 0;
        drop(open);
        let held = kernel
            || files
            || readers
            || lookups
            || state.captured.is_some()
            || state.base_readers != 0
            || self.tickets.pending()
            || pins;
        Ok(if held {
            CleanupState::Held
        } else {
            CleanupState::Queued
        })
    }
    /// Read-only exact route observation; it runs no cleanup and proves no unlink.
    pub fn cleanup_state(&self) -> OverlayResult<CleanupState> {
        let state = self.state()?;
        self.terminal_state(&state)
    }
    /// Closes mutation/acquisition admission. Existing exact consumers keep their
    /// original custody until fenced; no namespace row or payload is swept here.
    pub fn close(&self) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            self.live()?;
            self.change_state(|state| {
                state.closed = true;
                Ok(())
            })?;
            Ok(())
        })
    }
    /// Exact capture relinquishment after its consumer and publication fences.
    pub fn release_closed_capture(&self, capture: Capture) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            let state = self.state()?;
            self.checked_capture(capture)?;
            if !state.closed {
                return Err(OverlayError::Stale);
            }
            self.change_state(|state| {
                state.captured = None;
                state.captured_revision = None;
                Ok(())
            })?;
            self.wake_orphan_sources(capture.generation.0)
        })
    }
}
