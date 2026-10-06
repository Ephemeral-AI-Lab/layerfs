//! Neutral full-identity records; Content owns the meaning of every raw value.
use crate::{OperationOwner, OverlayError, OverlayResult, SCRATCH_BYTES};

/// One file construction scope under an actual independently retained owner.
/// The file scope is opaque; all u64 values retain their complete identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexedScope {
    pub owner: OperationOwner,
    pub file_scope: u64,
}

/// A complete raw record key. Kind ordering precedes the full binary identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct IndexedKey {
    pub kind: u32,
    pub key: [u8; 32],
}

/// Exact pre-state of one raw record, checked before any batch mutation.
#[derive(Debug, Eq, PartialEq)]
pub enum ExpectedValue {
    Missing,
    ExactBytes(Vec<u8>),
}

/// One guarded raw change. None deletes; an empty BLOB is a valid marker.
#[derive(Debug, Eq, PartialEq)]
pub struct IndexedChange {
    pub key: IndexedKey,
    pub expected: ExpectedValue,
    pub value: Option<Vec<u8>>,
}

/// One original atomic result. NotApplied changed no records and is not
/// permission to refetch or repeat the failed construction operation.
#[derive(Debug, Eq, PartialEq)]
pub enum IndexedApply {
    Applied,
    NotApplied {
        index: usize,
        key: IndexedKey,
        actual: Option<Vec<u8>>,
    },
}

/// Complete retained descriptors and nested value capacities of a borrowed
/// batch. Its targets must already be strictly ordered and unique. An owning
/// caller also accounts for unused capacity of its outer changes vector.
pub fn indexed_changes_bytes(changes: &[IndexedChange]) -> OverlayResult<usize> {
    let mut bytes = changes
        .len()
        .checked_mul(std::mem::size_of::<IndexedChange>())
        .ok_or(OverlayError::Invalid("indexed scratch byte window"))?;
    let mut previous = None;
    for change in changes {
        if previous.is_some_and(|key| key >= change.key) {
            return Err(OverlayError::Invalid("indexed scratch target order"));
        }
        previous = Some(change.key);
        let expected = match &change.expected {
            ExpectedValue::Missing => 0,
            ExpectedValue::ExactBytes(value) => value.capacity(),
        };
        bytes = bytes
            .checked_add(expected)
            .and_then(|n| n.checked_add(change.value.as_ref().map_or(0, Vec::capacity)))
            .ok_or(OverlayError::Invalid("indexed scratch byte window"))?;
        if bytes > SCRATCH_BYTES {
            return Err(OverlayError::Invalid("indexed scratch byte window"));
        }
    }
    if bytes > SCRATCH_BYTES {
        return Err(OverlayError::Invalid("indexed scratch byte window"));
    }
    Ok(bytes)
}
