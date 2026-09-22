//! Exact saved-version substitution and streaming construction of the live frontier.
use super::{completion::CommitAttempt, lower::Dirty};
use crate::{
    backing::{
        metadata::RootOwner,
        metadata_pages::{self, Cell, PageRef},
        segments::Window,
    },
    overlay::{
        directories::{Directory, Origin},
        pieces::{get, CapturedBase, Inode},
        snapshot::Submission,
    },
    *,
};
use layerfs_bridge::contract::CommitOutcomeWire;
use std::{sync::Arc, time::Instant};
struct Current {
    root: Arc<RootOwner>,
    generation: u64,
    revision: u64,
    count: usize,
}
impl Current {
    fn next(
        &self,
        after: u64,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Option<(u64, Dirty)>, WorkspaceError> {
        let key = metadata_pages::dirty_key(self.generation, after);
        let Some(cell) =
            self.root
                .arena
                .next(self.root.root()?, &key, after != 0, window, deadline)?
        else {
            return Ok(None);
        };
        if cell.key_len != 17 || cell.key()[..9] != key[..9] {
            return Ok(None);
        }
        let serial = get(cell.key(), 9)?;
        if serial <= after || cell.value() != [1] {
            return Err(WorkspaceError::Io);
        }
        if let Some(cell) = self.root.arena.find(
            self.root.root()?,
            &metadata_pages::inode_key(serial),
            window,
            deadline,
        )? {
            let inode = Inode::parse(cell.value())?;
            if inode.generation != self.generation || inode.revision > self.revision {
                return Err(WorkspaceError::Io);
            }
            return Ok(Some((serial, Dirty::File(inode))));
        }
        let cell = self
            .root
            .arena
            .find(
                self.root.root()?,
                &metadata_pages::namespace_key(serial),
                window,
                deadline,
            )?
            .ok_or(WorkspaceError::Io)?;
        let directory = Directory::parse(cell.value())?;
        if directory.generation != self.generation || directory.revision > self.revision {
            return Err(WorkspaceError::Io);
        }
        Ok(Some((serial, Dirty::Directory(directory))))
    }
}
fn canonical_inode(
    submission: &Submission,
    serial: u64,
    mut inode: Inode,
    window: &mut Window,
    deadline: Instant,
) -> Result<Inode, WorkspaceError> {
    let captured = submission.capture()?;
    let saved = captured.root.arena.find(
        submission.result_ref()?,
        &metadata_pages::result_key(serial),
        window,
        deadline,
    )?;
    if !inode.captured {
        if saved.is_some() {
            return Err(WorkspaceError::Io);
        }
        return Ok(inode);
    }
    let reference = CapturedBase::parse(inode.base)?;
    if reference.root != captured.root.root()?
        || reference.inode != serial
        || reference.generation != captured.generation
    {
        return Err(WorkspaceError::Io);
    }
    let cell = captured
        .root
        .arena
        .find(
            captured.root.root()?,
            &metadata_pages::inode_key(serial),
            window,
            deadline,
        )?
        .ok_or(WorkspaceError::Io)?;
    let original = Inode::parse(cell.value())?;
    let saved = saved.ok_or(WorkspaceError::Io)?;
    if saved.value_len != 80
        || original.captured
        || original.generation != captured.generation
        || original.revision != reference.revision
        || original.revision > captured.revision
        || original.length != inode.base_length
        || original.fresh != inode.fresh
        || get(saved.value(), 0)? != original.revision
        || get(saved.value(), 8)? != original.length
    {
        return Err(WorkspaceError::Io);
    }
    inode.base = saved.value()[16..48]
        .try_into()
        .map_err(|_| WorkspaceError::Io)?;
    inode.metadata = saved.value()[48..80]
        .try_into()
        .map_err(|_| WorkspaceError::Io)?;
    inode.captured = false;
    inode.fresh = false;
    // Piece offsets already name this exact captured file version; only its
    // representation changes from the pinned local version to its saved root.
    Ok(inode)
}
fn canonical_directory(
    submission: &Submission,
    serial: u64,
    mut directory: Directory,
    root: [u8; 32],
    window: &mut Window,
    deadline: Instant,
) -> Result<Directory, WorkspaceError> {
    let captured = submission.capture()?;
    match directory.origin {
        Origin::Empty => return Ok(directory),
        Origin::Canonical(base) if base == captured.context.effective_root => {}
        Origin::Captured(reference) => {
            if reference.root != captured.root.root()?
                || reference.inode != serial
                || reference.generation != captured.generation
            {
                return Err(WorkspaceError::Io);
            }
            let cell = captured
                .root
                .arena
                .find(
                    captured.root.root()?,
                    &metadata_pages::namespace_key(serial),
                    window,
                    deadline,
                )?
                .ok_or(WorkspaceError::Io)?;
            let prior = Directory::parse(cell.value())?;
            if prior.generation != captured.generation
                || prior.revision != reference.revision
                || matches!(prior.origin, Origin::Captured(_))
            {
                return Err(WorkspaceError::Io);
            }
        }
        _ => return Err(WorkspaceError::Io),
    }
    // Only origin changes. D1's E tree already contains exactly D1's names.
    directory.origin = Origin::Canonical(root);
    Ok(directory)
}
impl Workspace {
    pub(crate) fn reconcile_commit(
        &self,
        submission: &Submission,
        attempt: &CommitAttempt,
        outcome: &CommitOutcomeWire,
        deadline: Instant,
    ) -> Result<u64, WorkspaceError> {
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let _writer = host.writer()?;
        let captured = submission.capture()?;
        let (current, baseline) = {
            let state = self.state()?;
            self.available(&state)?;
            if state.base != captured.context.effective_root
                || state
                    .branch
                    .as_ref()
                    .is_none_or(|context| !Arc::ptr_eq(context, &captured.context))
                || state.generation != captured.generation + 1
                || state
                    .submission
                    .as_ref()
                    .is_none_or(|s| !std::ptr::eq(s.as_ref(), submission))
                || state.dirty_inodes > 128
                || state.completion.as_ref().map(|c| c.generation)
                    != (state.dirty_inodes > 0).then_some(state.generation)
            {
                return Err(WorkspaceError::Io);
            }
            (
                Current {
                    root: state.overlay.clone().ok_or(WorkspaceError::Io)?,
                    generation: state.generation,
                    revision: state.revision,
                    count: state.dirty_inodes,
                },
                state.baseline,
            )
        };
        let revision = current
            .revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        let next_baseline = baseline.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        let mut lease = host.payloads.window(3, 4)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        let (head, canonical) = match outcome {
            CommitOutcomeWire::Committed(commit) => (Some(commit.commit), commit.root),
            CommitOutcomeWire::UpToDate { head, root } => (*head, *root),
        };
        let mut after = 0;
        let mut seen = 0;
        let mut phase = 0;
        let root = attempt.root.build_ordered(
            |window| loop {
                if let Some((serial, inode)) = current.next(after, window, deadline)? {
                    if seen == current.count {
                        return Err(WorkspaceError::Io);
                    }
                    after = serial;
                    seen += 1;
                    match (phase, inode) {
                        (0, _) => {
                            return Ok(Some(Cell::new(
                                &metadata_pages::dirty_key(current.generation, serial),
                                &[1],
                            )?))
                        }
                        (1, Dirty::File(inode)) => {
                            let inode =
                                canonical_inode(submission, serial, inode, window, deadline)?;
                            return Ok(Some(Cell::new(
                                &metadata_pages::inode_key(serial),
                                &inode.value(),
                            )?));
                        }
                        (2, Dirty::Directory(directory)) => {
                            let directory = canonical_directory(
                                submission, serial, directory, canonical, window, deadline,
                            )?;
                            return Ok(Some(Cell::new(
                                &metadata_pages::namespace_key(serial),
                                &directory.value(),
                            )?));
                        }
                        _ => continue,
                    }
                }
                if seen != current.count {
                    return Err(WorkspaceError::Io);
                }
                if phase == 2 {
                    return Ok(None);
                }
                phase += 1;
                after = 0;
                seen = 0;
            },
            window,
            deadline,
        )?;
        attempt.root.seal(root, window, deadline)?;
        let mut next = attempt
            .next
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .take()
            .ok_or(WorkspaceError::Io)?;
        next.snapshot.branch.head_commit = head;
        next.snapshot.head_root = head.map(|_| canonical);
        next.snapshot.effective_root = canonical;
        let next = Arc::new(next);
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let mut status = attempt.status.lock().map_err(|_| WorkspaceError::Io)?;
        let retired = {
            let mut state = self.state()?;
            self.available(&state)?;
            if state.revision != current.revision
                || state.baseline != baseline
                || state.generation != current.generation
                || state
                    .overlay
                    .as_ref()
                    .is_none_or(|r| !Arc::ptr_eq(r, &current.root))
                || state
                    .submission
                    .as_ref()
                    .is_none_or(|s| !std::ptr::eq(s.as_ref(), submission))
            {
                return Err(WorkspaceError::Busy);
            }
            let old_root = std::mem::replace(
                &mut state.overlay,
                (root != PageRef::NULL).then(|| attempt.root.clone()),
            );
            let old_branch = state.branch.replace(next);
            state.base = canonical;
            state.baseline = next_baseline;
            state.revision = revision;
            status.installed_revision = Some(revision);
            // Fixed local bookkeeping is acquired before any publication changes
            // so a poisoned status cannot hide an installed canonical head.
            (old_root, old_branch)
        };
        drop(status);
        drop(retired);
        Ok(revision)
    }
}
