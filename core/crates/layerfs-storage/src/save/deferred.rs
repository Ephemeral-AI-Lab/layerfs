//! Whole-file objects whose delta base this Save has not published yet (#810).
//!
//! The similarity ring proposes, as the base of a whole-file object, the last
//! similar object this Save stored FULL; that one is often still pending. A
//! base is used only once its first-wins representation is acknowledged, so
//! the object used to wait for a publication of its own: one per similar
//! file. When a trial against the base's kept bytes shows the PREFIX wins, the
//! object is held here instead, and selected after the wave's own publication
//! has published every held object's base. Admission drains this cohort before
//! placing a dependent or using a held object as a declared predecessor.
//! Same-Save reads and exact reuse see held objects here.
use crate::store::policy::SAVE_DEFERRED_BYTES;
use layerfs_content::{FinalizedObject, ObjectId};

/// What one held object is charged besides its canonical bytes.
const ENTRY_CHARGE: usize = 96;

/// Held objects, each with the base it waits for, in arrival order, within
/// [`SAVE_DEFERRED_BYTES`].
#[derive(Default)]
pub(super) struct Deferred {
    held: Vec<(FinalizedObject, ObjectId)>,
    charged: usize,
}

impl Deferred {
    /// True when nothing is held.
    pub(super) fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// Holds `object` until `base` is published; false when the bound has no
    /// room, and the caller publishes the base first, as before.
    pub(super) fn hold(&mut self, object: &FinalizedObject, base: ObjectId) -> bool {
        let charge = object.canonical_len() + ENTRY_CHARGE;
        if self.charged + charge > SAVE_DEFERRED_BYTES {
            return false;
        }
        self.charged += charge;
        self.held.push((object.clone(), base));
        true
    }

    /// The canonical bytes of held object `id`.
    pub(super) fn canonical(&self, id: ObjectId) -> Option<&[u8]> {
        self.held
            .iter()
            .find(|(object, _)| object.id() == id)
            .map(|(object, _)| object.canonical())
    }

    /// True when `id` is held.
    pub(super) fn holds(&self, id: ObjectId) -> bool {
        self.canonical(id).is_some()
    }

    /// Canonical bytes held.
    pub(super) fn canonical_bytes(&self) -> u64 {
        self.held
            .iter()
            .map(|(object, _)| object.canonical_len() as u64)
            .sum()
    }

    /// The bases the held objects wait for, sorted and unique.
    pub(super) fn bases(&self) -> Vec<ObjectId> {
        let mut bases: Vec<ObjectId> = self.held.iter().map(|(_, base)| *base).collect();
        bases.sort_unstable();
        bases.dedup();
        bases
    }

    /// Releases every held object with its base, in arrival order.
    pub(super) fn take(&mut self) -> Vec<(FinalizedObject, ObjectId)> {
        self.charged = 0;
        std::mem::take(&mut self.held)
    }
}
