//! Captured frontier traversal and the bounded wire representation of one request.
use crate::{
    backing::{
        metadata_index::vector,
        metadata_pages::{self, Cell},
    },
    overlay::{
        directories::{Directory, Origin},
        pieces::{get, Inode, PieceKind},
        snapshot::Submission,
    },
    *,
};
use layerfs_bridge::contract::{Edit, InodeChange, MAX_REPLAY};
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
        let key = metadata_pages::dirty_key(captured.generation, after);
        let found =
            captured
                .root
                .arena
                .next(captured.root.root()?, &key, after != 0, window, deadline)?;
        let Some(cell) = found else { return Ok(None) };
        if cell.key().len() != 17 || cell.key()[..9] != key[..9] {
            return Ok(None);
        }
        if cell.value() != [1] {
            return Err(WorkspaceError::Io);
        }
        let serial = get(cell.key(), 9)?;
        if serial <= after {
            return Err(WorkspaceError::Io);
        }
        if let Some(record) = captured.root.arena.find(
            captured.root.root()?,
            &metadata_pages::inode_key(serial),
            window,
            deadline,
        )? {
            let inode = Inode::parse(record.value())?;
            if inode.generation != captured.generation
                || inode.revision > captured.revision
                || inode.captured
            {
                return Err(WorkspaceError::Io);
            }
            return Ok(Some((serial, Dirty::Inode(inode))));
        }
        let record = captured
            .root
            .arena
            .find(
                captured.root.root()?,
                &metadata_pages::namespace_key(serial),
                window,
                deadline,
            )?
            .ok_or(WorkspaceError::Io)?;
        let directory = Directory::parse(record.value())?;
        if directory.generation != captured.generation
            || directory.revision > captured.revision
            || matches!(directory.origin, Origin::Captured(_))
        {
            return Err(WorkspaceError::Io);
        }
        Ok(Some((serial, Dirty::Directory(directory))))
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
        let pieces = captured.root.arena.pieces(
            inode.pieces,
            inode.count,
            inode.length,
            window,
            deadline,
        )?;
        let mut edits = vector(256)?;
        let mut base = 0u64;
        let mut replacement = 0u64;
        let mut total = 0u64;
        let mut delta = 0i128;
        for piece in pieces {
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
        push_edit(&mut edits, base, inode.base_length, replacement, &mut delta)?;
        if edits.len() != usize::from(inode.edits)
            || total != inode.replacement
            || (complete && total != inode.length)
            || (!complete && total > MAX_REPLAY)
            || i128::from(inode.base_length) + delta != i128::from(inode.length)
        {
            return Err(WorkspaceError::Io);
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
        if files == 0 {
            if submission.result_ref()? != crate::backing::metadata_pages::PageRef::NULL
                || captured.fresh_files != 0
                || captured.fresh_symlinks != 0
            {
                return Err(WorkspaceError::Io);
            }
            return Ok((vector(0)?, vector(0)?, vector(0)?));
        }
        let root = submission.result_root()?.ok_or(WorkspaceError::Io)?;
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let _view = host.writer()?;
        let mut lease = host.payloads.window(1, 3)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        let mut inodes = vector(files)?;
        let mut fresh = vector(captured.fresh_files)?;
        let mut symlinks = vector(captured.fresh_symlinks)?;
        let mut after = 0;
        while let Some(cell) = root.arena.next(
            root.root()?,
            &metadata_pages::result_key(after),
            after != 0,
            window,
            deadline,
        )? {
            if cell.key().len() != 9
                || cell.key()[0] != b'R'
                || cell.value_len != 80
                || inodes.len() == files
            {
                return Err(WorkspaceError::Io);
            }
            let serial = get(cell.key(), 1)?;
            if serial <= after {
                return Err(WorkspaceError::Io);
            }
            let value = cell.value();
            let original = captured
                .root
                .arena
                .find(
                    captured.root.root()?,
                    &metadata_pages::inode_key(serial),
                    window,
                    deadline,
                )?
                .ok_or(WorkspaceError::Io)?;
            let inode = Inode::parse(original.value())?;
            if get(value, 0)? != inode.revision
                || get(value, 8)? != inode.length
                || inode.generation != captured.generation
            {
                return Err(WorkspaceError::Io);
            }
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
                content: value[16..48].try_into().map_err(|_| WorkspaceError::Io)?,
                metadata: value[48..80].try_into().map_err(|_| WorkspaceError::Io)?,
            });
            after = serial;
        }
        if inodes.len() != files
            || fresh.len() != captured.fresh_files
            || symlinks.len() != captured.fresh_symlinks
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
    if edits.len() == 256 {
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
