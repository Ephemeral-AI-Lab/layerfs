use crate::{
    AuthorityAddResult, BranchFact, BranchId, BranchRecord, CommitHistoryPage, CommitId,
    CommitRecord, Fact, FactKind, LayerId, LayerPrefixPage, LayerRecord, LayerStackFact,
    LayerStackId, LayerStackRecord, PushResult, Result, StorageError, StoreDb, StoreId,
};
use layerfs_content::ObjectId;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::Instant;

pub const ID_BATCH_COUNT: usize = 512;
pub const OBJECT_BATCH_COUNT: usize = 128;
pub const OBJECT_BATCH_BYTES: usize = 4 * 1024 * 1024;
pub const FACT_BATCH_COUNT: usize = 128;
pub const FACT_BATCH_BYTES: usize = 64 * 1024;
pub const TRANSFER_BUFFER_BYTES: usize = 34 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalObject {
    pub id: ObjectId,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RootTransferRequest {
    pub root_id: ObjectId,
    pub known_complete: bool,
}

impl CanonicalObject {
    pub fn new(bytes: Vec<u8>) -> Result<Self> {
        let id = ObjectId::for_bytes(&bytes);
        layerfs_content::authenticate_identity(&bytes, id)?;
        Ok(Self { id, bytes })
    }
}

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
pub fn reset_transfer_authentication_counts() {
    TRAVERSAL_AUTHENTICATIONS.store(0, std::sync::atomic::Ordering::Relaxed);
    RECEIVER_AUTHENTICATIONS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(feature = "test-instrumentation")]
pub fn transfer_authentication_counts() -> (u64, u64) {
    (
        TRAVERSAL_AUTHENTICATIONS.load(std::sync::atomic::Ordering::Relaxed),
        RECEIVER_AUTHENTICATIONS.load(std::sync::atomic::Ordering::Relaxed),
    )
}

pub trait ObjectSource {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>>;

    fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        if ids.len() > ID_BATCH_COUNT {
            return Err(StorageError::InvalidInput("object read page"));
        }
        ids.iter()
            .map(|id| {
                let bytes = self.read_object(*id)?;
                layerfs_content::authenticate_identity(&bytes, *id)?;
                Ok(CanonicalObject { id: *id, bytes })
            })
            .collect()
    }

    fn visit_objects(
        &self,
        ids: &[ObjectId],
        visitor: &mut dyn FnMut(CanonicalObject) -> Result<()>,
    ) -> Result<()> {
        if ids.len() > ID_BATCH_COUNT {
            return Err(StorageError::InvalidInput("object read page"));
        }
        for id in ids {
            let bytes = self.read_object(*id)?;
            layerfs_content::authenticate_identity(&bytes, *id)?;
            visitor(CanonicalObject { id: *id, bytes })?;
        }
        Ok(())
    }

    fn prune_existing_subtree(&self, _id: ObjectId) -> Result<bool> {
        Ok(false)
    }
}

