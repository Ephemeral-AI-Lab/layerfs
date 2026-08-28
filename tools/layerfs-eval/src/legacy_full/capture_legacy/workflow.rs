use super::super::session_legacy::{VfsError, VfsResult};
use super::super::{NativeRoute, OperationCounters};
use super::hard_links::{existing_inode, seed_existing_hard_links, seed_existing_paths};
use super::{capture_directory, HardLink, SemanticDigestCache};
use layerfs_core::inode::InodeKind;
use layerfs_core::namespace::NamespaceRootV1;
use layerfs_core::namespace_codec::{encode_namespace_root, profile_id};
use layerfs_materialization::driver::*;
use layerfs_storage::refs::RefState;
use layerfs_storage::scratch::DiskNamespace;
use layerfs_storage::Engine;
pub(crate) fn capture_workspace(
    engine: &Engine,
    digest_cache: &SemanticDigestCache,
    workspace: &dyn ProjectionWorkspace,
    expected: Option<&RefState>,
    live_hard_links: Option<&DiskNamespace<'_>>,
    seed_live_hard_links: bool,
    require_same_root: bool,
) -> VfsResult<(RefState, OperationCounters)> {
    let mut counters = OperationCounters::default();
    counters.native.route = Some(NativeRoute::CaptureStream);
    counters.authority_full_scans = 1;
    workspace.revalidate_root_binding()?;
    let root_handle = workspace.root_directory()?;
    let root_token = workspace.directory_token(root_handle.as_ref())?;
    let existing_table = engine.create_scratch_table("existing-paths")?;
    let existing = existing_table.namespace(b"paths")?;
    let prior_table = expected
        .map(|expected| seed_existing_paths(engine, expected.root, &existing, None, &mut counters))
        .transpose()?;
    let seeded_links_table = engine.create_scratch_table("existing-hardlinks")?;
    let seeded_links = seeded_links_table.namespace(b"links")?;
    if seed_live_hard_links || (prior_table.is_some() && live_hard_links.is_none()) {
        seed_existing_hard_links(
            workspace,
            root_handle.as_ref(),
            &existing,
            &seeded_links,
            &[],
            !seed_live_hard_links,
        )?;
    }
    let existing_links = live_hard_links.or(seed_live_hard_links.then_some(&seeded_links));
    let prior_links = live_hard_links.or(prior_table.map(|_| &seeded_links));
    let mut publication = engine.begin_publication(expected, "main")?;
    let root_inode = match existing_inode(&existing, b"", InodeKind::Directory)? {
        Some(inode) => inode,
        None => publication.allocate_inode_id()?,
    };
    let mut table = None;
    let hard_links = engine.create_scratch_table("hardlinks")?;
    let entries = engine.create_scratch_table("enumeration")?;
    let mut next_directory = 0_u64;
    let root_metadata = workspace.read_root_metadata()?;
    capture_directory(
        workspace,
        root_handle.as_ref(),
        digest_cache,
        root_inode,
        true,
        root_metadata,
        &mut publication,
        &mut table,
        &hard_links,
        &entries,
        &existing,
        existing_links,
        prior_links,
        prior_table,
        &[],
        &mut next_directory,
        &mut counters,
    )?;
    if workspace.directory_token(root_handle.as_ref())? != root_token {
        return Err(DriverError::Conflict.into());
    }
    workspace.revalidate_root_binding()?;
    hard_links
        .for_each(|bytes| {
            let link = HardLink::decode(bytes).map_err(|_| {
                layerfs_storage::EngineError::InvalidRecord("hard-link scratch value")
            })?;
            if link.observed != link.expected {
                return Err(layerfs_storage::EngineError::InvalidRecord(
                    "external hard-link boundary",
                ));
            }
            Ok(())
        })
        .map_err(|error| {
            if matches!(
                error,
                layerfs_storage::EngineError::InvalidRecord("external hard-link boundary")
            ) {
                VfsError::ExternalHardLinkBoundary
            } else {
                error.into()
            }
        })?;
    let namespace = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: table.ok_or(VfsError::InvalidState)?.into_root().0,
    })?;
    workspace.revalidate_root_binding()?;
    if require_same_root
        && expected.is_none_or(|state| layerfs_core::ObjectId::for_bytes(&namespace) != state.root)
    {
        return Err(VfsError::ExternalDirtyConflict);
    }
    let next = publication.publish_namespace(&namespace)?;
    for scratch in [existing_table, seeded_links_table, hard_links, entries] {
        super::super::add_scratch(&mut counters, scratch.finish()?)?;
    }
    Ok((next, counters))
}
