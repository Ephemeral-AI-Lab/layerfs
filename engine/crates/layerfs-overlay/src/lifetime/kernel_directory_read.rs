//! READDIR's reading visit: one read-only owner job that records nothing.
use crate::{
    db::{integer, unsigned},
    sql, DirectoryEntry, DirectoryEntryWindow, InodeKind, KernelDirectory, KernelDirectoryCursor,
    KernelDirectoryPage, KernelMount, Overlay, OverlayError, OverlayResult, StatementKind,
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
                local_kinds: Vec::new(),
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
        let mut local_kinds = Vec::new();
        let mut window = |generation: i64| {
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
                    let kind = match row.get::<_, Option<i64>>(4)? {
                        None => None,
                        Some(1) => Some(InodeKind::File),
                        Some(2) => Some(InodeKind::Directory),
                        Some(3) => Some(InodeKind::Symlink),
                        Some(_) => return Err(rusqlite::Error::InvalidQuery),
                    };
                    let entry = DirectoryEntry {
                        inherited: row.get(3)?,
                        parent: unsigned(row, 0)?,
                        name: row.get(1)?,
                        serial,
                    };
                    Ok((entry, serial.zip(kind)))
                },
            )
            .map(|rows| {
                let (entries, kinds): (Vec<_>, Vec<_>) = rows.into_iter().unzip();
                local_kinds.extend(kinds.into_iter().flatten());
                entries
            })
        };
        let active = window(state.active.0)?;
        let captured = match state.captured.or(state.consolidating) {
            Some(generation) => window(generation.0)?,
            None => Vec::new(),
        };
        local_kinds.sort_unstable_by_key(|(serial, _)| *serial);
        local_kinds.dedup_by_key(|(serial, _)| *serial);
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
            local_kinds,
        })
    }
}
