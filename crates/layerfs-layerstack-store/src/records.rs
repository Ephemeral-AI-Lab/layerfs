use crate::ids::TypedId;
use crate::{BranchId, CommitId, LayerId, LayerStackId, Result, StoreError};
use layerfs_content::ObjectId;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityName(String);

impl EntityName {
    pub const MAX_LEN: usize = 63;

    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        Self::validate(&value)?;
        Ok(Self(value))
    }

    pub fn validate(value: &str) -> Result<()> {
        let bytes = value.as_bytes();
        let valid_edge = |byte: &u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
        if bytes.is_empty()
            || bytes.len() > Self::MAX_LEN
            || !bytes.first().is_some_and(valid_edge)
            || !bytes.last().is_some_and(valid_edge)
            || !bytes.iter().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
        {
            return Err(StoreError::InvalidInput("entity name"));
        }
        Ok(())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for EntityName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for EntityName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for EntityName {
    type Err = StoreError;

    fn from_str(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<String> for EntityName {
    type Error = StoreError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<&str> for EntityName {
    type Error = StoreError;

    fn try_from(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerStackRecord {
    pub id: LayerStackId,
    pub name: EntityName,
    pub head_layer_id: LayerId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayerRecord {
    pub id: LayerId,
    pub layer_stack_id: LayerStackId,
    pub parent_layer_id: Option<LayerId>,
    pub root_id: ObjectId,
    pub source_branch_id: Option<BranchId>,
    pub source_commit_id: Option<CommitId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitRecord {
    pub id: CommitId,
    pub root_id: ObjectId,
    pub parent_commit_id: Option<CommitId>,
    pub base_layer_id: LayerId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchRecord {
    pub id: BranchId,
    pub layer_stack_id: LayerStackId,
    pub name: EntityName,
    pub base_layer_id: LayerId,
    pub head_commit_id: Option<CommitId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceStage {
    pub workspace_id: [u8; 16],
    pub branch_id: BranchId,
    pub root_id: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LayerStackInitialization {
    Empty,
    Directory(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalForkSource {
    Layer {
        layer_id: LayerId,
    },
    Branch {
        branch_id: BranchId,
        commit_id: CommitId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffRequest {
    BranchCommits {
        branch_id: BranchId,
        from_commit_id: CommitId,
        to_commit_id: CommitId,
    },
    BranchLayer {
        branch_id: BranchId,
        layer_id: LayerId,
    },
    Layers {
        from_layer_id: LayerId,
        to_layer_id: LayerId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitializeLayerStackResult {
    pub layer_stack_id: LayerStackId,
    pub genesis_layer_id: LayerId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddLayerResult {
    Added { layer_id: LayerId },
    UpToDate { layer_id: LayerId },
    NoChanges { head_layer_id: LayerId },
    HeadMoved { expected: LayerId, actual: LayerId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page<T, I> {
    pub records: Vec<T>,
    pub continuation: Option<I>,
}

pub type LayerStackRecordPage = Page<LayerStackRecord, LayerStackId>;
pub type LayerRecordPage = Page<LayerRecord, LayerId>;
pub type BranchRecordPage = Page<BranchRecord, BranchId>;
pub type CommitRecordPage = Page<CommitRecord, CommitId>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoreCounts {
    pub objects: u64,
    pub commits: u64,
    pub branches: u64,
    pub layer_stacks: u64,
    pub layers: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalStorage {
    pub objects: u64,
    pub encoded_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoreStorageSnapshot {
    pub database_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceReadReceipt {
    pub snapshot_database_calls: u64,
    pub snapshot_database_rows: u64,
    pub snapshot_database_bytes: u64,
    pub snapshot_cache_hits: u64,
    pub snapshot_cache_rows: u64,
    pub snapshot_cache_bytes: u64,
    pub max_readahead_bytes: u64,
    pub init_capabilities: u64,
    pub kernel_read_requests: u64,
    pub kernel_read_bytes: u64,
    pub kernel_read_le_4k: u64,
    pub kernel_read_le_64k: u64,
    pub kernel_read_le_256k: u64,
    pub kernel_read_le_1m: u64,
    pub kernel_read_gt_1m: u64,
    pub read_ahead_hits: u64,
    pub read_ahead_misses: u64,
    pub read_ahead_fetches: u64,
    pub read_ahead_requested_bytes: u64,
    pub read_ahead_fetched_bytes: u64,
    pub read_ahead_served_bytes: u64,
    pub read_ahead_unused_bytes: u64,
    pub read_ahead_cache_copy_bytes: u64,
    pub host_response_frames: u64,
    pub host_response_bytes: u64,
    pub host_response_copy_bytes: u64,
    pub host_encode_ns: u64,
    pub host_socket_write_ns: u64,
    pub client_response_frames: u64,
    pub client_response_bytes: u64,
    pub client_socket_read_ns: u64,
    pub client_decode_ns: u64,
    pub client_decode_copy_bytes: u64,
    pub host_dispatch_ns: u64,
    pub workspace_read_calls: u64,
    pub workspace_requested_bytes: u64,
    pub workspace_output_bytes: u64,
    pub workspace_read_ns: u64,
    pub read_plan_builds: u64,
    pub rope_nodes_read: u64,
    pub payload_ids: u64,
    pub payload_batches: u64,
    pub max_payload_batch: u64,
    pub payload_bytes_read: u64,
    pub local_calls: u64,
    pub local_ids: u64,
    pub local_rows: u64,
    pub local_bytes: u64,
    pub local_read_auth_ns: u64,
    pub collection_ns: u64,
    pub callback_lookup: u64,
    pub callback_getattr: u64,
    pub callback_setattr: u64,
    pub callback_readlink: u64,
    pub callback_mknod: u64,
    pub callback_mkdir: u64,
    pub callback_unlink: u64,
    pub callback_rmdir: u64,
    pub callback_symlink: u64,
    pub callback_rename: u64,
    pub callback_link: u64,
    pub callback_open: u64,
    pub callback_read: u64,
    pub callback_write: u64,
    pub callback_flush: u64,
    pub callback_release: u64,
    pub callback_fsync: u64,
    pub callback_opendir: u64,
    pub callback_readdir: u64,
    pub callback_readdirplus: u64,
    pub callback_releasedir: u64,
    pub callback_fsyncdir: u64,
    pub callback_statfs: u64,
    pub callback_access: u64,
    pub callback_create: u64,
    pub directory_entries_returned: u64,
    pub directory_nonzero_offset_requests: u64,
}

pub(crate) fn decode_layer_stack(row: &rusqlite::Row<'_>) -> rusqlite::Result<LayerStackRecord> {
    decode_layer_stack_at(row, 0)
}

pub(crate) fn decode_layer_stack_at(
    row: &rusqlite::Row<'_>,
    offset: usize,
) -> rusqlite::Result<LayerStackRecord> {
    Ok(LayerStackRecord {
        id: decode_sql(
            LayerStackId::from_slice(&row.get::<_, Vec<u8>>(offset)?),
            offset,
            rusqlite::types::Type::Blob,
        )?,
        name: decode_sql(
            EntityName::new(row.get::<_, String>(offset + 1)?),
            offset + 1,
            rusqlite::types::Type::Text,
        )?,
        head_layer_id: decode_sql(
            LayerId::from_slice(&row.get::<_, Vec<u8>>(offset + 2)?),
            offset + 2,
            rusqlite::types::Type::Blob,
        )?,
    })
}

pub(crate) fn decode_layer(row: &rusqlite::Row<'_>) -> rusqlite::Result<LayerRecord> {
    Ok(LayerRecord {
        id: decode_sql(
            LayerId::from_slice(&row.get::<_, Vec<u8>>(0)?),
            0,
            rusqlite::types::Type::Blob,
        )?,
        layer_stack_id: decode_sql(
            LayerStackId::from_slice(&row.get::<_, Vec<u8>>(1)?),
            1,
            rusqlite::types::Type::Blob,
        )?,
        parent_layer_id: decode_sql(optional_id(row.get(2)?), 2, rusqlite::types::Type::Blob)?,
        root_id: decode_sql(
            decode_object_id(row.get(3)?),
            3,
            rusqlite::types::Type::Blob,
        )?,
        source_branch_id: decode_sql(optional_id(row.get(4)?), 4, rusqlite::types::Type::Blob)?,
        source_commit_id: decode_sql(optional_id(row.get(5)?), 5, rusqlite::types::Type::Blob)?,
    })
}

pub(crate) fn decode_branch(row: &rusqlite::Row<'_>) -> rusqlite::Result<BranchRecord> {
    Ok(BranchRecord {
        id: decode_sql(
            BranchId::from_slice(&row.get::<_, Vec<u8>>(0)?),
            0,
            rusqlite::types::Type::Blob,
        )?,
        layer_stack_id: decode_sql(
            LayerStackId::from_slice(&row.get::<_, Vec<u8>>(1)?),
            1,
            rusqlite::types::Type::Blob,
        )?,
        name: decode_sql(
            EntityName::new(row.get::<_, String>(2)?),
            2,
            rusqlite::types::Type::Text,
        )?,
        base_layer_id: decode_sql(
            LayerId::from_slice(&row.get::<_, Vec<u8>>(3)?),
            3,
            rusqlite::types::Type::Blob,
        )?,
        head_commit_id: decode_sql(optional_id(row.get(4)?), 4, rusqlite::types::Type::Blob)?,
    })
}

pub(crate) fn decode_commit(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommitRecord> {
    Ok(CommitRecord {
        id: decode_sql(
            CommitId::from_slice(&row.get::<_, Vec<u8>>(0)?),
            0,
            rusqlite::types::Type::Blob,
        )?,
        root_id: decode_sql(
            decode_object_id(row.get(1)?),
            1,
            rusqlite::types::Type::Blob,
        )?,
        parent_commit_id: decode_sql(optional_id(row.get(2)?), 2, rusqlite::types::Type::Blob)?,
        base_layer_id: decode_sql(
            LayerId::from_slice(&row.get::<_, Vec<u8>>(3)?),
            3,
            rusqlite::types::Type::Blob,
        )?,
    })
}

pub(crate) fn decode_workspace_stage(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceStage> {
    let workspace = row.get::<_, Vec<u8>>(0)?;
    Ok(WorkspaceStage {
        workspace_id: decode_sql(
            workspace
                .try_into()
                .map_err(|_| StoreError::Integrity("Workspace ID length")),
            0,
            rusqlite::types::Type::Blob,
        )?,
        branch_id: decode_sql(
            BranchId::from_slice(&row.get::<_, Vec<u8>>(1)?),
            1,
            rusqlite::types::Type::Blob,
        )?,
        root_id: decode_sql(
            decode_object_id(row.get(2)?),
            2,
            rusqlite::types::Type::Blob,
        )?,
    })
}

pub(crate) fn optional_id<T: TypedId>(bytes: Option<Vec<u8>>) -> Result<Option<T>> {
    bytes.map(|bytes| T::from_slice(&bytes)).transpose()
}

pub(crate) fn decode_object_id(bytes: Vec<u8>) -> Result<ObjectId> {
    ObjectId::from_bytes(&bytes).map_err(Into::into)
}

fn decode_sql<T>(
    value: Result<T>,
    column: usize,
    value_type: rusqlite::types::Type,
) -> rusqlite::Result<T> {
    value.map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, value_type, Box::new(error))
    })
}
