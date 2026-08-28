use super::super::resolver_legacy::namespace;
use super::super::session_legacy::{VfsError, VfsResult};
use super::super::{AcceptedSplice, OperationCounters};
use layerfs_core::inode::{diff_inode_table_entries, InodeKind, InodeTableRoot};
use layerfs_core::CanonicalPath;
use layerfs_materialization::driver::{DriverError, ProjectionWorkspace};
use layerfs_storage::refs::RefState;
use layerfs_storage::scratch::DiskNamespace;
use layerfs_storage::Engine;

use super::delta::{
    encode_inode_diff, inode_diff_changes_namespace, namespace_delta_paths, record, snapshot_value,
    target_snapshot,
};
use super::directory::{apply_create, apply_directories, apply_renames, apply_update};
use super::entries::{apply_delete, finalize_hard_links};
use super::primitives::{decode_snapshot, encode_snapshot, has_mapped_ancestor, ordered_path};
use super::scratch::RefreshScratch;
use super::topology::{align_old_paths, plan_renames, rotate_topology, selective_paths};
pub(crate) fn apply(
    engine: &Engine,
    native: &dyn ProjectionWorkspace,
    authority: &DiskNamespace<'_>,
    topology: &DiskNamespace<'_>,
    transition: (&RefState, &RefState, Option<&AcceptedSplice>),
    visible: &mut bool,
) -> VfsResult<OperationCounters> {
    let (old, target, accepted) = transition;
    let scratch = RefreshScratch::create(engine)?;
    let old_paths = scratch.table("old-paths")?;
    let new_paths = scratch.table("new-paths")?;
    let old_representatives = scratch.table("old-inodes")?;
    let new_representatives = scratch.table("new-inodes")?;
    let renames = scratch.table("renames")?;
    let rename_targets = scratch.table("rename-targets")?;
    let rename_queue = scratch.table("rename-queue")?;
    let inode_changes = scratch.table("inode-diff")?;
    let mut counters = OperationCounters::default();
    let old_namespace = namespace(engine, old.root)?;
    let new_namespace = namespace(engine, target.root)?;
    let mut scratch_error = None;
    let inode_diff = diff_inode_table_entries(
        engine,
        InodeTableRoot(old_namespace.inode_table_root),
        InodeTableRoot(new_namespace.inode_table_root),
        |diff| {
            if let Err(error) = inode_changes.put(
                diff.inode.as_bytes(),
                &encode_inode_diff(diff.before, diff.after),
            ) {
                scratch_error = Some(error);
                return Err(layerfs_core::CoreError::Io);
            }
            Ok(())
        },
    );
    if let Some(error) = scratch_error {
        return Err(error.into());
    }
    let inode_diff = inode_diff?;
    counters.root_diff_nodes = counters
        .root_diff_nodes
        .checked_add(inode_diff.nodes_read)
        .ok_or(VfsError::InvalidState)?;
    counters.add_inode_table(inode_diff)?;
    let old_root_record = record(
        engine,
        InodeTableRoot(old_namespace.inode_table_root),
        old_namespace.root_directory_inode,
        &mut counters,
    )?;
    let old_root = snapshot_value(old_namespace.root_directory_inode, old_root_record);
    let new_root = target_snapshot(engine, old_root, &inode_changes)?;
    let old_edges = scratch.table("old-edges")?;
    let new_edges = scratch.table("new-edges")?;
    let namespace_changed = old_namespace.root_directory_inode
        != new_namespace.root_directory_inode
        || old_root.content_root != new_root.content_root
        || inode_diff_changes_namespace(engine, &inode_changes)?;
    if !namespace_changed {
        selective_paths(
            engine,
            &scratch,
            topology,
            old_namespace.root_directory_inode,
            &old_paths,
            &old_representatives,
            &new_paths,
            &new_representatives,
            &inode_changes,
            old_root,
            new_root,
        )?;
    } else if old_namespace.root_directory_inode == new_namespace.root_directory_inode {
        namespace_delta_paths(
            engine,
            &scratch,
            topology,
            old_namespace.root_directory_inode,
            InodeTableRoot(old_namespace.inode_table_root),
            InodeTableRoot(new_namespace.inode_table_root),
            &old_paths,
            &old_representatives,
            &new_paths,
            &new_representatives,
            &inode_changes,
            &old_edges,
            &new_edges,
            old_root,
            new_root,
            &mut counters,
        )?;
    } else {
        return Err(DriverError::Unsupported.into());
    }
    plan_renames(
        &scratch,
        &old_paths,
        &new_paths,
        &old_representatives,
        &new_representatives,
        &renames,
        &rename_targets,
        &rename_queue,
    )?;

    let aligned_old = scratch.table("aligned-old")?;
    let aligned_old_representatives = scratch.table("aligned-old-inodes")?;
    align_old_paths(
        &old_paths,
        &renames,
        &aligned_old,
        &aligned_old_representatives,
    )?;

    let plan = scratch.table("plan")?;
    let pre_directories = scratch.table("pre-directories")?;
    let post_directories = scratch.table("post-directories")?;
    let creates = scratch.table("creates")?;
    let updates = scratch.table("updates")?;
    let deletes = scratch.table("deletes")?;
    aligned_old.for_each_key(|path| plan.enqueue_once(path, &[]))?;
    new_paths.for_each_key(|path| plan.enqueue_once(path, &[]))?;
    while let Some((path, _)) = plan.pop_pending()? {
        counters.plan_rows = counters
            .plan_rows
            .checked_add(1)
            .ok_or(VfsError::InvalidState)?;
        let before = aligned_old
            .get(&path)?
            .map(|bytes| decode_snapshot(&bytes))
            .transpose()?;
        let after = new_paths
            .get(&path)?
            .map(|bytes| decode_snapshot(&bytes))
            .transpose()?;
        if before.map(encode_snapshot) == after.map(encode_snapshot) {
            continue;
        }
        counters.changed_paths = counters
            .changed_paths
            .checked_add(1)
            .ok_or(VfsError::InvalidState)?;
        match (before, after) {
            (None, Some(after)) if after.kind == InodeKind::Directory => {
                let queue = if has_mapped_ancestor(&path, &rename_targets)? {
                    &post_directories
                } else {
                    &pre_directories
                };
                queue.enqueue_once(&ordered_path(&path, false)?, &path)?;
            }
            (None, Some(_)) => creates.enqueue_once(&path, &path)?,
            (Some(_), None) => {
                deletes.enqueue_once(&ordered_path(&path, true)?, &path)?;
            }
            (Some(before), Some(after))
                if before.kind == InodeKind::Directory
                    && (before.kind != after.kind || before.inode != after.inode) =>
            {
                return Err(DriverError::Unsupported.into());
            }
            (Some(_), Some(_)) => {
                if accepted.is_some_and(|splice| splice.path().as_bytes() == path) {
                    updates.enqueue_once(&[], &path)?;
                } else {
                    updates.enqueue_once(&path, &path)?;
                }
            }
            (None, None) => return Err(VfsError::InvalidState),
        }
    }

    let created_representatives = scratch.table("created-inodes")?;
    let touched_groups = scratch.table("hard-link-groups")?;
    let processed_inodes = scratch.table("processed-inodes")?;
    let removed_authority = scratch.table("removed-authority")?;

    apply_directories(
        engine,
        native,
        &pre_directories,
        &new_paths,
        visible,
        &mut counters,
    )?;
    apply_renames(native, authority, &rename_queue, visible, &mut counters)?;
    apply_directories(
        engine,
        native,
        &post_directories,
        &new_paths,
        visible,
        &mut counters,
    )?;

    while let Some((_, path)) = creates.pop_pending()? {
        let after = new_paths
            .get(&path)?
            .ok_or(VfsError::InvalidState)
            .and_then(|bytes| decode_snapshot(&bytes))?;
        apply_create(
            engine,
            native,
            authority,
            &aligned_old_representatives,
            &created_representatives,
            &touched_groups,
            &CanonicalPath::from_bytes(&path)?,
            after,
            visible,
            &mut counters,
        )?;
    }

    while let Some((_, path)) = updates.pop_pending()? {
        let before = aligned_old
            .get(&path)?
            .ok_or(VfsError::InvalidState)
            .and_then(|bytes| decode_snapshot(&bytes))?;
        let after = new_paths
            .get(&path)?
            .ok_or(VfsError::InvalidState)
            .and_then(|bytes| decode_snapshot(&bytes))?;
        apply_update(
            engine,
            &scratch,
            native,
            authority,
            &aligned_old_representatives,
            &new_representatives,
            &created_representatives,
            &touched_groups,
            &processed_inodes,
            &removed_authority,
            &CanonicalPath::from_bytes(&path)?,
            before,
            after,
            accepted,
            visible,
            &mut counters,
        )?;
    }

    finalize_hard_links(
        engine,
        &scratch,
        native,
        &touched_groups,
        &new_paths,
        &new_representatives,
        visible,
        &mut counters,
    )?;

    while let Some((_, path)) = deletes.pop_pending()? {
        let before = aligned_old
            .get(&path)?
            .ok_or(VfsError::InvalidState)
            .and_then(|bytes| decode_snapshot(&bytes))?;
        apply_delete(
            native,
            authority,
            &new_representatives,
            &removed_authority,
            &CanonicalPath::from_bytes(&path)?,
            before,
            visible,
            &mut counters,
        )?;
    }
    removed_authority.for_each_key(|key| authority.remove(key))?;

    native.revalidate_root_binding()?;
    if namespace_changed {
        rotate_topology(topology, &old_edges, &new_edges)?;
    }
    let scratch = scratch.table.finish()?;
    counters.plan_scratch_high_water_bytes = scratch.high_water_bytes;
    super::super::add_scratch(&mut counters, scratch)?;
    Ok(counters)
}
