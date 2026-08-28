use layerfs_core::content::rope::build;
use layerfs_core::encode_bytes_object;
use layerfs_core::inode::{inode_table_from_root, InodeId, InodeKind, InodeRecordV1};
use layerfs_core::metadata::{build_metadata_tree, MetadataEntryV1, MetadataKey};
use layerfs_core::namespace::{empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{encode_inode_record, encode_namespace_root, profile_id};
use layerfs_core::object::access::ObjectStore;
use layerfs_core::{logical, CanonicalPath};
use layerfs_storage::integrity::IntegrityMode;
use layerfs_storage::{
    BranchHead, BranchId, BranchRollbackOutcome, ChildMergeCandidate, ChildMergeOutcome,
    LayerCandidateRequest, LayerId, LayerStackId, LayerStackMergeOutcome,
    LayerStackRollbackOutcome, LeaseId, OperationCandidate, OperationCommitOutcome, OperationId,
    OperationRecordRef, RecoverableOperationState, RequestId, VersionRef,
};
use layerfs_storage::{Engine, EngineError};
use rusqlite::Connection;
use std::fs;
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

mod candidate;
mod checkpoint;
mod lifecycle;
mod receipt;
#[cfg(feature = "test-hooks")]
mod reconciliation;
mod replay_identity;

fn valid_empty_root(engine: &Engine) -> layerfs_core::ObjectId {
    valid_empty_root_with_seed(engine, [0x18; 32])
}

struct PatternReader {
    remaining: u64,
    state: u64,
}

impl Read for PatternReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = output.len().min(self.remaining as usize);
        for byte in &mut output[..count] {
            self.state ^= self.state << 13;
            self.state ^= self.state >> 7;
            self.state ^= self.state << 17;
            *byte = self.state as u8;
        }
        self.remaining -= count as u64;
        Ok(count)
    }
}

fn valid_empty_root_with_seed(engine: &Engine, seed: [u8; 32]) -> layerfs_core::ObjectId {
    let mut publication = engine.begin_candidate_write().unwrap();
    let (mode, _) = build(&mut publication, 0o755_u32.to_be_bytes().as_slice()).unwrap();
    let mut mtime = Vec::new();
    mtime.extend_from_slice(&0_i64.to_be_bytes());
    mtime.extend_from_slice(&0_u32.to_be_bytes());
    let (mtime, _) = build(&mut publication, mtime.as_slice()).unwrap();
    let metadata = build_metadata_tree(
        &mut publication,
        &[
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_file_root: mode.0,
            },
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_file_root: mtime.0,
            },
        ],
    )
    .unwrap();
    let directory = empty_directory(&mut publication).unwrap();
    let inode = InodeId::allocate(seed, 0);
    let record = publication
        .put(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: directory.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let table = inode_table_from_root(&mut publication, inode, record).unwrap();
    let root = publication
        .put(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: inode,
                inode_table_root: table.0,
            })
            .unwrap(),
        )
        .unwrap();
    publication.commit_candidate(root).unwrap()
}
