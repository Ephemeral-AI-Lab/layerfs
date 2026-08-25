//! Thin AppleWorkspaceV1 public facade.

#![forbid(unsafe_code)]

use layerfs_os::apple::AppleDriver;
use layerfs_vfs::LayerVfs;
use std::path::Path;
use std::sync::Arc;

pub use layerfs_vfs::driver::NativeMetadata;
pub use layerfs_vfs::{
    AcceptedSplice, IntegrityMode, ManagedReplayStep, NativeRoute,
    OperationCounters as OperationDiagnostics, RefState, RootId, VfsError,
};

pub const COMPONENT: &str = "layerfs-sdk";

pub struct OpenedLayerFs {
    pub fs: LayerFs,
    pub head: RootId,
    pub ref_state: RefState,
}
pub struct LayerFs(LayerVfs);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompactionDiagnostics {
    pub old_generation_bytes: u64,
    pub new_generation_bytes: u64,
    pub mark_database_bytes: u64,
    pub candidate_journal_temp_peak_bytes: u64,
    pub verification_scratch_peak_bytes: u64,
    pub selector_temporary_bytes: u64,
    pub total_peak_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Diagnostics {
    pub transactions_started: u64,
    pub transactions_committed: u64,
    pub transactions_rolled_back: u64,
    pub statements: u64,
    pub busy_events: u64,
    pub locked_events: u64,
    pub objects_validated: u64,
    pub objects_created: u64,
    pub objects_reused: u64,
    pub object_bytes_read: u64,
    pub object_bytes_written: u64,
    pub range_bytes_requested: u64,
    pub range_bytes_returned: u64,
    pub logical_object_bytes: u64,
    pub logical_root_bytes: u64,
    pub logical_delta_bytes: u64,
    pub retained_union_scrubs: u64,
    pub root_verifications: u64,
    pub root_verification_objects: u64,
    pub root_verification_bytes: u64,
    pub fetched_rows: u64,
    pub fetched_row_authentication_passes: u64,
    pub fetched_row_role_decode_passes: u64,
    pub new_object_authentication_passes: u64,
    pub incumbent_authentication_passes: u64,
    pub payload_batch_queries: u64,
    pub payload_batch_references: u64,
    pub payload_batch_maximum: u64,
    pub put_lookup_statements: u64,
    pub put_insert_statements: u64,
    pub created_rows: u64,
    pub reused_rows: u64,
    pub publication_commits: u64,
    pub publication_closure_passes: u64,
    pub namespace_graph_verification_passes: u64,
    pub scratch_tables: u64,
    pub scratch_statements: u64,
    pub scratch_rows: u64,
    pub scratch_high_water_bytes: u64,
    pub page_size: i64,
    pub cache_pages: i64,
    pub cache_spill_pages: i64,
    pub database_bytes: Option<u64>,
    pub rollback_journal_bytes: Option<u64>,
    pub temporary_file_bytes: Option<u64>,
    pub logical_engine_bytes: Option<u64>,
    pub compaction: Option<CompactionDiagnostics>,
    pub active_connections: u64,
    pub operation_q_bound_bytes: u64,
    pub operation_q_current_bytes: u64,
    pub operation_q_high_water_bytes: u64,
}

impl LayerFs {
    pub fn open(path: &Path) -> Result<OpenedLayerFs, layerfs_vfs::VfsError> {
        Self::open_with_integrity(path, IntegrityMode::Verified)
    }
    pub fn open_with_integrity(
        path: &Path,
        mode: IntegrityMode,
    ) -> Result<OpenedLayerFs, layerfs_vfs::VfsError> {
        let fs = LayerVfs::from_engine(
            AppleDriver::open_store_with_integrity(path, mode)?,
            Arc::new(AppleDriver),
        )?;
        let ref_state = fs.current_head("main")?;
        let head = ref_state.root;
        Ok(OpenedLayerFs {
            fs: Self(fs),
            head,
            ref_state,
        })
    }
    pub fn current_head(&self, name: &str) -> Result<RefState, VfsError> {
        self.0.current_head(name)
    }
    pub fn store_id(&self) -> Result<[u8; 32], VfsError> {
        self.0.store_id()
    }
    pub fn read_range<W: std::io::Write>(
        &self,
        root: RootId,
        path: &str,
        range: std::ops::Range<u64>,
        output: W,
    ) -> Result<OperationDiagnostics, VfsError> {
        self.0
            .read_range(root, &layerfs_vfs::CanonicalPath::new(path)?, range, output)
    }
    pub fn read_to<W: std::io::Write>(
        &self,
        root: RootId,
        path: &str,
        output: W,
    ) -> Result<OperationDiagnostics, VfsError> {
        self.0
            .read_to(root, &layerfs_vfs::CanonicalPath::new(path)?, output)
    }
    pub fn replace_range<R: std::io::Read>(
        &self,
        expected: &RefState,
        path: &str,
        start: u64,
        delete_len: u64,
        input: R,
    ) -> Result<RefState, VfsError> {
        self.replace_range_observed(expected, path, start, delete_len, input)
            .map(|value| value.0)
    }
    pub fn replace_range_observed<R: std::io::Read>(
        &self,
        expected: &RefState,
        path: &str,
        start: u64,
        delete_len: u64,
        input: R,
    ) -> Result<(RefState, OperationDiagnostics), VfsError> {
        self.0.replace_range(
            expected,
            &layerfs_vfs::CanonicalPath::new(path)?,
            start,
            delete_len,
            input,
        )
    }
    pub fn replace_range_for_refresh_observed<R: std::io::Read>(
        &self,
        expected: &RefState,
        path: &str,
        start: u64,
        delete_len: u64,
        input: R,
    ) -> Result<(AcceptedSplice, OperationDiagnostics), VfsError> {
        self.0.replace_range_for_refresh(
            expected,
            &layerfs_vfs::CanonicalPath::new(path)?,
            start,
            delete_len,
            input,
        )
    }
    pub fn replace_file<R: std::io::Read>(
        &self,
        expected: &RefState,
        path: &str,
        input: R,
    ) -> Result<RefState, VfsError> {
        self.replace_file_observed(expected, path, input)
            .map(|value| value.0)
    }
    pub fn replace_file_observed<R: std::io::Read>(
        &self,
        expected: &RefState,
        path: &str,
        input: R,
    ) -> Result<(RefState, OperationDiagnostics), VfsError> {
        self.0
            .replace_file(expected, &layerfs_vfs::CanonicalPath::new(path)?, input)
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
    pub fn materialize_external_observed(
        &self,
        root: RootId,
        path: &Path,
    ) -> Result<(ExternalWorkspace, OperationDiagnostics), layerfs_vfs::VfsError> {
        self.0
            .materialize_external_observed(root, path)
            .map(|(workspace, counters)| (ExternalWorkspace(workspace), counters))
    }
    pub fn materialize_managed(
        &self,
        root: RootId,
    ) -> Result<ManagedWorkspace, layerfs_vfs::VfsError> {
        self.0.materialize_managed(root).map(ManagedWorkspace)
    }
    pub fn materialize_managed_observed(
        &self,
        root: RootId,
    ) -> Result<(ManagedWorkspace, OperationDiagnostics), layerfs_vfs::VfsError> {
        self.0
            .materialize_managed_observed(root)
            .map(|(workspace, counters)| (ManagedWorkspace(workspace), counters))
    }
    pub fn open_external(&self, path: &Path) -> Result<ExternalWorkspace, layerfs_vfs::VfsError> {
        self.0.open_external(path).map(ExternalWorkspace)
    }
    pub fn fork(&self, root: RootId, name: &str) -> Result<RootId, layerfs_vfs::VfsError> {
        self.0.fork(root, name)
    }
    pub fn rollback(
        &self,
        expected: &RefState,
        root: RootId,
    ) -> Result<RefState, layerfs_vfs::VfsError> {
        self.0.rollback(expected, root)
    }
    pub fn move_main(
        &self,
        expected: &RefState,
        target: RootId,
    ) -> Result<RefState, layerfs_vfs::VfsError> {
        self.0.move_main(expected, target)
    }
    pub fn compact(self, path: &Path) -> Result<OpenedLayerFs, layerfs_vfs::VfsError> {
        let engine = AppleDriver::compact_store(self.0.into_engine()?, path)?;
        let fs = LayerVfs::from_engine(engine, Arc::new(AppleDriver))?;
        let ref_state = fs.current_head("main")?;
        let head = ref_state.root;
        Ok(OpenedLayerFs {
            fs: Self(fs),
            head,
            ref_state,
        })
    }
    pub fn diagnostics(&self) -> Result<Diagnostics, VfsError> {
        let storage = self.0.observations();
        let mut diagnostics = self.counter_snapshot()?;
        diagnostics.database_bytes = storage.database_bytes;
        diagnostics.rollback_journal_bytes = storage.rollback_journal_bytes;
        diagnostics.temporary_file_bytes = storage.temporary_file_bytes;
        diagnostics.logical_engine_bytes = storage.logical_engine_bytes;
        Ok(diagnostics)
    }
    pub fn counter_snapshot(&self) -> Result<Diagnostics, VfsError> {
        let profile = self.0.profile();
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
            transactions_rolled_back: counters.transactions_rolled_back,
            statements: counters.statements,
            busy_events: counters.busy_events,
            locked_events: counters.locked_events,
            objects_validated: counters.objects_validated,
            objects_created: counters.objects_created,
            objects_reused: counters.objects_reused,
            object_bytes_read: counters.object_bytes_read,
            object_bytes_written: counters.object_bytes_written,
            range_bytes_requested: counters.range_bytes_requested,
            range_bytes_returned: counters.range_bytes_returned,
            logical_object_bytes: counters.logical_object_bytes,
            logical_root_bytes: counters.logical_root_bytes,
            logical_delta_bytes: counters.logical_delta_bytes,
            retained_union_scrubs: counters.retained_union_scrubs,
            root_verifications: counters.root_verifications,
            root_verification_objects: counters.root_verification_objects,
            root_verification_bytes: counters.root_verification_bytes,
            fetched_rows: counters.fetched_rows,
            fetched_row_authentication_passes: counters.fetched_row_authentication_passes,
            fetched_row_role_decode_passes: counters.fetched_row_role_decode_passes,
            new_object_authentication_passes: counters.new_object_authentication_passes,
            incumbent_authentication_passes: counters.incumbent_authentication_passes,
            payload_batch_queries: counters.payload_batch_queries,
            payload_batch_references: counters.payload_batch_references,
            payload_batch_maximum: counters.payload_batch_maximum,
            put_lookup_statements: counters.put_lookup_statements,
            put_insert_statements: counters.put_insert_statements,
            created_rows: counters.created_rows,
            reused_rows: counters.reused_rows,
            publication_commits: counters.publication_commits,
            publication_closure_passes: counters.publication_closure_passes,
            namespace_graph_verification_passes: counters.namespace_graph_verification_passes,
            scratch_tables: counters.scratch_tables,
            scratch_statements: counters.scratch_statements,
            scratch_rows: counters.scratch_rows,
            scratch_high_water_bytes: counters.scratch_high_water_bytes,
            page_size: profile.page_size,
            cache_pages: profile.cache_pages,
            cache_spill_pages: profile.cache_spill_pages,
            database_bytes: None,
            rollback_journal_bytes: None,
            temporary_file_bytes: None,
            logical_engine_bytes: None,
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
    pub fn read_metadata(&self, path: &str) -> Result<NativeMetadata, layerfs_vfs::VfsError> {
        self.0
            .read_metadata(&layerfs_vfs::CanonicalPath::new(path)?)
    }
    pub fn read_to<W: std::io::Write>(
        &self,
        path: &str,
        output: W,
    ) -> Result<OperationDiagnostics, layerfs_vfs::VfsError> {
        self.0
            .read_to(&layerfs_vfs::CanonicalPath::new(path)?, output)
    }
    pub fn checkpoint(&mut self) -> Result<RefState, layerfs_vfs::VfsError> {
        self.0.checkpoint()
    }
    pub fn checkpoint_observed(
        &mut self,
    ) -> Result<(RefState, OperationDiagnostics), layerfs_vfs::VfsError> {
        self.0.checkpoint_observed()
    }
    pub fn checkpoint_observed_detailed(
        &mut self,
    ) -> Result<
        (
            RefState,
            OperationDiagnostics,
            Vec<layerfs_vfs::ManagedReplayStep>,
        ),
        layerfs_vfs::VfsError,
    > {
        self.0.checkpoint_observed_detailed()
    }
    pub fn ensure_exact(
        &mut self,
        target: &RefState,
    ) -> Result<OperationDiagnostics, layerfs_vfs::VfsError> {
        self.0.ensure_exact(target)
    }
    pub fn refresh(
        &mut self,
        target: &RefState,
    ) -> Result<OperationDiagnostics, layerfs_vfs::VfsError> {
        self.0.refresh(target)
    }
    pub fn refresh_splice(
        &mut self,
        accepted: &AcceptedSplice,
    ) -> Result<OperationDiagnostics, layerfs_vfs::VfsError> {
        self.0.refresh_splice(accepted)
    }
    pub fn capture(&mut self) -> Result<RootId, layerfs_vfs::VfsError> {
        self.0.capture()
    }
    pub fn capture_observed(
        &mut self,
    ) -> Result<(RootId, OperationDiagnostics), layerfs_vfs::VfsError> {
        self.0.capture_observed()
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
    pub fn replace_observed(
        &mut self,
        path: &str,
        start: u64,
        delete_len: u64,
        bytes: &[u8],
    ) -> Result<OperationDiagnostics, layerfs_vfs::VfsError> {
        self.0.replace_path_observed(path, start, delete_len, bytes)
    }
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), layerfs_vfs::VfsError> {
        self.0.rename_path(from, to)
    }
    pub fn rename_observed(
        &mut self,
        from: &str,
        to: &str,
    ) -> Result<OperationDiagnostics, layerfs_vfs::VfsError> {
        self.0.rename_path_observed(from, to)
    }
    pub fn into_external(self) -> Result<ExternalWorkspace, layerfs_vfs::VfsError> {
        self.0.into_external().map(ExternalWorkspace)
    }
    pub fn discard(&mut self) -> Result<(), layerfs_vfs::VfsError> {
        self.0.discard()
    }
    pub fn discard_observed(&mut self) -> Result<OperationDiagnostics, layerfs_vfs::VfsError> {
        self.0.discard_observed()
    }
}

pub struct ExternalWorkspace(layerfs_vfs::ExternalWorkspace);
impl ExternalWorkspace {
    pub fn path(&self) -> &Path {
        self.0.path()
    }
    pub fn read_metadata(&self, path: &str) -> Result<NativeMetadata, layerfs_vfs::VfsError> {
        self.0
            .read_metadata(&layerfs_vfs::CanonicalPath::new(path)?)
    }
    pub fn capture_quiescent(&mut self) -> Result<RootId, layerfs_vfs::VfsError> {
        self.0.capture_quiescent()
    }
    pub fn capture_quiescent_observed(
        &mut self,
    ) -> Result<(RootId, OperationDiagnostics), layerfs_vfs::VfsError> {
        self.0.capture_quiescent_observed()
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
