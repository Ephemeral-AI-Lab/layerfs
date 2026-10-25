//! Scan, construct/save, consume inode identities and publish one genesis stack.
use crate::import::observe::InitCounters;
use crate::import::observe::InitStep;
use crate::{
    backing::Backing,
    error::{acquisition, storage},
    files,
    namespace::{self, ImportProgress},
    scan,
};
use crate::{ProjectError, ProjectResult};
use layerfs_content::filesystem::{profile_id, scope_for_seed, FilesystemObjects};
use layerfs_content::ObjectId;
use layerfs_history::{HistoryCatalog, HistoryName, LayerStackId, LayerStackRecord};
use layerfs_history::{ReserveRequest, StackInitialization};
use layerfs_storage::port::acquisition::Acquisition;
use layerfs_storage::port::acquisition::Begin;
use layerfs_storage::Save;
use layerfs_storage::{read::Diagnostics, Storage};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{path::Path, sync::Arc, time::Instant};
/// Explicit host authority and inputs for one namespace Init.
pub struct InitRequest<'a> {
    /// Host-visible regular directory, checked before construction.
    pub source: &'a Path,
    /// Provider-owned working state for the acquisition's input-sized rows.
    ///
    /// Where the provider reports a host placement, that directory must lie
    /// outside `source` through every path alias; one that is the source or
    /// inside it is refused before the operation begins.
    pub acquisition: &'a dyn Acquisition,
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
    /// Backed row and unit counts with resident window high-water marks; not a
    /// whole-importer bound.
    pub namespace_work: crate::NamespaceWork,
}
/// Initializes one directory through the supplied ports, without an engine dependency.
///
/// As [`init_observed`] with counters nobody reads.
pub fn init(
    store: &Storage,
    catalog: &dyn HistoryCatalog,
    request: InitRequest<'_>,
    timer: &TimingScope<'_, Active>,
) -> ProjectResult<Initialized> {
    init_observed(
        store,
        catalog,
        request,
        timer,
        Arc::new(InitCounters::new()),
    )
}

/// Initializes one directory, updating `counters` as it runs so another
/// thread can show how far it has come.
///
/// Input-sized acquisition state is held as one operation's rows behind
/// `acquisition`. Those rows and the operation record are removed, with a
/// checked result, before the tree save is finished or any history is
/// published. A failure of that removal is [`ProjectError::Cleanup`]; an
/// unknown Store, history or backing outcome is [`ProjectError::Uncertain`]
/// and leaves the operation untouched for its owner.
pub fn init_observed(
    store: &Storage,
    catalog: &dyn HistoryCatalog,
    request: InitRequest<'_>,
    timer: &TimingScope<'_, Active>,
    counters: Arc<InitCounters>,
) -> ProjectResult<Initialized> {
    counters.step(InitStep::Scan);
    let mut progress = ImportProgress::observed(request.deadline, counters);
    progress.tick()?;
    let source = scan::check_root(request.source)?;
    if let Some(placement) = request.acquisition.placement() {
        super::source::outside_source(request.source, &source, &placement)?;
    }
    let scope = scope_for_seed(request.scope_seed);
    let mut stack = [0; 16];
    stack.copy_from_slice(&request.stack.to_bytes()[1..]);
    let host = super::host::HostStat::of(&request.source, &source)?;
    let begun = request.acquisition.begin(&Begin {
        source_device: host.dev,
        source_inode: host.ino,
        stack,
        scope: scope.object(),
    });
    let backing = Backing::new(request.acquisition, begun.map_err(acquisition)?);
    let acquired = acquire(
        store,
        catalog,
        &request,
        &source,
        &backing,
        &mut progress,
        timer,
    );
    let (root, root_serial, entries, save) = match acquired {
        Ok(value) => value,
        Err(error) => return Err(backing.settle(error, &mut progress.work)),
    };
    progress.counters.step(InitStep::Publish);
    // Nothing final exists until the working rows and their record are gone.
    timer
        .child("history.discard_acquisition")
        .run(|_| backing.finish(&mut progress.work))
        .map_err(|cleanup| cleanup.after(None))?;
    progress.tick()?;
    timer
        .child("history.finish_tree_save")
        .run(|_| save.finish().map_err(storage))?;
    progress.tick()?;
    let reader = store.reader().map_err(storage)?;
    let logical = layerfs_content::filesystem::statistics::logical_size(
        &reader,
        layerfs_content::filesystem::FilesystemRootId(root),
    )
    .map_err(ProjectError::Content)?;
    timer.child("history.record_bindings").run(|_| {
        super::hints::record_new_bindings(
            &reader,
            catalog,
            None,
            layerfs_content::filesystem::FilesystemRootId(root),
        )
    })?;
    let stack = timer.child("history.initialize_layerstack").run(|_| {
        catalog
            .initialize_layerstack(&StackInitialization {
                stack: request.stack,
                name: request.name,
                scope: scope.object(),
                profile: profile_id(),
                genesis_root: root,
                logical,
            })
            .map_err(ProjectError::History)
    })?;
    Ok(Initialized {
        stack,
        root,
        root_serial,
        entries,
        diagnostics: store.diagnostics(),
        namespace_work: progress.work,
    })
}

