use crate::{
    quiescence, BeginOperationReceipt, EndOperationReceipt, FinalizedCandidate, OperationState,
    Presentation, Result, RuntimeLeases, WorkspaceDriver, WorkspaceError, WorkspacePaths,
    WorkspaceTicket,
};
use layerfs_core::ObjectId;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FreezeObservation {
    pub quiescence_ns: u128,
    pub driver_freeze_ns: u128,
}

pub struct OperationWorkspace<D> {
    ticket: WorkspaceTicket,
    state: OperationState,
    driver: D,
    paths: Option<WorkspacePaths>,
    leases: RuntimeLeases,
    candidate_root: Option<ObjectId>,
}

impl<D: WorkspaceDriver> OperationWorkspace<D> {
    pub fn start(
        ticket: WorkspaceTicket,
        driver: D,
        paths: Option<WorkspacePaths>,
    ) -> Result<(Self, BeginOperationReceipt)> {
        if ticket.presentation != driver.presentation()
            || (ticket.presentation == Presentation::Direct) != paths.is_none()
            || driver
                .view_path()
                .is_some_and(|view| paths.as_ref().is_none_or(|paths| paths.view() != view))
        {
            return Err(WorkspaceError::InvalidState);
        }
        if let Some(paths) = &paths {
            paths.validate()?;
        }
        let receipt = BeginOperationReceipt {
            operation_id: ticket.operation_id,
            working_storage_id: ticket.working_storage_id,
            expected_branch_generation: ticket.expected_branch_generation,
            base_root: ticket.base_root,
            presentation: ticket.presentation,
            state: OperationState::Active,
        };
        Ok((
            Self {
                ticket,
                state: OperationState::Active,
                driver,
                paths,
                leases: RuntimeLeases::default(),
                candidate_root: None,
            },
            receipt,
        ))
    }

    pub fn state(&self) -> OperationState {
        self.state
    }

    pub fn leases(&self) -> &RuntimeLeases {
        &self.leases
    }

    pub fn paths(&self) -> Option<&WorkspacePaths> {
        self.paths.as_ref()
    }

    pub fn driver(&self) -> &D {
        &self.driver
    }

    pub fn driver_mut(&mut self) -> &mut D {
        &mut self.driver
    }

    pub fn freeze(&mut self, timeout: Duration) -> Result<()> {
        self.freeze_observed(timeout).map(drop)
    }

    pub fn freeze_observed(&mut self, timeout: Duration) -> Result<FreezeObservation> {
        if self.state != OperationState::Active {
            return Err(WorkspaceError::InvalidState);
        }
        let quiescence = Instant::now();
        if quiescence::establish(&self.leases, timeout).is_err() {
            self.state = OperationState::Incomplete;
            return Err(WorkspaceError::Timeout);
        }
        if let Err(error) = self.driver.quiesce(timeout) {
            self.state = OperationState::Incomplete;
            return Err(error);
        }
        let quiescence_ns = quiescence.elapsed().as_nanos();
        let driver_freeze = Instant::now();
        if let Err(error) = self.driver.freeze() {
            self.state = OperationState::Incomplete;
            return Err(error);
        }
        let driver_freeze_ns = driver_freeze.elapsed().as_nanos();
        self.state = OperationState::Frozen;
        Ok(FreezeObservation {
            quiescence_ns,
            driver_freeze_ns,
        })
    }

    /// Binds a candidate already constructed by `layerfs-core::logical`; this
    /// function deliberately performs no Branch publication or synchronization.
    pub fn finalize_candidate(
        &mut self,
        base_root: ObjectId,
        candidate_root: ObjectId,
        normalized_transition: Vec<u8>,
    ) -> Result<FinalizedCandidate> {
        if self.state != OperationState::Frozen || base_root != self.ticket.base_root {
            return Err(WorkspaceError::InvalidState);
        }
        self.state = OperationState::Finalized;
        self.candidate_root = Some(candidate_root);
        Ok(FinalizedCandidate {
            operation_id: self.ticket.operation_id,
            expected_branch_generation: self.ticket.expected_branch_generation,
            base_root,
            candidate_root,
            normalized_transition,
        })
    }

    pub fn cleanup(&mut self) -> Result<EndOperationReceipt> {
        if !matches!(
            self.state,
            OperationState::Active
                | OperationState::Frozen
                | OperationState::Finalized
                | OperationState::Incomplete
        ) {
            return Err(WorkspaceError::InvalidState);
        }
        let runtime_terminal = self.leases.observation()?;
        if runtime_terminal != Default::default() {
            return Err(WorkspaceError::Busy);
        }
        self.driver.cleanup()?;
        if let Some(paths) = self.paths.as_mut() {
            paths.remove_owned()?;
        }
        self.paths = None;
        self.state = OperationState::Cleaned;
        Ok(EndOperationReceipt {
            operation_id: self.ticket.operation_id,
            state: self.state,
            candidate_root: self.candidate_root,
            runtime_terminal,
            cleanup_complete: true,
        })
    }

    pub fn discard(&mut self) -> Result<EndOperationReceipt> {
        self.cleanup()
    }
}
