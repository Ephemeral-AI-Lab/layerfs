use super::super::session_legacy::{VfsError, VfsResult};
use layerfs_core::content::rope::ObjectRead;
use layerfs_core::inode::InodeId;
use layerfs_core::namespace_codec::decode_inode_record;
use layerfs_core::CanonicalPath;
use layerfs_materialization::driver::DriverError;
use layerfs_storage::scratch::DiskNamespace;
use layerfs_storage::Engine;

use super::delta::{decode_inode_diff, snapshot_value};
use super::primitives::{decode_snapshot, encode_snapshot, ordered_path, translate_path};
use super::scratch::{EntryPrefix, RefreshScratch, Snapshot, ORDERED_PATH_PREFIX_BYTES};
#[allow(clippy::too_many_arguments)]
pub(super) fn selective_paths(
    engine: &Engine,
    scratch: &RefreshScratch,
    topology: &DiskNamespace<'_>,
    root_inode: InodeId,
    old_paths: &DiskNamespace<'_>,
    old_representatives: &DiskNamespace<'_>,
    new_paths: &DiskNamespace<'_>,
    new_representatives: &DiskNamespace<'_>,
    changes: &DiskNamespace<'_>,
    old_root: Snapshot,
    new_root: Snapshot,
) -> VfsResult<()> {
    old_paths.put(&[], &encode_snapshot(old_root))?;
    new_paths.put(&[], &encode_snapshot(new_root))?;
    old_representatives.put(root_inode.as_bytes(), &[])?;
    new_representatives.put(root_inode.as_bytes(), &[])?;
    let queue = scratch.table("changed-inodes")?;
    changes.for_each_entry(|inode, bytes| queue.enqueue_once(inode, bytes))?;
    while let Some((inode, change)) = queue.pop_pending()? {
        let inode = InodeId::from_slice(&inode)?;
        if inode == root_inode {
            continue;
        }
        let (before, after) = decode_inode_diff(&change)?;
        let before = engine.with_authenticated_canonical(
            before.ok_or(VfsError::InvalidState)?,
            decode_inode_record,
        )?;
        let after = after
            .map(|id| engine.with_authenticated_canonical(id, decode_inode_record))
            .transpose()?;
        let paths = topology_paths(scratch, topology, root_inode, inode)?;
        let mut found = false;
        let mut callback_error = None;
        paths.for_each_key(|path| {
            found = true;
            let result = (|| -> VfsResult<()> {
                let before = snapshot_value(inode, before);
                old_paths.put(path, &encode_snapshot(before))?;
                if old_representatives.get(inode.as_bytes())?.is_none() {
                    old_representatives.put(inode.as_bytes(), path)?;
                }
                if let Some(after) = after {
                    let after = snapshot_value(inode, after);
                    new_paths.put(path, &encode_snapshot(after))?;
                    if new_representatives.get(inode.as_bytes())?.is_none() {
                        new_representatives.put(inode.as_bytes(), path)?;
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                callback_error = Some(error);
                return Err(layerfs_storage::EngineError::InvalidRecord(
                    "refresh selective path",
                ));
            }
            Ok(())
        })?;
        if let Some(error) = callback_error {
            return Err(error);
        }
        if !found {
            return Err(VfsError::InvalidState);
        }
    }
    Ok(())
}

pub(super) fn topology_paths<'a>(
    scratch: &'a RefreshScratch,
    topology: &DiskNamespace<'_>,
    root_inode: InodeId,
    inode: InodeId,
) -> VfsResult<DiskNamespace<'a>> {
    let queue = scratch.table("topology-walk")?;
    let paths = scratch.table("topology-paths")?;
    let mut start = Vec::with_capacity(34);
    start.extend_from_slice(inode.as_bytes());
    start.extend_from_slice(&0_u16.to_be_bytes());
    queue.enqueue_once(&start, &[])?;
    while let Some((key, _)) = queue.pop_pending()? {
        let current = InodeId::from_slice(key.get(..32).ok_or(VfsError::InvalidState)?)?;
        let depth = u16::from_be_bytes(
            key.get(32..34)
                .ok_or(VfsError::InvalidState)?
                .try_into()
                .map_err(|_| VfsError::InvalidState)?,
        );
        let suffix = key.get(34..).ok_or(VfsError::InvalidState)?;
        if current == root_inode {
            if suffix.is_empty() {
                return Err(VfsError::InvalidState);
            }
            CanonicalPath::from_bytes(suffix)?;
            paths.put(suffix, &[])?;
            continue;
        }
        if usize::from(depth) == layerfs_core::limits::MAX_PATH_COMPONENTS {
            return Err(VfsError::InvalidState);
        }
        let mut callback_error = None;
        topology.for_each_entry_prefix(current.as_bytes(), |edge, value| {
            let result = (|| -> VfsResult<()> {
                if !value.is_empty()
                    || edge.len() <= 64
                    || edge.get(..32) != Some(current.as_bytes().as_slice())
                {
                    return Err(VfsError::InvalidState);
                }
                let parent = InodeId::from_slice(&edge[32..64])?;
                let name = layerfs_core::CanonicalName::from_bytes(&edge[64..])?;
                let mut next_suffix = Vec::with_capacity(
                    name.as_bytes().len() + usize::from(!suffix.is_empty()) + suffix.len(),
                );
                next_suffix.extend_from_slice(name.as_bytes());
                if !suffix.is_empty() {
                    next_suffix.push(b'/');
                    next_suffix.extend_from_slice(suffix);
                }
                CanonicalPath::from_bytes(&next_suffix)?;
                let mut next = Vec::with_capacity(34 + next_suffix.len());
                next.extend_from_slice(parent.as_bytes());
                next.extend_from_slice(&(depth + 1).to_be_bytes());
                next.extend_from_slice(&next_suffix);
                queue.enqueue_once(&next, &[])?;
                Ok(())
            })();
            if let Err(error) = result {
                callback_error = Some(error);
                return Err(layerfs_storage::EngineError::InvalidRecord(
                    "refresh topology edge",
                ));
            }
            Ok(())
        })?;
        if let Some(error) = callback_error {
            return Err(error);
        }
    }
    Ok(paths)
}

pub(super) fn topology_target_paths<'a>(
    scratch: &'a RefreshScratch,
    topology: &DiskNamespace<'_>,
    removed: &DiskNamespace<'_>,
    added: &DiskNamespace<'_>,
    root_inode: InodeId,
    inode: InodeId,
) -> VfsResult<DiskNamespace<'a>> {
    let queue = scratch.table("target-topology-walk")?;
    let paths = scratch.table("target-topology-paths")?;
    let mut start = Vec::with_capacity(34);
    start.extend_from_slice(inode.as_bytes());
    start.extend_from_slice(&0_u16.to_be_bytes());
    queue.enqueue_once(&start, &[])?;
    while let Some((key, _)) = queue.pop_pending()? {
        let current = InodeId::from_slice(key.get(..32).ok_or(VfsError::InvalidState)?)?;
        let depth = u16::from_be_bytes(
            key.get(32..34)
                .ok_or(VfsError::InvalidState)?
                .try_into()
                .map_err(|_| VfsError::InvalidState)?,
        );
        let suffix = key.get(34..).ok_or(VfsError::InvalidState)?;
        if current == root_inode {
            if suffix.is_empty() {
                return Err(VfsError::InvalidState);
            }
            CanonicalPath::from_bytes(suffix)?;
            paths.put(suffix, &[])?;
            continue;
        }
        if usize::from(depth) == layerfs_core::limits::MAX_PATH_COMPONENTS {
            return Err(VfsError::InvalidState);
        }
        enqueue_topology_parents(added, None, current, depth, suffix, &queue)?;
        enqueue_topology_parents(topology, Some(removed), current, depth, suffix, &queue)?;
    }
    Ok(paths)
}

