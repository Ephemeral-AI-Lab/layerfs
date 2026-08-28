use super::*;

pub(crate) fn materialize_workspace_working(
    working: &WorkingStore,
    digest_cache: &SemanticDigestCache,
    workspace: &dyn ProjectionWorkspace,
    root: ObjectId,
) -> VfsResult<(OperationCounters, DiskTable)> {
    let mut counters = OperationCounters::default();
    counters.native.route = Some(NativeRoute::MaterializeStream);
    workspace.revalidate_root_binding()?;
    let root_handle = workspace.root_directory()?;
    if workspace
        .enumerate_at(root_handle.as_ref())?
        .next()
        .is_some()
    {
        let (verified, mut counters) =
            capture_workspace_candidate(working, digest_cache, workspace, root, None)?;
        if verified != root {
            return Err(VfsError::ExternalDirtyConflict);
        }
        workspace.revalidate_root_binding()?;
        counters.native.route = Some(NativeRoute::ExactNoop);
        let (live_scratch, authority_counters) =
            live_hard_link_authority_working(working, workspace, root)?;
        return Ok((counters.merge(authority_counters)?, live_scratch));
    }
    let scratch = working
        .create_scratch_table("materialize")
        .map_err(|error| VfsError::Io(std::io::Error::other(error.to_string())))?;
    let links = scratch.namespace(b"hard-links")?;
    let authority = scratch.namespace(b"authority")?;
    let topology = scratch.namespace(b"topology")?;
    let mut target = MaterializeTarget::Native {
        workspace,
        workspace_root: root_handle.as_ref(),
    };
    let root_metadata = visit_materialization_source(
        working,
        root,
        &mut target,
        Some(root_handle.as_ref()),
        &links,
        &authority,
        &topology,
        &mut counters,
    )?;
    workspace.set_root_metadata(&root_metadata)?;
    counters.native.metadata_calls = checked_add(counters.native.metadata_calls, 1)?;
    workspace.sync_directory(root_handle.as_ref())?;
    counters.native.sync_calls = checked_add(counters.native.sync_calls, 1)?;
    workspace.revalidate_root_binding()?;
    counters.add_scratch(scratch.observation()?)?;
    Ok((counters, scratch))
}
