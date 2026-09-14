mod admission;
#[cfg(test)]
mod comparison_reuse_tests;
mod delta;
mod diagnostic;
pub(crate) mod metadata;
#[cfg(unix)]
pub mod scratch;
pub(crate) use admission::PreparedAdmission;
mod pack;
mod read;
pub(crate) mod small_candidates;
mod spill;
mod whole;
#[cfg(test)]
use spill::SeenStorage;
pub use spill::SpillableObjectSet;
use spill::{
    temporary_file, temporary_output_file, IdOrder, ScratchContext, SpillFile, SpillObjects,
    TempPath,
};

use crate::{Result, StoreError};
use layerfs_content::filesystem::{self, ContentChange, ReconcileConflict};
use layerfs_content::object::access::{ObjectRead, ObjectStore};
use layerfs_content::object::references::referenced_objects;
use layerfs_content::{CoreError, CoreResult, ObjectId};
use rusqlite::{params_from_iter, types::Value, Connection};
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
pub const ADMISSION_BATCH_COUNT: usize = 8191;
pub const ADMISSION_BATCH_BYTES: usize = OBJECT_PAGE_BYTES - 1;
// The public count is a ceiling, not a target. FILE provenance adds live
// associations beside the codec's 1 MiB context and retained pack buffers.
const PHYSICAL_ADMISSION_BATCH_COUNT: usize = INITIALIZATION_SLAB_OBJECTS;
pub(crate) const INITIALIZATION_ADMISSION_BATCH_COUNT: usize = ADMISSION_BATCH_COUNT;
pub(crate) const INITIALIZATION_SLAB_BYTES: usize = 256 * 1024;
pub(crate) const INITIALIZATION_SLAB_OBJECTS: usize = 512;
pub(crate) const INITIALIZATION_SLAB_QUEUE_SLOTS: usize = 4;
// Four producer slabs plus two bounded raw buffers each: 2 MiB; queued slabs:
// 1 MiB; admission input/pack ownership: <=1 MiB; static encoder: 2 MiB.
// These fit the existing 6-MiB data ledger; operand/handoff scratch uses the
// existing 2-MiB physical ledger. Predecessor-bearing work remains at one worker.
pub(crate) const SMALL_CONTENT_WORKERS: usize = 4;
const CANDIDATE_MEMORY_BYTES: usize = 8 * 1024 * 1024;
const CANDIDATE_INDEX_BYTES: usize = 64 * 1024 * 1024;
const FRESH_ADMISSION_FILTER_BYTES: usize = 4 * 1024 * 1024;
const COMPARISON_REUSE_BYTES: usize = 2 * 1024 * 1024;
// Covers a whole B-tree node even when it contains only one retained entry.
const COMPARISON_REUSE_ENTRY_BYTES: usize = 2048;
// C: cumulative metadata-lookup allowance, not resident memory. The frozen
// full157 bound is 3763 attached flat-root cursors * 2 grants * 131136 bytes.
const CORRESPONDENCE_OPERATION_RESERVATION_BYTES: u64 = 1024 * 1024 * 1024;
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

pub(crate) struct FinalizedObjectSlab {
    pub objects: Vec<AuthenticatedCanonicalObject>,
    pub payload_bytes: usize,
}

/// Immutable ownership of bytes whose identity and complete outer framing were checked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AuthenticatedCanonicalObject(CanonicalObject, PhysicalHints);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PhysicalHints {
    prior_ids: [Option<ObjectId>; 4],
    first_span: Option<(u64, u32)>,
    has_predecessor: bool,
    // Candidate-search signature precomputed by parallel output producers.
    small_signature: Option<[u64; 8]>,
    diagnostic: u8,
    diagnostic_grants: u8,
}

// Diagnostic bytes must not alter canonical batching or association reservations.
#[allow(dead_code)]
struct UndiagnosedHints {
    prior_ids: [Option<ObjectId>; 4],
    first_span: Option<(u64, u32)>,
    has_predecessor: bool,
    small_signature: Option<[u64; 8]>,
}
const _: () = {
    assert!(std::mem::size_of::<PhysicalHints>() == std::mem::size_of::<UndiagnosedHints>());
    assert!(std::mem::align_of::<PhysicalHints>() == std::mem::align_of::<UndiagnosedHints>());
    assert!(
        std::mem::size_of::<AuthenticatedCanonicalObject>()
            == std::mem::size_of::<(CanonicalObject, UndiagnosedHints)>()
    );
};

impl std::ops::Deref for AuthenticatedCanonicalObject {
    type Target = CanonicalObject;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl AsRef<CanonicalObject> for AuthenticatedCanonicalObject {
    fn as_ref(&self) -> &CanonicalObject {
        &self.0
    }
}
impl AsRef<CanonicalObject> for CanonicalObject {
    fn as_ref(&self) -> &CanonicalObject {
        self
    }
}
impl AuthenticatedCanonicalObject {
    fn is_small_content(&self) -> bool {
        layerfs_content::decode_bytes_object(&self.bytes)
            .is_ok_and(|v| v.starts_with(layerfs_content::file::content::MAGIC))
    }

    /// Real file-owner provenance, independent of diagnostic outcome counters.
    fn is_file_payload(&self) -> bool {
        self.1.diagnostic & diagnostic::FILE != 0
            && (self.is_small_content()
                || (diagnostic::chunk(&self.bytes) && self.bytes.len() + 9 <= pack::GROUP_LIMIT))
    }

    pub(crate) fn prior_ids(&self) -> &[Option<ObjectId>; 4] {
        &self.1.prior_ids
    }
    fn new(bytes: Vec<u8>, expected: Option<ObjectId>) -> CoreResult<Self> {
        if layerfs_content::decode_bytes_object(&bytes)
            .is_ok_and(|v| v.starts_with(layerfs_content::file::content::MAGIC))
        {
            layerfs_content::file::content::small_bytes(&bytes)?;
        }
        let id = match expected {
            Some(id) => {
                layerfs_content::authenticate_identity(&bytes, id)?;
                id
            }
            None => layerfs_content::identify_canonical(&bytes)?.0,
        };
        Ok(Self(
            CanonicalObject { id, bytes },
            PhysicalHints::default(),
        ))
    }
}

fn is_inode_table_leaf(canonical: &[u8]) -> CoreResult<bool> {
    let value = layerfs_content::decode_bytes_object(canonical)?;
    if value.starts_with(b"LFS6INT\0") {
        return Ok(matches!(
            layerfs_content::tree::compact::decode_inode(canonical)?,
            layerfs_content::tree::compact::InodeNode::Leaf(_)
        ));
    }
    if !value.starts_with(b"LFS4INT\0") {
        return Ok(false);
    }
    Ok(matches!(
        layerfs_content::tree::inode::codec::decode_inode_table_node(canonical)?,
        layerfs_content::tree::inode::codec::InodeTableNodeV1::Leaf(_)
    ))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OutputWriterMetrics {
    pub selected_memory_bytes: u64,
    pub selected_spill_bytes: u64,
    pub selected_storage_authentication_ns: u64,
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

impl OutputWriterMetrics {
    // Work counts/time totals sum. Per-worker slab peaks remain maxima; the sum
    // of structural peaks is a conservative simultaneous-memory bound.
    pub(crate) fn merge(&mut self, other: Self) {
        macro_rules! sum { ($($field:ident),* $(,)?) => { $(self.$field = self.$field.saturating_add(other.$field);)* }; }
        macro_rules! peak { ($($field:ident),* $(,)?) => { $(self.$field = self.$field.max(other.$field);)* }; }
        sum!(
            selected_memory_bytes,
            selected_spill_bytes,
            selected_storage_authentication_ns,
            handoffs,
            objects,
            payload_bytes,
            payload_capacity_bytes,
            canonical_hash_calls,
            blocked_ns,
            candidate_copy_bytes,
            parent_payload_copy_bytes,
            structural_peak_bytes,
            producer_tasks,
            producer_files,
            producer_bytes
        );
        peak!(
            partial_peak_objects,
            partial_peak_payload_bytes,
            producer_wall_ns,
            producer_completion_offset_ns
        );
    }
}

#[derive(Default)]
pub(crate) struct OutputQueueMetrics {
    queued: AtomicU64,
    queued_bytes: AtomicU64,
    peak: AtomicU64,
    peak_bytes: AtomicU64,
}

impl OutputQueueMetrics {
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

#[derive(Default)]
pub(crate) struct OutputPipelineMetrics {
    pub wall_ns: u64,
    pub consumer_idle_ns: u64,
    pub last_receive_ns: u64,
    pub queue_peak: u64,
    pub queue_peak_bytes: u64,
    pub producer_peak: u64,
    pub producers_after: u64,
}

// One bounded ownership/drain/join implementation for native and Workspace inputs.
// Keep lifecycle callbacks explicit rather than introducing a configuration wrapper.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_finalized_output<I, S: Send, T: Send>(
    worker_limit: usize,
    task_count: usize,
    tasks: I,
    cancelled: &std::sync::atomic::AtomicBool,
    initialize: impl Fn(usize) -> Result<S> + Sync,
    step: impl Fn(&mut S, usize, I::Item, &mut FinalizedOutputWriter) -> Result<()> + Sync,
    finish: impl Fn(S) -> Result<T> + Sync,
    mut consume: impl FnMut(Vec<AuthenticatedCanonicalObject>) -> Result<()>,
) -> Result<(Vec<(T, OutputWriterMetrics)>, OutputPipelineMetrics)>
where
    I: Iterator + Send,
    I::Item: Send,
{
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::atomic::Ordering;
    if worker_limit == 0 && task_count != 0 {
        return Err(StoreError::InvalidInput("canonical output worker limit"));
    }
    let workers = worker_limit.min(task_count);
    let started = Instant::now();
    let tasks = Mutex::new(tasks.enumerate());
    let claimed = AtomicU64::new(0);
    let completed = AtomicU64::new(0);
    let queue = std::sync::Arc::new(OutputQueueMetrics::default());
    let (sender, receiver) = std::sync::mpsc::sync_channel(INITIALIZATION_SLAB_QUEUE_SLOTS);
    let active = AtomicU64::new(0);
    let peak = AtomicU64::new(0);
    let mut metrics = OutputPipelineMetrics::default();
    let output = std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|index| {
                let mut writer = FinalizedOutputWriter::new(sender.clone(), queue.clone());
                let (initialize, step, finish) = (&initialize, &step, &finish);
                let (tasks, claimed, completed, active, peak) =
                    (&tasks, &claimed, &completed, &active, &peak);
                scope.spawn(move || {
                    struct Active<'a>(&'a AtomicU64);
                    impl Drop for Active<'_> {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::AcqRel);
                        }
                    }
                    let count = active.fetch_add(1, Ordering::AcqRel) + 1;
                    peak.fetch_max(count, Ordering::Relaxed);
                    let _active = Active(active);
                    let producer_started = Instant::now();
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        let mut state = initialize(index)?;
                        let mut task_total = 0_u64;
                        loop {
                            if cancelled.load(Ordering::Acquire) {
                                break;
                            }
                            let next = {
                                let mut tasks = tasks
                                    .lock()
                                    .map_err(|_| StoreError::Integrity("canonical task source"))?;
                                if cancelled.load(Ordering::Acquire) {
                                    None
                                } else {
                                    tasks.next()
                                }
                            };
                            let Some((ordinal, task)) = next else { break };
                            if ordinal >= task_count {
                                return Err(StoreError::Integrity("canonical task count"));
                            }
                            claimed.fetch_add(1, Ordering::Relaxed);
                            step(&mut state, ordinal, task, &mut writer)?;
                            task_total += 1;
                            completed.fetch_add(1, Ordering::Relaxed);
                        }
                        // Result journals seal before the final output flush. Neither a
                        // finish error nor a writer error may publish successful coverage.
                        let output = finish(state)?;
                        let mut writer = writer.finish()?;
                        writer.producer_tasks = task_total;
                        writer.producer_wall_ns = elapsed_ns(producer_started);
                        writer.producer_completion_offset_ns = elapsed_ns(started);
                        Ok((output, writer))
                    }))
                    .unwrap_or_else(|_| {
                        Err(StoreError::Integrity("canonical output producer panic"))
                    });
                    if result.is_err() {
                        cancelled.store(true, Ordering::Release);
                    }
                    result
                })
            })
            .collect::<Vec<_>>();
        drop(sender);
        let mut failure = None;
        loop {
            let waiting = Instant::now();
            let received = receiver.recv();
            metrics.consumer_idle_ns = metrics.consumer_idle_ns.saturating_add(elapsed_ns(waiting));
            let Ok(slab) = received else { break };
            metrics.last_receive_ns = elapsed_ns(started);
            queue.received(slab.payload_bytes);
            if failure.is_none() {
                let result = catch_unwind(AssertUnwindSafe(|| consume(slab.objects))).unwrap_or(
                    Err(StoreError::Integrity("canonical output consumer panic")),
                );
                if let Err(error) = result {
                    failure = Some(error);
                    cancelled.store(true, Ordering::Release);
                }
            }
            // Drain even after failure: every sender and retained source owner joins.
        }
        let mut output = Vec::with_capacity(workers);
        for handle in handles {
            match handle.join() {
                Ok(Ok(result)) => output.push(result),
                Ok(Err(error)) => {
                    failure.get_or_insert(error);
                }
                Err(_) => {
                    failure.get_or_insert(StoreError::Integrity("canonical output producer"));
                }
            }
        }
        if failure.is_none()
            && !cancelled.load(Ordering::Acquire)
            && (claimed.load(Ordering::Relaxed) != task_count as u64
                || completed.load(Ordering::Relaxed) != task_count as u64
                || tasks
                    .lock()
                    .map_err(|_| StoreError::Integrity("canonical task source"))?
                    .next()
                    .is_some())
        {
            failure = Some(StoreError::Integrity("canonical task coverage"));
            cancelled.store(true, Ordering::Release);
        }
        failure.map_or(Ok(output), Err)
    })?;
    metrics.wall_ns = elapsed_ns(started);
    metrics.queue_peak = queue.peak();
    metrics.queue_peak_bytes = queue.peak_bytes();
    metrics.producer_peak = peak.load(Ordering::Relaxed);
    metrics.producers_after = active.load(Ordering::Acquire);
    Ok((output, metrics))
}

pub(crate) struct NamespaceAllocation {
    scope: ObjectId,
    range: std::ops::Range<u64>,
}
impl NamespaceAllocation {
    pub(crate) fn new(scope: ObjectId, range: std::ops::Range<u64>) -> Self {
        Self { scope, range }
    }
    fn allocate(
        &mut self,
        scope: ObjectId,
    ) -> CoreResult<layerfs_content::tree::compact::InodeSerial> {
        if scope != self.scope {
            return Err(CoreError::ProfileMismatch);
        }
        layerfs_content::tree::compact::InodeSerial::new(
            self.range.next().ok_or(CoreError::ObjectLimitExceeded)?,
        )
    }
}

pub struct FinalizedOutputWriter {
    namespace: Option<NamespaceAllocation>,
    small_predecessor: Option<ObjectId>,
    small_content_format: bool,
    small_chain_format: bool,
    file_payload_context: bool,
    sender: std::sync::mpsc::SyncSender<FinalizedObjectSlab>,
    queue: std::sync::Arc<OutputQueueMetrics>,
    objects: Vec<AuthenticatedCanonicalObject>,
    payload_bytes: usize,
    metrics: OutputWriterMetrics,
}

pub(crate) struct InitializationDirectAdmissionWriter<'admission> {
    namespace: Option<NamespaceAllocation>,
    file_payload_context: bool,
    admission: &'admission mut CheckedOutputAdmission,
    error: Option<StoreError>,
    transient_owned_bytes: u64,
    pub metrics: OutputWriterMetrics,
}

impl<'admission> InitializationDirectAdmissionWriter<'admission> {
    pub(crate) fn set_namespace_allocation(&mut self, allocation: Option<NamespaceAllocation>) {
        self.namespace = allocation;
    }
    pub(crate) fn new(admission: &'admission mut CheckedOutputAdmission) -> Self {
        Self {
            namespace: None,
            file_payload_context: false,
            admission,
            error: None,
            transient_owned_bytes: 0,
            metrics: OutputWriterMetrics::default(),
        }
    }

    pub(crate) fn error(&mut self, fallback: CoreError) -> StoreError {
        self.error.take().unwrap_or_else(|| fallback.into())
    }

    fn push_owned(&mut self, canonical: Vec<u8>, copied: bool) -> CoreResult<ObjectId> {
        let object = AuthenticatedCanonicalObject::new(canonical, None)?;
        self.push_object(object, copied)
    }

