//! A terminal unmount's hold on a Workspace's queued maintenance (#679).
//!
//! The reclamation a Commit leaves queued drops rows of the Workspace's own
//! overlay file, which terminal close deletes as a whole. While a hold is
//! taken, no job of the Workspace drains that queue at its end, a reply
//! attempt does not either, and `reclaim` runs no further step: the unmount
//! pays only for its own work. Dropping the hold changes nothing else; when
//! the unmount was refused, the next job drains the queue as before.
use super::state::Shared;
use crate::{OwnerClient, OwnerError};
use layerfs_overlay::OverlayError;
use layerfs_workspace::Route;
use std::sync::Weak;

/// Held while it lives. It keeps no connection alive: a Workspace retired
/// meanwhile is released as it would be without it.
#[must_use = "the hold ends when it is dropped"]
pub struct MaintenanceHold {
    shared: Weak<Shared>,
}
impl OwnerClient {
    /// Holds the queued maintenance of the Workspace at `route` until the
    /// returned hold is dropped. Holds count, so concurrent ones nest.
    pub fn hold_maintenance(&self, route: Route) -> Result<MaintenanceHold, OwnerError> {
        if let Some(pool) = self.roster() {
            return pool.client(route.namespace())?.hold_maintenance(route);
        }
        let shared = self.connection()?;
        if shared.route.get() != Some(&route) {
            return Err(OwnerError::Overlay(OverlayError::Stale));
        }
        let mut state = shared.state.lock().map_err(|_| OwnerError::Stopped)?;
        state.held = state.held.saturating_add(1);
        Ok(MaintenanceHold {
            shared: std::sync::Arc::downgrade(shared),
        })
    }
}
impl Drop for MaintenanceHold {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.held = state.held.saturating_sub(1);
        }
    }
}
