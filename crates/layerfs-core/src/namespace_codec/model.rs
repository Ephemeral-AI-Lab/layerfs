const VERSION: u16 = 1;
const NODE_LIMIT: usize = 8192;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryNodeV1 {
    Leaf {
        subtree_encoded_bytes: u64,
        entries: Vec<(CanonicalName, InodeId)>,
    },
    Branch {
        level: u8,
        subtree_entry_count: u64,
        subtree_encoded_bytes: u64,
        children: Vec<(CanonicalName, ObjectId)>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InodeTableNodeV1 {
    Leaf(Vec<(InodeId, ObjectId)>),
    Branch {
        level: u8,
        subtree_entry_count: u64,
        children: Vec<(InodeId, ObjectId)>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataNodeV1 {
    Leaf {
        subtree_encoded_bytes: u64,
        entries: Vec<MetadataEntryV1>,
    },
    Branch {
        level: u8,
        subtree_entry_count: u64,
        subtree_encoded_bytes: u64,
        children: Vec<(MetadataKey, ObjectId)>,
    },
}
