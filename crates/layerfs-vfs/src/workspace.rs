use crate::capture::{capture_workspace, initialize_empty, SemanticDigestCache};
use crate::driver::{DriverError, ProjectionDriver};
use crate::materialize::materialize_workspace;
use crate::{NativeOperationCounters, NativeRoute, OperationCounters};
use layerfs_core::inode::InodeId;
use layerfs_core::CanonicalPath;
use layerfs_core::{CoreError, ObjectId};
use layerfs_engine::refs::RefState;
use layerfs_engine::scratch::DiskTable;
use layerfs_engine::{Engine, EngineError};
use std::collections::HashMap;
use std::fmt;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

type ManagedRootAdmission = (
    Box<dyn crate::driver::ProjectionWorkspace>,
    Box<dyn crate::driver::DirectoryHandle>,
    Vec<u8>,
);

pub(crate) fn admit_managed_root(
    native: Box<dyn crate::driver::ProjectionWorkspace>,
) -> VfsResult<ManagedRootAdmission> {
    let native_root = match native.root_directory() {
        Ok(root) => root,
        Err(error) => {
            return match native.discard_owned_root() {
                Ok(()) => Err(error.into()),
                Err(cleanup) => Err(cleanup.into()),
            }
        }
    };
    let owned_identity = match native.directory_identity(native_root.as_ref()) {
        Ok(identity) => identity,
        Err(error) => {
            drop(native_root);
            return match native.discard_owned_root() {
                Ok(()) => Err(error.into()),
                Err(cleanup) => Err(cleanup.into()),
            };
        }
    };
    Ok((native, native_root, owned_identity))
}

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
    Indeterminate,
    IncompleteDerived,
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
    pub(crate) engine: Arc<Engine>,
    driver: Arc<dyn ProjectionDriver>,
    head: RefState,
    pub(crate) operation_q: Arc<OperationQ>,
    pub(crate) resolved_read_cache: crate::resolver::ResolvedReadCache,
    digest_cache: Arc<SemanticDigestCache>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationQObservation {
    pub current_bytes: u64,
    pub high_water_bytes: u64,
}

#[derive(Default)]
pub(crate) struct OperationQ {
    current: AtomicU64,
    high_water: AtomicU64,
}

