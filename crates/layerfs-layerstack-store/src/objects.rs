use crate::{Result, StoreError};
use layerfs_content::filesystem::{self, ContentChange, ReconcileConflict};
use layerfs_content::object::access::{ObjectRead, ObjectStore};
use layerfs_content::object::references::referenced_objects;
use layerfs_content::{CoreError, CoreResult, ObjectId};
use rusqlite::{params_from_iter, types::Value, OptionalExtension};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub const OBJECT_PAGE_COUNT: usize = 128;
pub const OBJECT_PAGE_BYTES: usize = 4 * 1024 * 1024;
pub const ADMISSION_BATCH_COUNT: usize = OBJECT_PAGE_COUNT - 1;
pub const ADMISSION_BATCH_BYTES: usize = OBJECT_PAGE_BYTES - 1;
const CANDIDATE_MEMORY_BYTES: usize = 8 * 1024 * 1024;
const CANDIDATE_INDEX_BYTES: usize = 8 * 1024 * 1024;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalObject {
    pub id: ObjectId,
    pub bytes: Vec<u8>,
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

enum DeferredObjects {
    Memory {
        order: Vec<ObjectId>,
        rows: BTreeMap<ObjectId, Vec<u8>>,
        bytes: usize,
    },
    Spill(SpillObjects),
}

struct SpillObjects {
    file: Mutex<std::fs::File>,
    path: PathBuf,
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
        Ok(Self {
            storage: DeferredObjects::Memory {
                order: Vec::new(),
                rows: BTreeMap::new(),
                bytes: 0,
            },
            reachable: IdOrder::empty(),
            references: Some(BTreeMap::new()),
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

    pub fn read_prevalidated_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        if ids.len() > OBJECT_PAGE_COUNT {
            return Err(StoreError::InvalidInput("object read page"));
        }
        ids.iter()
            .map(|id| {
                let bytes = self.get(*id)?.ok_or(StoreError::MissingObject(*id))?;
                Ok(CanonicalObject { id: *id, bytes })
            })
            .collect()
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

    fn put(&mut self, id: ObjectId, canonical: &[u8]) -> Result<()> {
        if let Some(known) = self.get(id)? {
            return if known == canonical {
                Ok(())
            } else {
                Err(StoreError::Integrity("candidate object collision"))
            };
        }
        let charge = canonical.len().saturating_add(64);
        if matches!(&self.storage, DeferredObjects::Memory { bytes, .. } if bytes.saturating_add(charge) > CANDIDATE_MEMORY_BYTES)
        {
            self.spill()?;
        }
        match &mut self.storage {
            DeferredObjects::Memory { order, rows, bytes } => {
                order.push(id);
                rows.insert(id, canonical.to_vec());
                *bytes += charge;
            }
            DeferredObjects::Spill(spill) => spill.put(id, canonical)?,
        }
        self.count += 1;
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(canonical.len() as u64)
            .ok_or(StoreError::Integrity("candidate bytes"))?;
        self.first_store_write_bytes = self
            .first_store_write_bytes
            .checked_add(canonical.len() as u64)
            .ok_or(StoreError::Integrity("candidate first-store bytes"))?;
        if matches!(self.storage, DeferredObjects::Spill(_)) {
            self.spill_peak_bytes = self.spill_peak_bytes.max(self.encoded_bytes);
        }
        if self.references.is_some() {
            let mut children = referenced_objects(canonical)?;
            children.sort();
            children.dedup();
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
        let mut spill = SpillObjects {
            file: Mutex::new(file),
            path,
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
}

impl SpillObjects {
    fn seal(&mut self) -> Result<()> {
        let read_only = std::fs::File::open(&self.path)?;
        let writable = std::mem::replace(&mut self.file, Mutex::new(read_only));
        drop(writable);
        #[cfg(unix)]
        std::fs::remove_file(&self.path)?;
        Ok(())
    }

    fn put(&mut self, id: ObjectId, canonical: &[u8]) -> Result<()> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| StoreError::Integrity("candidate spool lock"))?;
        let start = file.seek(SeekFrom::End(0))?;
        file.write_all(id.as_bytes())?;
        file.write_all(&(canonical.len() as u64).to_le_bytes())?;
        file.write_all(canonical)?;
        if let Some(index) = &mut self.index {
            if self.index_bytes.saturating_add(64) > CANDIDATE_INDEX_BYTES {
                self.index = None;
                self.index_bytes = 0;
            } else {
                index.insert(id, (start + 40, canonical.len() as u64));
                self.index_bytes += 64;
            }
        }
        Ok(())
    }

    fn visit_ids(&self, visitor: &mut dyn FnMut(ObjectId) -> Result<()>) -> Result<()> {
        let mut file = self
            .file
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
            .file
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
        let mut file = self
            .file
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
            .file
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

    #[doc(hidden)]
    pub fn resume_prevalidated(source: &'a dyn ObjectSource, objects: DeferredObjectStore) -> Self {
        Self {
            source: Some(source),
            objects,
        }
    }

    #[doc(hidden)]
    pub fn into_prevalidated(self) -> DeferredObjectStore {
        self.objects
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
        self.objects.put(id, canonical).map_err(|_| CoreError::Io)?;
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
                combined.put(object.id, &object.bytes)?;
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
        let connection = self.reader()?;
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
            for (id, bytes) in batch {
                plan.candidate_objects += 1;
                plan.candidate_bytes = plan.candidate_bytes.saturating_add(*bytes);
                match known.get(id) {
                    Some(known) if known != bytes => {
                        return Err(StoreError::Integrity("object length collision"));
                    }
                    Some(_) => {
                        plan.reused_objects += 1;
                        plan.reused_bytes = plan.reused_bytes.saturating_add(*bytes);
                    }
                    None => {
                        missing.push(*id);
                        plan.inserted_objects += 1;
                        plan.inserted_bytes = plan.inserted_bytes.saturating_add(*bytes);
                    }
                }
            }
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
}

pub(crate) fn admit_planned_objects(
    db: &crate::schema::StoreDb,
    objects: &DeferredObjectStore,
    plan: &CandidatePlan,
    statement_number: &mut u64,
) -> Result<PlannedAdmission> {
    let mut batch = Vec::with_capacity(ADMISSION_BATCH_COUNT);
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
    objects.visit_prevalidated_order(&plan.missing_order, &mut |id, bytes| {
        if bytes.len() > ADMISSION_BATCH_BYTES {
            return Err(StoreError::Integrity("canonical object admission size"));
        }
        if !batch.is_empty()
            && (batch.len() == ADMISSION_BATCH_COUNT
                || batch_bytes.saturating_add(bytes.len()) > ADMISSION_BATCH_BYTES)
        {
            let metrics = insert_admission_batch(db, &batch, statement_number)?;
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
        || admission.max_transaction_objects >= OBJECT_PAGE_COUNT as u64
        || admission.max_transaction_bytes >= OBJECT_PAGE_BYTES as u64
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
) -> Result<AdmissionBatchMetrics> {
    let begin_started = Instant::now();
    let mut connection = db.writer()?;
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let begin_ns = elapsed_ns(begin_started);
    let insert = insert_object_batch(&transaction, batch, statement_number)?;
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
            objects.reachable.push(id).unwrap();
        }
        let plan = db.plan_candidate(&objects).unwrap();
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
            bytes: layerfs_content::encode_bytes_object(&u64::MAX.to_le_bytes()).unwrap(),
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
        assert!(spill.file.lock().unwrap().write_all(&[0]).is_err());
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
