//! Host projection driver contract.

use std::path::Path;

use super::{ProjectionFacts, ProjectionWorkspace, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspacePolicy {
    ManagedCreateOwned,
    ManagedPrivate,
    ExternalCooperative,
}

pub trait ProjectionDriver: Send + Sync {
    fn projection_facts(&self) -> ProjectionFacts {
        ProjectionFacts::default()
    }
    fn open_workspace(
        &self,
        path: &Path,
        policy: WorkspacePolicy,
        store_id: [u8; 32],
    ) -> Result<Box<dyn ProjectionWorkspace>>;
    fn recover_owned_workspaces(&self, _parent: &Path, _store_id: [u8; 32]) -> Result<()> {
        Ok(())
    }
}
