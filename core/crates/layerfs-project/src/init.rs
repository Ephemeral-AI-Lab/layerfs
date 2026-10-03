//! Scan, construct/save, consume inode identities and publish one genesis stack.
#[cfg(unix)]
use crate::{
    namespace::{self, ImportProgress},
    scan,
};
use crate::{ProjectError, ProjectResult};
#[cfg(unix)]
use layerfs_content::filesystem::{profile_id, scope_for_seed};
use layerfs_content::ObjectId;
use layerfs_history::{HistoryCatalog, HistoryName, LayerStackId, LayerStackRecord};
#[cfg(unix)]
use layerfs_history::{ReserveRequest, StackInitialization};
use layerfs_storage::{read::Diagnostics, Storage};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{path::Path, time::Instant};
/// Explicit host authority and inputs for one namespace Init.
pub struct InitRequest<'a> {
    /// Host-visible regular directory, checked before construction.
    pub source: &'a Path,
    /// Existing writable parent for operation-owned ordering scratch.
    pub scratch_parent: &'a Path,
    /// LayerStack authority identity selected by the application.
    pub stack: LayerStackId,
    /// Checked portable name for the new LayerStack.
    pub name: HistoryName,
    /// Explicit seed for the canonical inode allocation scope.
    pub scope_seed: [u8; 32],
    /// Fixed operation deadline, shared by construction workers.
    pub deadline: Instant,
}
/// Acknowledged namespace initialization and its actual storage counts.
#[derive(Debug)]
pub struct Initialized {
    /// Genesis LayerStack acknowledged by C5.
    pub stack: LayerStackRecord,
    /// Canonical filesystem root saved through C2.
    pub root: ObjectId,
    /// First serial consumed in the namespace's allocation scope.
    pub root_serial: u64,
    /// Number of scanned source entries, including the root directory.
    pub entries: u64,
    /// Cumulative actual storage counts for the supplied handle; diagnostics.
    pub diagnostics: Diagnostics,
}
/// Initializes one directory through the supplied ports, without an engine dependency.
#[cfg(unix)]
pub fn init(
    storage: &Storage,
    catalog: &dyn HistoryCatalog,
    request: InitRequest<'_>,
    timer: &TimingScope<'_, Active>,
) -> ProjectResult<Initialized> {
    let mut progress = ImportProgress::new(request.deadline);
    progress.tick()?;
    let entries = scan::scan_and_save(request.source, storage, &mut progress, timer)?;
    let count = u64::try_from(entries.len()).map_err(|_| ProjectError::Capacity)?;
    let scope = scope_for_seed(request.scope_seed);
    let reservation = timer.child("history.reserve_inodes").run(|_| {
        catalog
            .reserve_inodes(&ReserveRequest {
                scope: scope.object(),
                count,
            })
            .map_err(ProjectError::History)
    })?;
    let provider = storage.reader().map_err(crate::error::storage)?;
    let root = namespace::build_namespace(
        request.scratch_parent,
        storage,
        &provider,
        scope,
        reservation.start,
        &entries,
        true,
        &mut progress,
        timer,
    )?;
    progress.tick()?;
    let stack = timer.child("history.initialize_layerstack").run(|_| {
        catalog
            .initialize_layerstack(&StackInitialization {
                stack: request.stack,
                name: request.name,
                scope: scope.object(),
                profile: profile_id(),
                genesis_root: root,
            })
            .map_err(ProjectError::History)
    })?;
    Ok(Initialized {
        stack,
        root,
        root_serial: reservation.start,
        entries: count,
        diagnostics: storage.diagnostics(),
    })
}

/// Refuses native Init where the required Unix metadata contract is unavailable.
#[cfg(not(unix))]
pub fn init(
    _storage: &Storage,
    _catalog: &dyn HistoryCatalog,
    _request: InitRequest<'_>,
    _timer: &TimingScope<'_, Active>,
) -> ProjectResult<Initialized> {
    Err(ProjectError::Unsupported)
}
