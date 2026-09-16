//! Incremental direct-reference availability.
//!
//! A stored physical record is complete only when every direct logical reference
//! it carries is already resolved. References are checked against the objects
//! already inserted by this operation, the membership result of the current wave
//! and, only for the remainder, one bounded presence query. Nothing here queries
//! the engine for a child that this operation has only accepted into memory.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use layerfs_content::{FinalizedObject, ObjectId};

use crate::error::{StorageError, StorageResult};
use crate::sqlite::lookup;

/// Availability facts established for one preparation wave.
#[derive(Debug, Default)]
pub struct Availability {
    present: BTreeMap<ObjectId, ()>,
    inserted: BTreeSet<ObjectId>,
}

impl Availability {
    /// Seeds the wave with the membership result of its offered identities.
    pub fn new(present: impl IntoIterator<Item = ObjectId>) -> Self {
        Self {
            present: present.into_iter().map(|id| (id, ())).collect(),
            inserted: BTreeSet::new(),
        }
    }

    /// Records that this operation has already inserted `id` in this wave.
    pub fn inserted(&mut self, id: ObjectId) {
        self.inserted.insert(id);
    }

    /// True when `id` is known to be available without another query.
    pub fn known(&self, id: ObjectId) -> bool {
        self.inserted.contains(&id) || self.present.contains_key(&id)
    }

    /// Verifies every direct reference of `object`.
    ///
    /// `pending` reports identities this owner has accepted into an unfinished
    /// group but not yet written. They are available to the same operation and
    /// must not trigger a query for bytes that only exist in memory.
    pub fn validate(
        &mut self,
        connection: &Connection,
        object: &FinalizedObject,
        ceiling: i64,
        pending: impl Fn(ObjectId) -> bool,
    ) -> StorageResult<()> {
        let missing = self.unresolved(object, &pending);
        if missing.is_empty() {
            return Ok(());
        }
        for found in lookup::present(connection, &missing, ceiling)? {
            self.present.insert(found, ());
        }
        for reference in self.unresolved(object, &pending) {
            if !self.known(reference) {
                return Err(StorageError::MissingDependency {
                    object: object.id(),
                    reference,
                });
            }
        }
        Ok(())
    }

    fn unresolved(
        &self,
        object: &FinalizedObject,
        pending: &impl Fn(ObjectId) -> bool,
    ) -> Vec<ObjectId> {
        let mut missing = Vec::new();
        let mut seen = BTreeSet::new();
        for reference in object.references() {
            if seen.insert(*reference) && !self.known(*reference) && !pending(*reference) {
                missing.push(*reference);
            }
        }
        missing
    }
}
