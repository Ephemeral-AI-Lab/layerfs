use crate::capture::{capture_workspace, initialize_empty, live_hard_link_authority};
use crate::driver::{DriverError, ProjectionDriver};
use crate::materialize::materialize_workspace;
use crate::{NativeOperationCounters, NativeRoute, OperationCounters};
use layerfs_core::CanonicalPath;
use layerfs_core::{CoreError, ObjectId};
use layerfs_engine::refs::RefState;
use layerfs_engine::scratch::DiskTable;
use layerfs_engine::{Engine, EngineError};
use std::collections::HashMap;
use std::fmt;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum VfsError {
    Core(CoreError),
    Engine(EngineError),
    Driver(DriverError),
    Io(std::io::Error),
    WorkspaceBusy,
    ExternalDirtyConflict,
    ExternalHardLinkBoundary,
    NativeProtected,
    CommittedCleanup {
        root: ObjectId,
        error: Box<VfsError>,
    },
    InvalidState,
}
pub type VfsResult<T> = Result<T, VfsError>;
impl From<CoreError> for VfsError {
    fn from(value: CoreError) -> Self {
        Self::Core(value)
    }
}
impl From<EngineError> for VfsError {
    fn from(value: EngineError) -> Self {
        Self::Engine(value)
    }
}
impl From<DriverError> for VfsError {
    fn from(value: DriverError) -> Self {
        if matches!(value, DriverError::NativeProtected) {
            Self::NativeProtected
        } else {
            Self::Driver(value)
        }
    }
}
impl From<std::io::Error> for VfsError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for VfsError {}

pub struct LayerVfs {
    engine: Arc<Engine>,
    driver: Arc<dyn ProjectionDriver>,
    head: RefState,
    operation_q: Arc<OperationQ>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationQObservation {
    pub current_bytes: u64,
    pub high_water_bytes: u64,
}

#[derive(Default)]
struct OperationQ {
    current: AtomicU64,
    high_water: AtomicU64,
}

impl OperationQ {
    fn reserve(self: &Arc<Self>) -> OperationReservation {
        let current = self
            .current
            .fetch_add(crate::OPERATION_Q_BOUND_BYTES, Ordering::AcqRel)
            + crate::OPERATION_Q_BOUND_BYTES;
        self.high_water.fetch_max(current, Ordering::AcqRel);
        OperationReservation(self.clone())
    }

