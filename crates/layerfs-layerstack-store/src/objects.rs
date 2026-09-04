use crate::{Result, StoreError};
use layerfs_content::filesystem::{self, ContentChange, ReconcileConflict};
use layerfs_content::object::access::{ObjectRead, ObjectStore};
use layerfs_content::object::references::referenced_objects;
use layerfs_content::{CoreError, CoreResult, ObjectId};
use rusqlite::{params_from_iter, types::Value, Connection, OptionalExtension};
#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub const OBJECT_PAGE_COUNT: usize = 128;
pub const OBJECT_PAGE_BYTES: usize = 4 * 1024 * 1024;
pub const ADMISSION_BATCH_COUNT: usize = OBJECT_PAGE_COUNT - 1;
pub const ADMISSION_BATCH_BYTES: usize = OBJECT_PAGE_BYTES - 1;
pub(crate) const INITIALIZATION_ADMISSION_BATCH_COUNT: usize = 8191;
pub(crate) const INITIALIZATION_SLAB_BYTES: usize = 256 * 1024;
pub(crate) const INITIALIZATION_SLAB_OBJECTS: usize = 512;
pub(crate) const INITIALIZATION_SLAB_QUEUE_SLOTS: usize = 4;
pub(crate) const INITIALIZATION_TASK_STRUCTURAL_BYTES: usize = 256 * 1024;
const CANDIDATE_MEMORY_BYTES: usize = 8 * 1024 * 1024;
// ponytail: about one million candidate IDs covers the 100k-file tier; move to
// an on-disk hash index only when a larger measured campaign exceeds this bound.
const CANDIDATE_INDEX_BYTES: usize = 64 * 1024 * 1024;
const CANDIDATE_SPILL_BUFFER_BYTES: usize = 1024 * 1024;

#[cfg(feature = "test-instrumentation")]
thread_local! {
    static READ_BATCH_COUNTERS: std::cell::RefCell<ReadBatchCounters> = const {
        std::cell::RefCell::new(ReadBatchCounters {
            unique_hashes: 0,
            cloned_bytes: 0,
        })
    };
}

#[cfg(feature = "test-instrumentation")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReadBatchCounters {
    pub unique_hashes: u64,
    pub cloned_bytes: u64,
}

#[cfg(feature = "test-instrumentation")]
pub fn reset_read_batch_counters() {
    READ_BATCH_COUNTERS.with(|counters| *counters.borrow_mut() = ReadBatchCounters::default());
}

#[cfg(feature = "test-instrumentation")]
pub fn read_batch_counters() -> ReadBatchCounters {
    READ_BATCH_COUNTERS.with(|counters| *counters.borrow())
}

#[cfg(feature = "test-instrumentation")]
fn note_read_batch_hash() {
    READ_BATCH_COUNTERS.with(|counters| {
        counters.borrow_mut().unique_hashes += 1;
    });
}

#[cfg(not(feature = "test-instrumentation"))]
fn note_read_batch_hash() {}

#[cfg(feature = "test-instrumentation")]
fn note_read_batch_clone(bytes: usize) {
    READ_BATCH_COUNTERS.with(|counters| {
        let mut counters = counters.borrow_mut();
        counters.cloned_bytes = counters.cloned_bytes.saturating_add(bytes as u64);
    });
}

#[cfg(not(feature = "test-instrumentation"))]
fn note_read_batch_clone(_bytes: usize) {}

thread_local! {
    static PARENT_PAYLOAD_COPY_BYTES: std::cell::Cell<Option<u64>> = const {
        std::cell::Cell::new(None)
    };
}

pub(crate) struct ParentPayloadCopyCounter(Option<u64>);

impl ParentPayloadCopyCounter {
    pub(crate) fn start() -> Self {
        Self(PARENT_PAYLOAD_COPY_BYTES.with(|bytes| bytes.replace(Some(0))))
    }

    pub(crate) fn bytes(&self) -> u64 {
        PARENT_PAYLOAD_COPY_BYTES.with(|bytes| bytes.get().unwrap_or(0))
    }
}

impl Drop for ParentPayloadCopyCounter {
    fn drop(&mut self) {
        PARENT_PAYLOAD_COPY_BYTES.with(|bytes| bytes.set(self.0));
    }
}

fn note_parent_payload_copy(bytes: usize) {
    PARENT_PAYLOAD_COPY_BYTES.with(|total| {
        if let Some(current) = total.get() {
            total.set(Some(current.saturating_add(bytes as u64)));
        }
    });
}

#[derive(Debug, Eq, PartialEq)]
pub struct CanonicalObject {
    pub id: ObjectId,
    pub bytes: Vec<u8>,
}

impl Clone for CanonicalObject {
    fn clone(&self) -> Self {
        note_parent_payload_copy(self.bytes.len());
        Self {
            id: self.id,
            bytes: self.bytes.clone(),
        }
    }
}

pub(crate) struct InitializationObjectSlab {
    pub objects: Vec<CanonicalObject>,
    pub payload_bytes: usize,
}

pub(crate) struct InitializationTaskObjectBuffer {
    objects: Vec<CanonicalObject>,
    payload_bytes: usize,
}

impl InitializationTaskObjectBuffer {
    pub(crate) fn new() -> Self {
        Self {
            objects: Vec::with_capacity(128),
            payload_bytes: 0,
        }
    }

    pub(crate) fn explicit_owned_bytes(&self) -> u64 {
        self.payload_bytes as u64
            + (self.objects.capacity() * std::mem::size_of::<CanonicalObject>()) as u64
    }

    pub(crate) fn hash_invocations(&self) -> u64 {
        self.objects.len() as u64
    }

    pub(crate) fn move_into(self, store: &mut impl ObjectStore) -> CoreResult<()> {
        for object in self.objects {
            if store.put_owned(object.bytes)? != object.id {
                return Err(CoreError::IdentityMismatch);
            }
        }
        Ok(())
    }

    fn push_owned(&mut self, canonical: Vec<u8>) -> CoreResult<ObjectId> {
        let owned = self
            .payload_bytes
            .checked_add(canonical.len())
            .and_then(|payload| {
                payload.checked_add(
                    self.objects
                        .len()
                        .checked_add(1)?
                        .checked_mul(std::mem::size_of::<CanonicalObject>())?,
                )
            })
            .ok_or(CoreError::LengthOverflow)?;
        if owned > INITIALIZATION_TASK_STRUCTURAL_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        let id = ObjectId::for_bytes(&canonical);
        self.payload_bytes = self
            .payload_bytes
            .checked_add(canonical.len())
            .ok_or(CoreError::LengthOverflow)?;
        self.objects.push(CanonicalObject {
            id,
            bytes: canonical,
        });
        Ok(id)
    }
}

impl ObjectStore for InitializationTaskObjectBuffer {
    fn get(&self, _id: ObjectId) -> CoreResult<Vec<u8>> {
        Err(CoreError::InvalidRecord("direct structural get"))
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        self.push_owned(canonical.to_vec())
    }

    fn put_owned(&mut self, canonical: Vec<u8>) -> CoreResult<ObjectId> {
        self.push_owned(canonical)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct InitializationSlabWriterMetrics {
    pub handoffs: u64,
    pub objects: u64,
    pub payload_bytes: u64,
    pub payload_capacity_bytes: u64,
    pub canonical_hash_calls: u64,
    pub blocked_ns: u64,
    pub partial_peak_objects: u64,
    pub partial_peak_payload_bytes: u64,
    pub candidate_copy_bytes: u64,
    pub parent_payload_copy_bytes: u64,
    pub structural_peak_bytes: u64,
    pub producer_wall_ns: u64,
    pub producer_completion_offset_ns: u64,
    pub producer_tasks: u64,
    pub producer_files: u64,
    pub producer_bytes: u64,
}

#[derive(Default)]
pub(crate) struct InitializationSlabQueueMetrics {
    queued: AtomicU64,
    queued_bytes: AtomicU64,
    peak: AtomicU64,
    peak_bytes: AtomicU64,
}

impl InitializationSlabQueueMetrics {
    fn before_send(&self, bytes: usize) {
        let queued = self.queued.fetch_add(1, Ordering::AcqRel) + 1;
        let queued_bytes =
            self.queued_bytes.fetch_add(bytes as u64, Ordering::AcqRel) + bytes as u64;
        self.peak.fetch_max(
            queued.min(INITIALIZATION_SLAB_QUEUE_SLOTS as u64),
            Ordering::Relaxed,
        );
        self.peak_bytes.fetch_max(
            queued_bytes.min((INITIALIZATION_SLAB_QUEUE_SLOTS * INITIALIZATION_SLAB_BYTES) as u64),
            Ordering::Relaxed,
        );
    }

    fn send_failed(&self, bytes: usize) {
        self.received(bytes);
    }

    pub(crate) fn received(&self, bytes: usize) {
        self.queued.fetch_sub(1, Ordering::AcqRel);
        self.queued_bytes.fetch_sub(bytes as u64, Ordering::AcqRel);
    }

    pub(crate) fn peak(&self) -> u64 {
        self.peak.load(Ordering::Relaxed)
    }

    pub(crate) fn peak_bytes(&self) -> u64 {
        self.peak_bytes.load(Ordering::Relaxed)
    }
}

pub(crate) struct InitializationSlabWriter {
    sender: std::sync::mpsc::SyncSender<InitializationObjectSlab>,
    queue: std::sync::Arc<InitializationSlabQueueMetrics>,
    objects: Vec<CanonicalObject>,
    payload_bytes: usize,
    metrics: InitializationSlabWriterMetrics,
}

pub(crate) struct InitializationDirectAdmissionWriter<'admission, 'db> {
    admission: &'admission mut InitializationSegmentAdmission<'db>,
    error: Option<StoreError>,
    transient_owned_bytes: u64,
    pub metrics: InitializationSlabWriterMetrics,
}

impl<'admission, 'db> InitializationDirectAdmissionWriter<'admission, 'db> {
    pub(crate) fn new(admission: &'admission mut InitializationSegmentAdmission<'db>) -> Self {
        Self {
            admission,
            error: None,
            transient_owned_bytes: 0,
            metrics: InitializationSlabWriterMetrics::default(),
        }
    }

    pub(crate) fn error(&mut self, fallback: CoreError) -> StoreError {
        self.error.take().unwrap_or_else(|| fallback.into())
    }

    fn push_owned(&mut self, canonical: Vec<u8>, copied: bool) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(&canonical);
        self.metrics.canonical_hash_calls += 1;
        let bytes = canonical.len() as u64;
        self.metrics.payload_capacity_bytes = self
            .metrics
            .payload_capacity_bytes
            .saturating_add(canonical.capacity() as u64);
        self.admission
            .observe_final_owned_bytes(self.transient_owned_bytes, canonical.capacity() as u64);
        if let Err(error) = self.admission.admit_object(CanonicalObject {
            id,
            bytes: canonical,
        }) {
            self.error = Some(error);
            return Err(CoreError::Io);
        }
        self.admission
            .observe_final_owned_bytes(self.transient_owned_bytes, 0);
        self.metrics.objects += 1;
        self.metrics.payload_bytes = self.metrics.payload_bytes.saturating_add(bytes);
        if copied {
            self.metrics.candidate_copy_bytes =
                self.metrics.candidate_copy_bytes.saturating_add(bytes);
        }
        Ok(id)
    }
}

impl ObjectStore for InitializationDirectAdmissionWriter<'_, '_> {
    fn get(&self, _id: ObjectId) -> CoreResult<Vec<u8>> {
        Err(CoreError::InvalidRecord("direct initialization get"))
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        self.push_owned(canonical.to_vec(), true)
    }

    fn put_owned(&mut self, canonical: Vec<u8>) -> CoreResult<ObjectId> {
        self.push_owned(canonical, false)
    }

    fn note_transient_owned_bytes(&mut self, bytes: u64) -> CoreResult<()> {
        self.transient_owned_bytes = bytes;
        self.admission.observe_final_owned_bytes(bytes, 0);
        Ok(())
    }
}

impl InitializationSlabWriter {
    pub(crate) fn new(
        sender: std::sync::mpsc::SyncSender<InitializationObjectSlab>,
        queue: std::sync::Arc<InitializationSlabQueueMetrics>,
    ) -> Self {
        Self {
            sender,
            queue,
            objects: Vec::with_capacity(INITIALIZATION_SLAB_OBJECTS),
            payload_bytes: 0,
            metrics: InitializationSlabWriterMetrics::default(),
        }
    }

    pub(crate) fn finish(mut self) -> Result<InitializationSlabWriterMetrics> {
        self.flush()?;
        Ok(self.metrics)
    }

    pub(crate) fn note_hash_invocations(&mut self, calls: u64) {
        self.metrics.canonical_hash_calls = self.metrics.canonical_hash_calls.saturating_add(calls);
    }

    fn flush(&mut self) -> Result<()> {
        if self.objects.is_empty() {
            return Ok(());
        }
        let slab = InitializationObjectSlab {
            objects: std::mem::take(&mut self.objects),
            payload_bytes: std::mem::take(&mut self.payload_bytes),
        };
        self.queue.before_send(slab.payload_bytes);
        match self.sender.try_send(slab) {
            Ok(()) => {}
            Err(std::sync::mpsc::TrySendError::Full(slab)) => {
                let started = Instant::now();
                if let Err(error) = self.sender.send(slab) {
                    self.queue.send_failed(error.0.payload_bytes);
                    return Err(StoreError::Integrity("initialization slab receiver"));
                }
                self.metrics.blocked_ns =
                    self.metrics.blocked_ns.saturating_add(elapsed_ns(started));
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(slab)) => {
                self.queue.send_failed(slab.payload_bytes);
                return Err(StoreError::Integrity("initialization slab receiver"));
            }
        }
        self.metrics.handoffs += 1;
        self.objects = Vec::with_capacity(INITIALIZATION_SLAB_OBJECTS);
        Ok(())
    }

    fn push_owned(&mut self, canonical: Vec<u8>, copied: bool) -> CoreResult<ObjectId> {
        if canonical.len() > INITIALIZATION_SLAB_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        if !self.objects.is_empty()
            && (self.objects.len() == INITIALIZATION_SLAB_OBJECTS
                || self.payload_bytes.saturating_add(canonical.len()) > INITIALIZATION_SLAB_BYTES)
        {
            self.flush().map_err(|_| CoreError::Io)?;
        }
        let id = ObjectId::for_bytes(&canonical);
        self.metrics.canonical_hash_calls += 1;
        self.payload_bytes += canonical.len();
        self.metrics.objects += 1;
        self.metrics.payload_bytes = self
            .metrics
            .payload_bytes
            .saturating_add(canonical.len() as u64);
        self.metrics.payload_capacity_bytes = self
            .metrics
            .payload_capacity_bytes
            .saturating_add(canonical.capacity() as u64);
        if copied {
            self.metrics.candidate_copy_bytes = self
                .metrics
                .candidate_copy_bytes
                .saturating_add(canonical.len() as u64);
        }
        self.objects.push(CanonicalObject {
            id,
            bytes: canonical,
        });
        self.metrics.partial_peak_objects = self
            .metrics
            .partial_peak_objects
            .max(self.objects.len() as u64);
        self.metrics.partial_peak_payload_bytes = self
            .metrics
            .partial_peak_payload_bytes
            .max(self.payload_bytes as u64);
        Ok(id)
    }
}

impl ObjectStore for InitializationSlabWriter {
    fn get(&self, _id: ObjectId) -> CoreResult<Vec<u8>> {
        Err(CoreError::InvalidRecord("direct initialization get"))
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        self.push_owned(canonical.to_vec(), true)
    }

    fn put_owned(&mut self, canonical: Vec<u8>) -> CoreResult<ObjectId> {
        self.push_owned(canonical, false)
    }
}

pub trait ObjectSource: Send + Sync {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>>;

    fn read_authenticated_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        ids.iter()
            .map(|id| {
                let bytes = self.read_object(*id)?;
                layerfs_content::authenticate_identity(&bytes, *id)?;
                Ok(CanonicalObject { id: *id, bytes })
            })
            .collect()
    }
}

pub struct CoreReader<'a>(pub &'a dyn ObjectSource);