/// Scans with attributes, constructs each file once, consumes serials and
/// builds the tree. The returned tree save is unfinished.
fn acquire<'a>(
    store: &'a Storage,
    catalog: &dyn HistoryCatalog,
    request: &InitRequest<'_>,
    source: &std::fs::Metadata,
    backing: &Backing<'_>,
    progress: &mut ImportProgress,
    timer: &TimingScope<'_, Active>,
) -> ProjectResult<(ObjectId, u64, u64, Save<'a>)> {
    let scanned = {
        let provider = store.reader().map_err(storage)?;
        let save = store.begin_save().map_err(storage)?;
        let built = timer.child("history.import_scan").run(|_| {
            let mut sink = save.sink();
            let mut objects = FilesystemObjects::new(&provider, &mut sink);
            scan::scan(request.source, source, backing, &mut objects, progress)
        });
        let scanned = match save.take_failure() {
            Some(error) => Err(storage(error)),
            None => built,
        }?;
        progress.tick()?;
        timer
            .child("history.finish_prerequisite_save")
            .run(|_| save.finish().map_err(storage))?;
        scanned
    };
    let save = store.begin_save().map_err(storage)?;
    timer
        .child("history.import_files")
        .run(|_| files::save_files(backing, scanned.aliases, store, &save, progress))?;
    progress.tick()?;
    timer
        .child("history.import_finish_save")
        .run(|_| save.finish().map_err(storage))?;
    progress.counters.step(InitStep::Tree);
    let scope = scope_for_seed(request.scope_seed);
    let reservation = timer.child("history.reserve_inodes").run(|_| {
        catalog
            .reserve_inodes(&ReserveRequest {
                scope: scope.object(),
                count: scanned.entries,
            })
            .map_err(ProjectError::History)
    })?;
    let provider = store.reader().map_err(storage)?;
    let (root, save) = timer.child("history.import_tree").run(|_| {
        namespace::build_namespace(
            store,
            &provider,
            scope,
            reservation.start,
            &scanned,
            backing,
            progress,
        )
    })?;
    Ok((root, reservation.start, scanned.entries, save))
}

/// Refuses host Init where the host metadata contract is unavailable.
#[cfg(not(any(unix, target_os = "windows")))]
pub fn init_observed(
    _storage: &Storage,
    _catalog: &dyn HistoryCatalog,
    _request: InitRequest<'_>,
    _timer: &TimingScope<'_, Active>,
    _counters: Arc<InitCounters>,
) -> ProjectResult<Initialized> {
    Err(ProjectError::Unsupported)
}
