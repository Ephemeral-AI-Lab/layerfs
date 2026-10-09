//! READDIR's reading visit: one read-only owner job that records nothing.
use crate::{
    db::{integer, unsigned},
    lifetime::native_visit::Held,
    sql, DirectoryEntry, DirectoryEntryWindow, InodeKind, NativeDirectory, NativeDirectoryCursor,
    NativeDirectoryPage, NativeMount, Overlay, OverlayError, OverlayResult, StatementKind,
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
    /// The reply that holds `cookie`, and the cookie's name in it.
    pub(crate) fn native_directory_cursor(
        &self,
        directory: NativeDirectory,
        offset: u64,
    ) -> OverlayResult<NativeDirectoryCursor> {
        match offset {
            0 => return Ok(NativeDirectoryCursor::Start),
            1 => return Ok(NativeDirectoryCursor::AfterDot),
            2 => return Ok(NativeDirectoryCursor::Names(None)),
            _ => {}
        }
        let (first, names) = self
            .query(
                StatementKind::Lease,
                sql::COOKIE_PAGE,
                &[
                    &directory.mount.route.ns,
                    &integer(directory.owner)?,
                    &integer(offset)?,
                ],
                24,
                |r| Ok((unsigned(r, 0)?, r.get::<_, Vec<u8>>(1)?)),
            )?
            .pop()
            .ok_or(OverlayError::Stale)?;
        // Only an accepted entry's number is an offset: one past the reply's
        // last name was never handed out.
        let mut names = decode_names(&names).ok_or(OverlayError::Invalid("cookie names"))?;
        let index = usize::try_from(offset - first).map_err(|_| OverlayError::Stale)?;
        if index >= names.len() {
            return Err(OverlayError::Stale);
        }
        Ok(NativeDirectoryCursor::Names(Some(names.swap_remove(index))))
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
    pub fn read_native_directory_visit(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: u64,
        offset: u64,
        after: Option<&[u8]>,
    ) -> OverlayResult<NativeDirectoryPage> {
        if after.is_some_and(|name| name.is_empty() || name.len() > 255) {
            return Err(OverlayError::Invalid("native directory continuation"));
        }
        let (state, _) = self.native_fence(mount, serial, Held::Directory(handle), true)?;
        let directory = NativeDirectory {
            mount,
            owner: handle,
            serial,
        };
        let cursor = self.native_directory_cursor(directory, offset)?;
        if let (Some(after), Some(minimum)) = (after, cursor.after_name()) {
            if after < minimum {
                return Err(OverlayError::Invalid("native directory continuation"));
            }
        }
        let parent = match offset {
            0 | 1 => Some(self.native_parent(mount, serial)?),
            _ => None,
        };
        let source = self.visit_source(mount, state)?;
        let parent_inode = self.inode_at(mount.route, serial, state.active, state.installed)?;
        let after = after.or(cursor.after_name()).map(<[u8]>::to_vec);
        let start = after.as_deref().unwrap_or(&[]);
        let mut local_kinds = Vec::new();
        let mut window = |generation: i64| {
            self.query(
                StatementKind::DirectoryEntry,
                sql::SOURCE_NAMES_KINDS,
                &[
                    &mount.route.ns,
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
        Ok(NativeDirectoryPage {
            directory,
            cursor,
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