impl ObjectRead for CoreReader<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.0.read_object(id).map_err(core_read_error)
    }

    fn get_authenticated_batch<F>(&self, ids: &[ObjectId], mut callback: F) -> CoreResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
    {
        if ids.len() > OBJECT_PAGE_COUNT {
            return Err(CoreError::InvalidRecord("object read page"));
        }
        let objects = self
            .0
            .read_authenticated_objects(ids)
            .map_err(core_read_error)?;
        if objects.len() != ids.len() {
            return Err(CoreError::MissingObject);
        }
        for (expected, object) in ids.iter().zip(objects) {
            if object.id != *expected {
                return Err(CoreError::IdentityMismatch);
            }
            callback(
                object.id,
                layerfs_content::decode_bytes_object(&object.bytes)?,
            )?;
        }
        Ok(())
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> CoreResult<T>
    where
        F: FnOnce(&[u8]) -> CoreResult<T>,
    {
        let mut objects = self
            .0
            .read_authenticated_objects(&[id])
            .map_err(core_read_error)?;
        if objects.len() != 1 {
            return Err(CoreError::MissingObject);
        }
        let object = objects.pop().expect("authenticated object");
        if object.id != id {
            return Err(CoreError::IdentityMismatch);
        }
        callback(&object.bytes)
    }
}

