//! Live parent edges are retained by `State::collect` while a directory is pinned.
use super::state::State;
use crate::WorkspaceError;

impl State {
    /// Walk the attached resident chain; a detached or missing edge cannot
    /// authorize a live mutation. The node count bounds a corrupt cycle, not
    /// a valid namespace depth.
    pub(crate) fn live_chain(
        &self,
        serial: u64,
        ancestor: Option<u64>,
        root: u64,
    ) -> Result<bool, WorkspaceError> {
        let mut current = serial;
        let mut found = false;
        for _ in 0..self.nodes.len() {
            let node = self.node(current)?;
            if !node.attached {
                return Err(WorkspaceError::NotFound);
            }
            found |= Some(current) == ancestor;
            if current == root {
                return Ok(found);
            }
            current = node.parent;
        }
        Err(WorkspaceError::Io)
    }
}
