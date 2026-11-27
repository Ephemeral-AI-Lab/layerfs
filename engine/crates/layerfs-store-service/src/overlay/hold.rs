//! A terminal unmount's hold on a Workspace's queued maintenance (#679), and
//! a merged Commit's hold on its mutations.
//!
//! The reclamation a Commit leaves queued drops rows of the Workspace's own
//! overlay file, which terminal close deletes as a whole. While a hold is
//! taken, no job of the Workspace drains that queue at its end, a reply
//! attempt does not either, and `reclaim` runs no further step: the unmount
//! pays only for its own work. Dropping the hold runs nothing; an unmount
//! that leaves its Workspace bound (a refusal) drains the queue itself after
//! dropping it (`layerfs-daemon` `control/unmount.rs`).
use super::state::Shared;
use crate::{OwnerClient, OwnerError};
use layerfs_overlay::OverlayError;
use layerfs_vfs::Route;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

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
        // No lock: a drain loop retakes the Workspace lock step after step,
        // and the hold must land while it runs.
        shared.held.fetch_add(1, Ordering::AcqRel);
        Ok(MaintenanceHold {
            shared: Arc::downgrade(shared),
        })
    }
}
/// Held while it lives: the Workspace's mutations wait (the frontier wait
/// of `owner.rs`, lock released) until it is dropped. A merged Commit takes
/// it before it checks the active rows and keeps it through its publication
/// and install, so no row it checked changes before the new base is in
/// place (`store/live.rs`). It holds no lock and runs no job; other jobs,
/// the install among them, run as usual. Like [`MaintenanceHold`] it keeps
/// no connection alive.
#[must_use = "mutations resume when it is dropped"]
pub struct MutationHold {
    shared: Weak<Shared>,
}
impl OwnerClient {
    /// Holds the mutations of the Workspace at `route` until the returned
    /// hold is dropped. A mutation already running finishes first: the hold
    /// is taken under the Workspace lock. Holds count, so they nest.
    pub fn hold_mutations(&self, route: Route) -> Result<MutationHold, OwnerError> {
        if let Some(pool) = self.roster() {
            return pool.client(route.namespace())?.hold_mutations(route);
        }
        let shared = self.connection()?;
        if shared.route.get() != Some(&route) {
            return Err(OwnerError::Overlay(OverlayError::Stale));
        }
        let mut state =
            shared.foreground(|| shared.state.lock().unwrap_or_else(|e| e.into_inner()));
        state.mutation_holds += 1;
        Ok(MutationHold {
            shared: Arc::downgrade(shared),
        })
    }
}
impl Drop for MutationHold {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.mutation_holds -= 1;
            drop(state);
            shared.changed.notify_all();
        }
    }
}
impl Drop for MaintenanceHold {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.held.fetch_sub(1, Ordering::AcqRel);
        }
    }
}
