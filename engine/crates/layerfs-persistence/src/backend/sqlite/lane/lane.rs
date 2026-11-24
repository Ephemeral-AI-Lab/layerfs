//! The Store's publication lane: one connection turn and the cooperative
//! writer lock (Linux and macOS), held by one thread across a short
//! read-decide-write sequence. Its own SQL work runs as ordinary transactions
//! inside the held turn; no other publication of any process can interleave.
use super::writer_admission::Writer;
use crate::backend::records::BackendError;
use crate::backend::sqlite::session::connection::Session;
use std::{marker::PhantomData, sync::Arc};

/// A held lane; dropping it releases the writer lock, then the turn.
pub struct LaneHold {
    session: Arc<Session>,
    writer: Option<Writer>,
    // The hold belongs to the thread that took it.
    _thread: PhantomData<*const ()>,
}

impl Session {
    /// Waits its FIFO turn, then the cross-process writer lock, for the lane.
    /// A wait is readiness, not a retry: nothing is attempted before it is held.
    pub(crate) fn hold_lane(self: &Arc<Self>) -> Result<LaneHold, BackendError> {
        if !self.writable {
            return Err(BackendError::ReadOnly);
        }
        let turn = self
            .admission
            .enter(layerfs_telemetry::timer::waits::Wait::Writer)?;
        if turn.nested {
            return Err(BackendError::Integrity);
        }
        // A quarantined session, or one that still holds its own writer
        // (a release that failed), is refused before the blocking wait: that
        // writer is this very lock, so waiting for it would never end
        // (#776 S4).
        {
            let state = self.state();
            if state.quarantined || state.writer_hold.is_some() {
                return Err(BackendError::Unknown);
            }
        }
        let writer = Writer::acquire(&self.path)?;
        turn.hold()?;
        Ok(LaneHold {
            session: self.clone(),
            writer: Some(writer),
            _thread: PhantomData,
        })
    }
}

impl LaneHold {
    /// Takes the cross-process writer lock out of the hold and releases the
    /// connection turn, so a seal can consume the session while the lane is
    /// still held (§7 step 1). The caller becomes the lane's owner.
    pub(crate) fn into_writer(mut self) -> Writer {
        self.writer
            .take()
            .expect("a held Store lane owns its writer")
    }
}

impl Drop for LaneHold {
    fn drop(&mut self) {
        if let Some(writer) = self.writer.take() {
            // Nested work that quarantined the session may have left a write
            // transaction open on its connection. That connection is closed
            // here, while this thread still holds the lane, before the lane
            // is released (finding #2); if SQLite refuses the close with a
            // transaction open, the lane stays with the connection instead.
            // A state a panic poisoned is recovered quarantined, so its
            // connection (which may hold an open write transaction) is closed
            // here too, before the lane goes (#776 S3).
            let mut writer = Some(writer);
            {
                let mut state = self.session.state();
                if state.quarantined && !state.close_quarantined(self.session.background.as_ref()) {
                    state.writer_hold = writer.take();
                }
            }
            if let Some(Err(retained)) = writer.map(Writer::release) {
                // As a failed per-job release: keep the lock, refuse later work.
                let mut state = self.session.state();
                state.quarantined = true;
                state.writer_hold = Some(retained);
            }
        }
        self.session.admission.release_hold();
    }
}
