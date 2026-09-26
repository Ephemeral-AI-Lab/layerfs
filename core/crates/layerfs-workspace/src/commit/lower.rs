//! Captured frontier traversal and the bounded wire representation of one request.
use crate::{
    backing::{
        binary_plus_tree::keyed::KeyCursor,
        metadata::MetadataHost,
        metadata_index::vector,
        metadata_pages::{self, Cell, PageRef},
        segments::Window,
    },
    overlay::{
        directories::Directory,
        pieces::{get, Inode, PieceKind},
        snapshot::Submission,
    },
    *,
};
use layerfs_bridge::contract::{InodeChange, MAX_FILE};
use std::{cmp::Ordering, sync::Arc, time::Instant};
type PreparedInodes = (Vec<InodeChange>, Vec<u64>, Vec<u64>);
pub(crate) enum Dirty {
    Inode(Inode),
    Directory(Directory),
}
pub struct FilePlan {
    pub inode: Inode,
    pub extents: u64,
    pub changes: u64,
}
/// One ordered pass over the captured dirty frontier and the two record kinds a
/// frontier serial names.
///
/// The frontier is one keyed tree of `'D' + generation + serial` records, and the
/// namespace and inode record of a serial live in that same tree under their own
/// kind bytes. A bounded successor lookup descends from the root for every key it
/// answers, so the walk this replaces read one root path per dirty serial plus one
/// more for each record that serial names - `O(H * K)` page reads for `H` levels
/// and `K` dirty serials. Three persistent cursors advance with the serials in
/// order instead, so each reached leaf of each kind is read once: `O(H + L)`.
///
/// The walk holds no metadata writer gate. The captured root is immutable and
/// pinned for this submission, so an ordinary mounted mutation may publish while a
/// Commit reads the frontier - the same bounded read lease the frozen extent and
/// namespace walks take. A window is leased per step rather than for the whole
/// walk, so a concurrent mutation and a transfer pull keep both scratch windows.
pub(crate) struct DirtyWalk {
    workspace: Workspace,
    host: Arc<MetadataHost>,
    generation: u64,
    revision: u64,
    prefix: [u8; 17],
    frontier: KeyCursor,
    names: KeyCursor,
    inodes: KeyCursor,
    after: u64,
}

/// Advances one ordered cursor to `key` and returns that exact record, if the
/// tree holds it. Records below `key` are consumed; the cursor stays positioned,
/// so ascending targets cost one pass over the records between them.
fn reach(
    cursor: &mut KeyCursor,
    key: &[u8],
    window: &mut Window,
) -> Result<Option<Cell>, WorkspaceError> {
    while let Some(cell) = cursor.next(window)? {
        match cell.key().cmp(key) {
            Ordering::Less => continue,
            Ordering::Equal => return Ok(Some(cell)),
            Ordering::Greater => return Ok(None),
        }
    }
    Ok(None)
}

impl DirtyWalk {
    /// The next dirty serial in order, or `None` once this generation's frontier
    /// is exhausted.
    pub(crate) fn next(&mut self) -> Result<Option<(u64, Dirty)>, WorkspaceError> {
        let mut lease = self.host.payloads.window(1, 3)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        loop {
            let Some(cell) = self.frontier.next(window)? else {
                return Ok(None);
            };
            let key = cell.key();
            // The frontier is one contiguous run of this generation's keys: the
            // first key that is not one of them ends the walk, so a record the
            // capture does not describe is never returned.
            if key.len() != 17 || key[..9] != self.prefix[..9] {
                return Ok(None);
            }
            if cell.value() != [1] {
                return Err(WorkspaceError::Io);
            }
            let serial = get(key, 9)?;
            if serial <= self.after {
                return Err(WorkspaceError::Io);
            }
            self.after = serial;
            // A maintained directory owns a namespace record and no inode
            // record; a regular identity owns an inode record. The namespace
            // record is checked first so a directory is never mistaken for a
            // file.
            if let Some(record) =
                reach(&mut self.names, &metadata_pages::namespace_key(serial), window)?
            {
                let directory = Directory::parse(record.value())?;
                // A maintained delta may still be captured: that origin is the
                // exact root its entries and tombstones are deltas against, and
                // it is the same root this capture was taken from.
                if directory.generation != self.generation
                    || directory.revision > self.revision
                {
                    return Err(WorkspaceError::Io);
                }
                return Ok(Some((serial, Dirty::Directory(directory))));
            }
            let record = reach(&mut self.inodes, &metadata_pages::inode_key(serial), window)?
                .ok_or(WorkspaceError::Io)?;
            let inode = Inode::parse(record.value())?;
            if inode.generation != self.generation
                || inode.revision > self.revision
                || inode.captured
            {
                return Err(WorkspaceError::Io);
            }
            if inode.constructs_file() && self.workspace.state()?.unbound(serial) {
                // A complete-file construction this generation created that no
                // name binds any more has no identity to save, so lowering drops
                // it and the walk moves on.
                continue;
            }
            return Ok(Some((serial, Dirty::Inode(inode))));
        }
    }
}

