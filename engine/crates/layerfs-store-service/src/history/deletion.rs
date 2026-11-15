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
use layerfs_history::error::Missing;
use layerfs_history::{HistoryError, LayerStackId};
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
    let history = store.history();
    if let Some(recorded) = history.stack_deleted_ms(stack).map_err(lookup::history)? {
        return Ok(Deletion::Retired {
            deleted_ms: recorded,
        });
    }
    let live = history
        .layer_stack(stack)
        .map_err(lookup::history)?
        .is_some();
    if !live {
        // A concurrent attempt may have marked it after the read above.
        return Ok(
            match history.stack_deleted_ms(stack).map_err(lookup::history)? {
                Some(recorded) => Deletion::Retired {
                    deleted_ms: recorded,
                },
                None => Deletion::Absent,
            },
        );
    }
    match history.mark_stack_deleted(stack, deleted_ms) {
        Ok(()) => Ok(Deletion::Marked { deleted_ms }),
        // Another attempt marked it between the read and the write; reuse its
        // deleted_ms rather than fail a retry that lost only a reply.
        Err(HistoryError::Missing(Missing::LayerStack)) => Ok(
            match history.stack_deleted_ms(stack).map_err(lookup::history)? {
                Some(recorded) => Deletion::Retired {
                    deleted_ms: recorded,
                },
                None => Deletion::Absent,
            },
        ),
        Err(error) => Err(lookup::history(error)),
    }
}
