//! Stores that hold many projects (#506): a new empty Store, a project
//! added to a live Store, and the projects a Store holds.
//!
//! A project is a named LayerStack. Adding one runs the same namespace Init
//! as a new Store, over a writable session on the live Store: its Saves and
//! history writes go through that Store's admission and writer lane, one
//! short job at a time, while daemons and SDK processes keep using it. It
//! never seals or replaces the Store. Init statistics are recorded once per
//! Store, by its first Init, so an added project records none.
use crate::store::InitError;
use crate::{InitCounters, InitStep, Initialized, ProjectError};
use layerfs_history::{
    BranchId, BranchSnapshot, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig,
    HistoryName, HistoryResult, LayerStackId, LayerStackRecord, Page,
};
use layerfs_persistence::{
    Handles, PersistenceConfig, SealedStore, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use layerfs_storage::port::{acquisition::AcquisitionError, PersistenceError};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{path::PathBuf, sync::Arc, time::Instant};

/// A new Store with no project yet, created with the acquisition tables so
/// projects can be added to it later.
#[derive(Debug)]
pub struct CreateRequest {
    /// New Store file, explicit Disposable profile; must not exist.
    pub store: PersistenceConfig,
    /// Frozen content/storage policy every project of the Store shares.
    pub policy: StoragePolicy,
    /// The Store's catalog authority and cursor key.
    pub catalog: HistoryCatalogConfig,
}

/// One project added to a live Store.
#[derive(Debug)]
pub struct AddRequest {
    /// Complete source directory, with no Git-ignore filtering.
    pub source: PathBuf,
    /// The live Store file, explicit Disposable profile.
    pub store: PersistenceConfig,
    /// The Store's binding key.
    pub binding: Vec<u8>,
    /// The Store's cursor key in this home.
    pub cursor_key: [u8; 32],
    /// New LayerStack identity.
    pub stack: LayerStackId,
    /// The project's name, not yet taken in the Store.
    pub stack_name: HistoryName,
    /// New first Branch identity.
    pub branch: BranchId,
    /// Checked first Branch name.
    pub branch_name: HistoryName,
    /// Filesystem inode allocation scope seed.
    pub scope_seed: [u8; 32],
    /// Explicit Init deadline.
    pub deadline: Instant,
}

/// An acknowledged added project. The Store stays open elsewhere and closed here.
#[derive(Debug)]
pub struct AddedProject {
    /// Original acknowledged import and its count diagnostics.
    pub initialized: Initialized,
    /// Original acknowledged first Branch.
    pub branch: BranchSnapshot,
    /// The SQLite version of this session.
    pub sqlite_version: String,
}

/// Original failure of an add, without cleanup, retry or an inferred result.
#[derive(Debug)]
pub struct AddFailure {
    /// Exact inputs, to identify retained working state.
    pub request: AddRequest,
    /// Deciding failure in its original typed carrier.
    pub error: InitError,
    /// Original acknowledged Init, if it completed before the failure.
    pub initialized: Option<Initialized>,
}

impl std::fmt::Display for AddFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "add project: {:?}", self.error)
    }
}
impl std::error::Error for AddFailure {}

fn disposable(store: &PersistenceConfig) -> Result<(), InitError> {
    if store.sqlite_profile != SqlitePersistenceProfile::Disposable {
        return Err(InitError::Manifest(
            "explicit Disposable Store profile required",
        ));
    }
    Ok(())
}

/// Creates and seals one Store holding no project.
pub(crate) fn create(request: CreateRequest) -> Result<SealedStore, InitError> {
    disposable(&request.store)?;
    let config = request
        .store
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables);
    let handles = Handles::create(config, request.policy, &request.catalog)
        .map_err(InitError::Persistence)?;
    handles.seal().map_err(InitError::Persistence)
}

/// Imports `request.source` as a new project of the live Store.
pub(crate) fn add(
    request: AddRequest,
    timer: &TimingScope<'_, Active>,
    counters: Arc<InitCounters>,
) -> Result<AddedProject, Box<AddFailure>> {
    let mut initialized = None;
    let result = (|| {
        disposable(&request.store)?;
        let handles =
            Handles::open_writable(request.store.clone(), &request.binding, request.cursor_key)
                .map_err(InitError::Persistence)?;
        let taken = handles
            .history
            .layer_stack_named(&request.stack_name)
            .map_err(InitError::History)?;
        if taken.is_some() {
            return Err(InitError::Exists);
        }
        let storage = Storage::new(handles.storage.clone()).map_err(InitError::Storage)?;
        let imported = crate::init_observed(
            &storage,
            &handles.history,
            crate::InitRequest {
                source: &request.source,
                acquisition: &handles.acquisition,
                stack: request.stack,
                name: request.stack_name.clone(),
                scope_seed: request.scope_seed,
                deadline: request.deadline,
            },
            timer,
            Arc::clone(&counters),
        )
        .map_err(refused_acquisition)?;
        counters.step(InitStep::Seal);
        let source = ForkSource::Layer(imported.stack.head_layer);
        initialized = Some(imported);
        let branch = handles
            .history
            .fork(&ForkRequest {
                stack: request.stack,
                branch: request.branch,
                name: request.branch_name.clone(),
                source,
            })
            .map_err(InitError::History)?;
        let sqlite_version = handles.profile().sqlite_version.clone();
        drop(storage);
        handles.close();
        Ok((branch, sqlite_version))
    })();
    match result {
        Ok((branch, sqlite_version)) => Ok(AddedProject {
            initialized: initialized.expect("acknowledged Init"),
            branch,
            sqlite_version,
        }),
        Err(error) => Err(Box::new(AddFailure {
            request,
            error,
            initialized,
        })),
    }
}

/// A Store without the acquisition tables refuses the first acquisition
/// unit, before anything is scanned or written: that is [`InitError::NoAcquisition`].
/// Another add still working in the same Store holds its rows: [`InitError::Busy`].
fn refused_acquisition(error: ProjectError) -> InitError {
    match &error {
        ProjectError::Acquisition(AcquisitionError::Persistence(
            PersistenceError::BackendUnavailable,
        )) => InitError::NoAcquisition,
        ProjectError::Acquisition(AcquisitionError::Persistence(PersistenceError::Refused {
            status,
        })) if status == "acquisition working rows remain" => InitError::Busy,
        _ => InitError::Project(error),
    }
}

/// Every project of the Store `catalog`, in name order.
pub fn projects(catalog: &dyn HistoryCatalog) -> HistoryResult<Vec<LayerStackRecord>> {
    let mut out = Vec::new();
    let mut page = Page {
        cursor: None,
        limit: layerfs_history::MAXIMUM_PAGE_RECORDS,
    };
    loop {
        let result = catalog.layer_stacks(&page)?;
        out.extend(result.records);
        match result.continuation {
            Some(cursor) => page.cursor = Some(cursor),
            None => return Ok(out),
        }
    }
}
