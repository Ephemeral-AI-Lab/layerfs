use crate::tree::inode::InodeId;
use crate::{CanonicalName, CoreError, CoreResult, ObjectId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryStateV1 {
    pub entry_count: u64,
    pub tree_level: u8,
    pub profile_id: ObjectId,
    pub mapping_root: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymlinkStateV1 {
    pub target: Vec<u8>,
}

impl SymlinkStateV1 {
    pub fn new(target: Vec<u8>) -> CoreResult<Self> {
        if target.len() > 4096 || target.contains(&0) {
            return Err(CoreError::InvalidRecord("symlink target"));
        }
        Ok(Self { target })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryStateRoot(pub ObjectId);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NamespaceCounters {
    pub nodes_read: u64,
    pub nodes_created: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryPage {
    pub entries: Vec<(CanonicalName, InodeId)>,
    pub continuation: Option<CanonicalName>,
}

#[derive(Clone)]
pub(super) struct NodeSummary {
    pub(super) id: ObjectId,
    pub(super) min: Option<CanonicalName>,
    pub(super) max: Option<CanonicalName>,
    pub(super) entries: u64,
    pub(super) encoded_bytes: u64,
    pub(super) level: u8,
}

pub(super) struct ValidatedNode {
    pub(super) node: super::codec::DirectoryNodeV1,
    pub(super) summary: NodeSummary,
}
