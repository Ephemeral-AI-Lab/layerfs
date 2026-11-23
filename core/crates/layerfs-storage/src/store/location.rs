//! Engine-independent physical locations and stored value rows.

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
    /// The direct delta base the record names, `None` when it is FULL (#758).
    /// A publish input: the provider derives what it stores from it. Rows
    /// read back carry `None` unless the provider stores base ids.
    pub base: Option<ObjectId>,
}

/// One stored inode value: the row a pooled leaf names by its ordinal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeValueRow {
    /// The ordinal, assigned once from a Save's reserved block.
    pub ordinal: u32,
    /// The 73 value bytes of one inode-leaf row.
    pub value: [u8; layerfs_content::inode_leaf::INODE_VALUE_BYTES],
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
            PackLane::Ordinary => Self::Metadata,
            PackLane::Chunk | PackLane::WholeFile | PackLane::Singleton => Self::Payload,
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

/// One member of a located delta chain, nearest base first (#758). It is a
/// hint about where a base is stored: the reader binds it to the base the
/// child record names and verifies it before use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChainMember {
    /// The member's id when the provider stores ids; `None` from a physical
    /// list, where the reader binds the id the child record names.
    pub object_id: Option<ObjectId>,
    /// The pack holding the member's record.
    pub pack: PackInfo,
    /// Group ordinal inside the pack.
    pub group_number: usize,
    /// Record ordinal inside the group.
    pub record_number: usize,
    /// Canonical length of the member object.
    pub canonical_length: usize,
}

/// One located delta root's chain: every member down to its FULL record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocatedChain {
    /// A root the caller asked to locate.
    pub root: ObjectId,
    /// Members, nearest base first; the last one is the FULL record.
    pub members: Vec<ChainMember>,
}

/// A pack's control header and group directory, `body[0..body_offset)`.
/// Untrusted: the reader parses it against the pack length before use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackDirectory {
    /// The pack these bytes open.
    pub pack: PackInfo,
    /// The pack's first `body_offset` bytes.
    pub prefix: Vec<u8>,
}

/// What a chain locate returns beside the located rows.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LocatedChains {
    /// At most one chain per located root.
    pub chains: Vec<LocatedChain>,
    /// Directories of packs the rows or members name, where known.
    pub directories: Vec<PackDirectory>,
}
