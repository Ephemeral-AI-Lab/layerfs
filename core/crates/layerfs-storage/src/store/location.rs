//! Engine-independent physical locations and catalogue rows.

use layerfs_content::{ObjectId, ObjectRole};

/// One stored object's descriptor and physical location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectLocation {
    /// Canonical identity.
    pub object_id: ObjectId,
    /// Logical role code.
    pub role: ObjectRole,
    /// Canonical object length recorded at save time.
    pub canonical_length: usize,
    /// Pack holding the record.
    pub pack_id: i64,
    /// Group ordinal inside the pack.
    pub group_number: usize,
    /// Record ordinal inside the group.
    pub record_number: usize,
}

/// One catalogue row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueGroupRow {
    /// First ordinal the group covers.
    pub first_ordinal: u32,
    /// Number of values in the group.
    pub count: usize,
    /// Pack holding the group.
    pub pack_id: i64,
    /// Group ordinal inside the pack.
    pub group_number: usize,
    /// Digest of the decoded group body.
    pub digest: ObjectId,
}

/// One persisted advisory content-signature ring entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignatureRow {
    /// Ring slot overwritten by this entry.
    pub slot: usize,
    /// One-based insertion order.
    pub stamp: u64,
    /// Admitted canonical object identity.
    pub object_id: ObjectId,
    /// Eight folded hashes in their existing little-endian representation.
    pub signature: [u8; 32],
}

/// Physical body destination, determined by its lane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackDomain {
    /// Mapping and pooled metadata bytes.
    Metadata,
    /// File-content bytes.
    Payload,
}

impl PackDomain {
    /// Routes a lane without consulting an engine.
    pub const fn for_lane(lane: crate::pack::layout::PackLane) -> Self {
        use crate::pack::layout::PackLane;
        match lane {
            PackLane::Ordinary | PackLane::PooledMetadata => Self::Metadata,
            PackLane::Native | PackLane::WholeFile | PackLane::Singleton => Self::Payload,
        }
    }
}

/// Persisted descriptor of a sealed immutable pack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackInfo {
    /// Positive pack id, independent of content digest.
    pub pack_id: i64,
    /// Destination dictated by the lane.
    pub domain: PackDomain,
    /// SHA-256 of the complete sealed bytes.
    pub key: crate::port::ObjectKey,
    /// Exact sealed length, without capacity padding.
    pub length: usize,
}

/// One object location together with the descriptor needed to fetch its pack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocatedObject {
    /// Canonical object descriptor and position.
    pub location: ObjectLocation,
    /// Immutable body descriptor.
    pub pack: PackInfo,
}
