//! Bounded accepted/pending ownership.
//!
//! A batch holds finalized canonical objects until a preparation wave can carry
//! them. Both the object count and the canonical byte total are bounded, so a
//! producer that outruns storage stops instead of growing the queue. A batch
//! slot releases only when its object is actually released.

use layerfs_content::{FinalizedObject, ObjectId};

use crate::error::{StorageError, StorageResult};
use crate::policy::StorageCapacities;

/// Bounded buffer of objects awaiting preparation.
#[derive(Debug)]
pub struct PendingBatch {
    objects: Vec<FinalizedObject>,
    canonical_bytes: u64,
    object_limit: usize,
    byte_limit: u64,
}

impl PendingBatch {
    /// Empty batch under the declared capacities.
    pub fn new(capacities: StorageCapacities) -> Self {
        Self {
            objects: Vec::new(),
            canonical_bytes: 0,
            object_limit: capacities.batch_objects,
            byte_limit: capacities.batch_bytes,
        }
    }

    /// Objects currently held.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Canonical bytes currently held.
    pub fn canonical_bytes(&self) -> u64 {
        self.canonical_bytes
    }

    /// Accepts one object, returning the drained wave when a bound is reached.
    ///
    /// An object larger than the whole byte bound is still accepted into an empty
    /// batch: the declared singleton path must remain usable, and the producer
    /// pays for it because nothing else is held at that point.
    pub fn push(&mut self, object: FinalizedObject) -> StorageResult<Option<Vec<FinalizedObject>>> {
        let length = object.canonical_len() as u64;
        if !self.objects.is_empty()
            && (self.objects.len() >= self.object_limit
                || self.canonical_bytes.saturating_add(length) > self.byte_limit)
        {
            let drained = self.drain();
            self.canonical_bytes = length;
            self.objects.push(object);
            return Ok(Some(drained));
        }
        self.canonical_bytes =
            self.canonical_bytes
                .checked_add(length)
                .ok_or(StorageError::CapacityExceeded {
                    what: "batch.canonical_bytes",
                    limit: self.byte_limit,
                    actual: u64::MAX,
                })?;
        self.objects.push(object);
        Ok(None)
    }

    /// Takes every held object, releasing their allocations to the caller.
    pub fn drain(&mut self) -> Vec<FinalizedObject> {
        self.canonical_bytes = 0;
        std::mem::take(&mut self.objects)
    }

    /// Canonical bytes of the first held object with `id`, if it is still pending.
    ///
    /// A same-save read may resolve an identity that this operation has accepted
    /// but not yet prepared from this bounded state, without querying storage.
    pub fn pending_canonical(&self, id: ObjectId) -> Option<&[u8]> {
        self.objects
            .iter()
            .find(|object| object.id() == id)
            .map(FinalizedObject::canonical)
    }
}
