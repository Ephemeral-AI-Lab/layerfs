//! Exact reuse and collision decisions.
//!
//! Membership says an identity is present; it never proves equality. Every
//! occurrence that finds an existing row compares the stored canonical bytes with
//! the offered bytes and reuses the row only when they are identical. A mismatch
//! is a collision and fails the save; it is not repaired, replaced or retried.

use layerfs_content::{FinalizedObject, ObjectId};

use crate::cas::owner::MutationOwner;
use crate::error::{StorageError, StorageResult};
use crate::sqlite::lookup::ObjectLocation;

/// Reads the stored canonical bytes at one locator, reconstructing its chain.
pub fn stored_canonical(
    owner: &mut MutationOwner,
    location: ObjectLocation,
) -> StorageResult<Vec<u8>> {
    let canonical = owner.resolve_location(location)?;
    if ObjectId::for_bytes(&canonical) != location.object_id {
        return Err(StorageError::Integrity("stored object identity"));
    }
    Ok(canonical)
}

/// Reuses the existing row when the stored bytes match the offered bytes exactly.
///
/// A shorter, longer or different record under the same identity is a collision
/// and fails the save; it is never repaired, replaced or written again in place.
pub fn reuse_or_collide(
    owner: &mut MutationOwner,
    object: &FinalizedObject,
    location: ObjectLocation,
) -> StorageResult<()> {
    if location.object_id != object.id() {
        return Err(StorageError::Integrity("membership identity"));
    }
    if location.canonical_length != object.canonical_len() {
        return Err(StorageError::Collision(object.id()));
    }
    let stored = stored_canonical(owner, location)?;
    if stored == object.canonical() {
        Ok(())
    } else {
        Err(StorageError::Collision(object.id()))
    }
}