    fn observation(&self) -> OperationQObservation {
        OperationQObservation {
            current_bytes: self.current.load(Ordering::Acquire),
            high_water_bytes: self.high_water.load(Ordering::Acquire),
        }
    }
}

struct OperationReservation(Arc<OperationQ>);

impl Drop for OperationReservation {
    fn drop(&mut self) {
        self.0
            .current
            .fetch_sub(crate::OPERATION_Q_BOUND_BYTES, Ordering::AcqRel);
    }
}

#[derive(Default)]
struct LeaseState {
    writers: usize,
    capturing: bool,
    terminal: bool,
}
type SharedLeaseState = Arc<Mutex<LeaseState>>;
type WriterStateRegistry = Mutex<HashMap<Vec<u8>, Weak<Mutex<LeaseState>>>>;

static WRITER_STATES: OnceLock<WriterStateRegistry> = OnceLock::new();

fn shared_writers(identity: &[u8]) -> VfsResult<SharedLeaseState> {
    let states = WRITER_STATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut states = states.lock().map_err(|_| VfsError::InvalidState)?;
    states.retain(|_, state| state.strong_count() != 0);
    if let Some(state) = states.get(identity).and_then(Weak::upgrade) {
        return Ok(state);
    }
    let state = Arc::new(Mutex::new(LeaseState::default()));
    states.insert(identity.to_vec(), Arc::downgrade(&state));
    Ok(state)
}

impl LayerVfs {
    pub fn open(store: &Path, driver: Arc<dyn ProjectionDriver>) -> VfsResult<Self> {
        Self::from_engine(Engine::open(store)?, driver)
    }
    pub fn from_engine(engine: Engine, driver: Arc<dyn ProjectionDriver>) -> VfsResult<Self> {
        let engine = Arc::new(engine);
        driver.recover_owned_workspaces(
            engine.path().parent().unwrap_or_else(|| Path::new(".")),
            engine.store_id()?,
        )?;
        let head = match engine.read_ref("main")? {
            Some(head) => head,
            None => initialize_empty(&engine)?,
        };
        Ok(Self {
            engine,
            driver,
            head,
            operation_q: Arc::new(OperationQ::default()),
        })
    }
    pub fn into_engine(self) -> VfsResult<Engine> {
        Arc::try_unwrap(self.engine).map_err(|_| VfsError::WorkspaceBusy)
    }
    pub fn head(&self) -> &RefState {
        &self.head
    }
    pub fn profile(&self) -> &layerfs_engine::SqliteProfile {
        self.engine.profile()
    }
    pub fn observations(&self) -> layerfs_engine::StorageObservation {
        self.engine.observations()
    }
    pub fn last_compaction_observation(
        &self,
    ) -> Option<layerfs_engine::CompactionStorageObservation> {
        self.engine.last_compaction_observation()
    }
    pub fn counters(&self) -> VfsResult<layerfs_engine::EngineCounters> {
        Ok(self.engine.counters()?)
    }
    pub fn active_connection_count(&self) -> VfsResult<u64> {
        Ok(self.engine.active_connection_count()?)
    }
    pub fn operation_q_observation(&self) -> OperationQObservation {
        self.operation_q.observation()
    }
    pub fn materialize_external(
        &self,
        root: ObjectId,
        path: &Path,
    ) -> VfsResult<ExternalWorkspace> {
        self.materialize_external_observed(root, path)
            .map(|(workspace, _)| workspace)
    }
    pub fn materialize_external_observed(
        &self,
        root: ObjectId,
        path: &Path,
    ) -> VfsResult<(ExternalWorkspace, OperationCounters)> {
        let _reservation = self.operation_q.reserve();
        let native = self.driver.open_workspace(
            path,
            crate::driver::WorkspacePolicy::ExternalCooperative,
            self.engine.store_id()?,
        )?;
        let mut counters = materialize_workspace(&self.engine, native.as_ref(), root)?;
        let (hard_link_authority, authority_counters) =
            live_hard_link_authority(&self.engine, native.as_ref(), root)?;
        counters.add_rope(authority_counters.rope)?;
        counters.add_namespace(authority_counters.namespace)?;
        counters.add_inode_table(authority_counters.inode_table)?;
        counters.add_native(authority_counters.native)?;
        let native_root = native.root_directory()?;
        let identity = native.directory_identity(native_root.as_ref())?;
        let writers = shared_writers(&identity)?;
        Ok((
            ExternalWorkspace {
                engine: self.engine.clone(),
                native,
                path: path.to_owned(),
                expected: self
                    .engine
                    .read_ref("main")?
                    .ok_or(VfsError::InvalidState)?,
                writers,
                owned: false,
                owned_identity: None,
                hard_link_authority: Some(hard_link_authority),
                active: true,
                committed: None,
                operation_q: self.operation_q.clone(),
            },
            counters,
        ))
    }
    pub fn open_external(&self, path: &Path) -> VfsResult<ExternalWorkspace> {
        let _reservation = self.operation_q.reserve();
        let native = self.driver.open_workspace(
            path,
            crate::driver::WorkspacePolicy::ExternalCooperative,
            self.engine.store_id()?,
        )?;
        let native_root = native.root_directory()?;
        let identity = native.directory_identity(native_root.as_ref())?;
        let writers = shared_writers(&identity)?;
        Ok(ExternalWorkspace {
            engine: self.engine.clone(),
            native,
            path: path.to_owned(),
            expected: self
                .engine
                .read_ref("main")?
                .ok_or(VfsError::InvalidState)?,
            writers,
            owned: false,
            owned_identity: None,
            hard_link_authority: None,
            active: true,
            committed: None,
            operation_q: self.operation_q.clone(),
        })
    }
    pub fn materialize_managed(&self, root: ObjectId) -> VfsResult<ManagedWorkspace> {
        let _reservation = self.operation_q.reserve();
        let expected = self
            .engine
            .read_ref("main")?
            .ok_or(VfsError::InvalidState)?;
        if root != expected.root {
            return Err(VfsError::ExternalDirtyConflict);
        }
        let parent = self
            .engine
            .path()
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| VfsError::InvalidState)?
            .as_nanos();
        let path = parent.join(format!(".layerfs-managed-{}-{stamp}", std::process::id()));
        let native = self.driver.open_workspace(
            &path,
            crate::driver::WorkspacePolicy::ManagedCreateOwned,
            self.engine.store_id()?,
        )?;
        let native_root = native.root_directory()?;
        let owned_identity = native.directory_identity(native_root.as_ref())?;
        let setup = (|| {
            materialize_workspace(&self.engine, native.as_ref(), root)?;
            let spool = native.create_temp_at(native_root.as_ref())?;
            let writers = shared_writers(&owned_identity)?;
            let (hard_link_authority, _) =
                live_hard_link_authority(&self.engine, native.as_ref(), root)?;
            Ok::<_, VfsError>((spool, writers, hard_link_authority))
        })();
        let (spool, writers, hard_link_authority) = match setup {
            Ok(setup) => setup,
            Err(error) => {
                let _ = native.remove_owned_root(&owned_identity);
                return Err(error);
            }
        };
        Ok(ManagedWorkspace {
            external: Some(ExternalWorkspace {
                engine: self.engine.clone(),
                native,
                path,
                expected,
                writers,
                owned: true,
                owned_identity: Some(owned_identity),
                hard_link_authority: Some(hard_link_authority),
                active: true,
                committed: None,
                operation_q: self.operation_q.clone(),
            }),
            edits: Vec::new(),
            dirty: false,
            committed: None,
            spool: Some(spool),
        })
    }
    pub fn fork(&self, root: ObjectId, name: &str) -> VfsResult<ObjectId> {
        let source = RefState {
            name: "retained-source".to_owned(),
            generation: 0,
            root,
        };
        Ok(self.engine.fork_ref(&source, name)?.root)
    }
    pub fn rollback(&self, target: ObjectId) -> VfsResult<ObjectId> {
        let expected = self
            .engine
            .read_ref("main")?
            .ok_or(VfsError::InvalidState)?;
        Ok(self.engine.move_ref(&expected, target)?.root)
    }
}

