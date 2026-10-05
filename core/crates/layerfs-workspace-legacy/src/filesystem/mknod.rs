//! Regular-file creation without opening a handle.
use super::create::Creation;
use crate::{runtime::coherence::MutationOrigin, *};
use std::time::Instant;
impl Workspace {
    /// Creates one empty regular file and returns one Local lookup reference.
    /// No file handle is allocated or opened, so this consumes no handle slot.
    pub fn mknod(
        &self,
        parent: u64,
        name: &[u8],
        mode: u32,
        umask: u32,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        self.mknod_from(parent, name, mode, umask, deadline, MutationOrigin::Local)
    }
    pub(crate) fn mknod_from(
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
            Creation::File {
                options: FileCreateOptions {
                    mode,
                    umask,
                    exclusive: true,
                    open: FileOpenOptions::default(),
                },
                open: None,
                origin,
            },
            deadline,
        )
        .map(|(attr, _)| attr)
    }
}