fn core_read_error(error: StoreError) -> CoreError {
    match error {
        StoreError::MissingObject(_) => CoreError::MissingObject,
        StoreError::Integrity(message) | StoreError::InvalidInput(message) => {
            CoreError::InvalidRecord(message)
        }
        StoreError::StoreMissing => CoreError::ValidationAuthorityUnavailable,
        StoreError::WrongStoreSchema => CoreError::SchemaMismatch,
        StoreError::CommitHeadMoved { .. }
        | StoreError::LayerHeadMoved { .. }
        | StoreError::LayerStackNameConflict { .. }
        | StoreError::BranchNameConflict { .. } => CoreError::PublicationConflict,
        StoreError::Core(error) => error,
        StoreError::StoreBusy
        | StoreError::StoreAlreadyExists
        | StoreError::NotFound(_)
        | StoreError::Database(_)
        | StoreError::Io(_) => CoreError::Io,
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildCounters {
    pub cdc_bytes_scanned: u64,
    pub encode_hash_invocations: u64,
    pub first_store_write_bytes: u64,
    pub reachable_copy_write_bytes: u64,
    pub spill_peak_bytes: u64,
    pub spill_count: u64,
}

pub struct BuiltRoot {
    pub root_id: ObjectId,
    pub objects: DeferredObjectStore,
    pub counters: BuildCounters,
}

pub struct CandidateReconciliation {
    pub root_id: ObjectId,
    pub objects: DeferredObjectStore,
    pub conflicts: Vec<ReconcileConflict>,
}

pub fn reconcile_candidate(
    source: &dyn ObjectSource,
    base_root: ObjectId,
    current_root: ObjectId,
    candidate_root: ObjectId,
) -> Result<CandidateReconciliation> {
    reconcile_candidate_with(source, base_root, current_root, candidate_root, |_| None)
}

pub fn reconcile_candidate_with(
    source: &dyn ObjectSource,
    base_root: ObjectId,
    current_root: ObjectId,
    candidate_root: ObjectId,
    choice: impl FnMut(
        &layerfs_content::filesystem::ReconcileConflict,
    ) -> Option<layerfs_content::filesystem::ReconcileChoice>,
) -> Result<CandidateReconciliation> {
    let mut objects = ObjectBuffer::new(source)?;
    let reconciled = filesystem::reconcile_with(
        &mut objects,
        base_root,
        current_root,
        candidate_root,
        choice,
    )?;
    let built = objects.finish(reconciled.root_id, 0)?;
    Ok(CandidateReconciliation {
        root_id: reconciled.root_id,
        objects: built.objects,
        conflicts: reconciled.conflicts,
    })
}

pub fn apply_reconcile_choices(
    source: &dyn ObjectSource,
    working_root: ObjectId,
    branch_root: ObjectId,
    layer_root: ObjectId,
    conflicts: &[ReconcileConflict],
    choices: &[filesystem::ReconcileChoice],
) -> Result<BuiltRoot> {
    if conflicts.len() != choices.len() {
        return Err(StoreError::InvalidInput("reconciliation choice count"));
    }
    let mut objects = ObjectBuffer::new(source)?;
    let mut root = working_root;
    for (conflict, choice) in conflicts.iter().zip(choices) {
        let selected_root = match choice {
            filesystem::ReconcileChoice::Branch => branch_root,
            filesystem::ReconcileChoice::Layer => layer_root,
            filesystem::ReconcileChoice::WorkingTree => continue,
        };
        root = filesystem::replace_conflict_from_snapshot(
            &mut objects,
            root,
            selected_root,
            conflict,
        )?;
    }
    objects.finish(root, 0)
}

pub struct DeferredObjectStore {
    storage: DeferredObjects,
    reachable: IdOrder,
    references: Option<BTreeMap<ObjectId, Vec<ObjectId>>>,
    reference_bytes: usize,
    count: u64,
    encoded_bytes: u64,
    first_store_write_bytes: u64,
    spill_peak_bytes: u64,
    spill_count: u64,
}

#[cfg(test)]
pub(crate) struct AppendOnlyInitializationWriter {
    writer: std::fs::File,
    reader: std::fs::File,
    path: TempPath,
    pending: Vec<u8>,
    pending_limit: usize,
    end: u64,
    objects: u64,
    bytes: u64,
    write_calls: u64,
    write_bytes: u64,
    get_calls: Cell<u64>,
}

#[cfg(test)]
pub(crate) struct AppendOnlyInitializationSegment {
    reader: Option<BufReader<CountedFile>>,
    unbuffered_reader: Option<CountedFile>,
    reader_capacity: usize,
    _path: TempPath,
    cursor: u64,
    end: u64,
    objects: u64,
    bytes: u64,
    read_objects: u64,
    read_bytes: u64,
    write_calls: u64,
    write_bytes: u64,
    get_calls: u64,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InitializationTaskBlock {
    pub task_ordinal: usize,
    pub worker_index: usize,
    pub start: u64,
    pub end: u64,
    pub object_count: u64,
    pub byte_count: u64,
}

pub(crate) struct CompactInodePairWriter {
    writer: std::fs::File,
    reader: std::fs::File,
    path: TempPath,
    pending: Vec<u8>,
    pending_limit: usize,
    end: u64,
    pairs: u64,
    write_calls: u64,
    write_bytes: u64,
}

pub(crate) struct CompactInodePairSegment {
    reader: BufReader<CountedFile>,
    _path: TempPath,
    cursor: u64,
    end: u64,
    pairs: u64,
    read_pairs: u64,
    write_calls: u64,
    write_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CompactInodePairBlock {
    pub task_ordinal: usize,
    pub worker_index: usize,
    pub start: u64,
    pub end: u64,
    pub pair_count: u64,
}

pub(crate) struct CompactInodePairStream {
    segments: Vec<CompactInodePairSegment>,
    blocks: std::vec::IntoIter<CompactInodePairBlock>,
    current: Option<(CompactInodePairBlock, u64)>,
    last_task: Option<usize>,
    done: bool,
}

struct CountedFile {
    file: std::fs::File,
    reads: u64,
    bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct InitializationSegmentIoMetrics {
    pub frames: u64,
    pub payload_bytes: u64,
    pub framing_bytes: u64,
    pub write_calls: u64,
    pub write_bytes: u64,
    pub raw_read_calls: u64,
    pub raw_read_bytes: u64,
    pub passes: u64,
}

impl Read for CountedFile {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reads = self.reads.saturating_add(1);
        let bytes = self.file.read(buffer)?;
        self.bytes = self.bytes.saturating_add(bytes as u64);
        Ok(bytes)
    }
}

enum DeferredObjects {
    Memory {
        order: Vec<ObjectId>,
        rows: BTreeMap<ObjectId, Vec<u8>>,
        bytes: usize,
    },
    Spill(SpillObjects),
}

struct SpillObjects {
    writer: Option<std::fs::File>,
    reader: Mutex<std::fs::File>,
    path: PathBuf,
    pending: Vec<u8>,
    pending_index: BTreeMap<ObjectId, (usize, usize)>,
    end: u64,
    index: Option<BTreeMap<ObjectId, (u64, u64)>>,
    index_bytes: usize,
}

impl Drop for SpillObjects {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

enum IdOrder {
    Memory(Vec<ObjectId>),
    Spill { file: std::fs::File, path: TempPath },
}

struct TempPath(PathBuf);

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
impl AppendOnlyInitializationWriter {
    pub(crate) fn new(pending_limit: usize) -> Result<Self> {
        if pending_limit == 0 {
            return Err(StoreError::InvalidInput("initialization segment buffer"));
        }
        let (writer, path) = temporary_file("initialization-segment")?;
        let reader = std::fs::File::open(&path)?;
        Ok(Self {
            writer,
            reader,
            path: TempPath(path),
            pending: Vec::with_capacity(pending_limit),
            pending_limit,
            end: 0,
            objects: 0,
            bytes: 0,
            write_calls: 0,
            write_bytes: 0,
            get_calls: Cell::new(0),
        })
    }

    pub(crate) fn checkpoint(&self) -> (u64, u64, u64) {
        (self.end, self.objects, self.bytes)
    }

    pub(crate) fn block_since(
        &self,
        task_ordinal: usize,
        worker_index: usize,
        checkpoint: (u64, u64, u64),
    ) -> Result<InitializationTaskBlock> {
        let (start, objects, bytes) = checkpoint;
        Ok(InitializationTaskBlock {
            task_ordinal,
            worker_index,
            start,
            end: self.end,
            object_count: self
                .objects
                .checked_sub(objects)
                .ok_or(StoreError::Integrity("initialization segment objects"))?,
            byte_count: self
                .bytes
                .checked_sub(bytes)
                .ok_or(StoreError::Integrity("initialization segment bytes"))?,
        })
    }

    pub(crate) fn get_calls(&self) -> u64 {
        self.get_calls.get()
    }

    pub(crate) fn seal(mut self) -> Result<AppendOnlyInitializationSegment> {
        self.flush()?;
        if self.reader.metadata()?.len() != self.end {
            return Err(StoreError::Integrity("initialization segment length"));
        }
        let Self {
            writer,
            mut reader,
            path,
            end,
            objects,
            bytes,
            write_calls,
            write_bytes,
            get_calls,
            pending_limit,
            ..
        } = self;
        drop(writer);
        reader.seek(SeekFrom::Start(0))?;
        #[cfg(unix)]
        std::fs::remove_file(&path.0)?;
        Ok(AppendOnlyInitializationSegment {
            reader: None,
            unbuffered_reader: Some(CountedFile {
                file: reader,
                reads: 0,
                bytes: 0,
            }),
            reader_capacity: pending_limit,
            _path: path,
            cursor: 0,
            end,
            objects,
            bytes,
            read_objects: 0,
            read_bytes: 0,
            write_calls,
            write_bytes,
            get_calls: get_calls.get(),
        })
    }

    fn append(&mut self, id: ObjectId, canonical: &[u8]) -> Result<()> {
        let row_len = canonical
            .len()
            .checked_add(40)
            .ok_or(StoreError::Integrity("initialization segment length"))?;
        if !self.pending.is_empty()
            && self.pending.len().saturating_add(row_len) > self.pending_limit
        {
            self.flush()?;
        }
        self.pending.extend_from_slice(id.as_bytes());
        self.pending
            .extend_from_slice(&(canonical.len() as u64).to_le_bytes());
        self.pending.extend_from_slice(canonical);
        self.end = self
            .end
            .checked_add(row_len as u64)
            .ok_or(StoreError::Integrity("initialization segment length"))?;
        self.objects = self
            .objects
            .checked_add(1)
            .ok_or(StoreError::Integrity("initialization segment objects"))?;
        self.bytes = self
            .bytes
            .checked_add(canonical.len() as u64)
            .ok_or(StoreError::Integrity("initialization segment bytes"))?;
        if self.pending.len() >= self.pending_limit {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if !self.pending.is_empty() {
            self.writer.write_all(&self.pending)?;
            self.write_calls = self.write_calls.saturating_add(1);
            self.write_bytes = self.write_bytes.saturating_add(self.pending.len() as u64);
            self.pending.clear();
        }
        Ok(())
    }
}

#[cfg(test)]
impl ObjectStore for AppendOnlyInitializationWriter {
    fn get(&self, _id: ObjectId) -> CoreResult<Vec<u8>> {
        self.get_calls.set(self.get_calls.get().saturating_add(1));
        Err(CoreError::InvalidRecord("append-only initialization get"))
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        self.append(id, canonical).map_err(|_| CoreError::Io)?;
        Ok(id)
    }
}

#[cfg(test)]
impl AppendOnlyInitializationSegment {
    fn reader(&mut self) -> Result<&mut BufReader<CountedFile>> {
        if self.reader.is_none() {
            let reader = self
                .unbuffered_reader
                .take()
                .ok_or(StoreError::Integrity("initialization segment reader"))?;
            self.reader = Some(BufReader::with_capacity(self.reader_capacity, reader));
        }
        self.reader
            .as_mut()
            .ok_or(StoreError::Integrity("initialization segment reader"))
    }

    pub(crate) fn consume_block(
        &mut self,
        block: InitializationTaskBlock,
        mut visitor: impl FnMut(CanonicalObject) -> Result<()>,
    ) -> Result<()> {
        if block.start != self.cursor || block.end > self.end || block.start > block.end {
            return Err(StoreError::Integrity("initialization segment block order"));
        }
        let before_objects = self.read_objects;
        let before_bytes = self.read_bytes;
        while self.cursor < block.end {
            let mut id = [0; 32];
            self.reader()?.read_exact(&mut id)?;
            let id = ObjectId::from_bytes(&id)?;
            let mut length = [0; 8];
            self.reader()?.read_exact(&mut length)?;
            let length = usize::try_from(u64::from_le_bytes(length))
                .map_err(|_| StoreError::Integrity("initialization segment object length"))?;
            let next = self
                .cursor
                .checked_add(40)
                .and_then(|cursor| cursor.checked_add(length as u64))
                .ok_or(StoreError::Integrity("initialization segment length"))?;
            if next > block.end {
                return Err(StoreError::Integrity("initialization segment block length"));
            }
            let mut bytes = vec![0; length];
            self.reader()?.read_exact(&mut bytes)?;
            self.cursor = next;
            self.read_objects += 1;
            self.read_bytes = self.read_bytes.saturating_add(length as u64);
            visitor(CanonicalObject { id, bytes })?;
        }
        if self.cursor != block.end
            || self.read_objects - before_objects != block.object_count
            || self.read_bytes - before_bytes != block.byte_count
        {
            return Err(StoreError::Integrity("initialization segment block"));
        }
        Ok(())
    }

    pub(crate) fn finish_consumption(self) -> Result<InitializationSegmentIoMetrics> {
        if self.end == 0 {
            let (raw_read_calls, raw_read_bytes) = if let Some(reader) = &self.reader {
                (reader.get_ref().reads, reader.get_ref().bytes)
            } else if let Some(reader) = &self.unbuffered_reader {
                (reader.reads, reader.bytes)
            } else {
                return Err(StoreError::Integrity("initialization segment reader"));
            };
            if self.cursor != 0
                || self.objects != 0
                || self.bytes != 0
                || self.read_objects != 0
                || self.read_bytes != 0
                || self.write_calls != 0
                || self.write_bytes != 0
                || raw_read_calls != 0
                || raw_read_bytes != 0
                || self.get_calls != 0
            {
                return Err(StoreError::Integrity("initialization segment consumption"));
            }
            return Ok(InitializationSegmentIoMetrics::default());
        }
        let reader = self
            .reader
            .ok_or(StoreError::Integrity("initialization segment reader"))?;
        if self.cursor != self.end
            || self.read_objects != self.objects
            || self.read_bytes != self.bytes
            || self.end != self.bytes.saturating_add(self.objects.saturating_mul(40))
            || self.write_bytes != self.end
            || reader.get_ref().bytes != self.end
            || self.get_calls != 0
        {
            return Err(StoreError::Integrity("initialization segment consumption"));
        }
        Ok(InitializationSegmentIoMetrics {
            frames: self.objects,
            payload_bytes: self.bytes,
            framing_bytes: self.objects.saturating_mul(40),
            write_calls: self.write_calls,
            write_bytes: self.write_bytes,
            raw_read_calls: reader.get_ref().reads,
            raw_read_bytes: reader.get_ref().bytes,
            passes: 1,
        })
    }

    #[cfg(test)]
    fn path(&self) -> &std::path::Path {
        &self._path.0
    }

    #[cfg(test)]
    pub(crate) fn reader_capacity(&self) -> usize {
        self.reader
            .as_ref()
            .map_or(self.reader_capacity, BufReader::capacity)
    }

    #[cfg(test)]
    fn raw_reads(&self) -> u64 {
        self.reader
            .as_ref()
            .map_or(0, |reader| reader.get_ref().reads)
    }

    #[cfg(test)]
    fn raw_read_bytes(&self) -> u64 {
        self.reader
            .as_ref()
            .map_or(0, |reader| reader.get_ref().bytes)
    }
}

impl CompactInodePairWriter {
    pub(crate) fn new(pending_limit: usize) -> Result<Self> {
        if pending_limit < 64 {
            return Err(StoreError::InvalidInput("inode pair segment buffer"));
        }
        let (writer, path) = temporary_file("initialization-inode-pairs")?;
        let reader = std::fs::File::open(&path)?;
        Ok(Self {
            writer,
            reader,
            path: TempPath(path),
            pending: Vec::with_capacity(pending_limit),
            pending_limit,
            end: 0,
            pairs: 0,
            write_calls: 0,
            write_bytes: 0,
        })
    }

    pub(crate) fn checkpoint(&self) -> (u64, u64) {
        (self.end, self.pairs)
    }

    pub(crate) fn push(
        &mut self,
        inode: layerfs_content::tree::inode::InodeId,
        record: ObjectId,
    ) -> Result<()> {
        if !self.pending.is_empty() && self.pending.len() + 64 > self.pending_limit {
            self.flush()?;
        }
        self.pending.extend_from_slice(inode.as_bytes());
        self.pending.extend_from_slice(record.as_bytes());
        self.end = self
            .end
            .checked_add(64)
            .ok_or(StoreError::Integrity("inode pair segment length"))?;
        self.pairs = self
            .pairs
            .checked_add(1)
            .ok_or(StoreError::Integrity("inode pair segment count"))?;
        if self.pending.len() >= self.pending_limit {
            self.flush()?;
        }
        Ok(())
    }

    pub(crate) fn block_since(
        &self,
        task_ordinal: usize,
        worker_index: usize,
        checkpoint: (u64, u64),
    ) -> Result<CompactInodePairBlock> {
        let (start, pairs) = checkpoint;
        Ok(CompactInodePairBlock {
            task_ordinal,
            worker_index,
            start,
            end: self.end,
            pair_count: self
                .pairs
                .checked_sub(pairs)
                .ok_or(StoreError::Integrity("inode pair segment count"))?,
        })
    }

    pub(crate) fn seal(mut self) -> Result<CompactInodePairSegment> {
        self.flush()?;
        if self.reader.metadata()?.len() != self.end {
            return Err(StoreError::Integrity("inode pair segment length"));
        }
        let Self {
            writer,
            mut reader,
            path,
            end,
            pairs,
            write_calls,
            write_bytes,
            pending_limit,
            ..
        } = self;
        drop(writer);
        reader.seek(SeekFrom::Start(0))?;
        #[cfg(unix)]
        std::fs::remove_file(&path.0)?;
        Ok(CompactInodePairSegment {
            reader: BufReader::with_capacity(
                pending_limit,
                CountedFile {
                    file: reader,
                    reads: 0,
                    bytes: 0,
                },
            ),
            _path: path,
            cursor: 0,
            end,
            pairs,
            read_pairs: 0,
            write_calls,
            write_bytes,
        })
    }

    #[cfg(test)]
    pub(crate) fn pending_capacity(&self) -> usize {
        self.pending.capacity()
    }

    fn flush(&mut self) -> Result<()> {
        if !self.pending.is_empty() {
            self.writer.write_all(&self.pending)?;
            self.write_calls = self.write_calls.saturating_add(1);
            self.write_bytes = self.write_bytes.saturating_add(self.pending.len() as u64);
            self.pending.clear();
        }
        Ok(())
    }
}

impl CompactInodePairSegment {
    fn read_pair(
        &mut self,
        block_end: u64,
    ) -> Result<(layerfs_content::tree::inode::InodeId, ObjectId)> {
        let next = self
            .cursor
            .checked_add(64)
            .ok_or(StoreError::Integrity("inode pair segment length"))?;
        if next > block_end {
            return Err(StoreError::Integrity("inode pair block length"));
        }
        let mut pair = [0; 64];
        self.reader.read_exact(&mut pair)?;
        self.cursor = next;
        self.read_pairs += 1;
        Ok((
            layerfs_content::tree::inode::InodeId::from_slice(&pair[..32])?,
            ObjectId::from_bytes(&pair[32..])?,
        ))
    }

    fn consumed(&self) -> bool {
        self.cursor == self.end && self.read_pairs == self.pairs
    }

    #[cfg(test)]
    fn path(&self) -> &std::path::Path {
        &self._path.0
    }

    #[cfg(test)]
    pub(crate) fn reader_capacity(&self) -> usize {
        self.reader.capacity()
    }

    #[cfg(test)]
    fn raw_reads(&self) -> u64 {
        self.reader.get_ref().reads
    }

    #[cfg(test)]
    fn raw_read_bytes(&self) -> u64 {
        self.reader.get_ref().bytes
    }
}

impl CompactInodePairStream {
    pub(crate) fn new(
        segments: Vec<CompactInodePairSegment>,
        blocks: Vec<CompactInodePairBlock>,
    ) -> Result<Self> {
        if blocks.len() > 1_000
            || blocks.iter().enumerate().any(|(task, block)| {
                block.task_ordinal != task
                    || block.worker_index >= segments.len()
                    || block.start > block.end
                    || block.pair_count.checked_mul(64) != Some(block.end - block.start)
            })
        {
            return Err(StoreError::Integrity("inode pair block order"));
        }
        Ok(Self {
            segments,
            blocks: blocks.into_iter(),
            current: None,
            last_task: None,
            done: false,
        })
    }

    fn fail(
        &mut self,
        error: StoreError,
    ) -> Option<CoreResult<(layerfs_content::tree::inode::InodeId, ObjectId)>> {
        self.done = true;
        Some(Err(core_read_error(error)))
    }

    pub(crate) fn finish(self) -> Result<InitializationSegmentIoMetrics> {
        if !self.done
            || self.current.is_some()
            || self.blocks.len() != 0
            || !self.segments.iter().all(CompactInodePairSegment::consumed)
        {
            return Err(StoreError::Integrity("inode pair segment consumption"));
        }
        let mut metrics = InitializationSegmentIoMetrics::default();
        for segment in self.segments {
            if segment.end != segment.pairs.saturating_mul(64)
                || segment.write_bytes != segment.end
                || segment.reader.get_ref().bytes != segment.end
            {
                return Err(StoreError::Integrity("inode pair segment consumption"));
            }
            metrics.frames = metrics.frames.saturating_add(segment.pairs);
            metrics.payload_bytes = metrics.payload_bytes.saturating_add(segment.end);
            metrics.write_calls = metrics.write_calls.saturating_add(segment.write_calls);
            metrics.write_bytes = metrics.write_bytes.saturating_add(segment.write_bytes);
            metrics.raw_read_calls = metrics
                .raw_read_calls
                .saturating_add(segment.reader.get_ref().reads);
            metrics.raw_read_bytes = metrics
                .raw_read_bytes
                .saturating_add(segment.reader.get_ref().bytes);
            metrics.passes = metrics.passes.saturating_add(1);
        }
        Ok(metrics)
    }
}

impl Iterator for CompactInodePairStream {
    type Item = CoreResult<(layerfs_content::tree::inode::InodeId, ObjectId)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        loop {
            if let Some((block, read)) = self.current {
                let segment = match self.segments.get_mut(block.worker_index) {
                    Some(segment) => segment,
                    None => {
                        return self.fail(StoreError::Integrity("inode pair segment worker"));
                    }
                };
                if segment.cursor == block.end {
                    if read != block.pair_count {
                        return self.fail(StoreError::Integrity("inode pair block count"));
                    }
                    self.last_task = Some(block.task_ordinal);
                    self.current = None;
                    continue;
                }
                match segment.read_pair(block.end) {
                    Ok(pair) => {
                        self.current = Some((block, read + 1));
                        return Some(Ok(pair));
                    }
                    Err(error) => return self.fail(error),
                }
            }

            let Some(block) = self.blocks.next() else {
                self.done = true;
                if self.segments.iter().all(CompactInodePairSegment::consumed) {
                    return None;
                }
                return Some(Err(CoreError::InvalidRecord(
                    "inode pair segment consumption",
                )));
            };
            if self
                .last_task
                .is_some_and(|task| block.task_ordinal != task + 1)
                || self
                    .segments
                    .get(block.worker_index)
                    .is_none_or(|segment| segment.cursor != block.start)
            {
                return self.fail(StoreError::Integrity("inode pair block order"));
            }
            self.current = Some((block, 0));
        }
    }
}

impl IdOrder {
    fn empty() -> Self {
        Self::Memory(Vec::new())
    }

    fn push(&mut self, id: ObjectId) -> Result<()> {
        if matches!(self, Self::Memory(ids) if (ids.len() + 1) * 32 > CANDIDATE_INDEX_BYTES) {
            let Self::Memory(ids) = std::mem::replace(self, Self::Memory(Vec::new())) else {
                unreachable!()
            };
            let (mut file, path) = temporary_file("candidate-order")?;
            for id in ids {
                file.write_all(id.as_bytes())?;
            }
            *self = Self::Spill {
                file,
                path: TempPath(path),
            };
        }
        match self {
            Self::Memory(ids) => ids.push(id),
            Self::Spill { file, .. } => file.write_all(id.as_bytes())?,
        }
        Ok(())
    }

    fn visit(&self, mut visitor: impl FnMut(ObjectId) -> Result<()>) -> Result<()> {
        match self {
            Self::Memory(ids) => {
                for id in ids {
                    visitor(*id)?;
                }
            }
            Self::Spill { path, .. } => {
                let mut file = std::fs::File::open(&path.0)?;
                let mut bytes = [0; 32];
                loop {
                    match file.read_exact(&mut bytes) {
                        Ok(()) => visitor(ObjectId::from_bytes(&bytes)?)?,
                        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
                        Err(error) => return Err(error.into()),
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct SpillableObjectSet {
    storage: SeenStorage,
    count: usize,
}

pub(crate) struct CandidatePlan {
    missing: SpillableObjectSet,
    missing_order: IdOrder,
    all_missing: bool,
    pub candidate_objects: u64,
    pub candidate_bytes: u64,
    pub inserted_objects: u64,
    pub inserted_bytes: u64,
    pub reused_objects: u64,
    pub reused_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ObjectInsertMetrics {
    pub payload_ns: u64,
    pub insert_ns: u64,
    pub objects: u64,
    pub bytes: u64,
    pub submitted_rows: u64,
    pub returned_ids: u64,
    pub skipped_ids: u64,
    pub skipped_bytes: u64,
    pub collision_checks: u64,
    pub sql_string_build_ns: u64,
    pub sql_prepare_ns: u64,
    pub sql_bind_step_returning_ns: u64,
    pub conflict_read_calls: u64,
    pub conflict_read_rows: u64,
    pub conflict_read_bytes: u64,
    pub conflict_read_ns: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct InitializationAdmissionDiagnostics {
    pub pending_duplicate_objects: u64,
    pub pending_duplicate_bytes: u64,
    pub cross_batch_skipped_objects: u64,
    pub cross_batch_skipped_bytes: u64,
    pub collision_checks: u64,
    pub batch_peak_objects: u64,
    pub batch_peak_payload_bytes: u64,
    pub batch_peak_vec_capacity: u64,
    pub pending_index_peak_entries: u64,
    pub pending_index_peak_bytes: u64,
    pub final_batch_peak_payload_bytes: u64,
    pub final_batch_peak_vec_capacity: u64,
    pub final_pending_index_peak_bytes: u64,
    pub final_simultaneous_owned_peak_bytes: u64,
    pub sql_batch_count: u64,
    pub sql_row_shapes: BTreeSet<u64>,
    pub sql_submitted_rows: u64,
    pub sql_returned_ids: u64,
    pub sql_skipped_ids: u64,
    pub sql_string_build_ns: u64,
    pub sql_prepare_ns: u64,
    pub sql_bind_step_returning_ns: u64,
    pub conflict_read_calls: u64,
    pub conflict_read_rows: u64,
    pub conflict_read_bytes: u64,
    pub conflict_read_ns: u64,
    pub sql_begin_ns: u64,
    pub sql_commit_ns: u64,
    pub pipeline_commit_count: u64,
    pub pipeline_commit_ns: u64,
    pub pipeline_commit_max_ns: u64,
    pub pipeline_commit_max_ordinal: u64,
    pub final_build_commit_count: u64,
    pub final_build_commit_ns: u64,
    pub final_build_commit_max_ns: u64,
    pub final_build_commit_max_ordinal: u64,
    pub publication_commit_ns: u64,
}

#[derive(Clone, Copy)]
pub(crate) enum InitializationSqlPhase {
    Pipeline,
    FinalBuild,
    Publication,
}

impl InitializationAdmissionDiagnostics {
    pub(crate) fn record_sql_batch(
        &mut self,
        metrics: ObjectInsertMetrics,
        begin_ns: u64,
        commit_ns: u64,
        phase: InitializationSqlPhase,
    ) {
        if metrics.submitted_rows != 0 {
            self.sql_batch_count += 1;
            self.sql_row_shapes.insert(metrics.submitted_rows);
        }
        self.cross_batch_skipped_objects = self
            .cross_batch_skipped_objects
            .saturating_add(metrics.skipped_ids);
        self.cross_batch_skipped_bytes = self
            .cross_batch_skipped_bytes
            .saturating_add(metrics.skipped_bytes);
        self.collision_checks = self
            .collision_checks
            .saturating_add(metrics.collision_checks);
        self.sql_submitted_rows = self
            .sql_submitted_rows
            .saturating_add(metrics.submitted_rows);
        self.sql_returned_ids = self.sql_returned_ids.saturating_add(metrics.returned_ids);
        self.sql_skipped_ids = self.sql_skipped_ids.saturating_add(metrics.skipped_ids);
        self.sql_string_build_ns = self
            .sql_string_build_ns
            .saturating_add(metrics.sql_string_build_ns);
        self.sql_prepare_ns = self.sql_prepare_ns.saturating_add(metrics.sql_prepare_ns);
        self.sql_bind_step_returning_ns = self
            .sql_bind_step_returning_ns
            .saturating_add(metrics.sql_bind_step_returning_ns);
        self.conflict_read_calls = self
            .conflict_read_calls
            .saturating_add(metrics.conflict_read_calls);
        self.conflict_read_rows = self
            .conflict_read_rows
            .saturating_add(metrics.conflict_read_rows);
        self.conflict_read_bytes = self
            .conflict_read_bytes
            .saturating_add(metrics.conflict_read_bytes);
        self.conflict_read_ns = self
            .conflict_read_ns
            .saturating_add(metrics.conflict_read_ns);
        self.sql_begin_ns = self.sql_begin_ns.saturating_add(begin_ns);
        self.sql_commit_ns = self.sql_commit_ns.saturating_add(commit_ns);
        let ordinal = self.sql_batch_count;
        match phase {
            InitializationSqlPhase::Pipeline => {
                self.pipeline_commit_count += 1;
                self.pipeline_commit_ns = self.pipeline_commit_ns.saturating_add(commit_ns);
                if commit_ns > self.pipeline_commit_max_ns {
                    self.pipeline_commit_max_ns = commit_ns;
                    self.pipeline_commit_max_ordinal = ordinal;
                }
            }
            InitializationSqlPhase::FinalBuild => {
                self.final_build_commit_count += 1;
                self.final_build_commit_ns = self.final_build_commit_ns.saturating_add(commit_ns);
                if commit_ns > self.final_build_commit_max_ns {
                    self.final_build_commit_max_ns = commit_ns;
                    self.final_build_commit_max_ordinal = ordinal;
                }
            }
            InitializationSqlPhase::Publication => {
                self.publication_commit_ns = commit_ns;
            }
        }
    }
}

pub(crate) struct PlannedAdmission {
    pub final_batch: Vec<CanonicalObject>,
    pub batch_inserted_objects: u64,
    pub batch_inserted_bytes: u64,
    pub transactions: u64,
    pub max_transaction_objects: u64,
    pub max_transaction_bytes: u64,
    pub begin_ns: u64,
    pub insert_ns: u64,
    pub commit_ns: u64,
}

struct AdmissionBatchMetrics {
    insert: ObjectInsertMetrics,
    begin_ns: u64,
    commit_ns: u64,
}

pub(crate) struct InitializationSegmentAdmission<'a> {
    db: &'a crate::schema::StoreDb,
    batch: Vec<CanonicalObject>,
    pending: HashMap<ObjectId, usize>,
    batch_bytes: usize,
    statement_number: u64,
    receipt: crate::CandidateReceipt,
    diagnostics: InitializationAdmissionDiagnostics,
    final_phase: bool,
}

pub(crate) struct FinishedInitializationAdmission {
    pub final_batch: Vec<CanonicalObject>,
    pub statement_number: u64,
    pub receipt: crate::CandidateReceipt,
    pub diagnostics: InitializationAdmissionDiagnostics,
}

enum SeenStorage {
    Memory(BTreeSet<ObjectId>),
    Spill { file: std::fs::File, path: TempPath },
}

impl SpillableObjectSet {
    pub fn empty() -> Result<Self> {
        Ok(Self {
            storage: SeenStorage::Memory(BTreeSet::new()),
            count: 0,
        })
    }

    pub fn contains(&self, id: ObjectId) -> Result<bool> {
        match &self.storage {
            SeenStorage::Memory(ids) => Ok(ids.contains(&id)),
            SeenStorage::Spill { path, .. } => scan_id_file(&path.0, id),
        }
    }

    pub fn insert_page(&mut self, ids: &[ObjectId]) -> Result<Vec<ObjectId>> {
        let mut inserted = Vec::new();
        for id in ids {
            if self.contains(*id)? {
                continue;
            }
            if matches!(&self.storage, SeenStorage::Memory(_) if (self.count + 1) * 48 > CANDIDATE_INDEX_BYTES)
            {
                let SeenStorage::Memory(known) =
                    std::mem::replace(&mut self.storage, SeenStorage::Memory(BTreeSet::new()))
                else {
                    unreachable!()
                };
                let (mut file, path) = temporary_file("candidate-seen")?;
                for known in known {
                    file.write_all(known.as_bytes())?;
                }
                self.storage = SeenStorage::Spill {
                    file,
                    path: TempPath(path),
                };
            }
            match &mut self.storage {
                SeenStorage::Memory(known) => {
                    known.insert(*id);
                }
                SeenStorage::Spill { file, .. } => {
                    // ponytail: exact spill fallback is O(n²); add a bounded on-disk hash
                    // index only if candidate-ID counts make this measurable.
                    file.write_all(id.as_bytes())?;
                }
            }
            self.count += 1;
            inserted.push(*id);
        }
        Ok(inserted)
    }
}

impl DeferredObjectStore {
    pub fn new() -> Result<Self> {
        Self::with_reference_index(true)
    }

    pub(crate) fn new_all_reachable() -> Result<Self> {
        Self::with_reference_index(false)
    }

    fn with_reference_index(reference_index: bool) -> Result<Self> {
        Ok(Self {
            storage: DeferredObjects::Memory {
                order: Vec::new(),
                rows: BTreeMap::new(),
                bytes: 0,
            },
            reachable: IdOrder::empty(),
            references: reference_index.then(BTreeMap::new),
            reference_bytes: 0,
            count: 0,
            encoded_bytes: 0,
            first_store_write_bytes: 0,
            spill_peak_bytes: 0,
            spill_count: 0,
        })
    }

    pub fn len(&self) -> u64 {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn encoded_bytes(&self) -> u64 {
        self.encoded_bytes
    }

    pub fn has_reference_index(&self) -> bool {
        self.references.is_some()
    }

    pub fn cached_references(&self, id: ObjectId) -> Option<Vec<ObjectId>> {
        self.references
            .as_ref()
            .and_then(|references| references.get(&id).cloned())
    }

    pub fn ids_in_order(&self, limit: usize) -> Result<Option<Vec<ObjectId>>> {
        if self.count > limit as u64 {
            return Ok(None);
        }
        let mut ids = Vec::with_capacity(self.count as usize);
        self.reachable.visit(|id| {
            ids.push(id);
            Ok(())
        })?;
        Ok(Some(ids))
    }

    fn order_missing(&self, missing: &SpillableObjectSet) -> Result<IdOrder> {
        let mut output = IdOrder::empty();
        let mut count = 0_usize;
        let mut push = |id| {
            if missing.contains(id)? {
                output.push(id)?;
                count += 1;
            }
            Ok(())
        };
        match &self.storage {
            DeferredObjects::Memory { order, .. } => {
                for id in order {
                    push(*id)?;
                }
            }
            DeferredObjects::Spill(spill) => spill.visit_ids(&mut push)?,
        }
        if count != missing.count {
            return Err(StoreError::Integrity("candidate publication order"));
        }
        Ok(output)
    }

    fn visit_prevalidated_order(
        &self,
        order: &IdOrder,
        visitor: &mut dyn FnMut(ObjectId, &[u8]) -> Result<()>,
    ) -> Result<()> {
        match &self.storage {
            DeferredObjects::Memory { rows, .. } => {
                order.visit(|id| visitor(id, rows.get(&id).ok_or(StoreError::MissingObject(id))?))
            }
            DeferredObjects::Spill(spill) => spill.visit_ordered(order, visitor),
        }
    }

    #[cfg(test)]
    fn consume_prevalidated_pages(
        mut self,
        mut visitor: impl FnMut(Vec<CanonicalObject>) -> Result<()>,
    ) -> Result<()> {
        let mut page = Vec::with_capacity(INITIALIZATION_ADMISSION_BATCH_COUNT);
        let mut page_bytes = 0_usize;
        let mut push = |object: CanonicalObject| {
            if !page.is_empty()
                && (page.len() == INITIALIZATION_ADMISSION_BATCH_COUNT
                    || page_bytes.saturating_add(object.bytes.len()) > ADMISSION_BATCH_BYTES)
            {
                visitor(std::mem::replace(
                    &mut page,
                    Vec::with_capacity(INITIALIZATION_ADMISSION_BATCH_COUNT),
                ))?;
                page_bytes = 0;
            }
            page_bytes = page_bytes.saturating_add(object.bytes.len());
            page.push(object);
            Ok(())
        };
        match &mut self.storage {
            DeferredObjects::Memory { rows, .. } => self.reachable.visit(|id| {
                push(CanonicalObject {
                    id,
                    bytes: rows.remove(&id).ok_or(StoreError::MissingObject(id))?,
                })
            })?,
            DeferredObjects::Spill(spill) => {
                spill.visit_ordered(&self.reachable, &mut |id, bytes| {
                    push(CanonicalObject {
                        id,
                        bytes: bytes.to_vec(),
                    })
                })?;
            }
        }
        if !page.is_empty() {
            visitor(page)?;
        }
        Ok(())
    }

    pub fn visit_batches(
        &self,
        visitor: &mut dyn FnMut(&[CanonicalObject], bool) -> Result<()>,
    ) -> Result<()> {
        let mut batch = Vec::with_capacity(OBJECT_PAGE_COUNT);
        let mut bytes = 0_usize;
        self.reachable.visit(|id| {
            let object = CanonicalObject {
                id,
                bytes: self.get(id)?.ok_or(StoreError::MissingObject(id))?,
            };
            if !batch.is_empty()
                && (batch.len() == OBJECT_PAGE_COUNT
                    || bytes + object.bytes.len() > OBJECT_PAGE_BYTES)
            {
                visitor(&batch, false)?;
                batch.clear();
                bytes = 0;
            }
            bytes += object.bytes.len();
            batch.push(object);
            Ok(())
        })?;
        if !batch.is_empty() {
            visitor(&batch, true)?;
        }
        Ok(())
    }

    fn visit_membership_batches(
        &self,
        mut visitor: impl FnMut(&[(ObjectId, u64)]) -> Result<()>,
    ) -> Result<()> {
        let mut batch = Vec::with_capacity(OBJECT_PAGE_COUNT);
        self.reachable.visit(|id| {
            batch.push((id, self.encoded_length(id)?));
            if batch.len() == OBJECT_PAGE_COUNT {
                visitor(&batch)?;
                batch.clear();
            }
            Ok(())
        })?;
        if !batch.is_empty() {
            visitor(&batch)?;
        }
        Ok(())
    }

    fn reachable_from(mut self, root: ObjectId) -> Result<Self> {
        if let DeferredObjects::Spill(spill) = &mut self.storage {
            spill.flush()?;
        }
        let mut seen = SpillableObjectSet::empty()?;
        seen.insert_page(&[root])?;
        let mut active = BTreeSet::new();
        let mut stack = vec![(root, false)];
        let mut order = IdOrder::empty();
        let mut count = 0_u64;
        let mut encoded_bytes = 0_u64;
        while let Some((id, expanded)) = stack.pop() {
            let length = match self.encoded_length(id) {
                Ok(length) => length,
                Err(StoreError::MissingObject(_)) => continue,
                Err(error) => return Err(error),
            };
            if expanded {
                active.remove(&id);
                order.push(id)?;
                count += 1;
                encoded_bytes = encoded_bytes
                    .checked_add(length)
                    .ok_or(StoreError::Integrity("candidate bytes"))?;
                continue;
            }
            if !active.insert(id) {
                return Err(StoreError::Integrity("object cycle"));
            }
            let children = match self.cached_references(id) {
                Some(children) => children,
                None => {
                    let canonical = self.get(id)?.ok_or(StoreError::MissingObject(id))?;
                    layerfs_content::authenticate_identity(&canonical, id)?;
                    let mut children = referenced_objects(&canonical)?;
                    children.sort();
                    children.dedup();
                    children
                }
            };
            if children.iter().any(|child| active.contains(child)) {
                return Err(StoreError::Integrity("object cycle"));
            }
            let mut inserted = Vec::new();
            for page in children.chunks(OBJECT_PAGE_COUNT) {
                inserted.extend(seen.insert_page(page)?);
            }
            stack.push((id, true));
            stack.extend(inserted.into_iter().rev().map(|child| (child, false)));
        }
        self.reachable = order;
        self.count = count;
        self.encoded_bytes = encoded_bytes;
        if let DeferredObjects::Spill(spill) = &mut self.storage {
            spill.seal()?;
        }
        Ok(self)
    }

    fn retain_references(&mut self, id: ObjectId, children: &[ObjectId]) {
        let Some(references) = self.references.as_mut() else {
            return;
        };
        let charge = 64_usize.saturating_add(children.len().saturating_mul(32));
        if self.reference_bytes.saturating_add(charge) > CANDIDATE_INDEX_BYTES {
            self.references = None;
            self.reference_bytes = 0;
            return;
        }
        references.insert(id, children.to_vec());
        self.reference_bytes += charge;
    }

    fn get(&self, id: ObjectId) -> Result<Option<Vec<u8>>> {
        match &self.storage {
            DeferredObjects::Memory { rows, .. } => Ok(rows.get(&id).cloned()),
            DeferredObjects::Spill(spill) => spill.get(id),
        }
    }

    fn encoded_length(&self, id: ObjectId) -> Result<u64> {
        match &self.storage {
            DeferredObjects::Memory { rows, .. } => rows
                .get(&id)
                .map(|bytes| bytes.len() as u64)
                .ok_or(StoreError::MissingObject(id)),
            DeferredObjects::Spill(spill) => spill.encoded_length(id),
        }
    }

    #[cfg(test)]
    fn put(&mut self, id: ObjectId, canonical: &[u8]) -> Result<()> {
        layerfs_content::authenticate_identity(canonical, id)?;
        self.put_prevalidated(id, canonical)
    }

    fn put_prevalidated(&mut self, id: ObjectId, canonical: &[u8]) -> Result<()> {
        self.put_owned(id, canonical.to_vec())
    }

    fn put_owned(&mut self, id: ObjectId, canonical: Vec<u8>) -> Result<()> {
        let known = match &self.storage {
            DeferredObjects::Memory { rows, .. } => rows.get(&id).cloned(),
            DeferredObjects::Spill(_) => self.get(id)?,
        };
        if let Some(known) = known {
            return if known == canonical {
                Ok(())
            } else {
                Err(StoreError::Integrity("candidate object collision"))
            };
        }
        let length = canonical.len();
        let children = if self.references.is_some() {
            let mut children = referenced_objects(&canonical)?;
            children.sort();
            children.dedup();
            Some(children)
        } else {
            None
        };
        let charge = length.saturating_add(64);
        if matches!(&self.storage, DeferredObjects::Memory { bytes, .. } if bytes.saturating_add(charge) > CANDIDATE_MEMORY_BYTES)
        {
            self.spill()?;
        }
        match &mut self.storage {
            DeferredObjects::Memory { order, rows, bytes } => {
                order.push(id);
                rows.insert(id, canonical);
                *bytes += charge;
            }
            DeferredObjects::Spill(spill) => spill.put(id, &canonical)?,
        }
        self.reachable.push(id)?;
        self.count += 1;
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(length as u64)
            .ok_or(StoreError::Integrity("candidate bytes"))?;
        self.first_store_write_bytes = self
            .first_store_write_bytes
            .checked_add(length as u64)
            .ok_or(StoreError::Integrity("candidate first-store bytes"))?;
        if matches!(self.storage, DeferredObjects::Spill(_)) {
            self.spill_peak_bytes = self.spill_peak_bytes.max(self.encoded_bytes);
        }
        if let Some(children) = children {
            self.retain_references(id, &children);
        }
        Ok(())
    }

    fn spill(&mut self) -> Result<()> {
        let DeferredObjects::Memory { order, rows, .. } = std::mem::replace(
            &mut self.storage,
            DeferredObjects::Memory {
                order: Vec::new(),
                rows: BTreeMap::new(),
                bytes: 0,
            },
        ) else {
            return Ok(());
        };
        let (file, path) = temporary_file("candidate-objects")?;
        let reader = std::fs::File::open(&path)?;
        let mut spill = SpillObjects {
            writer: Some(file),
            reader: Mutex::new(reader),
            path,
            pending: Vec::with_capacity(CANDIDATE_SPILL_BUFFER_BYTES),
            pending_index: BTreeMap::new(),
            end: 0,
            index: Some(BTreeMap::new()),
            index_bytes: 0,
        };
        for id in order {
            spill.put(
                id,
                rows.get(&id)
                    .ok_or(StoreError::Integrity("candidate object"))?,
            )?;
        }
        self.storage = DeferredObjects::Spill(spill);
        self.spill_count += 1;
        self.spill_peak_bytes = self.spill_peak_bytes.max(self.encoded_bytes);
        Ok(())
    }

    fn all_reachable(mut self) -> Result<Self> {
        if let DeferredObjects::Spill(spill) = &mut self.storage {
            spill.seal()?;
        }
        Ok(self)
    }
}

impl SpillObjects {
    fn seal(&mut self) -> Result<()> {
        self.flush()?;
        self.writer = None;
        #[cfg(unix)]
        std::fs::remove_file(&self.path)?;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        self.writer
            .as_mut()
            .ok_or(StoreError::Integrity("sealed candidate spool"))?
            .write_all(&self.pending)?;
        self.pending.clear();
        self.pending_index.clear();
        Ok(())
    }

    fn put(&mut self, id: ObjectId, canonical: &[u8]) -> Result<()> {
        let row_len = canonical
            .len()
            .checked_add(40)
            .ok_or(StoreError::Integrity("candidate object length"))?;
        if !self.pending.is_empty()
            && self.pending.len().saturating_add(row_len) > CANDIDATE_SPILL_BUFFER_BYTES
        {
            self.flush()?;
        }
        let start = self.end;
        let pending_offset = self.pending.len() + 40;
        self.pending.extend_from_slice(id.as_bytes());
        self.pending
            .extend_from_slice(&(canonical.len() as u64).to_le_bytes());
        self.pending.extend_from_slice(canonical);
        self.pending_index
            .insert(id, (pending_offset, canonical.len()));
        self.end = self
            .end
            .checked_add(row_len as u64)
            .ok_or(StoreError::Integrity("candidate object length"))?;
        if let Some(index) = &mut self.index {
            if self.index_bytes.saturating_add(64) > CANDIDATE_INDEX_BYTES {
                self.index = None;
                self.index_bytes = 0;
            } else {
                index.insert(id, (start + 40, canonical.len() as u64));
                self.index_bytes += 64;
            }
        }
        if self.pending.len() >= CANDIDATE_SPILL_BUFFER_BYTES {
            self.flush()?;
        }
        Ok(())
    }

    fn visit_ids(&self, visitor: &mut dyn FnMut(ObjectId) -> Result<()>) -> Result<()> {
        let mut file = self
            .reader
            .lock()
            .map_err(|_| StoreError::Integrity("candidate spool lock"))?;
        file.seek(SeekFrom::Start(0))?;
        loop {
            let mut object_id = [0; 32];
            match file.read_exact(&mut object_id) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
                Err(error) => return Err(error.into()),
            }
            let mut length = [0; 8];
            file.read_exact(&mut length)?;
            visitor(ObjectId::from_bytes(&object_id)?)?;
            file.seek(SeekFrom::Current(
                i64::try_from(u64::from_le_bytes(length))
                    .map_err(|_| StoreError::Integrity("candidate object length"))?,
            ))?;
        }
    }

    fn visit_ordered(
        &self,
        order: &IdOrder,
        visitor: &mut dyn FnMut(ObjectId, &[u8]) -> Result<()>,
    ) -> Result<()> {
        let mut file = self
            .reader
            .lock()
            .map_err(|_| StoreError::Integrity("candidate spool lock"))?;
        file.seek(SeekFrom::Start(0))?;
        let mut canonical = Vec::new();
        order.visit(|expected| loop {
            let mut object_id = [0; 32];
            match file.read_exact(&mut object_id) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Err(StoreError::MissingObject(expected));
                }
                Err(error) => return Err(error.into()),
            }
            let mut length = [0; 8];
            file.read_exact(&mut length)?;
            let length = usize::try_from(u64::from_le_bytes(length))
                .map_err(|_| StoreError::Integrity("candidate object length"))?;
            if object_id == *expected.as_bytes() {
                if length > OBJECT_PAGE_BYTES {
                    return Err(StoreError::InvalidInput("candidate object page"));
                }
                canonical.resize(length, 0);
                file.read_exact(&mut canonical)?;
                return visitor(expected, &canonical);
            }
            file.seek(SeekFrom::Current(
                i64::try_from(length)
                    .map_err(|_| StoreError::Integrity("candidate object length"))?,
            ))?;
        })
    }

    fn get(&self, id: ObjectId) -> Result<Option<Vec<u8>>> {
        if let Some((offset, length)) = self.pending_index.get(&id) {
            return Ok(Some(self.pending[*offset..*offset + *length].to_vec()));
        }
        let mut file = self
            .reader
            .lock()
            .map_err(|_| StoreError::Integrity("candidate spool lock"))?;
        if let Some(index) = &self.index {
            let Some((offset, length)) = index.get(&id) else {
                return Ok(None);
            };
            file.seek(SeekFrom::Start(*offset))?;
            let mut bytes = vec![
                0;
                usize::try_from(*length).map_err(|_| StoreError::Integrity(
                    "candidate object length"
                ))?
            ];
            file.read_exact(&mut bytes)?;
            return Ok(Some(bytes));
        }
        file.seek(SeekFrom::Start(0))?;
        loop {
            let mut object_id = [0; 32];
            match file.read_exact(&mut object_id) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(error) => return Err(error.into()),
            }
            let mut length = [0; 8];
            file.read_exact(&mut length)?;
            let length = u64::from_le_bytes(length);
            if object_id == *id.as_bytes() {
                let mut bytes = vec![
                    0;
                    usize::try_from(length).map_err(|_| StoreError::Integrity(
                        "candidate object length"
                    ))?
                ];
                file.read_exact(&mut bytes)?;
                return Ok(Some(bytes));
            }
            file.seek(SeekFrom::Current(
                i64::try_from(length)
                    .map_err(|_| StoreError::Integrity("candidate object length"))?,
            ))?;
        }
    }

    fn encoded_length(&self, id: ObjectId) -> Result<u64> {
        if let Some(index) = &self.index {
            return index
                .get(&id)
                .map(|(_, length)| *length)
                .ok_or(StoreError::MissingObject(id));
        }
        let mut file = self
            .reader
            .lock()
            .map_err(|_| StoreError::Integrity("candidate spool lock"))?;
        file.seek(SeekFrom::Start(0))?;
        loop {
            let mut object_id = [0; 32];
            match file.read_exact(&mut object_id) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Err(StoreError::MissingObject(id));
                }
                Err(error) => return Err(error.into()),
            }
            let mut length = [0; 8];
            file.read_exact(&mut length)?;
            let length = u64::from_le_bytes(length);
            if object_id == *id.as_bytes() {
                return Ok(length);
            }
            file.seek(SeekFrom::Current(
                i64::try_from(length)
                    .map_err(|_| StoreError::Integrity("candidate object length"))?,
            ))?;
        }
    }
}

pub struct ObjectBuffer<'a> {
    source: Option<&'a dyn ObjectSource>,
    objects: DeferredObjectStore,
}

impl<'a> ObjectBuffer<'a> {
    pub fn new(source: &'a dyn ObjectSource) -> Result<Self> {
        Ok(Self {
            source: Some(source),
            objects: DeferredObjectStore::new()?,
        })
    }

    pub fn empty() -> Result<Self> {
        Ok(Self {
            source: None,
            objects: DeferredObjectStore::new()?,
        })
    }

    pub(crate) fn empty_all_reachable() -> Result<Self> {
        Ok(Self {
            source: None,
            objects: DeferredObjectStore::new_all_reachable()?,
        })
    }

    #[doc(hidden)]
    pub fn resume_prevalidated(source: &'a dyn ObjectSource, objects: DeferredObjectStore) -> Self {
        Self {
            source: Some(source),
            objects,
        }
    }

    #[doc(hidden)]
    pub fn into_resumable(self) -> DeferredObjectStore {
        self.objects
    }

    #[doc(hidden)]
    pub fn into_prevalidated(self) -> Result<DeferredObjectStore> {
        self.objects.all_reachable()
    }

    pub fn finish(self, root_id: ObjectId, cdc_bytes_scanned: u64) -> Result<BuiltRoot> {
        let encode_hash_invocations = self.objects.len();
        let objects = self.objects.reachable_from(root_id)?;
        Ok(BuiltRoot {
            root_id,
            counters: BuildCounters {
                cdc_bytes_scanned,
                encode_hash_invocations,
                first_store_write_bytes: objects.first_store_write_bytes,
                reachable_copy_write_bytes: 0,
                spill_peak_bytes: objects.spill_peak_bytes,
                spill_count: objects.spill_count,
            },
            objects,
        })
    }

    pub(crate) fn finish_all_reachable(
        self,
        root_id: ObjectId,
        cdc_bytes_scanned: u64,
    ) -> Result<BuiltRoot> {
        let encode_hash_invocations = self.objects.len();
        let objects = self.objects.all_reachable()?;
        Ok(BuiltRoot {
            root_id,
            counters: BuildCounters {
                cdc_bytes_scanned,
                encode_hash_invocations,
                first_store_write_bytes: objects.first_store_write_bytes,
                reachable_copy_write_bytes: 0,
                spill_peak_bytes: objects.spill_peak_bytes,
                spill_count: objects.spill_count,
            },
            objects,
        })
    }

    pub(crate) fn merge_prevalidated(&mut self, objects: DeferredObjectStore) -> Result<()> {
        objects.visit_prevalidated_order(&objects.reachable, &mut |id, bytes| {
            self.objects.put_prevalidated(id, bytes)
        })
    }
}

impl ObjectStore for ObjectBuffer<'_> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        if let Some(bytes) = self.objects.get(id).map_err(|_| CoreError::Io)? {
            return Ok(bytes);
        }
        self.source
            .ok_or(CoreError::MissingObject)?
            .read_object(id)
            .map_err(core_read_error)
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        self.objects
            .put_prevalidated(id, canonical)
            .map_err(|_| CoreError::Io)?;
        Ok(id)
    }

    fn put_owned(&mut self, canonical: Vec<u8>) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(&canonical);
        self.objects
            .put_owned(id, canonical)
            .map_err(|_| CoreError::Io)?;
        Ok(id)
    }
}

impl ObjectSource for ObjectBuffer<'_> {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        ObjectStore::get(self, id).map_err(StoreError::from)
    }
}

