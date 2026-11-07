//! READDIR's reading visit: one read-only owner job that records nothing.
use crate::{
    db::{integer, unsigned},
    sql, DirectoryEntry, DirectoryEntryWindow, Inode, InodeKind, KernelDirectory,
    KernelDirectoryCursor, KernelDirectoryPage, KernelMount, Overlay, OverlayError, OverlayResult,
    StatementKind,
};
/// The names of one published reply: one length byte and the bytes of each.
pub(crate) fn decode_names(blob: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut names = Vec::new();
    let mut rest = blob;
    while let Some((length, tail)) = rest.split_first() {
        let length = usize::from(*length);
        if length == 0 || tail.len() < length {
            return None;
        }
        names.push(tail[..length].to_vec());
        rest = &tail[length..];
    }
    Some(names)
}
impl Overlay {
    /// The reply that holds `cookie`, and the cookie's name in it. A number
    /// below the handle's floor is no offset since the handle's last rewind.
    pub(crate) fn kernel_directory_cursor(
        &self,
        directory: KernelDirectory,
        offset: u64,
        floor: u64,
    ) -> OverlayResult<KernelDirectoryCursor> {
        match offset {
            0 => return Ok(KernelDirectoryCursor::Start),
            1 => return Ok(KernelDirectoryCursor::AfterDot),
            2 => return Ok(KernelDirectoryCursor::Names(None)),
            _ if offset < floor => return Ok(KernelDirectoryCursor::Rewound),
            _ => {}
        }
        let (first, names) = {
            let state = self.open.borrow();
            let store = state
                .cookies
                .get(&directory.owner)
                .ok_or(OverlayError::Stale)?;
            let (first, page) = store
                .offsets
                .range(floor..=offset)
                .next_back()
                .ok_or(OverlayError::Stale)?;
            (*first, page.names.clone())
        };
        // Only an accepted entry's number is an offset: one past the reply's
        // last name was never handed out.
        let mut names = decode_names(&names).ok_or(OverlayError::Invalid("cookie names"))?;
        let index = usize::try_from(offset - first).map_err(|_| OverlayError::Stale)?;
        if index >= names.len() {
            return Err(OverlayError::Stale);
        }
        Ok(KernelDirectoryCursor::Names(Some(names.swap_remove(index))))
    }
    /// READDIR's reading visit. The fence on the open directory descriptor
    /// (the Workspace row, the mount, the handle), the position the kernel
    /// offset denotes, the parent for a reply that starts before `..`, the
    /// directory's local row, and one window of current local names with
    /// the kinds of their local inodes. `after` continues an earlier window
    /// of the same request and is never before the offset's own name.
    ///
    /// Nothing is written and nothing is recorded: the page's source is the
    /// current base, named for the request to read inherited names from.
    pub fn read_kernel_directory_visit(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: u64,
        offset: u64,
        after: Option<&[u8]>,
    ) -> OverlayResult<KernelDirectoryPage> {
        if after.is_some_and(|name| name.is_empty() || name.len() > 255) {
            return Err(OverlayError::Invalid("kernel directory continuation"));
        }
        let (state, floor) = self.directory_fence(mount, serial, handle, true)?;
        let directory = KernelDirectory {
            mount,
            owner: handle,
            serial,
        };
        let cursor = self.kernel_directory_cursor(directory, offset, floor)?;
        let source = self.visit_source(state)?;
        if cursor == KernelDirectoryCursor::Rewound {
            return Ok(KernelDirectoryPage {
                directory,
                cursor,
                floor,
                parent: None,
                after: None,
                local: DirectoryEntryWindow {
                    source,
                    parent: serial,
                    parent_inode: None,
                    active: Vec::new(),
                    captured: Vec::new(),
                },
                local_inodes: Vec::new(),
            });
        }
        if let (Some(after), Some(minimum)) = (after, cursor.after_name()) {
            if after < minimum {
                return Err(OverlayError::Invalid("kernel directory continuation"));
            }
        }
        let parent = match offset {
            0 | 1 => Some(self.kernel_parent(mount, serial)?),
            _ => None,
        };
        let parent_inode = self.inode_at(serial, state.active, state.installed)?;
        let after = after.or(cursor.after_name()).map(<[u8]>::to_vec);
        let start = after.as_deref().unwrap_or(&[]);
        let window = |generation: i64| {
            self.query(
                StatementKind::DirectoryEntry,
                sql::SOURCE_NAMES_KINDS,
                &[
                    &generation,
                    &integer(serial)?,
                    &start,
                    &state.active.0,
                    &state.installed,
                ],
                40 + start.len() as u64,
                |row| {
                    let serial = crate::inode::optional_serial(row.get(2)?, 2)?;
                    let inode = local_inode(row, 4)?;
                    let entry = DirectoryEntry {
                        inherited: row.get(3)?,
                        parent: unsigned(row, 0)?,
                        name: row.get(1)?,
                        serial,
                    };
                    Ok((entry, serial.zip(inode)))
                },
            )
            .map(|rows| {
                let (entries, inodes): (Vec<_>, Vec<_>) = rows.into_iter().unzip();
                let inodes: Vec<(u64, Inode)> = inodes.into_iter().flatten().collect();
                (entries, inodes)
            })
        };
        let (active, active_inodes) = window(state.active.0)?;
        let (captured, captured_inodes) = match state.captured.or(state.consolidating) {
            Some(generation) => window(generation.0)?,
            None => (Vec::new(), Vec::new()),
        };
        // One row per serial, ascending by serial: the active window's row is
        // the view's latest, so a captured row answers only a serial the
        // active window does not list.
        let mut local_inodes: std::collections::BTreeMap<u64, Inode> =
            active_inodes.into_iter().collect();
        for (serial, inode) in captured_inodes {
            local_inodes.entry(serial).or_insert(inode);
        }
        Ok(KernelDirectoryPage {
            directory,
            cursor,
            floor,
            parent,
            after,
            local: DirectoryEntryWindow {
                source,
                parent: serial,
                parent_inode,
                active,
                captured,
            },
            local_inodes: local_inodes.into_iter().collect(),
        })
    }
}
/// The eleven columns [`crate::sql::SOURCE_NAMES_KINDS`] reads after the
/// name's four, in the column order of [`crate::sql::INODE_CAPTURE`]: the
/// latest local row of the name's serial at the view, or None when it has
/// none and the name is inherited from the base alone.
fn local_inode(row: &rusqlite::Row<'_>, at: usize) -> rusqlite::Result<Option<Inode>> {
    if row.get::<_, Option<i64>>(at)?.is_none() {
        return Ok(None);
    }
    let kind = match row.get::<_, i64>(at + 1)? {
        1 => InodeKind::File,
        2 => InodeKind::Directory,
        3 => InodeKind::Symlink,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(Some(Inode {
        serial: unsigned(row, at)?,
        kind,
        mode: row.get(at + 2)?,
        mtime_seconds: row.get(at + 3)?,
        mtime_nanoseconds: row.get(at + 4)?,
        nlink: unsigned(row, at + 5)?,
        size: unsigned(row, at + 6)?,
        inherited_cutoff: unsigned(row, at + 7)?,
        born: unsigned(row, at + 8)?,
        entries: unsigned(row, at + 9)?,
        subdirs: unsigned(row, at + 10)?,
    }))
}
