use super::super::capture_legacy::{capture_workspace, SemanticDigestCache};
use super::super::OperationCounters;
use layerfs_core::CanonicalPath;
use layerfs_core::ObjectId;
use layerfs_storage::refs::RefState;
use layerfs_storage::scratch::{DiskTable, ScratchObservation};
use layerfs_storage::Engine;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::lease::{CaptureLease, SharedLeaseState, WriterLease};
use super::operation_q::OperationQ;
use super::{VfsError, VfsResult};
pub struct ExternalWorkspace {
    pub(super) engine: Arc<Engine>,
    pub(super) native: Box<dyn layerfs_materialization::driver::ProjectionWorkspace>,
    pub(super) path: PathBuf,
    pub(super) expected: RefState,
    pub(super) base_matches_expected: bool,
    pub(super) writers: SharedLeaseState,
    pub(super) owned: bool,
    pub(super) owned_identity: Option<Vec<u8>>,
    pub(super) live_scratch: Option<DiskTable>,
    pub(super) active: bool,
    pub(super) committed: Option<ObjectId>,
    pub(super) operation_q: Arc<OperationQ>,
    pub(super) digest_cache: Arc<SemanticDigestCache>,
}
impl ExternalWorkspace {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn scratch_connection_count(&self) -> u64 {
        u64::from(self.live_scratch.is_some())
    }
    fn read_metadata_canonical(
        &self,
        path: &CanonicalPath,
    ) -> VfsResult<layerfs_materialization::driver::NativeMetadata> {
        if !self.active {
            return Err(VfsError::InvalidState);
        }
        let root = self.native.root_directory()?;
        let (parent, name) =
            super::super::managed_edit_legacy::native_parent(self.native.as_ref(), root, path)?;
        let token = self.native.token_at(parent.as_ref(), name)?;
        Ok(self
            .native
            .read_metadata_at(parent.as_ref(), name, Some(&token))?)
    }
    pub fn read_metadata(
        &self,
        path: &str,
    ) -> VfsResult<layerfs_materialization::driver::NativeMetadata> {
        self.read_metadata_canonical(&CanonicalPath::new(path)?)
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
        let scratch_before = self.live_scratch_observation()?;
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
        self.add_live_scratch_delta(scratch_before, &mut counters)?;
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
        let cleanup = self
            .finish_live_scratch()
            .map_err(|error| VfsError::CommittedCleanup {
                root,
                error: Box::new(error),
            })?;
        counters = counters
            .merge(cleanup)
            .map_err(|error| VfsError::CommittedCleanup {
                root,
                error: Box::new(error.into()),
            })?;
        reservation.finish(&mut counters);
        Ok((root, counters))
    }
    pub fn discard(&mut self) -> VfsResult<()> {
        self.discard_observed().map(drop)
    }
    pub fn discard_observed(&mut self) -> VfsResult<OperationCounters> {
        let reservation = self.operation_q.reserve();
        let mut counters = self.discard_inner()?;
        reservation.finish(&mut counters);
        Ok(counters)
    }
    pub(super) fn discard_inner(&mut self) -> VfsResult<OperationCounters> {
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
        self.finish_live_scratch()
    }
    fn finish_live_scratch(&mut self) -> VfsResult<OperationCounters> {
        let mut counters = OperationCounters::default();
        if let Some(scratch) = self.live_scratch.take() {
            let before = scratch.observation()?;
            let terminal = scratch
                .finish()?
                .checked_delta(before)
                .ok_or(VfsError::InvalidState)?;
            super::super::add_scratch(&mut counters, terminal)?;
        }
        Ok(counters)
    }
    pub(super) fn live_scratch_observation(&self) -> VfsResult<Option<ScratchObservation>> {
        self.live_scratch
            .as_ref()
            .map(DiskTable::observation)
            .transpose()
            .map_err(Into::into)
    }
    pub(super) fn add_live_scratch_delta(
        &self,
        before: Option<ScratchObservation>,
        counters: &mut OperationCounters,
    ) -> VfsResult<()> {
        let Some(before) = before else {
            return Ok(());
        };
        let after = self
            .live_scratch
            .as_ref()
            .ok_or(VfsError::InvalidState)?
            .observation()?;
        let mut delta = after.checked_delta(before).ok_or(VfsError::InvalidState)?;
        if delta.operation_statements != 0 || delta.rows != 0 {
            delta.high_water_bytes = after.high_water_bytes;
        }
        super::super::add_scratch(counters, delta)?;
        Ok(())
    }
    pub fn register_writer(&self) -> VfsResult<WriterLease> {
        if !self.active {
            return Err(VfsError::InvalidState);
        }
        WriterLease::begin(self.writers.clone())
    }
}
