//! One atomic name removal: tombstone the exact binding, keep other owners.
//!
//! An unlink removes exactly one name. Every other name, open handle, pinned
//! read and frozen generation keeps its own reference to the same inode, and a
//! last-name removal frees no bytes while one of them is still live.
use crate::{runtime::coherence::MutationOrigin, *};
use std::time::Instant;
impl Workspace {
    /// Removes one regular-file or symlink name. The inode, its content and
    /// every other owner survive; a still-open handle keeps reading and writing
    /// the same version after its last name is gone.
    pub fn unlink(
        &self,
        parent: u64,
        name: &[u8],
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        self.remove_name_from(parent, name, false, deadline, MutationOrigin::Local)
    }
    /// Removes one empty directory name. A nonempty directory, the root and a
    /// non-directory name are refused before any publication.
    pub fn rmdir(&self, parent: u64, name: &[u8], deadline: Instant) -> Result<(), WorkspaceError> {
        self.remove_name_from(parent, name, true, deadline, MutationOrigin::Local)
    }
    pub(crate) fn unlink_from(
        &self,
        parent: u64,
        name: &[u8],
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<(), WorkspaceError> {
        self.remove_name_from(parent, name, false, deadline, origin)
    }
    pub(crate) fn rmdir_from(
        &self,
        parent: u64,
        name: &[u8],
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<(), WorkspaceError> {
        self.remove_name_from(parent, name, true, deadline, origin)
    }
    fn remove_name_from(
        &self,
        parent: u64,
        name: &[u8],
        directory: bool,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<(), WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        self.remove_name_active(parent, name, directory, deadline, origin)
    }
}
