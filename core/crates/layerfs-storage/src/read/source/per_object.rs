//! Per-object demand state (#818 H5): the packs a demand saw refused and the
//! ids it located in a pack row that is gone, held while the demand's guard
//! is open and dropped with it.

use super::super::Fetch;
use crate::{
    error::StorageError,
    location::{LocatedChains, LocatedObject},
    port::PersistenceError,
};
use layerfs_content::ObjectId;
use std::collections::{BTreeMap, BTreeSet};

/// What a per-object demand saw refused (#818 H5), dropped with it: each
/// pack with its failure, at most one entry per pack the demand reads, and
/// the located ids whose pack row is gone (`locate_chains_each`), at most
/// the ids the demand locates.
#[derive(Default)]
pub(crate) struct Refused {
    packs: BTreeMap<i64, StorageError>,
    unpacked: BTreeSet<ObjectId>,
}

/// Holds a per-object demand open ([`Fetch::per_object`]); dropping the
/// guard that opened it ends the demand's per-object reads and drops what it
/// refused (a guard taken inside an open one, a file-length demand's state
/// read, leaves it open).
pub(crate) struct PerObject<'a>(&'a Fetch, bool);
impl Drop for PerObject<'_> {
    fn drop(&mut self) {
        if self.1 {
            *self.0.refused.borrow_mut() = None;
        }
    }
}

impl Fetch {
    /// Makes the demand that holds the guard a per-object demand (#818 H5)
    /// until the guard drops: every page of it, the walk's and the
    /// reconstruction's included, is read with `read_groups_classified` (one
    /// call, as `read_groups`), so a pack the provider refuses (`Missing`,
    /// `Malformed`) or whose answer fails admission is kept refused with its
    /// failure, for the objects that need it, instead of failing the call; a
    /// pack only members name is still a stale hint (and refused when
    /// `Missing`). A refused pack is not planned or acquired again in the
    /// demand. A failure of the call, or one the answer does not tie to a
    /// pack, fails the call.
    pub(crate) fn per_object(&self) -> PerObject<'_> {
        let mut refused = self.refused.borrow_mut();
        let opens = refused.is_none();
        if opens {
            *refused = Some(Refused::default());
        }
        PerObject(self, opens)
    }
    /// The failure of pack `id` if this per-object demand saw it refused.
    pub(crate) fn refused_pack(&self, id: i64) -> Option<StorageError> {
        let refused = self.refused.borrow();
        refused
            .as_ref()?
            .packs
            .get(&id)
            .map(super::super::objects::duplicate)
    }
    /// Keeps pack `id` refused with `error` for the rest of a per-object
    /// demand (its first failure stands).
    pub(crate) fn refuse(&self, id: i64, error: StorageError) {
        if let Some(refused) = self.refused.borrow_mut().as_mut() {
            refused.packs.entry(id).or_insert(error);
        }
    }
    pub(super) fn is_refused(&self, id: i64) -> bool {
        let refused = self.refused.borrow();
        refused
            .as_ref()
            .is_some_and(|refused| refused.packs.contains_key(&id))
    }
    /// One locate page (a `locate_chains` call): in a per-object demand,
    /// `locate_chains_each`, whose located ids with no pack row are kept as
    /// unpacked instead of failing the call; they are never located again
    /// in the demand.
    pub(super) fn locate_page(
        &self,
        page: &[ObjectId],
        rows: &mut Vec<LocatedObject>,
        chains: &mut LocatedChains,
    ) -> Result<(), PersistenceError> {
        if self.refused.borrow().is_none() {
            return self.metadata.locate_chains(page, rows, chains);
        }
        let mut unpacked = Vec::new();
        self.metadata
            .locate_chains_each(page, rows, chains, &mut unpacked)?;
        if let Some(refused) = self.refused.borrow_mut().as_mut() {
            refused.unpacked.extend(unpacked);
        }
        Ok(())
    }
    /// The failure of `id` if this per-object demand located it in a pack
    /// row that is gone: the provider's refusal of a locate naming it
    /// (`Malformed`), which a point read of it meets.
    pub(crate) fn unpacked(&self, id: ObjectId) -> Option<StorageError> {
        let refused = self.refused.borrow();
        let unpacked = refused.as_ref()?.unpacked.contains(&id);
        unpacked.then(|| PersistenceError::Malformed.into())
    }
}
