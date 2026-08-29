use crate::Workspace;
use layerfs_storage_core::{CommitId, RefOutcome, Result, StorageError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceState {
    Active,
    Committed,
    Discarded,
}

impl Workspace {
    pub fn state(&self) -> WorkspaceState {
        self.state
    }

    pub fn commit(&mut self) -> Result<RefOutcome<CommitId>> {
        self.ensure_active()?;
        if self.pins() != 0 {
            return Err(StorageError::InvalidInput("workspace busy"));
        }
        let mutations = self.build_mutations()?;
        let outcome = self
            .branch
            .commit_staged(self.branch_id, self.expected_head, &mutations)?;
        self.state = WorkspaceState::Committed;
        let _ = self.clear_spool();
        Ok(outcome)
    }

    pub fn discard(&mut self) -> Result<()> {
        if self.state == WorkspaceState::Committed {
            return Err(StorageError::InvalidInput("workspace committed"));
        }
        self.clear_spool()?;
        self.state = WorkspaceState::Discarded;
        Ok(())
    }

    pub(crate) fn ensure_active(&self) -> Result<()> {
        if self.state == WorkspaceState::Active {
            Ok(())
        } else {
            Err(StorageError::InvalidInput("workspace inactive"))
        }
    }
}
