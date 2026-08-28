use super::*;

const SEMANTIC_DIGEST_CACHE_LIMIT: usize = 4096;

pub(crate) trait CaptureStore: ObjectStore {
    fn allocate_inode_id(&mut self) -> VfsResult<InodeId>;
}

impl CaptureStore for WorkingCandidateWrite<'_> {
    fn allocate_inode_id(&mut self) -> VfsResult<InodeId> {
        WorkingCandidateWrite::allocate_inode_id(self)
            .map_err(|error| VfsError::Io(std::io::Error::other(error.to_string())))
    }
}

#[derive(Default)]
pub(crate) struct SemanticDigestCache(Mutex<HashMap<layerfs_core::ObjectId, [u8; 32]>>);

impl SemanticDigestCache {
    pub(super) fn get(&self, root: FileStateRoot) -> VfsResult<Option<[u8; 32]>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| VfsError::InvalidState)?
            .get(&root.0)
            .copied())
    }

    pub(super) fn insert(&self, root: FileStateRoot, digest: [u8; 32]) -> VfsResult<()> {
        let mut entries = self.0.lock().map_err(|_| VfsError::InvalidState)?;
        if entries.len() == SEMANTIC_DIGEST_CACHE_LIMIT && !entries.contains_key(&root.0) {
            // ponytail: wholesale eviction keeps Store-lifetime memory bounded; add LRU only if
            // measured capture workloads repeatedly exceed 4,096 distinct retained file roots.
            entries.clear();
        }
        entries.insert(root.0, digest);
        Ok(())
    }
}

pub(super) struct HardLink {
    pub(super) inode: InodeId,
    pub(super) record: InodeRecordV1,
    pub(super) expected: u64,
    pub(super) observed: u64,
}

impl HardLink {
    pub(super) fn encode(&self) -> [u8; 121] {
        let mut bytes = [0_u8; 121];
        bytes[..32].copy_from_slice(self.inode.as_bytes());
        bytes[32] = self.record.kind as u8;
        bytes[33..41].copy_from_slice(&self.record.namespace_ref_count.to_be_bytes());
        bytes[41..73].copy_from_slice(self.record.content_root.as_bytes());
        bytes[73..105].copy_from_slice(self.record.metadata_root.as_bytes());
        bytes[105..113].copy_from_slice(&self.expected.to_be_bytes());
        bytes[113..121].copy_from_slice(&self.observed.to_be_bytes());
        bytes
    }

    pub(super) fn decode(bytes: &[u8]) -> VfsResult<Self> {
        if bytes.len() != 121 {
            return Err(VfsError::InvalidState);
        }
        Ok(Self {
            inode: InodeId::from_slice(&bytes[..32])?,
            record: InodeRecordV1 {
                kind: InodeKind::try_from(bytes[32])?,
                namespace_ref_count: u64::from_be_bytes(bytes[33..41].try_into().unwrap()),
                content_root: layerfs_core::ObjectId::from_bytes(&bytes[41..73])?,
                metadata_root: layerfs_core::ObjectId::from_bytes(&bytes[73..105])?,
            },
            expected: u64::from_be_bytes(bytes[105..113].try_into().unwrap()),
            observed: u64::from_be_bytes(bytes[113..121].try_into().unwrap()),
        })
    }
}
