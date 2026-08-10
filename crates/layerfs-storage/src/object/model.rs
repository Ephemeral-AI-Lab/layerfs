//! Backend-neutral canonical object model and typed strong-edge identities.

use crate::format::PhysicalObjectKindV1;
use crate::identity::{
    derive_physical_chunk_id_v1, derive_physical_file_id_v1, derive_physical_symlink_id_v1,
    derive_physical_tree_id_v1, derive_physical_version_record_id_v1, ChunkerSpecId, DigestSpecId,
    PhysicalChunkIdV1, PhysicalFileIdV1, PhysicalSymlinkIdV1, PhysicalTreeIdV1,
    PhysicalVersionRecordIdV1, ProfileId, VersionIdV1,
};
use crate::CoreResult;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalObjectHeaderV1 {
    pub(super) kind: PhysicalObjectKindV1,
    pub(super) profile_id: ProfileId,
    pub(super) payload_len: u64,
    pub(super) complete_len: u64,
}

impl PhysicalObjectHeaderV1 {
    pub const fn kind(self) -> PhysicalObjectKindV1 {
        self.kind
    }

    pub const fn profile_id(self) -> ProfileId {
        self.profile_id
    }

    pub const fn payload_len(self) -> u64 {
        self.payload_len
    }

    pub const fn complete_len(self) -> u64 {
        self.complete_len
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VersionRecordV1 {
    pub version_id: VersionIdV1,
    pub chunker_spec_id: ChunkerSpecId,
    pub digest_spec_id: DigestSpecId,
    pub root_tree_id: PhysicalTreeIdV1,
    pub canonical_len: u64,
    pub logical_file_bytes: u64,
    pub entry_count: u32,
    pub tree_count: u32,
    pub file_count: u32,
    pub symlink_count: u32,
    pub chunk_count: u32,
    pub extent_count: u32,
    pub chunk_ref_count: u32,
    pub total_object_count: u32,
    pub physical_chunk_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryTreeRecordV1 {
    pub mode: u16,
    pub entry_count: u32,
    pub page_depth: u8,
    pub root_page_id: Option<PhysicalTreeIdV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeafTreeRecordV1 {
    pub depth: u8,
    pub count: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexTreeRecordV1 {
    pub depth: u8,
    pub count: u16,
    pub subtree_entry_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeRecordV1 {
    Directory(DirectoryTreeRecordV1),
    Leaf(LeafTreeRecordV1),
    Index(IndexTreeRecordV1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileRecordV1 {
    pub mode: u16,
    pub logical_len: u64,
    pub extent_count: u32,
    pub chunk_ref_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SymlinkRecordV1 {
    pub target_len: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkRecordV1 {
    pub payload_len: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalObjectPayloadV1 {
    VersionRecord(VersionRecordV1),
    Tree(TreeRecordV1),
    File(FileRecordV1),
    Symlink(SymlinkRecordV1),
    Chunk(ChunkRecordV1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedPhysicalObjectV1<'a> {
    pub(super) header: PhysicalObjectHeaderV1,
    pub(super) payload: PhysicalObjectPayloadV1,
    pub(super) canonical_object: &'a [u8],
}

impl<'a> ValidatedPhysicalObjectV1<'a> {
    pub const fn header(&self) -> PhysicalObjectHeaderV1 {
        self.header
    }

    pub const fn payload(&self) -> PhysicalObjectPayloadV1 {
        self.payload
    }

    pub const fn canonical_bytes(&self) -> &'a [u8] {
        self.canonical_object
    }

    pub fn physical_id(&self) -> CoreResult<TypedPhysicalObjectIdV1> {
        match self.payload {
            PhysicalObjectPayloadV1::VersionRecord(_) => {
                derive_physical_version_record_id_v1(self.canonical_object)
                    .map(TypedPhysicalObjectIdV1::VersionRecord)
            }
            PhysicalObjectPayloadV1::Tree(_) => {
                derive_physical_tree_id_v1(self.canonical_object).map(TypedPhysicalObjectIdV1::Tree)
            }
            PhysicalObjectPayloadV1::File(_) => {
                derive_physical_file_id_v1(self.canonical_object).map(TypedPhysicalObjectIdV1::File)
            }
            PhysicalObjectPayloadV1::Symlink(_) => {
                derive_physical_symlink_id_v1(self.canonical_object)
                    .map(TypedPhysicalObjectIdV1::Symlink)
            }
            PhysicalObjectPayloadV1::Chunk(_) => derive_physical_chunk_id_v1(self.canonical_object)
                .map(TypedPhysicalObjectIdV1::Chunk),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TypedPhysicalObjectIdV1 {
    VersionRecord(PhysicalVersionRecordIdV1),
    Tree(PhysicalTreeIdV1),
    File(PhysicalFileIdV1),
    Symlink(PhysicalSymlinkIdV1),
    Chunk(PhysicalChunkIdV1),
}

impl TypedPhysicalObjectIdV1 {
    pub const fn kind(self) -> PhysicalObjectKindV1 {
        match self {
            Self::VersionRecord(_) => PhysicalObjectKindV1::VersionRecord,
            Self::Tree(_) => PhysicalObjectKindV1::Tree,
            Self::File(_) => PhysicalObjectKindV1::File,
            Self::Symlink(_) => PhysicalObjectKindV1::Symlink,
            Self::Chunk(_) => PhysicalObjectKindV1::Chunk,
        }
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        match self {
            Self::VersionRecord(id) => id.as_bytes(),
            Self::Tree(id) => id.as_bytes(),
            Self::File(id) => id.as_bytes(),
            Self::Symlink(id) => id.as_bytes(),
            Self::Chunk(id) => id.as_bytes(),
        }
    }

    pub(crate) const fn from_kind_and_digest(kind: PhysicalObjectKindV1, digest: [u8; 32]) -> Self {
        match kind {
            PhysicalObjectKindV1::VersionRecord => {
                Self::VersionRecord(PhysicalVersionRecordIdV1::from_digest(digest))
            }
            PhysicalObjectKindV1::Tree => Self::Tree(PhysicalTreeIdV1::from_digest(digest)),
            PhysicalObjectKindV1::File => Self::File(PhysicalFileIdV1::from_digest(digest)),
            PhysicalObjectKindV1::Symlink => {
                Self::Symlink(PhysicalSymlinkIdV1::from_digest(digest))
            }
            PhysicalObjectKindV1::Chunk => Self::Chunk(PhysicalChunkIdV1::from_digest(digest)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrongEdgeV1 {
    Tree(PhysicalTreeIdV1),
    File(PhysicalFileIdV1),
    Symlink(PhysicalSymlinkIdV1),
    Chunk(PhysicalChunkIdV1),
}

impl StrongEdgeV1 {
    pub const fn typed_id(self) -> TypedPhysicalObjectIdV1 {
        match self {
            Self::Tree(id) => TypedPhysicalObjectIdV1::Tree(id),
            Self::File(id) => TypedPhysicalObjectIdV1::File(id),
            Self::Symlink(id) => TypedPhysicalObjectIdV1::Symlink(id),
            Self::Chunk(id) => TypedPhysicalObjectIdV1::Chunk(id),
        }
    }
}

/// Transactional edge delivery. An implementation may buffer only within its
/// own independently bounded policy; decoders themselves retain no edges.
pub trait StrongEdgeVisitorV1 {
    fn begin_object(&mut self);
    fn visit_edge(&mut self, edge: StrongEdgeV1) -> CoreResult<()>;
    fn commit_object(&mut self);
    fn abort_object(&mut self);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DiscardStrongEdgesV1;

impl StrongEdgeVisitorV1 for DiscardStrongEdgesV1 {
    fn begin_object(&mut self) {}
    fn visit_edge(&mut self, _edge: StrongEdgeV1) -> CoreResult<()> {
        Ok(())
    }
    fn commit_object(&mut self) {}
    fn abort_object(&mut self) {}
}
