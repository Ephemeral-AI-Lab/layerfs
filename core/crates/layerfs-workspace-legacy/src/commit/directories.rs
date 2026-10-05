//! Lower only the captured directory frontier, one directory row at a time.
//!
//! One dirty directory owns at most one row here: a fresh directory's
//! declaration row - which carries its final bindings, when it has any - or a
//! maintained directory's row. A directory this generation touched without
//! changing a name carries no row at all: its selected fields travel as its
//! identity row instead.
//!
//! The bindings are read with the keyed tree's own cursors - one ordered pass
//! over the entry leaves and one over the removal leaves - and merged in name
//! order, so a directory that binds `K` names costs one pass over its reached
//! leaves rather than one successor lookup per name. A name cannot be both bound
//! and removed, so the merge is also the duplicate check the sort this replaces
//! performed.
//!
//! No name count, row count or directory count is admitted here. The frontier
//! charge this capture already reserved describes exactly these rows, and the
//! stream's own declared totals are measured from the rows this pass produces, so
//! the only refusals are a record that cannot describe itself and a stream the
//! charged bound refuses.
use super::lower::{Dirty, DirtyWalk};
use super::stream::PreparedRow;
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
        snapshot::Captured,
    },
    *,
};
use std::{cmp::Ordering, time::Instant};
/// One directory's final binding rows, in name order: a name's serial, or
/// `None` when this generation made the name absent.
type BindingRows = Vec<(Vec<u8>, Option<u64>)>;
/// The first section of a prepared stream: one row per changed directory.
pub(crate) struct DirectorySection<'a> {
    captured: &'a Captured,
    walk: DirtyWalk,
    /// True while this pass may record the declarations it emits.
    record: bool,
    count: usize,
    directories: usize,
    names: usize,
    bytes: usize,
    declarations: usize,
}
impl<'a> DirectorySection<'a> {
    /// Opens the section over one captured frontier.
    pub(crate) fn new(captured: &'a Captured, walk: DirtyWalk, record: bool) -> Self {
        Self {
            captured,
            walk,
            record,
            count: 0,
            directories: 0,
            names: 0,
            bytes: 0,
            declarations: 0,
        }
    }
    /// True while this pass records what it declares.
    pub(crate) fn record(&self) -> bool {
        self.record
    }
    /// The next directory row, in parent order, or `None` at the end.
    pub(crate) fn next(
        &mut self,
        workspace: &Workspace,
        deadline: Instant,
    ) -> Result<Option<PreparedRow>, WorkspaceError> {
        loop {
            let Some((serial, dirty)) = self.walk.next()? else {
                // The walk visits every dirty serial exactly once and stops at
                // the end of this generation's dirty frontier, so these counters
                // are its own self-check against the capture that charged them.
                if self.count != self.captured.count
                    || self.directories != self.captured.directories
                    || self.names > self.captured.names
                    || self.bytes != self.captured.name_bytes
                {
                    return Err(WorkspaceError::Io);
                }
                return Ok(None);
            };
            self.count += 1;
            let Dirty::Directory(directory) = dirty else {
                continue;
            };
            self.directories += 1;
            if self.directories > self.captured.directories {
                return Err(WorkspaceError::Io);
            }
            // A directory the canonical state has not accepted yet is declared as
            // its own final binding record: the prepared profile requires every
            // declared serial to own a parent link, and a name is read solely from
            // that record's change list. The declaration happens once, in the
            // first Commit that names the serial, because the service refuses a
            // serial its base already holds.
            let fresh =
                matches!(directory.origin, Origin::Empty) && workspace.state()?.undeclared(serial);
            if fresh {
                self.declarations += 1;
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
                    if reference.inode != serial || reference.generation != self.captured.generation
                    {
                        return Err(WorkspaceError::Io);
                    }
                    let host = workspace
                        .host
                        .metadata
                        .as_ref()
                        .ok_or(WorkspaceError::Unsupported)?;
                    let mut lease = host.payloads.window(1, 3)?;
                    directories::captured(
                        &self.captured.root,
                        reference,
                        lease.window.as_mut().ok_or(WorkspaceError::Io)?,
                        deadline,
                    )?;
                }
                Origin::Canonical(root) if root == self.captured.context.effective_root => {}
                // A canonical delta that is not the effective root cannot be
                // declared by this capture.
                Origin::Canonical(_) => return Err(WorkspaceError::Io),
            }
            let bare = directory.entries == PageRef::NULL && directory.tombstones == PageRef::NULL;
            if bare {
                // A fresh directory is already fully represented by its own (empty)
                // declaration row; a maintained one by its identity row.
                if fresh {
                    return Ok(Some(PreparedRow::Directory {
                        parent: serial,
                        changes: Vec::new(),
                    }));
                }
                continue;
            }
            let host = workspace
                .host
                .metadata
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            let mut lease = host.payloads.window(1, 3)?;
            let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
            let rows = Self::directory_rows(&self.captured.root, directory, window, deadline)?;
            self.names += rows.len();
            self.bytes += rows.iter().map(|(name, _)| 10 + name.len()).sum::<usize>();
            return Ok(Some(PreparedRow::Directory {
                parent: serial,
                changes: rows,
            }));
        }
    }
    /// Opens a declaration bookkeeping pass: the rows are produced, nothing is
    /// recorded. Used only by the measuring pass.
    #[allow(dead_code)]
    pub(crate) fn declarations(&self) -> usize {
        self.declarations
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
            Some(
                owner
                    .arena
                    .key_cursor(directory.tombstones, b"T", window, deadline)?,
            )
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
                crate::filesystem::namespace::check_name(name)?;
                rows.push((
                    name.to_vec(),
                    Some(directories::entry_serial(cell.value())?),
                ));
                bound += 1;
                entry = entries.next(window)?;
            } else {
                let cell = removal.as_ref().ok_or(WorkspaceError::Io)?;
                let name = directories::tombstone(cell.key())?;
                crate::filesystem::namespace::check_name(name)?;
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
