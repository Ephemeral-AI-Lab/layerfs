//! Prepare immutable G1 facts off the state gate; install only matching live rows.
use super::{
    active::{scan_dirty, scan_extents},
    completion::CommitAttempt,
};
use crate::{
    backing::{
        active::{dirty_key, inode_key, Extent, HotInode},
        budget::Charge,
    },
    overlay::snapshot::{Captured, Submission},
    NodeKind, Workspace, WorkspaceError,
};
use layerfs_bridge::contract::{CommitOutcomeWire, Root};
use std::{collections::BTreeMap, mem::size_of, sync::Arc, time::Instant};

struct PreparedRow {
    serial: u64,
    original: HotInode,
    content: Root,
    metadata: Root,
    deletions: Vec<Vec<u8>>,
    _charge: Charge,
}

fn prepare(
    workspace: &Workspace,
    captured: &Captured,
    submission: &Submission,
    deadline: Instant,
) -> Result<(Vec<PreparedRow>, Charge), WorkspaceError> {
    let view = captured.active_view()?;
    let (dirty, _dirty_charge) = scan_dirty(workspace, captured, &view)?;
    let rows_charge = workspace.host.budget.reserve(
        dirty
            .len()
            .checked_mul(size_of::<PreparedRow>())
            .ok_or(WorkspaceError::Capacity)?,
    )?;
    let mut rows = Vec::with_capacity(dirty.len());
    for (serial, original) in dirty {
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let (extents, _extents_charge) = if original.kind == NodeKind::File {
            scan_extents(workspace, &view, serial, original)?
        } else {
            (Vec::new(), workspace.host.budget.reserve(0)?)
        };
        let charge = workspace.host.budget.reserve(
            extents
                .len()
                .checked_mul(192)
                .ok_or(WorkspaceError::Capacity)?,
        )?;
        let mut deletions = Vec::with_capacity(extents.len() * 2);
        for extent in extents {
            if original.storage == 2 {
                deletions.push(Extent::key(serial, extent.start).to_vec());
            }
            if let Some(key) = extent.inverse_key(serial) {
                deletions.push(key);
            }
        }
        let (content, metadata) = if original.kind == NodeKind::Directory {
            ([0; 32], [0; 32])
        } else {
            let host = workspace
                .host
                .metadata
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            let mut lease = host.payloads.window(1, 3)?;
            let cell = captured
                .root
                .arena
                .find(
                    submission.result_ref()?,
                    &crate::backing::metadata_pages::result_key(serial),
                    lease.window.as_mut().ok_or(WorkspaceError::Io)?,
                    deadline,
                )?
                .ok_or(WorkspaceError::Io)?;
            let saved = cell.value();
            if saved.len() != 80
                || u64::from_be_bytes(saved[..8].try_into().map_err(|_| WorkspaceError::Io)?)
                    != original.revision
                || u64::from_be_bytes(saved[8..16].try_into().map_err(|_| WorkspaceError::Io)?)
                    != original.length
            {
                return Err(WorkspaceError::Io);
            }
            (
                saved[16..48].try_into().map_err(|_| WorkspaceError::Io)?,
                saved[48..80].try_into().map_err(|_| WorkspaceError::Io)?,
            )
        };
        rows.push(PreparedRow {
            serial,
            original,
            content,
            metadata,
            deletions,
            _charge: charge,
        });
    }
    Ok((rows, rows_charge))
}

