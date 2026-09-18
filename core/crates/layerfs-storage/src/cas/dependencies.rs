//! Incremental direct-reference availability.
//!
//! A stored physical record is complete only when every direct logical reference
//! it carries is already resolved. References are checked against the objects
//! already inserted by this operation, the membership result of the current wave
//! and, only for the remainder, one bounded presence query. Nothing here queries
//! the engine for a child that this operation has only accepted into memory.

use std::collections::BTreeSet;

use rusqlite::Connection;

use layerfs_content::{FinalizedObject, ObjectId};

use crate::error::{StorageError, StorageResult};
use crate::sqlite::lookup;

/// Availability facts established for one preparation wave.
#[derive(Debug, Default)]
pub struct Availability {
    known: BTreeSet<ObjectId>,
}

impl Availability {
    /// Seeds the wave with the membership result of its offered identities.
    pub fn new(present: impl IntoIterator<Item = ObjectId>) -> Self {
        Self {
            known: present.into_iter().collect(),
        }
    }

    /// Records that this operation has already inserted `id` in this wave.
    pub fn inserted(&mut self, id: ObjectId) {
        self.known.insert(id);
    }

    /// True when `id` is known to be available without another query.
    pub fn known(&self, id: ObjectId) -> bool {
        self.known.contains(&id)
    }

    /// Verifies every direct reference of `object`.
    ///
    /// `pending` reports identities this owner has accepted into an unfinished
    /// group but not yet written. They are available to the same operation and
    /// must not trigger a query for bytes that only exist in memory.
    ///
    /// Returns the presence queries this call had to issue: zero once the wave was
    /// seeded (§`seed`), which is the whole point of seeding it.
    pub fn validate(
        &mut self,
        connection: &Connection,
        object: &FinalizedObject,
        ceiling: i64,
        pending: impl Fn(ObjectId) -> bool,
    ) -> StorageResult<u64> {
        let missing = self.unresolved(object, &pending);
        if missing.is_empty() {
            return Ok(0);
        }
        for found in lookup::present(connection, &missing, ceiling)? {
            self.known.insert(found);
        }
        for reference in self.unresolved(object, &pending) {
            if !self.known(reference) {
                return Err(StorageError::MissingDependency {
                    object: object.id(),
                    reference,
                });
            }
        }
        Ok(1)
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