pub struct ManagedWorkspace {
    external: Option<ExternalWorkspace>,
    edits: Vec<crate::managed_edit::ManagedEdit>,
    dirty: bool,
    committed: Option<ObjectId>,
    spool: Option<Box<dyn crate::driver::OwnedTempHandle>>,
}
impl ManagedWorkspace {
    pub fn capture(&mut self) -> VfsResult<ObjectId> {
        self.capture_observed().map(|(root, _)| root)
    }
    pub fn capture_observed(&mut self) -> VfsResult<(ObjectId, OperationCounters)> {
        let _reservation = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .reserve();
        let (root, counters) = if let Some(root) = self.committed {
            (root, OperationCounters::default())
        } else {
            let external = self.external.as_mut().ok_or(VfsError::InvalidState)?;
            if self.dirty {
                return Err(VfsError::ExternalDirtyConflict);
            }
            let root = if self.edits.is_empty() {
                (external.expected.root, OperationCounters::default())
            } else {
                let spool = self.spool.as_mut().ok_or(VfsError::InvalidState)?;
                if let Err(error) = spool.flush() {
                    self.dirty = true;
                    return Err(error.into());
                }
                let (next, counters) = match crate::managed_edit::replay(
                    &external.engine,
                    &external.expected,
                    &self.edits,
                    spool.as_mut(),
                ) {
                    Ok(next) => next,
                    Err(error) => {
                        self.dirty = true;
                        return Err(error);
                    }
                };
                external.expected = next.clone();
                (next.root, counters)
            };
            self.edits.clear();
            self.dirty = false;
            self.committed = Some(root.0);
            (root.0, root.1)
        };
        if let Err(error) = self
            .external
            .as_mut()
            .ok_or(VfsError::InvalidState)?
            .discard_inner()
        {
            return Err(VfsError::CommittedCleanup {
                root,
                error: Box::new(error),
            });
        }
        if let Err(error) = self.remove_spool() {
            return Err(VfsError::CommittedCleanup {
                root,
                error: Box::new(error),
            });
        }
        self.external.take();
        self.committed.take();
        Ok((root, counters))
    }
    pub fn replace(
        &mut self,
        path: &CanonicalPath,
        start: u64,
        delete_len: u64,
        bytes: &[u8],
    ) -> VfsResult<()> {
        self.replace_observed(path, start, delete_len, bytes)
            .map(drop)
    }
    pub fn replace_observed(
        &mut self,
        path: &CanonicalPath,
        start: u64,
        delete_len: u64,
        bytes: &[u8],
    ) -> VfsResult<OperationCounters> {
        let _reservation = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .reserve();
        if self.edits.len() == 64 {
            return Err(VfsError::InvalidState);
        }
        if self.dirty {
            return Err(VfsError::InvalidState);
        }
        let external = self.external.as_mut().ok_or(VfsError::InvalidState)?;
        let spool = self.spool.as_mut().ok_or(VfsError::InvalidState)?;
        let spool_offset = spool.seek(SeekFrom::End(0))?;
        spool.write_all(bytes)?;
        let old_hard_link_key =
            match crate::managed_edit::native_hard_link_key(external.native.as_ref(), path) {
                Ok(key) => key,
                Err(error) => {
                    self.dirty = true;
                    return Err(error);
                }
            };
        let (metadata, native_counters) = match crate::managed_edit::mutate_native(
            external.native.as_ref(),
            path,
            start,
            delete_len,
            bytes,
        ) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.dirty = true;
                let _ = spool.seek(SeekFrom::Start(spool_offset));
                return Err(error);
            }
        };
        let new_hard_link_key =
            match crate::managed_edit::native_hard_link_key(external.native.as_ref(), path) {
                Ok(key) => key,
                Err(error) => {
                    external.hard_link_authority = None;
                    self.dirty = true;
                    return Err(error);
                }
            };
        if old_hard_link_key != new_hard_link_key {
            let transfer = external
                .hard_link_authority
                .as_ref()
                .ok_or(VfsError::InvalidState)
                .and_then(|authority| {
                    let inode = authority
                        .get(&old_hard_link_key)?
                        .ok_or(VfsError::InvalidState)?;
                    authority.put(&new_hard_link_key, &inode)?;
                    authority.remove(&old_hard_link_key)?;
                    Ok(())
                });
            if let Err(error) = transfer {
                external.hard_link_authority = None;
                self.dirty = true;
                return Err(error);
            }
        }
        let (metadata_offset, metadata_len) =
            match crate::managed_edit::spool_metadata(spool.as_mut(), &metadata) {
                Ok(evidence) => evidence,
                Err(error) => {
                    self.dirty = true;
                    return Err(error);
                }
            };
        self.edits.push(crate::managed_edit::ManagedEdit::Replace {
            path: path.clone(),
            start,
            delete_len,
            spool_offset,
            replacement_len: u64::try_from(bytes.len()).map_err(|_| VfsError::InvalidState)?,
            metadata_offset,
            metadata_len,
        });
        Ok(OperationCounters {
            native: native_counters,
            ..OperationCounters::default()
        })
    }
    pub fn replace_path(
        &mut self,
        path: &str,
        start: u64,
        delete_len: u64,
        bytes: &[u8],
    ) -> VfsResult<()> {
        self.replace(&CanonicalPath::new(path)?, start, delete_len, bytes)
    }
    pub fn replace_path_observed(
        &mut self,
        path: &str,
        start: u64,
        delete_len: u64,
        bytes: &[u8],
    ) -> VfsResult<OperationCounters> {
        self.replace_observed(&CanonicalPath::new(path)?, start, delete_len, bytes)
    }
    pub fn rename(&mut self, from: &CanonicalPath, to: &CanonicalPath) -> VfsResult<()> {
        self.rename_observed(from, to).map(drop)
    }
    pub fn rename_observed(
        &mut self,
        from: &CanonicalPath,
        to: &CanonicalPath,
    ) -> VfsResult<OperationCounters> {
        let _reservation = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .reserve();
        if self.edits.len() == 64 {
            return Err(VfsError::InvalidState);
        }
        if self.dirty {
            return Err(VfsError::InvalidState);
        }
        let external = self.external.as_ref().ok_or(VfsError::InvalidState)?;
        let result = crate::managed_edit::rename_native(external.native.as_ref(), from, to);
        match result {
            Ok((source_parent_metadata, target_parent_metadata)) => {
                let spool = self.spool.as_mut().ok_or(VfsError::InvalidState)?;
                let (source_metadata_offset, source_metadata_len) =
                    match crate::managed_edit::spool_metadata(
                        spool.as_mut(),
                        &source_parent_metadata,
                    ) {
                        Ok(evidence) => evidence,
                        Err(error) => {
                            self.dirty = true;
                            return Err(error);
                        }
                    };
                let (target_metadata_offset, target_metadata_len) =
                    match crate::managed_edit::spool_metadata(
                        spool.as_mut(),
                        &target_parent_metadata,
                    ) {
                        Ok(evidence) => evidence,
                        Err(error) => {
                            self.dirty = true;
                            return Err(error);
                        }
                    };
                self.edits.push(crate::managed_edit::ManagedEdit::Rename {
                    from: from.clone(),
                    to: to.clone(),
                    source_metadata_offset,
                    source_metadata_len,
                    target_metadata_offset,
                    target_metadata_len,
                });
                Ok(OperationCounters {
                    native: NativeOperationCounters {
                        route: Some(NativeRoute::Rename),
                        ..NativeOperationCounters::default()
                    },
                    ..OperationCounters::default()
                })
            }
            Err(error @ VfsError::NativeProtected) => Err(error),
            Err(error) => {
                self.dirty = true;
                Err(error)
            }
        }
    }
    pub fn rename_path(&mut self, from: &str, to: &str) -> VfsResult<()> {
        self.rename(&CanonicalPath::new(from)?, &CanonicalPath::new(to)?)
    }
    pub fn rename_path_observed(&mut self, from: &str, to: &str) -> VfsResult<OperationCounters> {
        self.rename_observed(&CanonicalPath::new(from)?, &CanonicalPath::new(to)?)
    }
    pub fn into_external(mut self) -> VfsResult<ExternalWorkspace> {
        let _reservation = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .reserve();
        self.remove_spool()?;
        self.external.take().ok_or(VfsError::InvalidState)
    }
    pub fn discard(&mut self) -> VfsResult<()> {
        let _reservation = self
            .external
            .as_ref()
            .map(|external| external.operation_q.reserve());
        if let Some(mut external) = self.external.take() {
            if let Err(error) = external.discard_inner() {
                self.external = Some(external);
                return Err(error);
            }
        }
        self.remove_spool()
    }
    fn remove_spool(&mut self) -> VfsResult<()> {
        self.spool.take();
        Ok(())
    }
}