pub(super) fn enqueue_topology_parents(
    source: &impl EntryPrefix,
    removed: Option<&DiskNamespace<'_>>,
    current: InodeId,
    depth: u16,
    suffix: &[u8],
    queue: &DiskNamespace<'_>,
) -> VfsResult<()> {
    let mut callback_error = None;
    source.visit_prefix(current.as_bytes(), |edge, value| {
        let result = (|| -> VfsResult<()> {
            if !value.is_empty()
                || edge.len() <= 64
                || edge.get(..32) != Some(current.as_bytes().as_slice())
            {
                return Err(VfsError::InvalidState);
            }
            if let Some(removed) = removed {
                if removed.get(edge)?.is_some() {
                    return Ok(());
                }
            }
            let parent = InodeId::from_slice(&edge[32..64])?;
            let name = layerfs_core::CanonicalName::from_bytes(&edge[64..])?;
            let mut next_suffix = Vec::with_capacity(
                name.as_bytes().len() + usize::from(!suffix.is_empty()) + suffix.len(),
            );
            next_suffix.extend_from_slice(name.as_bytes());
            if !suffix.is_empty() {
                next_suffix.push(b'/');
                next_suffix.extend_from_slice(suffix);
            }
            CanonicalPath::from_bytes(&next_suffix)?;
            let mut next = Vec::with_capacity(34 + next_suffix.len());
            next.extend_from_slice(parent.as_bytes());
            next.extend_from_slice(&(depth + 1).to_be_bytes());
            next.extend_from_slice(&next_suffix);
            queue.enqueue_once(&next, &[])?;
            Ok(())
        })();
        if let Err(error) = result {
            callback_error = Some(error);
            return Err(layerfs_storage::EngineError::InvalidRecord(
                "refresh target topology edge",
            ));
        }
        Ok(())
    })?;
    callback_error.map_or(Ok(()), Err)
}

