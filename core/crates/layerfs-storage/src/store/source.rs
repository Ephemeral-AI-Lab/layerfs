//! Engine-independent bounded physical input for the shared encoding algorithms.

use crate::error::StorageResult;
use crate::location::{ObjectLocation, SignatureRow, ValueGroupRow};
use layerfs_content::ObjectId;

/// Physical input and advisory-index persistence used by unchanged encoding.
pub trait Source {
    /// Legacy sources require physical allocation order; first-wins sources
    /// validate logical cycles instead because a winning locator can move forward.
    fn ordered_dependencies(&self) -> bool {
        true
    }
    /// Eligible location under the caller's retained ceiling.
    fn location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>>;
    /// Complete declared pack bytes, without capacity padding.
    fn pack_bytes(&self, pack_id: i64) -> StorageResult<Vec<u8>>;
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
    /// Advisory catalogue prefetch for one bounded pooled leaf, before value reads.
    /// Scalar sources deliberately do no prefetch; `value_group` remains the
    /// required lookup and missing values still fail during reconstruction.
    fn prepare_value_groups(&self, _ordinals: &[u32]) -> StorageResult<()> {
        Ok(())
    }
    /// Pooled catalogue row covering one ordinal.
    fn value_group(&self, ordinal: u32) -> StorageResult<Option<ValueGroupRow>>;
    /// Streams catalogue rows in ordinal order without an unbounded allocation.
    fn value_groups(
        &self,
        from: Option<u32>,
        visit: &mut dyn FnMut(ValueGroupRow) -> StorageResult<()>,
    ) -> StorageResult<()>;
    /// First ordinal in the retained candidate window.
    fn window_start(&self) -> StorageResult<u32>;
    /// At most 8,192 persisted signatures, in insertion order.
    fn signatures(&self) -> StorageResult<Vec<SignatureRow>>;
    /// Records a real cache consult for operation-count diagnostics.
    fn note_pack_cache_hit(&self) {}
    /// Records a full-directory validation attempt and its declared entry bound.
    /// `groups` counts the single-group walks the same cohort would require.
    fn note_pack_directory_validation(&self, _groups: usize, _entries: usize) {}
    /// Records capacity eviction of real acquired bodies, never private invalidation.
    fn note_pack_evictions(&self, _entries: u64, _bytes: u64) {}
    /// Stores the bounded signature changes in the caller's transaction.
    fn write_signatures(&self, rows: &[SignatureRow]) -> StorageResult<usize>;
}