impl OperationQ {
    pub(crate) fn reserve(self: &Arc<Self>) -> OperationReservation {
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

pub(crate) struct OperationReservation(Arc<OperationQ>);

impl OperationReservation {
    pub(crate) fn finish(self, counters: &mut OperationCounters) {
        let queue = self.0.clone();
        let active = queue.observation();
        counters.operation_q_current_bytes = active.current_bytes;
        counters.operation_q_high_water_bytes = active.high_water_bytes;
        drop(self);
        counters.operation_q_terminal_bytes = queue.observation().current_bytes;
    }
}

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

enum SpoolPart<'a> {
    Bytes(&'a [u8]),
    Metadata(&'a crate::driver::NativeMetadata),
}

pub(crate) fn topology_edge_key(child: InodeId, parent: InodeId, name: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(64 + name.len());
    key.extend_from_slice(child.as_bytes());
    key.extend_from_slice(parent.as_bytes());
    key.extend_from_slice(name);
    key
}

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
            resolved_read_cache: crate::resolver::ResolvedReadCache::default(),
            digest_cache: Arc::new(SemanticDigestCache::default()),
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
    pub fn store_id(&self) -> VfsResult<[u8; 32]> {
        Ok(self.engine.store_id()?)
    }
    pub fn active_connection_count(&self) -> VfsResult<u64> {
        Ok(self.engine.active_connection_count()?)
    }
    pub fn operation_q_observation(&self) -> OperationQObservation {
        self.operation_q.observation()
    }
    /// Cumulative native projection facts remain available after a failed open
    /// or after the returned workspace has been dropped.
    pub fn projection_facts(&self) -> crate::driver::ProjectionFacts {
        self.driver.projection_facts()
    }
    pub(crate) fn projection_driver(&self) -> &dyn ProjectionDriver {
        self.driver.as_ref()
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
        let materialize_started = Instant::now();
        let reservation = self.operation_q.reserve();
        let projection_before = self.driver.projection_facts();
        let expected = self
            .engine
            .read_ref("main")?
            .ok_or(VfsError::InvalidState)?;
        let base_matches_expected = root == expected.root;
        let native = self.driver.open_workspace(
            path,
            crate::driver::WorkspacePolicy::ExternalCooperative,
            self.engine.store_id()?,
        )?;
        let (mut counters, live_scratch) =
            materialize_workspace(&self.engine, &self.digest_cache, native.as_ref(), root)?;
        let native_root = native.root_directory()?;
        let identity = native.directory_identity(native_root.as_ref())?;
        let writers = shared_writers(&identity)?;
        counters.projection = self
            .driver
            .projection_facts()
            .checked_delta(projection_before)
            .ok_or(VfsError::InvalidState)?;
        reservation.finish(&mut counters);
        let workspace = ExternalWorkspace {
            engine: self.engine.clone(),
            native,
            path: path.to_owned(),
            expected,
            base_matches_expected,
            writers,
            owned: false,
            owned_identity: None,
            live_scratch: Some(live_scratch),
            active: true,
            committed: None,
            operation_q: self.operation_q.clone(),
            digest_cache: self.digest_cache.clone(),
        };
        counters.materialize_inclusive_ns = u64::try_from(materialize_started.elapsed().as_nanos())
            .map_err(|_| VfsError::InvalidState)?;
        Ok((workspace, counters))
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
            base_matches_expected: true,
            writers,
            owned: false,
            owned_identity: None,
            live_scratch: None,
            active: true,
            committed: None,
            operation_q: self.operation_q.clone(),
            digest_cache: self.digest_cache.clone(),
        })
    }
    pub fn materialize_managed(&self, root: ObjectId) -> VfsResult<ManagedWorkspace> {
        self.materialize_managed_observed(root)
            .map(|(workspace, _)| workspace)
    }
    pub fn materialize_managed_observed(
        &self,
        root: ObjectId,
    ) -> VfsResult<(ManagedWorkspace, OperationCounters)> {
        let reservation = self.operation_q.reserve();
        let projection_before = self.driver.projection_facts();
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
        let (native, native_root, owned_identity) = admit_managed_root(native)?;
        let setup = (|| {
            let (mut counters, live_scratch) =
                materialize_workspace(&self.engine, &self.digest_cache, native.as_ref(), root)?;
            counters.workspace_materializations = 1;
            let spool = native.create_temp_at(native_root.as_ref())?;
            let writers = shared_writers(&owned_identity)?;
            Ok::<_, VfsError>((spool, writers, live_scratch, counters))
        })();
        let (spool, writers, live_scratch, mut counters) = match setup {
            Ok(setup) => setup,
            Err(error) => {
                let _ = native.remove_owned_root(&owned_identity);
                return Err(error);
            }
        };
        let mut workspace = ManagedWorkspace {
            external: Some(ExternalWorkspace {
                engine: self.engine.clone(),
                native,
                path,
                expected,
                base_matches_expected: true,
                writers,
                owned: true,
                owned_identity: Some(owned_identity),
                live_scratch: Some(live_scratch),
                active: true,
                committed: None,
                operation_q: self.operation_q.clone(),
                digest_cache: self.digest_cache.clone(),
            }),
            edits: Vec::new(),
            state: ManagedState::Live,
            spool: Some(spool),
        };
        workspace.observe_spool(&mut counters)?;
        counters.projection = self
            .driver
            .projection_facts()
            .checked_delta(projection_before)
            .ok_or(VfsError::InvalidState)?;
        reservation.finish(&mut counters);
        Ok((workspace, counters))
    }
    pub fn fork(&self, root: ObjectId, name: &str) -> VfsResult<ObjectId> {
        let source = RefState {
            name: "retained-source".to_owned(),
            generation: 0,
            root,
        };
        Ok(self.engine.fork_ref(&source, name)?.root)
    }
    pub fn rollback(&self, expected: &RefState, target: ObjectId) -> VfsResult<RefState> {
        self.move_main(expected, target)
    }
    pub fn move_main(&self, expected: &RefState, target: ObjectId) -> VfsResult<RefState> {
        if expected.name != "main" {
            return Err(VfsError::InvalidState);
        }
        self.engine
            .move_ref(expected, target)
            .map_err(|error| match error {
                EngineError::PublicationConflict => VfsError::ExternalDirtyConflict,
                error => error.into(),
            })
    }
}

