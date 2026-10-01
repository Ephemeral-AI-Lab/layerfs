//! Existing inode values, directory metadata and fresh declarations under one save.
//!
//! The prepared-update surface is shared by the legacy content operation and by
//! history staging. It is the same production body in both cases: one builder,
//! one ownership rule, one place where a supplied root is checked against the
//! base tree. Callers validate fresh file/symlink roles; history validates every inode role.
//!
//! History arrives as a declared stream rather than as resident rows, so the
//! update is received into a charged [`RowSpool`] first and C1 is then fed from
//! that spool. The base precondition the resident form checked over a collected
//! serial list is checked in waves over the same sealed spool. Receive/subject
//! checking retains scalar names and bounded waves; C1's remaining graph and
//! reference populations have their separately recorded admission/proof gates.
use super::prepared::{check_subjects, receive, Received};
use crate::service::{error::content, read::content::id};
use layerfs_bridge::contract::*;
use layerfs_content::filesystem::rows::{
    PreparedBindingUpdate as StreamedUpdate, SpoolPreparation,
};
use layerfs_content::filesystem::state::{CanonicalConstructionState, GraphConstructionScopes};
use layerfs_content::filesystem::{root::FilesystemRootId, FilesystemRead, InodeScope};
use layerfs_content::{
    AuthenticatedObjects, FilesystemObjects, FilesystemResources, FinalizedConsumer,
};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{io::Read, time::Instant};

/// One checked prepared update: an immutable base root and the stream of rows
/// that change it.
pub(crate) struct PreparedUpdate<'a> {
    /// Immutable base root the update is applied to.
    pub(crate) base: Root,
    /// Allocation scope the base root records.
    pub(crate) scope: Root,
    /// Root directory serial of the base tree.
    pub(crate) root_serial: u64,
    /// The request that declared the stream.
    pub(crate) changes: &'a PreparedChanges,
    /// The ordered body the request declared.
    pub(crate) body: &'a mut dyn Read,
    /// Unique admitted source authority moved into the receive spool once.
    pub(crate) preparation: Option<SpoolPreparation>,
}

pub(crate) fn update(
    provider: &dyn AuthenticatedObjects,
    update: PreparedUpdate<'_>,
    consumer: &mut dyn FinalizedConsumer,
    state: &mut dyn CanonicalConstructionState,
    state_scope: &GraphConstructionScopes,
    deadline: Instant,
    scope: &TimingScope<'_, Active>,
) -> Result<(Root, u64), Failure> {
    let base = update.base;
    let allocation = update.scope;
    let root_serial = update.root_serial;
    let mut fs = FilesystemRead::new(provider, FilesystemRootId(id(&base))).map_err(content)?;
    if fs.root().scope().object() != id(&allocation)
        || fs.root().root_inode().serial() != root_serial
    {
        return Err(Code::InvalidInput.into());
    }
    let mut objects = FilesystemObjects::new(provider, consumer);
    // The body is validated and spooled once, and every row is given the
    // canonical work it needs on the way in: the role check on a saved file's
    // roots, the portable patch of a maintained directory, the first metadata of
    // a declared one.
    let received = receive(&mut objects, &mut fs, provider, scope, update, deadline)?;
    let Received { rows } = received;
    let mut rows = rows;
    let resources = FilesystemResources::default();
    check_subjects(&mut fs, &rows, resources.base_read_batch).map_err(content)?;
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    let input = StreamedUpdate {
        base: Some(FilesystemRootId(id(&base))),
        scope: InodeScope::from_object(id(&allocation)),
        root_serial,
        resources,
        rows: &rows,
    };
    let result =
        layerfs_content::filesystem::update::update_filesystem_binding_rows_with_canonical_state(
            &mut objects,
            &input,
            None,
            state,
            state_scope,
            &layerfs_content::filesystem::FilesystemPhases::new(scope),
        );
    let cleaned = rows.cleanup();
    let result = result.map_err(content)?;
    cleaned.map_err(content)?;
    Ok((*result.root.0.as_bytes(), 0))
}

