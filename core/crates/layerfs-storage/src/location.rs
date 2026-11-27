//! Engine-independent physical locations, stored value rows and the bounded
//! physical input (`Source`) the shared encoding algorithms read through.

use crate::error::StorageResult;
use layerfs_content::inode_leaf::INODE_VALUE_BYTES;
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
    fn validate_cached_pack(&self, _info: PackInfo) -> StorageResult<()> {
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
    /// The walk decoded `base` from `child`'s record and is about to locate
    /// it: locates it together with the deeper bases `child`'s recorded
    /// chain lets cached bytes name (#768), for a walk whose chain members
    /// are not bound. Default: nothing; the walk locates level by level.
    fn locate_ahead(
        &self,
        _child: &ObjectLocation,
        _base: ObjectId,
        _packs: &crate::encoding::PackCache,
        _workspace: &mut crate::encoding::codec::DecompressionWorkspace,
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
