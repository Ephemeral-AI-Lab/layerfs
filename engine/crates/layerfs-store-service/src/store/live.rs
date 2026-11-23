//! A merged install over rows written after the capture.
//!
//! Rows of the active generation hold absolute facts (size, inherited
//! cutoff, link and entry counts, inherited bits) computed against the base
//! under them. Install moves only the base, so a merge that changes a serial
//! or a directory's listing under such a row would leave it wrong: bytes cut
//! off or zero-filled, a non-empty directory counted empty. Before a merge is
//! published (or, conflicted, installed) the Commit holds the Workspace's
//! mutations and refuses with `LiveConflict` when an active row overlaps
//! what the merge changed; nothing is published and the capture folds back.
//! Without overlap the hold lasts through the publication and install, so no
//! checked row changes before the new base is in place.
use super::{BoundWorkspace, CommitError};
use crate::MutationHold;
use layerfs_content::{
    compare::{inode_differences, tree::locate, DifferenceWork, TreeTable},
    filesystem::FilesystemRootId,
    AuthenticatedObjects, ObjectId,
};
use std::collections::BTreeSet;

/// Paths a `LiveConflict` names, at most.
pub const LIVE_CONFLICT_PATHS: usize = 16;
/// Directories one path search lists, at most.
const LOCATE_DIRECTORIES: usize = 4096;

/// Serials whose stored inode value (content, metadata, link count, a
/// directory's listing) differs between two roots, or that only one holds,
/// in serial order. Work follows the difference, not the tree.
pub(super) fn changed_serials(
    reader: &dyn AuthenticatedObjects,
    from: FilesystemRootId,
    to: FilesystemRootId,
) -> Result<Vec<u64>, CommitError> {
    let mut changed = Vec::new();
    if from == to {
        return Ok(changed);
    }
    let (before, after) = (
        TreeTable::open(reader, from).map_err(CommitError::from)?,
        TreeTable::open(reader, to).map_err(CommitError::from)?,
    );
    inode_differences(
        reader,
        Some(&before),
        &after,
        &mut DifferenceWork::default(),
        |serial, _, _| {
            changed.push(serial);
            Ok(())
        },
    )
    .map_err(CommitError::from)?;
    Ok(changed)
}

/// The sorted union of two sorted serial lists.
pub(super) fn union(first: &[u64], second: &[u64]) -> Vec<u64> {
    let mut all: Vec<u64> = first.iter().chain(second).copied().collect();
    all.sort_unstable();
    all.dedup();
    all
}

impl BoundWorkspace {
    /// Holds this Workspace's mutations, then reads the active generation's
    /// changed rows a window at a time: an inode row of a changed serial, or
    /// a name binding under a changed directory, overlaps the merge. A row
    /// carried for an mtime-only capture and not written since holds no data
    /// of its own: it is no overlap, and the install deletes it when its
    /// serial changed. Returns the hold and up to `LIVE_CONFLICT_PATHS`
    /// overlapping serials; empty means the install may proceed.
    pub(super) fn hold_live(
        &self,
        changed: &[u64],
    ) -> Result<(MutationHold, Vec<u64>), CommitError> {
        let route = self.route();
        let hold = self
            .owner
            .hold_mutations(route)
            .map_err(CommitError::Owner)?;
        let hit = |serial: u64| changed.binary_search(&serial).is_ok();
        let mut found = BTreeSet::new();
        let mut after = 0;
        while found.len() < LIVE_CONFLICT_PATHS {
            let window = self
                .owner
                .active_inodes(route, after)
                .map_err(CommitError::Owner)?;
            let Some(last) = window.last() else { break };
            after = last.serial;
            found.extend(window.iter().map(|row| row.serial).filter(|s| hit(*s)));
        }
        let mut cursor = None;
        while found.len() < LIVE_CONFLICT_PATHS {
            let window = self
                .owner
                .active_directory_entries(route, cursor.clone())
                .map_err(CommitError::Owner)?;
            let Some(last) = window.last() else { break };
            cursor = Some((last.parent, last.name.clone()));
            found.extend(window.iter().map(|row| row.parent).filter(|p| hit(*p)));
        }
        Ok((hold, found.into_iter().take(LIVE_CONFLICT_PATHS).collect()))
    }

    /// The refusal of a merged install that overlaps `serials`, naming their
    /// paths in `ours` (the Workspace's own constructed root), else in the
    /// merged root. Naming is a bounded search and best effort: a path it
    /// does not find, or a read that fails, is left out, never the refusal.
    pub(super) fn live_conflict(
        &self,
        scope: ObjectId,
        ours: FilesystemRootId,
        merged: FilesystemRootId,
        serials: &[u64],
    ) -> CommitError {
        let ports = self.store.ports(scope);
        let reader = &*ports;
        let mut wanted: BTreeSet<u64> = serials.iter().copied().collect();
        let mut paths = Vec::new();
        for root in [ours, merged] {
            if wanted.is_empty() {
                break;
            }
            let found = TreeTable::open(reader, root)
                .and_then(|tree| locate(reader, &tree, &wanted, LOCATE_DIRECTORIES))
                .unwrap_or_default();
            for (serial, path) in found {
                wanted.remove(&serial);
                paths.push(path);
            }
        }
        paths.sort();
        CommitError::LiveConflict { paths }
    }
}
