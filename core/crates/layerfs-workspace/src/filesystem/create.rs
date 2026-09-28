//! One atomic child publication; regular-file creation also installs its handle.
use crate::{runtime::coherence::MutationOrigin, *};
use layerfs_bridge::contract::SYMLINK_TARGET_BYTES;
use std::time::Instant;
#[derive(Clone, Copy)]
pub(super) enum Creation<'a> {
    Directory {
        mode: u32,
        umask: u32,
        origin: MutationOrigin,
    },
    File {
        options: FileCreateOptions,
        /// `None` creates the regular file without opening a handle.
        open: Option<FileOpenOptions>,
        origin: MutationOrigin,
    },
    Symlink {
        target: &'a [u8],
        origin: MutationOrigin,
    },
    /// One additional name for an existing regular inode. No identity is
    /// allocated and no handle is opened.
    Link { serial: u64, origin: MutationOrigin },
}
impl Workspace {
    /// Creates or opens a regular file, returning one Local lookup reference and
    /// one Local handle. A new binding and its initial handle publish atomically.
    /// Mounted success includes required invalidation. A later Coherence error
    /// retains the published handle in its receipt and releases the unreturned
    /// lookup reference; callers can inspect or release that handle without replay.
    pub fn create_file(
        &self,
        parent: u64,
        name: &[u8],
        options: FileCreateOptions,
        deadline: Instant,
    ) -> Result<(NodeAttributes, HandleId), WorkspaceError> {
        self.create_file_from(parent, name, options, deadline, MutationOrigin::Local)
    }
    pub(crate) fn create_file_from(
        &self,
        parent: u64,
        name: &[u8],
        options: FileCreateOptions,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<(NodeAttributes, HandleId), WorkspaceError> {
        let (attr, handle) = self.create_child(
            parent,
            name,
            Creation::File {
                options,
                open: Some(options.open),
                origin,
            },
            deadline,
        )?;
        Ok((
            attr,
            handle.expect("a regular create/open publishes its handle"),
        ))
    }
    pub(super) fn create_child(
        &self,
        parent: u64,
        name: &[u8],
        creation: Creation<'_>,
        deadline: Instant,
    ) -> Result<(NodeAttributes, Option<HandleId>), WorkspaceError> {
        let (mode, umask, open, kind) = match creation {
            Creation::Directory { mode, umask, .. } => (mode, umask, None, NodeKind::Directory),
            Creation::File { options, open, .. } => {
                (options.mode, options.umask, open, NodeKind::File)
            }
            Creation::Symlink { .. } => (0o777, 0, None, NodeKind::Symlink),
            Creation::Link { .. } => (0, 0, None, NodeKind::File),
        };
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        if let Creation::Symlink { target, .. } = creation {
            if target.len() > SYMLINK_TARGET_BYTES {
                return Err(WorkspaceError::Capacity);
            }
            if target.contains(&0) {
                return Err(WorkspaceError::InvalidInput);
            }
        }
        if !matches!(creation, Creation::Link { .. })
            && (mode
                & !(if kind == NodeKind::File {
                    0o777
                } else {
                    0o1777
                })
                != 0
                || umask & !0o777 != 0)
        {
            return Err(WorkspaceError::InvalidInput);
        }
        if let Some(options) = open {
            super::open::check_options(options, self.inner.access)?;
        }
        self.create_child_active(parent, name, creation, deadline)
    }
}
