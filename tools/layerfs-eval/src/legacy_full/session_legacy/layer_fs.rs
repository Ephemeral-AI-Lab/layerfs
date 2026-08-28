use super::super::capture_legacy::{initialize_empty, SemanticDigestCache};
use super::super::materialize_legacy::materialize_workspace;
use super::super::{IntegrityMode, OperationCounters};
use layerfs_core::inode::InodeId;
use layerfs_core::ObjectId;
use layerfs_materialization::driver::{ProjectionDriver, WorkspacePolicy};
use layerfs_storage::refs::RefState;
use layerfs_storage::{Engine, EngineError};
use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::lease::shared_writers;
use super::operation_q::{OperationQ, OperationQObservation};
use super::{ExternalWorkspace, ManagedState, ManagedWorkspace, VfsError, VfsResult};
type ManagedRootAdmission = (
    Box<dyn layerfs_materialization::driver::ProjectionWorkspace>,
    Box<dyn layerfs_materialization::driver::DirectoryHandle>,
    Vec<u8>,
);

pub(crate) fn admit_managed_root(
    native: Box<dyn layerfs_materialization::driver::ProjectionWorkspace>,
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

pub struct LayerFs {
    pub(crate) engine: Arc<Engine>,
    driver: Arc<dyn ProjectionDriver>,
    integrity_mode: IntegrityMode,
    pub(crate) operation_q: Arc<OperationQ>,
    pub(crate) resolved_read_cache: super::super::resolver_legacy::ResolvedReadCache,
    digest_cache: Arc<SemanticDigestCache>,
}

pub(crate) fn topology_edge_key(child: InodeId, parent: InodeId, name: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(64 + name.len());
    key.extend_from_slice(child.as_bytes());
    key.extend_from_slice(parent.as_bytes());
    key.extend_from_slice(name);
    key
}

impl LayerFs {
    pub fn from_engine(
        engine: Engine,
        driver: Arc<dyn ProjectionDriver>,
        integrity_mode: IntegrityMode,
    ) -> VfsResult<Self> {
        let engine = Arc::new(engine);
        driver.recover_owned_workspaces(
            engine.path().parent().unwrap_or_else(|| Path::new(".")),
            engine.store_id()?,
        )?;
        if engine.read_ref("main")?.is_none() {
            initialize_empty(&engine)?;
        }
        Ok(Self {
            engine,
            driver,
            integrity_mode,
            operation_q: Arc::new(OperationQ::default()),
            resolved_read_cache: super::super::resolver_legacy::ResolvedReadCache::default(),
            digest_cache: Arc::new(SemanticDigestCache::default()),
        })
    }
    pub fn into_engine(self) -> VfsResult<Engine> {
        Arc::try_unwrap(self.engine).map_err(|_| VfsError::WorkspaceBusy)
    }
    pub(crate) fn integrity_mode(&self) -> IntegrityMode {
        self.integrity_mode
    }
    pub fn store_id(&self) -> VfsResult<[u8; 32]> {
        Ok(self.engine.store_id()?)
    }
    pub fn operation_q_observation(&self) -> OperationQObservation {
        self.operation_q.observation()
    }
    /// Cumulative native projection facts remain available after a failed open
    /// or after the returned workspace has been dropped.
    pub fn projection_facts(&self) -> layerfs_materialization::driver::ProjectionFacts {
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
            WorkspacePolicy::ExternalCooperative,
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
            WorkspacePolicy::ExternalCooperative,
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
            WorkspacePolicy::ManagedCreateOwned,
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
