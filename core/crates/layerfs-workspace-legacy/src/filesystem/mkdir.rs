//! Directory creation delegates to the shared child-publication path.
use super::create::Creation;
use crate::{runtime::coherence::MutationOrigin, *};
use std::time::Instant;
impl Workspace {
    /// Creates one directory and returns one Local lookup reference. Mounted
    /// success includes checked parent-attribute and entry invalidation.
    pub fn mkdir(
        &self,
        parent: u64,
        name: &[u8],
        mode: u32,
        umask: u32,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        self.mkdir_from(parent, name, mode, umask, deadline, MutationOrigin::Local)
    }
    pub(crate) fn mkdir_from(
        &self,
        parent: u64,
        name: &[u8],
        mode: u32,
        umask: u32,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<NodeAttributes, WorkspaceError> {
        self.create_child(
            parent,
            name,
            Creation::Directory {
                mode,
                umask,
                origin,
            },
            deadline,
        )
        .map(|(attr, _)| attr)
    }
}
