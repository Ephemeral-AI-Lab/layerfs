//! Provider-neutral access to physically placed C2 records.
//!
//! An implementation captures one publication/private-owner scope and one logical
//! domain. Lookup must enforce that scope, required domain placement and logical
//! reference use. Body orders are globally unique across domains and immutable
//! for every cache lifetime; private snapshots require a new order when their
//! generation changes. Ordinals likewise name one immutable global value group.
//! These identities permit the existing bounded pack/group caches to be shared.
//! Legacy SQLite writers retain their explicit cache invalidation on append.

use layerfs_content::ObjectId;
use rusqlite::Connection;

use crate::error::{StorageError, StorageResult};
use crate::sqlite::{lookup, pool};

pub use crate::sqlite::lookup::ObjectLocation;
pub use crate::sqlite::pool::ValueGroupRow;

/// Captured catalog eligibility plus complete, authenticated physical bodies.
///
/// Metadata pack bytes retain the complete C2 header/directory/frame envelope.
/// A separate encoded value group must use its descriptor and decoder, and must
/// never be returned here disguised as a pack. Acquired I/O/integrity failures
/// are terminal: implementations do not probe an alternate backend or retry.
pub trait PackAccess {
    /// Whether this legacy access requires the selector's SQLite arbitration.
    /// Provider-neutral adapters serialize their operation separately and return false.
    fn requires_arbitration(&self) -> bool {
        false
    }

    /// Selects the first valid locator in this access's domain and captured scope.
    fn location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>>;

    /// Reads complete pack bytes for a qualified immutable body order.
    fn pack_bytes(&self, pack_id: i64) -> StorageResult<Vec<u8>>;

    /// Returns the eligible immutable group covering this ordinal.
    fn group_for(&self, ordinal: u32) -> StorageResult<Option<ValueGroupRow>>;

    /// First ordinal of the retained metadata candidate window.
    fn metadata_window_start(&self) -> StorageResult<u32>;

    /// Materializes one bounded keyset page, releasing SQL statements before return.
    /// The maximum page width is the existing 128-descriptor lookup bound.
    fn group_page(&self, from: u32, limit: usize) -> StorageResult<Vec<ValueGroupRow>>;

    /// Revalidates a supplied locator before any body or decoded-cache answer.
    fn authorize_location(&self, location: &ObjectLocation, ceiling: i64) -> StorageResult<()> {
        check_ceiling(location.pack_id, ceiling)?;
        match self.location(location.object_id, ceiling)? {
            Some(selected) if selected == *location => Ok(()),
            _ => Err(StorageError::Integrity("selected locator eligibility")),
        }
    }

    /// Revalidates covering group custody before a pooled-value cache answer.
    fn authorize_group(&self, row: &ValueGroupRow, ceiling: i64) -> StorageResult<()> {
        check_ceiling(row.pack_id, ceiling)?;
        match self.group_for(row.first_ordinal)? {
            Some(selected) if selected == *row => Ok(()),
            _ => Err(StorageError::Integrity("selected value group eligibility")),
        }
    }
}

fn check_ceiling(pack_id: i64, ceiling: i64) -> StorageResult<()> {
    if pack_id > ceiling {
        return Err(StorageError::VisibilityCeiling { pack_id, ceiling });
    }
    Ok(())
}

impl PackAccess for Connection {
    fn requires_arbitration(&self) -> bool {
        true
    }

    fn location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        lookup::location(self, id, ceiling)
    }

    fn pack_bytes(&self, pack_id: i64) -> StorageResult<Vec<u8>> {
        lookup::pack_bytes(self, pack_id)
    }

    fn group_for(&self, ordinal: u32) -> StorageResult<Option<ValueGroupRow>> {
        pool::group_for(self, ordinal)
    }

    fn metadata_window_start(&self) -> StorageResult<u32> {
        pool::window_start(self)
    }

    fn group_page(&self, from: u32, limit: usize) -> StorageResult<Vec<ValueGroupRow>> {
        pool::group_page(self, from, limit)
    }

    fn authorize_location(&self, location: &ObjectLocation, ceiling: i64) -> StorageResult<()> {
        check_ceiling(location.pack_id, ceiling)?;
        // Collision validation deliberately reads each eligible duplicate, rather
        // than only the selected one. Membership remains bounded by save slots.
        if lookup::candidates(self, &[location.object_id], ceiling)?
            .iter()
            .any(|(candidate, _, eligible)| *eligible && candidate == location)
        {
            Ok(())
        } else {
            Err(StorageError::Integrity("locator eligibility"))
        }
    }
}
