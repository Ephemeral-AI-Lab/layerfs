use crate::driver::{DriverError, ProjectionWorkspace};
use crate::managed_edit::mutate_native;
use crate::{OperationCounters, VfsError, VfsResult};
use layerfs_core::content::rope::{diff_ranges, read_range, FileStateRoot, ObjectRead};
use layerfs_core::inode::{InodeId, InodeKind};
use layerfs_core::logical::{diff_roots, RootDiff};
use layerfs_core::namespace_codec::{decode_inode_record, decode_namespace_root};
use layerfs_core::{CanonicalPath, ObjectId};
use layerfs_workspace::{DiskNamespace, DiskTable, WorkingStore};

const MAX_LOCAL_REFRESH_BYTES: u64 = 1024 * 1024;
const MAX_LOCAL_REFRESH_RANGES: usize = 16;
const MAX_PATH_DEPTH: usize = 256;

pub(crate) fn refresh_workspace_working(
    working: &WorkingStore,
    workspace: &dyn ProjectionWorkspace,
    topology_table: &DiskTable,
    base: ObjectId,
    target: ObjectId,
) -> VfsResult<OperationCounters> {
    let mut counters = OperationCounters::default();
    if base == target {
        counters.native.route = Some(crate::NativeRoute::ExactNoop);
        return Ok(counters);
    }

    let mut changes = Vec::with_capacity(2);
    let logical = diff_roots(working, base, target, |change| {
        if changes.len() == 2 {
            return Err(layerfs_core::CoreError::ObjectLimitExceeded);
        }
        changes.push(change);
        Ok(())
    })?;
    counters.add_inode_table(logical.inode_table)?;
    counters.root_diff_nodes = logical.inode_table.nodes_read;
    let [RootDiff {
        inode,
        before: Some(before),
        after: Some(after),
    }] = changes.as_slice()
    else {
        return Err(DriverError::Unsupported.into());
    };
    let before = working.with_authenticated_canonical(*before, decode_inode_record)?;
    let after = working.with_authenticated_canonical(*after, decode_inode_record)?;
    if before.kind != InodeKind::RegularFile
        || after.kind != InodeKind::RegularFile
        || before.namespace_ref_count != 1
        || after.namespace_ref_count != 1
        || before.metadata_root != after.metadata_root
    {
        return Err(DriverError::Unsupported.into());
    }

    let mut ranges = Vec::new();
    let (same_length, rope) = diff_ranges(
        working,
        FileStateRoot(before.content_root),
        FileStateRoot(after.content_root),
        |range| {
            if ranges.len() == MAX_LOCAL_REFRESH_RANGES {
                return Err(layerfs_core::CoreError::ObjectLimitExceeded);
            }
            ranges.push(range);
            Ok(())
        },
    )?;
    counters.add_rope(rope)?;
    let changed_bytes = ranges.iter().try_fold(0_u64, |total, range| {
        total
            .checked_add(range.end - range.start)
            .ok_or(VfsError::InvalidState)
    })?;
    if !same_length || changed_bytes > MAX_LOCAL_REFRESH_BYTES {
        return Err(DriverError::Unsupported.into());
    }

    let namespace = working.with_authenticated_canonical(base, decode_namespace_root)?;
    let topology = topology_table.namespace(b"topology")?;
    let path = topology_path(&topology, *inode, namespace.root_directory_inode)?;
    for range in ranges {
        let mut replacement = Vec::with_capacity(
            usize::try_from(range.end - range.start).map_err(|_| VfsError::InvalidState)?,
        );
        let read = read_range(
            working,
            FileStateRoot(after.content_root),
            range.clone(),
            &mut replacement,
        )?;
        counters.add_rope(read)?;
        let (_, native, fallback) = mutate_native(
            workspace,
            &path,
            range.start,
            range.end - range.start,
            &replacement,
        )?;
        if fallback {
            counters.full_fallback_files = counters
                .full_fallback_files
                .checked_add(1)
                .ok_or(VfsError::InvalidState)?;
        }
        counters.add_native(native)?;
    }
    counters.changed_paths = 1;
    Ok(counters)
}

fn topology_path(
    topology: &DiskNamespace<'_>,
    mut inode: InodeId,
    root: InodeId,
) -> VfsResult<CanonicalPath> {
    let mut components: Vec<Vec<u8>> = Vec::new();
    for _ in 0..MAX_PATH_DEPTH {
        if inode == root {
            components.reverse();
            let mut bytes = Vec::new();
            for component in components {
                if !bytes.is_empty() {
                    bytes.push(b'/');
                }
                bytes.extend_from_slice(&component);
            }
            return Ok(CanonicalPath::from_bytes(&bytes)?);
        }
        let mut edge = None;
        topology.for_each_entry_prefix(inode.as_bytes(), |key, _| {
            if key.len() <= 64 || edge.is_some() {
                return Err(layerfs_workspace::StorageError::InvalidRecord(
                    "refresh topology edge",
                ));
            }
            let parent = InodeId::from_slice(&key[32..64]).map_err(|_| {
                layerfs_workspace::StorageError::InvalidRecord("refresh topology inode")
            })?;
            edge = Some((parent, key[64..].to_vec()));
            Ok(())
        })?;
        let (parent, name) = edge.ok_or(VfsError::InvalidState)?;
        components.push(name);
        inode = parent;
    }
    Err(VfsError::InvalidState)
}