impl ObjectSource for DeferredObjectStore {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        self.get(id)?.ok_or(StoreError::MissingObject(id))
    }
}

pub fn empty_root(seed: [u8; 32]) -> Result<BuiltRoot> {
    let mut store = ObjectBuffer::empty()?;
    let root_id = filesystem::empty_root(&mut store, seed)?;
    store.finish(root_id, 0)
}

pub fn apply_changes(
    source: &dyn ObjectSource,
    base_root: ObjectId,
    changes: &[ContentChange],
    seed: [u8; 32],
) -> Result<BuiltRoot> {
    let mut store = ObjectBuffer::new(source)?;
    let applied = filesystem::apply_changes(&mut store, base_root, changes, seed)?;
    store.finish(applied.root_id, applied.counters.cdc_bytes_scanned)
}

pub(crate) fn combine_candidates(
    root_id: ObjectId,
    candidates: &[&DeferredObjectStore],
) -> Result<DeferredObjectStore> {
    let mut combined = DeferredObjectStore::new()?;
    for candidate in candidates {
        candidate.visit_batches(&mut |batch, _| {
            for object in batch {
                combined.put_prevalidated(object.id, &object.bytes)?;
            }
            Ok(())
        })?;
    }
    combined.reachable_from(root_id)
}

fn temporary_file(label: &str) -> Result<(std::fs::File, PathBuf)> {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir();
    for _ in 0..32 {
        let path = directory.join(format!(
            "layerfs-{label}-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(StoreError::Integrity("candidate temporary file"))
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

fn scan_id_file(path: &std::path::Path, id: ObjectId) -> Result<bool> {
    let mut file = std::fs::File::open(path)?;
    let mut bytes = [0; 32];
    loop {
        match file.read_exact(&mut bytes) {
            Ok(()) if bytes == *id.as_bytes() => return Ok(true),
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(false),
            Err(error) => return Err(error.into()),
        }
    }
}

impl crate::schema::StoreDb {
    pub fn read_object_row(&self, id: ObjectId) -> Result<Vec<u8>> {
        let connection = self.reader()?;
        let mut statement = connection.prepare_cached(crate::statements::objects::GET)?;
        let bytes = statement
            .query_row([id.as_bytes().as_slice()], |row| row.get::<_, Vec<u8>>(0))
            .optional()?
            .ok_or(StoreError::Integrity("visible object missing"))?;
        layerfs_content::authenticate_identity(&bytes, id)?;
        Ok(bytes)
    }

    pub fn read_object_rows(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        let connection = self.reader()?;
        read_object_rows_from_connection(&connection, ids)
    }

    pub fn object_membership(&self, ids: &[ObjectId]) -> Result<BTreeMap<ObjectId, u64>> {
        if ids.len() > OBJECT_PAGE_COUNT {
            return Err(StoreError::InvalidInput("object membership page"));
        }
        if ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut values = ids
            .iter()
            .map(|id| Value::Blob(id.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        values.resize(OBJECT_PAGE_COUNT, Value::Null);
        let connection = self.reader()?;
        let mut statement =
            connection.prepare_cached(crate::statements::objects::MEMBERSHIP_128)?;
        let membership = statement
            .query_map(params_from_iter(values), |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
            })?
            .map(|row| {
                let (id, length) = row?;
                Ok((
                    ObjectId::from_bytes(&id)?,
                    length
                        .try_into()
                        .map_err(|_| StoreError::Integrity("object length"))?,
                ))
            })
            .collect();
        membership
    }

    pub(crate) fn plan_candidate(&self, objects: &DeferredObjectStore) -> Result<CandidatePlan> {
        let mut plan = CandidatePlan {
            missing: SpillableObjectSet::empty()?,
            missing_order: IdOrder::empty(),
            all_missing: false,
            candidate_objects: 0,
            candidate_bytes: 0,
            inserted_objects: 0,
            inserted_bytes: 0,
            reused_objects: 0,
            reused_bytes: 0,
        };
        objects.visit_membership_batches(|batch| {
            let ids = batch.iter().map(|(id, _)| *id).collect::<Vec<_>>();
            let known = self.object_membership(&ids)?;
            let mut missing = Vec::new();
            let mut reused = Vec::new();
            for (id, bytes) in batch {
                plan.candidate_objects += 1;
                plan.candidate_bytes = plan.candidate_bytes.saturating_add(*bytes);
                match known.get(id) {
                    Some(known) if known != bytes => {
                        return Err(StoreError::Integrity("object length collision"));
                    }
                    Some(_) => {
                        reused.push((*id, *bytes));
                    }
                    None => {
                        missing.push(*id);
                        plan.inserted_objects += 1;
                        plan.inserted_bytes = plan.inserted_bytes.saturating_add(*bytes);
                    }
                }
            }
            plan.reused_objects += reused.len() as u64;
            plan.reused_bytes = plan
                .reused_bytes
                .saturating_add(reused.iter().map(|(_, bytes)| *bytes).sum::<u64>());
            plan.missing.insert_page(&missing)?;
            Ok(())
        })?;
        if plan.candidate_objects != plan.inserted_objects + plan.reused_objects
            || plan.candidate_bytes != plan.inserted_bytes + plan.reused_bytes
        {
            return Err(StoreError::Integrity("candidate equation"));
        }
        plan.missing_order = objects.order_missing(&plan.missing)?;
        Ok(plan)
    }

    pub(crate) fn plan_initialization_candidate(
        &self,
        objects: &DeferredObjectStore,
    ) -> Result<CandidatePlan> {
        if !self.initialization_store_is_empty()? {
            return self.plan_candidate(objects);
        }
        Ok(CandidatePlan {
            missing: SpillableObjectSet::empty()?,
            missing_order: IdOrder::empty(),
            all_missing: true,
            candidate_objects: objects.len(),
            candidate_bytes: objects.encoded_bytes(),
            inserted_objects: objects.len(),
            inserted_bytes: objects.encoded_bytes(),
            reused_objects: 0,
            reused_bytes: 0,
        })
    }

    pub(crate) fn initialization_store_is_empty(&self) -> Result<bool> {
        Ok(self.reader()?.query_row(
            "SELECT NOT EXISTS(SELECT 1 FROM objects LIMIT 1)",
            [],
            |row| row.get::<_, bool>(0),
        )?)
    }

    pub(crate) fn clear_failed_direct_initialization(&self) -> Result<()> {
        let mut connection = self.writer()?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        transaction.execute("DELETE FROM objects", [])?;
        transaction.commit()?;
        Ok(())
    }
}

fn read_object_rows_from_connection(
    connection: &Connection,
    ids: &[ObjectId],
) -> Result<Vec<CanonicalObject>> {
    if ids.len() > OBJECT_PAGE_COUNT {
        return Err(StoreError::InvalidInput("object read page"));
    }
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut values = ids
        .iter()
        .map(|id| Value::Blob(id.as_bytes().to_vec()))
        .collect::<Vec<_>>();
    values.resize(OBJECT_PAGE_COUNT, Value::Null);
    let mut statement = connection.prepare_cached(crate::statements::objects::GET_MANY_128)?;
    let mut rows = BTreeMap::new();
    for row in statement.query_map(params_from_iter(values), |row| {
        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
    })? {
        let (id, bytes) = row?;
        let id = ObjectId::from_bytes(&id)?;
        layerfs_content::authenticate_identity(&bytes, id)?;
        note_read_batch_hash();
        if rows.insert(id, bytes).is_some() {
            return Err(StoreError::Integrity("duplicate visible object"));
        }
    }
    let mut remaining = BTreeMap::<ObjectId, usize>::new();
    for id in ids {
        *remaining.entry(*id).or_default() += 1;
    }
    if rows.len() != remaining.len() || rows.keys().any(|id| !remaining.contains_key(id)) {
        return Err(StoreError::Integrity("visible object cardinality"));
    }
    let mut output = Vec::with_capacity(ids.len());
    for id in ids {
        let count = *remaining
            .get(id)
            .ok_or(StoreError::Integrity("visible object order"))?;
        let bytes = if count == 1 {
            remaining.remove(id);
            rows.remove(id)
                .ok_or(StoreError::Integrity("visible object missing"))?
        } else {
            remaining.insert(*id, count - 1);
            let bytes = rows
                .get(id)
                .ok_or(StoreError::Integrity("visible object missing"))?
                .clone();
            note_read_batch_clone(bytes.len());
            bytes
        };
        output.push(CanonicalObject { id: *id, bytes });
    }
    if !rows.is_empty() || !remaining.is_empty() {
        return Err(StoreError::Integrity("visible object order"));
    }
    Ok(output)
}

impl<'a> InitializationSegmentAdmission<'a> {
    pub(crate) fn new(db: &'a crate::schema::StoreDb) -> Result<Self> {
        if !db.initialization_store_is_empty()? {
            return Err(StoreError::Integrity(
                "direct initialization requires empty Store",
            ));
        }
        Ok(Self {
            db,
            batch: Vec::with_capacity(INITIALIZATION_ADMISSION_BATCH_COUNT),
            pending: HashMap::new(),
            batch_bytes: 0,
            statement_number: 0,
            receipt: crate::CandidateReceipt::default(),
            diagnostics: InitializationAdmissionDiagnostics::default(),
            final_phase: false,
        })
    }

    #[cfg(test)]
    pub(crate) fn admit_worker_segment(&mut self, objects: DeferredObjectStore) -> Result<()> {
        self.admit(objects)
    }

    pub(crate) fn finish(self) -> Result<FinishedInitializationAdmission> {
        Ok(FinishedInitializationAdmission {
            final_batch: self.batch,
            statement_number: self.statement_number,
            receipt: self.receipt,
            diagnostics: self.diagnostics,
        })
    }

    pub(crate) fn prepare_final_phase(&mut self) -> Result<()> {
        self.flush_batch()?;
        self.batch = Vec::with_capacity(INITIALIZATION_SLAB_OBJECTS);
        self.pending = HashMap::with_capacity(INITIALIZATION_SLAB_OBJECTS);
        self.final_phase = true;
        Ok(())
    }

    fn observe_final_owned_bytes(&mut self, transient: u64, incoming: u64) {
        if !self.final_phase {
            return;
        }
        let owned = transient
            .saturating_add(incoming)
            .saturating_add(self.batch_bytes as u64)
            .saturating_add((self.batch.capacity() * std::mem::size_of::<CanonicalObject>()) as u64)
            .saturating_add((self.pending.capacity() * 64) as u64);
        self.diagnostics.final_simultaneous_owned_peak_bytes = self
            .diagnostics
            .final_simultaneous_owned_peak_bytes
            .max(owned);
    }

    #[cfg(test)]
    fn admit(&mut self, objects: DeferredObjectStore) -> Result<()> {
        objects.consume_prevalidated_pages(|page| self.admit_page(page))
    }

    pub(crate) fn admit_page(&mut self, page: Vec<CanonicalObject>) -> Result<()> {
        for object in page {
            self.admit_object(object)?;
        }
        Ok(())
    }

    pub(crate) fn admit_object(&mut self, object: CanonicalObject) -> Result<()> {
        if self.pending.contains_key(&object.id) {
            return self.admit_duplicate(object.id, &object.bytes);
        }
        self.push_pending(object)
    }

    fn admit_duplicate(&mut self, id: ObjectId, bytes: &[u8]) -> Result<()> {
        let index = *self
            .pending
            .get(&id)
            .ok_or(StoreError::Integrity("pending initialization object"))?;
        self.diagnostics.collision_checks += 1;
        if self.batch[index].bytes != bytes {
            return Err(StoreError::Integrity("object collision"));
        }
        self.diagnostics.pending_duplicate_objects += 1;
        self.diagnostics.pending_duplicate_bytes = self
            .diagnostics
            .pending_duplicate_bytes
            .saturating_add(bytes.len() as u64);
        Ok(())
    }

    fn push_pending(&mut self, object: CanonicalObject) -> Result<()> {
        if object.bytes.len() > ADMISSION_BATCH_BYTES {
            return Err(StoreError::Integrity("canonical object admission size"));
        }
        if !self.batch.is_empty()
            && (self.batch.len() == INITIALIZATION_ADMISSION_BATCH_COUNT
                || self.batch_bytes.saturating_add(object.bytes.len()) > ADMISSION_BATCH_BYTES)
        {
            self.flush_batch()?;
        }
        self.batch_bytes = self.batch_bytes.saturating_add(object.bytes.len());
        self.pending.insert(object.id, self.batch.len());
        self.batch.push(object);
        self.diagnostics.batch_peak_objects = self
            .diagnostics
            .batch_peak_objects
            .max(self.batch.len() as u64);
        self.diagnostics.batch_peak_payload_bytes = self
            .diagnostics
            .batch_peak_payload_bytes
            .max(self.batch_bytes as u64);
        self.diagnostics.batch_peak_vec_capacity = self
            .diagnostics
            .batch_peak_vec_capacity
            .max(self.batch.capacity() as u64);
        self.diagnostics.pending_index_peak_entries = self
            .diagnostics
            .pending_index_peak_entries
            .max(self.pending.len() as u64);
        self.diagnostics.pending_index_peak_bytes = self
            .diagnostics
            .pending_index_peak_bytes
            .max((self.pending.capacity() * 64) as u64);
        if self.final_phase {
            self.diagnostics.final_batch_peak_payload_bytes = self
                .diagnostics
                .final_batch_peak_payload_bytes
                .max(self.batch_bytes as u64);
            self.diagnostics.final_batch_peak_vec_capacity = self
                .diagnostics
                .final_batch_peak_vec_capacity
                .max(self.batch.capacity() as u64);
            self.diagnostics.final_pending_index_peak_bytes = self
                .diagnostics
                .final_pending_index_peak_bytes
                .max((self.pending.capacity() * 64) as u64);
        }
        Ok(())
    }

    fn flush_batch(&mut self) -> Result<()> {
        if self.batch.is_empty() {
            return Ok(());
        }
        let capacity = self.batch.capacity();
        let mut batch = std::mem::take(&mut self.batch);
        batch.sort_unstable_by(|left, right| left.id.as_bytes().cmp(right.id.as_bytes()));
        let metrics = insert_initialization_segment_admission_batch(
            self.db,
            &batch,
            &mut self.statement_number,
        )?;
        drop(batch);
        self.batch = Vec::with_capacity(capacity);
        self.diagnostics.record_sql_batch(
            metrics.insert,
            metrics.begin_ns,
            metrics.commit_ns,
            if self.final_phase {
                InitializationSqlPhase::FinalBuild
            } else {
                InitializationSqlPhase::Pipeline
            },
        );
        self.batch_bytes = 0;
        self.pending.clear();
        self.receipt.candidate_objects += metrics.insert.objects;
        self.receipt.candidate_bytes = self
            .receipt
            .candidate_bytes
            .saturating_add(metrics.insert.bytes);
        self.receipt.inserted_objects += metrics.insert.objects;
        self.receipt.inserted_bytes = self
            .receipt
            .inserted_bytes
            .saturating_add(metrics.insert.bytes);
        self.receipt.batch_inserted_objects = self
            .receipt
            .batch_inserted_objects
            .saturating_add(metrics.insert.objects);
        self.receipt.batch_inserted_bytes = self
            .receipt
            .batch_inserted_bytes
            .saturating_add(metrics.insert.bytes);
        self.receipt.admission_transactions += 1;
        self.receipt.max_transaction_objects = self
            .receipt
            .max_transaction_objects
            .max(metrics.insert.objects);
        self.receipt.max_transaction_bytes =
            self.receipt.max_transaction_bytes.max(metrics.insert.bytes);
        Ok(())
    }
}

pub(crate) fn admit_planned_objects(
    db: &crate::schema::StoreDb,
    objects: &DeferredObjectStore,
    plan: &CandidatePlan,
    statement_number: &mut u64,
) -> Result<PlannedAdmission> {
    admit_planned_objects_with_limits(
        db,
        objects,
        plan,
        statement_number,
        ADMISSION_BATCH_COUNT,
        ADMISSION_BATCH_BYTES,
        false,
    )
}

pub(crate) fn admit_initialization_objects(
    db: &crate::schema::StoreDb,
    objects: &DeferredObjectStore,
    plan: &CandidatePlan,
    statement_number: &mut u64,
) -> Result<PlannedAdmission> {
    admit_planned_objects_with_limits(
        db,
        objects,
        plan,
        statement_number,
        INITIALIZATION_ADMISSION_BATCH_COUNT,
        ADMISSION_BATCH_BYTES,
        true,
    )
}

fn admit_planned_objects_with_limits(
    db: &crate::schema::StoreDb,
    objects: &DeferredObjectStore,
    plan: &CandidatePlan,
    statement_number: &mut u64,
    batch_count: usize,
    batch_bytes_limit: usize,
    bulk_insert: bool,
) -> Result<PlannedAdmission> {
    let mut batch = Vec::with_capacity(batch_count);
    let mut batch_bytes = 0_usize;
    let mut admission = PlannedAdmission {
        final_batch: Vec::new(),
        batch_inserted_objects: 0,
        batch_inserted_bytes: 0,
        transactions: 0,
        max_transaction_objects: 0,
        max_transaction_bytes: 0,
        begin_ns: 0,
        insert_ns: 0,
        commit_ns: 0,
    };
    let order = if plan.all_missing {
        &objects.reachable
    } else {
        &plan.missing_order
    };
    objects.visit_prevalidated_order(order, &mut |id, bytes| {
        if bytes.len() > batch_bytes_limit {
            return Err(StoreError::Integrity("canonical object admission size"));
        }
        if !batch.is_empty()
            && (batch.len() == batch_count
                || batch_bytes.saturating_add(bytes.len()) > batch_bytes_limit)
        {
            let metrics = insert_admission_batch(db, &batch, statement_number, bulk_insert)?;
            admission.batch_inserted_objects = admission
                .batch_inserted_objects
                .saturating_add(metrics.insert.objects);
            admission.batch_inserted_bytes = admission
                .batch_inserted_bytes
                .saturating_add(metrics.insert.bytes);
            admission.transactions += 1;
            admission.max_transaction_objects = admission
                .max_transaction_objects
                .max(metrics.insert.objects);
            admission.max_transaction_bytes =
                admission.max_transaction_bytes.max(metrics.insert.bytes);
            admission.begin_ns = admission.begin_ns.saturating_add(metrics.begin_ns);
            admission.insert_ns = admission.insert_ns.saturating_add(metrics.insert.insert_ns);
            admission.commit_ns = admission.commit_ns.saturating_add(metrics.commit_ns);
            batch.clear();
            batch_bytes = 0;
        }
        batch_bytes = batch_bytes.saturating_add(bytes.len());
        batch.push(CanonicalObject {
            id,
            bytes: bytes.to_vec(),
        });
        Ok(())
    })?;
    admission.final_batch = batch;
    let final_objects = admission.final_batch.len() as u64;
    let final_bytes = admission
        .final_batch
        .iter()
        .map(|object| object.bytes.len() as u64)
        .sum::<u64>();
    admission.max_transaction_objects = admission.max_transaction_objects.max(final_objects);
    admission.max_transaction_bytes = admission.max_transaction_bytes.max(final_bytes);
    if admission.batch_inserted_objects + final_objects != plan.inserted_objects
        || admission.batch_inserted_bytes + final_bytes != plan.inserted_bytes
        || admission.max_transaction_objects > batch_count as u64
        || admission.max_transaction_bytes > batch_bytes_limit as u64
    {
        return Err(StoreError::Integrity(
            "bounded candidate admission equation",
        ));
    }
    Ok(admission)
}

fn insert_admission_batch(
    db: &crate::schema::StoreDb,
    batch: &[CanonicalObject],
    statement_number: &mut u64,
    bulk_insert: bool,
) -> Result<AdmissionBatchMetrics> {
    let begin_started = Instant::now();
    let mut connection = db.writer()?;
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let begin_ns = elapsed_ns(begin_started);
    #[cfg(feature = "test-instrumentation")]
    if !bulk_insert {
        crate::schema::verification_store_checkpoint(
            crate::schema::VerificationStoreFault::LaterAdmissionBatch,
        )?;
    }
    let insert = if bulk_insert {
        insert_initialization_object_batch(&transaction, batch, statement_number)?
    } else {
        insert_object_batch(&transaction, batch, statement_number)?
    };
    let commit_started = Instant::now();
    transaction.commit()?;
    #[cfg(feature = "test-instrumentation")]
    if !bulk_insert {
        crate::schema::verification_early_committed();
    }
    Ok(AdmissionBatchMetrics {
        insert,
        begin_ns,
        commit_ns: elapsed_ns(commit_started),
    })
}

fn insert_initialization_segment_admission_batch(
    db: &crate::schema::StoreDb,
    batch: &[CanonicalObject],
    statement_number: &mut u64,
) -> Result<AdmissionBatchMetrics> {
    let begin_started = Instant::now();
    let mut connection = db.writer()?;
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let begin_ns = elapsed_ns(begin_started);
    let insert = insert_initialization_segment_batch(&transaction, batch, statement_number)?;
    let commit_started = Instant::now();
    transaction.commit()?;
    Ok(AdmissionBatchMetrics {
        insert,
        begin_ns,
        commit_ns: elapsed_ns(commit_started),
    })
}

pub(crate) fn insert_object_batch(
    transaction: &rusqlite::Transaction<'_>,
    objects: &[CanonicalObject],
    statement_number: &mut u64,
) -> Result<ObjectInsertMetrics> {
    let prepare_started = Instant::now();
    let mut insert = transaction.prepare_cached(crate::statements::objects::INSERT)?;
    let mut equal = transaction.prepare_cached(crate::statements::objects::EQUAL)?;
    let mut metrics = ObjectInsertMetrics {
        insert_ns: elapsed_ns(prepare_started),
        ..ObjectInsertMetrics::default()
    };
    let mut payload_started = Instant::now();
    for object in objects {
        metrics.payload_ns = metrics
            .payload_ns
            .saturating_add(elapsed_ns(payload_started));
        *statement_number += 1;
        crate::schema::fail_transaction_statement(*statement_number)?;
        let insert_started = Instant::now();
        let inserted = insert.execute(rusqlite::params![
            object.id.as_bytes().as_slice(),
            object.bytes.as_slice()
        ])?;
        if inserted == 0 {
            let same = equal.exists(rusqlite::params![
                object.id.as_bytes().as_slice(),
                object.bytes.as_slice()
            ])?;
            return Err(StoreError::Integrity(if same {
                "unexpected existing object"
            } else {
                "object collision"
            }));
        }
        metrics.insert_ns = metrics.insert_ns.saturating_add(elapsed_ns(insert_started));
        metrics.objects += 1;
        metrics.bytes = metrics.bytes.saturating_add(object.bytes.len() as u64);
        payload_started = Instant::now();
    }
    Ok(metrics)
}

pub(crate) fn insert_initialization_object_batch(
    transaction: &rusqlite::Transaction<'_>,
    objects: &[CanonicalObject],
    statement_number: &mut u64,
) -> Result<ObjectInsertMetrics> {
    if objects.is_empty() {
        return Ok(ObjectInsertMetrics::default());
    }
    for _ in objects {
        *statement_number += 1;
        crate::schema::fail_transaction_statement(*statement_number)?;
    }
    let mut sql = String::with_capacity(80 + objects.len() * 6);
    sql.push_str("INSERT INTO objects(object_id, bytes) VALUES ");
    for index in 0..objects.len() {
        if index != 0 {
            sql.push(',');
        }
        sql.push_str("(?,?)");
    }
    sql.push_str(" ON CONFLICT(object_id) DO NOTHING");
    let started = Instant::now();
    let inserted = transaction.execute(
        &sql,
        params_from_iter(
            objects
                .iter()
                .flat_map(|object| [object.id.as_bytes().as_slice(), object.bytes.as_slice()]),
        ),
    )?;
    if inserted != objects.len() {
        return Err(StoreError::Integrity("unexpected existing object"));
    }
    Ok(ObjectInsertMetrics {
        insert_ns: elapsed_ns(started),
        objects: objects.len() as u64,
        bytes: objects.iter().map(|object| object.bytes.len() as u64).sum(),
        ..ObjectInsertMetrics::default()
    })
}

pub(crate) fn insert_initialization_segment_batch(
    transaction: &rusqlite::Transaction<'_>,
    objects: &[CanonicalObject],
    statement_number: &mut u64,
) -> Result<ObjectInsertMetrics> {
    if objects.is_empty() {
        return Ok(ObjectInsertMetrics::default());
    }
    for _ in objects {
        *statement_number += 1;
        crate::schema::fail_transaction_statement(*statement_number)?;
    }
    let started = Instant::now();
    let prepare_started = Instant::now();
    let mut statement = transaction.prepare_cached(crate::statements::objects::INSERT)?;
    let sql_prepare_ns = elapsed_ns(prepare_started);
    let step_started = Instant::now();
    let mut inserted_objects = 0_u64;
    let mut inserted_bytes = 0_u64;
    let mut skipped = Vec::new();
    for object in objects {
        if statement.execute(rusqlite::params![
            object.id.as_bytes().as_slice(),
            object.bytes.as_slice()
        ])? == 0
        {
            skipped.push(object);
        } else {
            inserted_objects += 1;
            inserted_bytes = inserted_bytes.saturating_add(object.bytes.len() as u64);
        }
    }
    let sql_bind_step_returning_ns = elapsed_ns(step_started);
    drop(statement);
    let skipped_bytes = skipped
        .iter()
        .map(|object| object.bytes.len() as u64)
        .sum::<u64>();
    let mut conflict_read_calls = 0_u64;
    let mut conflict_read_rows = 0_u64;
    let mut conflict_read_bytes = 0_u64;
    let mut conflict_read_ns = 0_u64;
    for page in skipped.chunks(OBJECT_PAGE_COUNT) {
        let ids = page.iter().map(|object| object.id).collect::<Vec<_>>();
        let conflict_started = Instant::now();
        let durable = read_object_rows_from_connection(transaction, &ids)?;
        conflict_read_ns = conflict_read_ns.saturating_add(elapsed_ns(conflict_started));
        conflict_read_calls += 1;
        conflict_read_rows = conflict_read_rows.saturating_add(durable.len() as u64);
        conflict_read_bytes = conflict_read_bytes.saturating_add(
            durable
                .iter()
                .map(|object| object.bytes.len() as u64)
                .sum::<u64>(),
        );
        if durable
            .iter()
            .zip(page)
            .any(|(durable, object)| durable.bytes != object.bytes)
        {
            return Err(StoreError::Integrity("object collision"));
        }
    }
    Ok(ObjectInsertMetrics {
        insert_ns: elapsed_ns(started),
        objects: inserted_objects,
        bytes: inserted_bytes,
        submitted_rows: objects.len() as u64,
        returned_ids: inserted_objects,
        skipped_ids: skipped.len() as u64,
        skipped_bytes,
        collision_checks: skipped.len() as u64,
        sql_string_build_ns: 0,
        sql_prepare_ns,
        sql_bind_step_returning_ns,
        conflict_read_calls,
        conflict_read_rows,
        conflict_read_bytes,
        conflict_read_ns,
        ..ObjectInsertMetrics::default()
    })
}

impl ObjectSource for crate::schema::StoreDb {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        self.read_object_row(id)
    }

    fn read_authenticated_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        self.read_object_rows(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_payload_copy_counter_has_a_positive_control() {
        let counter = ParentPayloadCopyCounter::start();
        let object = CanonicalObject {
            id: ObjectId::for_bytes(b"parent-copy-control"),
            bytes: b"parent-copy-control".to_vec(),
        };
        let copy = object.clone();
        assert_eq!(counter.bytes(), copy.bytes.len() as u64);
    }

    fn sealed_segment(objects: Vec<CanonicalObject>) -> DeferredObjectStore {
        let mut segment = DeferredObjectStore::new_all_reachable().unwrap();
        for object in objects {
            segment.put(object.id, &object.bytes).unwrap();
        }
        segment.all_reachable().unwrap()
    }

    fn finish_segment_admission(
        db: &crate::schema::StoreDb,
        admission: InitializationSegmentAdmission<'_>,
    ) -> (crate::CandidateReceipt, Vec<ObjectId>) {
        let finished = admission.finish().unwrap();
        let ids = finished
            .final_batch
            .iter()
            .map(|object| object.id)
            .collect::<Vec<_>>();
        let mut statement_number = finished.statement_number;
        let mut connection = db.writer().unwrap();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let metrics = insert_initialization_segment_batch(
            &transaction,
            &finished.final_batch,
            &mut statement_number,
        )
        .unwrap();
        transaction.commit().unwrap();
        let mut receipt = finished.receipt;
        receipt.candidate_objects += metrics.objects;
        receipt.candidate_bytes = receipt.candidate_bytes.saturating_add(metrics.bytes);
        receipt.inserted_objects += metrics.objects;
        receipt.inserted_bytes = receipt.inserted_bytes.saturating_add(metrics.bytes);
        receipt.final_inserted_objects = metrics.objects;
        receipt.final_inserted_bytes = metrics.bytes;
        receipt.max_transaction_objects = receipt.max_transaction_objects.max(metrics.objects);
        receipt.max_transaction_bytes = receipt.max_transaction_bytes.max(metrics.bytes);
        assert_eq!(receipt.candidate_objects, receipt.inserted_objects);
        assert_eq!(receipt.candidate_bytes, receipt.inserted_bytes);
        assert_eq!(
            receipt.inserted_objects,
            receipt.batch_inserted_objects + receipt.final_inserted_objects
        );
        assert_eq!(
            receipt.inserted_bytes,
            receipt.batch_inserted_bytes + receipt.final_inserted_bytes
        );
        (receipt, ids)
    }

    #[test]
    fn memory_segment_moves_owned_canonical_bytes() {
        let bytes = layerfs_content::encode_bytes_object(b"owned").unwrap();
        let id = ObjectId::for_bytes(&bytes);
        let mut segment = DeferredObjectStore::new_all_reachable().unwrap();
        segment.put(id, &bytes).unwrap();
        let original = match &segment.storage {
            DeferredObjects::Memory { rows, .. } => rows.get(&id).unwrap().as_ptr(),
            DeferredObjects::Spill(_) => panic!("small segment spilled"),
        };
        segment
            .all_reachable()
            .unwrap()
            .consume_prevalidated_pages(|page| {
                assert_eq!(page.len(), 1);
                assert_eq!(page[0].id, id);
                assert_eq!(page[0].bytes.as_ptr(), original);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn append_only_writer_rejects_and_counts_outer_gets() {
        let writer = AppendOnlyInitializationWriter::new(64).unwrap();
        let path = writer.path.0.clone();
        assert!(ObjectStore::get(&writer, ObjectId::for_bytes(b"missing")).is_err());
        assert_eq!(writer.get_calls(), 1);
        drop(writer);
        assert!(!path.exists());
    }

    #[test]
    fn empty_append_only_segment_finishes_without_a_read_pass() {
        let writer = AppendOnlyInitializationWriter::new(64).unwrap();
        let path = writer.path.0.clone();
        let segment = writer.seal().unwrap();
        assert!(!path.exists());
        assert_eq!(
            segment.finish_consumption().unwrap(),
            InitializationSegmentIoMetrics::default()
        );
    }

    #[cfg(unix)]
    #[test]
    fn append_only_blocks_are_written_once_read_forward_and_unlinked() {
        let first = layerfs_content::encode_bytes_object(b"first").unwrap();
        let second = layerfs_content::encode_bytes_object(b"second").unwrap();
        let mut writer = AppendOnlyInitializationWriter::new(64).unwrap();
        let first_checkpoint = writer.checkpoint();
        let first_id = ObjectStore::put(&mut writer, &first).unwrap();
        let first_block = writer.block_since(0, 0, first_checkpoint).unwrap();
        let second_checkpoint = writer.checkpoint();
        let second_id = ObjectStore::put(&mut writer, &second).unwrap();
        let second_block = writer.block_since(1, 0, second_checkpoint).unwrap();
        assert_eq!(first_block.object_count, 1);
        assert_eq!(first_block.byte_count, first.len() as u64);
        assert_eq!(second_block.object_count, 1);
        assert_eq!(second_block.byte_count, second.len() as u64);
        assert_eq!(second_block.end, (first.len() + second.len() + 80) as u64);
        assert_eq!(writer.get_calls(), 0);
        let path = writer.path.0.clone();
        let mut segment = writer.seal().unwrap();
        assert!(segment.reader_capacity() <= 64);
        assert_eq!(segment.path(), path);
        assert!(!path.exists());
        assert!(segment.consume_block(second_block, |_| Ok(())).is_err());
        let mut ids = Vec::new();
        segment
            .consume_block(first_block, |object| {
                ids.push(object.id);
                Ok(())
            })
            .unwrap();
        segment
            .consume_block(second_block, |object| {
                ids.push(object.id);
                Ok(())
            })
            .unwrap();
        assert_eq!(ids, vec![first_id, second_id]);
        assert_eq!(segment.raw_read_bytes(), second_block.end);
        assert!(
            segment.raw_reads()
                < first_block
                    .object_count
                    .saturating_add(second_block.object_count)
                    * 3
        );
        segment.finish_consumption().unwrap();

        let mut failed = AppendOnlyInitializationWriter::new(64).unwrap();
        ObjectStore::put(&mut failed, &first).unwrap();
        failed.flush().unwrap();
        failed.writer.set_len(0).unwrap();
        let failed_path = failed.path.0.clone();
        assert!(failed.seal().is_err());
        assert!(!failed_path.exists());
    }

    #[test]
    fn buffered_append_reader_scales_raw_reads_with_bytes_not_tiny_frames() {
        let capacity = 4096;
        let mut writer = AppendOnlyInitializationWriter::new(capacity).unwrap();
        let checkpoint = writer.checkpoint();
        for index in 0_u64..5_000 {
            let canonical = layerfs_content::encode_bytes_object(&index.to_be_bytes()).unwrap();
            ObjectStore::put(&mut writer, &canonical).unwrap();
        }
        let block = writer.block_since(0, 0, checkpoint).unwrap();
        let mut segment = writer.seal().unwrap();
        let mut frames = 0_u64;
        segment
            .consume_block(block, |_| {
                frames += 1;
                Ok(())
            })
            .unwrap();
        let expected_max_reads = block.end.div_ceil(capacity as u64).saturating_add(1);
        assert_eq!(frames, 5_000);
        assert_eq!(segment.raw_read_bytes(), block.end);
        assert!(segment.raw_reads() <= expected_max_reads);
        assert!(segment.raw_reads() * 10 < frames);
        let metrics = segment.finish_consumption().unwrap();
        assert_eq!(metrics.frames, frames);
        assert_eq!(metrics.write_bytes, block.end);
        assert_eq!(metrics.raw_read_bytes, block.end);
    }

    #[test]
    fn append_only_reversed_worker_completion_sorts_to_task_order() {
        let first = layerfs_content::encode_bytes_object(b"first-task").unwrap();
        let second = layerfs_content::encode_bytes_object(b"second-task").unwrap();
        let mut worker_zero = AppendOnlyInitializationWriter::new(64).unwrap();
        let zero_checkpoint = worker_zero.checkpoint();
        let second_id = ObjectStore::put(&mut worker_zero, &second).unwrap();
        let second_block = worker_zero.block_since(1, 0, zero_checkpoint).unwrap();
        let mut worker_one = AppendOnlyInitializationWriter::new(64).unwrap();
        let one_checkpoint = worker_one.checkpoint();
        let first_id = ObjectStore::put(&mut worker_one, &first).unwrap();
        let first_block = worker_one.block_since(0, 1, one_checkpoint).unwrap();
        let mut segments = vec![worker_zero.seal().unwrap(), worker_one.seal().unwrap()];
        let mut completed = vec![second_block, first_block];
        completed.sort_by_key(|block| block.task_ordinal);
        let mut ids = Vec::new();
        for block in completed {
            segments[block.worker_index]
                .consume_block(block, |object| {
                    ids.push(object.id);
                    Ok(())
                })
                .unwrap();
        }
        assert_eq!(ids, vec![first_id, second_id]);
        for segment in segments {
            segment.finish_consumption().unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn compact_inode_pair_sidecar_is_bounded_forward_only_and_unlinked() {
        let mut writer = CompactInodePairWriter::new(128).unwrap();
        let pair_buffer_capacity = writer.pending_capacity();
        assert!(pair_buffer_capacity >= 128);
        let first_checkpoint = writer.checkpoint();
        let first = (
            layerfs_content::tree::inode::InodeId::allocate([1; 32], 1),
            ObjectId::for_bytes(b"first-record"),
        );
        writer.push(first.0, first.1).unwrap();
        let first_block = writer.block_since(0, 0, first_checkpoint).unwrap();
        let second_checkpoint = writer.checkpoint();
        let second = (
            layerfs_content::tree::inode::InodeId::allocate([1; 32], 2),
            ObjectId::for_bytes(b"second-record"),
        );
        writer.push(second.0, second.1).unwrap();
        let second_block = writer.block_since(1, 0, second_checkpoint).unwrap();
        assert_eq!((first_block.start, first_block.end), (0, 64));
        assert_eq!((second_block.start, second_block.end), (64, 128));
        let path = writer.path.0.clone();
        let segment = writer.seal().unwrap();
        assert_eq!(segment.path(), path);
        assert!(!path.exists());
        assert!(
            CompactInodePairStream::new(vec![segment], vec![second_block, first_block]).is_err()
        );

        let mut writer = CompactInodePairWriter::new(1024).unwrap();
        let checkpoint = writer.checkpoint();
        writer.push(first.0, first.1).unwrap();
        let first_block = writer.block_since(0, 0, checkpoint).unwrap();
        let checkpoint = writer.checkpoint();
        writer.push(second.0, second.1).unwrap();
        let second_block = writer.block_since(1, 0, checkpoint).unwrap();
        let mut segment = writer.seal().unwrap();
        assert!(segment.reader_capacity() <= 1024);
        let pairs = vec![
            segment.read_pair(first_block.end).unwrap(),
            segment.read_pair(second_block.end).unwrap(),
        ];
        assert_eq!(pairs, vec![first, second]);
        assert_eq!(segment.raw_read_bytes(), second_block.end);
        assert!(segment.raw_reads() < 2);
        assert!(segment.consumed());
    }

    #[cfg(unix)]
    #[test]
    fn spilled_segment_streams_bounded_pages_once_and_unlinks() {
        let first = layerfs_content::encode_bytes_object(b"first").unwrap();
        let first_id = ObjectId::for_bytes(&first);
        let tail = layerfs_content::encode_bytes_object(b"pending tail").unwrap();
        let tail_id = ObjectId::for_bytes(&tail);
        let mut segment = DeferredObjectStore::new_all_reachable().unwrap();
        segment.put(first_id, &first).unwrap();
        segment.spill().unwrap();
        segment.put(tail_id, &tail).unwrap();
        let path = match &segment.storage {
            DeferredObjects::Spill(spill) => {
                assert!(!spill.pending.is_empty());
                spill.path.clone()
            }
            DeferredObjects::Memory { .. } => panic!("forced segment did not spill"),
        };
        let mut ids = Vec::new();
        segment
            .all_reachable()
            .unwrap()
            .consume_prevalidated_pages(|page| {
                assert!(page.len() <= INITIALIZATION_ADMISSION_BATCH_COUNT);
                assert!(
                    page.iter().map(|object| object.bytes.len()).sum::<usize>()
                        <= ADMISSION_BATCH_BYTES
                );
                ids.extend(page.into_iter().map(|object| object.id));
                Ok(())
            })
            .unwrap();
        assert_eq!(ids, vec![first_id, tail_id]);
        assert!(!path.exists());
    }

    #[test]
    fn segment_admission_deduplicates_across_pending_segments() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-segment-admission-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db = crate::schema::StoreDb::create(root.join("store.sqlite")).unwrap();

        let shared = layerfs_content::encode_bytes_object(b"shared").unwrap();
        let shared_id = ObjectId::for_bytes(&shared);
        let mut admission = InitializationSegmentAdmission::new(&db).unwrap();
        admission
            .admit_worker_segment(sealed_segment(vec![CanonicalObject {
                id: shared_id,
                bytes: shared.clone(),
            }]))
            .unwrap();
        admission
            .admit_worker_segment(sealed_segment(vec![CanonicalObject {
                id: shared_id,
                bytes: shared,
            }]))
            .unwrap();
        let finished = admission.finish().unwrap();
        assert_eq!(finished.receipt, crate::CandidateReceipt::default());
        assert_eq!(finished.final_batch.len(), 1);
        assert_eq!(finished.diagnostics.pending_duplicate_objects, 1);
        assert_eq!(
            finished.diagnostics.pending_duplicate_bytes,
            finished.final_batch[0].bytes.len() as u64
        );
        assert_eq!(finished.diagnostics.collision_checks, 1);

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn direct_admission_honors_every_frozen_count_boundary() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-segment-boundaries-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        for count in [0_usize, 1, 127, 128, 8190, 8191] {
            let db = crate::schema::StoreDb::create(root.join(format!("{count}.sqlite"))).unwrap();
            let objects = (0..count)
                .map(|index| {
                    let bytes = layerfs_content::encode_bytes_object(&(index as u64).to_be_bytes())
                        .unwrap();
                    CanonicalObject {
                        id: ObjectId::for_bytes(&bytes),
                        bytes,
                    }
                })
                .collect::<Vec<_>>();
            let expected_bytes = objects
                .iter()
                .map(|object| object.bytes.len() as u64)
                .sum::<u64>();
            assert!(expected_bytes < ADMISSION_BATCH_BYTES as u64);
            let mut admission = InitializationSegmentAdmission::new(&db).unwrap();
            admission
                .admit_worker_segment(sealed_segment(objects))
                .unwrap();
            let (receipt, _) = finish_segment_admission(&db, admission);
            assert_eq!(receipt.candidate_objects, count as u64);
            assert_eq!(receipt.candidate_bytes, expected_bytes);
            assert_eq!(receipt.max_transaction_objects, count as u64);
            assert_eq!(receipt.max_transaction_bytes, expected_bytes);
            drop(db);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn direct_admission_is_independent_of_segment_order() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-segment-order-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let canonical = [
            b"shared".as_slice(),
            b"left".as_slice(),
            b"right".as_slice(),
        ]
        .map(|payload| layerfs_content::encode_bytes_object(payload).unwrap());
        let ids = canonical
            .iter()
            .map(|bytes| ObjectId::for_bytes(bytes))
            .collect::<Vec<_>>();
        let mut results = Vec::new();
        for reverse in [false, true] {
            let db =
                crate::schema::StoreDb::create(root.join(format!("{reverse}.sqlite"))).unwrap();
            let left = sealed_segment(vec![
                CanonicalObject {
                    id: ids[0],
                    bytes: canonical[0].clone(),
                },
                CanonicalObject {
                    id: ids[1],
                    bytes: canonical[1].clone(),
                },
            ]);
            let right = sealed_segment(vec![
                CanonicalObject {
                    id: ids[0],
                    bytes: canonical[0].clone(),
                },
                CanonicalObject {
                    id: ids[2],
                    bytes: canonical[2].clone(),
                },
            ]);
            let mut admission = InitializationSegmentAdmission::new(&db).unwrap();
            let mut segments = if reverse {
                vec![right, left]
            } else {
                vec![left, right]
            };
            for segment in segments.drain(..) {
                admission.admit_worker_segment(segment).unwrap();
            }
            let (receipt, _) = finish_segment_admission(&db, admission);
            let mut sorted_ids = ids.to_vec();
            sorted_ids.sort_unstable();
            let rows = db.read_object_rows(&sorted_ids).unwrap();
            results.push((receipt, rows));
            drop(db);
        }
        assert_eq!(results[0], results[1]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn segment_admission_deduplicates_after_a_batch_boundary() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-segment-boundary-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db = crate::schema::StoreDb::create(root.join("store.sqlite")).unwrap();
        let mut objects = Vec::new();
        for index in 0_u64..130 {
            let mut payload = vec![index as u8; layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES];
            payload[..8].copy_from_slice(&index.to_le_bytes());
            let bytes = layerfs_content::file::extent_codec::encode_chunk_object(&payload).unwrap();
            objects.push(CanonicalObject {
                id: ObjectId::for_bytes(&bytes),
                bytes,
            });
        }
        let duplicate = objects[0].clone();
        let duplicate_id = duplicate.id;
        let expected_objects = objects.len() as u64;
        let mut admission = InitializationSegmentAdmission::new(&db).unwrap();
        admission
            .admit_worker_segment(sealed_segment(objects))
            .unwrap();
        assert!(admission.receipt.admission_transactions > 0);
        admission
            .admit_worker_segment(sealed_segment(vec![duplicate]))
            .unwrap();
        let finished = admission.finish().unwrap();
        let retained_objects = finished.final_batch.len() as u64;
        let mut statement_number = finished.statement_number;
        let mut connection = db.writer().unwrap();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let final_metrics = insert_initialization_segment_batch(
            &transaction,
            &finished.final_batch,
            &mut statement_number,
        )
        .unwrap();
        transaction.commit().unwrap();
        let mut diagnostics = finished.diagnostics;
        diagnostics.record_sql_batch(final_metrics, 0, 0, InitializationSqlPhase::Publication);
        assert!(final_metrics.objects < retained_objects);
        assert_eq!(diagnostics.cross_batch_skipped_objects, 1);
        assert_eq!(diagnostics.sql_submitted_rows, expected_objects + 1);
        assert_eq!(
            diagnostics.sql_returned_ids + diagnostics.sql_skipped_ids,
            diagnostics.sql_submitted_rows
        );
        assert_eq!(diagnostics.conflict_read_rows, 1);
        assert_eq!(
            finished.receipt.batch_inserted_objects + final_metrics.objects,
            expected_objects
        );

        let forged = layerfs_content::file::extent_codec::encode_chunk_object(&vec![
            255;
            layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES
        ])
        .unwrap();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert!(matches!(
            insert_initialization_segment_batch(
                &transaction,
                &[CanonicalObject {
                    id: duplicate_id,
                    bytes: forged,
                }],
                &mut statement_number,
            ),
            Err(StoreError::Integrity("object collision"))
        ));
        drop(transaction);

        drop(connection);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn planned_admission_keeps_every_object_transaction_below_the_frozen_bounds() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-bounded-admission-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db = crate::schema::StoreDb::create(root.join("store.sqlite")).unwrap();
        let mut objects = DeferredObjectStore::new().unwrap();
        for index in 0_u64..300 {
            let canonical = layerfs_content::encode_bytes_object(&index.to_le_bytes()).unwrap();
            let id = ObjectId::for_bytes(&canonical);
            objects.put(id, &canonical).unwrap();
        }
        let plan = db.plan_initialization_candidate(&objects).unwrap();
        assert!(plan.all_missing);
        let mut initialization_statement_number = 0;
        let initialization = admit_initialization_objects(
            &db,
            &objects,
            &plan,
            &mut initialization_statement_number,
        )
        .unwrap();
        assert_eq!(initialization.transactions, 0);
        assert_eq!(initialization.final_batch.len(), 300);
        assert_eq!(initialization.max_transaction_objects, 300);
        let bulk_db = crate::schema::StoreDb::create(root.join("bulk.sqlite")).unwrap();
        {
            let mut connection = bulk_db.writer().unwrap();
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            let metrics = insert_initialization_object_batch(
                &transaction,
                &initialization.final_batch,
                &mut initialization_statement_number,
            )
            .unwrap();
            assert_eq!(metrics.objects, 300);
            transaction.commit().unwrap();
        }
        assert_eq!(
            bulk_db
                .plan_initialization_candidate(&objects)
                .unwrap()
                .reused_objects,
            300
        );

        let mut statement_number = 0;
        let admission = admit_planned_objects(&db, &objects, &plan, &mut statement_number).unwrap();
        assert_eq!(admission.transactions, 2);
        assert_eq!(admission.batch_inserted_objects, 254);
        assert_eq!(admission.final_batch.len(), 46);
        assert_eq!(admission.max_transaction_objects, 127);
        assert!(admission.max_transaction_bytes < OBJECT_PAGE_BYTES as u64);
        {
            let mut connection = db.writer().unwrap();
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            let final_metrics =
                insert_object_batch(&transaction, &admission.final_batch, &mut statement_number)
                    .unwrap();
            assert_eq!(final_metrics.objects, 46);
            transaction.commit().unwrap();
        }
        assert_eq!(db.plan_candidate(&objects).unwrap().reused_objects, 300);
        let existing = admission.final_batch[0].clone();
        let mut forged = DeferredObjectStore::new_all_reachable().unwrap();
        let forged_bytes = layerfs_content::encode_bytes_object(&u64::MAX.to_le_bytes()).unwrap();
        assert_eq!(forged_bytes.len(), existing.bytes.len());
        assert!(matches!(
            forged.put(existing.id, &forged_bytes),
            Err(StoreError::Integrity("object identity"))
        ));
        {
            let mut connection = db.writer().unwrap();
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            assert!(matches!(
                insert_object_batch(
                    &transaction,
                    std::slice::from_ref(&existing),
                    &mut statement_number,
                ),
                Err(StoreError::Integrity("unexpected existing object"))
            ));
        }
        let collision = CanonicalObject {
            id: existing.id,
            bytes: forged_bytes,
        };
        let mut connection = db.writer().unwrap();
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert!(matches!(
            insert_object_batch(&transaction, &[collision], &mut statement_number),
            Err(StoreError::Integrity("object collision"))
        ));
        drop(transaction);
        drop(connection);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn finished_candidate_payload_spill_is_private_read_only_and_authenticated() {
        use std::os::unix::fs::PermissionsExt;

        let mut objects = DeferredObjectStore::new().unwrap();
        let mut root = None;
        let chunk_bytes = layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES;
        for index in 0..=CANDIDATE_MEMORY_BYTES / chunk_bytes + 1 {
            let mut payload = vec![index as u8; chunk_bytes];
            payload[..8].copy_from_slice(&(index as u64).to_be_bytes());
            let canonical =
                layerfs_content::file::extent_codec::encode_chunk_object(&payload).unwrap();
            let id = ObjectId::for_bytes(&canonical);
            objects.put(id, &canonical).unwrap();
            root = Some(id);
        }
        let path = match &objects.storage {
            DeferredObjects::Spill(spill) => spill.path.clone(),
            DeferredObjects::Memory { .. } => panic!("candidate did not spill"),
        };
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let root = root.unwrap();
        let objects = objects.reachable_from(root).unwrap();
        assert!(!path.exists());
        let spill = match &objects.storage {
            DeferredObjects::Spill(spill) => spill,
            DeferredObjects::Memory { .. } => panic!("candidate did not spill"),
        };
        assert!(spill.writer.is_none());
        assert!(spill.reader.lock().unwrap().write_all(&[0]).is_err());
        let mut missing = SpillableObjectSet::empty().unwrap();
        missing.insert_page(&[root]).unwrap();
        let order = objects.order_missing(&missing).unwrap();
        let mut visited = Vec::new();
        objects
            .visit_prevalidated_order(&order, &mut |id, bytes| {
                layerfs_content::authenticate_identity(bytes, id)?;
                visited.push(id);
                Ok(())
            })
            .unwrap();
        assert_eq!(visited, vec![root]);
        let canonical = objects.read_object(root).unwrap();
        layerfs_content::authenticate_identity(&canonical, root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn prevalidated_transfer_seals_a_nonempty_spill_tail() {
        let mut objects = DeferredObjectStore::new_all_reachable().unwrap();
        let chunk_bytes = layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES;
        for index in 0..=CANDIDATE_MEMORY_BYTES / chunk_bytes + 1 {
            let mut payload = vec![index as u8; chunk_bytes];
            payload[..8].copy_from_slice(&(index as u64).to_be_bytes());
            let canonical =
                layerfs_content::file::extent_codec::encode_chunk_object(&payload).unwrap();
            objects
                .put(ObjectId::for_bytes(&canonical), &canonical)
                .unwrap();
        }
        let tail = layerfs_content::encode_bytes_object(b"pending tail").unwrap();
        let tail_id = ObjectId::for_bytes(&tail);
        objects.put(tail_id, &tail).unwrap();
        let expected_count = objects.len();

        let (path, pending_bytes) = match &objects.storage {
            DeferredObjects::Spill(spill) => (spill.path.clone(), spill.pending.len()),
            DeferredObjects::Memory { .. } => panic!("candidate did not spill"),
        };
        assert!(pending_bytes > 0);

        let objects = ObjectBuffer {
            source: None,
            objects,
        }
        .into_prevalidated()
        .unwrap();
        let spill = match &objects.storage {
            DeferredObjects::Spill(spill) => spill,
            DeferredObjects::Memory { .. } => panic!("candidate did not spill"),
        };
        assert!(spill.writer.is_none());
        assert!(spill.pending.is_empty());
        assert!(!path.exists());

        let mut receiver = ObjectBuffer::empty_all_reachable().unwrap();
        receiver.merge_prevalidated(objects).unwrap();
        assert_eq!(receiver.objects.len(), expected_count);
        let transferred_tail = receiver.objects.read_object(tail_id).unwrap();
        assert_eq!(transferred_tail, tail);
        layerfs_content::authenticate_identity(&transferred_tail, tail_id).unwrap();
    }

    #[test]
    fn resumable_transfer_keeps_a_spill_writable() {
        let mut objects = DeferredObjectStore::new_all_reachable().unwrap();
        let chunk_bytes = layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES;
        for index in 0..=CANDIDATE_MEMORY_BYTES / chunk_bytes + 1 {
            let mut payload = vec![index as u8; chunk_bytes];
            payload[..8].copy_from_slice(&(index as u64).to_be_bytes());
            let canonical =
                layerfs_content::file::extent_codec::encode_chunk_object(&payload).unwrap();
            objects
                .put(ObjectId::for_bytes(&canonical), &canonical)
                .unwrap();
        }
        assert!(matches!(objects.storage, DeferredObjects::Spill(_)));

        let objects = ObjectBuffer {
            source: None,
            objects,
        }
        .into_resumable();
        let mut resumed = ObjectBuffer {
            source: None,
            objects,
        };
        let tail = layerfs_content::encode_bytes_object(b"resumed tail").unwrap();
        let tail_id = ObjectId::for_bytes(&tail);
        resumed.objects.put(tail_id, &tail).unwrap();
        assert_eq!(resumed.objects.read_object(tail_id).unwrap(), tail);
    }

    #[cfg(feature = "test-instrumentation")]
    #[test]
    fn durable_batches_hash_unique_rows_once_and_move_on_last_use() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-read-batch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = crate::schema::StoreDb::create(root.join("store.sqlite")).unwrap();
        let first = layerfs_content::encode_bytes_object(b"first").unwrap();
        let second = layerfs_content::encode_bytes_object(b"second").unwrap();
        let corrupt = layerfs_content::encode_bytes_object(b"corrupt-id").unwrap();
        let first_id = ObjectId::for_bytes(&first);
        let second_id = ObjectId::for_bytes(&second);
        let corrupt_id = ObjectId::for_bytes(&corrupt);
        {
            let connection = db.writer().unwrap();
            for (id, bytes) in [
                (first_id, first.as_slice()),
                (second_id, second.as_slice()),
                (corrupt_id, second.as_slice()),
            ] {
                connection
                    .execute(
                        crate::statements::objects::INSERT,
                        rusqlite::params![id.as_bytes().as_slice(), bytes],
                    )
                    .unwrap();
            }
        }

        reset_read_batch_counters();
        let rows = db
            .read_object_rows(&[first_id, second_id, first_id])
            .unwrap();
        assert_eq!(
            rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![first_id, second_id, first_id]
        );
        assert_eq!(
            read_batch_counters(),
            ReadBatchCounters {
                unique_hashes: 2,
                cloned_bytes: first.len() as u64,
            }
        );
        assert!(db
            .read_object_rows(&[ObjectId::for_bytes(b"missing")])
            .is_err());
        assert!(db.read_object_rows(&[corrupt_id]).is_err());

        struct Claimed(Vec<CanonicalObject>);
        impl ObjectSource for Claimed {
            fn read_object(&self, _: ObjectId) -> Result<Vec<u8>> {
                Err(StoreError::Integrity("unexpected single read"))
            }

            fn read_authenticated_objects(&self, _: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
                Ok(self.0.clone())
            }
        }
        let reversed = Claimed(vec![
            CanonicalObject {
                id: second_id,
                bytes: second.clone(),
            },
            CanonicalObject {
                id: first_id,
                bytes: first.clone(),
            },
        ]);
        assert!(CoreReader(&reversed)
            .get_authenticated_batch(&[first_id, second_id], |_, _| Ok(()))
            .is_err());
        let short = Claimed(vec![CanonicalObject {
            id: first_id,
            bytes: first,
        }]);
        assert!(CoreReader(&short)
            .get_authenticated_batch(&[first_id, second_id], |_, _| Ok(()))
            .is_err());
        struct Untrusted(Vec<u8>);
        impl ObjectSource for Untrusted {
            fn read_object(&self, _: ObjectId) -> Result<Vec<u8>> {
                Ok(self.0.clone())
            }
        }
        assert!(CoreReader(&Untrusted(second))
            .get_authenticated_batch(&[corrupt_id], |_, _| Ok(()))
            .is_err());

        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
