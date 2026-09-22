//! One additional regular-file name for an existing inode.
use super::{create::Creation, original::Original};
use crate::runtime::state::OperationGuard;
use crate::{runtime::coherence::MutationOrigin, *};
use std::time::Instant;
impl Workspace {
    /// Binds one additional name to an existing regular file. Both names share
    /// one serial, content root and metadata root, and a later write through
    /// either is visible through the other. No handle is opened.
    pub fn link(
        &self,
        parent: u64,
        name: &[u8],
        serial: u64,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        self.link_from(parent, name, serial, deadline, MutationOrigin::Local)
    }
    pub(crate) fn link_from(
        &self,
        parent: u64,
        name: &[u8],
        serial: u64,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<NodeAttributes, WorkspaceError> {
        self.create_child(parent, name, Creation::Link { serial, origin }, deadline)
            .map(|(attr, _)| attr)
    }
    /// The selected attributes of one hard-link target plus the exact base
    /// version they were resolved from, refusing a directory, a symlink and a
    /// serial this delta no longer binds to any live name.
    pub(super) fn link_target(
        &self,
        serial: u64,
        operation: &mut OperationGuard,
        deadline: Instant,
    ) -> Result<(NodeAttributes, Original), WorkspaceError> {
        let deadline = Self::callback_deadline(deadline);
        let attr = {
            let state = self.state()?;
            self.available(&state)?;
            let node = state.node(serial)?;
            if node.attr.kind != NodeKind::File || node.names == 0 {
                return Err(WorkspaceError::Unsupported);
            }
            node.attr
        };
        let base = self.serial_original(serial, deadline, operation)?;
        if base.0.kind != NodeKind::File || base.0.serial != serial {
            return Err(WorkspaceError::Unsupported);
        }
        Ok((attr, base))
    }
}
