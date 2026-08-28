use super::super::managed_edit_legacy::native_parent;
use super::super::materialize_legacy::metadata;
use super::super::session_legacy::{VfsError, VfsResult};
use super::super::OperationCounters;
use layerfs_core::inode::InodeKind;
use layerfs_core::CanonicalPath;
use layerfs_materialization::driver::{DriverError, ProjectionWorkspace};
use layerfs_storage::scratch::DiskNamespace;
use layerfs_storage::Engine;

use super::primitives::{checked_add, decode_snapshot, mark_ambiguous};
use super::scratch::{RefreshScratch, Snapshot};
#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_hard_links(
    engine: &Engine,
    scratch: &RefreshScratch,
    native: &dyn ProjectionWorkspace,
    touched_groups: &DiskNamespace<'_>,
    new_paths: &DiskNamespace<'_>,
    new_representatives: &DiskNamespace<'_>,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let queue = scratch.table("finalize-links")?;
    touched_groups.for_each_key(|inode| queue.enqueue_once(inode, &[]))?;
    while let Some((inode, _)) = queue.pop_pending()? {
        let path = new_representatives
            .get(&inode)?
            .ok_or(VfsError::InvalidState)?;
        let after = new_paths
            .get(&path)?
            .ok_or(VfsError::InvalidState)
            .and_then(|bytes| decode_snapshot(&bytes))?;
        let root = native.root_directory()?;
        let path = CanonicalPath::from_bytes(&path)?;
        let (parent, name) = native_parent(native, root, &path)?;
        let identity = native.identity_at(parent.as_ref(), name)?;
        *visible = true;
        native.finish_hard_link_at(
            parent.as_ref(),
            name,
            &identity,
            &metadata(engine, after.metadata_root, counters)?,
        )?;
        counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
        counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_delete(
    native: &dyn ProjectionWorkspace,
    authority: &DiskNamespace<'_>,
    new_representatives: &DiskNamespace<'_>,
    removed_authority: &DiskNamespace<'_>,
    path: &CanonicalPath,
    before: Snapshot,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    if path.is_root() {
        return Err(DriverError::Unsupported.into());
    }
    let root = native.root_directory()?;
    let (parent, name) = native_parent(native, root, path)?;
    let identity = native.identity_at(parent.as_ref(), name)?;
    if before.kind == InodeKind::RegularFile
        && authority.get(&identity)?.as_deref() != Some(before.inode.as_bytes())
    {
        return Err(VfsError::InvalidState);
    }
    let result = match before.kind {
        InodeKind::RegularFile => native.unlink_regular_at(parent.as_ref(), name, &identity),
        InodeKind::Symlink => native.unlink_symlink_at(parent.as_ref(), name, &identity),
        InodeKind::Directory => native.remove_directory_at(parent.as_ref(), name, &identity),
    };
    match result {
        Ok(()) => *visible = true,
        Err(error) => {
            mark_ambiguous(visible, &error);
            return Err(error.into());
        }
    }
    if before.kind == InodeKind::RegularFile
        && new_representatives.get(before.inode.as_bytes())?.is_none()
    {
        removed_authority.put(&identity, before.inode.as_bytes())?;
    }
    counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
    counters.native.remove_calls = checked_add(counters.native.remove_calls, 1)?;
    Ok(())
}
