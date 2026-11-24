//! A Save's last publication held for its caller's own transaction (#768,
//! `Save::finish_prepared`), served to every read of the handle until it is
//! published or dropped.
//!
//! The Store does not have the held locations, pack bodies and inode values
//! yet, so the bounded read caches cannot stand in for them: an evicted
//! entry would be looked up in the Store and missed. The held publication is
//! in memory already and its size is bounded by the publication's own row
//! and byte bounds, so it is indexed whole and consulted before the caches
//! and the Store. A held location found here is kept in the locator cache as
//! a Store answer would be.
use super::fetch::Fetch;
use crate::{
    error::StorageResult,
    location::{LocatedObject, PackInfo},
    port::Publication,
};
use layerfs_content::{inode_leaf::INODE_VALUE_BYTES, ObjectId};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// What one held publication answers.
#[derive(Default)]
pub(super) struct Held {
    locations: BTreeMap<ObjectId, LocatedObject>,
    bodies: BTreeMap<i64, (PackInfo, Arc<Vec<u8>>)>,
    values: BTreeMap<u32, [u8; INODE_VALUE_BYTES]>,
}

impl Fetch {
    /// Serves `batch` to this handle's reads until [`Self::release_held`].
    pub(crate) fn hold(&self, batch: &Publication) -> StorageResult<()> {
        let mut held = Held::default();
        for pack in &batch.packs {
            let info = pack.info();
            super::fetch::check_info(info)?;
            held.bodies
                .insert(info.pack_id, (info, pack.body().clone()));
        }
        let mut absent = self.absent.borrow_mut();
        for row in &batch.objects {
            let pack = held
                .bodies
                .get(&row.pack_id)
                .map(|(info, _)| *info)
                .ok_or(crate::error::StorageError::Integrity("held pack missing"))?;
            absent.remove(&row.object_id);
            held.locations.insert(
                row.object_id,
                LocatedObject {
                    location: *row,
                    pack,
                },
            );
        }
        for row in &batch.inode_values {
            held.values.insert(row.ordinal, row.value);
        }
        *self.held.borrow_mut() = Some(held);
        Ok(())
    }
    /// Stops serving the held publication: it was published (its entries
    /// are the Store's now) or dropped.
    pub(crate) fn release_held(&self) {
        *self.held.borrow_mut() = None;
    }
    /// Takes the held locations of `missing` out of it.
    pub(super) fn held_locations(&self, missing: &mut BTreeSet<ObjectId>) -> Vec<LocatedObject> {
        let held = self.held.borrow();
        let Some(held) = held.as_ref() else {
            return Vec::new();
        };
        let found: Vec<LocatedObject> = missing
            .iter()
            .filter_map(|id| held.locations.get(id).copied())
            .collect();
        for row in &found {
            missing.remove(&row.location.object_id);
        }
        found
    }
    /// A held pack body.
    pub(super) fn held_body(&self, id: i64) -> Option<(PackInfo, Arc<Vec<u8>>)> {
        self.held
            .borrow()
            .as_ref()
            .and_then(|held| held.bodies.get(&id).cloned())
    }
    /// A held inode value.
    pub(super) fn held_value(&self, ordinal: u32) -> Option<[u8; INODE_VALUE_BYTES]> {
        self.held
            .borrow()
            .as_ref()
            .and_then(|held| held.values.get(&ordinal).copied())
    }
}