pub struct ManagedWorkspace {
    external: Option<ExternalWorkspace>,
    edits: Vec<crate::managed_edit::ManagedEdit>,
    state: ManagedState,
    spool: Option<Box<dyn crate::driver::OwnedTempHandle>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManagedState {
    Live,
    Dirty,
    Refreshing,
    ExternalDirtyConflict,
    Indeterminate,
    IncompleteDerived,
    Closed,
}
impl ManagedWorkspace {
    pub fn read_metadata(&self, path: &CanonicalPath) -> VfsResult<crate::driver::NativeMetadata> {
        self.require_editable()?;
        let external = self.external.as_ref().ok_or(VfsError::InvalidState)?;
        let root = external.native.root_directory()?;
        let (parent, name) =
            crate::managed_edit::native_parent(external.native.as_ref(), root, path)?;
        let token = external.native.token_at(parent.as_ref(), name)?;
        Ok(external
            .native
            .read_metadata_at(parent.as_ref(), name, Some(&token))?)
    }
    pub fn read_to<W: Write>(
        &self,
        path: &CanonicalPath,
        mut output: W,
    ) -> VfsResult<OperationCounters> {
        self.require_editable()?;
        let reservation = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .reserve();
        let external = self.external.as_ref().ok_or(VfsError::InvalidState)?;
        let root = external.native.root_directory()?;
        let (parent, name) =
            crate::managed_edit::native_parent(external.native.as_ref(), root, path)?;
        let token = external.native.token_at(parent.as_ref(), name)?;
        let mut file = external
            .native
            .open_regular_read_at(parent.as_ref(), name, Some(&token))?;
        let bytes = std::io::copy(&mut file, &mut output)?;
        let mut counters = OperationCounters {
            native: NativeOperationCounters {
                bytes_read: bytes,
                ..NativeOperationCounters::default()
            },
            workspace_reuses: 1,
            ..OperationCounters::default()
        };
        reservation.finish(&mut counters);
        Ok(counters)
    }
    pub fn capture(&mut self) -> VfsResult<ObjectId> {
        self.capture_observed().map(|(root, _)| root)
    }
    pub fn capture_observed(&mut self) -> VfsResult<(ObjectId, OperationCounters)> {
        let (state, mut counters) = self.checkpoint_observed()?;
        let root = state.root;
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
        self.state = ManagedState::Closed;
        self.observe_spool(&mut counters)?;
        Ok((root, counters))
    }
    pub fn checkpoint(&mut self) -> VfsResult<RefState> {
        self.checkpoint_observed().map(|value| value.0)
    }
    pub fn checkpoint_observed(&mut self) -> VfsResult<(RefState, OperationCounters)> {
        self.checkpoint_observed_inner(false)
            .map(|(state, counters, _)| (state, counters))
    }
    pub fn checkpoint_observed_detailed(
        &mut self,
    ) -> VfsResult<(RefState, OperationCounters, Vec<crate::ManagedReplayStep>)> {
        self.checkpoint_observed_inner(true)
    }
    fn checkpoint_observed_inner(
        &mut self,
        collect_steps: bool,
    ) -> VfsResult<(RefState, OperationCounters, Vec<crate::ManagedReplayStep>)> {
        self.require_checkpointable()?;
        let reservation = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .reserve();
        let external = self.external.as_mut().ok_or(VfsError::InvalidState)?;
        if self.state == ManagedState::Live {
            if external.native.revalidate_root_binding().is_err() {
                self.state = ManagedState::ExternalDirtyConflict;
                return Err(VfsError::ExternalDirtyConflict);
            }
            let state = external.expected.clone();
            let mut counters = OperationCounters {
                workspace_reuses: 1,
                ..OperationCounters::default()
            };
            self.observe_spool(&mut counters)?;
            reservation.finish(&mut counters);
            return Ok((state, counters, Vec::new()));
        }
        crate::managed_edit::sync_pending(external.native.as_ref(), &self.edits)?;
        let root = external.native.root_directory()?;
        let next_spool = external.native.create_temp_at(root.as_ref())?;
        let replay = {
            let spool = self.spool.as_mut().ok_or(VfsError::InvalidState)?;
            spool.flush()?;
            crate::managed_edit::replay(
                &external.engine,
                &external.expected,
                &self.edits,
                spool.as_mut(),
                collect_steps,
            )
        };
        let (next, mut counters, steps) = match replay {
            Ok(next) => next,
            Err(VfsError::ExternalDirtyConflict) => {
                self.state = ManagedState::ExternalDirtyConflict;
                return Err(VfsError::ExternalDirtyConflict);
            }
            Err(error @ VfsError::Engine(EngineError::AmbiguousDurability)) => {
                self.state = ManagedState::Indeterminate;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        external.expected = next.clone();
        self.edits.clear();
        self.spool = Some(next_spool);
        self.state = ManagedState::Live;
        counters.workspace_reuses = 1;
        counters.descriptor_resets = 1;
        Self::record_spool_observation(&mut counters, Some(0));
        reservation.finish(&mut counters);
        Ok((next, counters, steps))
    }
    pub fn ensure_exact(&mut self, target: &RefState) -> VfsResult<OperationCounters> {
        self.require_live()?;
        if !self.edits.is_empty() {
            return Err(VfsError::InvalidState);
        }
        let external = self.external.as_ref().ok_or(VfsError::InvalidState)?;
        if external.native.revalidate_root_binding().is_err() {
            self.state = ManagedState::ExternalDirtyConflict;
            return Err(VfsError::ExternalDirtyConflict);
        }
        if &external.expected != target
            || external.engine.read_ref("main")?.as_ref() != Some(target)
        {
            return Err(VfsError::ExternalDirtyConflict);
        }
        let mut counters = OperationCounters {
            native: NativeOperationCounters {
                route: Some(NativeRoute::ExactNoop),
                ..NativeOperationCounters::default()
            },
            workspace_reuses: 1,
            ..OperationCounters::default()
        };
        self.observe_spool(&mut counters)?;
        let queue = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .observation();
        counters.operation_q_current_bytes = queue.current_bytes;
        counters.operation_q_high_water_bytes = queue.high_water_bytes;
        counters.operation_q_terminal_bytes = queue.current_bytes;
        Ok(counters)
    }
    pub fn refresh(&mut self, target: &RefState) -> VfsResult<OperationCounters> {
        self.refresh_inner(target, None)
    }
    pub fn refresh_splice(
        &mut self,
        accepted: &crate::AcceptedSplice,
    ) -> VfsResult<OperationCounters> {
        self.refresh_inner(accepted.after(), Some(accepted))
    }
    fn refresh_inner(
        &mut self,
        target: &RefState,
        accepted: Option<&crate::AcceptedSplice>,
    ) -> VfsResult<OperationCounters> {
        self.require_live()?;
        if !self.edits.is_empty() || target.name != "main" {
            return Err(VfsError::InvalidState);
        }
        let reservation = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .reserve();
        let external = self.external.as_mut().ok_or(VfsError::InvalidState)?;
        if accepted
            .is_some_and(|splice| splice.before() != &external.expected || splice.after() != target)
        {
            return Err(VfsError::ExternalDirtyConflict);
        }
        if external.engine.read_ref("main")?.as_ref() != Some(target) {
            return Err(VfsError::ExternalDirtyConflict);
        }
        if external.expected.root == target.root {
            if external.native.revalidate_root_binding().is_err() {
                self.state = ManagedState::ExternalDirtyConflict;
                return Err(VfsError::ExternalDirtyConflict);
            }
            external.expected = target.clone();
            let mut counters = OperationCounters {
                native: NativeOperationCounters {
                    route: Some(NativeRoute::ExactNoop),
                    ..NativeOperationCounters::default()
                },
                workspace_reuses: 1,
                ..OperationCounters::default()
            };
            self.observe_spool(&mut counters)?;
            reservation.finish(&mut counters);
            return Ok(counters);
        }
        let live_scratch = external
            .live_scratch
            .as_ref()
            .ok_or(VfsError::InvalidState)?;
        let authority = live_scratch.namespace(b"authority")?;
        let topology = live_scratch.namespace(b"topology")?;
        self.state = ManagedState::Refreshing;
        let mut visible = false;
        match crate::refresh::apply(
            &external.engine,
            external.native.as_ref(),
            &authority,
            &topology,
            (&external.expected, target, accepted),
            &mut visible,
        ) {
            Ok(mut counters) => {
                external.expected = target.clone();
                self.state = ManagedState::Live;
                counters.workspace_reuses = 1;
                self.observe_spool(&mut counters)?;
                reservation.finish(&mut counters);
                Ok(counters)
            }
            Err(error) => {
                self.state = refresh_error_state(visible);
                Err(error)
            }
        }
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
        self.require_editable()?;
        let reservation = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .reserve();
        if self.edits.len() == 64 {
            return Err(VfsError::InvalidState);
        }
        let replacement_len = u64::try_from(bytes.len()).map_err(|_| VfsError::InvalidState)?;
        let external = self.external.as_mut().ok_or(VfsError::InvalidState)?;
        let old_hard_link_key =
            match crate::managed_edit::native_hard_link_key(external.native.as_ref(), path) {
                Ok(key) => key,
                Err(error) => {
                    self.state = ManagedState::ExternalDirtyConflict;
                    return Err(error);
                }
            };
        let (metadata, native_counters, sync_required) = match crate::managed_edit::mutate_native(
            external.native.as_ref(),
            path,
            start,
            delete_len,
            bytes,
        ) {
            Ok(metadata) => metadata,
            Err(error @ VfsError::NativeProtected) => return Err(error),
            Err(error) => {
                self.state = ManagedState::Indeterminate;
                return Err(error);
            }
        };
        let new_hard_link_key =
            match crate::managed_edit::native_hard_link_key(external.native.as_ref(), path) {
                Ok(key) => key,
                Err(error) => {
                    external.live_scratch = None;
                    self.state = ManagedState::Indeterminate;
                    return Err(error);
                }
            };
        if old_hard_link_key != new_hard_link_key {
            let transfer = external
                .live_scratch
                .as_ref()
                .ok_or(VfsError::InvalidState)
                .and_then(|scratch| {
                    let authority = scratch.namespace(b"authority")?;
                    let inode = authority
                        .get(&old_hard_link_key)?
                        .ok_or(VfsError::InvalidState)?;
                    authority.put(&new_hard_link_key, &inode)?;
                    authority.remove(&old_hard_link_key)?;
                    Ok(())
                });
            if let Err(error) = transfer {
                external.live_scratch = None;
                self.state = ManagedState::Indeterminate;
                return Err(error);
            }
        }
        let (offsets, spool_bytes) =
            self.append_spool_parts(&[SpoolPart::Bytes(bytes), SpoolPart::Metadata(&metadata)])?;
        let (spool_offset, _) = offsets[0];
        let (metadata_offset, metadata_len) = offsets[1];
        self.edits.push(crate::managed_edit::ManagedEdit::Replace {
            path: path.clone(),
            start,
            delete_len,
            spool_offset,
            replacement_len,
            metadata_offset,
            metadata_len,
            sync_required,
            native_identity: new_hard_link_key,
        });
        self.state = ManagedState::Dirty;
        let mut counters = OperationCounters {
            native: native_counters,
            ..OperationCounters::default()
        };
        Self::record_spool_observation(&mut counters, Some(spool_bytes));
        reservation.finish(&mut counters);
        Ok(counters)
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
        self.require_editable()?;
        let reservation = self
            .external
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .operation_q
            .reserve();
        if self.edits.len() == 64 {
            return Err(VfsError::InvalidState);
        }
        let external = self.external.as_mut().ok_or(VfsError::InvalidState)?;
        let mut topology_counters = OperationCounters::default();
        let (old_edge, new_edge) = managed_rename_edges(
            &external.engine,
            &external.expected,
            &self.edits,
            from,
            to,
            &mut topology_counters,
        )?;
        let topology = external
            .live_scratch
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .namespace(b"topology")?;
        if topology.get(&old_edge)?.is_none() {
            return Err(VfsError::InvalidState);
        }
        let result = crate::managed_edit::rename_native(external.native.as_ref(), from, to);
        match result {
            Ok((source_parent_metadata, target_parent_metadata)) => {
                let topology_update = external
                    .live_scratch
                    .as_ref()
                    .ok_or(VfsError::InvalidState)
                    .and_then(|scratch| {
                        let topology = scratch.namespace(b"topology")?;
                        topology.remove(&old_edge)?;
                        topology.put(&new_edge, &[])?;
                        Ok(())
                    });
                if let Err(error) = topology_update {
                    external.live_scratch = None;
                    self.state = ManagedState::Indeterminate;
                    return Err(error);
                }
                let (offsets, spool_bytes) = self.append_spool_parts(&[
                    SpoolPart::Metadata(&source_parent_metadata),
                    SpoolPart::Metadata(&target_parent_metadata),
                ])?;
                let (source_metadata_offset, source_metadata_len) = offsets[0];
                let (target_metadata_offset, target_metadata_len) = offsets[1];
                self.edits.push(crate::managed_edit::ManagedEdit::Rename {
                    from: from.clone(),
                    to: to.clone(),
                    source_metadata_offset,
                    source_metadata_len,
                    target_metadata_offset,
                    target_metadata_len,
                });
                self.state = ManagedState::Dirty;
                let mut counters = topology_counters.merge(OperationCounters {
                    native: NativeOperationCounters {
                        route: Some(NativeRoute::Rename),
                        ..NativeOperationCounters::default()
                    },
                    ..OperationCounters::default()
                })?;
                Self::record_spool_observation(&mut counters, Some(spool_bytes));
                reservation.finish(&mut counters);
                Ok(counters)
            }
            Err(error @ VfsError::NativeProtected) | Err(error @ VfsError::InvalidState) => {
                Err(error)
            }
            Err(error) => {
                self.state = ManagedState::Indeterminate;
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
        self.require_editable()?;
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
        self.discard_observed().map(drop)
    }
    pub fn discard_observed(&mut self) -> VfsResult<OperationCounters> {
        let reservation = self
            .external
            .as_ref()
            .map(|external| external.operation_q.reserve());
        let mut counters = OperationCounters::default();
        if let Some(mut external) = self.external.take() {
            match external.discard_inner() {
                Ok(cleanup) => counters = counters.merge(cleanup)?,
                Err(error) => {
                    self.external = Some(external);
                    return Err(error);
                }
            }
        }
        self.remove_spool()?;
        self.state = ManagedState::Closed;
        self.observe_spool(&mut counters)?;
        if let Some(reservation) = reservation {
            reservation.finish(&mut counters);
        }
        Ok(counters)
    }
    fn remove_spool(&mut self) -> VfsResult<()> {
        self.spool.take();
        Ok(())
    }
    fn append_spool_parts(&mut self, parts: &[SpoolPart<'_>]) -> VfsResult<(Vec<(u64, u64)>, u64)> {
        let start = match self.spool.as_mut() {
            Some(spool) => spool.seek(SeekFrom::End(0)).map_err(VfsError::from),
            None => Err(VfsError::InvalidState),
        };
        let start = match start {
            Ok(start) => start,
            Err(error) => {
                self.state = ManagedState::Indeterminate;
                return Err(error);
            }
        };
        let append = (|| {
            let spool = self.spool.as_mut().ok_or(VfsError::InvalidState)?;
            let mut offsets = Vec::with_capacity(parts.len());
            let mut offset = start;
            for part in parts {
                let len = match part {
                    SpoolPart::Bytes(bytes) => {
                        let len = u64::try_from(bytes.len()).map_err(|_| VfsError::InvalidState)?;
                        spool.write_all(bytes)?;
                        len
                    }
                    SpoolPart::Metadata(metadata) => {
                        crate::managed_edit::write_spooled_metadata(metadata, spool.as_mut())?
                    }
                };
                offsets.push((offset, len));
                offset = offset.checked_add(len).ok_or(VfsError::InvalidState)?;
            }
            Ok((offsets, offset))
        })();
        match append {
            Ok(offsets) => Ok(offsets),
            Err(error) => {
                self.state = ManagedState::Indeterminate;
                self.restore_spool_prefix(start)
                    .map_err(|_| VfsError::Indeterminate)?;
                Err(error)
            }
        }
    }
    fn restore_spool_prefix(&mut self, len: u64) -> VfsResult<()> {
        let external = self.external.as_ref().ok_or(VfsError::InvalidState)?;
        let root = external.native.root_directory()?;
        let mut replacement = external.native.create_temp_at(root.as_ref())?;
        let spool = self.spool.as_mut().ok_or(VfsError::InvalidState)?;
        spool.seek(SeekFrom::Start(0))?;
        let copied = std::io::copy(&mut spool.take(len), replacement.as_mut())?;
        if copied != len {
            return Err(VfsError::InvalidState);
        }
        self.spool = Some(replacement);
        Ok(())
    }
    fn observe_spool(&mut self, counters: &mut OperationCounters) -> VfsResult<()> {
        let observation = (|| {
            if let Some(spool) = self.spool.as_mut() {
                let position = spool.stream_position()?;
                let bytes = spool.seek(SeekFrom::End(0))?;
                spool.seek(SeekFrom::Start(position))?;
                Ok(Some(bytes))
            } else {
                Ok(None)
            }
        })();
        match observation {
            Ok(bytes) => {
                Self::record_spool_observation(counters, bytes);
                Ok(())
            }
            Err(error) => {
                self.state = ManagedState::Indeterminate;
                Err(error)
            }
        }
    }
    fn record_spool_observation(counters: &mut OperationCounters, bytes: Option<u64>) {
        counters.owned_temp_current = u64::from(bytes.is_some());
        counters.owned_temp_terminal = counters.owned_temp_current;
        counters.descriptor_spool_bytes_current = bytes.unwrap_or(0);
        counters.descriptor_spool_bytes_terminal = counters.descriptor_spool_bytes_current;
    }
    fn require_live(&self) -> VfsResult<()> {
        match self.state {
            ManagedState::Live => Ok(()),
            ManagedState::Dirty => Err(VfsError::InvalidState),
            ManagedState::Refreshing => Err(VfsError::InvalidState),
            ManagedState::ExternalDirtyConflict => Err(VfsError::ExternalDirtyConflict),
            ManagedState::Indeterminate => Err(VfsError::Indeterminate),
            ManagedState::IncompleteDerived => Err(VfsError::IncompleteDerived),
            ManagedState::Closed => Err(VfsError::InvalidState),
        }
    }
    fn require_editable(&self) -> VfsResult<()> {
        match self.state {
            ManagedState::Live | ManagedState::Dirty => Ok(()),
            ManagedState::Refreshing => Err(VfsError::InvalidState),
            ManagedState::ExternalDirtyConflict => Err(VfsError::ExternalDirtyConflict),
            ManagedState::Indeterminate => Err(VfsError::Indeterminate),
            ManagedState::IncompleteDerived => Err(VfsError::IncompleteDerived),
            ManagedState::Closed => Err(VfsError::InvalidState),
        }
    }
    fn require_checkpointable(&self) -> VfsResult<()> {
        self.require_editable()?;
        if (self.state == ManagedState::Live) == self.edits.is_empty() {
            Ok(())
        } else {
            Err(VfsError::InvalidState)
        }
    }
}

fn refresh_error_state(possibly_visible: bool) -> ManagedState {
    if possibly_visible {
        ManagedState::IncompleteDerived
    } else {
        ManagedState::Live
    }
}

fn managed_rename_edges(
    engine: &Engine,
    expected: &RefState,
    edits: &[crate::managed_edit::ManagedEdit],
    from: &CanonicalPath,
    to: &CanonicalPath,
    counters: &mut OperationCounters,
) -> VfsResult<(Vec<u8>, Vec<u8>)> {
    let namespace = crate::resolver::namespace(engine, expected.root)?;
    let original = translate_prior_renames(from, edits)?;
    let source_parent_path = translate_prior_renames(&parent_path(from)?, edits)?;
    let target_parent_path = translate_prior_renames(&parent_path(to)?, edits)?;
    let (child, _) = crate::resolver::resolve(engine, namespace, &original, counters)?;
    let (source_parent, _) =
        crate::resolver::resolve(engine, namespace, &source_parent_path, counters)?;
    let (target_parent, _) =
        crate::resolver::resolve(engine, namespace, &target_parent_path, counters)?;
    Ok((
        topology_edge_key(child, source_parent, basename(from)?),
        topology_edge_key(child, target_parent, basename(to)?),
    ))
}

fn translate_prior_renames(
    path: &CanonicalPath,
    edits: &[crate::managed_edit::ManagedEdit],
) -> VfsResult<CanonicalPath> {
    let mut bytes = path.as_bytes().to_vec();
    for edit in edits.iter().rev() {
        let crate::managed_edit::ManagedEdit::Rename { from, to, .. } = edit else {
            continue;
        };
        let target = to.as_bytes();
        if bytes == target
            || bytes
                .strip_prefix(target)
                .is_some_and(|suffix| suffix.first() == Some(&b'/'))
        {
            let suffix = &bytes[target.len()..];
            let mut translated = Vec::with_capacity(from.as_bytes().len() + suffix.len());
            translated.extend_from_slice(from.as_bytes());
            translated.extend_from_slice(suffix);
            bytes = translated;
        }
    }
    Ok(CanonicalPath::from_bytes(&bytes)?)
}

fn parent_path(path: &CanonicalPath) -> VfsResult<CanonicalPath> {
    let bytes = path.as_bytes();
    Ok(CanonicalPath::from_bytes(
        bytes
            .iter()
            .rposition(|byte| *byte == b'/')
            .map_or(&[][..], |separator| &bytes[..separator]),
    )?)
}

fn basename(path: &CanonicalPath) -> VfsResult<&[u8]> {
    let bytes = path.as_bytes();
    let name = bytes
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or(bytes, |separator| &bytes[separator + 1..]);
    if name.is_empty() {
        return Err(VfsError::InvalidState);
    }
    Ok(name)
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
    base_matches_expected: bool,
    writers: SharedLeaseState,
    owned: bool,
    owned_identity: Option<Vec<u8>>,
    live_scratch: Option<DiskTable>,
    active: bool,
    committed: Option<ObjectId>,
    operation_q: Arc<OperationQ>,
    digest_cache: Arc<SemanticDigestCache>,
}
impl ExternalWorkspace {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn scratch_connection_count(&self) -> u64 {
        u64::from(self.live_scratch.is_some())
    }
    pub fn read_metadata(&self, path: &CanonicalPath) -> VfsResult<crate::driver::NativeMetadata> {
        if !self.active {
            return Err(VfsError::InvalidState);
        }
        let root = self.native.root_directory()?;
        let (parent, name) = crate::managed_edit::native_parent(self.native.as_ref(), root, path)?;
        let token = self.native.token_at(parent.as_ref(), name)?;
        Ok(self
            .native
            .read_metadata_at(parent.as_ref(), name, Some(&token))?)
    }
    pub fn capture_quiescent(&mut self) -> VfsResult<ObjectId> {
        self.capture_quiescent_observed().map(|(root, _)| root)
    }
    pub fn capture_quiescent_observed(&mut self) -> VfsResult<(ObjectId, OperationCounters)> {
        let reservation = self.operation_q.reserve();
        if !self.active {
            return Err(VfsError::InvalidState);
        }
        if !self.base_matches_expected {
            return Err(VfsError::ExternalDirtyConflict);
        }
        let mut capture = CaptureLease::begin(self.writers.clone())?;
        let (root, mut counters) = if let Some(root) = self.committed {
            (root, OperationCounters::default())
        } else {
            let live_hard_links = self
                .live_scratch
                .as_ref()
                .map(|scratch| scratch.namespace(b"authority"))
                .transpose()?;
            let (next, counters) = capture_workspace(
                &self.engine,
                &self.digest_cache,
                self.native.as_ref(),
                Some(&self.expected),
                live_hard_links.as_ref(),
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
        reservation.finish(&mut counters);
        Ok((root, counters))
    }
    pub fn discard(&mut self) -> VfsResult<()> {
        let _reservation = self.operation_q.reserve();
        self.discard_inner().map(drop)
    }
    fn discard_inner(&mut self) -> VfsResult<OperationCounters> {
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
        let mut counters = OperationCounters::default();
        if let Some(scratch) = self.live_scratch.take() {
            counters.add_scratch(scratch.finish()?)?;
        }
        Ok(counters)
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

    struct EndSeekFailure;

    struct LaterEndSeekFailure {
        end_seeks: u8,
        len: u64,
        position: u64,
    }

    impl Read for EndSeekFailure {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            Ok(0)
        }
    }

    impl Write for EndSeekFailure {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Seek for EndSeekFailure {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            match position {
                SeekFrom::End(_) => Err(std::io::Error::other("injected end-seek failure")),
                _ => Ok(0),
            }
        }
    }

    impl crate::driver::OwnedTempHandle for EndSeekFailure {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn set_len(&mut self, _len: u64) -> crate::driver::Result<()> {
            Err(crate::driver::DriverError::Unsupported)
        }

        fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
            self
        }
    }

    impl Read for LaterEndSeekFailure {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            Ok(0)
        }
    }

    impl Write for LaterEndSeekFailure {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            let written = u64::try_from(buffer.len()).unwrap();
            self.position += written;
            self.len = self.len.max(self.position);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Seek for LaterEndSeekFailure {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            match position {
                SeekFrom::Start(position) => self.position = position,
                SeekFrom::Current(0) => {}
                SeekFrom::End(0) => {
                    self.end_seeks += 1;
                    if self.end_seeks > 1 {
                        return Err(std::io::Error::other("injected observation failure"));
                    }
                    self.position = self.len;
                }
                _ => return Err(std::io::Error::other("unsupported test seek")),
            }
            Ok(self.position)
        }
    }

    impl crate::driver::OwnedTempHandle for LaterEndSeekFailure {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn set_len(&mut self, len: u64) -> crate::driver::Result<()> {
            self.len = len;
            self.position = self.position.min(len);
            Ok(())
        }

        fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
            self
        }
    }

    #[test]
    fn initial_spool_seek_failure_is_fail_closed() {
        let mut workspace = ManagedWorkspace {
            external: None,
            edits: Vec::new(),
            state: ManagedState::Live,
            spool: Some(Box::new(EndSeekFailure)),
        };

        assert!(workspace
            .append_spool_parts(&[SpoolPart::Bytes(b"edit")])
            .is_err());
        assert_eq!(workspace.state, ManagedState::Indeterminate);
    }

    #[test]
    fn post_append_spool_observation_failure_is_fail_closed() {
        let mut workspace = ManagedWorkspace {
            external: None,
            edits: Vec::new(),
            state: ManagedState::Live,
            spool: Some(Box::new(LaterEndSeekFailure {
                end_seeks: 0,
                len: 0,
                position: 0,
            })),
        };

        workspace
            .append_spool_parts(&[SpoolPart::Bytes(b"edit")])
            .unwrap();
        workspace.state = ManagedState::Dirty;
        assert!(workspace
            .observe_spool(&mut OperationCounters::default())
            .is_err());
        assert_eq!(workspace.state, ManagedState::Indeterminate);
    }

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

    #[test]
    fn post_visibility_refresh_failure_requires_discard_or_rebuild() {
        assert_eq!(refresh_error_state(false), ManagedState::Live);
        assert_eq!(refresh_error_state(true), ManagedState::IncompleteDerived);
        let workspace = ManagedWorkspace {
            external: None,
            edits: Vec::new(),
            state: refresh_error_state(true),
            spool: None,
        };
        assert!(matches!(
            workspace.require_live(),
            Err(VfsError::IncompleteDerived)
        ));
    }
}
