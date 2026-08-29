use crate::{
    AddResult, BaseId, BranchId, BranchRecord, CanonicalObject, CommitId, CommitRecord, Fact,
    FactKind, LayerHistoryId, LayerHistoryRecord, LayerId, LayerRecord, MissingBitmap,
    StackHistoryId, StackHistoryRecord, StackId, StackRecord,
};
use layerfs_core::ObjectId;
use std::fmt;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, StorageError>;

#[cfg(feature = "test-instrumentation")]
static TRAVERSAL_AUTHENTICATIONS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "test-instrumentation")]
static RECEIVER_AUTHENTICATIONS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

pub(crate) fn note_traversal_authentication() {
    #[cfg(feature = "test-instrumentation")]
    TRAVERSAL_AUTHENTICATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn note_receiver_authentication() {
    #[cfg(feature = "test-instrumentation")]
    RECEIVER_AUTHENTICATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(feature = "test-instrumentation")]
#[doc(hidden)]
pub fn reset_transfer_authentication_counts() {
    TRAVERSAL_AUTHENTICATIONS.store(0, std::sync::atomic::Ordering::Relaxed);
    RECEIVER_AUTHENTICATIONS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(feature = "test-instrumentation")]
#[doc(hidden)]
pub fn transfer_authentication_counts() -> (u64, u64) {
    (
        TRAVERSAL_AUTHENTICATIONS.load(std::sync::atomic::Ordering::Relaxed),
        RECEIVER_AUTHENTICATIONS.load(std::sync::atomic::Ordering::Relaxed),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeadMoved<I> {
    pub expected: Option<I>,
    pub actual: Option<I>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrongHistory<H> {
    pub expected: H,
    pub actual: H,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadOnlyHistory<H> {
    pub history_id: H,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Conflict {
    pub path: String,
    pub base: Option<ObjectId>,
    pub current: Option<ObjectId>,
    pub candidate: Option<ObjectId>,
}

#[derive(Debug)]
pub enum StorageError {
    CommitHeadMoved(HeadMoved<CommitId>),
    StackHeadMoved(HeadMoved<StackId>),
    LayerHeadMoved(HeadMoved<LayerId>),
    WrongStackHistory(WrongHistory<StackHistoryId>),
    WrongLayerHistory(WrongHistory<LayerHistoryId>),
    ReadOnlyStackHistory(ReadOnlyHistory<StackHistoryId>),
    WrongSourceRoute,
    NoCommonBase,
    AmbiguousMergeBase,
    MissingBaseData,
    Conflict(Box<Conflict>),
    Integrity(&'static str),
    StoreBusy,
    NotFound(&'static str),
    InvalidInput(&'static str),
    Database(String),
    Io(std::io::Error),
    Core(layerfs_core::CoreError),
}

impl PartialEq for StorageError {
    fn eq(&self, other: &Self) -> bool {
        format!("{self:?}") == format!("{other:?}")
    }
}

impl Eq for StorageError {}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for StorageError {}

impl From<rusqlite::Error> for StorageError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value.to_string())
    }
}

impl From<std::io::Error> for StorageError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<layerfs_core::CoreError> for StorageError {
    fn from(value: layerfs_core::CoreError) -> Self {
        Self::Core(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Change {
    Write {
        path: String,
        bytes: Vec<u8>,
        mode: u32,
    },
    Splice {
        path: String,
        start: u64,
        delete_len: u64,
        replacement: Vec<u8>,
    },
    Mkdir {
        path: String,
        mode: u32,
    },
    Symlink {
        path: String,
        target: Vec<u8>,
    },
    HardLink {
        source: String,
        target: String,
    },
    Rename {
        source: String,
        target: String,
    },
    Remove {
        path: String,
    },
    SetMode {
        path: String,
        mode: u32,
    },
    SetMtime {
        path: String,
        seconds: i64,
        nanoseconds: u32,
    },
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StagedChange {
    Inline(Change),
    SpliceFile {
        path: String,
        start: u64,
        delete_len: u64,
        spool: PathBuf,
        spool_offset: u64,
        replacement_len: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddLayerSource {
    BranchSource {
        branch_id: BranchId,
        commit_id: CommitId,
    },
    StackSource(StackId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefOutcome<I> {
    Created(I),
    FastForwarded(I),
    UpToDate(I),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MergeOutcome {
    UpToDate(CommitId),
    FastForwarded(CommitId),
    Merged(CommitId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaseSnapshot {
    pub base_id: BaseId,
    pub layer_history_id: LayerHistoryId,
    pub root_id: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StackPush {
    pub history_id: StackHistoryId,
    pub base_layer_id: LayerId,
    pub expected_head: Option<StackId>,
    pub incoming_head: StackId,
    pub fact_count: u64,
    pub root_count: u64,
    pub provenance_digest: [u8; 32],
    pub publication_count: u64,
    pub publication_digest: [u8; 32],
    pub public_key: [u8; 32],
    pub signature: [u8; 64],
}

impl StackPush {
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"layerfs/stack-push/v1\0");
        bytes.extend_from_slice(crate::StorageId::as_slice(&self.history_id));
        bytes.extend_from_slice(crate::StorageId::as_slice(&self.base_layer_id));
        match self.expected_head {
            Some(id) => {
                bytes.push(1);
                bytes.extend_from_slice(crate::StorageId::as_slice(&id));
            }
            None => bytes.push(0),
        }
        bytes.extend_from_slice(crate::StorageId::as_slice(&self.incoming_head));
        bytes.extend_from_slice(&self.fact_count.to_be_bytes());
        bytes.extend_from_slice(&self.root_count.to_be_bytes());
        bytes.extend_from_slice(&self.provenance_digest);
        bytes.extend_from_slice(&self.publication_count.to_be_bytes());
        bytes.extend_from_slice(&self.publication_digest);
        bytes
    }
}

#[doc(hidden)]
pub struct StackAttestation {
    hasher: blake3::Hasher,
    fact_count: u64,
    root_count: u64,
    last_kind: Option<FactKind>,
}

impl Default for StackAttestation {
    fn default() -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/stack-foundation/v1\0");
        Self {
            hasher,
            fact_count: 0,
            root_count: 0,
            last_kind: None,
        }
    }
}

impl StackAttestation {
    pub fn observe(&mut self, kind: FactKind, ids: &[Vec<u8>]) {
        if !matches!(kind, FactKind::Commit | FactKind::Stack) {
            return;
        }
        if self.last_kind != Some(kind) {
            self.hasher.update(&[match kind {
                FactKind::Commit => 0,
                FactKind::Stack => 1,
                _ => unreachable!(),
            }]);
            self.last_kind = Some(kind);
        }
        for id in ids {
            self.hasher.update(&(id.len() as u64).to_be_bytes());
            self.hasher.update(id);
            self.fact_count += 1;
            self.root_count += 1;
        }
    }

    pub fn finish(self) -> (u64, u64, [u8; 32]) {
        (
            self.fact_count,
            self.root_count,
            *self.hasher.finalize().as_bytes(),
        )
    }

    pub fn verify(self, push: &StackPush) -> Result<()> {
        let (facts, roots, digest) = self.finish();
        if facts == push.fact_count && roots == push.root_count && digest == push.provenance_digest
        {
            Ok(())
        } else {
            Err(StorageError::Integrity("Stack provenance digest"))
        }
    }
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EndpointRequest {
    ReadObjects(Vec<ObjectId>),
    TransferBeginBranch {
        branch: BranchRecord,
        root: ObjectId,
    },
    TransferBeginStack {
        history_id: StackHistoryId,
        base_layer_id: LayerId,
        incoming: StackId,
        root: ObjectId,
    },
    Transfer {
        objects: Vec<(ObjectId, u64)>,
        facts: Vec<Fact>,
        object_ids: Vec<ObjectId>,
        fact_kind: Option<FactKind>,
        fact_ids: Vec<Vec<u8>>,
    },
    TransferEnd {
        objects: Vec<(ObjectId, u64)>,
        facts: Vec<Fact>,
        intent: Box<TransferIntent>,
    },
    TransferAbort,
    HistoryMissing(MissingBitmap),
    BaseSnapshot(BaseId),
    CommonBase {
        left: BaseId,
        right: BaseId,
    },
    BranchRecord(BranchId),
    CommitPages(BranchId),
    LayerHistoryRecord(LayerHistoryId),
    LayerHistoryPrefix {
        history_id: LayerHistoryId,
        through: LayerId,
    },
    StackHistoryPrefix {
        history_id: StackHistoryId,
        through: StackId,
    },
    StackHistoryHead(StackHistoryId),
    AddStack {
        stack_history_id: StackHistoryId,
        branch_id: BranchId,
        commit_id: CommitId,
    },
    PushStack(StackId),
    AddLayer {
        layer_history_id: LayerHistoryId,
        source: AddLayerSource,
    },
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EndpointResponse {
    Objects(Vec<(ObjectId, u64)>),
    Exchange(TransferExchange),
    TransferDone {
        exchange: TransferExchange,
        outcome: TransferOutcome,
    },
    BaseSnapshot(BaseSnapshot),
    BranchRecord(BranchRecord),
    LayerHistoryRecord(LayerHistoryRecord),
    StackHistoryRecord(StackHistoryRecord),
    Facts(Vec<Fact>),
    FactIds {
        kind: FactKind,
        ids: Vec<Vec<u8>>,
    },
    CommitRef(RefOutcome<CommitId>),
    StackRef(RefOutcome<StackId>),
    StackAdd(AddResult<StackId>),
    LayerAdd(AddResult<LayerId>),
    Unit,
}

#[doc(hidden)]
pub type EndpointReply = Result<EndpointResponse>;

pub trait ObjectSource {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>>;

    fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        ids.iter()
            .map(|id| {
                Ok(CanonicalObject {
                    id: *id,
                    bytes: self.read_object(*id)?,
                })
            })
            .collect()
    }

    fn visit_objects(
        &self,
        ids: &[ObjectId],
        visitor: &mut dyn FnMut(CanonicalObject) -> Result<()>,
    ) -> Result<()> {
        for object in self.read_objects(ids)? {
            visitor(object)?;
        }
        Ok(())
    }
}

pub trait StoreEndpoint: ObjectSource + Send + Sync {
    #[doc(hidden)]
    fn begin_transfer(&self) -> Result<Box<dyn TransferTarget + '_>>;

    #[doc(hidden)]
    fn transfer_exchange_unlocked(
        &self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        object_ids: &[ObjectId],
        fact_ids: Option<(FactKind, &[Vec<u8>])>,
    ) -> Result<TransferExchange>;

    fn base_snapshot(&self, _base_id: BaseId) -> Result<BaseSnapshot> {
        Err(StorageError::WrongSourceRoute)
    }

    fn common_base(&self, left: BaseId, right: BaseId) -> Result<BaseSnapshot> {
        if left == right {
            self.base_snapshot(left)
        } else {
            Err(StorageError::NoCommonBase)
        }
    }

    fn branch_record(&self, _branch_id: BranchId) -> Result<BranchRecord> {
        Err(StorageError::WrongSourceRoute)
    }

    #[allow(clippy::type_complexity)]
    fn visit_commits(
        &self,
        _branch_id: BranchId,
        _membership: &mut dyn FnMut(FactKind, &[Vec<u8>]) -> Result<MissingBitmap>,
        _visitor: &mut dyn FnMut(&[CommitRecord]) -> Result<()>,
    ) -> Result<()> {
        Err(StorageError::WrongSourceRoute)
    }

    fn layer_history_record(&self, _history_id: LayerHistoryId) -> Result<LayerHistoryRecord> {
        Err(StorageError::WrongSourceRoute)
    }

    #[allow(clippy::type_complexity)]
    fn visit_layers(
        &self,
        _history_id: LayerHistoryId,
        _through: LayerId,
        _membership: &mut dyn FnMut(FactKind, &[Vec<u8>]) -> Result<MissingBitmap>,
        _visitor: &mut dyn FnMut(&[LayerRecord]) -> Result<()>,
    ) -> Result<()> {
        Err(StorageError::WrongSourceRoute)
    }

    fn stack_history_record(&self, _history_id: StackHistoryId) -> Result<StackHistoryRecord> {
        Err(StorageError::WrongSourceRoute)
    }

    #[allow(clippy::type_complexity)]
    fn visit_stacks(
        &self,
        _history_id: StackHistoryId,
        _through: StackId,
        _membership: &mut dyn FnMut(FactKind, &[Vec<u8>]) -> Result<MissingBitmap>,
        _visitor: &mut dyn FnMut(&[StackRecord]) -> Result<()>,
    ) -> Result<()> {
        Err(StorageError::WrongSourceRoute)
    }

    fn add_stack(
        &self,
        _stack_history_id: StackHistoryId,
        _branch_id: BranchId,
        _commit_id: CommitId,
    ) -> Result<crate::AddResult<StackId>> {
        Err(StorageError::WrongSourceRoute)
    }

    fn add_layer(
        &self,
        _layer_history_id: LayerHistoryId,
        _source: AddLayerSource,
    ) -> Result<crate::AddResult<LayerId>> {
        Err(StorageError::WrongSourceRoute)
    }
}

#[doc(hidden)]
pub trait TransferTarget {
    fn preflight_branch(
        &mut self,
        _branch: BranchRecord,
        _root: ObjectId,
    ) -> Result<(Option<CommitId>, bool, MissingBitmap)> {
        Err(StorageError::WrongSourceRoute)
    }

    fn preflight_stack(
        &mut self,
        _history_id: StackHistoryId,
        _base_layer_id: LayerId,
        _incoming: StackId,
        _root: ObjectId,
    ) -> Result<(Option<StackId>, bool, MissingBitmap)> {
        Err(StorageError::WrongSourceRoute)
    }

    fn exchange(
        &mut self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        object_ids: &[ObjectId],
        fact_ids: Option<(FactKind, &[Vec<u8>])>,
    ) -> Result<TransferExchange>;

    fn defer_publication(&mut self, _facts: &[Fact]) -> Result<()> {
        Err(StorageError::WrongSourceRoute)
    }

    fn finish(
        self: Box<Self>,
        objects: &[CanonicalObject],
        facts: &[Fact],
        intent: TransferIntent,
    ) -> Result<(TransferExchange, TransferOutcome)>;
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransferIntent {
    None,
    Branch {
        branch: BranchRecord,
        expected: Option<CommitId>,
    },
    Stack(StackPush),
    ObserveLayer(LayerHistoryRecord),
    ObserveStack {
        history: StackHistoryRecord,
        expected: Option<StackId>,
    },
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferOutcome {
    Unit,
    Commit(RefOutcome<CommitId>),
    Stack(RefOutcome<StackId>),
    Layer(RefOutcome<LayerId>),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AdmissionStats {
    pub(crate) inserted_ids: u64,
    pub(crate) inserted_bytes: u64,
    pub(crate) raced_existing_ids: u64,
    pub(crate) raced_existing_bytes: u64,
    pub(crate) transactions: u64,
}

impl AdmissionStats {
    pub(crate) fn merge(&mut self, other: Self) {
        self.inserted_ids += other.inserted_ids;
        self.inserted_bytes += other.inserted_bytes;
        self.raced_existing_ids += other.raced_existing_ids;
        self.raced_existing_bytes += other.raced_existing_bytes;
        self.transactions += other.transactions;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct TransferExchange {
    admission: AdmissionStats,
    objects: MissingBitmap,
    facts: MissingBitmap,
}

impl TransferExchange {
    pub(crate) fn new(
        admission: AdmissionStats,
        objects: MissingBitmap,
        facts: MissingBitmap,
    ) -> Self {
        Self {
            admission,
            objects,
            facts,
        }
    }

    pub(crate) fn into_parts(self) -> (AdmissionStats, MissingBitmap, MissingBitmap) {
        (self.admission, self.objects, self.facts)
    }

    pub fn membership(objects: MissingBitmap, facts: MissingBitmap) -> Self {
        Self::new(AdmissionStats::default(), objects, facts)
    }

    pub fn missing(self) -> (MissingBitmap, MissingBitmap) {
        (self.objects, self.facts)
    }

    pub fn absorb(&mut self, other: Self) -> Result<()> {
        if other.objects != MissingBitmap::empty() || other.facts != MissingBitmap::empty() {
            return Err(StorageError::Integrity("final transfer membership"));
        }
        self.admission.merge(other.admission);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TransferStats {
    pub(crate) announced_ids: u64,
    pub(crate) announced_bytes: u64,
    pub(crate) missing_ids: u64,
    pub(crate) missing_bytes: u64,
    pub(crate) sent_ids: u64,
    pub(crate) sent_bytes: u64,
    pub(crate) inserted_ids: u64,
    pub(crate) inserted_bytes: u64,
    pub(crate) raced_existing_ids: u64,
    pub(crate) raced_existing_bytes: u64,
    pub(crate) object_pages: u64,
    pub(crate) typed_pages: u64,
    pub(crate) payload_batches: u64,
    pub(crate) payload_frames: u64,
    pub(crate) peak_buffer_bytes: u64,
    pub(crate) transactions: u64,
    pub(crate) wire_turns: u64,
}

impl TransferStats {
    pub(crate) fn merge_admission(&mut self, stats: AdmissionStats) {
        self.inserted_ids += stats.inserted_ids;
        self.inserted_bytes += stats.inserted_bytes;
        self.raced_existing_ids += stats.raced_existing_ids;
        self.raced_existing_bytes += stats.raced_existing_bytes;
        self.transactions += stats.transactions;
    }

    pub(crate) fn validate(self) -> Result<()> {
        let pages = self.object_pages + self.typed_pages;
        if self.sent_ids != self.missing_ids
            || self.sent_bytes != self.missing_bytes
            || self.missing_ids != self.inserted_ids + self.raced_existing_ids
            || self.missing_bytes != self.inserted_bytes + self.raced_existing_bytes
            || self.wire_turns > pages + 1
        {
            return Err(StorageError::Integrity("transfer set equation"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/support/contract_unit.rs"]
mod transfer_tests;
