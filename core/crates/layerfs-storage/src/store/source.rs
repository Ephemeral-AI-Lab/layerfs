//! Engine-independent bounded physical input for the shared encoding algorithms.

use crate::error::StorageResult;
use crate::location::{ObjectLocation, SignatureRow};
use layerfs_content::{inode_leaf::INODE_VALUE_BYTES, ObjectId};

/// Physical input and advisory-index persistence used by unchanged encoding.
pub trait Source {
    /// Legacy sources require physical allocation order; first-wins sources
    /// validate logical cycles instead because a winning locator can move forward.
    fn ordered_dependencies(&self) -> bool {
        true
    }
    /// Eligible location under the caller's retained ceiling.
    fn location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>>;
    /// Location of a base that a stored record names. Such a base is always
    /// stored, so a source that caches negative answers re-asks here: its
    /// "absent" may predate another writer's publication of the base (F-036).
    fn base_location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        self.location(id, ceiling)
    }
    /// Complete declared pack bytes, without capacity padding.
    fn pack_bytes(&self, pack_id: i64) -> StorageResult<Vec<u8>>;
    /// The bounded control/directory prefix of one pack, with its declared
    /// length. A metadata-only scan reads this instead of the whole body; a
    /// source that cannot answer returns `None` and the caller reads the body.
    fn pack_prefix(&self, _pack_id: i64, _limit: usize) -> StorageResult<Option<(usize, Vec<u8>)>> {
        Ok(None)
    }
    /// Acquires the complete encoded groups needed by this bounded physical
    /// demand. `whole_for_reuse` requests whole materialization before I/O when
    /// this owner already retained another group. Custom sources use whole bodies.
    fn acquire_groups(
        &self,
        pack_id: i64,
        _groups: &[usize],
        _whole_for_reuse: bool,
    ) -> StorageResult<crate::encoding::PackAcquisition> {
        Ok(crate::encoding::PackAcquisition::Whole {
            info: None,
            body: self.pack_bytes(pack_id)?,
        })
    }
    /// Validates a retained descriptor against this Source's immutable view.
    fn validate_cached_pack(&self, _info: crate::location::PackInfo) -> StorageResult<()> {
        Ok(())
    }
    /// The stored inode value of every ordinal, in the order asked, for the
    /// rows of one leaf and of the chain it is stored against. An ordinal
    /// without a stored value is an integrity failure, never an omission.
    fn inode_values(&self, ordinals: &[u32]) -> StorageResult<Vec<[u8; INODE_VALUE_BYTES]>>;
    /// At most 8,192 persisted signatures, in insertion order.
    fn signatures(&self) -> StorageResult<Vec<SignatureRow>>;
    /// Records a real cache consult for operation-count diagnostics.
    fn note_pack_cache_hit(&self) {}
    /// Records a full-directory validation attempt and its declared entry bound.
    /// `groups` counts the single-group walks the same cohort would require.
    fn note_pack_directory_validation(&self, _groups: usize, _entries: usize) {}
    /// Records capacity eviction of real acquired bodies, never private invalidation.
    fn note_pack_evictions(&self, _entries: u64, _bytes: u64) {}
    /// Before a walk reads `location`'s group: fetches it together with the
    /// groups of its recorded delta chain (#758). Default: nothing.
    fn prefetch_chain(
        &self,
        _location: &ObjectLocation,
        _packs: &mut crate::encoding::PackCache,
    ) -> StorageResult<()> {
        Ok(())
    }
    /// The walk decoded `base` from `child`'s record: binds and checks the
    /// chain member there, if any (#758). Default: nothing.
    fn bind_base(&self, _child: &ObjectLocation, _base: Option<ObjectId>) -> StorageResult<()> {
        Ok(())
    }
    /// Whether `id`'s locator came from a chain hint (#758) and is not
    /// authoritative. Default: never.
    fn hinted(&self, _id: ObjectId) -> bool {
        false
    }
    /// After a failure a stale hint may have caused on the walk of `chain`
    /// (empty: any walk): drops every hint; true when a hinted locator was on
    /// it, so the caller rebuilds once.
    fn drop_hints(&self, _chain: &[ObjectId]) -> StorageResult<bool> {
        Ok(false)
    }
    /// Stores the bounded signature changes in the caller's transaction.
    fn write_signatures(&self, rows: &[SignatureRow]) -> StorageResult<usize>;
}