impl Workspace {
    /// Opens one ordered pass over this submission's captured frontier.
    ///
    /// A capture with nothing dirty opens empty cursors, so a Commit that lowers
    /// nothing reads no page at all.
    pub(crate) fn dirty_walk(
        &self,
        submission: &Submission,
        deadline: Instant,
    ) -> Result<DirtyWalk, WorkspaceError> {
        let captured = submission.capture()?;
        let host = self
            .host
            .metadata
            .as_ref()
            .cloned()
            .ok_or(WorkspaceError::Unsupported)?;
        let root = if captured.count == 0 {
            PageRef::NULL
        } else {
            captured.root.root()?
        };
        let arena = captured.root.arena.clone();
        let (frontier, names, inodes) = {
            let mut lease = host.payloads.window(1, 3)?;
            let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
            (
                arena.key_cursor(
                    root,
                    &metadata_pages::dirty_key(captured.generation, 0),
                    window,
                    deadline,
                )?,
                arena.key_cursor(root, &metadata_pages::namespace_key(0), window, deadline)?,
                arena.key_cursor(root, &metadata_pages::inode_key(0), window, deadline)?,
            )
        };
        Ok(DirtyWalk {
            workspace: self.clone(),
            host,
            generation: captured.generation,
            revision: captured.revision,
            prefix: metadata_pages::dirty_key(captured.generation, 0),
            frontier,
            names,
            inodes,
            after: 0,
        })
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
        // The captured sequence is immutable and pinned for this submission, so
        // the walk reads it under a bounded read lease: no Commit phase holds
        // the shared metadata writer gate across it, and a mounted write may
        // publish while the walk is in progress.
        let mut lease = host.payloads.window(1, 3)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        // One ordered walk checks the immutable final sequence and counts its
        // fixed wire records without retaining them in memory.
        let mut cursor =
            captured
                .root
                .arena
                .cursor(inode.pieces, 0, inode.length, window, deadline)?;
        let mut base = 0u64;
        let mut pending = 0u64;
        let mut total = 0u64;
        let mut changes = 0u64;
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
                pending = pending
                    .checked_add(piece.length)
                    .ok_or(WorkspaceError::Capacity)?;
                total = total
                    .checked_add(piece.length)
                    .ok_or(WorkspaceError::Capacity)?;
            } else {
                if complete
                    || piece.offset < base
                    || piece
                        .offset
                        .checked_add(piece.length)
                        .is_none_or(|end| end > inode.base_length)
                {
                    return Err(WorkspaceError::Io);
                }
                if piece.offset != base || pending != 0 {
                    changes = changes.checked_add(1).ok_or(WorkspaceError::Capacity)?;
                }
                base = piece.offset + piece.length;
                pending = 0;
            }
        }
        if position != inode.length {
            return Err(WorkspaceError::Io);
        }
        if base != inode.base_length || pending != 0 {
            changes = changes.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        }
        // A recorded edit count is exact unless a splice shared a subtree it
        // did not fold; `u16::MAX` records exactly that.
        if inode.edits != u16::MAX && changes != u64::from(inode.edits) {
            return Err(WorkspaceError::Io);
        }
        if total != inode.replacement
            || (complete && total != inode.length)
            || total > MAX_FILE
            || position != inode.length
        {
            return Err(WorkspaceError::Io);
        }
        if std::env::var_os("LAYERFS_COMPLEXITY_DIAGNOSTIC").is_some() {
            eprintln!("LFS_PIECE_LOWER v=2 pieces={extents} changes={changes}");
        }
        Ok(FilePlan {
            inode,
            extents,
            changes,
        })
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
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        // The result records live in the submission's own result root, and one
        // ordered cursor serves every saved identity the same way the frontier
        // walk serves the dirty serials: one pass over the records between two
        // ascending serials instead of one root path per record.
        let saved = submission.result_ref()?;
        let mut results = {
            let mut lease = host.payloads.window(1, 3)?;
            let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
            captured.root.arena.key_cursor(
                saved,
                &metadata_pages::result_key(0),
                window,
                deadline,
            )?
        };
        let mut walk = self.dirty_walk(submission, deadline)?;
        let mut seen = 0;
        while let Some((serial, dirty)) = walk.next()? {
            seen += 1;
            let Dirty::Inode(inode) = dirty else {
                continue;
            };
            if inodes.len() == files {
                return Err(WorkspaceError::Io);
            }
            let record = {
                let mut lease = host.payloads.window(1, 3)?;
                let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
                reach(&mut results, &metadata_pages::result_key(serial), window)?
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