pub(super) fn reconcile(
    workspace: &Workspace,
    submission: &Submission,
    attempt: &CommitAttempt,
    outcome: &CommitOutcomeWire,
    deadline: Instant,
) -> Result<u64, WorkspaceError> {
    crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
    let captured = submission.capture()?;
    let (rows, _rows_charge) = prepare(workspace, captured, submission, deadline)?;
    let bytes = rows.iter().try_fold(0usize, |bytes, row| {
        row.deletions.iter().try_fold(
            bytes
                .checked_add(2 * 128 + 17 + 9 + 416)
                .ok_or(WorkspaceError::Capacity)?,
            |bytes, key| {
                bytes
                    .checked_add(128 + key.len())
                    .ok_or(WorkspaceError::Capacity)
            },
        )
    })?;
    let _updates_charge = workspace.host.budget.reserve(bytes)?;
    let entries = rows.iter().try_fold(0usize, |count, row| {
        count
            .checked_add(row.deletions.len() + 2)
            .ok_or(WorkspaceError::Capacity)
    })?;
    let _ordered_charge = workspace.host.budget.reserve(
        entries
            .checked_mul(size_of::<(Vec<u8>, Option<Vec<u8>>)>())
            .ok_or(WorkspaceError::Capacity)?,
    )?;
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
        None => CommitAttempt::successor(workspace, submission, head, canonical)?,
    };
    let next = Arc::new(next);
    let active = workspace
        .inner
        .active
        .as_ref()
        .ok_or(WorkspaceError::Unsupported)?;
    let mut state = workspace.state()?;
    workspace.available(&state)?;
    if state.base != captured.context.effective_root
        || state
            .branch
            .as_ref()
            .is_none_or(|branch| !Arc::ptr_eq(branch, &captured.context))
        || state.generation != captured.generation + 1
        || state
            .submission
            .as_ref()
            .is_none_or(|held| !std::ptr::eq(held.as_ref(), submission))
        || active.generation_revision()? != (state.generation, state.revision)
    {
        return Err(WorkspaceError::Io);
    }
    let revision = state
        .revision
        .checked_add(1)
        .ok_or(WorkspaceError::Capacity)?;
    let baseline = state
        .baseline
        .checked_add(1)
        .ok_or(WorkspaceError::Capacity)?;
    let mut updates = BTreeMap::new();
    for row in &rows {
        updates.insert(dirty_key(captured.generation, row.serial).to_vec(), None);
        let mut current = HotInode::parse(
            &active
                .get(&inode_key(row.serial))?
                .ok_or(WorkspaceError::Io)?,
        )?;
        if current.kind != row.original.kind {
            return Err(WorkspaceError::Io);
        }
        if current.revision == row.original.revision && current.kind == NodeKind::File {
            for key in &row.deletions {
                updates.insert(key.clone(), None);
            }
            current.storage = u8::from(current.length > 0);
            current.inline = if current.length > 0 {
                [Some(Extent::base(0, current.length)), None, None, None]
            } else {
                [None; 4]
            };
        }
        if current.kind != NodeKind::Directory {
            current.base = row.content;
            current.metadata = row.metadata;
        }
        current.fresh =
            row.original.fresh && row.original.kind == NodeKind::File && row.original.links == 0;
        current.generation = state.generation;
        current.revision = revision;
        updates.insert(
            inode_key(row.serial).to_vec(),
            Some(current.value()?.to_vec()),
        );
    }
    let mut status = attempt.status.lock().map_err(|_| WorkspaceError::Io)?;
    let publication = active.publish_reconcile(&updates.into_iter().collect::<Vec<_>>())?;
    if publication.revision != revision {
        return Err(WorkspaceError::Io);
    }
    let old_branch = state.branch.replace(next);
    state.base = canonical;
    state.baseline = baseline;
    state.revision = revision;
    for row in rows {
        if let Ok(node) = state.node_mut(row.serial) {
            node.original.size = row.original.length;
            node.original.mode = row.original.mode;
            node.original.mtime_seconds = row.original.seconds;
            node.original.mtime_nanoseconds = row.original.nanos;
            node.original.references = u64::from(row.original.links);
            if row.original.kind != NodeKind::Directory {
                node.content = row.content;
                node.metadata = row.metadata;
            }
            node.baseline = baseline;
        }
    }
    let accepted = &submission
        .state
        .lock()
        .map_err(|_| WorkspaceError::Io)?
        .declared;
    state.declared.retain(|serial| !accepted.contains(serial));
    status.installed_revision = Some(revision);
    drop(state);
    drop(status);
    drop(old_branch);
    captured.release_active()?;
    active.maintain_until(deadline)?;
    if let Some(error) = publication.cleanup_error {
        return Err(error);
    }
    Ok(revision)
}
