#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InodeId(pub [u8; 32]);

impl InodeId {
    pub fn allocate(store_id: [u8; 32], serial: u64) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/inode-id/v1\0");
        hasher.update(&store_id);
        hasher.update(&serial.to_be_bytes());
        Self(*hasher.finalize().as_bytes())
    }

    pub fn from_slice(bytes: &[u8]) -> CoreResult<Self> {
        Ok(Self(ObjectId::from_bytes(bytes)?.to_bytes()))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum InodeKind {
    RegularFile = 1,
    Directory = 2,
    Symlink = 3,
}

impl TryFrom<u8> for InodeKind {
    type Error = crate::CoreError;
    fn try_from(value: u8) -> CoreResult<Self> {
        match value {
            1 => Ok(Self::RegularFile),
            2 => Ok(Self::Directory),
            3 => Ok(Self::Symlink),
            _ => Err(crate::CoreError::InvalidRecord("inode kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeRecordV1 {
    pub kind: InodeKind,
    pub namespace_ref_count: u64,
    pub content_root: ObjectId,
    pub metadata_root: ObjectId,
}

impl InodeRecordV1 {
    pub fn validate(self, is_root: bool) -> CoreResult<()> {
        let valid = if is_root {
            self.kind == InodeKind::Directory && self.namespace_ref_count == 0
        } else {
            self.namespace_ref_count >= 1
                && (self.kind == InodeKind::RegularFile || self.namespace_ref_count == 1)
        };
        if valid {
            Ok(())
        } else {
            Err(crate::CoreError::InvalidRecord("namespace ref count"))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeTableRoot(pub ObjectId);

pub struct GeneratedInodeTable(InodeTableRoot);

impl GeneratedInodeTable {
    pub const fn root(&self) -> InodeTableRoot {
        self.0
    }

    pub const fn into_root(self) -> InodeTableRoot {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InodeTableCounters {
    pub nodes_read: u64,
    pub nodes_created: u64,
}

#[derive(Clone, Copy)]
struct Summary {
    id: ObjectId,
    min: InodeId,
    max: InodeId,
    entries: u64,
    level: u8,
}

struct ValidatedNode {
    node: InodeTableNodeV1,
    summary: Summary,
}