    fn push_object(
        &mut self,
        object: AuthenticatedCanonicalObject,
        copied: bool,
    ) -> CoreResult<ObjectId> {
        let id = object.id;
        self.metrics.canonical_hash_calls += 1;
        let bytes = object.bytes.len() as u64;
        self.metrics.payload_capacity_bytes = self
            .metrics
            .payload_capacity_bytes
            .saturating_add(object.bytes.capacity() as u64);
        self.admission
            .observe_final_owned_bytes(self.transient_owned_bytes, object.bytes.capacity() as u64);
        if let Err(error) = self.admission.admit_object(object) {
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

impl ObjectStore for InitializationDirectAdmissionWriter<'_> {
    fn compact_namespace(&self) -> bool {
        self.namespace.is_some()
    }
    fn allocate_inode_serial(
        &mut self,
        scope: ObjectId,
    ) -> CoreResult<layerfs_content::tree::compact::InodeSerial> {
        self.namespace
            .as_mut()
            .ok_or(CoreError::Unsupported)?
            .allocate(scope)
    }
    fn small_content_format(&self) -> bool {
        self.admission.db.small_content_format()
    }

    fn set_file_payload_context(&mut self, enabled: bool) -> bool {
        std::mem::replace(&mut self.file_payload_context, enabled)
    }

    fn put_file_payload(
        &mut self,
        canonical: Vec<u8>,
        start: u64,
        len: u32,
    ) -> CoreResult<ObjectId> {
        let mut object = AuthenticatedCanonicalObject::new(canonical, None)?;
        object.1.first_span = Some((start, len));
        if self.file_payload_context {
            object.1.diagnostic = diagnostic::FILE;
        }
        self.push_object(object, false)
    }

    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        if let Some(index) = self.admission.incoming_index.get(&id) {
            return Ok(self.admission.incoming[*index].bytes.clone());
        }
        if let Some(index) = self.admission.pending.get(&id) {
            return Ok(self.admission.batch[*index].bytes.clone());
        }
        self.admission
            .db
            .read_object_row(id)
            .map_err(core_read_error)
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

impl FinalizedOutputWriter {
    pub(crate) fn set_namespace_allocation(&mut self, allocation: Option<NamespaceAllocation>) {
        self.namespace = allocation;
    }
    pub fn set_small_content_format(&mut self, enabled: bool) {
        self.small_content_format = enabled;
    }
    pub fn set_small_chain_format(&mut self, enabled: bool) {
        self.small_chain_format = enabled;
    }
    pub fn supports_small_content(&self) -> bool {
        self.small_content_format
    }
    pub fn build_small_file(
        &mut self,
        source: impl Read,
        len: u64,
        predecessor: Option<ObjectId>,
    ) -> Result<(ObjectId, BuildCounters)> {
        if !self.small_content_format
            || len == 0
            || len >= layerfs_content::file::content::SMALL_LIMIT as u64
        {
            return Err(StoreError::InvalidInput("small file construction"));
        }
        let previous = std::mem::replace(&mut self.small_predecessor, predecessor);
        let result = self.build_complete_file(source, len);
        self.small_predecessor = previous;
        result
    }
    pub(crate) fn new(
        sender: std::sync::mpsc::SyncSender<FinalizedObjectSlab>,
        queue: std::sync::Arc<OutputQueueMetrics>,
    ) -> Self {
        Self {
            small_predecessor: None,
            namespace: None,
            small_content_format: false,
            small_chain_format: false,
            file_payload_context: false,
            sender,
            queue,
            objects: Vec::with_capacity(INITIALIZATION_SLAB_OBJECTS),
            payload_bytes: 0,
            metrics: OutputWriterMetrics::default(),
        }
    }

    /// Accept only completed selected output from ObjectBuffer::finish or the
    /// complete-file builder's established finality contract.
    pub fn send_selected(&mut self, objects: DeferredObjectStore) -> Result<()> {
        let bytes = objects.encoded_bytes();
        if matches!(objects.storage, DeferredObjects::Spill(_)) {
            self.metrics.selected_spill_bytes =
                self.metrics.selected_spill_bytes.saturating_add(bytes);
        } else {
            self.metrics.selected_memory_bytes =
                self.metrics.selected_memory_bytes.saturating_add(bytes);
        }
        let storage_authentication_ns = objects.consume_prevalidated_pages(|page| {
            for object in page {
                self.push_object(object, false)?;
            }
            Ok(())
        })?;
        self.metrics.selected_storage_authentication_ns = self
            .metrics
            .selected_storage_authentication_ns
            .saturating_add(storage_authentication_ns);
        Ok(())
    }

    /// Build a new complete file using the owner's unpublished admission session.
    /// The caller must discard the operation on any error, including a late EOF check.
    pub fn build_complete_file(
        &mut self,
        source: impl Read,
        expected_len: u64,
    ) -> Result<(ObjectId, BuildCounters)> {
        let before = self.metrics.canonical_hash_calls;
        let before_payload = self.metrics.payload_bytes;
        let completed = build_checked_file(self, source, expected_len)?;
        self.metrics.selected_memory_bytes += self.metrics.payload_bytes - before_payload;
        Ok((
            completed.root.0,
            BuildCounters {
                cdc_bytes_scanned: completed.counters.cdc_bytes_scanned,
                encode_hash_invocations: self.metrics.canonical_hash_calls - before,
                ..BuildCounters::default()
            },
        ))
    }

    pub(crate) fn finish(mut self) -> Result<OutputWriterMetrics> {
        self.flush()?;
        Ok(self.metrics)
    }

    fn flush(&mut self) -> Result<()> {
        if self.objects.is_empty() {
            return Ok(());
        }
        let slab = FinalizedObjectSlab {
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
        let object = AuthenticatedCanonicalObject::new(canonical, None)?;
        let id = object.id;
        self.metrics.canonical_hash_calls += 1;
        self.push_object(object, copied)?;
        Ok(id)
    }

    fn push_object(
        &mut self,
        object: AuthenticatedCanonicalObject,
        copied: bool,
    ) -> CoreResult<()> {
        let canonical = &object.bytes;
        if canonical.len() > INITIALIZATION_SLAB_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        if !self.objects.is_empty()
            && (self.objects.len() == INITIALIZATION_SLAB_OBJECTS
                || self.payload_bytes.saturating_add(canonical.len()) > INITIALIZATION_SLAB_BYTES)
        {
            self.flush().map_err(|_| CoreError::Io)?;
        }
        self.payload_bytes += canonical.len();
        self.metrics.objects += 1;
        self.metrics.payload_bytes = self
            .metrics
            .payload_bytes
            .saturating_add(canonical.len() as u64);
        self.metrics.payload_capacity_bytes = self
            .metrics
            .payload_capacity_bytes
            .saturating_add(object.bytes.capacity() as u64);
        if copied {
            self.metrics.candidate_copy_bytes = self
                .metrics
                .candidate_copy_bytes
                .saturating_add(canonical.len() as u64);
        }
        self.objects.push(object);
        self.metrics.partial_peak_objects = self
            .metrics
            .partial_peak_objects
            .max(self.objects.len() as u64);
        self.metrics.partial_peak_payload_bytes = self
            .metrics
            .partial_peak_payload_bytes
            .max(self.payload_bytes as u64);
        Ok(())
    }
}

impl ObjectStore for FinalizedOutputWriter {
    fn compact_namespace(&self) -> bool {
        self.namespace.is_some()
    }
    fn allocate_inode_serial(
        &mut self,
        scope: ObjectId,
    ) -> CoreResult<layerfs_content::tree::compact::InodeSerial> {
        self.namespace
            .as_mut()
            .ok_or(CoreError::Unsupported)?
            .allocate(scope)
    }
    fn small_content_format(&self) -> bool {
        self.small_content_format
    }

    fn set_file_payload_context(&mut self, enabled: bool) -> bool {
        std::mem::replace(&mut self.file_payload_context, enabled)
    }

    fn put_file_payload(
        &mut self,
        canonical: Vec<u8>,
        start: u64,
        len: u32,
    ) -> CoreResult<ObjectId> {
        let mut object = AuthenticatedCanonicalObject::new(canonical, None)?;
        if object.is_small_content() {
            object.1.prior_ids[0] = self.small_predecessor;
            object.1.has_predecessor = self.small_predecessor.is_some();
            // Producer-side candidate-search signature: the admission consumer
            // would otherwise rescan these immutable bytes on its serial path.
            // Explicit-predecessor targets keep the lazy consumer fallback,
            // and Stores without the small-chain format never precompute.
            if self.small_chain_format && self.small_predecessor.is_none() {
                let raw = layerfs_content::file::content::small_bytes(&object.bytes)?
                    .ok_or(CoreError::InvalidRecord("SmallContent role"))?;
                object.1.small_signature = Some(small_candidates::signature(raw));
            }
        }
        object.1.first_span = Some((start, len));
        if self.file_payload_context {
            object.1.diagnostic = diagnostic::FILE;
        }
        let id = object.id;
        self.metrics.canonical_hash_calls += 1;
        self.push_object(object, false)?;
        Ok(id)
    }

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
    fn small_content_format(&self) -> bool {
        false
    }
    fn compact_namespace(&self) -> bool {
        false
    }
    fn allocate_inode_serial(
        &self,
        _scope: ObjectId,
    ) -> Result<layerfs_content::tree::compact::InodeSerial> {
        Err(StoreError::InvalidInput("inode allocator unavailable"))
    }

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

    /// The existing bounded packed batch path: one demand wave over the whole
    /// batch, grouped and sorted by physical location, so each physical group is
    /// selected and decompressed once for all of its demands. Every object is
    /// authenticated and canonical-format checked exactly as the point route
    /// checks it, and the original demand order is preserved.
    fn get_authenticated_canonical_batch<F>(
        &self,
        ids: &[ObjectId],
        mut callback: F,
    ) -> CoreResult<()>
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
            callback(object.id, &object.bytes)?;
        }
        Ok(())
    }
}

/// Whether one demanded object is already owned by this buffer.
fn owns_batch_demand(objects: &DeferredObjectStore, id: ObjectId) -> CoreResult<bool> {
    match &objects.storage {
        DeferredObjects::Memory { rows, .. } => Ok(rows.contains_key(&id)),
        DeferredObjects::Spill(_) => Ok(objects.get(id).map_err(core_read_error)?.is_some()),
    }
}

/// Answer one batch demand this buffer owns, with the same check the point route
/// performs on an owned object.
fn emit_owned_batch_demand<F>(
    objects: &DeferredObjectStore,
    id: ObjectId,
    callback: &mut F,
) -> CoreResult<()>
where
    F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
{
    match &objects.storage {
        DeferredObjects::Memory { rows, .. } => {
            let object = rows.get(&id).ok_or(CoreError::MissingObject)?;
            callback(id, &object.bytes)
        }
        DeferredObjects::Spill(_) => {
            let bytes = objects
                .get(id)
                .map_err(core_read_error)?
                .ok_or(CoreError::MissingObject)?;
            callback(id, &bytes)
        }
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
    small_predecessor: Option<ObjectId>,
    storage: DeferredObjects,
    reachable: IdOrder,
    references: Option<BTreeMap<ObjectId, Vec<ObjectId>>>,
    reference_bytes: usize,
    count: u64,
    encoded_bytes: u64,
    first_store_write_bytes: u64,
    spill_peak_bytes: u64,
    spill_count: u64,
    memory_limit: usize,
    index_limit: usize,
    spill_buffer_bytes: usize,
    order_memory_bytes: usize,
    diagnostic_file_context: bool,
    predecessor: Option<(
        crate::SnapshotReader,
        layerfs_content::file::rope::FileStateRoot,
        std::sync::Arc<std::sync::atomic::AtomicU64>,
        bool,
    )>,
    scratch: ScratchContext,
    // Drop last: private buffers and predecessor readers remain accounted first.
    private_owner: Option<std::sync::Arc<dyn Send + Sync>>,
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
    width: usize,
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
    width: usize,
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
    width: usize,
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
        rows: BTreeMap<ObjectId, AuthenticatedCanonicalObject>,
        bytes: usize,
    },
    Spill(SpillObjects),
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
            path: TempPath::new(path),
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
        Self::with_width(pending_limit, 64)
    }
    pub(crate) fn new_inline(pending_limit: usize) -> Result<Self> {
        Self::with_width(pending_limit, 81)
    }
    pub(crate) fn inline(&self) -> bool {
        self.width == 81
    }
    fn with_width(pending_limit: usize, width: usize) -> Result<Self> {
        if pending_limit < width {
            return Err(StoreError::InvalidInput("inode pair segment buffer"));
        }
        let (writer, path) = temporary_file("initialization-inode-pairs")?;
        let reader = std::fs::File::open(&path)?;
        Ok(Self {
            width,
            writer,
            reader,
            path: TempPath::new(path),
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
        if self.width != 64 {
            return Err(StoreError::Integrity("inode pair format"));
        }
        let mut bytes = [0; 64];
        bytes[..32].copy_from_slice(inode.as_bytes());
        bytes[32..].copy_from_slice(record.as_bytes());
        self.push_bytes(&bytes)
    }

    pub(crate) fn push_inline(
        &mut self,
        serial: layerfs_content::tree::compact::InodeSerial,
        record: layerfs_content::tree::inode::InodeRecordV1,
    ) -> Result<()> {
        if self.width != 81 {
            return Err(StoreError::Integrity("inode pair format"));
        }
        let mut bytes = [0; 81];
        bytes[..8].copy_from_slice(&serial.get().to_be_bytes());
        bytes[8..].copy_from_slice(&layerfs_content::tree::compact::encode_inode_value(record));
        self.push_bytes(&bytes)
    }

    fn push_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() != self.width {
            return Err(StoreError::Integrity("inode pair width"));
        }
        if !self.pending.is_empty() && self.pending.len() + self.width > self.pending_limit {
            self.flush()?;
        }
        self.pending.extend_from_slice(bytes);
        self.end = self
            .end
            .checked_add(self.width as u64)
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
            width,
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
            width,
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
    #[cfg(test)]
    fn read_pair(&mut self, end: u64) -> Result<(layerfs_content::tree::inode::InodeId, ObjectId)> {
        if self.width != 64 {
            return Err(StoreError::Integrity("inode pair format"));
        }
        let row = self.read_raw_pair(end)?;
        Ok((
            layerfs_content::tree::inode::InodeId::from_slice(&row[..32])?,
            ObjectId::from_bytes(&row[32..64])?,
        ))
    }
    fn read_raw_pair(&mut self, block_end: u64) -> Result<[u8; 81]> {
        let next = self
            .cursor
            .checked_add(self.width as u64)
            .ok_or(StoreError::Integrity("inode pair segment length"))?;
        if next > block_end {
            return Err(StoreError::Integrity("inode pair block length"));
        }
        let mut pair = [0; 81];
        self.reader.read_exact(&mut pair[..self.width])?;
        self.cursor = next;
        self.read_pairs += 1;
        Ok(pair)
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
        let width = segments.first().map_or(64, |segment| segment.width);
        if !matches!(width, 64 | 81)
            || segments.iter().any(|segment| segment.width != width)
            || blocks.len() > 1_000
            || blocks.iter().enumerate().any(|(task, block)| {
                block.task_ordinal != task
                    || block.worker_index >= segments.len()
                    || block.start > block.end
                    || block.pair_count.checked_mul(width as u64) != Some(block.end - block.start)
            })
        {
            return Err(StoreError::Integrity("inode pair block order"));
        }
        Ok(Self {
            width,
            segments,
            blocks: blocks.into_iter(),
            current: None,
            last_task: None,
            done: false,
        })
    }

    fn fail(&mut self, error: StoreError) -> Option<CoreResult<[u8; 81]>> {
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
            if segment.end != segment.pairs.saturating_mul(segment.width as u64)
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

impl CompactInodePairStream {
    fn next_raw(&mut self) -> Option<CoreResult<[u8; 81]>> {
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
                match segment.read_raw_pair(block.end) {
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

impl Iterator for CompactInodePairStream {
    type Item = CoreResult<(layerfs_content::tree::inode::InodeId, ObjectId)>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        if self.width != 64 {
            self.done = true;
            return Some(Err(CoreError::InvalidRecord("inode pair format")));
        }
        self.next_raw().map(|row| {
            let row = row?;
            Ok((
                layerfs_content::tree::inode::InodeId::from_slice(&row[..32])?,
                ObjectId::from_bytes(&row[32..64])?,
            ))
        })
    }
}
impl CompactInodePairStream {
    pub(crate) fn next_inline(
        &mut self,
    ) -> Option<
        CoreResult<(
            layerfs_content::tree::compact::InodeSerial,
            layerfs_content::tree::inode::InodeRecordV1,
        )>,
    > {
        if self.done {
            return None;
        }
        if self.segments.is_empty() && self.blocks.len() == 0 {
            self.done = true;
            return None;
        }
        if self.width != 81 {
            self.done = true;
            return Some(Err(CoreError::InvalidRecord("inode pair format")));
        }
        self.next_raw().map(|row| {
            let row = row?;
            Ok((
                layerfs_content::tree::compact::InodeSerial::new(u64::from_be_bytes(
                    row[..8].try_into().unwrap(),
                ))?,
                layerfs_content::tree::compact::decode_inode_value(&row[8..])?,
            ))
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ObjectInsertMetrics {
    pub sql: AdmissionSqlMetrics,
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
    pub fresh_probe_ids_skipped: u64,
    pub probe_ids_queried: u64,
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
        let ordinal = self.sql_batch_count + metrics.sql.max_commit_offset;
        self.sql_batch_count += metrics.sql.commits;
        if metrics.submitted_rows != 0 {
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
        match phase {
            InitializationSqlPhase::Pipeline => {
                self.pipeline_commit_count += metrics.sql.commits;
                self.pipeline_commit_ns = self.pipeline_commit_ns.saturating_add(commit_ns);
                if metrics.sql.max_commit_ns > self.pipeline_commit_max_ns {
                    self.pipeline_commit_max_ns = metrics.sql.max_commit_ns;
                    self.pipeline_commit_max_ordinal = ordinal;
                }
            }
            InitializationSqlPhase::FinalBuild => {
                self.final_build_commit_count += metrics.sql.commits;
                self.final_build_commit_ns = self.final_build_commit_ns.saturating_add(commit_ns);
                if metrics.sql.max_commit_ns > self.final_build_commit_max_ns {
                    self.final_build_commit_max_ns = metrics.sql.max_commit_ns;
                    self.final_build_commit_max_ordinal = ordinal;
                }
            }
            InitializationSqlPhase::Publication => {
                self.publication_commit_ns = commit_ns;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CheckedAdmission {
    pub candidate_objects: u64,
    pub candidate_bytes: u64,
    pub inserted_objects: u64,
    pub inserted_bytes: u64,
    pub reused_objects: u64,
    pub reused_bytes: u64,
    pub transactions: u64,
    pub max_transaction_objects: u64,
    pub max_transaction_bytes: u64,
    pub begin_ns: u64,
    pub insert_ns: u64,
    pub commit_ns: u64,
}

impl CheckedAdmission {
    fn record(&mut self, metrics: &AdmissionBatchMetrics) {
        let bytes = metrics
            .insert
            .bytes
            .saturating_add(metrics.insert.skipped_bytes);
        self.transactions += metrics.insert.sql.commits;
        self.max_transaction_objects = self
            .max_transaction_objects
            .max(metrics.insert.sql.max_objects);
        self.max_transaction_bytes = self.max_transaction_bytes.max(metrics.insert.sql.max_bytes);
        self.begin_ns = self.begin_ns.saturating_add(metrics.begin_ns);
        self.insert_ns = self.insert_ns.saturating_add(metrics.insert.insert_ns);
        self.commit_ns = self.commit_ns.saturating_add(metrics.commit_ns);
        self.candidate_objects += metrics.insert.submitted_rows;
        self.candidate_bytes = self.candidate_bytes.saturating_add(bytes);
        self.inserted_objects += metrics.insert.objects;
        self.inserted_bytes = self.inserted_bytes.saturating_add(metrics.insert.bytes);
        self.reused_objects += metrics.insert.skipped_ids;
        self.reused_bytes = self
            .reused_bytes
            .saturating_add(metrics.insert.skipped_bytes);
    }
}

pub(crate) struct AdmissionBatchMetrics {
    pub(crate) insert: ObjectInsertMetrics,
    pub(crate) begin_ns: u64,
    pub(crate) commit_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AdmissionSqlMetrics {
    pub commits: u64,
    pub final_commits: u64,
    pub max_objects: u64,
    pub max_bytes: u64,
    pub begin_ns: u64,
    pub commit_ns: u64,
    pub max_commit_ns: u64,
    pub max_commit_offset: u64,
}

#[derive(Default)]
struct AdmissionCohort {
    open: bool,
    objects: u64,
    bytes: u64,
}

impl AdmissionCohort {
    fn commit(
        &mut self,
        connection: &rusqlite::Connection,
        metrics: &mut AdmissionSqlMetrics,
        final_commit: bool,
    ) -> Result<()> {
        if !self.open {
            return Ok(());
        }
        let started = Instant::now();
        connection.execute_batch("COMMIT")?;
        let elapsed = elapsed_ns(started);
        metrics.commits += 1;
        metrics.final_commits += u64::from(final_commit);
        metrics.max_objects = metrics.max_objects.max(self.objects);
        metrics.max_bytes = metrics.max_bytes.max(self.bytes);
        metrics.commit_ns += elapsed;
        if elapsed > metrics.max_commit_ns {
            metrics.max_commit_ns = elapsed;
            metrics.max_commit_offset = metrics.commits;
        }
        *self = Self::default();
        #[cfg(feature = "test-instrumentation")]
        if !final_commit {
            crate::schema::verification_early_committed();
        }
        Ok(())
    }
}

/// The session's still-open pack for one framing lane. Its row already exists,
/// so a later admission of the same lane can append groups instead of opening a
/// new row with its own partially used overflow page.
struct OpenPack {
    pack_id: i64,
    groups: Vec<pack::EncodedGroup>,
    bytes: usize,
    records: usize,
}

/// One admission owns publication until it either retains its output or removes it.
/// Existing pack IDs never change, so the held writer gate makes this high-water
/// mark an exact ownership boundary, including every physical dependency.
pub(crate) struct AdmissionSession {
    db: crate::schema::StoreDb,
    baseline_pack: i64,
    fresh_ids: Option<Mutex<Box<[u64]>>>,
    small_candidates: Option<Mutex<small_candidates::Candidates>>,
    open_packs: Mutex<BTreeMap<u32, OpenPack>>,
    coalesce: bool,
    cohort: Mutex<AdmissionCohort>,
    publication_epoch: AtomicU64,
    state: std::sync::atomic::AtomicU8,
    _permit: crate::schema::OperationPermit,
}

impl AdmissionSession {
    fn new(db: &crate::schema::StoreDb) -> Result<std::sync::Arc<Self>> {
        Self::with_coalescing(db, false)
    }

    fn with_coalescing(
        db: &crate::schema::StoreDb,
        coalesce: bool,
    ) -> Result<std::sync::Arc<Self>> {
        let permit = db.enter_operation()?;
        let (baseline_pack, initially_empty) = {
            let connection = db.reader()?;
            if !connection.is_autocommit() {
                return Err(StoreError::Integrity("admission starts inside transaction"));
            }
            let baseline_pack: i64 = connection.query_row(
                "SELECT COALESCE(MAX(pack_id),0) FROM object_packs",
                [],
                |row| row.get(0),
            )?;
            // Do not infer empty membership solely from pack chronology: retain
            // ordinary lookup even for an orphaned/inconsistent locator table.
            let initially_empty = baseline_pack == 0
                && connection.query_row("SELECT NOT EXISTS(SELECT 1 FROM objects)", [], |row| {
                    row.get::<_, bool>(0)
                })?;
            (baseline_pack, initially_empty)
        };
        let fresh_ids = initially_empty.then(|| {
            Mutex::new(
                vec![0_u64; FRESH_ADMISSION_FILTER_BYTES / std::mem::size_of::<u64>()]
                    .into_boxed_slice(),
            )
        });
        Ok(std::sync::Arc::new(Self {
            db: db.clone(),
            baseline_pack,
            fresh_ids,
            small_candidates: if db.small_chain_format() {
                Some(Mutex::new(
                    db.take_small_candidates()
                        .unwrap_or_else(small_candidates::Candidates::new),
                ))
            } else {
                None
            },
            open_packs: Mutex::new(BTreeMap::new()),
            coalesce,
            cohort: Mutex::new(AdmissionCohort::default()),
            publication_epoch: AtomicU64::new(0),
            state: std::sync::atomic::AtomicU8::new(0),
            _permit: permit,
        }))
    }

    fn begin_batch(
        &self,
        connection: &rusqlite::Connection,
        objects: u64,
        bytes: u64,
        metrics: &mut AdmissionSqlMetrics,
    ) -> Result<()> {
        if objects > ADMISSION_BATCH_COUNT as u64 || bytes > ADMISSION_BATCH_BYTES as u64 {
            return Err(StoreError::Integrity("SQL admission cohort bound"));
        }
        let mut cohort = self
            .cohort
            .lock()
            .map_err(|_| StoreError::Integrity("admission cohort"))?;
        if cohort.open == connection.is_autocommit() {
            return Err(StoreError::Integrity("admission transaction ownership"));
        }
        if cohort.open
            && (cohort.objects + objects > ADMISSION_BATCH_COUNT as u64
                || cohort.bytes + bytes > ADMISSION_BATCH_BYTES as u64)
        {
            cohort.commit(connection, metrics, false)?;
        }
        if !cohort.open {
            let started = Instant::now();
            connection.execute_batch("BEGIN IMMEDIATE")?;
            metrics.begin_ns += elapsed_ns(started);
            cohort.open = true;
        }
        cohort.objects += objects;
        cohort.bytes += bytes;
        Ok(())
    }

    fn commit_pending(
        &self,
        connection: &rusqlite::Connection,
        metrics: &mut AdmissionSqlMetrics,
        final_commit: bool,
    ) -> Result<()> {
        self.cohort
            .lock()
            .map_err(|_| StoreError::Integrity("admission cohort"))?
            .commit(connection, metrics, final_commit)
    }

    /// Append one lane's final group vector to this session's still-open pack of
    /// the same framing, inside the caller's transaction. A merge keeps the pack
    /// bound, the version bytes, the record framing and every locator meaning
    /// unchanged; it only fills the existing 256 KiB `PACK_LIMIT` instead of
    /// opening another partially used row. `None` closes the open pack and lets
    /// the caller publish the pack as a new row.
    fn append_open_pack(
        &self,
        transaction: &rusqlite::Connection,
        version: u32,
        groups: &[pack::EncodedGroup],
        statement_number: &mut u64,
    ) -> Result<Option<(i64, usize)>> {
        let mut open = self
            .open_packs
            .lock()
            .map_err(|_| StoreError::Integrity("admission open pack"))?;
        let Some(entry) = open.get_mut(&version) else {
            return Ok(None);
        };
        let offset = entry.groups.len();
        if groups.is_empty() {
            // The pack contributes only pooled catalogue groups; its row exists.
            return Ok(Some((entry.pack_id, offset)));
        }
        let records = groups.iter().try_fold(entry.records, |sum, group| {
            sum.checked_add(group.records)
                .ok_or(StoreError::Integrity("pack record bound"))
        })?;
        if offset + groups.len() > pack::GROUP_COUNT_LIMIT || records > pack::RECORD_COUNT_LIMIT {
            open.remove(&version);
            return Ok(None);
        }
        // The candidate is assembled before the open pack is modified so a pack
        // that no longer fits closes cleanly and the caller keeps its groups.
        let mut candidate = entry.groups.clone();
        candidate.extend_from_slice(groups);
        let bytes = match pack::assemble_version(version, &candidate) {
            Ok(bytes) if bytes.len() <= pack::PACK_LIMIT => bytes,
            Ok(_) | Err(_) => {
                open.remove(&version);
                return Ok(None);
            }
        };
        let pack_id = entry.pack_id;
        *statement_number += 1;
        crate::schema::fail_transaction_statement(*statement_number)?;
        if transaction
            .prepare_cached("UPDATE object_packs SET data=?2 WHERE pack_id=?1")?
            .execute(rusqlite::params![pack_id, &bytes])?
            != 1
        {
            return Err(StoreError::Integrity("pack append cardinality"));
        }
        entry.groups = candidate;
        entry.bytes = bytes.len();
        entry.records = records;
        Ok(Some((pack_id, offset)))
    }

    /// Register a freshly inserted pack row as this lane's open pack. The group
    /// vector is retained only when the pack has room for a later append.
    fn note_open_pack(
        &self,
        version: u32,
        pack_id: i64,
        bytes: usize,
        groups: Vec<pack::EncodedGroup>,
    ) -> Result<()> {
        let mut open = self
            .open_packs
            .lock()
            .map_err(|_| StoreError::Integrity("admission open pack"))?;
        open.remove(&version);
        if groups.len() < pack::GROUP_COUNT_LIMIT && bytes < pack::PACK_LIMIT {
            let records = groups.iter().map(|group| group.records).sum();
            open.insert(
                version,
                OpenPack {
                    pack_id,
                    groups,
                    bytes,
                    records,
                },
            );
        }
        Ok(())
    }

    fn fresh_bit_positions(id: &ObjectId) -> impl Iterator<Item = usize> + '_ {
        // ObjectId is an authenticated 32-byte digest. Bit collisions only add
        // SQL work; they never establish presence or authorize a dependency.
        id.as_bytes().chunks_exact(8).map(|word| {
            (u64::from_le_bytes(word.try_into().expect("ObjectId word")) as usize)
                & (FRESH_ADMISSION_FILTER_BYTES * 8 - 1)
        })
    }

    fn retain_possible_ids(&self, ids: &mut Vec<ObjectId>) -> Result<()> {
        let Some(bitmap) = &self.fresh_ids else {
            return Ok(());
        };
        let bitmap = bitmap
            .lock()
            .map_err(|_| StoreError::Integrity("fresh admission filter"))?;
        ids.retain(|id| {
            Self::fresh_bit_positions(id).all(|bit| bitmap[bit / 64] & (1_u64 << (bit % 64)) != 0)
        });
        // The guard drops before the caller takes the Store connection.
        Ok(())
    }

    fn note_published_ids<'a>(&self, ids: impl Iterator<Item = &'a ObjectId>) -> Result<()> {
        let Some(bitmap) = &self.fresh_ids else {
            return Ok(());
        };
        let mut bitmap = bitmap
            .lock()
            .map_err(|_| StoreError::Integrity("fresh admission filter"))?;
        for id in ids {
            for bit in Self::fresh_bit_positions(id) {
                bitmap[bit / 64] |= 1_u64 << (bit % 64);
            }
        }
        // Publication calls this before commit/epoch release. A failed commit
        // can leave only false positives; rollback closes the entire session.
        Ok(())
    }

    pub(crate) fn retain(&self) {
        let _ = self.state.compare_exchange(
            0,
            1,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        );
    }

    pub(crate) fn resolve<T>(&self, result: Result<T>) -> Result<T> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                self.rollback().map_err(|cleanup| {
                    self.db.quarantine_writes();
                    StoreError::Io(std::io::Error::other(format!(
                        "{error}; admission cleanup failed: {cleanup}"
                    )))
                })?;
                Err(error)
            }
        }
    }

    fn rollback(&self) -> Result<()> {
        if self.state.load(std::sync::atomic::Ordering::Acquire) != 0 {
            return Ok(());
        }
        let mut connection = self.db.writer()?;
        // The connection can contain a bounded, not-yet-committed cohort even
        // though no Rust Transaction or mutex guard survived its physical batch.
        let mut cohort = self
            .cohort
            .lock()
            .map_err(|_| StoreError::Integrity("admission cohort"))?;
        if !cohort.open && !connection.is_autocommit() {
            return Err(StoreError::Integrity(
                "admission rollback transaction ownership",
            ));
        }
        if !connection.is_autocommit() {
            connection.execute_batch("ROLLBACK")?;
        }
        *cohort = AdmissionCohort::default();
        drop(cohort);
        let mut after = Vec::<u8>::new();
        loop {
            let ids = {
                let mut statement = connection.prepare(
                    "SELECT object_id FROM objects WHERE object_id > ?1 AND pack_id > ?2 ORDER BY object_id LIMIT 512",
                )?;
                let rows = statement
                    .query_map(rusqlite::params![after, self.baseline_pack], |row| {
                        row.get::<_, Vec<u8>>(0)
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                rows
            };
            let Some(last) = ids.last() else {
                break;
            };
            after.clone_from(last);
            let transaction =
                connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            for id in ids {
                transaction.execute("DELETE FROM objects WHERE object_id = ?1", [id])?;
            }
            transaction.commit()?;
        }
        loop {
            let transaction =
                connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let removed = transaction.execute(
                "DELETE FROM object_packs WHERE pack_id IN (SELECT pack_id FROM object_packs WHERE pack_id > ?1 ORDER BY pack_id LIMIT 512)",
                [self.baseline_pack],
            )?;
            transaction.commit()?;
            if removed == 0 {
                break;
            }
        }
        self.db.clear_metadata_index()?;
        self.state.store(2, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    fn ensure_active(&self) -> Result<()> {
        if self.state.load(std::sync::atomic::Ordering::Acquire) == 0 {
            Ok(())
        } else {
            Err(StoreError::Integrity("admission session closed"))
        }
    }
}

impl Drop for AdmissionSession {
    fn drop(&mut self) {
        // Abandoned tokens and unwinding still release their independently
        // committed batches. Fallible public paths call resolve explicitly.
        if let Err(error) = self.rollback() {
            self.db.quarantine_writes();
            eprintln!("LayerFS admission cleanup failed: {error}");
        }
        // The writer permit still belongs to this session. Move, never clone,
        // only retained hints; rollback discards inherited and private entries.
        if self.state.load(std::sync::atomic::Ordering::Acquire) == 1 {
            if let Some(candidates) = self
                .small_candidates
                .take()
                .and_then(|cache| cache.into_inner().ok())
            {
                self.db.return_small_candidates(candidates);
            }
        }
    }
}

pub(crate) struct CheckedOutputAdmission {
    session: std::sync::Arc<AdmissionSession>,
    db: crate::schema::StoreDb,
    incoming: Vec<AuthenticatedCanonicalObject>,
    incoming_index: HashMap<ObjectId, usize>,
    incoming_bytes: usize,
    batch: Vec<AuthenticatedCanonicalObject>,
    seen: SpillableObjectSet,
    compared: BTreeMap<ObjectId, (read::Location, Vec<u8>)>,
    compared_bytes: usize,
    compared_limit: usize,
    pending: HashMap<ObjectId, usize>,
    batch_bytes: usize,
    batch_epoch: u64,
    statement_number: u64,
    receipt: crate::CandidateReceipt,
    checked: CheckedAdmission,
    diagnostics: InitializationAdmissionDiagnostics,
    final_phase: bool,
}

/// Owned initially missing canonical output with closed external dependencies.
/// Only the accumulator issues absence proofs; stale proofs require a final recheck.
pub(crate) struct MissingBatch(
    Vec<AuthenticatedCanonicalObject>,
    std::sync::Arc<AdmissionSession>,
    bool,
    Option<u64>,
);

impl std::ops::Deref for MissingBatch {
    type Target = [AuthenticatedCanonicalObject];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub(crate) struct FinishedOutputAdmission {
    pub final_batch: MissingBatch,
    pub statement_number: u64,
    pub receipt: crate::CandidateReceipt,
    pub checked: CheckedAdmission,
    pub diagnostics: InitializationAdmissionDiagnostics,
}

impl DeferredObjectStore {
    pub fn new() -> Result<Self> {
        Self::with_reference_index(true)
    }

    #[cfg(test)]
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
            memory_limit: CANDIDATE_MEMORY_BYTES - 2 * 1024 * 1024,
            index_limit: CANDIDATE_INDEX_BYTES,
            spill_buffer_bytes: CANDIDATE_SPILL_BUFFER_BYTES - 2 * spill::ID_BUFFER_BYTES,
            order_memory_bytes: CANDIDATE_MEMORY_BYTES,
            private_owner: None,
            scratch: None,
            diagnostic_file_context: false,
            small_predecessor: None,
            predecessor: None,
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

    fn order_missing(&self, missing: &SpillableObjectSet, expected: usize) -> Result<IdOrder> {
        let mut output = IdOrder::empty();
        let mut count = 0_usize;
        let mut page = Vec::with_capacity(OBJECT_PAGE_COUNT);
        let mut consume = |page: &[ObjectId]| -> Result<()> {
            let known = missing.membership(page)?;
            for &id in page {
                if known.contains(&id) {
                    output.push_scoped(id, self.index_limit, &self.scratch)?;
                    count += 1;
                }
            }
            Ok(())
        };
        let mut push = |id| -> Result<()> {
            page.push(id);
            if page.len() == OBJECT_PAGE_COUNT {
                consume(&page)?;
                page.clear();
            }
            Ok(())
        };
        match &self.storage {
            DeferredObjects::Memory { order, .. } => {
                for &id in order {
                    push(id)?;
                }
            }
            DeferredObjects::Spill(spill) => spill.visit_ids(&mut push)?,
        }
        if !page.is_empty() {
            consume(&page)?;
        }
        if count != expected {
            return Err(StoreError::Integrity("candidate publication order"));
        }
        output.seal()?;
        Ok(output)
    }

    #[cfg(test)]
    fn visit_prevalidated_order(
        &self,
        order: &IdOrder,
        visitor: &mut dyn FnMut(ObjectId, &[u8]) -> Result<()>,
    ) -> Result<()> {
        match &self.storage {
            DeferredObjects::Memory { rows, .. } => order.visit(|id| {
                visitor(
                    id,
                    &rows.get(&id).ok_or(StoreError::MissingObject(id))?.bytes,
                )
            }),
            DeferredObjects::Spill(spill) => {
                spill.visit_ordered(order, &mut |id, bytes, _| visitor(id, bytes))
            }
        }
    }

    fn visit_authenticated_order(
        &self,
        order: &IdOrder,
        visitor: &mut dyn FnMut(&AuthenticatedCanonicalObject) -> Result<()>,
    ) -> Result<()> {
        match &self.storage {
            DeferredObjects::Memory { rows, .. } => {
                order.visit(|id| visitor(rows.get(&id).ok_or(StoreError::MissingObject(id))?))
            }
            DeferredObjects::Spill(spill) => spill.visit_ordered(order, &mut |id, bytes, hints| {
                let mut checked =
                    AuthenticatedCanonicalObject::new(std::mem::take(bytes), Some(id))?;
                checked.1 = hints;
                let result = visitor(&checked);
                *bytes = checked.0.bytes;
                result
            }),
        }
    }

    // Returns required selected-spill authentication time; memory owners retain proof.
    fn consume_prevalidated_pages(
        mut self,
        mut visitor: impl FnMut(Vec<AuthenticatedCanonicalObject>) -> Result<()>,
    ) -> Result<u64> {
        self.reachable.seal()?;
        // Preserve the checked graph's child-first order through spill delivery;
        // each bounded admission can carry closed dependency facts forward.
        let mut memory_owned_bytes = 0_u64;
        let mut spill_readback_bytes = 0_u64;
        let mut storage_authentication_ns = 0_u64;
        let capacity = usize::try_from(self.count)
            .unwrap_or(INITIALIZATION_ADMISSION_BATCH_COUNT)
            .min(INITIALIZATION_SLAB_OBJECTS)
            .min((self.memory_limit / std::mem::size_of::<AuthenticatedCanonicalObject>()).max(1));
        let page_limit = self.memory_limit.min(INITIALIZATION_SLAB_BYTES);
        let mut page = Vec::with_capacity(capacity);
        let mut page_bytes = 0_usize;
        let small_predecessor = self
            .small_predecessor
            .or_else(|| self.predecessor.as_ref().map(|(_, root, _, _)| root.0));
        let mut predecessor = self
            .predecessor
            .take()
            .map(|(reader, root, budget, available)| {
                (
                    reader,
                    layerfs_content::file::rope::PredecessorCursor::new(
                        layerfs_content::file::rope::FileStateRoot(root.0),
                    ),
                    budget,
                    available,
                )
            });
        let mut file_reserved = 0u64;
        let mut diagnostic_stop = 0u8;
        let mut diagnostic_stats = crate::PhysicalStorageReceipt {
            diag_cursor_attached: u64::from(predecessor.is_some()),
            ..Default::default()
        };
        let mut push = |mut object: AuthenticatedCanonicalObject| {
            object.1.has_predecessor |= predecessor.is_some();
            if object.is_small_content() {
                object.1.prior_ids[0] = small_predecessor.or(object.1.prior_ids[0]);
                object.1.has_predecessor |= small_predecessor.is_some();
                object.1.first_span = None;
            }
            if let (Some((reader, cursor, operation_reserved, available)), Some((start, len))) =
                (&mut predecessor, object.1.first_span)
            {
                // Each optional metadata object can require a target group and a FULL
                // anchor group. Reserve the larger encoded bound for both counters.
                const FETCH_RESERVATION: u64 = 131136;
                diagnostic_stats.diag_invalid +=
                    u64::from(object.1.diagnostic & 15 != 0 || object.1.diagnostic_grants != 0);
                let inherited = cursor.counters().1 != 0;
                let reserved_before = file_reserved;
                let mut denied = 0;
                diagnostic_stats.diag_cursor_queries += 1;
                diagnostic_stats.diag_cursor_query_bytes += object.bytes.len() as u64;
                diagnostic_stats.diag_cursor_inherited += u64::from(inherited);
                object.1.prior_ids = cursor.hints(&CoreReader(reader), start, len, || {
                    if !*available {
                        denied = diagnostic::MEMORY;
                        return false;
                    }
                    if file_reserved + FETCH_RESERVATION > 1024 * 1024 {
                        denied = diagnostic::FILE_LIMIT;
                        return false;
                    }
                    if operation_reserved
                        .fetch_update(
                            std::sync::atomic::Ordering::Relaxed,
                            std::sync::atomic::Ordering::Relaxed,
                            |used| {
                                used.checked_add(FETCH_RESERVATION).filter(|sum| {
                                    *sum <= CORRESPONDENCE_OPERATION_RESERVATION_BYTES
                                })
                            },
                        )
                        .is_err()
                    {
                        denied = diagnostic::OPERATION;
                        return false;
                    }
                    file_reserved += FETCH_RESERVATION;
                    true
                })?;
                let grants = (file_reserved - reserved_before) / FETCH_RESERVATION;
                diagnostic_stats.diag_cursor_grants += grants;
                object.1.diagnostic_grants = grants as u8;
                if !inherited && cursor.counters().1 != 0 {
                    diagnostic_stop = if denied != 0 {
                        denied
                    } else {
                        diagnostic::DESCRIPTOR
                    };
                    match diagnostic_stop {
                        diagnostic::MEMORY => diagnostic_stats.diag_cursor_memory_limit += 1,
                        diagnostic::FILE_LIMIT => diagnostic_stats.diag_cursor_file_limit += 1,
                        diagnostic::OPERATION => diagnostic_stats.diag_cursor_operation_limit += 1,
                        _ => diagnostic_stats.diag_cursor_descriptor_limit += 1,
                    }
                }
                object.1.diagnostic = (object.1.diagnostic & diagnostic::FILE)
                    | if cursor.counters().1 != 0 {
                        diagnostic_stop
                    } else {
                        diagnostic::COMPLETE
                    }
                    | if inherited { diagnostic::INHERITED } else { 0 };
                diagnostic_stats.diag_invalid += u64::from(!diagnostic::valid(
                    object.1.diagnostic,
                    object.1.diagnostic_grants,
                ));
            }

            if object.bytes.len() > ADMISSION_BATCH_BYTES {
                return Err(StoreError::Integrity("canonical object admission size"));
            }
            if !page.is_empty()
                && (page.len() == INITIALIZATION_ADMISSION_BATCH_COUNT
                    || page_bytes.saturating_add(object.bytes.len()) > page_limit)
            {
                visitor(std::mem::replace(&mut page, Vec::with_capacity(capacity)))?;
                page_bytes = 0;
            }
            page_bytes = page_bytes.saturating_add(object.bytes.len());
            page.push(object);
            Ok(())
        };
        let delivery_result: Result<()> = match &mut self.storage {
            DeferredObjects::Memory { rows, .. } => self.reachable.visit(|id| {
                let object = rows.remove(&id).ok_or(StoreError::MissingObject(id))?;
                memory_owned_bytes = memory_owned_bytes.saturating_add(object.bytes.len() as u64);
                push(object)
            }),
            DeferredObjects::Spill(spill) => {
                spill.visit_ordered(&self.reachable, &mut |id, bytes, hints| {
                    spill_readback_bytes = spill_readback_bytes.saturating_add(bytes.len() as u64);
                    let started = Instant::now();
                    let mut object =
                        AuthenticatedCanonicalObject::new(std::mem::take(bytes), Some(id))?;
                    object.1 = hints;
                    storage_authentication_ns =
                        storage_authentication_ns.saturating_add(elapsed_ns(started));
                    push(object)
                })
            }
        };
        if let Err(error) = delivery_result {
            if let Some((reader, _, _, _)) = &predecessor {
                diagnostic_stats.diag_invalid += 1;
                // Include a successful reservation whose metadata read failed
                // before its occurrence could reach admission.
                diagnostic_stats.diag_cursor_grants = file_reserved / 131136;
                reader.note_delivery_diagnostic(diagnostic_stats);
            }
            return Err(error);
        }
        if let Some((reader, cursor, _, available)) = predecessor {
            let (descriptors, skips) = cursor.counters();
            reader.note_predecessor_correspondence(file_reserved, descriptors, skips, !available);
            reader.note_delivery_diagnostic(diagnostic_stats);
        }
        if !page.is_empty() {
            visitor(page)?;
        }
        crate::telemetry::note_workspace_candidate_delivery(
            memory_owned_bytes,
            spill_readback_bytes,
            storage_authentication_ns,
        );
        Ok(storage_authentication_ns)
    }

    pub fn visit_batches(
        &self,
        visitor: &mut dyn FnMut(&[CanonicalObject], bool) -> Result<()>,
    ) -> Result<()> {
        let mut batch = Vec::with_capacity(OBJECT_PAGE_COUNT);
        let mut bytes = 0_usize;
        let mut push = |object: CanonicalObject| -> Result<()> {
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
        };
        match &self.storage {
            DeferredObjects::Memory { rows, .. } => self.reachable.visit(|id| {
                push(
                    rows.get(&id)
                        .ok_or(StoreError::MissingObject(id))?
                        .as_ref()
                        .clone(),
                )
            })?,
            DeferredObjects::Spill(spill) => {
                spill.visit_ordered(&self.reachable, &mut |id, bytes, _| {
                    push(CanonicalObject {
                        id,
                        bytes: std::mem::take(bytes),
                    })
                })?
            }
        }
        if !batch.is_empty() {
            visitor(&batch, true)?;
        }
        Ok(())
    }

    fn reachable_from(mut self, root: ObjectId) -> Result<Self> {
        if let DeferredObjects::Spill(spill) = &mut self.storage {
            spill.flush()?;
        }
        let mut seen = SpillableObjectSet::bounded_scoped(self.index_limit, self.scratch.clone())?;
        seen.insert_page(&[root])?;
        let mut active = BTreeSet::new();
        let mut stack = vec![(root, false)];
        let mut order = self.predecessor.is_none().then(IdOrder::empty);
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
                if let Some(order) = &mut order {
                    order.push_scoped(id, self.index_limit, &self.scratch)?;
                }
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
        // File constructors emit children before parents. Preserve first payload
        // spans through their existing selection; generic callers retain DFS order.
        self.reachable = if let Some(mut order) = order {
            order.seal()?;
            order
        } else {
            self.order_missing(&seen, count as usize)?
        };
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
        if self.reference_bytes.saturating_add(charge) > self.index_limit {
            self.references = None;
            self.reference_bytes = 0;
            return;
        }
        references.insert(id, children.to_vec());
        self.reference_bytes += charge;
    }

    fn get(&self, id: ObjectId) -> Result<Option<Vec<u8>>> {
        match &self.storage {
            DeferredObjects::Memory { rows, .. } => {
                Ok(rows.get(&id).map(|object| object.bytes.clone()))
            }
            DeferredObjects::Spill(spill) => spill.get(id),
        }
    }

    fn encoded_length(&self, id: ObjectId) -> Result<u64> {
        match &self.storage {
            DeferredObjects::Memory { rows, .. } => rows
                .get(&id)
                .map(|object| object.bytes.len() as u64)
                .ok_or(StoreError::MissingObject(id)),
            DeferredObjects::Spill(spill) => spill.encoded_length(id),
        }
    }

    #[cfg(test)]
    fn put(&mut self, id: ObjectId, canonical: &[u8]) -> Result<()> {
        self.put_authenticated(AuthenticatedCanonicalObject::new(
            canonical.to_vec(),
            Some(id),
        )?)
    }

    fn put_authenticated(&mut self, object: AuthenticatedCanonicalObject) -> Result<()> {
        let id = object.id;
        if let Some(known) = self.get(id)? {
            return if known == object.bytes {
                Ok(())
            } else {
                Err(StoreError::Integrity("candidate object collision"))
            };
        }
        let length = object.bytes.len();
        let children = if self.references.is_some() {
            let mut children = referenced_objects(&object.bytes)?;
            children.sort();
            children.dedup();
            Some(children)
        } else {
            None
        };
        // The checked owner retains its identity alongside the existing index key.
        let charge = object
            .bytes
            .capacity()
            .saturating_add(64 + std::mem::size_of::<AuthenticatedCanonicalObject>());
        if matches!(&self.storage, DeferredObjects::Memory { bytes, .. } if bytes.saturating_add(charge) > self.memory_limit)
        {
            self.spill()?;
        }
        match &mut self.storage {
            DeferredObjects::Memory { order, rows, bytes } => {
                order.push(id);
                rows.insert(id, object);
                *bytes += charge;
            }
            DeferredObjects::Spill(spill) => spill.put(&object)?,
        }
        self.reachable
            .push_scoped(id, self.index_limit, &self.scratch)?;
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
        let (file, path) = temporary_output_file("candidate-objects", &self.scratch)?;
        let reader = if self.scratch.is_some() {
            let reader = file.try_clone()?;
            std::fs::remove_file(&path)?;
            reader
        } else {
            SpillFile::Native(std::fs::File::open(&path)?)
        };
        let mut spill = SpillObjects {
            writer: Some(file),
            reader: Mutex::new(reader),
            path,
            pending: Vec::with_capacity(self.spill_buffer_bytes),
            pending_index: BTreeMap::new(),
            end: 0,
            index: Some(BTreeMap::new()),
            index_bytes: 0,
            disk_index: None,
            index_limit: self.index_limit,
            buffer_bytes: self.spill_buffer_bytes,
            order_memory_bytes: self.order_memory_bytes,
            failed: false,
            scratch: self.scratch.clone(),
        };
        for id in order {
            spill.put(
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
        self.reachable.seal()?;
        if let DeferredObjects::Spill(spill) = &mut self.storage {
            spill.seal()?;
        }
        Ok(self)
    }
}

pub(crate) struct CompletedContent {
    pub root: layerfs_content::file::content::FileContentRoot,
    pub counters: layerfs_content::file::rope::RopeCounters,
}

/// The same full-file completion check serves direct native output and private
/// Workspace candidates; persistence/finality policy remains with the owner.
pub(crate) fn build_checked_file(
    objects: &mut impl ObjectStore,
    source: impl Read,
    expected_len: u64,
) -> Result<CompletedContent> {
    let previous = objects.set_file_payload_context(true);
    let result = build_checked_file_inner(objects, source, expected_len);
    objects.set_file_payload_context(previous);
    result
}

fn build_checked_file_inner(
    objects: &mut impl ObjectStore,
    mut source: impl Read,
    expected_len: u64,
) -> Result<CompletedContent> {
    if objects.small_content_format()
        && expected_len > 0
        && expected_len < layerfs_content::file::content::SMALL_LIMIT as u64
    {
        let mut bytes = vec![0; expected_len as usize];
        source.read_exact(&mut bytes)?;
        if source.read(&mut [0; 1])? != 0 {
            return Err(StoreError::Integrity("completed file length"));
        }
        let (root, counters) = layerfs_content::file::content::build_bytes(objects, &bytes)?;
        return Ok(CompletedContent { root, counters });
    }
    if expected_len < layerfs_content::file::cdc::MINIMUM_CHUNK_BYTES as u64 {
        // One extra byte detects growth; no canonical output precedes the EOF check.
        let mut bytes = vec![0; expected_len as usize + 1];
        let mut length = 0;
        loop {
            let read = match source.read(&mut bytes[length..]) {
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            };
            if read == 0 {
                break;
            }
            length += read;
            if length > expected_len as usize {
                return Err(StoreError::Integrity("completed file length"));
            }
        }
        if length != expected_len as usize {
            return Err(StoreError::Integrity("completed file length"));
        }
        let (root, counters) = layerfs_content::file::rope::build_bytes(objects, &bytes[..length])?;
        return Ok(CompletedContent {
            root: layerfs_content::file::content::FileContentRoot(root.0),
            counters,
        });
    }
    let completed = layerfs_content::file::rope::build_complete(objects, source)?;
    if completed.logical_len != expected_len {
        return Err(StoreError::Integrity("completed file length"));
    }
    Ok(CompletedContent {
        root: layerfs_content::file::content::FileContentRoot(completed.root.0),
        counters: completed.counters,
    })
}

pub struct ObjectBuffer<'a> {
    source: Option<&'a dyn ObjectSource>,
    objects: DeferredObjectStore,
}

impl<'a> ObjectBuffer<'a> {
    /// Diagnostic source marker: actual regular-file owners only. Generic rope
    /// construction also serves mode/mtime metadata and must leave this unset.
    #[doc(hidden)]
    pub fn diagnostic_file_payloads(&mut self) {
        self.objects.diagnostic_file_context = true;
    }

    #[doc(hidden)]
    pub fn set_physical_predecessor(
        &mut self,
        reader: crate::SnapshotReader,
        root: impl Into<layerfs_content::file::content::FileContentRoot>,
        operation_reserved: std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Result<()> {
        let root = root.into();
        self.objects.small_predecessor = Some(root.0);
        if matches!(
            layerfs_content::file::content::inspect(&CoreReader(&reader), root)?,
            layerfs_content::file::content::Content::Small { .. }
        ) {
            return Ok(());
        }
        // Producer correspondence may overlap admission and other producers.
        // Reserve its cursor (64 KiB) plus one bounded physical metadata read
        // (512 KiB) from this producer's existing partition, never the encoder's
        // 2-MiB scratch. Small partitions preserve context but skip optional work.
        const CORRESPONDENCE_MEMORY: usize = 576 * 1024;
        let available = self.objects.memory_limit >= CORRESPONDENCE_MEMORY + 32 * 1024;
        if available {
            self.objects.memory_limit -= CORRESPONDENCE_MEMORY;
            // Spilled output retains its pending Vec capacity while ordered
            // delivery allocates read-ahead. Shrink both owners prospectively;
            // changing only the canonical spill threshold does not release them.
            self.objects.spill_buffer_bytes = self
                .objects
                .spill_buffer_bytes
                .saturating_sub(CORRESPONDENCE_MEMORY)
                .max(spill::ID_BUFFER_BYTES);
            if let DeferredObjects::Spill(spill) = &mut self.objects.storage {
                spill.flush()?;
                spill.pending = Vec::with_capacity(self.objects.spill_buffer_bytes);
                spill.buffer_bytes = self.objects.spill_buffer_bytes;
            }
            if matches!(&self.objects.storage, DeferredObjects::Memory { bytes, .. } if *bytes > self.objects.memory_limit)
            {
                self.objects.spill()?;
            }
        }
        self.objects.predecessor = Some((
            reader,
            layerfs_content::file::rope::FileStateRoot(root.0),
            operation_reserved,
            available,
        ));
        Ok(())
    }

    #[doc(hidden)]
    pub fn build_complete_with_predecessor(
        mut self,
        source: impl Read,
        expected_len: u64,
    ) -> Result<BuiltRoot> {
        self.diagnostic_file_payloads();
        self.objects.references = None;
        let completed = build_checked_file(&mut self, source, expected_len)?;
        self.finish_all_reachable(completed.root.0, completed.counters.cdc_bytes_scanned)
    }

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

    #[cfg(test)]
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
    pub fn into_resumable(mut self) -> Result<DeferredObjectStore> {
        // The canonical spool remains resumable; the ID reader sees only a sealed order.
        self.objects.reachable.seal()?;
        Ok(self.objects)
    }

    #[doc(hidden)]
    pub fn into_prevalidated(self) -> Result<DeferredObjectStore> {
        self.objects.all_reachable()
    }

    /// Scratch for a single completed-file producer; queue and admission own the
    /// other bounded portions of the existing canonical-output allowance.
    pub fn bounded_output(source: Option<&'a dyn ObjectSource>) -> Result<Self> {
        let mut output = match source {
            Some(source) => Self::new(source)?,
            None => Self::empty()?,
        };
        output.objects.memory_limit = CANDIDATE_SPILL_BUFFER_BYTES;
        Ok(output)
    }

    /// Configure charged private scratch before any canonical output exists.
    /// Released callers retain their existing default backend and allowances.
    #[cfg(unix)]
    pub fn set_private_scratch(
        &mut self,
        scratch: std::sync::Arc<scratch::ScratchBudget>,
    ) -> Result<()> {
        if self.objects.count != 0 || self.objects.scratch.is_some() {
            return Err(StoreError::InvalidInput(
                "private scratch context already active",
            ));
        }
        self.objects.scratch = Some(scratch);
        Ok(())
    }

    /// Lower private index allowances before construction, keeping the existing
    /// disk spill implementation and its fixed 4-MiB SQLite cache. This changes
    /// neither canonical encoding nor the released defaults for other callers.
    pub fn limit_private_indexes(&mut self, index_bytes: usize, order_bytes: usize) -> Result<()> {
        if self.objects.count != 0
            || index_bytes > self.objects.index_limit
            || order_bytes > self.objects.order_memory_bytes
            || !(4 * 1024 * 1024..=CANDIDATE_INDEX_BYTES).contains(&index_bytes)
            || !(spill::ID_BUFFER_BYTES..=CANDIDATE_MEMORY_BYTES).contains(&order_bytes)
        {
            return Err(StoreError::InvalidInput(
                "private candidate index allowance",
            ));
        }
        self.objects.index_limit = index_bytes;
        self.objects.order_memory_bytes = order_bytes;
        Ok(())
    }

    /// Retain the caller's aggregate admission with the actual private output.
    /// A finished BuiltRoot keeps this owner until consumed or discarded.
    pub fn retain_private_owner(&mut self, owner: std::sync::Arc<dyn Send + Sync>) -> Result<()> {
        if self.objects.private_owner.is_some() {
            return Err(StoreError::InvalidInput(
                "private candidate owner already set",
            ));
        }
        self.objects.private_owner = Some(owner);
        Ok(())
    }

    /// Partition the existing aggregate candidate allowances, including spill
    /// buffers and indexes. The SQLite fallback's fixed cache must still fit.
    pub fn partition_output(&mut self, partitions: usize) -> Result<()> {
        if partitions == 0 || partitions > 8 {
            return Err(StoreError::InvalidInput("candidate partitions"));
        }
        if partitions == 1 {
            return Ok(());
        }
        self.objects.memory_limit = CANDIDATE_SPILL_BUFFER_BYTES / partitions;
        self.objects.index_limit = CANDIDATE_INDEX_BYTES / partitions;
        self.objects.spill_buffer_bytes = (CANDIDATE_SPILL_BUFFER_BYTES - spill::ID_BUFFER_BYTES)
            / partitions
            - spill::ID_BUFFER_BYTES;
        self.objects.order_memory_bytes = CANDIDATE_MEMORY_BYTES / partitions;
        if matches!(&self.objects.storage, DeferredObjects::Memory { bytes, .. } if *bytes > self.objects.memory_limit)
        {
            self.objects.spill()?;
        }
        if let DeferredObjects::Spill(spill) = &mut self.objects.storage {
            spill.flush()?;
            spill.pending = Vec::with_capacity(self.objects.spill_buffer_bytes);
            spill.buffer_bytes = self.objects.spill_buffer_bytes;
            spill.index_limit = self.objects.index_limit;
            spill.order_memory_bytes = self.objects.order_memory_bytes;
            if spill.index_bytes > spill.index_limit {
                spill.spill_index()?;
            }
        }
        if matches!(&self.objects.reachable, IdOrder::Memory(ids) if ids.len().saturating_mul(32) > self.objects.index_limit)
        {
            let mut order = IdOrder::empty();
            self.objects.reachable.seal()?;
            self.objects.reachable.visit(|id| {
                order.push_scoped(id, self.objects.index_limit, &self.objects.scratch)
            })?;
            order.seal()?;
            self.objects.reachable = order;
        }
        if self.objects.reference_bytes > self.objects.index_limit {
            self.objects.references = None;
            self.objects.reference_bytes = 0;
        }
        Ok(())
    }

    /// Full-file rope construction emits sealed prefixes, attaches every prefix
    /// to its final mapping root, and then emits FileState. This is the same
    /// append-only finality used by native import; incremental edits do not use it.
    /// Keep the entire file private until successful construction and length check.
    pub fn build_complete_file(source: impl Read, expected_len: u64) -> Result<BuiltRoot> {
        Self::build_complete_file_partition(source, expected_len, 1)
    }

    pub fn build_complete_file_partition(
        source: impl Read,
        expected_len: u64,
        partitions: usize,
    ) -> Result<BuiltRoot> {
        let mut objects = Self::bounded_output(None)?;
        objects.diagnostic_file_payloads();
        objects.partition_output(partitions)?;
        objects.objects.references = None;
        let completed = build_checked_file(&mut objects, source, expected_len)?;
        objects.finish_all_reachable(completed.root.0, completed.counters.cdc_bytes_scanned)
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

    /// Preview/reconciliation uses the same task driver, with a private sink.
    pub fn construct_files<I, S: Send, T: Send>(
        &mut self,
        worker_limit: usize,
        task_count: usize,
        tasks: I,
        initialize: impl Fn(usize) -> Result<S> + Sync,
        step: impl Fn(&mut S, usize, I::Item, &mut FinalizedOutputWriter) -> Result<()> + Sync,
        finish: impl Fn(S) -> Result<T> + Sync,
    ) -> Result<Vec<T>>
    where
        I: Iterator + Send,
        I::Item: Send,
    {
        let worker_limit = if self.source.is_some_and(ObjectSource::small_content_format) {
            worker_limit.min(SMALL_CONTENT_WORKERS)
        } else {
            worker_limit
        };
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let (output, _) = run_finalized_output(
            worker_limit,
            task_count,
            tasks,
            &cancelled,
            initialize,
            |state, ordinal, task, writer| {
                writer.set_small_content_format(
                    self.source.is_some_and(ObjectSource::small_content_format),
                );
                step(state, ordinal, task, writer)
            },
            finish,
            |page| {
                for object in page {
                    self.objects.put_authenticated(object)?;
                }
                Ok(())
            },
        )?;
        Ok(output.into_iter().map(|(result, _)| result).collect())
    }

    pub fn merge_prevalidated(&mut self, objects: DeferredObjectStore) -> Result<()> {
        objects
            .consume_prevalidated_pages(|page| {
                for object in page {
                    self.objects.put_authenticated(object)?;
                }
                Ok(())
            })
            .map(|_| ())
    }
}

impl ObjectStore for ObjectBuffer<'_> {
    fn small_content_format(&self) -> bool {
        self.source.is_some_and(ObjectSource::small_content_format)
    }
    fn compact_namespace(&self) -> bool {
        self.source.is_some_and(ObjectSource::compact_namespace)
    }
    fn allocate_inode_serial(
        &mut self,
        scope: ObjectId,
    ) -> CoreResult<layerfs_content::tree::compact::InodeSerial> {
        self.source
            .ok_or(CoreError::Unsupported)?
            .allocate_inode_serial(scope)
            .map_err(core_read_error)
    }

    fn set_file_payload_context(&mut self, enabled: bool) -> bool {
        std::mem::replace(&mut self.objects.diagnostic_file_context, enabled)
    }

    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        if let Some(bytes) = self.objects.get(id).map_err(|_| CoreError::Io)? {
            return Ok(bytes);
        }
        self.source
            .ok_or(CoreError::MissingObject)?
            .read_object(id)
            .map_err(core_read_error)
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> CoreResult<T>
    where
        F: FnOnce(&[u8]) -> CoreResult<T>,
    {
        match &self.objects.storage {
            DeferredObjects::Memory { rows, .. } => {
                if let Some(object) = rows.get(&id) {
                    return callback(&object.bytes);
                }
            }
            DeferredObjects::Spill(_) => {
                if let Some(bytes) = self.objects.get(id).map_err(core_read_error)? {
                    layerfs_content::authenticate_identity(&bytes, id)?;
                    return callback(&bytes);
                }
            }
        }
        CoreReader(self.source.ok_or(CoreError::MissingObject)?)
            .with_authenticated_canonical(id, callback)
    }

    /// Bounded authenticated canonical batch read. Objects this buffer already
    /// owns are served exactly as the point route serves them; every other demand
    /// goes to the source through one bounded packed batch read, which resolves
    /// all locations once and selects and decompresses each physical group once
    /// for all of its demands. Identity and canonical-format checks are the same
    /// checks the point route performs, on every demanded object.
    ///
    /// The batch-count ceiling is enforced here, on the whole request, before the
    /// source is asked and before any callback runs: a batch this buffer wholly
    /// owns never reaches the source and must not bypass the reader's page bound.
    /// Callbacks run in the declared demand order, as `ObjectRead` promises, so an
    /// owned object never overtakes an earlier unowned demand.
    fn get_authenticated_canonical_batch<F>(
        &self,
        ids: &[ObjectId],
        mut callback: F,
    ) -> CoreResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
    {
        if ids.is_empty() {
            return Ok(());
        }
        if ids.len() > OBJECT_PAGE_COUNT {
            return Err(CoreError::InvalidRecord("object read page"));
        }
        // One pass records which demands this buffer owns and which must be
        // fetched. Only the positions of owned demands are retained, so the
        // retained state is bounded by this one request.
        let mut owned: Vec<(usize, ObjectId)> = Vec::new();
        let mut missing = Vec::new();
        for (position, id) in ids.iter().enumerate() {
            if owns_batch_demand(&self.objects, *id)? {
                owned.push((position, *id));
            } else {
                missing.push(*id);
            }
        }
        if owned.is_empty() {
            // Every demand is unowned: stream the single bounded source call
            // straight to the callback, exactly as the promoted route did, so the
            // common case retains no per-demand copy.
            return match self.source {
                Some(source) => {
                    CoreReader(source).get_authenticated_canonical_batch(&missing, callback)
                }
                None => Err(CoreError::MissingObject),
            };
        }
        // Interleaved owned and unowned demands: the source call must still run
        // exactly once, and its results are held only until each demand has been
        // answered in order. One copy per unowned demand is the transient price
        // of preserving the documented callback order.
        let source = self.source.ok_or(CoreError::MissingObject)?;
        let mut fetched = BTreeMap::new();
        CoreReader(source).get_authenticated_canonical_batch(&missing, |id, canonical| {
            fetched.entry(id).or_insert_with(|| canonical.to_vec());
            Ok(())
        })?;
        for id in &missing {
            if !fetched.contains_key(id) {
                return Err(CoreError::MissingObject);
            }
        }
        let mut next_owned = 0;
        for (position, id) in ids.iter().enumerate() {
            if owned
                .get(next_owned)
                .is_some_and(|(index, _)| *index == position)
            {
                next_owned += 1;
                emit_owned_batch_demand(&self.objects, *id, &mut callback)?;
                continue;
            }
            let bytes = fetched.get(id).ok_or(CoreError::MissingObject)?;
            layerfs_content::authenticate_identity(bytes, *id)?;
            callback(*id, bytes)?;
        }
        Ok(())
    }

    fn put_file_payload(
        &mut self,
        canonical: Vec<u8>,
        start: u64,
        len: u32,
    ) -> CoreResult<ObjectId> {
        let mut object = AuthenticatedCanonicalObject::new(canonical, None)?;
        object.1.first_span = Some((start, len));
        if self.objects.diagnostic_file_context {
            object.1.diagnostic = diagnostic::FILE;
        }
        let id = object.id;
        self.objects
            .put_authenticated(object)
            .map_err(|_| CoreError::Io)?;
        Ok(id)
    }

    fn put_tree_origin(
        &mut self,
        canonical: Vec<u8>,
        origin: Option<ObjectId>,
    ) -> CoreResult<ObjectId> {
        let mut object = AuthenticatedCanonicalObject::new(canonical, None)?;
        if is_inode_table_leaf(&object.bytes)? {
            object.1.prior_ids[0] = origin;
            object.1.has_predecessor = origin.is_some();
        }
        let id = object.id;
        self.objects
            .put_authenticated(object)
            .map_err(|_| CoreError::Io)?;
        Ok(id)
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        self.put_owned(canonical.to_vec())
    }

    fn put_owned(&mut self, canonical: Vec<u8>) -> CoreResult<ObjectId> {
        let object = AuthenticatedCanonicalObject::new(canonical, None)?;
        let id = object.id;
        self.objects
            .put_authenticated(object)
            .map_err(|_| CoreError::Io)?;
        Ok(id)
    }
}

impl ObjectSource for ObjectBuffer<'_> {
    fn compact_namespace(&self) -> bool {
        ObjectStore::compact_namespace(self)
    }
    fn allocate_inode_serial(
        &self,
        scope: ObjectId,
    ) -> Result<layerfs_content::tree::compact::InodeSerial> {
        self.source
            .ok_or(StoreError::InvalidInput("inode allocator unavailable"))?
            .allocate_inode_serial(scope)
    }
    fn small_content_format(&self) -> bool {
        ObjectStore::small_content_format(self)
    }

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
        candidate.visit_authenticated_order(&candidate.reachable, &mut |object| {
            combined.put_authenticated(object.clone())
        })?;
    }
    combined.reachable_from(root_id)
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

impl crate::schema::StoreDb {
    pub(crate) fn check_canonical_format(&self, canonical: &[u8]) -> Result<()> {
        if layerfs_content::decode_bytes_object(canonical)
            .is_ok_and(|value| value.starts_with(layerfs_content::file::content::WHOLE_MAGIC))
        {
            if !self.compact_namespace() {
                return Err(StoreError::Integrity("whole-file owner requires schema 10"));
            }
            layerfs_content::file::content::whole_bytes(canonical)?;
        }
        if !self.compact_namespace()
            && layerfs_content::decode_bytes_object(canonical).is_ok_and(|value| {
                matches!(
                    value.get(..8),
                    Some(b"LFS6FSR\0" | b"LFS6INT\0" | b"LFS6NSP\0")
                )
            })
        {
            return Err(StoreError::Integrity(
                "compact namespace requires schema 10",
            ));
        }
        Ok(())
    }
    pub fn read_object_row(&self, id: ObjectId) -> Result<Vec<u8>> {
        let location = self
            .object_locations(&[id])?
            .remove(&id)
            .ok_or(StoreError::Integrity("visible object missing"))?;
        let mut bytes = None;
        self.visit_locations(&mut [(id, location)], |object| {
            bytes = Some(object.bytes);
            Ok(())
        })?;
        bytes.ok_or(StoreError::Integrity("visible object missing"))
    }

    pub fn read_object_rows(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        if let [id] = ids {
            return Ok(vec![CanonicalObject {
                id: *id,
                bytes: self.read_object_row(*id)?,
            }]);
        }
        if ids.len() > OBJECT_PAGE_COUNT {
            return Err(StoreError::InvalidInput("object read page"));
        }
        let mut remaining = BTreeMap::<ObjectId, usize>::new();
        for id in ids {
            *remaining.entry(*id).or_default() += 1;
        }
        let distinct = remaining.keys().copied().collect::<Vec<_>>();
        let locations = self.object_locations(&distinct)?;
        if locations.len() != distinct.len() {
            return Err(StoreError::Integrity("visible object cardinality"));
        }
        let mut locations = locations.into_iter().collect::<Vec<_>>();
        let mut rows = BTreeMap::new();
        self.visit_locations(&mut locations, |object| {
            rows.insert(object.id, object.bytes);
            Ok(())
        })?;
        let mut output = Vec::with_capacity(ids.len());
        for id in ids {
            let count = remaining
                .get_mut(id)
                .ok_or(StoreError::Integrity("visible object order"))?;
            *count -= 1;
            let bytes = if *count == 0 {
                rows.remove(id)
                    .ok_or(StoreError::Integrity("visible object missing"))?
            } else {
                let bytes = rows
                    .get(id)
                    .ok_or(StoreError::Integrity("visible object missing"))?
                    .clone();
                note_read_batch_clone(bytes.len());
                bytes
            };
            output.push(CanonicalObject { id: *id, bytes });
        }
        Ok(output)
    }

    #[cfg(test)]
    pub fn object_membership(&self, ids: &[ObjectId]) -> Result<BTreeMap<ObjectId, u64>> {
        if ids.len() > OBJECT_PAGE_COUNT {
            return Err(StoreError::InvalidInput("object membership page"));
        }
        Ok(self
            .object_locations(ids)?
            .into_iter()
            .map(|(id, location)| (id, location.canonical_length as u64))
            .collect())
    }
}

impl CheckedOutputAdmission {
    pub(crate) fn new(db: &crate::schema::StoreDb) -> Result<Self> {
        Self::with_session(db, AdmissionSession::new(db)?, 0)
    }

    pub(crate) fn new_for_initialization(db: &crate::schema::StoreDb) -> Result<Self> {
        Self::with_session(
            db,
            AdmissionSession::with_coalescing(db, true)?,
            COMPARISON_REUSE_BYTES,
        )
    }

    fn with_session(
        db: &crate::schema::StoreDb,
        session: std::sync::Arc<AdmissionSession>,
        compared_limit: usize,
    ) -> Result<Self> {
        // Repartition the existing 16MiB allowance for owner-local caches.
        let seen_limit = CANDIDATE_INDEX_BYTES / 4
            - compared_limit
            - if session.small_candidates.is_some() {
                small_candidates::INDEX_BYTES
            } else {
                0
            }
            - if session.fresh_ids.is_some() {
                FRESH_ADMISSION_FILTER_BYTES
            } else {
                0
            };
        Ok(Self {
            session,
            db: db.clone(),
            incoming: Vec::with_capacity(INITIALIZATION_SLAB_OBJECTS),
            incoming_index: HashMap::new(),
            incoming_bytes: 0,
            batch: Vec::with_capacity(INITIALIZATION_SLAB_OBJECTS),
            // Only preexisting occurrences need a separate uniqueness index.
            // This session's published packs already identify its fresh output.
            seen: SpillableObjectSet::bounded(seen_limit)?,
            compared: BTreeMap::new(),
            compared_bytes: 0,
            compared_limit,
            pending: HashMap::new(),
            batch_bytes: 0,
            batch_epoch: 0,
            statement_number: 0,
            receipt: crate::CandidateReceipt::default(),
            checked: CheckedAdmission::default(),
            diagnostics: InitializationAdmissionDiagnostics::default(),
            final_phase: false,
        })
    }

    #[cfg(test)]
    pub(crate) fn admit_worker_segment(&mut self, objects: DeferredObjectStore) -> Result<()> {
        self.admit(objects)
    }

    pub(crate) fn session(&self) -> std::sync::Arc<AdmissionSession> {
        self.session.clone()
    }

    pub(crate) fn resolve<T>(&self, result: Result<T>) -> Result<T> {
        self.session.resolve(result)
    }

    pub(crate) fn abort(self) -> Result<()> {
        self.session.rollback()
    }

    pub(crate) fn finish(self) -> Result<FinishedOutputAdmission> {
        let session = self.session.clone();
        session.resolve(self.finish_inner())
    }

    fn finish_inner(mut self) -> Result<FinishedOutputAdmission> {
        self.probe_incoming()?;
        let mut final_batch = self.take_batch()?;
        final_batch.2 = true;
        Ok(FinishedOutputAdmission {
            final_batch,
            statement_number: self.statement_number,
            receipt: self.receipt,
            checked: self.checked,
            diagnostics: self.diagnostics,
        })
    }

    pub(crate) fn prepare_final_phase(&mut self) -> Result<()> {
        self.final_phase = true;
        Ok(())
    }

    fn observe_final_owned_bytes(&mut self, transient: u64, incoming: u64) {
        if !self.final_phase {
            return;
        }
        let owned = transient
            .saturating_add(incoming)
            .saturating_add(self.compared_bytes as u64)
            .saturating_add(self.incoming_bytes as u64)
            .saturating_add(
                (self.incoming.capacity() * std::mem::size_of::<CanonicalObject>()) as u64,
            )
            .saturating_add((self.incoming_index.capacity() * 64) as u64)
            .saturating_add(self.batch_bytes as u64)
            .saturating_add((self.batch.capacity() * std::mem::size_of::<CanonicalObject>()) as u64)
            .saturating_add((self.pending.capacity() * 64) as u64);
        self.diagnostics.final_simultaneous_owned_peak_bytes = self
            .diagnostics
            .final_simultaneous_owned_peak_bytes
            .max(owned);
    }

    pub(crate) fn admit(&mut self, objects: DeferredObjectStore) -> Result<()> {
        let session = self.session.clone();
        let result = objects
            .consume_prevalidated_pages(|page| self.admit_page(page))
            .map(|_| ());
        session.resolve(result)
    }

    pub(crate) fn admit_page(&mut self, page: Vec<AuthenticatedCanonicalObject>) -> Result<()> {
        let session = self.session.clone();
        session.resolve(
            page.into_iter()
                .try_for_each(|object| self.admit_object(object)),
        )
    }

    fn take_batch(&mut self) -> Result<MissingBatch> {
        self.session.ensure_active()?;
        self.close_dependencies()?;
        let batch = std::mem::take(&mut self.batch);
        self.pending.clear();
        self.batch_bytes = 0;
        Ok(MissingBatch(
            batch,
            self.session.clone(),
            false,
            Some(self.batch_epoch),
        ))
    }

    fn close_dependencies(&mut self) -> Result<()> {
        let mut dependencies = BTreeSet::new();
        for object in &self.batch {
            for id in referenced_objects(&object.bytes)? {
                if !self.pending.contains_key(&id) {
                    dependencies.insert(id);
                }
            }
        }
        let dependencies = dependencies.into_iter().collect::<Vec<_>>();
        for page in dependencies.chunks(OBJECT_PAGE_COUNT) {
            if !self.db.objects_exist(page)? {
                return Err(StoreError::Integrity("new object dependency missing"));
            }
        }
        Ok(())
    }

    fn probe_incoming(&mut self) -> Result<()> {
        if self.incoming.is_empty() {
            return Ok(());
        }
        let page = std::mem::take(&mut self.incoming);
        self.incoming_index.clear();
        self.incoming_bytes = 0;
        let mut ids = page.iter().map(|object| object.id).collect::<Vec<_>>();
        let mut probe_epoch = self
            .session
            .publication_epoch
            .load(std::sync::atomic::Ordering::Acquire);
        let before_filter = ids.len();
        self.session.retain_possible_ids(&mut ids)?;
        self.diagnostics.fresh_probe_ids_skipped += (before_filter - ids.len()) as u64;
        self.diagnostics.probe_ids_queried += ids.len() as u64;
        let known = self.db.object_locations(&ids)?;
        drop(ids);
        let preexisting = known
            .iter()
            .filter_map(|(&id, location)| {
                (location.pack <= self.session.baseline_pack).then_some(id)
            })
            .collect::<Vec<_>>();
        let first = self
            .seen
            .insert_page(&preexisting)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        drop(preexisting);
        let mut supplied = page
            .iter()
            .map(|object| (object.id, object.bytes.as_slice()))
            .collect::<Vec<_>>();
        let mut metrics = ObjectInsertMetrics::default();
        if self.compared_limit == 0 {
            admission::compare(&self.db, &known, &mut supplied, &mut metrics, 0)?;
        } else {
            // Reuse only bytes already compared against an authenticated read at this
            // exact immutable location. Every occurrence still checks actual bytes.
            let mut unread = BTreeMap::new();
            for object in &page {
                if let Some(location) = known.get(&object.id) {
                    if let Some((previous, bytes)) = self
                        .compared
                        .get(&object.id)
                        .filter(|(previous, _)| previous == location)
                    {
                        if previous.canonical_length != object.bytes.len() {
                            return Err(StoreError::Integrity("object length collision"));
                        }
                        if *bytes != object.bytes {
                            return Err(StoreError::Integrity("object collision"));
                        }
                        self.diagnostics.collision_checks += 1;
                    } else {
                        unread.insert(object.id, *location);
                    }
                }
            }
            // The original locator map remains live beside the misses-only map.
            admission::compare(
                &self.db,
                &unread,
                &mut supplied,
                &mut metrics,
                known.len() * 512,
            )?;
            drop(unread);
        }
        self.note_collision_reads(metrics);
        drop(supplied);
        let mut stats = crate::PhysicalStorageReceipt::default();
        for mut object in page {
            if let Some(location) = known.get(&object.id) {
                if location.pack <= self.session.baseline_pack && first.contains(&object.id) {
                    let bytes = object.bytes.len() as u64;
                    self.checked.candidate_objects += 1;
                    self.checked.candidate_bytes += bytes;
                    self.checked.reused_objects += 1;
                    self.checked.reused_bytes += bytes;
                    self.receipt.candidate_objects += 1;
                    self.receipt.candidate_bytes += bytes;
                    self.receipt.reused_objects += 1;
                    self.receipt.reused_bytes += bytes;
                    self.receipt.preexisting_reused_objects += 1;
                    self.receipt.preexisting_reused_bytes += bytes;
                    diagnostic::occurrence(&mut object, 0, &mut stats);
                } else {
                    self.diagnostics.cross_batch_skipped_objects += 1;
                    self.diagnostics.cross_batch_skipped_bytes += object.bytes.len() as u64;
                    diagnostic::occurrence(&mut object, 2, &mut stats);
                }
                self.remember_comparison(object.id, *location, object.0.bytes);
            } else {
                diagnostic::occurrence(&mut object, 1, &mut stats);
                diagnostic::eligible(&object, &mut stats);
                self.push_pending(object, &mut probe_epoch)?;
            }
        }
        self.db.note_physical(stats);
        self.incoming = Vec::with_capacity(INITIALIZATION_SLAB_OBJECTS);
        Ok(())
    }

    fn remember_comparison(&mut self, id: ObjectId, location: read::Location, bytes: Vec<u8>) {
        if self
            .compared
            .get(&id)
            .is_some_and(|(previous, _)| *previous == location)
        {
            return;
        }
        // Charge payload capacity and conservative B-tree node/index ownership.
        let charge = bytes
            .capacity()
            .saturating_add(COMPARISON_REUSE_ENTRY_BYTES);
        if charge > self.compared_limit {
            return;
        }
        if let Some((_, previous)) = self.compared.remove(&id) {
            self.compared_bytes -= previous.capacity() + COMPARISON_REUSE_ENTRY_BYTES;
        }
        if self.compared_bytes + charge > self.compared_limit {
            // ponytail: clear on saturation; use incremental eviction only if measured thrashing warrants it.
            self.compared.clear();
            self.compared_bytes = 0;
        }
        self.compared_bytes += charge;
        self.compared.insert(id, (location, bytes));
    }

    fn note_collision_reads(&mut self, metrics: ObjectInsertMetrics) {
        self.diagnostics.collision_checks += metrics.collision_checks;
        self.diagnostics.conflict_read_calls += metrics.conflict_read_calls;
        self.diagnostics.conflict_read_rows += metrics.conflict_read_rows;
        self.diagnostics.conflict_read_bytes += metrics.conflict_read_bytes;
        self.diagnostics.conflict_read_ns += metrics.conflict_read_ns;
    }

    pub(crate) fn admit_object(&mut self, mut object: AuthenticatedCanonicalObject) -> Result<()> {
        self.session.ensure_active()?;
        if object.bytes.len() > ADMISSION_BATCH_BYTES {
            return Err(StoreError::Integrity("canonical object admission size"));
        }
        if let Some(&index) = self.pending.get(&object.id) {
            let mut stats = crate::PhysicalStorageReceipt::default();
            diagnostic::occurrence(&mut object, 2, &mut stats);
            self.db.note_physical(stats);
            return self.admit_duplicate(index, &object.bytes);
        }
        if let Some(&index) = self.incoming_index.get(&object.id) {
            let mut stats = crate::PhysicalStorageReceipt::default();
            diagnostic::occurrence(&mut object, 2, &mut stats);
            self.db.note_physical(stats);
            self.diagnostics.collision_checks += 1;
            if self.incoming[index].bytes != object.bytes {
                return Err(StoreError::Integrity("object collision"));
            }
            self.diagnostics.pending_duplicate_objects += 1;
            self.diagnostics.pending_duplicate_bytes += object.bytes.len() as u64;
            return Ok(());
        }
        let large = object.bytes.len() > INITIALIZATION_SLAB_BYTES;
        if !self.incoming.is_empty()
            && (self.incoming.len() == INITIALIZATION_SLAB_OBJECTS
                || self.incoming_bytes + object.bytes.len() > INITIALIZATION_SLAB_BYTES)
        {
            self.probe_incoming()?;
        }
        if large {
            self.flush_batch()?;
        }
        self.incoming_bytes += object.bytes.len();
        self.incoming_index.insert(object.id, self.incoming.len());
        self.incoming.push(object);
        if large {
            // Retire a maximal source object before requesting another source
            // page; it must not coexist with a second maximal canonical read.
            self.probe_incoming()?;
            self.flush_batch()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.probe_incoming()?;
        self.flush_batch()
    }

    fn admit_duplicate(&mut self, index: usize, bytes: &[u8]) -> Result<()> {
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

    fn push_pending(
        &mut self,
        object: AuthenticatedCanonicalObject,
        probe_epoch: &mut u64,
    ) -> Result<()> {
        if object.bytes.len() > ADMISSION_BATCH_BYTES {
            return Err(StoreError::Integrity("canonical object admission size"));
        }
        if !self.batch.is_empty()
            && (self.batch.len() == PHYSICAL_ADMISSION_BATCH_COUNT
                || self.batch_bytes.saturating_add(object.bytes.len())
                    > 2 * INITIALIZATION_SLAB_BYTES)
        {
            let before = self
                .session
                .publication_epoch
                .load(std::sync::atomic::Ordering::Acquire);
            self.flush_batch()?;
            let after = self
                .session
                .publication_epoch
                .load(std::sync::atomic::Ordering::Acquire);
            // Incoming/pending duplicate guards make our flushed batch disjoint
            // from this page. Any other publication keeps the old proof stale.
            if *probe_epoch == before && before.checked_add(1) == Some(after) {
                *probe_epoch = after;
            }
        }
        self.batch_epoch = if self.batch.is_empty() {
            *probe_epoch
        } else {
            self.batch_epoch.min(*probe_epoch)
        };
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
        let batch = self.take_batch()?;
        if batch.is_empty() {
            return Ok(());
        }
        let metrics = consume_checked_owned_page(&self.db, batch, &mut self.statement_number)?;
        self.checked.record(&metrics);
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
        record_admission_receipt(&mut self.receipt, metrics.insert, false);
        Ok(())
    }

    fn commit_pending(&mut self) -> Result<()> {
        let mut sql = AdmissionSqlMetrics::default();
        {
            let connection = self.db.writer()?;
            self.session.commit_pending(&connection, &mut sql, false)?;
        }
        let metrics = AdmissionBatchMetrics {
            insert: ObjectInsertMetrics {
                sql,
                ..Default::default()
            },
            begin_ns: sql.begin_ns,
            commit_ns: sql.commit_ns,
        };
        self.checked.record(&metrics);
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
        record_admission_receipt(&mut self.receipt, metrics.insert, false);
        Ok(())
    }
}

pub(crate) fn record_admission_receipt(
    receipt: &mut crate::CandidateReceipt,
    metrics: ObjectInsertMetrics,
    final_batch: bool,
) {
    let bytes = metrics.bytes + metrics.skipped_bytes;
    receipt.candidate_objects += metrics.submitted_rows;
    receipt.candidate_bytes += bytes;
    receipt.inserted_objects += metrics.objects;
    receipt.inserted_bytes += metrics.bytes;
    receipt.reused_objects += metrics.skipped_ids;
    receipt.reused_bytes += metrics.skipped_bytes;
    receipt.preexisting_reused_objects += metrics.skipped_ids;
    receipt.preexisting_reused_bytes += metrics.skipped_bytes;
    receipt.max_transaction_objects = receipt.max_transaction_objects.max(metrics.sql.max_objects);
    receipt.max_transaction_bytes = receipt.max_transaction_bytes.max(metrics.sql.max_bytes);
    receipt.admission_transactions += metrics.sql.commits - metrics.sql.final_commits;
    if final_batch {
        receipt.final_inserted_objects += metrics.objects;
        receipt.final_inserted_bytes += metrics.bytes;
    } else {
        receipt.batch_inserted_objects += metrics.objects;
        receipt.batch_inserted_bytes += metrics.bytes;
    }
}

/// Owns one synchronous admission through publication or abandonment.
/// Complete or drop this token before requesting another mutation on the same
/// Store from this thread. Other writers queue; authenticated reads remain usable.
pub struct WorkspaceAdmission {
    pub(crate) db: crate::schema::StoreDb,
    pub(crate) workspace_id: [u8; 16],
    admission: CheckedOutputAdmission,
    private_owner: Option<std::sync::Arc<dyn Send + Sync>>,
}

impl crate::LayerStackStore {
    pub fn workspace_admission(&self, workspace_id: [u8; 16]) -> Result<WorkspaceAdmission> {
        Ok(WorkspaceAdmission {
            db: self.db.clone(),
            workspace_id,
            private_owner: None,
            admission: CheckedOutputAdmission::with_session(
                &self.db,
                AdmissionSession::with_coalescing(&self.db, true)?,
                0,
            )?,
        })
    }

    // Match the shared driver's explicit task inputs and lifecycle callbacks.
    #[allow(clippy::too_many_arguments)]
    pub fn construct_workspace_files<I, S: Send, T: Send>(
        &self,
        workspace_id: [u8; 16],
        worker_limit: usize,
        task_count: usize,
        tasks: I,
        initialize: impl Fn(usize) -> Result<S> + Sync,
        step: impl Fn(&mut S, usize, I::Item, &mut FinalizedOutputWriter) -> Result<()> + Sync,
        finish: impl Fn(S) -> Result<T> + Sync,
    ) -> Result<(Vec<T>, WorkspaceAdmission)>
    where
        I: Iterator + Send,
        I::Item: Send,
    {
        let worker_limit = if self.db.small_content_format() {
            worker_limit.min(SMALL_CONTENT_WORKERS)
        } else {
            worker_limit
        };
        let mut token = self.workspace_admission(workspace_id)?;
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let mut admission_ns = 0_u64;
        let result = run_finalized_output(
            worker_limit,
            task_count,
            tasks,
            &cancelled,
            initialize,
            |state, ordinal, task, writer| {
                writer.set_small_content_format(self.db.small_content_format());
                writer.set_small_chain_format(self.db.small_chain_format());
                step(state, ordinal, task, writer)
            },
            finish,
            |page| {
                let started = Instant::now();
                let result = token.admission.admit_page(page);
                admission_ns = admission_ns.saturating_add(elapsed_ns(started));
                result
            },
        );
        let (output, pipeline) = token.admission.resolve(result)?;
        // Keep the final file batch owned by the Workspace token. Namespace
        // construction may add to it before staging closes the operation.
        let mut writer = OutputWriterMetrics::default();
        let output = output
            .into_iter()
            .map(|(result, metrics)| {
                writer.merge(metrics);
                result
            })
            .collect();
        if writer.producer_tasks != task_count as u64 {
            return token
                .admission
                .resolve(Err(StoreError::Integrity("Workspace file task coverage")));
        }
        crate::telemetry::note_workspace_candidate_delivery(
            writer.selected_memory_bytes,
            writer.selected_spill_bytes,
            writer.selected_storage_authentication_ns,
        );
        crate::telemetry::note_workspace_output_pipeline(
            pipeline.wall_ns,
            admission_ns,
            writer.blocked_ns,
            pipeline.consumer_idle_ns,
            pipeline.queue_peak_bytes,
        );
        Ok((output, token))
    }
}

impl WorkspaceAdmission {
    /// Keep construction resources accounted while admission retains encoded
    /// pages after the source DeferredObjectStore has been consumed.
    pub fn retain_private_owner(&mut self, owner: std::sync::Arc<dyn Send + Sync>) -> Result<()> {
        if self.private_owner.is_some() {
            return Err(StoreError::InvalidInput(
                "private admission owner already set",
            ));
        }
        self.private_owner = Some(owner);
        Ok(())
    }

    pub(crate) fn admit_remaining(
        self,
        objects: DeferredObjectStore,
    ) -> Result<(CheckedAdmission, u64, std::sync::Arc<AdmissionSession>)> {
        let session = self.admission.session.clone();
        session.resolve(self.admit_remaining_inner(objects))
    }

    fn admit_remaining_inner(
        mut self,
        objects: DeferredObjectStore,
    ) -> Result<(CheckedAdmission, u64, std::sync::Arc<AdmissionSession>)> {
        objects.consume_prevalidated_pages(|page| self.admission.admit_page(page))?;
        self.admission.flush()?;
        // Workspace staging starts a separate transaction and retains the session.
        // Close the bounded cohort before handing the connection to staging.
        self.admission.commit_pending()?;
        let finished = self.admission.finish()?;
        let admission = finished.checked;
        if admission.candidate_objects != admission.inserted_objects + admission.reused_objects
            || admission.candidate_bytes != admission.inserted_bytes + admission.reused_bytes
            || admission.max_transaction_objects > ADMISSION_BATCH_COUNT as u64
            || admission.max_transaction_bytes > ADMISSION_BATCH_BYTES as u64
        {
            return Err(StoreError::Integrity("checked admission equation"));
        }
        Ok((admission, finished.statement_number, finished.final_batch.1))
    }
}

fn consume_checked_owned_page(
    db: &crate::schema::StoreDb,
    batch: MissingBatch,
    statement_number: &mut u64,
) -> Result<AdmissionBatchMetrics> {
    let prepared = admission::PreparedAdmission::prepare_missing(db, batch)?;
    let (_, metrics) = prepared.publish(db, statement_number, |_, _, _| {
        #[cfg(feature = "test-instrumentation")]
        crate::schema::verification_store_checkpoint(
            crate::schema::VerificationStoreFault::LaterAdmissionBatch,
        )?;
        Ok(())
    })?;
    Ok(metrics)
}

impl ObjectSource for crate::schema::StoreDb {
    fn small_content_format(&self) -> bool {
        self.small_content_format()
    }
    fn compact_namespace(&self) -> bool {
        self.compact_namespace()
    }
    fn allocate_inode_serial(
        &self,
        scope: ObjectId,
    ) -> Result<layerfs_content::tree::compact::InodeSerial> {
        Ok(layerfs_content::tree::compact::InodeSerial::new(
            self.reserve_inode_serials(scope, 1)?.start,
        )?)
    }

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
    fn complete_file_output_matches_reachability_selection_across_spill_and_tree_boundaries() {
        use layerfs_content::file::{cdc::MAXIMUM_CHUNK_BYTES, extent::MAX_ENTRIES, rope};
        for size in [
            0,
            1,
            MAXIMUM_CHUNK_BYTES + 1,
            MAXIMUM_CHUNK_BYTES * (MAX_ENTRIES + 1),
        ] {
            for repetitive in [false, true] {
                let mut random = 7_u64;
                let bytes = (0..size)
                    .map(|_| {
                        random ^= random << 13;
                        random ^= random >> 7;
                        random ^= random << 17;
                        if repetitive {
                            0
                        } else {
                            random as u8
                        }
                    })
                    .collect::<Vec<_>>();
                let mut selected = ObjectBuffer::bounded_output(None).unwrap();
                let (root, counters) = rope::build(&mut selected, bytes.as_slice()).unwrap();
                let selected = selected.finish(root.0, counters.cdc_bytes_scanned).unwrap();
                let direct =
                    ObjectBuffer::build_complete_file(bytes.as_slice(), size as u64).unwrap();
                assert_eq!(direct.root_id, selected.root_id);
                assert_eq!(direct.objects.len(), selected.objects.len());
                assert_eq!(
                    direct.objects.encoded_bytes(),
                    selected.objects.encoded_bytes()
                );
                assert_eq!(
                    direct
                        .objects
                        .ids_in_order(usize::MAX)
                        .unwrap()
                        .unwrap()
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                    selected
                        .objects
                        .ids_in_order(usize::MAX)
                        .unwrap()
                        .unwrap()
                        .into_iter()
                        .collect::<BTreeSet<_>>()
                );
                assert!(!direct.objects.has_reference_index());
                if !repetitive && size > CANDIDATE_SPILL_BUFFER_BYTES {
                    assert!(direct.counters.spill_count > 0);
                }
            }
        }
        assert!(matches!(
            ObjectBuffer::build_complete_file(b"short".as_slice(), 6),
            Err(StoreError::Integrity("completed file length"))
        ));
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::Other.into())
            }
        }
        assert!(ObjectBuffer::build_complete_file(Broken, 1).is_err());
    }

    #[test]
    fn small_completed_files_match_streaming_and_validate_eof_before_output() {
        struct Fragmented<'a> {
            bytes: &'a [u8],
            interrupt: bool,
        }
        impl Read for Fragmented<'_> {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                if std::mem::take(&mut self.interrupt) {
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                let count = output.len().min(self.bytes.len()).min(7);
                output[..count].copy_from_slice(&self.bytes[..count]);
                self.bytes = &self.bytes[count..];
                Ok(count)
            }
        }
        for size in [0, 1, 8_191, 8_192] {
            let data = (0..size).map(|i| (i % 251) as u8).collect::<Vec<_>>();
            let mut expected = ObjectBuffer::empty().unwrap();
            let streamed =
                layerfs_content::file::rope::build_complete(&mut expected, data.as_slice())
                    .unwrap();
            let mut actual = ObjectBuffer::empty().unwrap();
            let completed = build_checked_file(
                &mut actual,
                Fragmented {
                    bytes: &data,
                    interrupt: size < layerfs_content::file::cdc::MINIMUM_CHUNK_BYTES,
                },
                size as u64,
            )
            .unwrap();
            assert_eq!(completed.root.0, streamed.root.0);
            assert_eq!(
                layerfs_content::file::content::length(&actual, completed.root).unwrap(),
                streamed.logical_len
            );
            assert_eq!(completed.counters, streamed.counters);
            let expected = expected.finish(streamed.root.0, size as u64).unwrap();
            let actual = actual.finish(completed.root.0, size as u64).unwrap();
            assert_eq!(actual.objects.len(), expected.objects.len());
            for id in expected.objects.ids_in_order(usize::MAX).unwrap().unwrap() {
                assert_eq!(
                    actual.objects.read_object(id).unwrap(),
                    expected.objects.read_object(id).unwrap()
                );
            }
        }
        for (bytes, expected) in [
            (b"x".as_slice(), 0),
            (b"".as_slice(), 1),
            (b"xy".as_slice(), 1),
        ] {
            let mut objects = ObjectBuffer::empty().unwrap();
            assert!(matches!(
                build_checked_file(&mut objects, bytes, expected),
                Err(StoreError::Integrity("completed file length"))
            ));
            assert_eq!(objects.objects.len(), 0);
        }
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::Other.into())
            }
        }
        let mut objects = ObjectBuffer::empty().unwrap();
        let error_after_payload = std::io::Cursor::new(b"x").chain(Broken);
        assert!(build_checked_file(&mut objects, error_after_payload, 1).is_err());
        assert_eq!(objects.objects.len(), 0);
    }

    #[test]
    fn partitioned_completed_files_share_direct_facts_and_keep_failures_private() {
        let mut random = 23_u64;
        let data = (0..2 * 1024 * 1024 + 17)
            .map(|_| {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                random as u8
            })
            .collect::<Vec<_>>();
        let private =
            ObjectBuffer::build_complete_file_partition(data.as_slice(), data.len() as u64, 4)
                .unwrap();
        assert!(private.counters.spill_count > 0);
        let expected_ids = private
            .objects
            .ids_in_order(usize::MAX)
            .unwrap()
            .unwrap()
            .into_iter()
            .collect::<BTreeSet<_>>();
        let mut direct_ids = BTreeSet::new();
        let consumed = AtomicU64::new(0);
        struct ObservedEof<'a> {
            bytes: &'a [u8],
            consumed: &'a AtomicU64,
        }
        impl Read for ObservedEof<'_> {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                if self.bytes.is_empty() && self.consumed.load(Ordering::Acquire) == 0 {
                    return Err(std::io::Error::other("no output before EOF"));
                }
                self.bytes.read(output)
            }
        }
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let (direct, _) = run_finalized_output(
            1,
            1,
            std::iter::once(()),
            &cancelled,
            |_| Ok(None),
            |result, _, _, writer| {
                *result = Some(writer.build_complete_file(
                    ObservedEof {
                        bytes: &data,
                        consumed: &consumed,
                    },
                    data.len() as u64,
                )?);
                Ok(())
            },
            |result| result.ok_or(StoreError::Integrity("missing completion")),
            |page| {
                consumed.fetch_add(page.len() as u64, Ordering::Release);
                direct_ids.extend(page.into_iter().map(|object| object.id));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(direct[0].0 .0, private.root_id);
        assert_eq!(
            direct[0].0 .1.cdc_bytes_scanned,
            private.counters.cdc_bytes_scanned
        );
        assert_eq!(direct[0].0 .1.spill_count, 0);
        assert_eq!(direct[0].0 .1.first_store_write_bytes, 0);
        assert_eq!(direct[0].1.selected_spill_bytes, 0);
        assert_eq!(direct[0].1.selected_memory_bytes, direct[0].1.payload_bytes);
        assert!(direct[0].1.partial_peak_payload_bytes <= INITIALIZATION_SLAB_BYTES as u64);
        assert_eq!(direct_ids, expected_ids);
        let mut buffer = ObjectBuffer::bounded_output(None).unwrap();
        buffer.partition_output(4).unwrap();
        assert_eq!(
            buffer.objects.memory_limit * 4,
            CANDIDATE_SPILL_BUFFER_BYTES
        );
        assert_eq!(buffer.objects.index_limit * 4, CANDIDATE_INDEX_BYTES);
        assert_eq!(
            (buffer.objects.spill_buffer_bytes + spill::ID_BUFFER_BYTES) * 4
                + spill::ID_BUFFER_BYTES,
            CANDIDATE_SPILL_BUFFER_BYTES
        );
        assert!(buffer.partition_output(0).is_err());

        struct Broken {
            remaining: usize,
            random: u64,
        }
        impl Read for Broken {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                if self.remaining == 0 {
                    return Err(std::io::ErrorKind::Other.into());
                }
                let size = bytes.len().min(self.remaining);
                for byte in &mut bytes[..size] {
                    self.random ^= self.random << 13;
                    self.random ^= self.random >> 7;
                    self.random ^= self.random << 17;
                    *byte = self.random as u8;
                }
                self.remaining -= size;
                Ok(size)
            }
        }
        let path = std::env::temp_dir().join(format!(
            "layerfs-private-failure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = crate::LayerStackStore::create(&path).unwrap();
        let before = store.store_counts().unwrap();
        let published = std::sync::atomic::AtomicBool::new(false);
        let failed = store.construct_workspace_files(
            [5; 16],
            4,
            1,
            std::iter::once(()),
            |_| Ok(()),
            |_, _, _, writer| {
                let result = writer.build_complete_file(
                    Broken {
                        remaining: 2 * 1024 * 1024,
                        random: 31,
                    },
                    2 * 1024 * 1024 + 1,
                );
                published.store(store.store_counts()? != before, Ordering::Release);
                result.map(|_| ())
            },
            |_| Ok(()),
        );
        assert!(failed.is_err());
        assert!(published.load(Ordering::Acquire));
        assert_eq!(store.store_counts().unwrap(), before);
        assert!(ObjectBuffer::build_complete_file_partition(
            data.as_slice(),
            data.len() as u64 + 1,
            4
        )
        .is_err());
        drop(store);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn seen_index_spill_preserves_exact_membership_and_private_cleanup() {
        let ids = [b"first".as_slice(), b"second", b"third"].map(ObjectId::for_bytes);
        let mut seen = SpillableObjectSet::empty().unwrap();
        assert_eq!(seen.insert_page(&ids[..2]).unwrap(), ids[..2]);
        seen.spill().unwrap();
        let path = match &seen.storage {
            SeenStorage::Spill {
                connection, _path, ..
            } => {
                let connection = connection.lock().unwrap();
                let plan: String = connection
                    .query_row(
                        "EXPLAIN QUERY PLAN SELECT 1 FROM seen WHERE id=?1",
                        [ids[0].as_bytes().as_slice()],
                        |row| row.get(3),
                    )
                    .unwrap();
                assert!(plan.contains("SEARCH") && plan.contains("PRIMARY KEY"));
                assert_eq!(
                    connection
                        .pragma_query_value::<i64, _>(None, "cache_size", |row| row.get(0))
                        .unwrap(),
                    -4096
                );
                _path.0.clone()
            }
            _ => panic!("forced seen spill"),
        };
        assert!(seen.contains(ids[0]).unwrap());
        assert!(!seen.contains(ids[2]).unwrap());
        assert_eq!(
            seen.insert_page(&[ids[1], ids[2], ids[2]]).unwrap(),
            [ids[2]]
        );
        assert_eq!(seen.count, 3);
        drop(seen);
        assert!(!path.exists());
    }

    #[test]
    fn consumer_batches_flushed_duplicate_checks_without_recounting() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-duplicate-pages-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db = crate::schema::StoreDb::create(root.join("store.sqlite")).unwrap();
        let mut admission = CheckedOutputAdmission::new(&db).unwrap();
        let objects = (0..300_u64)
            .map(|index| {
                let bytes = layerfs_content::encode_bytes_object(&index.to_le_bytes()).unwrap();
                AuthenticatedCanonicalObject::new(bytes, None).unwrap()
            })
            .collect::<Vec<_>>();
        admission.admit_page(objects.clone()).unwrap();
        admission.flush().unwrap();
        assert_eq!(
            admission.seen.count, 0,
            "published fresh objects use the session watermark, not a second index"
        );
        #[cfg(feature = "test-instrumentation")]
        crate::schema::reset_sql_trace();
        admission
            .admit_page(objects.iter().rev().cloned().collect())
            .unwrap();
        admission.flush().unwrap();
        #[cfg(feature = "test-instrumentation")]
        assert_eq!(
            crate::schema::sql_trace()
                .iter()
                .filter(|sql| sql.contains("WHERE object_id IN ("))
                .count(),
            3
        );
        assert_eq!(
            (
                admission.checked.candidate_objects,
                admission.checked.inserted_objects,
                admission.checked.reused_objects
            ),
            (300, 300, 0)
        );
        let mut corrupt = objects[0].clone();
        // Deliberately violate the private invariant to exercise exact conflict comparison.
        // Production owners expose no mutable bytes.
        corrupt.0.bytes = objects[1].bytes.clone();
        assert!(matches!(
            admission
                .admit_page(vec![corrupt])
                .and_then(|_| admission.flush()),
            Err(StoreError::Integrity("object collision"))
        ));
        drop(admission);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fresh_filter_negatives_false_positives_and_collisions_keep_exact_admission() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-fresh-filter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db = crate::schema::StoreDb::create(root.join("store.sqlite")).unwrap();
        let mut owner = CheckedOutputAdmission::new_for_initialization(&db).unwrap();
        let first = AuthenticatedCanonicalObject::new(
            layerfs_content::encode_bytes_object(b"one").unwrap(),
            None,
        )
        .unwrap();
        let second = AuthenticatedCanonicalObject::new(
            layerfs_content::encode_bytes_object(b"two").unwrap(),
            None,
        )
        .unwrap();
        let bitmap_bytes = owner
            .session
            .fresh_ids
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .len()
            * std::mem::size_of::<u64>();
        assert_eq!(bitmap_bytes, FRESH_ADMISSION_FILTER_BYTES);
        assert_eq!(
            owner.seen.memory_limit
                + bitmap_bytes
                + owner.compared_limit
                + small_candidates::INDEX_BYTES,
            CANDIDATE_INDEX_BYTES / 4
        );
        let mut ids = vec![first.id, second.id];
        owner.session.retain_possible_ids(&mut ids).unwrap();
        assert!(
            ids.is_empty(),
            "empty filter proves only definite negatives"
        );
        owner.admit_object(first.clone()).unwrap();
        owner.flush().unwrap();
        assert_eq!(owner.diagnostics.fresh_probe_ids_skipped, 1);
        assert_eq!(owner.diagnostics.probe_ids_queried, 0);
        assert!(!db.reader().unwrap().is_autocommit());
        let mut ids = vec![first.id];
        owner.session.retain_possible_ids(&mut ids).unwrap();
        assert_eq!(
            ids,
            vec![first.id],
            "uncommitted publication must already set its bits"
        );

        // A saturated filter is a deterministic false-positive control. It must
        // only request ordinary SQL lookup, never turn a missing object into reuse.
        owner
            .session
            .fresh_ids
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .fill(u64::MAX);
        owner.admit_object(second.clone()).unwrap();
        owner.flush().unwrap();
        assert_eq!(owner.checked.inserted_objects, 2);
        assert_eq!(db.read_object_row(second.id).unwrap(), second.bytes);
        owner.admit_object(first.clone()).unwrap();
        owner.flush().unwrap();
        assert_eq!(owner.diagnostics.probe_ids_queried, 2);
        assert_eq!(owner.checked.candidate_objects, 2);
        assert_eq!(owner.checked.inserted_objects, 2);
        assert_eq!(owner.diagnostics.collision_checks, 1);

        let mut corrupt = first;
        corrupt.0.bytes = second.bytes.clone(); // Test-only violation of private authenticated ownership.
        assert!(matches!(
            owner
                .session()
                .resolve(owner.admit_object(corrupt).and_then(|_| owner.flush())),
            Err(StoreError::Integrity("object collision"))
        ));
        assert!(db.reader().unwrap().is_autocommit());
        assert_eq!(
            db.reader()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM objects", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(owner);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fresh_filter_nonempty_and_orphaned_stores_keep_sql_fallback() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-filter-fallback-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&root).unwrap();
        for orphan in [false, true] {
            let db = crate::schema::StoreDb::create(root.join(format!("{orphan}.sqlite"))).unwrap();
            let original = AuthenticatedCanonicalObject::new(
                layerfs_content::encode_bytes_object(b"old").unwrap(),
                None,
            )
            .unwrap();
            if orphan {
                let connection = db.writer().unwrap();
                connection
                    .pragma_update(None, "foreign_keys", false)
                    .unwrap();
                connection
                    .execute(
                        "INSERT INTO objects VALUES (?1,?2,1,0,0)",
                        rusqlite::params![
                            original.id.as_bytes().as_slice(),
                            original.bytes.len() as i64
                        ],
                    )
                    .unwrap();
                connection
                    .pragma_update(None, "foreign_keys", true)
                    .unwrap();
            } else {
                let mut initial = CheckedOutputAdmission::new(&db).unwrap();
                initial.admit_object(original.clone()).unwrap();
                finish_segment_admission(&db, initial);
            }
            let mut owner = CheckedOutputAdmission::new_for_initialization(&db).unwrap();
            assert!(owner.session.fresh_ids.is_none());
            assert_eq!(
                owner.seen.memory_limit + owner.compared_limit + small_candidates::INDEX_BYTES,
                CANDIDATE_INDEX_BYTES / 4
            );
            let mut ids = vec![original.id, ObjectId::for_bytes(b"missing")];
            let expected = ids.clone();
            owner.session.retain_possible_ids(&mut ids).unwrap();
            assert_eq!(ids, expected);
            if orphan {
                assert_eq!(owner.session.baseline_pack, 0);
                owner.session.retain(); // No writes: this case exercises only the activation guard.
            } else {
                owner.admit_object(original.clone()).unwrap();
                owner.flush().unwrap();
                assert_eq!(owner.diagnostics.probe_ids_queried, 1);
                assert_eq!(owner.checked.reused_objects, 1);
                let mut corrupt = original.clone();
                corrupt.0.bytes = layerfs_content::encode_bytes_object(b"bad").unwrap();
                assert!(matches!(
                    owner
                        .session()
                        .resolve(owner.admit_object(corrupt).and_then(|_| owner.flush())),
                    Err(StoreError::Integrity("object collision"))
                ));
                assert_eq!(db.read_object_row(original.id).unwrap(), original.bytes);
            }
            drop(owner);
            drop(db);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finalized_output_tasks_restore_coverage_and_join_all_failures() {
        use std::sync::atomic::AtomicBool;
        use std::sync::{Condvar, Mutex};
        for workers in [1, 4] {
            let cancelled = AtomicBool::new(false);
            let order = (Mutex::new((3_usize, Vec::new())), Condvar::new());
            let (output, metrics) = run_finalized_output(
                workers,
                4,
                0..4,
                &cancelled,
                |_| Ok(Vec::new()),
                |local, ordinal, task, _| {
                    assert_eq!(ordinal, task);
                    if workers > 1 {
                        let mut state = order.0.lock().unwrap();
                        while state.0 != ordinal {
                            state = order.1.wait(state).unwrap();
                        }
                        state.1.push(ordinal);
                        state.0 = state.0.saturating_sub(1);
                        order.1.notify_all();
                    }
                    local.push(ordinal);
                    Ok(())
                },
                Ok,
                |_| Ok(()),
            )
            .unwrap();
            assert_eq!(metrics.producers_after, 0);
            assert_eq!(
                output
                    .iter()
                    .map(|(_, metrics)| metrics.producer_tasks)
                    .sum::<u64>(),
                4
            );
            let mut ordinals = output
                .into_iter()
                .flat_map(|(rows, _)| rows)
                .collect::<Vec<_>>();
            ordinals.sort();
            assert_eq!(ordinals, vec![0, 1, 2, 3]);
            if workers > 1 {
                assert_eq!(order.0.lock().unwrap().1, vec![3, 2, 1, 0]);
            }
        }
        let cancelled = AtomicBool::new(false);
        let (empty, metrics) = run_finalized_output(
            4,
            0,
            std::iter::empty::<()>(),
            &cancelled,
            |_| -> Result<()> { panic!("empty worker") },
            |_, _, _, _| Ok(()),
            Ok,
            |_| Ok(()),
        )
        .unwrap();
        assert!(empty.is_empty());
        assert_eq!(metrics.producer_peak, 0);
        assert!(run_finalized_output(
            1,
            2,
            0..1,
            &cancelled,
            |_| Ok(()),
            |_, _, _, _| Ok(()),
            Ok,
            |_| Ok(())
        )
        .is_err());

        for failure in [
            "producer",
            "producer-panic",
            "consumer",
            "consumer-panic",
            "finish",
            "finish-panic",
            "writer-finish",
        ] {
            let cancelled = AtomicBool::new(false);
            let joined = AtomicU64::new(0);
            struct Finished<'a>(&'a AtomicU64);
            impl Drop for Finished<'_> {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::Release);
                }
            }
            let result = run_finalized_output(
                2,
                64,
                0..64,
                &cancelled,
                |_| Ok(Finished(&joined)),
                |_, index, _, writer| {
                    if index == 0 && failure == "producer" {
                        return Err(StoreError::Integrity("test producer failure"));
                    }
                    if index == 0 && failure == "producer-panic" {
                        panic!("test producer panic");
                    }
                    writer.push_object(
                        AuthenticatedCanonicalObject::new(
                            layerfs_content::encode_bytes_object(b"selected")?,
                            None,
                        )?,
                        false,
                    )?;
                    if failure == "writer-finish" {
                        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
                        drop(receiver);
                        writer.sender = sender;
                    } else {
                        writer.flush()?;
                    }
                    Ok(())
                },
                |owner| {
                    if failure == "finish" {
                        return Err(StoreError::Integrity("test finish failure"));
                    }
                    if failure == "finish-panic" {
                        panic!("test finish panic");
                    }
                    drop(owner);
                    Ok(())
                },
                |_| {
                    if failure == "consumer-panic" {
                        panic!("test consumer panic");
                    }
                    if failure == "consumer" {
                        Err(StoreError::Integrity("test consumer failure"))
                    } else {
                        Ok(())
                    }
                },
            );
            assert!(result.is_err(), "{failure}");
            assert_eq!(joined.load(Ordering::Acquire), 2, "{failure}");
            assert!(cancelled.load(Ordering::Acquire), "{failure}");
        }
    }

    #[test]
    fn workspace_cohorts_flush_for_staging_and_rollback_pending_and_committed_objects() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-workspace-cohorts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        for outcome in ["publish", "drop", "collision", "commit_failure"] {
            let store =
                crate::LayerStackStore::create(root.join(format!("{outcome}.sqlite"))).unwrap();
            let baseline = AuthenticatedCanonicalObject::new(
                layerfs_content::encode_bytes_object(b"retained baseline").unwrap(),
                None,
            )
            .unwrap();
            let mut initial = CheckedOutputAdmission::new(&store.db).unwrap();
            initial.admit_object(baseline.clone()).unwrap();
            finish_segment_admission(&store.db, initial);
            let mut token = store.workspace_admission([1; 16]).unwrap();
            assert_eq!(token.admission.compared_limit, 0);
            for batch in 0..9_u64 {
                for index in 0..8_u64 {
                    let mut payload = vec![0; 65_500];
                    payload[..8].copy_from_slice(&(batch * 8 + index).to_le_bytes());
                    token
                        .admission
                        .admit_object(
                            AuthenticatedCanonicalObject::new(
                                layerfs_content::encode_bytes_object(&payload).unwrap(),
                                None,
                            )
                            .unwrap(),
                        )
                        .unwrap();
                }
                token.admission.flush().unwrap();
            }
            assert!(token.admission.checked.transactions > 0);
            assert!(!store.db.reader().unwrap().is_autocommit());
            assert!(token.admission.checked.max_transaction_objects < 8192);
            assert!(token.admission.checked.max_transaction_bytes < 4 * 1024 * 1024);
            assert_eq!(
                store.db.read_object_row(baseline.id).unwrap(),
                baseline.bytes
            );
            let mut payload = vec![0; 65_500];
            payload[..8].copy_from_slice(&71_u64.to_le_bytes());
            let pending = AuthenticatedCanonicalObject::new(
                layerfs_content::encode_bytes_object(&payload).unwrap(),
                None,
            )
            .unwrap();
            std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        assert_eq!(store.db.read_object_row(pending.id).unwrap(), pending.bytes);
                    })
                    .join()
                    .unwrap();
            });
            token.admission.admit_object(pending.clone()).unwrap();
            token.admission.flush().unwrap();
            assert_eq!(token.admission.checked.inserted_objects, 72);
            assert!(!store.db.reader().unwrap().is_autocommit());
            if outcome == "publish" {
                let (receipt, _, session) = token
                    .admit_remaining(DeferredObjectStore::new().unwrap())
                    .unwrap();
                assert!(
                    store.db.reader().unwrap().is_autocommit(),
                    "staging must start outside admission transaction"
                );
                assert_eq!(receipt.inserted_objects, 72);
                assert_eq!(receipt.transactions, 2);
                session.retain();
                drop(session);
                assert_eq!(store.store_counts().unwrap().objects, 73);
            } else if outcome == "commit_failure" {
                let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let hook_fired = fired.clone();
                store
                    .db
                    .writer()
                    .unwrap()
                    .commit_hook(Some(move || !hook_fired.swap(true, Ordering::SeqCst)))
                    .unwrap();
                assert!(token
                    .admit_remaining(DeferredObjectStore::new().unwrap())
                    .is_err());
                assert!(fired.load(Ordering::SeqCst));
                assert!(store.db.reader().unwrap().is_autocommit());
                assert_eq!(store.store_counts().unwrap().objects, 1);
                assert!(store.workspace_stage([1; 16]).unwrap().is_none());
                assert_eq!(
                    store.db.read_object_row(baseline.id).unwrap(),
                    baseline.bytes
                );
            } else {
                if outcome == "collision" {
                    let mut corrupt = pending.clone();
                    payload[8] = 1;
                    corrupt.0.bytes = layerfs_content::encode_bytes_object(&payload).unwrap();
                    let result = token
                        .admission
                        .admit_object(corrupt)
                        .and_then(|_| token.admission.flush());
                    assert!(matches!(
                        result,
                        Err(StoreError::Integrity("object collision"))
                    ));
                }
                drop(token);
                assert!(store.db.reader().unwrap().is_autocommit());
                assert_eq!(store.store_counts().unwrap().objects, 1);
                assert_eq!(
                    store.db.read_object_row(baseline.id).unwrap(),
                    baseline.bytes
                );
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workspace_delivery_selects_before_admission_and_deduplicates_across_phases() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-output-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = crate::LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let canonical = [b"existing".as_slice(), b"selected", b"provisional"]
            .map(|bytes| layerfs_content::encode_bytes_object(bytes).unwrap());
        let ids = canonical.each_ref().map(|bytes| ObjectId::for_bytes(bytes));
        let selected = |index: usize| {
            let mut buffer = ObjectBuffer::empty().unwrap();
            buffer.put(&canonical[2]).unwrap();
            let id = buffer.put(&canonical[index]).unwrap();
            buffer.finish(id, 0).unwrap().objects
        };
        store
            .workspace_admission([0; 16])
            .unwrap()
            .admit_remaining(selected(0))
            .unwrap()
            .2
            .retain();
        let (_, token) = store
            .construct_workspace_files(
                [1; 16],
                2,
                3,
                [0, 1, 1].into_iter(),
                |_| Ok(()),
                |_, _, index, writer| writer.send_selected(selected(index)),
                |_| Ok(()),
            )
            .unwrap();
        let (receipt, _, session) = token.admit_remaining(selected(1)).unwrap();
        session.retain();
        drop(session);
        assert_eq!(
            (
                receipt.candidate_objects,
                receipt.inserted_objects,
                receipt.reused_objects
            ),
            (2, 1, 1)
        );
        assert_eq!(
            receipt.candidate_bytes,
            (canonical[0].len() + canonical[1].len()) as u64
        );
        assert_eq!(store.db.read_object_row(ids[0]).unwrap(), canonical[0]);
        assert_eq!(store.db.read_object_row(ids[1]).unwrap(), canonical[1]);
        assert!(store.db.object_membership(&[ids[2]]).unwrap().is_empty());
        assert_eq!(store.store_counts().unwrap().objects, 2);
        assert_eq!(store.store_counts().unwrap().commits, 0);
        assert!(store.workspace_stage([1; 16]).unwrap().is_none());
        crate::schema::set_transaction_failure_at(Some(1));
        let (_, pending) = store
            .construct_workspace_files(
                [2; 16],
                1,
                1,
                std::iter::once(2),
                |_| Ok(()),
                |_, _, index, writer| writer.send_selected(selected(index)),
                |_| Ok(()),
            )
            .unwrap();
        // The small final file batch stays in the token until namespace delivery
        // closes admission. The consumer and this final flush run on this thread.
        assert_eq!(store.store_counts().unwrap().objects, 2);
        assert!(store.db.object_membership(&[ids[2]]).unwrap().is_empty());
        let failed = pending.admit_remaining(selected(2));
        crate::schema::set_transaction_failure_at(None);
        assert!(
            matches!(
                failed,
                Err(StoreError::Integrity("injected transaction failure"))
            ),
            "actual error (None means unexpected success): {:?}",
            failed.as_ref().err()
        );
        assert_eq!(store.store_counts().unwrap().objects, 2);
        assert!(store.workspace_stage([2; 16]).unwrap().is_none());
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn owned_delivery_authenticates_fresh_spill_before_handoff() {
        use std::os::unix::fs::FileExt;
        let mut buffer = ObjectBuffer::empty().unwrap();
        let root = buffer
            .put_owned(layerfs_content::encode_bytes_object(b"spill witness").unwrap())
            .unwrap();
        buffer.objects.spill().unwrap();
        let (writer, offset) = match &buffer.objects.storage {
            DeferredObjects::Spill(spill) => (
                spill.writer.as_ref().unwrap().try_clone().unwrap(),
                spill.location(root).unwrap().unwrap().0,
            ),
            _ => panic!("forced payload spill"),
        };
        let built = buffer.finish(root, 0).unwrap();
        // Simulate corruption through a test-only descriptor retained before seal.
        writer.write_all_at(&[0xff], offset + 9).unwrap();
        let mut handed_off = 0;
        assert!(built
            .objects
            .consume_prevalidated_pages(|page| {
                handed_off += page.len();
                Ok(())
            })
            .is_err());
        assert_eq!(handed_off, 0);
    }

    #[test]
    fn checked_owned_output_requires_complete_framing_and_identity() {
        let canonical = layerfs_content::encode_bytes_object(b"checked owner").unwrap();
        let owner = AuthenticatedCanonicalObject::new(canonical.clone(), None).unwrap();
        assert_eq!(owner.id, ObjectId::for_bytes(&canonical));
        assert_eq!(
            std::mem::size_of::<AuthenticatedCanonicalObject>(),
            std::mem::size_of::<CanonicalObject>() + std::mem::size_of::<PhysicalHints>()
        );
        assert!(matches!(
            AuthenticatedCanonicalObject::new(
                canonical.clone(),
                Some(ObjectId::for_bytes(b"wrong"))
            ),
            Err(CoreError::IdentityMismatch)
        ));
        for invalid in [
            canonical[..8].to_vec(),
            [canonical.as_slice(), &[0]].concat(),
            b"unframed".to_vec(),
        ] {
            let expected = ObjectId::for_bytes(&invalid);
            assert!(AuthenticatedCanonicalObject::new(invalid.clone(), None).is_err());
            assert!(AuthenticatedCanonicalObject::new(invalid, Some(expected)).is_err());
        }
    }

    /// A source that serves exactly the listed canonical objects through the one
    /// bounded batch entry, and refuses every other source call.
    struct BatchOnlySource(Vec<CanonicalObject>);
    impl ObjectSource for BatchOnlySource {
        fn read_object(&self, _: ObjectId) -> Result<Vec<u8>> {
            Err(StoreError::Integrity("unexpected single read"))
        }
        fn read_authenticated_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
            Ok(ids
                .iter()
                .map(|id| {
                    self.0
                        .iter()
                        .find(|object| object.id == *id)
                        .cloned()
                        .ok_or(StoreError::MissingObject(*id))
                })
                .collect::<Result<Vec<_>>>()?)
        }
    }

    fn batch_object(tag: &[u8]) -> CanonicalObject {
        let bytes = layerfs_content::encode_bytes_object(tag).unwrap();
        CanonicalObject {
            id: ObjectId::for_bytes(&bytes),
            bytes,
        }
    }

    /// The `ObjectRead` contract promises callbacks in demand order. A buffer that
    /// owns part of the batch must not emit its owned objects ahead of earlier
    /// unowned demands.
    #[test]
    fn owned_objects_do_not_overtake_earlier_source_demands_in_a_batch() {
        let source_only = batch_object(b"batch-source-only");
        let owned = batch_object(b"batch-owned");
        for spill in [false, true] {
            let source = BatchOnlySource(vec![source_only.clone()]);
            let mut buffer = ObjectBuffer::new(&source).unwrap();
            let owned_id = buffer.put_owned(owned.bytes.clone()).unwrap();
            assert_eq!(owned_id, owned.id);
            if spill {
                buffer.objects.spill().unwrap();
            }
            for ids in [
                vec![source_only.id, owned.id],
                vec![owned.id, source_only.id],
                vec![source_only.id, owned.id, source_only.id],
                vec![owned.id, owned.id, source_only.id],
            ] {
                let mut seen = Vec::new();
                ObjectStore::get_authenticated_canonical_batch(&buffer, &ids, |id, bytes| {
                    seen.push((id, ObjectId::for_bytes(bytes)));
                    Ok(())
                })
                .unwrap();
                assert_eq!(
                    seen,
                    ids.iter().map(|id| (*id, *id)).collect::<Vec<_>>(),
                    "spill={spill} ids={ids:?}"
                );
            }
            let mut calls = 0;
            ObjectStore::get_authenticated_canonical_batch(&buffer, &[], |_, _| {
                calls += 1;
                Ok(())
            })
            .unwrap();
            assert_eq!(calls, 0);
        }
    }

    /// The batch-count ceiling is a property of the entrypoint, so an all-owned
    /// batch that never reaches the source must be rejected the same way.
    #[test]
    fn oversized_all_owned_batch_is_rejected_before_any_callback() {
        for spill in [false, true] {
            let mut buffer = ObjectBuffer::empty().unwrap();
            let mut ids = Vec::new();
            for index in 0..=OBJECT_PAGE_COUNT {
                let object = batch_object(format!("oversized-owned-{index}").as_bytes());
                ids.push(buffer.put_owned(object.bytes).unwrap());
            }
            assert_eq!(ids.len(), OBJECT_PAGE_COUNT + 1);
            if spill {
                buffer.objects.spill().unwrap();
            }
            let mut calls = 0;
            let error = ObjectStore::get_authenticated_canonical_batch(&buffer, &ids, |_, _| {
                calls += 1;
                Ok(())
            })
            .unwrap_err();
            assert_eq!(calls, 0, "spill={spill}");
            assert!(
                matches!(error, CoreError::InvalidRecord(_)),
                "spill={spill} error={error:?}"
            );
        }
    }

    #[test]
    fn mixed_batch_reports_callback_and_identity_failures_after_the_source_call() {
        let source_only = batch_object(b"batch-source-failure");
        let owned = batch_object(b"batch-owned-failure");
        let source = BatchOnlySource(vec![source_only.clone()]);
        let mut buffer = ObjectBuffer::new(&source).unwrap();
        let owned_id = buffer.put_owned(owned.bytes.clone()).unwrap();
        let ids = [source_only.id, owned_id];
        let mut seen = Vec::new();
        let error = ObjectStore::get_authenticated_canonical_batch(&buffer, &ids, |id, _| {
            seen.push(id);
            Err(CoreError::Io)
        })
        .unwrap_err();
        assert_eq!(error, CoreError::Io);
        assert_eq!(seen, vec![source_only.id]);
        // A source that answers with the wrong identity is rejected before the
        // owned object is emitted.
        let wrong = batch_object(b"batch-wrong-identity");
        let source = BatchOnlySource(vec![CanonicalObject {
            id: source_only.id,
            bytes: wrong.bytes.clone(),
        }]);
        let mut buffer = ObjectBuffer::new(&source).unwrap();
        let owned_id = buffer.put_owned(owned.bytes.clone()).unwrap();
        let mut seen = Vec::new();
        let error = ObjectStore::get_authenticated_canonical_batch(
            &buffer,
            &[source_only.id, owned_id],
            |id, _| {
                seen.push(id);
                Ok(())
            },
        )
        .unwrap_err();
        assert!(
            matches!(error, CoreError::IdentityMismatch | CoreError::Io),
            "error={error:?}"
        );
        assert!(seen.is_empty(), "seen={seen:?}");
        // A demanded object that the source does not have is missing, not a
        // silently skipped demand.
        let absent = batch_object(b"batch-absent");
        let source = BatchOnlySource(vec![source_only.clone()]);
        let mut buffer = ObjectBuffer::new(&source).unwrap();
        let owned_id = buffer.put_owned(owned.bytes.clone()).unwrap();
        let mut seen = Vec::new();
        let error = ObjectStore::get_authenticated_canonical_batch(
            &buffer,
            &[absent.id, owned_id],
            |id, _| {
                seen.push(id);
                Ok(())
            },
        )
        .unwrap_err();
        assert!(matches!(error, CoreError::MissingObject), "error={error:?}");
        assert!(seen.is_empty(), "seen={seen:?}");
    }

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
        admission: CheckedOutputAdmission,
    ) -> (crate::CandidateReceipt, Vec<ObjectId>) {
        let finished = admission.finish().unwrap();
        let ids = finished
            .final_batch
            .iter()
            .map(|object| object.id)
            .collect::<Vec<_>>();
        let mut statement_number = finished.statement_number;
        let (_, metrics) = PreparedAdmission::prepare_missing(db, finished.final_batch)
            .unwrap()
            .publish(db, &mut statement_number, |_, _, _| Ok(()))
            .unwrap();
        let mut receipt = finished.receipt;
        record_admission_receipt(&mut receipt, metrics.insert, true);
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
    fn s1_inode_origin_survives_delivery_and_delta_origin_stays_full() {
        use layerfs_content::tree::{
            batch::inode_table_apply_sorted,
            inode::{
                codec::{encode_inode_table_node, InodeTableNodeV1},
                InodeId, InodeTableRoot,
            },
        };
        for spill in [false, true] {
            let directory = std::env::temp_dir().join(format!(
                "layerfs-s1-origin-{}-{}-{spill}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&directory).unwrap();
            let db = crate::schema::StoreDb::create(directory.join("store.sqlite")).unwrap();
            let mut entries = Vec::new();
            let mut initial = Vec::new();
            for serial in 0..64u64 {
                let bytes = layerfs_content::encode_bytes_object(&serial.to_be_bytes()).unwrap();
                let id = ObjectId::for_bytes(&bytes);
                initial.push(CanonicalObject { id, bytes });
                entries.push((InodeId::allocate([7; 32], serial), id));
            }
            entries.sort();
            let leaf = encode_inode_table_node(&InodeTableNodeV1::Leaf(entries.clone())).unwrap();
            let mut prior = ObjectId::for_bytes(&leaf);
            initial.push(CanonicalObject {
                id: prior,
                bytes: leaf,
            });
            let mut admission = CheckedOutputAdmission::new(&db).unwrap();
            admission.admit(sealed_segment(initial)).unwrap();
            finish_segment_admission(&db, admission);

            for generation in 0..2u64 {
                let mut buffer = ObjectBuffer::new(&db).unwrap();
                let record = buffer
                    .put_owned(
                        layerfs_content::encode_bytes_object(&(1000 + generation).to_be_bytes())
                            .unwrap(),
                    )
                    .unwrap();
                entries[generation as usize].1 = record;
                let expected =
                    encode_inode_table_node(&InodeTableNodeV1::Leaf(entries.clone())).unwrap();
                let (next, _) = inode_table_apply_sorted(
                    &mut buffer,
                    InodeTableRoot(prior),
                    std::iter::once(Ok((entries[generation as usize].0, Some(record)))),
                )
                .unwrap();
                assert_eq!(next.0, ObjectId::for_bytes(&expected));
                if spill {
                    buffer.objects.spill().unwrap();
                }
                let built = buffer.finish(next.0, 0).unwrap();
                built
                    .objects
                    .visit_authenticated_order(&built.objects.reachable, &mut |object| {
                        if object.id == next.0 {
                            assert_eq!(object.prior_ids(), &[Some(prior), None, None, None]);
                            assert!(object.1.has_predecessor);
                            assert!(object.1.first_span.is_none());
                            assert_eq!(object.1.diagnostic & diagnostic::FILE, 0);
                        }
                        Ok(())
                    })
                    .unwrap();
                let before = db.physical_storage_receipt();
                let mut admission = CheckedOutputAdmission::new(&db).unwrap();
                admission.admit(built.objects).unwrap();
                finish_segment_admission(&db, admission);
                let physical = db.physical_storage_receipt().since(before);
                assert_eq!(physical.delta_selected, u64::from(generation == 0));
                assert_eq!(physical.diag_eligible_count, 0);
                assert_eq!(physical.diag_eligible_bytes, 0);
                assert_eq!(physical.diag_missing_span_count, 0);
                assert_eq!(
                    physical.diag_new_full_count + physical.diag_new_delta_count,
                    0
                );
                assert_eq!(physical.diag_invalid, 0);
                assert_eq!(db.read_object_row(next.0).unwrap(), expected);
                prior = next.0;
            }
            let chunk =
                layerfs_content::file::extent_codec::encode_chunk_object(b"LFS4INT\0").unwrap();
            assert!(!is_inode_table_leaf(&chunk).unwrap());
            let mut bad = encode_inode_table_node(&InodeTableNodeV1::Leaf(entries)).unwrap();
            bad.pop();
            assert!(is_inode_table_leaf(&bad).is_err());
            drop(db);
            std::fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn memory_segment_moves_owned_canonical_bytes() {
        let bytes = layerfs_content::encode_bytes_object(b"owned").unwrap();
        let id = ObjectId::for_bytes(&bytes);
        let mut segment = DeferredObjectStore::new_all_reachable().unwrap();
        segment.put(id, &bytes).unwrap();
        let original = match &segment.storage {
            DeferredObjects::Memory { rows, .. } => rows.get(&id).unwrap().bytes.as_ptr(),
            DeferredObjects::Spill(_) => panic!("small segment spilled"),
        };
        let buffer = ObjectBuffer {
            source: None,
            objects: segment,
        };
        ObjectStore::with_authenticated_canonical(&buffer, id, |bytes| {
            assert_eq!(bytes.as_ptr(), original);
            Ok(())
        })
        .unwrap();
        buffer
            .into_resumable()
            .unwrap()
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
    fn spill_index_overflow_preserves_lookup_dedup_and_cleanup_without_scans() {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(CANDIDATE_INDEX_BYTES, 64 * 1024 * 1024);
        let canonical = [b"first".as_slice(), b"second", b"third", b"fourth"]
            .map(|bytes| layerfs_content::encode_bytes_object(bytes).unwrap());
        let ids = canonical.each_ref().map(|bytes| ObjectId::for_bytes(bytes));
        let mut objects = DeferredObjectStore::new_all_reachable().unwrap();
        for index in 0..2 {
            objects.put(ids[index], &canonical[index]).unwrap();
        }
        objects.spill().unwrap();
        let DeferredObjects::Spill(spill) = &mut objects.storage else {
            panic!("forced spill");
        };
        assert_eq!(spill.index.as_ref().unwrap().len(), 2);
        assert!(spill.disk_index.is_none());
        spill.index_limit = 2 * 64; // Test-only transition; production remains64MiB.
        objects.put(ids[2], &canonical[2]).unwrap();
        let DeferredObjects::Spill(spill) = &objects.storage else {
            panic!("forced spill");
        };
        assert!(spill.index.is_none());
        assert_eq!(spill.index_bytes, 0);
        assert!(spill.pending.is_empty());
        let disk = spill.disk_index.as_ref().unwrap();
        let index_path = disk.test_path().to_path_buf();
        let payload_path = spill.path.clone();
        assert_eq!(
            std::fs::metadata(&index_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        {
            let connection = disk.test_connection();
            let rows: i64 = connection
                .query_row("SELECT count(*) FROM offsets", [], |row| row.get(0))
                .unwrap();
            assert_eq!(rows, 3);
            let cache: i64 = connection
                .pragma_query_value(None, "cache_size", |row| row.get(0))
                .unwrap();
            let mmap: i64 = connection
                .pragma_query_value(None, "mmap_size", |row| row.get(0))
                .unwrap();
            let spill: i64 = connection
                .pragma_query_value(None, "cache_spill", |row| row.get(0))
                .unwrap();
            let temp: i64 = connection
                .pragma_query_value(None, "temp_store", |row| row.get(0))
                .unwrap();
            assert_eq!((cache, mmap, temp), (-4096, 0, 1));
            assert!(spill > 0);
        }
        spill
            .reader
            .lock()
            .unwrap()
            .seek(SeekFrom::Start(0))
            .unwrap();
        let missing = ObjectId::for_bytes(b"not-present");
        assert_eq!(objects.get(missing).unwrap(), None);
        assert_eq!(
            objects.encoded_length(ids[1]).unwrap(),
            canonical[1].len() as u64
        );
        assert!(
            matches!(objects.encoded_length(missing), Err(StoreError::MissingObject(id)) if id == missing)
        );
        // Missing-ID and length queries must not touch/scan the payload spool.
        assert_eq!(spill.reader.lock().unwrap().stream_position().unwrap(), 0);
        for index in 0..3 {
            assert_eq!(
                objects.get(ids[index]).unwrap(),
                Some(canonical[index].clone())
            );
        }
        objects.put(ids[3], &canonical[3]).unwrap();
        assert_eq!(objects.get(ids[3]).unwrap(), Some(canonical[3].clone()));
        assert_eq!(
            objects.encoded_length(ids[3]).unwrap(),
            canonical[3].len() as u64
        );
        let count = objects.len();
        let written = objects.first_store_write_bytes;
        objects.put(ids[0], &canonical[0]).unwrap();
        assert_eq!(
            (objects.len(), objects.first_store_write_bytes),
            (count, written)
        );
        assert!(matches!(
            objects.put_authenticated(AuthenticatedCanonicalObject(
                CanonicalObject {
                    id: ids[0],
                    bytes: canonical[1].clone()
                },
                PhysicalHints::default()
            )),
            Err(StoreError::Integrity("candidate object collision"))
        ));
        let objects = objects.all_reachable().unwrap();
        assert!(!payload_path.exists());
        assert_eq!(objects.get(ids[3]).unwrap(), Some(canonical[3].clone()));
        drop(objects);
        assert!(!index_path.exists());
        for suffix in ["-journal", "-wal", "-shm"] {
            assert!(!PathBuf::from(format!("{}{suffix}", index_path.display())).exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn spilled_candidate_visits_selected_objects_in_graph_order() {
        let mut segment = DeferredObjectStore::new_all_reachable().unwrap();
        let mut ids = Vec::new();
        // The reversed selected order crosses the bounded read-ahead window.
        for value in [1, 2, 3] {
            let bytes = layerfs_content::encode_bytes_object(&vec![value; 40_000]).unwrap();
            let id = ObjectId::for_bytes(&bytes);
            segment.put(id, &bytes).unwrap();
            ids.push(id);
        }
        segment.spill().unwrap();
        let mut segment = segment.all_reachable().unwrap();
        if let DeferredObjects::Spill(spill) = &segment.storage {
            spill
                .reader
                .lock()
                .unwrap()
                .seek(SeekFrom::Start(7))
                .unwrap();
            let mut physical_ids = Vec::new();
            spill
                .visit_ids(&mut |id| {
                    physical_ids.push(id);
                    Ok(())
                })
                .unwrap();
            assert_eq!(physical_ids, ids);
            assert_eq!(
                spill.reader.lock().unwrap().stream_position().unwrap(),
                7,
                "bounded resident offset index needs no payload framing scan"
            );
        }
        let order = IdOrder::Memory(vec![ids[2], ids[0]]);
        let mut visited = Vec::new();
        segment
            .visit_prevalidated_order(&order, &mut |id, bytes| {
                layerfs_content::authenticate_identity(bytes, id)?;
                visited.push(id);
                Ok(())
            })
            .unwrap();
        assert_eq!(visited, vec![ids[2], ids[0]]);
        segment.reachable = order;
        let mut owned = Vec::new();
        segment
            .consume_prevalidated_pages(|page| {
                for object in &page {
                    layerfs_content::authenticate_identity(&object.bytes, object.id)?;
                }
                owned.extend(page.into_iter().map(|object| object.id));
                Ok(())
            })
            .unwrap();
        // Consumption preserves the checked child-first graph order, excluding the middle row.
        assert_eq!(owned, vec![ids[2], ids[0]]);
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
        let mut admission = CheckedOutputAdmission::new(&db).unwrap();
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

        drop(finished);
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
        for count in [0_usize, 1, 127, 128, 511, 512, 513, 8190, 8191] {
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
            let mut admission = CheckedOutputAdmission::new(&db).unwrap();
            admission
                .admit_worker_segment(sealed_segment(objects))
                .unwrap();
            let (receipt, _) = finish_segment_admission(&db, admission);
            assert_eq!(receipt.candidate_objects, count as u64);
            assert_eq!(receipt.candidate_bytes, expected_bytes);
            assert_eq!(
                receipt.max_transaction_objects,
                count.min(PHYSICAL_ADMISSION_BATCH_COUNT) as u64
            );
            assert_eq!(
                receipt.max_transaction_bytes,
                if count == 0 {
                    0
                } else {
                    (expected_bytes / count as u64)
                        * count.min(PHYSICAL_ADMISSION_BATCH_COUNT) as u64
                }
            );
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
            let mut admission = CheckedOutputAdmission::new(&db).unwrap();
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
        let mut admission = CheckedOutputAdmission::new(&db).unwrap();
        admission
            .admit_worker_segment(sealed_segment(objects))
            .unwrap();
        assert!(admission.receipt.admission_transactions > 0);
        admission
            .admit_worker_segment(sealed_segment(vec![duplicate]))
            .unwrap();
        let finished = admission.finish().unwrap();
        let mut statement_number = finished.statement_number;
        let (_, metrics) = PreparedAdmission::prepare_missing(&db, finished.final_batch)
            .unwrap()
            .publish(&db, &mut statement_number, |_, _, _| Ok(()))
            .unwrap();
        assert_eq!(finished.diagnostics.cross_batch_skipped_objects, 1);
        assert_eq!(
            finished.receipt.batch_inserted_objects + metrics.insert.objects,
            expected_objects
        );
        let forged = layerfs_content::file::extent_codec::encode_chunk_object(
            &vec![255; layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES],
        )
        .unwrap();
        assert!(AuthenticatedCanonicalObject::new(forged, Some(duplicate_id)).is_err());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shared_admission_keeps_every_object_transaction_below_the_frozen_bounds() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-bounded-admission-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        for payload_length in [8, 1000] {
            let db = crate::schema::StoreDb::create(root.join(format!("{payload_length}.sqlite")))
                .unwrap();
            let count = 2 * ADMISSION_BATCH_COUNT as u64 + 46;
            let mut admission = CheckedOutputAdmission::new(&db).unwrap();
            for index in 0..count {
                let mut random = index + 1;
                let mut payload = (0..payload_length)
                    .map(|_| {
                        random ^= random << 13;
                        random ^= random >> 7;
                        random ^= random << 17;
                        random as u8
                    })
                    .collect::<Vec<_>>();
                payload[..8].copy_from_slice(&index.to_le_bytes());
                let bytes = layerfs_content::encode_bytes_object(&payload).unwrap();
                admission
                    .admit_object(AuthenticatedCanonicalObject::new(bytes, None).unwrap())
                    .unwrap();
            }
            let (receipt, _) = finish_segment_admission(&db, admission);
            assert_eq!(receipt.candidate_objects, count);
            assert_eq!(receipt.inserted_objects, count);
            assert!(receipt.max_transaction_objects <= ADMISSION_BATCH_COUNT as u64);
            assert!(receipt.max_transaction_bytes < OBJECT_PAGE_BYTES as u64);
            drop(db);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn init_cohorts_bound_sql_commits_flush_empty_final_and_rollback_pending() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-init-cohorts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&root).unwrap();
        // The first case crosses the object cap; the second crosses the byte cap.
        for (payload_len, batches, per_batch, rollback) in
            [(8, 16, 512, true), (65_500, 9, 8, false)]
        {
            let db =
                crate::schema::StoreDb::create(root.join(format!("{payload_len}.sqlite"))).unwrap();
            let mut owner = CheckedOutputAdmission::new_for_initialization(&db).unwrap();
            let mut canonical_length = 0;
            for batch in 0..batches {
                for index in 0..per_batch {
                    let mut payload = vec![0; payload_len];
                    payload[..8]
                        .copy_from_slice(&((batch * per_batch + index) as u64).to_le_bytes());
                    let bytes = layerfs_content::encode_bytes_object(&payload).unwrap();
                    canonical_length = bytes.len();
                    owner
                        .admit_object(AuthenticatedCanonicalObject::new(bytes, None).unwrap())
                        .unwrap();
                }
                owner.flush().unwrap();
                assert!(!db.reader().unwrap().is_autocommit());
                assert_eq!(owner.checked.transactions, u64::from(batch == batches - 1));
                assert!(owner.diagnostics.batch_peak_objects <= 512);
                assert!(owner.diagnostics.batch_peak_payload_bytes <= 512 * 1024);
            }
            assert_eq!(
                owner.checked.max_transaction_objects,
                ((batches - 1) * per_batch) as u64
            );
            assert_eq!(
                owner.checked.max_transaction_bytes,
                ((batches - 1) * per_batch * canonical_length) as u64
            );
            assert!(owner.checked.max_transaction_objects <= ADMISSION_BATCH_COUNT as u64);
            assert!(owner.checked.max_transaction_bytes <= ADMISSION_BATCH_BYTES as u64);
            let session = owner.session();
            assert_eq!(session.cohort.lock().unwrap().objects, per_batch as u64);
            if rollback {
                // Undo both the currently open cohort and the earlier committed one.
                owner.abort().unwrap();
                let connection = db.reader().unwrap();
                assert!(connection.is_autocommit());
                assert_eq!(
                    connection
                        .query_row("SELECT COUNT(*) FROM objects", [], |row| row
                            .get::<_, i64>(0))
                        .unwrap(),
                    0
                );
                assert_eq!(
                    connection
                        .query_row("SELECT COUNT(*) FROM object_packs", [], |row| row
                            .get::<_, i64>(0))
                        .unwrap(),
                    0
                );
            } else {
                let finished = owner.finish().unwrap();
                assert!(finished.final_batch.is_empty());
                let called = std::cell::Cell::new(false);
                let (_, metrics) =
                    admission::PreparedAdmission::prepare_missing(&db, finished.final_batch)
                        .unwrap()
                        .publish(&db, &mut 0, |connection, _, _| {
                            assert!(!connection.is_autocommit());
                            called.set(true);
                            Ok(())
                        })
                        .unwrap();
                assert!(called.get());
                assert_eq!(metrics.insert.sql.commits, 1);
                assert_eq!(metrics.insert.sql.final_commits, 1);
                assert_eq!(metrics.insert.sql.max_objects, per_batch as u64);
                let connection = db.reader().unwrap();
                assert!(connection.is_autocommit());
                assert_eq!(
                    connection
                        .query_row("SELECT COUNT(*) FROM objects", [], |row| row
                            .get::<_, i64>(0))
                        .unwrap(),
                    (batches * per_batch) as i64
                );
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn init_cohorts_final_metadata_and_commit_failures_restore_baseline() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-cohort-final-failure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&root).unwrap();
        let object = |payload: &[u8]| {
            AuthenticatedCanonicalObject::new(
                layerfs_content::encode_bytes_object(payload).unwrap(),
                None,
            )
            .unwrap()
        };
        for commit_failure in [false, true] {
            let db = crate::schema::StoreDb::create(root.join(format!("{commit_failure}.sqlite")))
                .unwrap();
            let baseline = object(b"retained baseline");
            let mut initial = CheckedOutputAdmission::new(&db).unwrap();
            initial.admit_object(baseline.clone()).unwrap();
            finish_segment_admission(&db, initial);
            let mut owner = CheckedOutputAdmission::new_for_initialization(&db).unwrap();
            owner.admit_object(object(b"private committed")).unwrap();
            owner.flush().unwrap();
            owner.commit_pending().unwrap();
            assert_eq!(owner.checked.transactions, 1);
            let pending = object(b"private pending root");
            owner.admit_object(pending.clone()).unwrap();
            owner.flush().unwrap();
            assert!(!db.reader().unwrap().is_autocommit());
            let session = owner.session();
            let finished = owner.finish().unwrap();
            assert!(finished.final_batch.is_empty());
            let prepared = PreparedAdmission::prepare_missing(&db, finished.final_batch).unwrap();
            let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            if commit_failure {
                let fired = fired.clone();
                // Veto exactly the final COMMIT; cleanup transactions must remain usable.
                db.writer()
                    .unwrap()
                    .commit_hook(Some(move || !fired.swap(true, Ordering::SeqCst)))
                    .unwrap();
            }
            let stack = crate::LayerStackId::new();
            let layer = crate::LayerId::derive(stack, None, pending.id);
            let partial_written = std::cell::Cell::new(false);
            let result = prepared.publish(&db, &mut 0, |connection, _, _| {
                connection.execute(
                    crate::statements::layerstack::INSERT_LAYER,
                    rusqlite::params![
                        layer.as_slice(),
                        stack.as_slice(),
                        Option::<&[u8]>::None,
                        pending.id.as_bytes().as_slice(),
                        Option::<&[u8]>::None,
                        Option::<&[u8]>::None,
                    ],
                )?;
                partial_written.set(true);
                if !commit_failure {
                    return Err(StoreError::Integrity("partial final metadata"));
                }
                connection.execute(
                    crate::statements::layerstack::INSERT,
                    rusqlite::params![stack.as_slice(), "atomic-final", layer.as_slice()],
                )?;
                Ok(())
            });
            db.writer()
                .unwrap()
                .commit_hook(None::<fn() -> bool>)
                .unwrap();
            assert!(partial_written.get());
            if commit_failure {
                assert!(fired.load(Ordering::SeqCst));
                assert!(result.is_err());
            } else {
                assert!(matches!(
                    result,
                    Err(StoreError::Integrity("partial final metadata"))
                ));
            }
            assert_eq!(session.state.load(Ordering::Acquire), 2);
            {
                let connection = db.reader().unwrap();
                assert!(connection.is_autocommit());
                assert_eq!(
                    connection
                        .query_row("SELECT COUNT(*) FROM objects", [], |row| row
                            .get::<_, i64>(0))
                        .unwrap(),
                    1
                );
                assert_eq!(
                    connection
                        .query_row("SELECT COUNT(*) FROM object_packs", [], |row| row
                            .get::<_, i64>(0))
                        .unwrap(),
                    1
                );
                assert_eq!(
                    connection
                        .query_row("SELECT COUNT(*) FROM layers", [], |row| row
                            .get::<_, i64>(0))
                        .unwrap(),
                    0
                );
                assert_eq!(
                    connection
                        .query_row("SELECT COUNT(*) FROM layer_stacks", [], |row| row
                            .get::<_, i64>(0))
                        .unwrap(),
                    0
                );
            }
            assert_eq!(db.read_object_row(baseline.id).unwrap(), baseline.bytes);
            drop(session);
            let mut retry = CheckedOutputAdmission::new(&db).unwrap();
            retry.admit_object(object(b"subsequent owner")).unwrap();
            finish_segment_admission(&db, retry);
        }
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
        let order = objects.order_missing(&missing, missing.count).unwrap();
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
        .into_resumable()
        .unwrap();
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
        let objects = [
            (first_id, &first),
            (second_id, &second),
            (corrupt_id, &corrupt),
        ]
        .into_iter()
        .map(|(id, bytes)| CanonicalObject {
            id,
            bytes: bytes.clone(),
        })
        .collect();
        let mut admission = CheckedOutputAdmission::new(&db).unwrap();
        admission.admit(sealed_segment(objects)).unwrap();
        finish_segment_admission(&db, admission);
        // Admission itself must authenticate. Corruption belongs after a valid
        // fixture: point the unrelated ID at the second object's locator/length.
        db.writer().unwrap().execute(
            "UPDATE objects SET (canonical_length,pack_id,group_number,record_number) = (SELECT canonical_length,pack_id,group_number,record_number FROM objects WHERE object_id=?1) WHERE object_id=?2",
            rusqlite::params![second_id.as_bytes().as_slice(), corrupt_id.as_bytes().as_slice()],
        ).unwrap();

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
        reset_read_batch_counters();
        crate::schema::reset_sql_trace();
        assert!(db.read_object_rows(&[]).unwrap().is_empty());
        assert!(crate::schema::sql_trace().is_empty());
        let singleton = db.read_object_rows(&[first_id]).unwrap();
        assert_eq!(
            singleton,
            vec![CanonicalObject {
                id: first_id,
                bytes: first.clone()
            }]
        );
        assert_eq!(
            read_batch_counters(),
            ReadBatchCounters {
                unique_hashes: 1,
                cloned_bytes: 0
            }
        );
        let trace = crate::schema::sql_trace();
        let locator_reads = trace
            .iter()
            .filter(|sql| sql.contains("FROM objects "))
            .collect::<Vec<_>>();
        assert_eq!(locator_reads.len(), 1, "{trace:?}");
        assert!(locator_reads[0].contains("WHERE object_id ="));
        assert!(!locator_reads[0].contains(" IN "));
        let missing = ObjectId::for_bytes(b"missing");
        assert_eq!(
            db.read_object_rows(&[missing]),
            Err(StoreError::Integrity("visible object missing"))
        );
        assert_eq!(
            db.read_object_rows(&[first_id, missing]),
            Err(StoreError::Integrity("visible object cardinality"))
        );
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

#[cfg(test)]
mod ingestion_tests;
