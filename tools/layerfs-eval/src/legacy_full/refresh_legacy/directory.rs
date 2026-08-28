use super::super::managed_edit_legacy::native_parent;
use super::super::materialize_legacy::metadata;
use super::super::session_legacy::{VfsError, VfsResult};
use super::super::{AcceptedSplice, NativeRoute, OperationCounters};
use layerfs_core::content::rope::{read_all, FileStateRoot, ObjectRead};
use layerfs_core::inode::InodeKind;
use layerfs_core::namespace_codec::decode_symlink;
use layerfs_core::CanonicalPath;
use layerfs_materialization::driver::{DriverError, ProjectionWorkspace};
use layerfs_storage::scratch::DiskNamespace;
use layerfs_storage::Engine;

use super::entries::apply_delete;
use super::primitives::{checked_add, decode_snapshot, mark_ambiguous};
use super::regular::refresh_regular;
use super::scratch::{RefreshScratch, Snapshot, ORDERED_PATH_PREFIX_BYTES};
pub(super) fn apply_directories(
    engine: &Engine,
    native: &dyn ProjectionWorkspace,
    queue: &DiskNamespace<'_>,
    new_paths: &DiskNamespace<'_>,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    while let Some((_, path)) = queue.pop_pending()? {
        let after = new_paths
            .get(&path)?
            .ok_or(VfsError::InvalidState)
            .and_then(|bytes| decode_snapshot(&bytes))?;
        let root = native.root_directory()?;
        let path = CanonicalPath::from_bytes(&path)?;
        let (parent, name) = native_parent(native, root, &path)?;
        let directory = match native.create_directory_at(parent.as_ref(), name) {
            Ok(directory) => {
                *visible = true;
                directory
            }
            Err(error) => {
                mark_ambiguous(visible, &error);
                return Err(error.into());
            }
        };
        let identity = native.directory_identity(directory.as_ref())?;
        *visible = true;
        native.set_entry_metadata(
            parent.as_ref(),
            name,
            &identity,
            &metadata(engine, after.metadata_root, counters)?,
        )?;
        native.sync_directory(directory.as_ref())?;
        native.sync_directory(parent.as_ref())?;
        counters.native.create_calls = checked_add(counters.native.create_calls, 1)?;
        counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
        counters.native.sync_calls = checked_add(counters.native.sync_calls, 2)?;
    }
    Ok(())
}

