//! Internal encoding access seam while the two storage paths coexist.
//!
//! This seam carries physical bytes and catalogue rows, never engine types.
//! The old path retains its publication ceiling and transaction ownership.

use crate::error::{StorageError, StorageResult};
use crate::location::{ObjectLocation, SignatureRow, ValueGroupRow};
use layerfs_content::ObjectId;
use std::sync::{Mutex, MutexGuard};

/// Physical input and advisory-index persistence used by unchanged encoding.
pub trait Source {
    /// Eligible location under the caller's retained ceiling.
    fn location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>>;
    /// Complete declared pack bytes, without capacity padding.
    fn pack_bytes(&self, pack_id: i64) -> StorageResult<Vec<u8>>;
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
    /// Stores the bounded signature changes in the caller's transaction.
    fn write_signatures(&self, rows: &[SignatureRow]) -> StorageResult<usize>;
}

/// Takes the legacy caller's arbitration once, preserving its wave ownership.
pub(crate) fn lock_unless_held(
    owner: &Mutex<()>,
    held: bool,
) -> StorageResult<Option<MutexGuard<'_, ()>>> {
    if held {
        return Ok(None);
    }
    Ok(Some(owner.lock().map_err(|_| {
        StorageError::Integrity("Store arbitration")
    })?))
}