impl Drop for ManagedWorkspace {
    fn drop(&mut self) {
        if let Some(external) = self.external.as_mut() {
            let _ = external.discard();
        }
        let _ = self.remove_spool();
    }
}

pub struct ExternalWorkspace {
    engine: Arc<Engine>,
    native: Box<dyn crate::driver::ProjectionWorkspace>,
    path: PathBuf,
    expected: RefState,
    writers: SharedLeaseState,
    owned: bool,
    owned_identity: Option<Vec<u8>>,
    hard_link_authority: Option<DiskTable>,
    active: bool,
    committed: Option<ObjectId>,
    operation_q: Arc<OperationQ>,
}
impl ExternalWorkspace {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn capture_quiescent(&mut self) -> VfsResult<ObjectId> {
        self.capture_quiescent_observed().map(|(root, _)| root)
    }
    pub fn capture_quiescent_observed(&mut self) -> VfsResult<(ObjectId, OperationCounters)> {
        let _reservation = self.operation_q.reserve();
        if !self.active {
            return Err(VfsError::InvalidState);
        }
        let mut capture = CaptureLease::begin(self.writers.clone())?;
        let (root, counters) = if let Some(root) = self.committed {
            (root, OperationCounters::default())
        } else {
            let (next, counters) = capture_workspace(
                &self.engine,
                self.native.as_ref(),
                Some(&self.expected),
                self.hard_link_authority.as_ref(),
                false,
                false,
            )?;
            self.expected = next.clone();
            self.committed = Some(next.root);
            (next.root, counters)
        };
        if self.owned {
            if let Err(error) = self.native.remove_owned_root(
                self.owned_identity
                    .as_deref()
                    .ok_or(VfsError::InvalidState)?,
            ) {
                return Err(VfsError::CommittedCleanup {
                    root,
                    error: Box::new(error.into()),
                });
            }
        }
        self.active = false;
        self.committed = None;
        capture.finish()?;
        Ok((root, counters))
    }
    pub fn discard(&mut self) -> VfsResult<()> {
        let _reservation = self.operation_q.reserve();
        self.discard_inner()
    }
    fn discard_inner(&mut self) -> VfsResult<()> {
        let mut capture = CaptureLease::begin(self.writers.clone())?;
        if self.active && self.owned {
            self.native.remove_owned_root(
                self.owned_identity
                    .as_deref()
                    .ok_or(VfsError::InvalidState)?,
            )?;
        }
        self.active = false;
        self.committed = None;
        capture.finish()?;
        Ok(())
    }
    pub fn register_writer(&self) -> VfsResult<WriterLease> {
        if !self.active {
            return Err(VfsError::InvalidState);
        }
        WriterLease::begin(self.writers.clone())
    }
}

