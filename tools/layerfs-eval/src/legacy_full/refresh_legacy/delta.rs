use super::super::session_legacy::{VfsError, VfsResult};
use super::super::OperationCounters;
use layerfs_core::content::rope::ObjectRead;
use layerfs_core::inode::{
    inode_table_lookup, InodeId, InodeKind, InodeRecordV1, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::namespace::{
    diff_directory_entries, visit_directory_entries, DirectoryStateRoot, NamespaceCounters,
};
use layerfs_core::namespace_codec::decode_inode_record;
use layerfs_core::ObjectId;
use layerfs_storage::scratch::DiskNamespace;
use layerfs_storage::Engine;

use super::primitives::encode_snapshot;
use super::scratch::{RefreshScratch, Snapshot};
use super::topology::{topology_paths, topology_target_paths};
#[allow(clippy::too_many_arguments)]
pub(super) fn namespace_delta_paths(
    engine: &Engine,
    scratch: &RefreshScratch,
    topology: &DiskNamespace<'_>,
    root_inode: InodeId,
    old_table: InodeTableRoot,
    new_table: InodeTableRoot,
    old_paths: &DiskNamespace<'_>,
    old_representatives: &DiskNamespace<'_>,
    new_paths: &DiskNamespace<'_>,
    new_representatives: &DiskNamespace<'_>,
    inode_changes: &DiskNamespace<'_>,
    old_edges: &DiskNamespace<'_>,
    new_edges: &DiskNamespace<'_>,
    old_root: Snapshot,
    new_root: Snapshot,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    old_paths.put(&[], &encode_snapshot(old_root))?;
    new_paths.put(&[], &encode_snapshot(new_root))?;
    old_representatives.put(root_inode.as_bytes(), &[])?;
    new_representatives.put(root_inode.as_bytes(), &[])?;
    let queue = scratch.table("directory-inodes")?;
    let enumerated = scratch.table("enumerated-directories")?;
    inode_changes.for_each_entry(|inode, change| queue.enqueue_once(inode, change))?;
    while let Some((inode, change)) = queue.pop_pending()? {
        let inode = InodeId::from_slice(&inode)?;
        let (before_id, after_id) = decode_inode_diff(&change)?;
        let before = before_id
            .map(|id| engine.with_authenticated_canonical(id, decode_inode_record))
            .transpose()?;
        let after = after_id
            .map(|id| engine.with_authenticated_canonical(id, decode_inode_record))
            .transpose()?;
        if after.is_none() {
            topology.for_each_entry_prefix(inode.as_bytes(), |edge, value| {
                if !value.is_empty() {
                    return Err(layerfs_storage::EngineError::InvalidRecord(
                        "refresh topology value",
                    ));
                }
                old_edges.put(edge, &[])
            })?;
        }
        match (before, after) {
            (Some(before), Some(after))
                if before.kind == InodeKind::Directory
                    && after.kind == InodeKind::Directory
                    && before.content_root != after.content_root =>
            {
                let mut scratch_error = None;
                let diff = diff_directory_entries(
                    engine,
                    DirectoryStateRoot(before.content_root),
                    DirectoryStateRoot(after.content_root),
                    |entry| {
                        let result = (|| -> VfsResult<()> {
                            if let Some(child) = entry.before {
                                old_edges.put(&topology_edge(child, inode, &entry.name), &[])?;
                            }
                            if let Some(child) = entry.after {
                                new_edges.put(&topology_edge(child, inode, &entry.name), &[])?;
                            }
                            Ok(())
                        })();
                        if let Err(error) = result {
                            scratch_error = Some(error);
                            return Err(layerfs_core::CoreError::Io);
                        }
                        Ok(())
                    },
                );
                if let Some(error) = scratch_error {
                    return Err(error);
                }
                let diff = diff?;
                counters.root_diff_nodes = counters
                    .root_diff_nodes
                    .checked_add(diff.nodes_read)
                    .ok_or(VfsError::InvalidState)?;
                counters.add_namespace(diff)?;
            }
            (None, Some(after))
                if after.kind == InodeKind::Directory
                    && enumerated.get(inode.as_bytes())?.is_none() =>
            {
                enumerated.put(inode.as_bytes(), &[])?;
                enumerate_new_edges(
                    engine,
                    new_table,
                    inode_changes,
                    inode,
                    DirectoryStateRoot(after.content_root),
                    new_edges,
                    &enumerated,
                    counters,
                )?;
            }
            _ => {}
        }
    }

    let affected = scratch.table("affected-inodes")?;
    inode_changes.for_each_key(|inode| affected.enqueue_once(inode, &[]))?;
    old_edges.for_each_key(|edge| {
        affected.enqueue_once(
            edge.get(..32)
                .ok_or(layerfs_storage::EngineError::InvalidRecord("refresh edge"))?,
            &[],
        )
    })?;
    new_edges.for_each_key(|edge| {
        affected.enqueue_once(
            edge.get(..32)
                .ok_or(layerfs_storage::EngineError::InvalidRecord("refresh edge"))?,
            &[],
        )
    })?;
    while let Some((inode, _)) = affected.pop_pending()? {
        let inode = InodeId::from_slice(&inode)?;
        if inode == root_inode {
            continue;
        }
        if let Some(before) = record_at(engine, old_table, inode_changes, inode, false, counters)? {
            let paths = topology_paths(scratch, topology, root_inode, inode)?;
            put_snapshot_paths(
                &paths,
                old_paths,
                old_representatives,
                snapshot_value(inode, before),
            )?;
        }
        if let Some(after) = record_at(engine, new_table, inode_changes, inode, true, counters)? {
            let paths =
                topology_target_paths(scratch, topology, old_edges, new_edges, root_inode, inode)?;
            put_snapshot_paths(
                &paths,
                new_paths,
                new_representatives,
                snapshot_value(inode, after),
            )?;
        }
    }
    Ok(())
}

pub(super) fn record_at(
    engine: &Engine,
    table: InodeTableRoot,
    changes: &DiskNamespace<'_>,
    inode: InodeId,
    after: bool,
    counters: &mut OperationCounters,
) -> VfsResult<Option<InodeRecordV1>> {
    let id = if let Some(change) = changes.get(inode.as_bytes())? {
        let (before, next) = decode_inode_diff(&change)?;
        if after {
            next
        } else {
            before
        }
    } else {
        let mut visits = InodeTableCounters::default();
        let id = inode_table_lookup(engine, table, inode, &mut visits)?;
        counters.add_inode_table(visits)?;
        id
    };
    id.map(|id| engine.with_authenticated_canonical(id, decode_inode_record))
        .transpose()
        .map_err(Into::into)
}

pub(super) fn topology_edge(
    child: InodeId,
    parent: InodeId,
    name: &layerfs_core::CanonicalName,
) -> Vec<u8> {
    let mut edge = Vec::with_capacity(64 + name.as_bytes().len());
    edge.extend_from_slice(child.as_bytes());
    edge.extend_from_slice(parent.as_bytes());
    edge.extend_from_slice(name.as_bytes());
    edge
}

#[allow(clippy::too_many_arguments)]
pub(super) fn enumerate_new_edges(
    engine: &Engine,
    table: InodeTableRoot,
    changes: &DiskNamespace<'_>,
    parent: InodeId,
    directory: DirectoryStateRoot,
    edges: &DiskNamespace<'_>,
    enumerated: &DiskNamespace<'_>,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let mut callback_error = None;
    let mut visits = NamespaceCounters::default();
    let result = visit_directory_entries(engine, directory, &mut visits, |entries| {
        for (name, child) in entries {
            let nested = (|| -> VfsResult<()> {
                edges.put(&topology_edge(*child, parent, name), &[])?;
                let record = record_at(engine, table, changes, *child, true, counters)?
                    .ok_or(VfsError::InvalidState)?;
                if record.kind == InodeKind::Directory
                    && enumerated.get(child.as_bytes())?.is_none()
                {
                    enumerated.put(child.as_bytes(), &[])?;
                    enumerate_new_edges(
                        engine,
                        table,
                        changes,
                        *child,
                        DirectoryStateRoot(record.content_root),
                        edges,
                        enumerated,
                        counters,
                    )?;
                }
                Ok(())
            })();
            if let Err(error) = nested {
                callback_error = Some(error);
                return Err(layerfs_core::CoreError::Io);
            }
        }
        Ok(())
    });
    if let Some(error) = callback_error {
        return Err(error);
    }
    result?;
    counters.add_namespace(visits)?;
    Ok(())
}

pub(super) fn put_snapshot_paths(
    source: &DiskNamespace<'_>,
    paths: &DiskNamespace<'_>,
    representatives: &DiskNamespace<'_>,
    snapshot: Snapshot,
) -> VfsResult<()> {
    let mut callback_error = None;
    let mut found = false;
    source.for_each_key(|path| {
        found = true;
        let result = (|| -> VfsResult<()> {
            paths.put(path, &encode_snapshot(snapshot))?;
            if representatives.get(snapshot.inode.as_bytes())?.is_none() {
                representatives.put(snapshot.inode.as_bytes(), path)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            callback_error = Some(error);
            return Err(layerfs_storage::EngineError::InvalidRecord(
                "refresh snapshot paths",
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
    Ok(())
}

pub(super) fn record(
    engine: &Engine,
    table: InodeTableRoot,
    inode: InodeId,
    counters: &mut OperationCounters,
) -> VfsResult<InodeRecordV1> {
    let mut visits = InodeTableCounters::default();
    let id =
        inode_table_lookup(engine, table, inode, &mut visits)?.ok_or(VfsError::InvalidState)?;
    counters.add_inode_table(visits)?;
    Ok(engine.with_authenticated_canonical(id, decode_inode_record)?)
}

pub(super) fn encode_inode_diff(before: Option<ObjectId>, after: Option<ObjectId>) -> [u8; 66] {
    let mut bytes = [0_u8; 66];
    if let Some(before) = before {
        bytes[0] = 1;
        bytes[2..34].copy_from_slice(before.as_bytes());
    }
    if let Some(after) = after {
        bytes[1] = 1;
        bytes[34..66].copy_from_slice(after.as_bytes());
    }
    bytes
}

pub(super) fn decode_inode_diff(bytes: &[u8]) -> VfsResult<(Option<ObjectId>, Option<ObjectId>)> {
    if bytes.len() != 66 || bytes[0] > 1 || bytes[1] > 1 {
        return Err(VfsError::InvalidState);
    }
    let before = (bytes[0] == 1)
        .then(|| ObjectId::from_bytes(&bytes[2..34]))
        .transpose()?;
    let after = (bytes[1] == 1)
        .then(|| ObjectId::from_bytes(&bytes[34..66]))
        .transpose()?;
    Ok((before, after))
}

pub(super) fn inode_diff_changes_namespace(
    engine: &Engine,
    changes: &DiskNamespace<'_>,
) -> VfsResult<bool> {
    let mut changed = false;
    let mut callback_error = None;
    changes.for_each(|bytes| {
        let result = (|| -> VfsResult<bool> {
            let (before, after) = decode_inode_diff(bytes)?;
            let before = before
                .map(|id| engine.with_authenticated_canonical(id, decode_inode_record))
                .transpose()?;
            let after = after
                .map(|id| engine.with_authenticated_canonical(id, decode_inode_record))
                .transpose()?;
            Ok(match (before, after) {
                (Some(before), Some(after)) => {
                    before.kind != after.kind
                        || (before.kind == InodeKind::Directory
                            && before.content_root != after.content_root)
                }
                (Some(record), None) | (None, Some(record)) => record.kind == InodeKind::Directory,
                (None, None) => return Err(VfsError::InvalidState),
            })
        })();
        match result {
            Ok(value) => changed |= value,
            Err(error) => {
                callback_error = Some(error);
                return Err(layerfs_storage::EngineError::InvalidRecord(
                    "refresh inode diff",
                ));
            }
        }
        Ok(())
    })?;
    callback_error.map_or(Ok(changed), Err)
}

pub(super) fn target_snapshot(
    engine: &Engine,
    old: Snapshot,
    changes: &DiskNamespace<'_>,
) -> VfsResult<Snapshot> {
    let Some(change) = changes.get(old.inode.as_bytes())? else {
        return Ok(old);
    };
    let (_, after) = decode_inode_diff(&change)?;
    let record = engine
        .with_authenticated_canonical(after.ok_or(VfsError::InvalidState)?, decode_inode_record)?;
    Ok(Snapshot {
        inode: old.inode,
        kind: record.kind,
        namespace_ref_count: record.namespace_ref_count,
        content_root: record.content_root,
        metadata_root: record.metadata_root,
    })
}

pub(super) fn snapshot_value(inode: InodeId, record: InodeRecordV1) -> Snapshot {
    Snapshot {
        inode,
        kind: record.kind,
        namespace_ref_count: record.namespace_ref_count,
        content_root: record.content_root,
        metadata_root: record.metadata_root,
    }
}
