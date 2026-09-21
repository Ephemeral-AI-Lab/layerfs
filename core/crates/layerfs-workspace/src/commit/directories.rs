//! Lower only the captured directory frontier; canonical objects remain service-owned.
use super::lower::Dirty;
use crate::{
    backing::{metadata_index::vector, metadata_pages},
    overlay::{
        directories::{self, Origin},
        snapshot::Submission,
    },
    *,
};
use layerfs_bridge::contract::{DirectoryChange, DirectoryMetadata};
use std::time::Instant;
type PreparedDirectories = (
    Vec<DirectoryChange>,
    Vec<DirectoryMetadata>,
    Vec<DirectoryMetadata>,
);
impl Workspace {
    pub(super) fn prepared_directories(
        &self,
        submission: &Submission,
        deadline: Instant,
    ) -> Result<PreparedDirectories, WorkspaceError> {
        let captured = submission.capture()?;
        let mut changes = vector(captured.directories)?;
        let mut new = vector(captured.directories)?;
        let mut patches = vector(captured.directories)?;
        let mut after = 0;
        let mut count = 0;
        let mut names = 0;
        let mut bytes = 0;
        while let Some((serial, dirty)) = self.next_dirty(submission, after, deadline)? {
            after = serial;
            count += 1;
            if count > captured.count {
                return Err(WorkspaceError::Io);
            }
            let Dirty::Directory(directory) = dirty else {
                continue;
            };
            if changes.len() == captured.directories {
                return Err(WorkspaceError::Io);
            }
            let metadata = DirectoryMetadata {
                serial,
                mode: directory.mode,
                mtime_seconds: directory.seconds,
                mtime_nanoseconds: directory.nanos,
            };
            match directory.origin {
                Origin::Empty => new.push(metadata),
                Origin::Canonical(root) if root == captured.context.effective_root => {
                    patches.push(metadata)
                }
                _ => return Err(WorkspaceError::Io),
            }
            let host = self
                .host
                .metadata
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            let _view = host.writer()?;
            let mut lease = host.payloads.window(1, 3)?;
            let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
            let mut entries = vector(usize::from(directory.count))?;
            let mut lower = metadata_pages::entry_key(&[])?;
            let mut local_bytes = 0;
            while let Some(cell) = captured.root.arena.next(
                directory.entries,
                &lower,
                !entries.is_empty(),
                window,
                deadline,
            )? {
                if entries.len() == usize::from(directory.count) {
                    return Err(WorkspaceError::Io);
                }
                let name = &cell.key()[1..];
                crate::filesystem::namespace::child_path(&[], name)?;
                let mut owned = vector(name.len())?;
                owned.extend_from_slice(name);
                local_bytes += 10 + name.len();
                entries.push((owned, Some(directories::entry_serial(cell.value())?)));
                lower = cell.key().to_vec();
            }
            if entries.len() != usize::from(directory.count)
                || local_bytes != directory.bytes as usize
            {
                return Err(WorkspaceError::Io);
            }
            names += entries.len();
            bytes += local_bytes;
            changes.push(DirectoryChange {
                parent: serial,
                changes: entries,
            });
        }
        if count != captured.count
            || changes.len() != captured.directories
            || names != captured.names
            || bytes != captured.name_bytes
        {
            return Err(WorkspaceError::Io);
        }
        Ok((changes, new, patches))
    }
}
