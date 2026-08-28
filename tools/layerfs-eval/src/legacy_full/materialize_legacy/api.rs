use super::super::capture_legacy::{
    capture_workspace, live_hard_link_authority, SemanticDigestCache,
};
use super::super::session_legacy::{VfsError, VfsResult};
use super::super::{NativeRoute, OperationCounters};
use layerfs_core::ObjectId;
use layerfs_materialization::driver::*;
use layerfs_storage::scratch::DiskTable;
use layerfs_storage::Engine;
use std::io::{Read, Write};
use std::path::Path;

use super::output::{checked_add, project_regular_file};
use super::traversal::{visit_materialization_source, MaterializeTarget};
pub(crate) fn materialize_workspace(
    engine: &Engine,
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
        let expected = engine.read_ref("main")?.ok_or(VfsError::InvalidState)?;
        if expected.root != root {
            return Err(VfsError::ExternalDirtyConflict);
        }
        let (verified, mut counters) = capture_workspace(
            engine,
            digest_cache,
            workspace,
            Some(&expected),
            None,
            true,
            true,
        )?;
        if verified.root != root {
            return Err(VfsError::ExternalDirtyConflict);
        }
        workspace.revalidate_root_binding()?;
        counters.native.route = Some(NativeRoute::ExactNoop);
        let (live_scratch, authority_counters) = live_hard_link_authority(engine, workspace, root)?;
        return Ok((counters.merge(authority_counters)?, live_scratch));
    }
    let scratch = engine.create_scratch_table("materialize")?;
    let links = scratch.namespace(b"hard-links")?;
    let authority = scratch.namespace(b"authority")?;
    let topology = scratch.namespace(b"topology")?;
    let mut target = MaterializeTarget::Native {
        workspace,
        workspace_root: root_handle.as_ref(),
    };
    let root_metadata = visit_materialization_source(
        engine,
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
    super::super::add_scratch(&mut counters, scratch.observation()?)?;
    Ok((counters, scratch))
}

/// Runs the exact canonical source side of full materialization and sends each
/// unique regular-file payload to `output`, without opening a native workspace.
pub fn materialize_authenticated_to<W: Write>(
    engine: &Engine,
    root: ObjectId,
    mut output: W,
) -> VfsResult<OperationCounters> {
    let mut counters = OperationCounters::default();
    let scratch = engine.create_scratch_table("materialize")?;
    let links = scratch.namespace(b"hard-links")?;
    let authority = scratch.namespace(b"authority")?;
    let topology = scratch.namespace(b"topology")?;
    let mut target = MaterializeTarget::Sink(&mut output);
    visit_materialization_source(
        engine,
        root,
        &mut target,
        None,
        &links,
        &authority,
        &topology,
        &mut counters,
    )?;
    output.flush()?;
    super::super::add_scratch(&mut counters, scratch.finish()?)?;
    Ok(counters)
}

impl super::super::session_legacy::LayerFs {
    pub fn materialize_authenticated_to<W: Write>(
        &self,
        root: ObjectId,
        output: W,
    ) -> VfsResult<OperationCounters> {
        let reservation = self.operation_q.reserve();
        let mut counters = materialize_authenticated_to(&self.engine, root, output)?;
        reservation.finish(&mut counters);
        Ok(counters)
    }

    pub fn native_durable_output<R: Read>(
        &self,
        path: &Path,
        name: &[u8],
        metadata: &NativeMetadata,
        logical_len: u64,
        input: R,
    ) -> VfsResult<OperationCounters> {
        let reservation = self.operation_q.reserve();
        let mut counters = native_durable_output(
            self.projection_driver(),
            self.engine.store_id()?,
            path,
            name,
            metadata,
            logical_len,
            input,
        )?;
        reservation.finish(&mut counters);
        Ok(counters)
    }
}

/// Projects one exact-length bounded stream through the same native regular-file
/// temp, metadata, install, and sync route used by full materialization.
pub fn native_durable_output<R: Read>(
    driver: &dyn ProjectionDriver,
    store_id: [u8; 32],
    path: &Path,
    name: &[u8],
    metadata: &NativeMetadata,
    logical_len: u64,
    input: R,
) -> VfsResult<OperationCounters> {
    let projection_before = driver.projection_facts();
    let workspace = driver.open_workspace(path, WorkspacePolicy::ExternalCooperative, store_id)?;
    workspace.revalidate_root_binding()?;
    let root = workspace.root_directory()?;
    if workspace
        .enumerate_at(root.as_ref())?
        .next()
        .transpose()?
        .is_some()
    {
        return Err(VfsError::InvalidState);
    }
    let mut preflight = workspace.begin_name_preflight()?;
    preflight.add(name)?;
    preflight.finish()?;

    let mut counters = OperationCounters::default();
    counters.native.route = Some(NativeRoute::NativeDurableOutput);
    let mut input = input.take(logical_len);
    project_regular_file(
        workspace.as_ref(),
        root.as_ref(),
        name,
        metadata,
        DirectoryDurability::ImmediateDirectoryDurability,
        |output| {
            let written = std::io::copy(&mut input, output)?;
            if written != logical_len {
                return Err(VfsError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "native durable source ended before its declared length",
                )));
            }
            Ok(((), written))
        },
        &mut counters,
    )?;
    workspace.revalidate_root_binding()?;
    counters.projection = driver
        .projection_facts()
        .checked_delta(projection_before)
        .ok_or(VfsError::InvalidState)?;
    Ok(counters)
}