pub(super) fn apply_renames(
    native: &dyn ProjectionWorkspace,
    authority: &DiskNamespace<'_>,
    queue: &DiskNamespace<'_>,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    while let Some((key, payload)) = queue.pop_pending()? {
        let source_path = CanonicalPath::from_bytes(
            key.get(ORDERED_PATH_PREFIX_BYTES..)
                .ok_or(VfsError::InvalidState)?,
        )?;
        let kind = InodeKind::try_from(*payload.first().ok_or(VfsError::InvalidState)?)?;
        let expected_inode = payload.get(1..33).ok_or(VfsError::InvalidState)?;
        let target_path =
            CanonicalPath::from_bytes(payload.get(33..).ok_or(VfsError::InvalidState)?)?;
        let root = native.root_directory()?;
        let (source_parent, source) = native_parent(native, root, &source_path)?;
        let root = native.root_directory()?;
        let (target_parent, target) = native_parent(native, root, &target_path)?;
        let identity = native.identity_at(source_parent.as_ref(), source)?;
        if kind == InodeKind::RegularFile
            && authority.get(&identity)?.as_deref() != Some(expected_inode)
        {
            return Err(VfsError::InvalidState);
        }
        let syncs = if native.directory_identity(source_parent.as_ref())?
            == native.directory_identity(target_parent.as_ref())?
        {
            1
        } else {
            2
        };
        match native.rename_at(
            source_parent.as_ref(),
            source,
            target_parent.as_ref(),
            target,
        ) {
            Ok(()) => *visible = true,
            Err(error) => {
                mark_ambiguous(visible, &error);
                return Err(error.into());
            }
        }
        counters.native.route = Some(NativeRoute::Rename);
        counters.native.rename_calls = checked_add(counters.native.rename_calls, 1)?;
        counters.native.sync_calls = checked_add(counters.native.sync_calls, syncs)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_update(
    engine: &Engine,
    scratch: &RefreshScratch,
    native: &dyn ProjectionWorkspace,
    authority: &DiskNamespace<'_>,
    old_representatives: &DiskNamespace<'_>,
    new_representatives: &DiskNamespace<'_>,
    created_representatives: &DiskNamespace<'_>,
    touched_groups: &DiskNamespace<'_>,
    processed_inodes: &DiskNamespace<'_>,
    removed_authority: &DiskNamespace<'_>,
    path: &CanonicalPath,
    before: Snapshot,
    after: Snapshot,
    accepted: Option<&AcceptedSplice>,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    if path.is_root() {
        if before.kind != InodeKind::Directory
            || after.kind != InodeKind::Directory
            || before.inode != after.inode
        {
            return Err(DriverError::Unsupported.into());
        }
        if before.metadata_root != after.metadata_root {
            let root = native.root_directory()?;
            *visible = true;
            native.set_root_metadata(&metadata(engine, after.metadata_root, counters)?)?;
            native.sync_directory(root.as_ref())?;
            counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
            counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
        }
        return Ok(());
    }
    if before.kind != after.kind
        || (before.inode != after.inode && before.kind == InodeKind::Directory)
    {
        return Err(DriverError::Unsupported.into());
    }
    if before.inode != after.inode {
        apply_delete(
            native,
            authority,
            new_representatives,
            removed_authority,
            path,
            before,
            visible,
            counters,
        )?;
        return apply_create(
            engine,
            native,
            authority,
            old_representatives,
            created_representatives,
            touched_groups,
            path,
            after,
            visible,
            counters,
        );
    }
    match after.kind {
        InodeKind::Directory => {
            refresh_directory_metadata(engine, native, path, before, after, visible, counters)
        }
        InodeKind::RegularFile => {
            if processed_inodes.get(after.inode.as_bytes())?.is_some() {
                return Ok(());
            }
            processed_inodes.put(after.inode.as_bytes(), path.as_bytes())?;
            refresh_regular(
                engine, scratch, native, authority, path, before, after, accepted, visible,
                counters,
            )
        }
        InodeKind::Symlink => {
            refresh_symlink(engine, native, path, before, after, visible, counters)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_create(
    engine: &Engine,
    native: &dyn ProjectionWorkspace,
    authority: &DiskNamespace<'_>,
    old_representatives: &DiskNamespace<'_>,
    created_representatives: &DiskNamespace<'_>,
    touched_groups: &DiskNamespace<'_>,
    path: &CanonicalPath,
    after: Snapshot,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let root = native.root_directory()?;
    let (parent, name) = native_parent(native, root, path)?;
    match after.kind {
        InodeKind::Directory => Err(VfsError::InvalidState),
        InodeKind::Symlink => {
            let link = engine.with_authenticated_canonical(after.content_root, decode_symlink)?;
            let metadata = metadata(engine, after.metadata_root, counters)?;
            match native.atomic_replace_symlink(
                parent.as_ref(),
                name,
                None,
                &link.target,
                &metadata,
            ) {
                Ok(()) => *visible = true,
                Err(error) => {
                    mark_ambiguous(visible, &error);
                    return Err(error.into());
                }
            }
            counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
            counters.native.create_calls = checked_add(counters.native.create_calls, 1)?;
            counters.native.replace_calls = checked_add(counters.native.replace_calls, 1)?;
            counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
            Ok(())
        }
        InodeKind::RegularFile => {
            let source = old_representatives
                .get(after.inode.as_bytes())?
                .or(created_representatives.get(after.inode.as_bytes())?);
            if after.namespace_ref_count > 1 {
                if let Some(source) = source.filter(|source| source != path.as_bytes()) {
                    let root = native.root_directory()?;
                    let source_path = CanonicalPath::from_bytes(&source)?;
                    let (source_parent, source_name) = native_parent(native, root, &source_path)?;
                    let source_identity =
                        native.identity_at(source_parent.as_ref(), source_name)?;
                    if authority.get(&source_identity)?.as_deref() != Some(after.inode.as_bytes()) {
                        return Err(VfsError::InvalidState);
                    }
                    *visible = true;
                    native.create_hard_link_at(
                        source_parent.as_ref(),
                        source_name,
                        &source_identity,
                        parent.as_ref(),
                        name,
                    )?;
                    native.sync_directory(parent.as_ref())?;
                    touched_groups.put(after.inode.as_bytes(), path.as_bytes())?;
                    counters.native.hard_link_calls =
                        checked_add(counters.native.hard_link_calls, 1)?;
                    counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
                    return Ok(());
                }
            }
            let mut temp = native.create_temp_at(parent.as_ref())?;
            counters.native.temp_calls = checked_add(counters.native.temp_calls, 1)?;
            let rope = read_all(engine, FileStateRoot(after.content_root), temp.as_mut())?;
            counters.native.bytes_written =
                checked_add(counters.native.bytes_written, rope.payload_bytes_read)?;
            counters.add_rope(rope)?;
            let mut target_metadata = metadata(engine, after.metadata_root, counters)?;
            if after.namespace_ref_count > 1 {
                target_metadata.bsd_flags = 0;
            }
            native.set_temp_metadata(temp.as_mut(), &target_metadata)?;
            counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
            match native.atomic_replace_checked(temp, parent.as_ref(), name, None) {
                Ok(()) => *visible = true,
                Err(error) => {
                    mark_ambiguous(visible, &error);
                    return Err(error.into());
                }
            }
            counters.native.replace_calls = checked_add(counters.native.replace_calls, 1)?;
            counters.native.create_calls = checked_add(counters.native.create_calls, 1)?;
            counters.native.sync_calls = checked_add(counters.native.sync_calls, 2)?;
            let key = native.identity_at(parent.as_ref(), name)?;
            authority.put(&key, after.inode.as_bytes())?;
            created_representatives.put(after.inode.as_bytes(), path.as_bytes())?;
            if after.namespace_ref_count > 1 {
                touched_groups.put(after.inode.as_bytes(), path.as_bytes())?;
            }
            Ok(())
        }
    }
}

pub(super) fn refresh_directory_metadata(
    engine: &Engine,
    native: &dyn ProjectionWorkspace,
    path: &CanonicalPath,
    before: Snapshot,
    after: Snapshot,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    if before.metadata_root == after.metadata_root {
        return Ok(());
    }
    let root = native.root_directory()?;
    let (parent, name) = native_parent(native, root, path)?;
    let identity = native.identity_at(parent.as_ref(), name)?;
    *visible = true;
    native.set_entry_metadata(
        parent.as_ref(),
        name,
        &identity,
        &metadata(engine, after.metadata_root, counters)?,
    )?;
    let directory = native.open_directory_at(parent.as_ref(), name, None)?;
    native.sync_directory(directory.as_ref())?;
    counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
    counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
    Ok(())
}

pub(super) fn refresh_symlink(
    engine: &Engine,
    native: &dyn ProjectionWorkspace,
    path: &CanonicalPath,
    before: Snapshot,
    after: Snapshot,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let root = native.root_directory()?;
    let (parent, name) = native_parent(native, root, path)?;
    let identity = native.identity_at(parent.as_ref(), name)?;
    if before.content_root == after.content_root {
        if before.metadata_root == after.metadata_root {
            return Ok(());
        }
        *visible = true;
        native.set_entry_metadata(
            parent.as_ref(),
            name,
            &identity,
            &metadata(engine, after.metadata_root, counters)?,
        )?;
        native.sync_directory(parent.as_ref())?;
        counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
        counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
        return Ok(());
    }
    let link = engine.with_authenticated_canonical(after.content_root, decode_symlink)?;
    let target_metadata = metadata(engine, after.metadata_root, counters)?;
    match native.atomic_replace_symlink(
        parent.as_ref(),
        name,
        Some(&identity),
        &link.target,
        &target_metadata,
    ) {
        Ok(()) => *visible = true,
        Err(error) => {
            mark_ambiguous(visible, &error);
            return Err(error.into());
        }
    }
    counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
    counters.native.replace_calls = checked_add(counters.native.replace_calls, 1)?;
    counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
    Ok(())
}
