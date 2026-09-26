//! Lower only the captured directory frontier; canonical objects remain service-owned.
//!
//! One dirty directory owns at most one row here: its final bindings, its
//! declaration, its metadata patch, or both the second and third. The bindings
//! are read with the keyed tree's own cursors - one ordered pass over the entry
//! leaves and one over the removal leaves - and merged in name order, so a
//! directory that binds `K` names costs one pass over its reached leaves rather
//! than one successor lookup per name. A name cannot be both bound and removed,
//! so the merge is also the duplicate check the sort this replaces performed.
//!
//! No name count, row count or directory count is admitted here. The frontier
//! charge this capture already reserved describes exactly these rows, so the
//! only refusals are a record that cannot describe itself and the transport's
//! own metadata frame, which `save.rs` checks against the exact encoded total.
use super::lower::Dirty;
use crate::{
    backing::{
        binary_plus_tree::keyed::KeyCursor,
        metadata::RootOwner,
        metadata_index::vector,
        metadata_pages::{self, PageRef},
        segments::Window,
    },
    overlay::{
        directories::{self, Origin},
        snapshot::Submission,
    },
    *,
};
use layerfs_bridge::contract::{DirectoryChange, DirectoryMetadata};
use std::{cmp::Ordering, time::Instant};
type PreparedDirectories = (
    Vec<DirectoryChange>,
    Vec<DirectoryMetadata>,
    Vec<DirectoryMetadata>,
);
/// One directory's final binding rows, in name order: a name's serial, or
/// `None` when this generation made the name absent.
type BindingRows = Vec<(Vec<u8>, Option<u64>)>;
impl Workspace {
    pub(super) fn prepared_directories(
        &self,
        submission: &Submission,
        deadline: Instant,
    ) -> Result<PreparedDirectories, WorkspaceError> {
        let captured = submission.capture()?;
        // One row per dirty directory at most: a maintained directory that binds
        // and removes nothing is represented by its metadata patch alone.
        let mut changes = vector(captured.directories)?;
        let mut new = vector(captured.directories)?;
        let mut patches = vector(captured.directories)?;
        let mut count = 0;
        let mut names = 0;
        let mut bytes = 0;
        let mut directories = 0;
        // Declarations this delta emitted; each owns one name record too.
        let mut declarations = 0;
        let mut walk = self.dirty_walk(submission, deadline)?;
        while let Some((serial, dirty)) = walk.next()? {
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
                    // in the same generation. Both roots are immutable and pinned
                    // for this submission, so the read needs no writer gate.
                    if reference.inode != serial || reference.generation != captured.generation {
                        return Err(WorkspaceError::Io);
                    }
                    let host = self
                        .host
                        .metadata
                        .as_ref()
                        .ok_or(WorkspaceError::Unsupported)?;
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
            let mut lease = host.payloads.window(1, 3)?;
            let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
            let rows = Self::directory_rows(&captured.root, directory, window, deadline)?;
            names += rows.len();
            bytes += rows.iter().map(|(name, _)| 10 + name.len()).sum::<usize>();
            if declared {
                // The declaration this delta already emitted carries the names.
                match changes.last_mut() {
                    Some(record) if record.parent == serial => record.changes = rows,
                    _ => return Err(WorkspaceError::Io),
                }
            } else {
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
    /// The final binding rows of one maintained directory, in name order.
    ///
    /// The entry and removal leaves are two ordered runs and the merge is their
    /// union: every entry cell becomes one binding row, every removal cell one
    /// absent row, and a name in both is a record that cannot describe one final
    /// state. The record's own count is the exact number of entry rows, so a page
    /// that holds a different number of cells than it declares is refused rather
    /// than lowered.
    fn directory_rows(
        owner: &std::sync::Arc<RootOwner>,
        directory: directories::Directory,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<BindingRows, WorkspaceError> {
        // One entry row per cell the record declares; a removal row is charged
        // when its tombstone is written, so it is reserved against that charge as
        // the merge reaches it.
        let mut rows = vector(usize::from(directory.count))?;
        let mut entries = owner.arena.key_cursor(
            directory.entries,
            &metadata_pages::entry_key(&[])?,
            window,
            deadline,
        )?;
        let mut removals: Option<KeyCursor> = if directory.tombstones == PageRef::NULL {
            None
        } else {
            Some(owner.arena.key_cursor(
                directory.tombstones,
                b"T",
                window,
                deadline,
            )?)
        };
        let mut entry = entries.next(window)?;
        let mut removal = match removals.as_mut() {
            Some(cursor) => cursor.next(window)?,
            None => None,
        };
        let mut bound = 0usize;
        loop {
            let take_entry = match (&entry, &removal) {
                (None, None) => break,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (Some(bound_cell), Some(removal_cell)) => {
                    match bound_cell.key()[1..].cmp(directories::tombstone(removal_cell.key())?) {
                        Ordering::Less => true,
                        Ordering::Greater => false,
                        // One name cannot be both bound and removed: the two
                        // records are two claims about one final state.
                        Ordering::Equal => return Err(WorkspaceError::Io),
                    }
                }
            };
            if take_entry {
                let cell = entry.as_ref().ok_or(WorkspaceError::Io)?;
                let name = &cell.key()[1..];
                crate::filesystem::namespace::child_path(&[], name)?;
                rows.push((name.to_vec(), Some(directories::entry_serial(cell.value())?)));
                bound += 1;
                entry = entries.next(window)?;
            } else {
                let cell = removal.as_ref().ok_or(WorkspaceError::Io)?;
                let name = directories::tombstone(cell.key())?;
                crate::filesystem::namespace::child_path(&[], name)?;
                rows.try_reserve(1).map_err(|_| WorkspaceError::Capacity)?;
                rows.push((name.to_vec(), None));
                removal = match removals.as_mut() {
                    Some(cursor) => cursor.next(window)?,
                    None => None,
                };
            }
        }
        if bound != usize::from(directory.count) {
            return Err(WorkspaceError::Io);
        }
        Ok(rows)
    }
}
