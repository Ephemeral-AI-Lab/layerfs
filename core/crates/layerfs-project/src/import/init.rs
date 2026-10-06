//! Scan, construct/save, consume inode identities and publish one genesis stack.
#[cfg(unix)]
use crate::{
    files,
    namespace::{self, ImportProgress},
    scan,
    scratch::Scratch,
};
use crate::{ProjectError, ProjectResult};
#[cfg(unix)]
use layerfs_content::filesystem::{profile_id, scope_for_seed};
use layerfs_content::ObjectId;
use layerfs_history::{HistoryCatalog, HistoryName, LayerStackId, LayerStackRecord};
#[cfg(unix)]
use layerfs_history::{ReserveRequest, StackInitialization};
#[cfg(unix)]
use layerfs_storage::Save;
use layerfs_storage::{read::Diagnostics, Storage};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{path::Path, time::Instant};
/// Explicit host authority and inputs for one namespace Init.
pub struct InitRequest<'a> {
    /// Host-visible regular directory, checked before construction.
    pub source: &'a Path,
    /// Existing writable parent for the operation-owned backed acquisition scratch.
    ///
    /// It must lie outside `source` through every path alias; a parent that is
    /// the source or inside it is refused before anything is created.
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
    /// Backed row counts and resident window capacities; not a whole-importer bound.
    pub namespace_work: crate::NamespaceWork,
}
/// Initializes one directory through the supplied ports, without an engine dependency.
///
/// Input-sized acquisition state is held in one operation-owned scratch
/// directory under `scratch_parent`. It is removed, with a checked result,
/// before the tree save is finished or any history is published.
#[cfg(unix)]
pub fn init(
    storage: &Storage,
    catalog: &dyn HistoryCatalog,
    request: InitRequest<'_>,
    timer: &TimingScope<'_, Active>,
) -> ProjectResult<Initialized> {
    let mut progress = ImportProgress::new(request.deadline);
    progress.tick()?;
    let source = scan::check_root(request.source)?;
    super::source::outside_source(&source, request.scratch_parent)?;
    let scope = scope_for_seed(request.scope_seed);
    let mut scratch = Scratch::create(request.scratch_parent)?;
    let acquired = acquire(
        storage,
        catalog,
        &request,
        &source,
        &mut scratch,
        &mut progress,
        timer,
    );
    progress.work.backing_bytes = scratch.peak_bytes();
    let (root, root_serial, entries, save) = match (acquired, scratch.finish()) {
        (Ok(value), Ok(())) => value,
        (Err(error), Ok(())) => return Err(error),
        (result, Err((error, retained))) => {
            return Err(ProjectError::Cleanup {
                cause: result.err().map(Box::new),
                error,
                retained,
            })
        }
    };
    progress.tick()?;
    timer
        .child("history.finish_tree_save")
        .run(|_| save.finish().map_err(crate::error::storage))?;
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
        root_serial,
        entries,
        diagnostics: storage.diagnostics(),
        namespace_work: progress.work,
    })
}

/// Scans, constructs each file once, consumes serials and builds the tree. The
/// returned tree save is unfinished: nothing final exists until scratch is gone.
#[cfg(unix)]
fn acquire<'a>(
    storage: &'a Storage,
    catalog: &dyn HistoryCatalog,
    request: &InitRequest<'_>,
    source: &std::fs::Metadata,
    scratch: &mut Scratch,
    progress: &mut ImportProgress,
    timer: &TimingScope<'_, Active>,
) -> ProjectResult<(ObjectId, u64, u64, Save<'a>)> {
    let (scanned, identities) = timer.child("history.import_scan").run(|_| {
        let (scanned, natives) = scan::scan(request.source, source, scratch, progress)?;
        let identities = scan::group_aliases(natives, scratch, progress)?;
        Ok::<_, ProjectError>((scanned, identities))
    })?;
    let save = storage.begin_save().map_err(crate::error::storage)?;
    let contents = timer
        .child("history.import_files")
        .run(|_| files::save_files(&identities, scratch, storage, &save, progress))?;
    progress.tick()?;
    timer
        .child("history.import_finish_save")
        .run(|_| save.finish().map_err(crate::error::storage))?;
    let scope = scope_for_seed(request.scope_seed);
    let reservation = timer.child("history.reserve_inodes").run(|_| {
        catalog
            .reserve_inodes(&ReserveRequest {
                scope: scope.object(),
                count: scanned.entries,
            })
            .map_err(ProjectError::History)
    })?;
    let provider = storage.reader().map_err(crate::error::storage)?;
    let (root, save) = namespace::build_namespace(
        storage,
        &provider,
        scope,
        reservation.start,
        &namespace::Acquired {
            scanned: &scanned,
            identities: &identities,
            contents: &contents,
        },
        scratch,
        progress,
        timer,
    )?;
    Ok((root, reservation.start, scanned.entries, save))
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
