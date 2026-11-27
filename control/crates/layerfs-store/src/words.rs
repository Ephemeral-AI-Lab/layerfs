//! Engine failures in plain words, for the message of a public refusal.
//! The engine's own `Display` of some of them prints Rust `Debug` text
//! (`PersistenceError`, `InitFailure`, `ProjectError`, `AcquisitionError`,
//! `AddFailure`, `InstallFailure`), which no answer of the call surface
//! carries; these say the same in words, keeping every inner cause that has
//! words of its own.
use layerfs_history::HistoryError;
use layerfs_project::store::InitError;
use layerfs_project::ProjectError;
use layerfs_storage::port::acquisition::AcquisitionError;

/// A Store creation or project add that failed.
pub fn init_error(error: &InitError) -> String {
    match error {
        InitError::Manifest(what) => format!("the Store's setup was refused: {what}"),
        InitError::Persistence(error) => persistence(error),
        InitError::Storage(error) => error.to_string(),
        InitError::Project(error) => project_error(error),
        InitError::History(error) => history(error),
        InitError::NameTaken(_) => "the Store already holds a project of that name".to_owned(),
        InitError::NoAcquisition => {
            "the Store was made without the tables a project is added through".to_owned()
        }
        InitError::Busy => "another project is being added to the Store".to_owned(),
    }
}

/// A folder's import into a Store that failed.
pub fn project_error(error: &ProjectError) -> String {
    match error {
        ProjectError::InvalidInput { reason, path } => format!("{}: {reason}", path.display()),
        ProjectError::Unsupported => "this platform cannot import the folder".to_owned(),
        ProjectError::BackingInsideSource => {
            "the import's working files would lie inside the folder it imports".to_owned()
        }
        ProjectError::Capacity => "the import ran out of room".to_owned(),
        ProjectError::Deadline => "the import ran out of time".to_owned(),
        ProjectError::WorkerUnavailable => "the import's worker did not start".to_owned(),
        ProjectError::Content(error) => error.to_string(),
        ProjectError::Storage(error) => error.to_string(),
        ProjectError::History(error) => history(error),
        ProjectError::Acquisition(error) => acquisition(error),
        ProjectError::Io(error) => error.to_string(),
        ProjectError::Cleanup { cause, error, .. } => match cause {
            Some(cause) => format!(
                "{}; its working rows were then not cleared: {}",
                project_error(cause),
                acquisition(error)
            ),
            None => format!("its working rows were not cleared: {}", acquisition(error)),
        },
        ProjectError::Uncertain { cause, .. } => {
            format!("{}; its outcome is not known", project_error(cause))
        }
    }
}

/// A history operation that failed: a moved head in words (its engine
/// `Display` prints the moved state as `Debug`), every other one as the
/// history crate says it.
pub fn history(error: &HistoryError) -> String {
    match error {
        HistoryError::HeadMoved(_) => "the Branch's head moved since it was read".to_owned(),
        other => other.to_string(),
    }
}

/// The import's working tables in the Store that failed.
pub fn acquisition(error: &AcquisitionError) -> String {
    match error {
        AcquisitionError::Persistence(error) => persistence(error),
        AcquisitionError::Stale => "the import's working rows belong to no live import".to_owned(),
        AcquisitionError::Bounds => "an import window was too large or malformed".to_owned(),
        AcquisitionError::Changed { .. } => {
            "a file changed while it was imported; nothing was changed".to_owned()
        }
    }
}

// Store database failures in plain words, for a refusal's message: the
// engine's own `Display` of a `PersistenceError` prints Rust `Debug` text.
// It lives here, below the Store crate, so a backup's own failures use it
// too; the Store crate's `words` module re-exports it.
use layerfs_storage::port::PersistenceError;

/// A Store database operation that failed: each outcome in words, a
/// refusal with the backend's own status text.
pub fn persistence(error: &PersistenceError) -> String {
    match error {
        PersistenceError::Busy => "the Store is held by another writer; nothing was changed".into(),
        PersistenceError::BackendUnavailable => {
            "the Store's database is not available to this build".into()
        }
        PersistenceError::Missing => "the Store holds no record of what was asked".into(),
        PersistenceError::Refused { status } => format!("the Store refused it: {status}"),
        PersistenceError::Malformed => "the Store answered something malformed".into(),
        PersistenceError::Uncertain => {
            "the Store's answer was lost; whether it changed is not known".into()
        }
    }
}
