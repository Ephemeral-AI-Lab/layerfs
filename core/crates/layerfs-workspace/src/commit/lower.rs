//! Captured frontier traversal and the bounded wire representation of one request.
use crate::{
    backing::{
        metadata_index::vector,
        metadata_pages::{self, Cell},
    },
    overlay::{
        directories::Directory,
        pieces::{get, Inode, PieceKind},
        snapshot::Submission,
    },
    *,
};
use layerfs_bridge::contract::{Edit, InodeChange, MAX_EDITS_PER_OPERATION, MAX_FILE};
use std::time::Instant;
type PreparedInodes = (Vec<InodeChange>, Vec<u64>, Vec<u64>);
pub(crate) enum Dirty {
    Inode(Inode),
    Directory(Directory),
}
pub struct FilePlan {
    pub inode: Inode,
    pub edits: Vec<Edit>,
}
impl Workspace {
    pub(crate) fn next_dirty(
        &self,
        submission: &Submission,
        after: u64,
        deadline: Instant,
    ) -> Result<Option<(u64, Dirty)>, WorkspaceError> {
        let captured = submission.capture()?;
        if captured.count == 0 {
            return Ok(None);
        }
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let _view = host.writer()?;
        let mut lease = host.payloads.window(1, 3)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        // The cursor is the exact key of the last record this walk returned, so
        // a successor is the next key and never the same serial again. A record
        // the capture does not count is stepped over rather than returned: an
        // identity this generation created and no name binds any more is exactly
        // one of those, and it stays out of everything the captured counters
        // describe.
        let mut cursor = after;
        loop {
            let key = metadata_pages::dirty_key(captured.generation, cursor);
            let found = captured.root.arena.next(
                captured.root.root()?,
                &key,
                cursor != 0,
                window,
                deadline,
            )?;
            let Some(cell) = found else { return Ok(None) };
            if cell.key().len() != 17 || cell.key()[..9] != key[..9] {
                return Ok(None);
            }
            if cell.value() != [1] {
                return Err(WorkspaceError::Io);
            }
            let serial = get(cell.key(), 9)?;
            if serial <= cursor {
                return Err(WorkspaceError::Io);
            }
            // A maintained directory owns a namespace record and no inode
            // record; a regular identity owns an inode record. The namespace
            // record is checked first so a directory is never mistaken for a file.
            let directory = captured.root.arena.find(
                captured.root.root()?,
                &metadata_pages::namespace_key(serial),
                window,
                deadline,
            )?;
            if let Some(record) = directory {
                let directory = Directory::parse(record.value())?;
                // A maintained delta may still be captured: that origin is the
                // exact root its entries and tombstones are deltas against, and it
                // is the same root this capture was taken from.
                if directory.generation != captured.generation
                    || directory.revision > captured.revision
                {
                    return Err(WorkspaceError::Io);
                }
                return Ok(Some((serial, Dirty::Directory(directory))));
            }
            let record = captured
                .root
                .arena
                .find(
                    captured.root.root()?,
                    &metadata_pages::inode_key(serial),
                    window,
                    deadline,
                )?
                .ok_or(WorkspaceError::Io)?;
            let inode = Inode::parse(record.value())?;
            if inode.generation != captured.generation
                || inode.revision > captured.revision
                || inode.captured
            {
                return Err(WorkspaceError::Io);
            }
            if inode.constructs_file() && self.state()?.unbound(serial) {
                // A complete-file construction this generation created that no
                // name binds any more has no identity to save, so lowering drops
                // it and the walk moves on.
                cursor = serial;
                continue;
            }
            return Ok(Some((serial, Dirty::Inode(inode))));
        }
    }
    pub(crate) fn lower_file(
        &self,
        submission: &Submission,
        inode: Inode,
        deadline: Instant,
    ) -> Result<FilePlan, WorkspaceError> {
        if inode.symlink {
            return Err(WorkspaceError::Io);
        }
        let complete = inode.constructs_file();
        let captured = submission.capture()?;
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let _view = host.writer()?;
        let mut lease = host.payloads.window(1, 3)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        // One ordered walk over the frozen sequence. `edits` holds the ordered
        // replacement intervals only, so the walk's memory is bounded by the
        // edit count and never by the file's extent count.
        let mut cursor =
            captured
                .root
                .arena
                .cursor(inode.pieces, 0, inode.length, window, deadline)?;
        let mut edits = vector(MAX_EDITS_PER_OPERATION as usize)?;
        let mut base = 0u64;
        let mut replacement = 0u64;
        let mut total = 0u64;
        let mut delta = 0i128;
        let mut extents = 0u64;
        let mut position = 0u64;
        while let Some((start, piece)) = cursor.next(window)? {
            if start != position {
                return Err(WorkspaceError::Io);
            }
            position = position
                .checked_add(piece.length)
                .filter(|position| *position <= MAX_FILE)
                .ok_or(WorkspaceError::Io)?;
            extents = extents.checked_add(1).ok_or(WorkspaceError::Io)?;
            if piece.kind != PieceKind::Base {
                replacement = replacement
                    .checked_add(piece.length)
                    .ok_or(WorkspaceError::Capacity)?;
                total = total
                    .checked_add(piece.length)
                    .ok_or(WorkspaceError::Capacity)?;
            } else {
                if complete
                    || piece.offset < base
                    || piece.offset + piece.length > inode.base_length
                {
                    return Err(WorkspaceError::Io);
                }
                push_edit(&mut edits, base, piece.offset, replacement, &mut delta)?;
                base = piece.offset + piece.length;
                replacement = 0;
            }
        }
        if position != inode.length {
            return Err(WorkspaceError::Io);
        }
        push_edit(&mut edits, base, inode.base_length, replacement, &mut delta)?;
        // A recorded edit count is exact unless a splice shared a subtree it
        // did not fold; `u16::MAX` records exactly that.
        if inode.edits != u16::MAX && edits.len() != usize::from(inode.edits) {
            return Err(WorkspaceError::Io);
        }
        if total != inode.replacement
            || (complete && total != inode.length)
            || total > MAX_FILE
            || i128::from(inode.base_length) + delta != i128::from(inode.length)
        {
            return Err(WorkspaceError::Io);
        }
        if std::env::var_os("LAYERFS_COMPLEXITY_DIAGNOSTIC").is_some() {
            eprintln!("LFS_PIECE_LOWER v=1 pieces={extents} edits={}", edits.len());
        }
        Ok(FilePlan { inode, edits })
    }
    pub(crate) fn persist_saved(
        &self,
        submission: &Submission,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        let (entry, old) = {
            let state = submission.state.lock().map_err(|_| WorkspaceError::Io)?;
            (state.pending.ok_or(WorkspaceError::Io)?, state.result_slot)
        };
        let metadata = entry.metadata.ok_or(WorkspaceError::Io)?;
        let next = old.map_or(0, |slot| 1 - slot);
        let target = &submission.results[next];
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let _writer = host.writer()?;
        let mut lease = host.payloads.window(3, 4)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        target.reserve_result_update()?;
        let mut value = [0; 80];
        value[..8].copy_from_slice(&entry.revision.to_be_bytes());
        value[8..16].copy_from_slice(&entry.length.to_be_bytes());
        value[16..48].copy_from_slice(&entry.content);
        value[48..].copy_from_slice(&metadata);
        let mut update = vector(1)?;
        update.push(Cell::new(
            &metadata_pages::result_key(entry.serial),
            &value,
        )?);
        let root = target.update(submission.result_ref()?, update, window, deadline)?;
        target.seal(root, window, deadline)?;
        {
            let mut state = submission.state.lock().map_err(|_| WorkspaceError::Io)?;
            state.result_slot = Some(next);
            state.pending = None;
        }
        if let Some(old) = old {
            let mut report = MetadataCleanupReport::default();
            submission.results[old].reclaim(window, deadline, &mut report)?;
        }
        Ok(())
    }
    /// One typed row per dirty regular identity, in serial order. The saved
    /// version of each is exactly the result record this capture wrote for it;
    /// an identity with no result record was saved by no content at all, which
    /// only a fresh empty file can be.
    pub(crate) fn prepared_inodes(
        &self,
        submission: &Submission,
        deadline: Instant,
    ) -> Result<PreparedInodes, WorkspaceError> {
        let captured = submission.capture()?;
        let files = captured
            .count
            .checked_sub(captured.directories)
            .ok_or(WorkspaceError::Io)?;
        let mut inodes = vector(files)?;
        let mut fresh = vector(captured.fresh_files)?;
        let mut symlinks = vector(captured.fresh_symlinks)?;
        let mut after = 0;
        let mut seen = 0;
        while let Some((serial, dirty)) = self.next_dirty(submission, after, deadline)? {
            after = serial;
            seen += 1;
            let Dirty::Inode(inode) = dirty else {
                continue;
            };
            if inodes.len() == files {
                return Err(WorkspaceError::Io);
            }
            // `next_dirty` owns its own read window, so this record is read
            // under a separate one rather than nested inside it.
            let saved = submission.result_ref()?;
            let record = {
                let host = self
                    .host
                    .metadata
                    .as_ref()
                    .ok_or(WorkspaceError::Unsupported)?;
                let _view = host.writer()?;
                let mut lease = host.payloads.window(1, 3)?;
                let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
                captured.root.arena.find(
                    saved,
                    &metadata_pages::result_key(serial),
                    window,
                    deadline,
                )?
            };
            let (content, metadata) = match record {
                Some(cell) => {
                    let value = cell.value();
                    if cell.value_len != 80
                        || get(value, 0)? != inode.revision
                        || get(value, 8)? != inode.length
                    {
                        return Err(WorkspaceError::Io);
                    }
                    (
                        value[16..48].try_into().map_err(|_| WorkspaceError::Io)?,
                        value[48..80].try_into().map_err(|_| WorkspaceError::Io)?,
                    )
                }
                // A fresh empty file constructs no content, so its declaration
                // carries no saved root.
                None if inode.fresh && inode.length == 0 => ([0; 32], [0; 32]),
                None => return Err(WorkspaceError::Io),
            };
            if inode.fresh {
                let (list, maximum) = if inode.symlink {
                    (&mut symlinks, captured.fresh_symlinks)
                } else {
                    (&mut fresh, captured.fresh_files)
                };
                if list.len() == maximum {
                    return Err(WorkspaceError::Io);
                }
                list.push(serial);
            }
            inodes.push(InodeChange {
                serial,
                kind: if inode.symlink { 3 } else { 1 },
                content,
                metadata,
            });
        }
        if seen != captured.count
            || inodes.len() != files
            || fresh.len() > captured.fresh_files
            || symlinks.len() > captured.fresh_symlinks
        {
            return Err(WorkspaceError::Io);
        }
        Ok((inodes, fresh, symlinks))
    }
}
fn push_edit(
    edits: &mut Vec<Edit>,
    base: u64,
    end: u64,
    replacement: u64,
    delta: &mut i128,
) -> Result<(), WorkspaceError> {
    if end < base {
        return Err(WorkspaceError::Io);
    }
    if end == base && replacement == 0 {
        return Ok(());
    }
    if edits.len() == MAX_EDITS_PER_OPERATION as usize {
        return Err(WorkspaceError::Capacity);
    }
    let start = u64::try_from(i128::from(base) + *delta).map_err(|_| WorkspaceError::Io)?;
    let stop = u64::try_from(i128::from(end) + *delta).map_err(|_| WorkspaceError::Io)?;
    edits.push(Edit {
        start,
        end: stop,
        replacement,
    });
    *delta += i128::from(replacement) - i128::from(end - base);
    Ok(())
}