/// The exact ALL-ZERO body still runs authenticated namespace validation and phases.
pub(crate) fn update_empty(
    provider: &dyn AuthenticatedObjects,
    update: PreparedUpdate<'_>,
    consumer: &mut dyn FinalizedConsumer,
    state: &mut layerfs_content::filesystem::state::VerifiedEmptyState,
    state_scope: &GraphConstructionScopes,
    deadline: Instant,
    scope: &TimingScope<'_, Active>,
) -> Result<(Root, u64), Failure> {
    let base = update.base;
    let allocation = update.scope;
    let root_serial = update.root_serial;
    let fs = FilesystemRead::new(provider, FilesystemRootId(id(&base))).map_err(content)?;
    if fs.root().scope().object() != id(&allocation)
        || fs.root().root_inode().serial() != root_serial
    {
        return Err(Code::InvalidInput.into());
    }
    let rows = crate::service::empty_receive::receive(
        update.changes,
        update.body,
        update.preparation.ok_or(Code::Ownership)?,
        state,
        deadline,
    )?;
    let input = StreamedUpdate {
        base: Some(FilesystemRootId(id(&base))),
        scope: InodeScope::from_object(id(&allocation)),
        root_serial,
        resources: FilesystemResources::default(),
        rows: &rows,
    };
    let result =
        layerfs_content::filesystem::update::update_filesystem_binding_rows_with_namespace_state(
            &mut FilesystemObjects::new(provider, consumer),
            &input,
            None,
            state,
            state_scope,
            &layerfs_content::filesystem::FilesystemPhases::new(scope),
        )
        .map_err(content)?;
    if !state.completed() {
        return Err(Code::Ownership.into());
    }
    rows.cleanup().map_err(content)?;
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    if std::env::var_os("LAYERFS_COMPLEXITY_DIAGNOSTIC").is_some() {
        let physical = state.physical();
        eprintln!("LFS_CONSTRUCTION_PLAN v=1 plan=verified-empty native_files={} reserved_bytes={} allocated_bytes={} native_cleanup={} working_bytes={}",
            physical.native_files, physical.reserved_bytes, physical.allocated_bytes,
            physical.cleanup_events, state.working_bytes());
    }
    Ok((*result.root.0.as_bytes(), 0))
}

/// Exact <=8 ordinary existing-file updates use the same Canonical8 coordinator.
pub(crate) fn update_small(
    provider: &dyn AuthenticatedObjects,
    update: PreparedUpdate<'_>,
    consumer: &mut dyn FinalizedConsumer,
    pending: layerfs_content::filesystem::rows::PendingSmallFiles,
    deadline: Instant,
    scope: &TimingScope<'_, Active>,
) -> Result<(Root, u64), Failure> {
    if update.preparation.is_some() {
        return Err(Code::Ownership.into());
    }
    let (rows, mut state) = crate::service::small_file_receive::receive(
        update.changes,
        update.body,
        pending,
        provider,
        deadline,
        scope,
    )?;
    let scopes = state.scopes().clone();
    let input = StreamedUpdate {
        base: Some(FilesystemRootId(id(&update.base))),
        scope: InodeScope::from_object(id(&update.scope)),
        root_serial: update.root_serial,
        resources: FilesystemResources::default(),
        rows: &rows,
    };
    let result =
        layerfs_content::filesystem::update::update_filesystem_binding_rows_with_canonical_state(
            &mut FilesystemObjects::new(provider, consumer),
            &input,
            None,
            &mut state,
            &scopes,
            &layerfs_content::filesystem::FilesystemPhases::new(scope),
        )
        .map_err(content)?;
    if !state.completed() {
        return Err(Code::Ownership.into());
    }
    rows.cleanup().map_err(content)?;
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    if std::env::var_os("LAYERFS_COMPLEXITY_DIAGNOSTIC").is_some() {
        let physical = state.physical();
        eprintln!("LFS_CONSTRUCTION_PLAN v=1 plan=verified-small-files native_files={} reserved_bytes={} allocated_bytes={} native_cleanup={} working_bytes={}",
            physical.native_files, physical.reserved_bytes, physical.allocated_bytes,
            physical.cleanup_events, state.working_bytes());
    }
    Ok((*result.root.0.as_bytes(), 0))
}
