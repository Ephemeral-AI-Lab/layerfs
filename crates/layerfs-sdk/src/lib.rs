//! Thin AppleWorkspaceV1 public facade.

#![forbid(unsafe_code)]

use layerfs_os::apple::AppleDriver;
use layerfs_vfs::LayerVfs;
use std::path::Path;
use std::sync::Arc;

pub use layerfs_vfs::{RootId, VfsError};

pub const COMPONENT: &str = "layerfs-sdk";

pub struct OpenedLayerFs {
    pub fs: LayerFs,
    pub head: RootId,
}
pub struct LayerFs(LayerVfs);

#[derive(Clone, Copy, Debug, Default)]
pub struct CompactionDiagnostics {
    pub old_generation_bytes: u64,
    pub new_generation_bytes: u64,
    pub mark_database_bytes: u64,
    pub candidate_journal_temp_peak_bytes: u64,
    pub verification_scratch_peak_bytes: u64,
    pub selector_temporary_bytes: u64,
    pub total_peak_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub transactions_started: u64,
    pub transactions_committed: u64,
    pub objects_created: u64,
    pub objects_reused: u64,
    pub page_size: i64,
    pub cache_pages: i64,
    pub database_bytes: Option<u64>,
    pub logical_engine_bytes: Option<u64>,
    pub compaction: Option<CompactionDiagnostics>,
    pub active_connections: u64,
    pub operation_q_bound_bytes: u64,
    pub operation_q_current_bytes: u64,
    pub operation_q_high_water_bytes: u64,
}

impl LayerFs {
    pub fn open(path: &Path) -> Result<OpenedLayerFs, layerfs_vfs::VfsError> {
        let fs = LayerVfs::from_engine(AppleDriver::open_store(path)?, Arc::new(AppleDriver))?;
        let head = fs.head().root;
        Ok(OpenedLayerFs { fs: Self(fs), head })
    }
    pub fn materialize_external(
        &self,
        root: RootId,
        path: &Path,
    ) -> Result<ExternalWorkspace, layerfs_vfs::VfsError> {
        self.0
            .materialize_external(root, path)
            .map(ExternalWorkspace)
    }
    pub fn materialize_managed(
        &self,
        root: RootId,
    ) -> Result<ManagedWorkspace, layerfs_vfs::VfsError> {
        self.0.materialize_managed(root).map(ManagedWorkspace)
    }
    pub fn open_external(&self, path: &Path) -> Result<ExternalWorkspace, layerfs_vfs::VfsError> {
        self.0.open_external(path).map(ExternalWorkspace)
    }
    pub fn fork(&self, root: RootId, name: &str) -> Result<RootId, layerfs_vfs::VfsError> {
        self.0.fork(root, name)
    }
    pub fn rollback(&self, root: RootId) -> Result<RootId, layerfs_vfs::VfsError> {
        self.0.rollback(root)
    }
    pub fn compact(self, path: &Path) -> Result<OpenedLayerFs, layerfs_vfs::VfsError> {
        let engine = AppleDriver::compact_store(self.0.into_engine()?, path)?;
        let fs = LayerVfs::from_engine(engine, Arc::new(AppleDriver))?;
        let head = fs.head().root;
        Ok(OpenedLayerFs { fs: Self(fs), head })
    }
    pub fn diagnostics(&self) -> Result<Diagnostics, VfsError> {
        let profile = self.0.profile();
        let storage = self.0.observations();
        let counters = self.0.counters()?;
        let operation_q = self.0.operation_q_observation();
        let compaction = self
            .0
            .last_compaction_observation()
            .map(|value| CompactionDiagnostics {
                old_generation_bytes: value.old_generation_bytes,
                new_generation_bytes: value.new_generation_bytes,
                mark_database_bytes: value.mark_database_bytes,
                candidate_journal_temp_peak_bytes: value.candidate_journal_temp_peak_bytes,
                verification_scratch_peak_bytes: value.verification_scratch_peak_bytes,
                selector_temporary_bytes: value.selector_temporary_bytes,
                total_peak_bytes: value.total_peak_bytes,
            });
        Ok(Diagnostics {
            transactions_started: counters.transactions_started,
            transactions_committed: counters.transactions_committed,
            objects_created: counters.objects_created,
            objects_reused: counters.objects_reused,
            page_size: profile.page_size,
            cache_pages: profile.cache_pages,
            database_bytes: storage.database_bytes,
            logical_engine_bytes: storage.logical_engine_bytes,
            compaction,
            active_connections: self.0.active_connection_count()?,
            operation_q_bound_bytes: layerfs_vfs::OPERATION_Q_BOUND_BYTES,
            operation_q_current_bytes: operation_q.current_bytes,
            operation_q_high_water_bytes: operation_q.high_water_bytes,
        })
    }
}

pub struct ManagedWorkspace(layerfs_vfs::ManagedWorkspace);
impl ManagedWorkspace {
    pub fn capture(&mut self) -> Result<RootId, layerfs_vfs::VfsError> {
        self.0.capture()
    }
    pub fn replace(
        &mut self,
        path: &str,
        start: u64,
        delete_len: u64,
        bytes: &[u8],
    ) -> Result<(), layerfs_vfs::VfsError> {
        self.0.replace_path(path, start, delete_len, bytes)
    }
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), layerfs_vfs::VfsError> {
        self.0.rename_path(from, to)
    }
    pub fn into_external(self) -> Result<ExternalWorkspace, layerfs_vfs::VfsError> {
        self.0.into_external().map(ExternalWorkspace)
    }
    pub fn discard(&mut self) -> Result<(), layerfs_vfs::VfsError> {
        self.0.discard()
    }
}

pub struct ExternalWorkspace(layerfs_vfs::ExternalWorkspace);
impl ExternalWorkspace {
    pub fn path(&self) -> &Path {
        self.0.path()
    }
    pub fn capture_quiescent(&mut self) -> Result<RootId, layerfs_vfs::VfsError> {
        self.0.capture_quiescent()
    }
    pub fn discard(&mut self) -> Result<(), layerfs_vfs::VfsError> {
        self.0.discard()
    }
    pub fn register_writer(&self) -> Result<WriterLease, layerfs_vfs::VfsError> {
        Ok(WriterLease {
            _lease: self.0.register_writer()?,
        })
    }
}

pub struct WriterLease {
    _lease: layerfs_vfs::workspace::WriterLease,
}