pub(super) fn rotate_topology(
    topology: &DiskNamespace<'_>,
    old_edges: &DiskNamespace<'_>,
    new_edges: &DiskNamespace<'_>,
) -> VfsResult<()> {
    old_edges.for_each_key(|edge| {
        if new_edges.get(edge)?.is_none() {
            topology.remove(edge)?;
        }
        Ok(())
    })?;
    new_edges.for_each_key(|edge| {
        if old_edges.get(edge)?.is_none() {
            topology.put(edge, &[])?;
        }
        Ok(())
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn plan_renames(
    scratch: &RefreshScratch,
    old_paths: &DiskNamespace<'_>,
    new_paths: &DiskNamespace<'_>,
    old_representatives: &DiskNamespace<'_>,
    new_representatives: &DiskNamespace<'_>,
    renames: &DiskNamespace<'_>,
    rename_targets: &DiskNamespace<'_>,
    rename_queue: &DiskNamespace<'_>,
) -> VfsResult<()> {
    let representatives = scratch.table("rename-inodes")?;
    let candidates = scratch.table("rename-candidates")?;
    old_representatives
        .for_each_entry(|inode, old_path| representatives.enqueue_once(inode, old_path))?;
    while let Some((inode, old_path)) = representatives.pop_pending()? {
        let Some(new_path) = new_representatives.get(&inode)? else {
            continue;
        };
        if old_path != new_path
            && new_paths.get(&old_path)?.is_none()
            && old_paths.get(&new_path)?.is_none()
        {
            candidates.enqueue_once(&ordered_path(&old_path, false)?, &new_path)?;
        }
    }
    while let Some((key, new_path)) = candidates.pop_pending()? {
        let old_path = key
            .get(ORDERED_PATH_PREFIX_BYTES..)
            .ok_or(VfsError::InvalidState)?;
        let before = old_paths
            .get(old_path)?
            .ok_or(VfsError::InvalidState)
            .and_then(|bytes| decode_snapshot(&bytes))?;
        let after = new_paths
            .get(&new_path)?
            .ok_or(VfsError::InvalidState)
            .and_then(|bytes| decode_snapshot(&bytes))?;
        if before.inode != after.inode
            || before.kind != after.kind
            || before.namespace_ref_count != 1
            || after.namespace_ref_count != 1
        {
            continue;
        }
        let translated = translate_path(old_path, renames)?;
        if translated != old_path {
            if translated != new_path {
                return Err(DriverError::Unsupported.into());
            }
            continue;
        }
        if rename_targets.get(&new_path)?.is_some() {
            return Err(DriverError::Unsupported.into());
        }
        renames.put(old_path, &new_path)?;
        rename_targets.put(&new_path, old_path)?;
        let mut payload = Vec::with_capacity(33 + new_path.len());
        payload.push(after.kind as u8);
        payload.extend_from_slice(after.inode.as_bytes());
        payload.extend_from_slice(&new_path);
        rename_queue.enqueue_once(&ordered_path(old_path, false)?, &payload)?;
    }
    Ok(())
}

pub(super) fn align_old_paths(
    old_paths: &DiskNamespace<'_>,
    renames: &DiskNamespace<'_>,
    aligned: &DiskNamespace<'_>,
    representatives: &DiskNamespace<'_>,
) -> VfsResult<()> {
    let mut callback_error = None;
    old_paths.for_each_entry(|path, bytes| {
        let result = (|| -> VfsResult<()> {
            let path = translate_path(path, renames)?;
            if let Some(existing) = aligned.get(&path)? {
                if existing != bytes {
                    return Err(VfsError::InvalidState);
                }
            } else {
                aligned.put(&path, bytes)?;
            }
            let snapshot = decode_snapshot(bytes)?;
            if representatives.get(snapshot.inode.as_bytes())?.is_none() {
                representatives.put(snapshot.inode.as_bytes(), &path)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            callback_error = Some(error);
            return Err(layerfs_storage::EngineError::InvalidRecord(
                "refresh path alignment",
            ));
        }
        Ok(())
    })?;
    callback_error.map_or(Ok(()), Err)
}