struct CaptureLease {
    state: SharedLeaseState,
    active: bool,
}

impl CaptureLease {
    fn begin(state: SharedLeaseState) -> VfsResult<Self> {
        {
            let mut value = state.lock().map_err(|_| VfsError::InvalidState)?;
            if value.terminal {
                return Err(VfsError::InvalidState);
            }
            if value.capturing || value.writers != 0 {
                return Err(VfsError::WorkspaceBusy);
            }
            value.capturing = true;
        }
        Ok(Self {
            state,
            active: true,
        })
    }
    fn finish(&mut self) -> VfsResult<()> {
        let mut state = self.state.lock().map_err(|_| VfsError::InvalidState)?;
        state.capturing = false;
        state.terminal = true;
        self.active = false;
        Ok(())
    }
}

impl Drop for CaptureLease {
    fn drop(&mut self) {
        if self.active {
            if let Ok(mut state) = self.state.lock() {
                state.capturing = false;
            }
        }
    }
}

pub struct WriterLease(SharedLeaseState);
impl WriterLease {
    fn begin(state: SharedLeaseState) -> VfsResult<Self> {
        {
            let mut value = state.lock().map_err(|_| VfsError::InvalidState)?;
            if value.capturing || value.terminal {
                return Err(VfsError::WorkspaceBusy);
            }
            value.writers = value.writers.checked_add(1).ok_or(VfsError::InvalidState)?;
        }
        Ok(Self(state))
    }
}
impl Drop for WriterLease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.lock() {
            state.writers = state.writers.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod lease_tests {
    use super::*;

    #[test]
    fn writer_and_capture_admission_are_one_atomic_state_transition() {
        let state = Arc::new(Mutex::new(LeaseState::default()));
        let writer = WriterLease::begin(state.clone()).unwrap();
        assert!(matches!(
            CaptureLease::begin(state.clone()),
            Err(VfsError::WorkspaceBusy)
        ));
        drop(writer);
        let mut capture = CaptureLease::begin(state.clone()).unwrap();
        assert!(matches!(
            WriterLease::begin(state.clone()),
            Err(VfsError::WorkspaceBusy)
        ));
        capture.finish().unwrap();
        assert!(matches!(
            WriterLease::begin(state),
            Err(VfsError::WorkspaceBusy)
        ));
    }
}
