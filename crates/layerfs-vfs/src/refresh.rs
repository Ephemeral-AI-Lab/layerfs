use crate::driver::{DriverError, ProjectionWorkspace};
use crate::managed_edit::native_parent;
use crate::materialize::metadata;
use crate::resolver::namespace;
use crate::workspace::{VfsError, VfsResult};
use crate::{NativeRoute, OperationCounters};
use layerfs_core::content::rope::{diff_ranges, read_all, read_range, FileStateRoot, ObjectRead};
use layerfs_core::inode::{
    diff_inode_table_entries, inode_table_lookup, InodeId, InodeKind, InodeRecordV1,
    InodeTableCounters, InodeTableRoot,
};
use layerfs_core::namespace::{
    diff_directory_entries, visit_directory_entries, DirectoryStateRoot, NamespaceCounters,
};
use layerfs_core::namespace_codec::{decode_inode_record, decode_symlink};
use layerfs_core::{CanonicalPath, ObjectId};
use layerfs_engine::refs::RefState;
use layerfs_engine::scratch::{DiskNamespace, DiskTable};
use layerfs_engine::Engine;
use std::cell::Cell;
use std::io::{Seek, SeekFrom, Write};

const SNAPSHOT_BYTES: usize = 105;
const ORDERED_PATH_PREFIX_BYTES: usize = 2;

struct RefreshScratch {
    table: DiskTable,
    serial: Cell<u32>,
}

impl RefreshScratch {
    fn create(engine: &Engine) -> VfsResult<Self> {
        let table = DiskTable::create_near(engine.path(), "refresh")?;
        Ok(Self {
            table,
            serial: Cell::new(0),
        })
    }

    fn table(&self, label: &str) -> VfsResult<DiskNamespace<'_>> {
        let serial = self.serial.get();
        self.serial
            .set(serial.checked_add(1).ok_or(VfsError::InvalidState)?);
        let name = format!("{serial:04x}-{label}");
        Ok(self.table.namespace(name.as_bytes())?)
    }
}

trait EntryPrefix {
    fn visit_prefix(
        &self,
        prefix: &[u8],
        callback: impl FnMut(&[u8], &[u8]) -> layerfs_engine::EngineResult<()>,
    ) -> layerfs_engine::EngineResult<()>;
}

impl EntryPrefix for DiskTable {
    fn visit_prefix(
        &self,
        prefix: &[u8],
        callback: impl FnMut(&[u8], &[u8]) -> layerfs_engine::EngineResult<()>,
    ) -> layerfs_engine::EngineResult<()> {
        self.for_each_entry_prefix(prefix, callback)
    }
}

