use crate::{BranchId, CommitId, LayerId, LayerStackId, StorageError, StorageId};
use layerfs_content::ObjectId;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityName(String);

impl EntityName {
    pub const MAX_LEN: usize = 63;

    pub fn new(value: impl Into<String>) -> crate::Result<Self> {
        let value = value.into();
        Self::validate(&value)?;
        Ok(Self(value))
    }

    pub fn validate(value: &str) -> crate::Result<()> {
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
            return Err(StorageError::InvalidInput("entity name"));
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
    type Err = StorageError;

    fn from_str(value: &str) -> crate::Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<String> for EntityName {
    type Error = StorageError;

    fn try_from(value: String) -> crate::Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<&str> for EntityName {
    type Error = StorageError;

    fn try_from(value: &str) -> crate::Result<Self> {
        Self::new(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemotePlacement {
    Reference,
    Replica,
}

pub type ServingMode = RemotePlacement;

impl RemotePlacement {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Replica => "replica",
        }
    }

    pub(crate) fn parse(value: &str) -> crate::Result<Self> {
        match value {
            "reference" => Ok(Self::Reference),
            "replica" => Ok(Self::Replica),
            _ => Err(StorageError::Integrity("serving mode")),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerStackFact {
    pub id: LayerStackId,
    pub name: EntityName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerStackRecord {
    pub id: LayerStackId,
    pub name: EntityName,
    pub head_layer_id: LayerId,
}

impl LayerStackRecord {
    pub fn fact(&self) -> LayerStackFact {
        LayerStackFact {
            id: self.id,
            name: self.name.clone(),
        }
    }
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
pub struct BranchFact {
    pub id: BranchId,
    pub layer_stack_id: LayerStackId,
    pub name: EntityName,
    pub forked_from_layer_id: Option<LayerId>,
    pub forked_from_branch_id: Option<BranchId>,
    pub forked_from_commit_id: Option<CommitId>,
}

impl BranchFact {
    pub fn validate_origin(&self) -> crate::Result<()> {
        let layer_origin = self.forked_from_layer_id.is_some()
            && self.forked_from_branch_id.is_none()
            && self.forked_from_commit_id.is_none();
        let branch_origin = self.forked_from_layer_id.is_none()
            && self.forked_from_branch_id.is_some()
            && self.forked_from_commit_id.is_some();
        if !layer_origin && !branch_origin {
            return Err(StorageError::InvalidInput("Branch origin"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchRecord {
    pub id: BranchId,
    pub layer_stack_id: LayerStackId,
    pub name: EntityName,
    pub base_layer_id: LayerId,
    pub head_commit_id: Option<CommitId>,
    pub forked_from_layer_id: Option<LayerId>,
    pub forked_from_branch_id: Option<BranchId>,
    pub forked_from_commit_id: Option<CommitId>,
}

impl BranchRecord {
    pub fn fact(&self) -> BranchFact {
        BranchFact {
            id: self.id,
            layer_stack_id: self.layer_stack_id,
            name: self.name.clone(),
            forked_from_layer_id: self.forked_from_layer_id,
            forked_from_branch_id: self.forked_from_branch_id,
            forked_from_commit_id: self.forked_from_commit_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayerStackScopeRecord {
    pub layer_stack_id: LayerStackId,
    pub through_layer_id: LayerId,
    pub serving_mode: ServingMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchScope {
    Local,
    Remote {
        through_commit_id: CommitId,
        serving_mode: ServingMode,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BranchScopeRecord {
    pub branch_id: BranchId,
    pub scope: BranchScope,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinnedBranchRecord {
    pub branch: BranchRecord,
    pub scope: BranchScopeRecord,
    pub root_id: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Fact {
    Commit(CommitRecord),
    Branch(BranchFact),
    LayerStack(LayerStackFact),
    Layer(LayerRecord),
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FactKind {
    Commit = 0,
    Branch = 1,
    LayerStack = 2,
    Layer = 3,
}

impl Fact {
    pub const fn kind(&self) -> FactKind {
        match self {
            Self::Commit(_) => FactKind::Commit,
            Self::Branch(_) => FactKind::Branch,
            Self::LayerStack(_) => FactKind::LayerStack,
            Self::Layer(_) => FactKind::Layer,
        }
    }

    pub fn id(&self) -> Vec<u8> {
        match self {
            Self::Commit(value) => value.id.as_slice().to_vec(),
            Self::Branch(value) => value.id.as_slice().to_vec(),
            Self::LayerStack(value) => value.id.as_slice().to_vec(),
            Self::Layer(value) => value.id.as_slice().to_vec(),
        }
    }

    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![self.kind() as u8];
        match self {
            Self::Commit(value) => {
                bytes.extend_from_slice(value.id.as_slice());
                bytes.extend_from_slice(value.root_id.as_bytes());
                optional(&mut bytes, value.parent_commit_id.as_ref());
                bytes.extend_from_slice(value.base_layer_id.as_slice());
            }
            Self::Branch(value) => {
                bytes.extend_from_slice(value.id.as_slice());
                bytes.extend_from_slice(value.layer_stack_id.as_slice());
                framed_name(&mut bytes, &value.name);
                optional(&mut bytes, value.forked_from_layer_id.as_ref());
                optional(&mut bytes, value.forked_from_branch_id.as_ref());
                optional(&mut bytes, value.forked_from_commit_id.as_ref());
            }
            Self::LayerStack(value) => {
                bytes.extend_from_slice(value.id.as_slice());
                framed_name(&mut bytes, &value.name);
            }
            Self::Layer(value) => {
                bytes.extend_from_slice(value.id.as_slice());
                bytes.extend_from_slice(value.layer_stack_id.as_slice());
                optional(&mut bytes, value.parent_layer_id.as_ref());
                bytes.extend_from_slice(value.root_id.as_bytes());
                optional(&mut bytes, value.source_branch_id.as_ref());
                optional(&mut bytes, value.source_commit_id.as_ref());
            }
        }
        bytes
    }

    pub fn encoded_size(&self) -> usize {
        self.signing_bytes().len()
    }
}

fn framed_name(bytes: &mut Vec<u8>, value: &EntityName) {
    bytes.push(value.as_str().len() as u8);
    bytes.extend_from_slice(value.as_str().as_bytes());
}

fn optional<T: StorageId>(bytes: &mut Vec<u8>, value: Option<&T>) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        bytes.extend_from_slice(value.as_slice());
    }
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
pub enum PullLayerResult {
    Created {
        through_layer_id: LayerId,
        placement: RemotePlacement,
    },
    Advanced {
        previous_layer_id: LayerId,
        through_layer_id: LayerId,
        placement: RemotePlacement,
    },
    ModeChanged {
        through_layer_id: LayerId,
        previous: RemotePlacement,
        placement: RemotePlacement,
    },
    UpToDate {
        through_layer_id: LayerId,
        placement: RemotePlacement,
    },
    AlreadyContained {
        current_layer_id: LayerId,
        requested_layer_id: LayerId,
        placement: RemotePlacement,
    },
    HeadMoved {
        current_layer_id: LayerId,
        requested_layer_id: LayerId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PullBranchResult {
    Created {
        through_commit_id: CommitId,
        placement: RemotePlacement,
    },
    Advanced {
        previous_commit_id: CommitId,
        through_commit_id: CommitId,
        placement: RemotePlacement,
    },
    ModeChanged {
        through_commit_id: CommitId,
        previous: RemotePlacement,
        placement: RemotePlacement,
    },
    UpToDate {
        through_commit_id: CommitId,
        placement: RemotePlacement,
    },
    AlreadyContained {
        current_commit_id: CommitId,
        requested_commit_id: CommitId,
        placement: RemotePlacement,
    },
    HeadMoved {
        current_commit_id: CommitId,
        requested_commit_id: CommitId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PushResult {
    Created {
        commit_id: CommitId,
    },
    Advanced {
        previous: CommitId,
        commit_id: CommitId,
    },
    UpToDate {
        commit_id: CommitId,
    },
    HeadMoved {
        authority_head: CommitId,
        local_head: CommitId,
    },
    NoChanges,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorityAddResult {
    Added { layer_id: LayerId },
    UpToDate { layer_id: LayerId },
    NoChanges { head_layer_id: LayerId },
    HeadMoved { expected: LayerId, actual: LayerId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InventoryEntry {
    pub object_id: ObjectId,
    pub encoded_length: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryPage {
    pub entries: Vec<InventoryEntry>,
    pub continuation: Option<ObjectId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerPrefixPage {
    pub records: Vec<LayerRecord>,
    pub continuation: Option<LayerId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitHistoryPage {
    pub records: Vec<CommitRecord>,
    pub continuation: Option<CommitId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerStackRecordPage {
    pub records: Vec<LayerStackRecord>,
    pub continuation: Option<LayerStackId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchRecordPage {
    pub records: Vec<BranchRecord>,
    pub continuation: Option<BranchId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerStackScopePage {
    pub records: Vec<(LayerStackFact, LayerStackScopeRecord)>,
    pub continuation: Option<LayerStackId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchScopePage {
    pub records: Vec<(BranchRecord, BranchScopeRecord)>,
    pub continuation: Option<BranchId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreStorageSnapshot {
    pub database_bytes: u64,
    pub wal_bytes: u64,
    pub shm_bytes: u64,
}