pub trait LayerStackEndpoint: ObjectSource + Send + Sync {
    fn store_id(&self) -> Result<StoreId>;
    fn layer_stack_fact(&self, id: LayerStackId) -> Result<Option<LayerStackFact>>;
    fn layer_stack(&self, id: LayerStackId) -> Result<Option<LayerStackRecord>>;
    fn layer(&self, id: LayerId) -> Result<Option<LayerRecord>>;
    fn branch_fact(&self, id: BranchId) -> Result<Option<BranchFact>>;
    fn branch(&self, id: BranchId) -> Result<Option<BranchRecord>>;
    fn commit(&self, id: CommitId) -> Result<Option<CommitRecord>>;
    fn layer_prefix_page(
        &self,
        through_layer_id: LayerId,
        cursor: Option<LayerId>,
        limit: u16,
    ) -> Result<LayerPrefixPage>;
    fn layer_ancestry_page(
        &self,
        through_layer_id: LayerId,
        stop_exclusive: Option<LayerId>,
        cursor: Option<LayerId>,
        limit: u16,
    ) -> Result<LayerPrefixPage>;
    fn commit_history_page(
        &self,
        branch_id: BranchId,
        through_commit_id: CommitId,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitHistoryPage>;
    fn commit_ancestry_page(
        &self,
        through_commit_id: CommitId,
        stop_exclusive: Option<CommitId>,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitHistoryPage>;
    fn owned_commit_page(
        &self,
        branch_id: BranchId,
        through_commit_id: CommitId,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitHistoryPage>;
    fn missing_objects(&self, ids: &[ObjectId]) -> Result<MissingBitmap>;
    fn object_membership(&self, ids: &[ObjectId]) -> Result<(MissingBitmap, Vec<Option<u64>>)> {
        let missing = self.missing_objects(ids)?;
        let mut lengths = Vec::with_capacity(ids.len());
        for (index, id) in ids.iter().enumerate() {
            lengths.push(if missing.is_missing(index)? {
                None
            } else {
                Some(self.read_object(*id)?.len() as u64)
            });
        }
        Ok((missing, lengths))
    }
    fn missing_facts(&self, facts: &[Fact]) -> Result<MissingBitmap>;
    fn admit_objects(&self, objects: &[CanonicalObject]) -> Result<AdmissionSetReceipt>;
    fn admit_facts(&self, facts: &[Fact]) -> Result<AdmissionSetReceipt>;
    fn publish_branch(
        &self,
        branch: &BranchRecord,
        observed_head: Option<CommitId>,
    ) -> Result<PushResult>;
    fn add_layer(&self, branch_id: BranchId) -> Result<AuthorityAddResult>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MissingBitmap([u8; 64]);

impl MissingBitmap {
    pub const fn empty() -> Self {
        Self([0; 64])
    }

    pub fn from_missing(indices: impl IntoIterator<Item = usize>) -> Result<Self> {
        let mut bitmap = Self::empty();
        for index in indices {
            if index >= ID_BATCH_COUNT {
                return Err(StorageError::InvalidInput("membership index"));
            }
            bitmap.0[index / 8] |= 1 << (index % 8);
        }
        Ok(bitmap)
    }

    pub fn is_missing(self, index: usize) -> Result<bool> {
        if index >= ID_BATCH_COUNT {
            return Err(StorageError::InvalidInput("membership index"));
        }
        Ok(self.0[index / 8] & (1 << (index % 8)) != 0)
    }

    pub fn count(self) -> u64 {
        self.0.iter().map(|byte| byte.count_ones() as u64).sum()
    }

    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    pub fn validate_tail(self, len: usize) -> Result<()> {
        if len > ID_BATCH_COUNT
            || (len..ID_BATCH_COUNT).any(|index| self.is_missing(index).unwrap())
        {
            return Err(StorageError::Integrity("membership bitmap tail"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AdmissionSetReceipt {
    pub inserted_ids: u64,
    pub inserted_bytes: u64,
    pub raced_existing_ids: u64,
    pub raced_existing_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MeasuredBytes {
    Exact(u64),
    #[default]
    Unavailable,
}

impl MeasuredBytes {
    pub fn exact(self) -> Option<u64> {
        match self {
            Self::Exact(bytes) => Some(bytes),
            Self::Unavailable => None,
        }
    }

    fn add(&mut self, bytes: u64) {
        if let Self::Exact(total) = self {
            *total = total.saturating_add(bytes);
        }
    }

    fn merge(&mut self, other: Self) {
        match (*self, other) {
            (Self::Exact(left), Self::Exact(right)) => {
                *self = Self::Exact(left.saturating_add(right));
            }
            _ => *self = Self::Unavailable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferSetReceipt {
    pub announced_ids: u64,
    pub announced_bytes: MeasuredBytes,
    pub missing_ids: u64,
    pub missing_bytes: u64,
    pub sent_ids: u64,
    pub sent_bytes: u64,
    pub inserted_ids: u64,
    pub inserted_bytes: u64,
    pub raced_existing_ids: u64,
    pub raced_existing_bytes: u64,
}

impl Default for TransferSetReceipt {
    fn default() -> Self {
        Self {
            announced_ids: 0,
            announced_bytes: MeasuredBytes::Unavailable,
            missing_ids: 0,
            missing_bytes: 0,
            sent_ids: 0,
            sent_bytes: 0,
            inserted_ids: 0,
            inserted_bytes: 0,
            raced_existing_ids: 0,
            raced_existing_bytes: 0,
        }
    }
}

impl TransferSetReceipt {
    fn exact_bytes() -> Self {
        Self {
            announced_bytes: MeasuredBytes::Exact(0),
            ..Self::default()
        }
    }

    pub fn preexisting_ids(self) -> u64 {
        self.announced_ids.saturating_sub(self.missing_ids)
    }

    pub fn preexisting_bytes(self) -> MeasuredBytes {
        match self.announced_bytes {
            MeasuredBytes::Exact(announced) => {
                MeasuredBytes::Exact(announced.saturating_sub(self.missing_bytes))
            }
            MeasuredBytes::Unavailable => MeasuredBytes::Unavailable,
        }
    }

    pub fn validate(self) -> Result<()> {
        if self.missing_ids > self.announced_ids
            || self.sent_ids != self.missing_ids
            || self.missing_ids != self.inserted_ids + self.raced_existing_ids
            || self.missing_bytes != self.sent_bytes
            || self.sent_bytes != self.inserted_bytes + self.raced_existing_bytes
            || matches!(
                self.announced_bytes,
                MeasuredBytes::Exact(announced) if self.missing_bytes > announced
            )
        {
            return Err(StorageError::Integrity("transfer set equation"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LocalObjectReceipt {
    pub candidate_ids: u64,
    pub candidate_bytes: u64,
    pub inserted_ids: u64,
    pub inserted_bytes: u64,
    pub reused_ids: u64,
    pub reused_bytes: u64,
}

impl LocalObjectReceipt {
    pub fn validate(self) -> Result<()> {
        if self.candidate_ids != self.inserted_ids + self.reused_ids
            || self.candidate_bytes != self.inserted_bytes + self.reused_bytes
        {
            return Err(StorageError::Integrity("local CAS equation"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LocalAdmissionReceipt {
    pub objects: LocalObjectReceipt,
    pub cdc_bytes_scanned: u64,
    pub encode_hash_invocations: u64,
    pub source_reused_ids: u64,
    pub source_reused_bytes: u64,
}

impl LocalAdmissionReceipt {
    pub fn validate(&self) -> Result<()> {
        self.objects.validate()?;
        if self.source_reused_ids > self.objects.reused_ids
            || self.source_reused_bytes > self.objects.reused_bytes
        {
            return Err(StorageError::Integrity("source reuse equation"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DurabilityReceipt {
    pub store_id: StoreId,
    pub role: crate::StoreRole,
    pub stable_ns: u64,
    pub checkpoint_ns: u64,
    pub database_fsync_ns: u64,
    pub directory_fsync_ns: u64,
    pub unattributed_ns: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureMode {
    Live,
    Materialized,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceCommitReceipt {
    pub total_ns: u64,
    pub pause_fence_ns: u64,
    pub quiesce_ns: u64,
    pub capture_ns: u64,
    pub capture_mode: Option<CaptureMode>,
    pub captured_files: u64,
    pub captured_bytes: u64,
    pub candidate_plan_ns: u64,
    pub dirty_compare_ns: u64,
    pub content_ns: u64,
    pub namespace_ns: u64,
    pub candidate_finish_ns: u64,
    pub local_admission_ns: u64,
    pub completeness_verify_ns: u64,
    pub publication_ns: u64,
    pub in_place_rebase_ns: u64,
    pub resume_ns: u64,
    pub unattributed_ns: u64,
}

impl WorkspaceCommitReceipt {
    fn attributed_ns(self) -> u64 {
        [
            self.pause_fence_ns,
            self.quiesce_ns,
            self.capture_ns,
            self.candidate_plan_ns,
            self.dirty_compare_ns,
            self.content_ns,
            self.namespace_ns,
            self.candidate_finish_ns,
            self.local_admission_ns,
            self.completeness_verify_ns,
            self.publication_ns,
            self.in_place_rebase_ns,
            self.resume_ns,
        ]
        .into_iter()
        .fold(0_u64, u64::saturating_add)
    }

    pub fn validate(self) -> Result<()> {
        if self.capture_mode.is_none()
            || self.total_ns != self.attributed_ns().saturating_add(self.unattributed_ns)
            || (self.capture_mode == Some(CaptureMode::Live)
                && (self.captured_files != 0 || self.captured_bytes != 0))
        {
            return Err(StorageError::Integrity("Workspace Commit timing equation"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceCommitPhase {
    PauseFence,
    Quiesce,
    Capture,
    CandidatePlan,
    DirtyCompare,
    Content,
    Namespace,
    CandidateFinish,
    LocalAdmission,
    CompletenessVerify,
    Publication,
    InPlaceRebase,
    Resume,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PushPhaseReceipt {
    pub total_ns: u64,
    pub history_ns: u64,
    pub frontier_ns: u64,
    pub membership_ns: u64,
    pub source_read_auth_ns: u64,
    pub object_admission_ns: u64,
    pub fact_admission_ns: u64,
    pub authority_transition_verify_ns: u64,
    pub publication_ns: u64,
    pub durability_ns: u64,
    pub unattributed_ns: u64,
    pub endpoint_calls: u64,
}

impl PushPhaseReceipt {
    fn attributed_ns(self) -> u64 {
        [
            self.history_ns,
            self.frontier_ns,
            self.membership_ns,
            self.source_read_auth_ns,
            self.object_admission_ns,
            self.fact_admission_ns,
            self.authority_transition_verify_ns,
            self.publication_ns,
            self.durability_ns,
        ]
        .into_iter()
        .fold(0_u64, u64::saturating_add)
    }

    pub fn validate(self) -> Result<()> {
        if self.total_ns != self.attributed_ns().saturating_add(self.unattributed_ns) {
            return Err(StorageError::Integrity("Push timing equation"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PushPhase {
    History,
    Frontier,
    Membership,
    SourceReadAuth,
    ObjectAdmission,
    FactAdmission,
    AuthorityTransitionVerify,
    Publication,
    Durability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatabaseOperation {
    ObjectAdmission,
    FactAdmission,
    CommitCas,
    AuthorityPublish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DatabaseReceipt {
    pub store_id: StoreId,
    pub role: crate::StoreRole,
    pub operation: DatabaseOperation,
    pub total_ns: u64,
    pub connection_wait_ns: u64,
    pub writer_acquire_ns: u64,
    pub statement_ns: u64,
    pub publication_ns: u64,
    pub commit_sync_ns: u64,
    pub unattributed_ns: u64,
    pub statement_count: u64,
    pub rows: u64,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceLifecycleKind {
    Attach,
    End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceLifecycleReceipt {
    pub kind: WorkspaceLifecycleKind,
    pub total_ns: u64,
    pub proxy_ns: u64,
    pub docker_setup_ns: u64,
    pub helper_copy_ns: u64,
    pub mount_ready_ns: u64,
    pub unmount_ns: u64,
    pub wait_ns: u64,
    pub cleanup_ns: u64,
    pub unattributed_ns: u64,
    pub docker_calls: u64,
}

impl WorkspaceLifecycleReceipt {
    pub fn validate(self) -> Result<()> {
        if self.total_ns
            != self
                .proxy_ns
                .saturating_add(self.docker_setup_ns)
                .saturating_add(self.helper_copy_ns)
                .saturating_add(self.mount_ready_ns)
                .saturating_add(self.unmount_ns)
                .saturating_add(self.wait_ns)
                .saturating_add(self.cleanup_ns)
                .saturating_add(self.unattributed_ns)
        {
            return Err(StorageError::Integrity(
                "Workspace lifecycle timing equation",
            ));
        }
        Ok(())
    }
}

impl DatabaseReceipt {
    pub fn validate(self) -> Result<()> {
        if self.total_ns
            != self
                .connection_wait_ns
                .saturating_add(self.writer_acquire_ns)
                .saturating_add(self.statement_ns)
                .saturating_add(self.publication_ns)
                .saturating_add(self.commit_sync_ns)
                .saturating_add(self.unattributed_ns)
        {
            return Err(StorageError::Integrity("database timing equation"));
        }
        Ok(())
    }
}

impl DurabilityReceipt {
    pub fn validate(self) -> Result<()> {
        if self.stable_ns
            != self
                .checkpoint_ns
                .saturating_add(self.database_fsync_ns)
                .saturating_add(self.directory_fsync_ns)
                .saturating_add(self.unattributed_ns)
        {
            return Err(StorageError::Integrity("durability timing equation"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageReceipt {
    Local(LocalAdmissionReceipt),
    Transfer(TransferReceipt),
    Durability(DurabilityReceipt),
    WorkspaceCommit(WorkspaceCommitReceipt),
    Push(PushPhaseReceipt),
    Database(DatabaseReceipt),
    WorkspaceLifecycle(WorkspaceLifecycleReceipt),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TransferReceipt {
    pub objects: TransferSetReceipt,
    pub facts: BTreeMap<FactKind, TransferSetReceipt>,
    pub membership_pages: u64,
    pub payload_batches: u64,
    pub peak_buffer_bytes: u64,
    pub known_roots_pruned: u64,
}

impl TransferReceipt {
    pub fn validate(&self) -> Result<()> {
        self.objects.validate()?;
        for receipt in self.facts.values() {
            receipt.validate()?;
        }
        if self.peak_buffer_bytes >= TRANSFER_BUFFER_BYTES as u64 {
            return Err(StorageError::Integrity("transfer buffer ceiling"));
        }
        Ok(())
    }
}

pub trait TransferTarget {
    fn object_membership(&self, ids: &[ObjectId]) -> Result<(MissingBitmap, Vec<Option<u64>>)>;
    fn missing_objects(&self, ids: &[ObjectId]) -> Result<MissingBitmap> {
        Ok(self.object_membership(ids)?.0)
    }
    fn missing_facts(&self, facts: &[Fact]) -> Result<MissingBitmap>;
    fn admit_objects(&self, objects: &[CanonicalObject]) -> Result<AdmissionSetReceipt>;
    fn admit_facts(&self, facts: &[Fact]) -> Result<AdmissionSetReceipt>;
}

impl TransferTarget for StoreDb {
    fn object_membership(&self, ids: &[ObjectId]) -> Result<(MissingBitmap, Vec<Option<u64>>)> {
        StoreDb::object_membership(self, ids)
    }

    fn missing_facts(&self, facts: &[Fact]) -> Result<MissingBitmap> {
        StoreDb::missing_facts(self, facts)
    }

    fn admit_objects(&self, objects: &[CanonicalObject]) -> Result<AdmissionSetReceipt> {
        StoreDb::admit_objects(self, objects)
    }

    fn admit_facts(&self, facts: &[Fact]) -> Result<AdmissionSetReceipt> {
        StoreDb::admit_facts(self, facts)
    }
}

pub struct EndpointTarget<'a>(pub &'a dyn LayerStackEndpoint);

impl TransferTarget for EndpointTarget<'_> {
    fn object_membership(&self, ids: &[ObjectId]) -> Result<(MissingBitmap, Vec<Option<u64>>)> {
        self.0.object_membership(ids)
    }

    fn missing_facts(&self, facts: &[Fact]) -> Result<MissingBitmap> {
        self.0.missing_facts(facts)
    }

    fn admit_objects(&self, objects: &[CanonicalObject]) -> Result<AdmissionSetReceipt> {
        self.0.admit_objects(objects)
    }

    fn admit_facts(&self, facts: &[Fact]) -> Result<AdmissionSetReceipt> {
        self.0.admit_facts(facts)
    }
}

pub struct TransferPipeline<'a> {
    target: &'a dyn TransferTarget,
    objects: Vec<CanonicalObject>,
    object_bytes: usize,
    receipt: TransferReceipt,
}

impl<'a> TransferPipeline<'a> {
    pub fn new(target: &'a dyn TransferTarget) -> Result<Self> {
        Ok(Self {
            target,
            objects: Vec::with_capacity(OBJECT_BATCH_COUNT),
            object_bytes: 0,
            receipt: TransferReceipt {
                objects: TransferSetReceipt::exact_bytes(),
                ..TransferReceipt::default()
            },
        })
    }

    pub fn announce_objects(&mut self, ids: &[ObjectId]) -> Result<MissingBitmap> {
        if ids.len() > ID_BATCH_COUNT || ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(StorageError::InvalidInput("object membership page"));
        }
        note_push_endpoint_call();
        let started = Instant::now();
        let membership = self.target.object_membership(ids);
        note_push_phase(PushPhase::Membership, elapsed_ns(started));
        let (missing, lengths) = membership?;
        missing.validate_tail(ids.len())?;
        if lengths.len() != ids.len()
            || lengths.iter().enumerate().any(|(index, length)| {
                missing
                    .is_missing(index)
                    .is_ok_and(|is_missing| is_missing != length.is_none())
            })
        {
            return Err(StorageError::Integrity("object membership lengths"));
        }
        self.receipt.membership_pages += 1;
        self.receipt.objects.announced_ids += ids.len() as u64;
        self.receipt
            .objects
            .announced_bytes
            .add(lengths.into_iter().flatten().sum());
        self.receipt.objects.missing_ids += missing.count();
        Ok(missing)
    }

    pub fn stage_object(&mut self, object: CanonicalObject) -> Result<()> {
        layerfs_content::authenticate_identity(&object.bytes, object.id)?;
        if !self.objects.is_empty()
            && (self.objects.len() == OBJECT_BATCH_COUNT
                || self.object_bytes + object.bytes.len() > OBJECT_BATCH_BYTES)
        {
            self.flush_objects()?;
        }
        if object.bytes.len() > layerfs_content::limits::MAX_OBJECT_BYTES {
            return Err(StorageError::InvalidInput("object size"));
        }
        self.object_bytes += object.bytes.len();
        self.objects.push(object);
        if self.objects.len() == OBJECT_BATCH_COUNT || self.object_bytes >= OBJECT_BATCH_BYTES {
            self.flush_objects()?;
        }
        Ok(())
    }

    pub(crate) fn observe_external_buffer(&mut self, bytes: usize) -> Result<()> {
        let combined = bytes
            .checked_add(self.object_bytes)
            .and_then(|value| value.checked_add(self.objects.len().saturating_mul(64)))
            .ok_or(StorageError::Integrity("transfer buffer ceiling"))?
            as u64;
        self.receipt.peak_buffer_bytes = self.receipt.peak_buffer_bytes.max(combined);
        if self.receipt.peak_buffer_bytes >= TRANSFER_BUFFER_BYTES as u64 {
            return Err(StorageError::Integrity("transfer buffer ceiling"));
        }
        Ok(())
    }

    fn flush_objects(&mut self) -> Result<()> {
        if self.objects.is_empty() {
            return Ok(());
        }
        let bytes = self.object_bytes as u64;
        let count = self.objects.len() as u64;
        self.receipt.objects.announced_bytes.add(bytes);
        self.receipt.peak_buffer_bytes = self
            .receipt
            .peak_buffer_bytes
            .max(bytes + count.saturating_mul(64));
        if self.receipt.peak_buffer_bytes >= TRANSFER_BUFFER_BYTES as u64 {
            return Err(StorageError::Integrity("transfer buffer ceiling"));
        }
        note_push_endpoint_call();
        let admission = self.target.admit_objects(&self.objects)?;
        merge_set(&mut self.receipt.objects, count, bytes, admission);
        self.receipt.payload_batches += 1;
        self.objects.clear();
        self.object_bytes = 0;
        Ok(())
    }

    pub fn facts(&mut self, facts: &[Fact]) -> Result<()> {
        for same_kind in facts.chunk_by(|left, right| left.kind() == right.kind()) {
            let kind = same_kind[0].kind();
            for membership_page in same_kind.chunks(ID_BATCH_COUNT) {
                let mut announced = membership_page.to_vec();
                announced.sort_by_key(Fact::id);
                if announced
                    .windows(2)
                    .any(|pair| pair[0].id() == pair[1].id())
                {
                    return Err(StorageError::InvalidInput("duplicate fact"));
                }
                note_push_endpoint_call();
                let started = Instant::now();
                let missing = self.target.missing_facts(&announced);
                note_push_phase(PushPhase::Membership, elapsed_ns(started));
                let missing = missing?;
                missing.validate_tail(announced.len())?;
                self.receipt.membership_pages += 1;
                let set = self
                    .receipt
                    .facts
                    .entry(kind)
                    .or_insert_with(TransferSetReceipt::exact_bytes);
                set.announced_ids += announced.len() as u64;
                set.announced_bytes.add(
                    announced
                        .iter()
                        .map(|fact| fact.encoded_size() as u64)
                        .sum(),
                );
                set.missing_ids += missing.count();
                let missing_ids = announced
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| missing.is_missing(*index).unwrap())
                    .map(|(_, fact)| fact.id())
                    .collect::<std::collections::BTreeSet<_>>();
                let selected = membership_page
                    .iter()
                    .filter(|fact| missing_ids.contains(&fact.id()))
                    .cloned()
                    .collect::<Vec<_>>();
                for batch in fact_batches(&selected)? {
                    let bytes = batch.iter().map(|fact| fact.encoded_size()).sum::<usize>();
                    note_push_endpoint_call();
                    let admission = self.target.admit_facts(batch)?;
                    merge_set(
                        self.receipt
                            .facts
                            .entry(kind)
                            .or_insert_with(TransferSetReceipt::exact_bytes),
                        batch.len() as u64,
                        bytes as u64,
                        admission,
                    );
                    self.receipt.payload_batches += 1;
                    self.receipt.peak_buffer_bytes =
                        self.receipt.peak_buffer_bytes.max(bytes as u64);
                }
            }
        }
        Ok(())
    }

    pub fn prune_complete_root(&mut self) {
        self.receipt.known_roots_pruned += 1;
    }

    pub fn finish(mut self) -> Result<TransferReceipt> {
        self.flush_objects()?;
        self.receipt.validate()?;
        record(StorageReceipt::Transfer(self.receipt.clone()));
        Ok(self.receipt)
    }
}

pub fn transfer_root(
    source: &(impl ObjectSource + ?Sized),
    target: &dyn TransferTarget,
    root: ObjectId,
    known_complete: bool,
) -> Result<TransferReceipt> {
    transfer_roots(
        source,
        target,
        [RootTransferRequest {
            root_id: root,
            known_complete,
        }],
    )
}

pub fn transfer_roots<S, I>(
    source: &S,
    target: &dyn TransferTarget,
    roots: I,
) -> Result<TransferReceipt>
where
    S: ObjectSource + ?Sized,
    I: IntoIterator<Item = RootTransferRequest>,
{
    let mut pipeline = TransferPipeline::new(target)?;
    crate::admission::transfer_root_union(
        source,
        roots
            .into_iter()
            .map(|request| (request.root_id, request.known_complete)),
        &mut pipeline,
    )?;
    pipeline.finish()
}

pub fn transfer_facts(target: &dyn TransferTarget, facts: &[Fact]) -> Result<TransferReceipt> {
    let mut pipeline = TransferPipeline::new(target)?;
    pipeline.facts(facts)?;
    pipeline.finish()
}

fn fact_batches(facts: &[Fact]) -> Result<Vec<&[Fact]>> {
    let mut batches = Vec::new();
    let mut start = 0;
    while start < facts.len() {
        let mut end = start;
        let mut bytes = 0;
        while end < facts.len() && end - start < FACT_BATCH_COUNT {
            let next = facts[end].encoded_size();
            if end > start && bytes + next > FACT_BATCH_BYTES {
                break;
            }
            bytes += next;
            end += 1;
        }
        if end == start {
            return Err(StorageError::InvalidInput("fact size"));
        }
        batches.push(&facts[start..end]);
        start = end;
    }
    Ok(batches)
}

fn merge_set(
    set: &mut TransferSetReceipt,
    sent_ids: u64,
    sent_bytes: u64,
    admission: AdmissionSetReceipt,
) {
    set.sent_ids += sent_ids;
    set.missing_bytes += sent_bytes;
    set.sent_bytes += sent_bytes;
    set.inserted_ids += admission.inserted_ids;
    set.inserted_bytes += admission.inserted_bytes;
    set.raced_existing_ids += admission.raced_existing_ids;
    set.raced_existing_bytes += admission.raced_existing_bytes;
}

thread_local! {
    static RECEIPTS: RefCell<Vec<StorageReceipt>> = const { RefCell::new(Vec::new()) };
    static WORKSPACE_COMMIT: RefCell<Option<WorkspaceCommitReceipt>> = const { RefCell::new(None) };
    static PUSH_PHASES: RefCell<Option<PushPhaseReceipt>> = const { RefCell::new(None) };
}

fn record(receipt: StorageReceipt) {
    RECEIPTS.with(|receipts| receipts.borrow_mut().push(receipt));
}

pub struct WorkspaceCommitTimer(Instant);

impl Drop for WorkspaceCommitTimer {
    fn drop(&mut self) {
        WORKSPACE_COMMIT.with(|current| {
            let Some(mut receipt) = current.borrow_mut().take() else {
                return;
            };
            receipt.total_ns = elapsed_ns(self.0);
            receipt.unattributed_ns = receipt.total_ns.saturating_sub(receipt.attributed_ns());
            if receipt.validate().is_ok() {
                record(StorageReceipt::WorkspaceCommit(receipt));
            }
        });
    }
}

pub fn begin_workspace_commit(mode: CaptureMode) -> Result<WorkspaceCommitTimer> {
    WORKSPACE_COMMIT.with(|current| {
        let mut current = current.borrow_mut();
        if current.is_some() {
            return Err(StorageError::Integrity("nested Workspace Commit timing"));
        }
        *current = Some(WorkspaceCommitReceipt {
            capture_mode: Some(mode),
            ..WorkspaceCommitReceipt::default()
        });
        Ok(WorkspaceCommitTimer(Instant::now()))
    })
}

pub fn note_workspace_commit_phase(phase: WorkspaceCommitPhase, elapsed_ns: u64) {
    WORKSPACE_COMMIT.with(|current| {
        let mut current = current.borrow_mut();
        let Some(receipt) = current.as_mut() else {
            return;
        };
        let target = match phase {
            WorkspaceCommitPhase::PauseFence => &mut receipt.pause_fence_ns,
            WorkspaceCommitPhase::Quiesce => &mut receipt.quiesce_ns,
            WorkspaceCommitPhase::Capture => &mut receipt.capture_ns,
            WorkspaceCommitPhase::CandidatePlan => &mut receipt.candidate_plan_ns,
            WorkspaceCommitPhase::DirtyCompare => &mut receipt.dirty_compare_ns,
            WorkspaceCommitPhase::Content => &mut receipt.content_ns,
            WorkspaceCommitPhase::Namespace => &mut receipt.namespace_ns,
            WorkspaceCommitPhase::CandidateFinish => &mut receipt.candidate_finish_ns,
            WorkspaceCommitPhase::LocalAdmission => &mut receipt.local_admission_ns,
            WorkspaceCommitPhase::CompletenessVerify => &mut receipt.completeness_verify_ns,
            WorkspaceCommitPhase::Publication => &mut receipt.publication_ns,
            WorkspaceCommitPhase::InPlaceRebase => &mut receipt.in_place_rebase_ns,
            WorkspaceCommitPhase::Resume => &mut receipt.resume_ns,
        };
        *target = target.saturating_add(elapsed_ns);
    });
}

pub fn note_workspace_capture(captured_files: u64, captured_bytes: u64) {
    WORKSPACE_COMMIT.with(|current| {
        if let Some(receipt) = current.borrow_mut().as_mut() {
            receipt.captured_files = receipt.captured_files.saturating_add(captured_files);
            receipt.captured_bytes = receipt.captured_bytes.saturating_add(captured_bytes);
        }
    });
}

pub struct PushTimer(Instant);

impl Drop for PushTimer {
    fn drop(&mut self) {
        PUSH_PHASES.with(|current| {
            let Some(mut receipt) = current.borrow_mut().take() else {
                return;
            };
            receipt.total_ns = elapsed_ns(self.0);
            receipt.unattributed_ns = receipt.total_ns.saturating_sub(receipt.attributed_ns());
            if receipt.validate().is_ok() {
                record(StorageReceipt::Push(receipt));
            }
        });
    }
}

pub fn begin_push() -> Result<PushTimer> {
    PUSH_PHASES.with(|current| {
        let mut current = current.borrow_mut();
        if current.is_some() {
            return Err(StorageError::Integrity("nested Push timing"));
        }
        *current = Some(PushPhaseReceipt::default());
        Ok(PushTimer(Instant::now()))
    })
}

pub fn note_push_phase(phase: PushPhase, elapsed_ns: u64) {
    PUSH_PHASES.with(|current| {
        let mut current = current.borrow_mut();
        let Some(receipt) = current.as_mut() else {
            return;
        };
        let target = match phase {
            PushPhase::History => &mut receipt.history_ns,
            PushPhase::Frontier => &mut receipt.frontier_ns,
            PushPhase::Membership => &mut receipt.membership_ns,
            PushPhase::SourceReadAuth => &mut receipt.source_read_auth_ns,
            PushPhase::ObjectAdmission => &mut receipt.object_admission_ns,
            PushPhase::FactAdmission => &mut receipt.fact_admission_ns,
            PushPhase::AuthorityTransitionVerify => &mut receipt.authority_transition_verify_ns,
            PushPhase::Publication => &mut receipt.publication_ns,
            PushPhase::Durability => &mut receipt.durability_ns,
        };
        *target = target.saturating_add(elapsed_ns);
    });
}

pub fn note_push_endpoint_call() {
    PUSH_PHASES.with(|current| {
        if let Some(receipt) = current.borrow_mut().as_mut() {
            receipt.endpoint_calls = receipt.endpoint_calls.saturating_add(1);
        }
    });
}

pub fn record_database(receipt: DatabaseReceipt) -> Result<()> {
    receipt.validate()?;
    record(StorageReceipt::Database(receipt));
    Ok(())
}

pub fn record_workspace_lifecycle(receipt: WorkspaceLifecycleReceipt) -> Result<()> {
    receipt.validate()?;
    record(StorageReceipt::WorkspaceLifecycle(receipt));
    Ok(())
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

pub fn record_local_admission(receipt: LocalAdmissionReceipt) -> Result<()> {
    receipt.validate()?;
    record(StorageReceipt::Local(receipt));
    Ok(())
}

pub fn record_durability(receipt: DurabilityReceipt) -> Result<()> {
    receipt.validate()?;
    note_push_phase(PushPhase::Durability, receipt.stable_ns);
    record(StorageReceipt::Durability(receipt));
    Ok(())
}

pub fn take_storage_receipts() -> Vec<StorageReceipt> {
    RECEIPTS.with(|receipts| std::mem::take(&mut *receipts.borrow_mut()))
}

pub fn take_transfer_receipts() -> Vec<TransferReceipt> {
    take_storage_receipts()
        .into_iter()
        .filter_map(|receipt| match receipt {
            StorageReceipt::Transfer(receipt) => Some(receipt),
            StorageReceipt::Local(_)
            | StorageReceipt::Durability(_)
            | StorageReceipt::WorkspaceCommit(_)
            | StorageReceipt::Push(_)
            | StorageReceipt::Database(_)
            | StorageReceipt::WorkspaceLifecycle(_) => None,
        })
        .collect()
}

pub fn receipt_totals(receipts: &[TransferReceipt]) -> BTreeMap<FactKind, TransferSetReceipt> {
    let mut totals = BTreeMap::new();
    for receipt in receipts {
        for (kind, set) in &receipt.facts {
            let total: &mut TransferSetReceipt = totals
                .entry(*kind)
                .or_insert_with(TransferSetReceipt::exact_bytes);
            total.announced_ids += set.announced_ids;
            total.announced_bytes.merge(set.announced_bytes);
            total.missing_ids += set.missing_ids;
            total.missing_bytes += set.missing_bytes;
            total.sent_ids += set.sent_ids;
            total.sent_bytes += set.sent_bytes;
            total.inserted_ids += set.inserted_ids;
            total.inserted_bytes += set.inserted_bytes;
            total.raced_existing_ids += set.raced_existing_ids;
            total.raced_existing_bytes += set.raced_existing_bytes;
        }
    }
    totals
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn transfer_equation_rejects_missing_payload() {
        let invalid = TransferSetReceipt {
            announced_ids: 1,
            missing_ids: 1,
            ..TransferSetReceipt::default()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn local_and_transfer_byte_equations_are_explicit() {
        let local = LocalObjectReceipt {
            candidate_ids: 2,
            candidate_bytes: 20,
            inserted_ids: 1,
            inserted_bytes: 12,
            reused_ids: 1,
            reused_bytes: 8,
        };
        local.validate().unwrap();

        LocalAdmissionReceipt {
            objects: local,
            source_reused_ids: 1,
            source_reused_bytes: 8,
            ..LocalAdmissionReceipt::default()
        }
        .validate()
        .unwrap();

        let transfer = TransferSetReceipt {
            announced_ids: 2,
            announced_bytes: MeasuredBytes::Exact(20),
            missing_ids: 1,
            missing_bytes: 12,
            sent_ids: 1,
            sent_bytes: 12,
            inserted_ids: 1,
            inserted_bytes: 12,
            raced_existing_ids: 0,
            raced_existing_bytes: 0,
        };
        transfer.validate().unwrap();
        assert_eq!(transfer.preexisting_ids(), 1);
        assert_eq!(transfer.preexisting_bytes(), MeasuredBytes::Exact(8));
        assert_eq!(
            TransferSetReceipt::default().preexisting_bytes(),
            MeasuredBytes::Unavailable
        );
    }

    #[test]
    fn object_visitor_streams_without_materializing_the_id_page() {
        struct Source(BTreeMap<ObjectId, Vec<u8>>);

        impl ObjectSource for Source {
            fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
                self.0
                    .get(&id)
                    .cloned()
                    .ok_or(StorageError::MissingObject(id))
            }

            fn read_objects(&self, _ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
                panic!("streaming visitor must not call the materializing batch API")
            }
        }

        let rows = (0..ID_BATCH_COUNT)
            .map(|serial| {
                let bytes = layerfs_content::encode_bytes_object(&serial.to_be_bytes()).unwrap();
                (ObjectId::for_bytes(&bytes), bytes)
            })
            .collect::<BTreeMap<_, _>>();
        let ids = rows.keys().copied().collect::<Vec<_>>();
        let mut visited = Vec::new();
        Source(rows)
            .visit_objects(&ids, &mut |object| {
                visited.push(object.id);
                Ok(())
            })
            .unwrap();
        assert_eq!(visited, ids);
    }
}
