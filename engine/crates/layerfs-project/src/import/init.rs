//! Scan, construct/save, consume inode identities and publish one genesis stack.
use crate::import::observe::InitCounters;
#[cfg(unix)]
use crate::import::observe::InitStep;
#[cfg(unix)]
use crate::{
    backing::Backing,
    error::{acquisition, storage},
    files,
    namespace::{self, ImportProgress},
    scan,
};
use crate::{ProjectError, ProjectResult};
#[cfg(unix)]
use layerfs_content::filesystem::{profile_id, scope_for_seed, FilesystemObjects};
use layerfs_content::ObjectId;
#[cfg(unix)]
use layerfs_history::records::GcScopeKind;
use layerfs_history::{HistoryCatalog, HistoryName, LayerStackId, LayerStackRecord};
#[cfg(unix)]
use layerfs_history::{ReserveRequest, StackInitialization, UnknownOutcomeState};
use layerfs_storage::port::acquisition::Acquisition;
#[cfg(unix)]
use layerfs_storage::port::acquisition::Begin;
#[cfg(unix)]
use layerfs_storage::Save;
use layerfs_storage::{read::Diagnostics, Storage};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{fmt::Write as _, path::Path, sync::Arc, time::Instant};
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
#[cfg(unix)]
pub fn init_observed(
    store: &Storage,
    catalog: &dyn HistoryCatalog,
    request: InitRequest<'_>,
    timer: &TimingScope<'_, Active>,
    counters: Arc<InitCounters>,
) -> ProjectResult<Initialized> {
    use std::os::unix::fs::MetadataExt;
    // Admission: an Init shares the project scope with removals and is
    // visible as a holder until it finishes or fails.
    let op = admission_op_id(request.stack);
    let (runtime, boot) = owner_tokens(op);
    let intent = layerfs_history::records::GcIntent::shared(
        op,
        layerfs_history::records::GcIntentKind::Init,
        layerfs_history::records::GcScopeKind::Project,
        request.stack.as_slice(),
    )
    .named(Some(request.name.as_str()), None)
    .with_phase(Some("init"))
    .owned(Some(&runtime), Some(&boot));
    let admitted = match catalog.gc_register(&intent) {
        Ok(()) => true,
        // A catalog without admission cannot exclude a removal; Init still
        // proceeds, as it did before #41 T6.
        Err(layerfs_history::HistoryError::Unsupported(_)) => false,
        Err(error) => {
            return Err(ProjectError::History(
                error.with_unknown(init_unknown_state(&request, op)),
            ))
        }
    };
    let mut admission = InitAdmission {
        catalog,
        op_id: op,
        admitted,
        released: false,
    };
    counters.step(InitStep::Scan);
    let mut progress = ImportProgress::observed(request.deadline, counters);
    progress.tick()?;
    let source = scan::check_root(request.source)?;
    if let Some(placement) = request.acquisition.placement() {
        super::source::outside_source(&source, &placement)?;
    }
    let scope = scope_for_seed(request.scope_seed);
    let mut stack = [0; 16];
    stack.copy_from_slice(&request.stack.to_bytes()[1..]);
    let begun = request.acquisition.begin(&Begin {
        source_device: source.dev(),
        source_inode: source.ino(),
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
    // The release below runs after `request.name` moves into the Stack; name
    // the project and operation now (F43R4-2).
    let unknown = init_unknown_state(&request, op);
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
    admission
        .release()
        .map_err(|error| ProjectError::History(error.with_unknown(unknown)))?;
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
#[cfg(unix)]
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

/// Refuses host Init where the required Unix metadata contract is unavailable.
#[cfg(not(unix))]
pub fn init_observed(
    _storage: &Storage,
    _catalog: &dyn HistoryCatalog,
    _request: InitRequest<'_>,
    _timer: &TimingScope<'_, Active>,
    _counters: Arc<InitCounters>,
) -> ProjectResult<Initialized> {
    Err(ProjectError::Unsupported)
}

/// This process's owner evidence for one Init: its pid/start and PID
/// namespace, and this boot. The boot token carries the operation so two
/// shared intents of one project from one process stay distinct.
fn owner_tokens(op: [u8; 16]) -> (String, String) {
    let pid = std::process::id();
    let mut suffix = String::with_capacity(op.len() * 2);
    for byte in op {
        let _ = write!(suffix, "{byte:02x}");
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let start = layerfs_bridge::process::stat(pid)
            .map(|stat| stat.start)
            .unwrap_or(0);
        let boot = layerfs_bridge::process::boot().unwrap_or(0);
        let namespace = {
            #[cfg(target_os = "linux")]
            {
                std::fs::read_link("/proc/self/ns/pid")
                    .ok()
                    .and_then(|target| target.to_str().map(str::to_owned))
                    .and_then(|text| {
                        text.strip_prefix("pid:[")
                            .and_then(|body| body.strip_suffix(']'))
                            .and_then(|body| body.parse::<u64>().ok())
                    })
                    .map(|inode| format!("@ns:{inode}"))
                    .unwrap_or_default()
            }
            #[cfg(not(target_os = "linux"))]
            {
                String::new()
            }
        };
        (
            format!("pid:{pid}@{start}{namespace}"),
            format!("boot:{boot:032x}:op:{suffix}"),
        )
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        (format!("pid:{pid}"), format!("boot:unknown:op:{suffix}"))
    }
}

/// One fresh 16-byte Init admission identity.
fn admission_op_id(stack: LayerStackId) -> [u8; 16] {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let mut seed = Vec::with_capacity(56);
    seed.extend_from_slice(stack.as_slice());
    seed.extend_from_slice(&std::process::id().to_le_bytes());
    seed.extend_from_slice(&now.to_le_bytes());
    seed.extend_from_slice(&NEXT.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    let identity = ObjectId::for_bytes(&seed);
    let mut op = [0u8; 16];
    op.copy_from_slice(&identity.as_bytes()[..16]);
    op
}

/// The A6 context of an Init admission answer that was lost: the project
/// being initialized, the Init phase and its durable operation.
#[cfg(unix)]
fn init_unknown_state(request: &InitRequest<'_>, op: [u8; 16]) -> UnknownOutcomeState {
    UnknownOutcomeState::new(
        GcScopeKind::Project,
        request.name.as_str(),
        Some("init".to_owned()),
        Some(layerfs_bridge::sdk_values::hex(&op)),
    )
}

/// Releases one Init's admission intent on success, and on drop after a
/// failure (best effort: the original failure is the reported one).
struct InitAdmission<'a> {
    catalog: &'a dyn HistoryCatalog,
    op_id: [u8; 16],
    admitted: bool,
    released: bool,
}

impl InitAdmission<'_> {
    fn release(&mut self) -> Result<(), layerfs_history::HistoryError> {
        self.released = true;
        if !self.admitted {
            return Ok(());
        }
        match self.catalog.gc_release(&self.op_id) {
            Ok(()) | Err(layerfs_history::HistoryError::Unsupported(_)) => Ok(()),
            Err(error) => {
                // The owner cannot settle its row; mark it uncertain so a
                // later call reports `Unresolved` and its owner can adopt it.
                let _ = self.catalog.gc_mark_uncertain(&self.op_id);
                Err(error)
            }
        }
    }
}

impl Drop for InitAdmission<'_> {
    fn drop(&mut self) {
        if !self.released && self.admitted && self.catalog.gc_release(&self.op_id).is_err() {
            let _ = self.catalog.gc_mark_uncertain(&self.op_id);
        }
    }
}
