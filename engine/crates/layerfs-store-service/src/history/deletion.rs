//! The Store side of project.delete: the marker and the retry it makes safe.
//!
//! The marker is the commit point and is written before the caller removes its
//! home entry, so a lost reply is retried by the stack id the caller recorded
//! in its durable intent. A retry reads the marker first: a retired stack
//! reuses the stored deleted_ms, a live stack with no marker redoes the marker
//! with the same deleted_ms and keeps the entry, and an absent stack lets the
//! caller finish removing its entry. This module never resolves a name: the
//! caller has the stack id, and a hidden project has no name to resolve.
use crate::history::lookup;
use crate::store::Store;
use layerfs_bridge::control::SdkRefusal;
use layerfs_history::records::{GcDeletion, GcIntent};
use layerfs_history::LayerStackId;
use std::sync::Arc;

/// What the Store side of project.delete found or did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Deletion {
    /// The marker was already recorded; its deleted_ms is reused.
    Retired {
        /// The deleted_ms the marker holds.
        deleted_ms: i64,
    },
    /// A live stack was marked with the caller's deleted_ms.
    Marked {
        /// The deleted_ms written.
        deleted_ms: i64,
    },
    /// Neither a live stack nor a marker: the caller removes its stale entry.
    Absent,
}

impl Deletion {
    /// The deleted_ms a project.delete result reports: the marker's when the
    /// stack was already retired, else the requested one.
    pub fn deleted_ms(self, requested: i64) -> i64 {
        match self {
            Self::Retired { deleted_ms } | Self::Marked { deleted_ms } => deleted_ms,
            Self::Absent => requested,
        }
    }
}

/// Records the project.delete marker for stack, in one Store transaction, or
/// classifies why it cannot: retired (the marker is there), live (mark it now),
/// or absent (nothing is there to mark). A repeat is Retired, never a second
/// marker: the caller reuses the marker's deleted_ms.
pub fn delete_stack(
    store: &Arc<Store>,
    stack: LayerStackId,
    deleted_ms: i64,
) -> Result<Deletion, SdkRefusal> {
    let intent = crate::history::admission::project_removal_from_id(stack);
    delete_stack_with(store, stack, deleted_ms, &intent)
}

/// [`delete_stack`] under an admission intent the caller already registered.
/// The marker's own admission scan excludes this operation identity, so the
/// caller's row is reused and released once instead of being seen as another
/// removal. A provider without admission writes the marker as before.
pub fn delete_stack_with(
    store: &Arc<Store>,
    stack: LayerStackId,
    deleted_ms: i64,
    intent: &GcIntent,
) -> Result<Deletion, SdkRefusal> {
    let history = store.history();
    match history
        .gc_delete_stack(stack, deleted_ms, intent)
        .map_err(lookup::history)?
    {
        GcDeletion::Retired { deleted_ms } => Ok(Deletion::Retired { deleted_ms }),
        GcDeletion::Marked { deleted_ms } => Ok(Deletion::Marked { deleted_ms }),
        GcDeletion::Absent => Ok(Deletion::Absent),
    }
}
