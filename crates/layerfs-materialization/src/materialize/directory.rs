use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn materialize_directory(
    target: &mut MaterializeTarget<'_>,
    engine: &impl ObjectRead,
    table: InodeTableRoot,
    directory_inode: InodeId,
    directory: DirectoryStateRoot,
    parent: Option<&dyn DirectoryHandle>,
    links: &DiskNamespace<'_>,
    authority: &DiskNamespace<'_>,
    topology: &DiskNamespace<'_>,
    current_path: &[u8],
    counters: &mut OperationCounters,
) -> VfsResult<()> {
    let MaterializeTarget::Native { workspace, .. } = target;
    let mut preflight = Some(workspace.begin_name_preflight()?);
    let mut error = None;
    let mut namespace_counters = NamespaceCounters::default();
    let visited = visit_directory_entries(engine, directory, &mut namespace_counters, |entries| {
        for (name, _) in entries {
            if let Some(preflight) = preflight.as_mut() {
                if let Err(cause) = preflight.add(name.as_bytes()) {
                    error = Some(VfsError::Driver(cause));
                    return Err(layerfs_core::CoreError::Io);
                }
            }
        }
        Ok(())
    });
    if let Some(error) = error {
        return Err(error);
    }
    visited?;
    counters.add_namespace(namespace_counters)?;
    if let Some(preflight) = preflight {
        preflight.finish()?;
    }

    let mut error = None;
    let mut namespace_counters = NamespaceCounters::default();
    let visited = visit_directory_entries(engine, directory, &mut namespace_counters, |entries| {
        for (name, inode) in entries {
            if let Err(cause) = topology.put(
                &topology_edge_key(*inode, directory_inode, name.as_bytes()),
                &[],
            ) {
                error = Some(cause.into());
                return Err(layerfs_core::CoreError::Io);
            }
            if let Err(cause) = materialize_entry(
                target,
                engine,
                table,
                parent,
                links,
                authority,
                topology,
                current_path,
                name.as_bytes(),
                *inode,
                counters,
            ) {
                error = Some(cause);
                return Err(layerfs_core::CoreError::Io);
            }
        }
        Ok(())
    });
    if let Some(error) = error {
        return Err(error);
    }
    visited?;
    counters.add_namespace(namespace_counters)?;
    Ok(())
}
