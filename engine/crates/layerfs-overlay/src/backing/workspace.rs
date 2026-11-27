//! One Workspace's journaled runtime state; the database owns its filesystem rows.
use crate::{Generation, Overlay, OverlayError, OverlayResult, WorkspaceState};

impl Overlay {
    /// Binds one complete root without scanning it. Authority/root validation
    /// belongs to the Workspace/runtime caller; a second binding is refused.
    pub fn open_workspace(&self, base_root: [u8; 32]) -> OverlayResult<()> {
        self.atomic(|| {
            if self.workspace.get().is_some() {
                return Err(OverlayError::Invalid("database already holds a Workspace"));
            }
            self.workspace.set(Some(WorkspaceState {
                active: Generation(1),
                captured: None,
                captured_revision: None,
                installed: 0,
                revision: 0,
                base_root,
                dirty_inodes: 0,
                dirty_directory_entries: 0,
                closed: false,
                base_readers: 0,
                consolidating: None,
            }));
            Ok(())
        })
    }
    pub fn state(&self) -> OverlayResult<WorkspaceState> {
        self.available()?;
        self.workspace.get().ok_or(OverlayError::Stale)
    }
    pub(crate) fn change_state(
        &self,
        change: impl FnOnce(&mut WorkspaceState) -> OverlayResult<()>,
    ) -> OverlayResult<()> {
        let mut state = self.state()?;
        change(&mut state)?;
        self.workspace.set(Some(state));
        Ok(())
    }
    pub(crate) fn live(&self) -> OverlayResult<WorkspaceState> {
        let state = self.state()?;
        if state.closed {
            Err(OverlayError::Closed)
        } else {
            Ok(state)
        }
    }
    pub(crate) fn source_count(&self, add: bool) -> OverlayResult<()> {
        self.change_state(|state| {
            state.base_readers = changed_count(state.base_readers, if add { 1 } else { -1 })?;
            Ok(())
        })
    }
    pub(crate) fn dirty_counts(&self, inodes: i64, names: i64) -> OverlayResult<()> {
        self.change_state(|state| {
            state.dirty_inodes = changed_count(state.dirty_inodes, inodes)?;
            state.dirty_directory_entries = changed_count(state.dirty_directory_entries, names)?;
            Ok(())
        })
    }
}
/// Preserve the former nonnegative signed SQLite counter domain.
fn changed_count(value: u64, delta: i64) -> OverlayResult<u64> {
    value
        .checked_add_signed(delta)
        .filter(|value| *value <= i64::MAX as u64)
        .ok_or(OverlayError::Invalid("Workspace counter range"))
}
