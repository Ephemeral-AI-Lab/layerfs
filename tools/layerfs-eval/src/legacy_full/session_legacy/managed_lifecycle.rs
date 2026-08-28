use super::super::{NativeOperationCounters, NativeRoute, OperationCounters};
use layerfs_core::CanonicalPath;
use layerfs_core::ObjectId;
use layerfs_storage::refs::RefState;
use layerfs_storage::EngineError;
use std::io::Write;

use super::managed_state::refresh_error_state;
use super::{ExternalWorkspace, VfsError, VfsResult};
pub struct ManagedWorkspace {
    pub(super) external: Option<ExternalWorkspace>,
    pub(super) edits: Vec<super::super::managed_edit_legacy::ManagedEdit>,
    pub(super) state: ManagedState,
    pub(super) spool: Option<Box<dyn layerfs_materialization::driver::OwnedTempHandle>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ManagedState {
    Live,
    Dirty,
    Refreshing,
    ExternalDirtyConflict,
    Indeterminate,
    IncompleteDerived,
    Closed,
}
impl ManagedWorkspace {
    fn read_metadata_canonical(
        &self,
        path: &CanonicalPath,
    ) -> VfsResult<layerfs_materialization::driver::NativeMetadata> {
        self.require_editable()?;
        let external = self.external.as_ref().ok_or(VfsError::InvalidState)?;
        let root = external.native.root_directory()?;
        let (parent, name) =
            super::super::managed_edit_legacy::native_parent(external.native.as_ref(), root, path)?;
        let token = external.native.token_at(parent.as_ref(), name)?;
        Ok(external
            .native
            .read_metadata_at(parent.as_ref(), name, Some(&token))?)
    }

    fn read_to_canonical<W: Write>(
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
            super::super::managed_edit_legacy::native_parent(external.native.as_ref(), root, path)?;
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
    pub fn read_metadata(
        &self,
        path: &str,
    ) -> VfsResult<layerfs_materialization::driver::NativeMetadata> {
        self.read_metadata_canonical(&CanonicalPath::new(path)?)
    }
    pub fn read_to<W: Write>(&self, path: &str, output: W) -> VfsResult<OperationCounters> {
        self.read_to_canonical(&CanonicalPath::new(path)?, output)
    }
    pub fn capture_observed(&mut self) -> VfsResult<(ObjectId, OperationCounters)> {
        let (state, mut counters) = self.checkpoint_observed()?;
        let root = state.root;
        let cleanup = match self
            .external
            .as_mut()
            .ok_or(VfsError::InvalidState)?
            .discard_inner()
        {
            Ok(cleanup) => cleanup,
            Err(error) => {
                return Err(VfsError::CommittedCleanup {
                    root,
                    error: Box::new(error),
                })
            }
        };
        counters = counters
            .merge(cleanup)
            .map_err(|error| VfsError::CommittedCleanup {
                root,
                error: Box::new(error.into()),
            })?;
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
    pub fn checkpoint_observed(&mut self) -> VfsResult<(RefState, OperationCounters)> {
        self.checkpoint_observed_inner(false)
            .map(|(state, counters, _)| (state, counters))
    }
    pub fn checkpoint_observed_detailed(
        &mut self,
    ) -> VfsResult<(
        RefState,
        OperationCounters,
        Vec<super::super::ManagedReplayStep>,
    )> {
        self.checkpoint_observed_inner(true)
    }
    fn checkpoint_observed_inner(
        &mut self,
        collect_steps: bool,
    ) -> VfsResult<(
        RefState,
        OperationCounters,
        Vec<super::super::ManagedReplayStep>,
    )> {
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
        super::super::managed_edit_legacy::sync_pending(external.native.as_ref(), &self.edits)?;
        let root = external.native.root_directory()?;
        let next_spool = external.native.create_temp_at(root.as_ref())?;
        let replay = {
            let spool = self.spool.as_mut().ok_or(VfsError::InvalidState)?;
            spool.flush()?;
            super::super::managed_edit_legacy::replay(
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
        accepted: &super::super::AcceptedSplice,
    ) -> VfsResult<OperationCounters> {
        self.refresh_inner(accepted.after(), Some(accepted))
    }
    fn refresh_inner(
        &mut self,
        target: &RefState,
        accepted: Option<&super::super::AcceptedSplice>,
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
        let scratch_before = external.live_scratch_observation()?;
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
            external.add_live_scratch_delta(scratch_before, &mut counters)?;
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
        match super::super::refresh_legacy::apply(
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
                external.add_live_scratch_delta(scratch_before, &mut counters)?;
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
}
