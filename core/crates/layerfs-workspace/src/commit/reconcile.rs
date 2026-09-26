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
/// One record the successor root must carry: a counted dirty identity, or an
/// identity no name binds any more that still has a live local owner.
enum Frontier {
    Counted(Dirty),
    /// No dirty key and no declaration, but its record still moves to the
    /// successor root so a live handle and a pinned reader keep reading their
    /// own version locally instead of asking the service for a name that is gone.
    /// Either this generation created the identity, or the attached base owns it
    /// and a live local owner still addresses the record this generation left.
    Unbound(Inode),
}
struct Current {
    root: Arc<RootOwner>,
    generation: u64,
    revision: u64,
    count: usize,
    /// Serials of identities no name binds any more that still have a live local
    /// owner, in increasing order: the ones this generation created, and the
    /// base identities a replacement or removal left with a record to keep.
    unbound: Vec<u64>,
}
impl Current {
    /// The next record of this phase in serial order. Only the inode phase also
    /// carries the unbound identities; the dirty-key and namespace phases walk
    /// exactly the counted frontier, so `count` keeps describing them.
    fn next(
        &self,
        submission: &Submission,
        phase: u8,
        after: u64,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Option<(u64, Frontier)>, WorkspaceError> {
        let counted = self.counted(submission, after, window, deadline)?;
        let unbound = (phase == 1)
            .then(|| self.unbound.iter().copied().find(|serial| *serial > after))
            .flatten();
        let Some(unbound) = unbound else {
            return Ok(counted.map(|(serial, dirty)| (serial, Frontier::Counted(dirty))));
        };
        if counted
            .as_ref()
            .is_some_and(|(serial, _)| *serial < unbound)
        {
            return Ok(counted.map(|(serial, dirty)| (serial, Frontier::Counted(dirty))));
        }
        let cell = self
            .root
            .arena
            .find(
                self.root.root()?,
                &metadata_pages::inode_key(unbound),
                window,
                deadline,
            )?
            .ok_or(WorkspaceError::Io)?;
        let inode = Inode::parse(cell.value())?;
        let captured = submission.capture()?.generation;
        if (inode.generation != self.generation && inode.generation != captured)
            || inode.revision > self.revision
        {
            return Err(WorkspaceError::Io);
        }
        Ok(Some((unbound, Frontier::Unbound(inode))))
    }
    fn counted(
        &self,
        submission: &Submission,
        after: u64,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Option<(u64, Dirty)>, WorkspaceError> {
        let mut cursor = after;
        loop {
            let key = metadata_pages::dirty_key(self.generation, cursor);
            let Some(cell) =
                self.root
                    .arena
                    .next(self.root.root()?, &key, cursor != 0, window, deadline)?
            else {
                return Ok(None);
            };
            if cell.key_len != 17 || cell.key()[..9] != key[..9] {
                return Ok(None);
            }
            let serial = get(cell.key(), 9)?;
            if serial <= cursor || cell.value() != [1] {
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
                if inode.constructs_file() && !prior_inode(submission, serial, window, deadline)? {
                    // This generation created the identity and no name binds it
                    // any more, so it has no canonical identity and no dirty
                    // frontier: the successor names nothing for it.
                    cursor = serial;
                    continue;
                }
                return Ok(Some((serial, Dirty::Inode(inode))));
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
            return Ok(Some((serial, Dirty::Directory(directory))));
        }
    }
}
/// True when the captured root already declared this identity as an existing
/// one. A fresh record there is still this delta's own creation.
fn prior_inode(
    submission: &Submission,
    serial: u64,
    window: &mut Window,
    deadline: Instant,
) -> Result<bool, WorkspaceError> {
    let captured = submission.capture()?;
    Ok(captured
        .root
        .arena
        .find(
            captured.root.root()?,
            &metadata_pages::inode_key(serial),
            window,
            deadline,
        )?
        .is_some_and(|cell| Inode::parse(cell.value()).is_ok_and(|inode| !inode.fresh)))
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
        || original.symlink != inode.symlink
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
    /// Reconciles one published Commit outcome onto the live frontier.
    ///
    /// The successor build reads one snapshot of the live frontier and writes
    /// only the attempt root's own pages, so it runs **without** the writer
    /// gate: later writes publish while it runs. The two ordering points - the
    /// state read that freezes the rebuild input, and the install that
    /// publishes the successor - hold the gate briefly and wait for a current
    /// holder deadline-bounded instead of refusing with `Busy`. If the live
    /// frontier moved while the successor was built, the built tree describes
    /// a stale frontier, so the loop rebuilds from the newer one; the rebuild
    /// is bounded by the operation deadline, and only the converged tree is
    /// sealed and installed. Frozen-root custody is unchanged: the snapshot
    /// root is an `Arc` the build holds, so the pages it reads stay alive for
    /// exactly as long as the build needs them.
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
        let captured = submission.capture()?;
        let (head, canonical) = match outcome {
            CommitOutcomeWire::Committed(commit) => (Some(commit.commit), commit.root),
            CommitOutcomeWire::UpToDate { head, root } => (*head, *root),
        };
        let next = match attempt.next.lock().map_err(|_| WorkspaceError::Io)?.take() {
            Some(mut next) => {
                next.snapshot.branch.head_commit = head;
                next.snapshot.head_root = head.map(|_| canonical);
                next.snapshot.effective_root = canonical;
                next
            }
            // A resumed attempt's first pass consumed its successor; the same
            // capture and the same known outcome rebuild the identical context.
            None => CommitAttempt::successor(self, submission, head, canonical)?,
        };
        let next = Arc::new(next);
        loop {
            // Ordering point one: the state read that freezes this rebuild's
            // input. The hold is short - one state read and the unbound list.
            let (current, baseline, revision, next_baseline) = {
                let _writer = host.writer_until(deadline)?;
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
                // A record is carried only while a live local owner can still
                // address it. An identity whose handle and lookup references
                // are both gone has no owner left to serve, so nothing is
                // carried for it and the arena can be reclaimed.
                let mut unbound = state.unbound.clone();
                unbound.extend(state.carried.iter().copied().filter(|serial| {
                    state.nodes.iter().any(|node| {
                        node.attr.serial == *serial
                            && (node.lookups > 0 || node.projection_lookups > 0 || node.handles > 0)
                    })
                }));
                unbound.sort_unstable();
                unbound.dedup();
                let current = Current {
                    root: state.overlay.clone().ok_or(WorkspaceError::Io)?,
                    generation: state.generation,
                    revision: state.revision,
                    count: state.dirty_inodes,
                    unbound,
                };
                let baseline = state.baseline;
                let revision = current
                    .revision
                    .checked_add(1)
                    .ok_or(WorkspaceError::Capacity)?;
                let next_baseline = baseline.checked_add(1).ok_or(WorkspaceError::Capacity)?;
                (current, baseline, revision, next_baseline)
            };
            // The ungated build. Pages an earlier iteration of this loop wrote
            // but never sealed stay in the attempt root's temporary chain; the
            // one seal below pops every one of them, so a rebuilt iteration
            // leaves no orphan behind.
            let mut lease = host.payloads.window(3, 4)?;
            let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
            let root = {
                let mut after = 0;
                let mut seen = 0;
                let mut phase = 0;
                let built = attempt.root.build_ordered(
                    |window| loop {
                        if let Some((serial, frontier)) =
                            current.next(submission, phase, after, window, deadline)?
                        {
                            after = serial;
                            let dirty = match frontier {
                                Frontier::Unbound(inode) => {
                                    // Emitted exactly as this generation left it: the
                                    // identity has no canonical version to substitute.
                                    return Ok(Some(Cell::new(
                                        &metadata_pages::inode_key(serial),
                                        &inode.value(),
                                    )?));
                                }
                                Frontier::Counted(dirty) => dirty,
                            };
                            if seen == current.count {
                                return Err(WorkspaceError::Io);
                            }
                            seen += 1;
                            match (phase, dirty) {
                                (0, _) => {
                                    return Ok(Some(Cell::new(
                                        &metadata_pages::dirty_key(current.generation, serial),
                                        &[1],
                                    )?))
                                }
                                (1, Dirty::Inode(inode)) => {
                                    let inode = canonical_inode(
                                        submission, serial, inode, window, deadline,
                                    )?;
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
                built
            };
            crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
            let mut status = attempt.status.lock().map_err(|_| WorkspaceError::Io)?;
            // Ordering point two: the install. The hold is short - one state
            // compare and the swap. A frontier that moved under the build
            // converges by rebuilding from the newer root, never by refusing.
            let _writer = host.writer_until(deadline)?;
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
                drop(state);
                drop(status);
                if Instant::now() >= deadline {
                    return Err(WorkspaceError::Deadline);
                }
                continue;
            }
            // No writer can move this frontier before publication. Seal only
            // this converged tree; stale builds remain temporary pages on the
            // attempt root and are cleaned by this one seal.
            attempt.root.seal(root, window, deadline)?;
            let old_root = std::mem::replace(
                &mut state.overlay,
                (root != PageRef::NULL).then(|| attempt.root.clone()),
            );
            let old_branch = state.branch.replace(next.clone());
            state.base = canonical;
            state.baseline = next_baseline;
            state.revision = revision;
            let accepted = submission
                .state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .declared
                .clone();
            state.declared_committed(&accepted);
            status.installed_revision = Some(revision);
            // Fixed local bookkeeping is acquired before any publication
            // changes so a poisoned status cannot hide an installed canonical
            // head.
            drop(state);
            drop(status);
            drop(old_root);
            drop(old_branch);
            return Ok(revision);
        }
    }
}
