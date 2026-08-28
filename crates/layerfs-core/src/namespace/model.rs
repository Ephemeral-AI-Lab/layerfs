#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NamespaceRootV1 {
    pub profile_id: ObjectId,
    pub root_directory_inode: InodeId,
    pub inode_table_root: ObjectId,
}

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
struct NodeSummary {
    id: ObjectId,
    min: Option<CanonicalName>,
    max: Option<CanonicalName>,
    entries: u64,
    encoded_bytes: u64,
    level: u8,
}

struct ValidatedNode {
    node: DirectoryNodeV1,
    summary: NodeSummary,
}