impl EntryPrefix for DiskNamespace<'_> {
    fn visit_prefix(
        &self,
        prefix: &[u8],
        callback: impl FnMut(&[u8], &[u8]) -> layerfs_engine::EngineResult<()>,
    ) -> layerfs_engine::EngineResult<()> {
        self.for_each_entry_prefix(prefix, callback)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snapshot {
    inode: InodeId,
    kind: InodeKind,
    namespace_ref_count: u64,
    content_root: ObjectId,
    metadata_root: ObjectId,
}

pub(crate) fn apply(
    engine: &Engine,
    native: &dyn ProjectionWorkspace,
    authority: &DiskTable,
    topology: &DiskTable,
    old: &RefState,
    target: &RefState,
    visible: &mut bool,
) -> VfsResult<OperationCounters> {
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
            (Some(_), Some(_)) => updates.enqueue_once(&path, &path)?,
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
    let scratch = scratch.table.observation()?;
    counters.plan_scratch_high_water_bytes = scratch.high_water_bytes;
    counters.add_scratch(scratch)?;
    Ok(counters)
}

#[allow(clippy::too_many_arguments)]
fn namespace_delta_paths(
    engine: &Engine,
    scratch: &RefreshScratch,
    topology: &DiskTable,
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
                    return Err(layerfs_engine::EngineError::InvalidRecord(
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
                .ok_or(layerfs_engine::EngineError::InvalidRecord("refresh edge"))?,
            &[],
        )
    })?;
    new_edges.for_each_key(|edge| {
        affected.enqueue_once(
            edge.get(..32)
                .ok_or(layerfs_engine::EngineError::InvalidRecord("refresh edge"))?,
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

fn record_at(
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

fn topology_edge(child: InodeId, parent: InodeId, name: &layerfs_core::CanonicalName) -> Vec<u8> {
    let mut edge = Vec::with_capacity(64 + name.as_bytes().len());
    edge.extend_from_slice(child.as_bytes());
    edge.extend_from_slice(parent.as_bytes());
    edge.extend_from_slice(name.as_bytes());
    edge
}

#[allow(clippy::too_many_arguments)]
fn enumerate_new_edges(
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

fn put_snapshot_paths(
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
            return Err(layerfs_engine::EngineError::InvalidRecord(
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

fn record(
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

fn encode_inode_diff(before: Option<ObjectId>, after: Option<ObjectId>) -> [u8; 66] {
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

fn decode_inode_diff(bytes: &[u8]) -> VfsResult<(Option<ObjectId>, Option<ObjectId>)> {
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

fn inode_diff_changes_namespace(engine: &Engine, changes: &DiskNamespace<'_>) -> VfsResult<bool> {
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
                return Err(layerfs_engine::EngineError::InvalidRecord(
                    "refresh inode diff",
                ));
            }
        }
        Ok(())
    })?;
    callback_error.map_or(Ok(changed), Err)
}

fn target_snapshot(
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

fn snapshot_value(inode: InodeId, record: InodeRecordV1) -> Snapshot {
    Snapshot {
        inode,
        kind: record.kind,
        namespace_ref_count: record.namespace_ref_count,
        content_root: record.content_root,
        metadata_root: record.metadata_root,
    }
}

#[allow(clippy::too_many_arguments)]
fn selective_paths(
    engine: &Engine,
    scratch: &RefreshScratch,
    topology: &DiskTable,
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
                return Err(layerfs_engine::EngineError::InvalidRecord(
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

fn topology_paths<'a>(
    scratch: &'a RefreshScratch,
    topology: &DiskTable,
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
                return Err(layerfs_engine::EngineError::InvalidRecord(
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

fn topology_target_paths<'a>(
    scratch: &'a RefreshScratch,
    topology: &DiskTable,
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

fn enqueue_topology_parents(
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
            return Err(layerfs_engine::EngineError::InvalidRecord(
                "refresh target topology edge",
            ));
        }
        Ok(())
    })?;
    callback_error.map_or(Ok(()), Err)
}

fn rotate_topology(
    topology: &DiskTable,
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
fn plan_renames(
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

fn align_old_paths(
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
            return Err(layerfs_engine::EngineError::InvalidRecord(
                "refresh path alignment",
            ));
        }
        Ok(())
    })?;
    callback_error.map_or(Ok(()), Err)
}

fn apply_directories(
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

fn apply_renames(
    native: &dyn ProjectionWorkspace,
    authority: &DiskTable,
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
fn apply_update(
    engine: &Engine,
    scratch: &RefreshScratch,
    native: &dyn ProjectionWorkspace,
    authority: &DiskTable,
    old_representatives: &DiskNamespace<'_>,
    new_representatives: &DiskNamespace<'_>,
    created_representatives: &DiskNamespace<'_>,
    touched_groups: &DiskNamespace<'_>,
    processed_inodes: &DiskNamespace<'_>,
    removed_authority: &DiskNamespace<'_>,
    path: &CanonicalPath,
    before: Snapshot,
    after: Snapshot,
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
                engine, scratch, native, authority, path, before, after, visible, counters,
            )
        }
        InodeKind::Symlink => {
            refresh_symlink(engine, native, path, before, after, visible, counters)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_create(
    engine: &Engine,
    native: &dyn ProjectionWorkspace,
    authority: &DiskTable,
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

fn refresh_directory_metadata(
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

fn refresh_symlink(
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

#[allow(clippy::too_many_arguments)]
fn finalize_hard_links(
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
fn apply_delete(
    native: &dyn ProjectionWorkspace,
    authority: &DiskTable,
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

#[allow(clippy::too_many_arguments)]
fn refresh_regular(
    engine: &Engine,
    scratch: &RefreshScratch,
    native: &dyn ProjectionWorkspace,
    authority: &DiskTable,
    path: &CanonicalPath,
    before: Snapshot,
    after: Snapshot,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    if before.content_root == after.content_root {
        if before.metadata_root == after.metadata_root {
            return Ok(());
        }
        let root = native.root_directory()?;
        let (parent, name) = native_parent(native, root, path)?;
        let identity = native.identity_at(parent.as_ref(), name)?;
        ensure_unprotected(native, parent.as_ref(), name, counters)?;
        *visible = true;
        native.set_entry_metadata(
            parent.as_ref(),
            name,
            &identity,
            &metadata(engine, after.metadata_root, counters)?,
        )?;
        let mut file = native.open_regular_at(parent.as_ref(), name, Some(&identity))?;
        native.sync_regular(file.as_mut())?;
        counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 2)?;
        counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
        return Ok(());
    }

    let ranges = scratch.table("ranges")?;
    let (same_length, count) = changed_ranges(
        engine,
        before.content_root,
        after.content_root,
        &ranges,
        counters,
    )?;
    let root = native.root_directory()?;
    let (parent, name) = native_parent(native, root, path)?;
    let old_key = native.identity_at(parent.as_ref(), name)?;
    let old_token = native.token_at(parent.as_ref(), name)?;
    if authority.get(&old_key)?.as_deref() != Some(before.inode.as_bytes()) {
        return Err(VfsError::InvalidState);
    }
    ensure_unprotected(native, parent.as_ref(), name, counters)?;
    if !same_length {
        return full_fallback(
            engine,
            native,
            authority,
            parent.as_ref(),
            name,
            &old_key,
            &old_token,
            before,
            after,
            visible,
            counters,
        );
    }
    let source = native.open_regular_read_at(parent.as_ref(), name, Some(&old_token))?;
    let clone = if before.namespace_ref_count == 1 && after.namespace_ref_count == 1 {
        native.clone_temp_from_regular(source.as_ref())
    } else {
        Err(DriverError::Unsupported)
    };
    match clone {
        Ok(mut temp) => {
            counters.native.clone_attempts = checked_add(counters.native.clone_attempts, 1)?;
            counters.native.clone_successes = checked_add(counters.native.clone_successes, 1)?;
            counters.native.temp_calls = checked_add(counters.native.temp_calls, 1)?;
            for index in 0..count {
                let (start, end) = decode_range(
                    &ranges
                        .get(&index.to_be_bytes())?
                        .ok_or(VfsError::InvalidState)?,
                )?;
                copy_target_range(
                    engine,
                    after.content_root,
                    start,
                    end,
                    temp.as_mut(),
                    counters,
                )?;
            }
            native.set_temp_metadata(
                temp.as_mut(),
                &metadata(engine, after.metadata_root, counters)?,
            )?;
            counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 2)?;
            match native.atomic_replace_checked(temp, parent.as_ref(), name, Some(&old_key)) {
                Ok(()) => *visible = true,
                Err(error) => {
                    mark_ambiguous(visible, &error);
                    return Err(error.into());
                }
            }
            counters.native.replace_calls = checked_add(counters.native.replace_calls, 1)?;
            counters.native.sync_calls = checked_add(counters.native.sync_calls, 2)?;
            counters.native.route = Some(NativeRoute::ClonePatch);
            let next_key = native.identity_at(parent.as_ref(), name)?;
            authority.remove(&old_key)?;
            authority.put(&next_key, before.inode.as_bytes())?;
        }
        Err(DriverError::Unsupported) => {
            counters.native.clone_attempts = checked_add(counters.native.clone_attempts, 1)?;
            counters.native.clone_fallbacks = checked_add(counters.native.clone_fallbacks, 1)?;
            let mut file = native.open_regular_at(parent.as_ref(), name, Some(&old_token))?;
            for index in 0..count {
                let (start, end) = decode_range(
                    &ranges
                        .get(&index.to_be_bytes())?
                        .ok_or(VfsError::InvalidState)?,
                )?;
                *visible = true;
                copy_target_range(
                    engine,
                    after.content_root,
                    start,
                    end,
                    file.as_mut(),
                    counters,
                )?;
            }
            native.set_entry_metadata(
                parent.as_ref(),
                name,
                &old_key,
                &metadata(engine, after.metadata_root, counters)?,
            )?;
            native.sync_regular(file.as_mut())?;
            counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 2)?;
            counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
            counters.native.route = Some(NativeRoute::InPlacePatch);
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn full_fallback(
    engine: &Engine,
    native: &dyn ProjectionWorkspace,
    authority: &DiskTable,
    parent: &dyn crate::driver::DirectoryHandle,
    name: &[u8],
    old_key: &[u8],
    old_token: &[u8],
    before: Snapshot,
    after: Snapshot,
    visible: &mut bool,
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let target_metadata = metadata(engine, after.metadata_root, counters)?;
    if before.namespace_ref_count > 1 || after.namespace_ref_count > 1 {
        let mut file = native.open_regular_at(parent, name, Some(old_token))?;
        *visible = true;
        native.set_regular_len(file.as_mut(), 0)?;
        file.seek(SeekFrom::Start(0))?;
        let rope = read_all(engine, FileStateRoot(after.content_root), file.as_mut())?;
        counters.native.bytes_written =
            checked_add(counters.native.bytes_written, rope.payload_bytes_read)?;
        counters.add_rope(rope)?;
        native.set_entry_metadata(parent, name, old_key, &target_metadata)?;
        native.sync_regular(file.as_mut())?;
        counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
        counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
    } else {
        let mut temp = native.create_temp_at(parent)?;
        counters.native.temp_calls = checked_add(counters.native.temp_calls, 1)?;
        let rope = read_all(engine, FileStateRoot(after.content_root), temp.as_mut())?;
        counters.native.bytes_written =
            checked_add(counters.native.bytes_written, rope.payload_bytes_read)?;
        counters.add_rope(rope)?;
        native.set_temp_metadata(temp.as_mut(), &target_metadata)?;
        counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
        match native.atomic_replace_checked(temp, parent, name, Some(old_key)) {
            Ok(()) => *visible = true,
            Err(error) => {
                mark_ambiguous(visible, &error);
                return Err(error.into());
            }
        }
        counters.native.replace_calls = checked_add(counters.native.replace_calls, 1)?;
        counters.native.sync_calls = checked_add(counters.native.sync_calls, 2)?;
        let next_key = native.identity_at(parent, name)?;
        authority.remove(old_key)?;
        authority.put(&next_key, after.inode.as_bytes())?;
    }
    counters.native.route = Some(NativeRoute::FullFallback);
    counters.full_fallback_files = checked_add(counters.full_fallback_files, 1)?;
    Ok(())
}

fn ensure_unprotected(
    native: &dyn ProjectionWorkspace,
    parent: &dyn crate::driver::DirectoryHandle,
    name: &[u8],
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let token = native.token_at(parent, name)?;
    let metadata = native.read_metadata_at(parent, name, Some(&token))?;
    counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
    if metadata.bsd_flags & 0x6 != 0 {
        return Err(VfsError::NativeProtected);
    }
    Ok(())
}

fn ordered_path(path: &[u8], reverse: bool) -> VfsResult<Vec<u8>> {
    let depth = if path.is_empty() {
        0
    } else {
        path.iter().filter(|byte| **byte == b'/').count() + 1
    };
    let depth = u16::try_from(depth).map_err(|_| VfsError::InvalidState)?;
    let depth = if reverse { u16::MAX - depth } else { depth };
    let mut key = Vec::with_capacity(path.len() + ORDERED_PATH_PREFIX_BYTES);
    key.extend_from_slice(&depth.to_be_bytes());
    key.extend_from_slice(path);
    Ok(key)
}

fn translate_path(path: &[u8], mappings: &DiskNamespace<'_>) -> VfsResult<Vec<u8>> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    for (index, _) in path
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, byte)| *byte == b'/')
        .chain(std::iter::once((path.len(), b'/')))
    {
        let prefix = &path[..index];
        if let Some(target) = mappings.get(prefix)? {
            let suffix = path.get(index..).ok_or(VfsError::InvalidState)?;
            let mut translated = target;
            translated.extend_from_slice(suffix);
            return Ok(translated);
        }
    }
    Ok(path.to_vec())
}

fn has_mapped_ancestor(path: &[u8], mappings: &DiskNamespace<'_>) -> VfsResult<bool> {
    for (index, _) in path.iter().enumerate().filter(|(_, byte)| **byte == b'/') {
        if mappings.get(&path[..index])?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn mark_ambiguous(visible: &mut bool, error: &DriverError) {
    if matches!(
        error,
        DriverError::VisibilityAmbiguous | DriverError::DurabilityAmbiguous
    ) {
        *visible = true;
    }
}

fn checked_add(left: u64, right: u64) -> VfsResult<u64> {
    left.checked_add(right).ok_or(VfsError::InvalidState)
}

fn changed_ranges(
    engine: &Engine,
    old_root: ObjectId,
    new_root: ObjectId,
    ranges: &DiskNamespace<'_>,
    counters: &mut OperationCounters,
) -> VfsResult<(bool, u64)> {
    let mut count = 0_u64;
    let mut scratch_error = None;
    let result = diff_ranges(
        engine,
        FileStateRoot(old_root),
        FileStateRoot(new_root),
        |range| {
            if let Err(error) =
                ranges.put(&count.to_be_bytes(), &encode_range(range.start, range.end))
            {
                scratch_error = Some(error);
                return Err(layerfs_core::CoreError::Io);
            }
            count = count
                .checked_add(1)
                .ok_or(layerfs_core::CoreError::LengthOverflow)?;
            Ok(())
        },
    );
    if let Some(error) = scratch_error {
        return Err(error.into());
    }
    let (same_length, rope) = result?;
    counters.root_diff_nodes = counters
        .root_diff_nodes
        .checked_add(rope.nodes_read)
        .ok_or(VfsError::InvalidState)?;
    counters.add_rope(rope)?;
    Ok((same_length, count))
}

fn copy_target_range(
    engine: &Engine,
    target: ObjectId,
    start: u64,
    end: u64,
    output: &mut (impl Write + Seek + ?Sized),
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let mut offset = start;
    while offset < end {
        let stop = end.min(offset + 1024 * 1024);
        let mut bytes = Vec::with_capacity((stop - offset) as usize);
        let rope = read_range(engine, FileStateRoot(target), offset..stop, &mut bytes)?;
        counters.add_rope(rope)?;
        output.seek(SeekFrom::Start(offset))?;
        output.write_all(&bytes)?;
        counters.native.bytes_written += stop - offset;
        counters.native.patch_bytes += stop - offset;
        offset = stop;
    }
    Ok(())
}

fn encode_snapshot(value: Snapshot) -> [u8; SNAPSHOT_BYTES] {
    let mut bytes = [0; SNAPSHOT_BYTES];
    bytes[0] = value.kind as u8;
    bytes[1..33].copy_from_slice(value.inode.as_bytes());
    bytes[33..41].copy_from_slice(&value.namespace_ref_count.to_be_bytes());
    bytes[41..73].copy_from_slice(value.content_root.as_bytes());
    bytes[73..105].copy_from_slice(value.metadata_root.as_bytes());
    bytes
}

fn decode_snapshot(bytes: &[u8]) -> VfsResult<Snapshot> {
    if bytes.len() != SNAPSHOT_BYTES {
        return Err(VfsError::InvalidState);
    }
    Ok(Snapshot {
        kind: InodeKind::try_from(bytes[0])?,
        inode: InodeId::from_slice(&bytes[1..33])?,
        namespace_ref_count: u64::from_be_bytes(bytes[33..41].try_into().unwrap()),
        content_root: ObjectId::from_bytes(&bytes[41..73])?,
        metadata_root: ObjectId::from_bytes(&bytes[73..105])?,
    })
}

fn encode_range(start: u64, end: u64) -> [u8; 16] {
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(&start.to_be_bytes());
    bytes[8..].copy_from_slice(&end.to_be_bytes());
    bytes
}

fn decode_range(bytes: &[u8]) -> VfsResult<(u64, u64)> {
    if bytes.len() != 16 {
        return Err(VfsError::InvalidState);
    }
    Ok((
        u64::from_be_bytes(bytes[..8].try_into().unwrap()),
        u64::from_be_bytes(bytes[8..].try_into().unwrap()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_paths_support_the_valid_256_component_depth() {
        let parent = std::iter::repeat_n("x", 255).collect::<Vec<_>>().join("/");
        let child = format!("{parent}/x");
        let parent = CanonicalPath::new(&parent).unwrap();
        let child = CanonicalPath::new(&child).unwrap();

        assert!(
            ordered_path(parent.as_bytes(), false).unwrap()
                < ordered_path(child.as_bytes(), false).unwrap()
        );
        assert!(
            ordered_path(child.as_bytes(), true).unwrap()
                < ordered_path(parent.as_bytes(), true).unwrap()
        );
        assert!(ordered_path(b"", false).unwrap() < ordered_path(b"x", false).unwrap());
    }
}
