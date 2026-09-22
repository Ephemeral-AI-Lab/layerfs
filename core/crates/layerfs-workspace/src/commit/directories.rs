//! Lower only the captured directory frontier; canonical objects remain service-owned.
use super::lower::Dirty;
use crate::{
    backing::{
        metadata_index::vector,
        metadata_pages::{self, PageRef},
    },
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
        // One name record per dirty directory plus one declaration per fresh one.
        let mut changes = vector(128 + captured.directories)?;
        let mut new = vector(captured.directories)?;
        let mut patches = vector(captured.directories)?;
        let mut after = 0;
        let mut count = 0;
        let mut names = 0;
        let mut bytes = 0;
        let mut directories = 0;
        // Declarations this delta emitted; each owns one name record too.
        let mut declarations = 0;
        while let Some((serial, dirty)) = self.next_dirty(submission, after, deadline)? {
            after = serial;
            count += 1;
            let Dirty::Directory(directory) = dirty else {
                continue;
            };
            directories += 1;
            if directories > captured.directories {
                return Err(WorkspaceError::Io);
            }
            let metadata = DirectoryMetadata {
                serial,
                mode: directory.mode,
                mtime_seconds: directory.seconds,
                mtime_nanoseconds: directory.nanos,
            };
            // A directory the canonical state has not accepted yet is declared as
            // its own final binding record: the prepared profile requires every
            // declared serial to own a parent link, and a name is read solely from
            // that record's change list. The declaration happens once, in the
            // first Commit that names the serial, because the service refuses a
            // serial its base already holds.
            let fresh =
                matches!(directory.origin, Origin::Empty) && self.state()?.undeclared(serial);
            let mut declared = false;
            if fresh {
                new.push(metadata);
                changes.push(DirectoryChange {
                    parent: serial,
                    changes: vector(0)?,
                });
                declared = true;
                declarations += 1;
            }
            match directory.origin {
                Origin::Empty => {}
                Origin::Captured(reference) => {
                    // The delta anchors on one exact previous version of this
                    // directory, so the reference must still name a record this
                    // capture can read. The root page it was taken from is not
                    // required to be the page this capture published, because the
                    // operation that wrote it may have published a further root
                    // in the same generation.
                    if reference.inode != serial || reference.generation != captured.generation {
                        return Err(WorkspaceError::Io);
                    }
                    let host = self
                        .host
                        .metadata
                        .as_ref()
                        .ok_or(WorkspaceError::Unsupported)?;
                    let _view = host.writer()?;
                    let mut lease = host.payloads.window(1, 3)?;
                    directories::captured(
                        &captured.root,
                        reference,
                        lease.window.as_mut().ok_or(WorkspaceError::Io)?,
                        deadline,
                    )?;
                }
                Origin::Canonical(root) if root == captured.context.effective_root => {}
                // A canonical delta that is not the effective root cannot be
                // declared by this capture.
                Origin::Canonical(_) => return Err(WorkspaceError::Io),
            }
            let bare = directory.entries == PageRef::NULL && directory.tombstones == PageRef::NULL;
            if !declared {
                // A maintained delta always carries its own inode row, so its
                // selected mode and mtime reach the result that way.
                patches.push(metadata);
            }
            if bare {
                // A fresh directory is already fully represented by its
                // declaration; a maintained one by the patch above.
                continue;
            }
            let host = self
                .host
                .metadata
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            let _view = host.writer()?;
            let mut lease = host.payloads.window(1, 3)?;
            let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
            let mut rows = vector(usize::from(directory.count) + 128)?;
            let mut lower = metadata_pages::entry_key(&[])?;
            let mut local_bytes = 0;
            while let Some(cell) = captured.root.arena.next(
                directory.entries,
                &lower,
                !rows.is_empty(),
                window,
                deadline,
            )? {
                if rows.len() == usize::from(directory.count) {
                    return Err(WorkspaceError::Io);
                }
                let name = &cell.key()[1..];
                crate::filesystem::namespace::child_path(&[], name)?;
                local_bytes += 10 + name.len();
                rows.push((
                    name.to_vec(),
                    Some(directories::entry_serial(cell.value())?),
                ));
                lower = cell.key().to_vec();
            }
            if rows.len() != usize::from(directory.count) {
                return Err(WorkspaceError::Io);
            }
            let mut removal = vec![b'T'];
            while let Some(cell) = captured.root.arena.next(
                directory.tombstones,
                &removal,
                rows.len() > usize::from(directory.count),
                window,
                deadline,
            )? {
                let name = directories::tombstone(cell.key())?;
                crate::filesystem::namespace::child_path(&[], name)?;
                if rows.len() == 128 {
                    return Err(WorkspaceError::Capacity);
                }
                local_bytes += 10 + name.len();
                rows.push((name.to_vec(), None));
                removal = cell.key().to_vec();
            }
            rows.sort_unstable_by(|a, b| a.0.cmp(&b.0));
            if rows.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
                return Err(WorkspaceError::Io);
            }
            names += rows.len();
            bytes += local_bytes;
            if declared {
                // The declaration this delta already emitted carries the names.
                match changes.last_mut() {
                    Some(record) if record.parent == serial => record.changes = rows,
                    _ => return Err(WorkspaceError::Io),
                }
            } else {
                if changes.len() == 128 {
                    return Err(WorkspaceError::Capacity);
                }
                changes.push(DirectoryChange {
                    parent: serial,
                    changes: rows,
                });
            }
        }
        // The walk visits every dirty serial exactly once and stops at the end of
        // this generation's dirty frontier, so these counters are its own
        // self-check. A fresh directory owns a declaration plus its name record, a
        // maintained one owns a name record, its metadata patch, or both.
        if count != captured.count
            || directories != captured.directories
            || names > captured.names
            || bytes != captured.name_bytes
            || changes.len() < declarations
        {
            return Err(WorkspaceError::Io);
        }
        {
            // The submission owns the exact declarations this Commit sends, so a
            // completed Commit forgets those and only those. A directory a later
            // generation created is not one of them and stays undeclared.
            let mut state = submission.state.lock().map_err(|_| WorkspaceError::Io)?;
            state.declared.clear();
            for metadata in &new {
                state.declared.push(metadata.serial);
            }
        }
        Ok((changes, new, patches))
    }
}
