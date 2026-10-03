//! Explicit backend and host-local database selection.
use std::path::PathBuf;
/// Explicit persistence engine; no error-driven fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendSelection {
    /// Embedded host-local SQLite.
    Sqlite,
    /// Unavailable placeholder.
    Postgres,
}
/// One complete Store database path and engine selection.
#[derive(Clone, Debug)]
pub struct PersistenceConfig {
    /// Backend requested by the application.
    pub backend: BackendSelection,
    /// Host-local SQLite file; never a network endpoint.
    pub path: PathBuf,
}
impl PersistenceConfig {
    /// Selects the embedded durable profile at an explicit path.
    pub fn sqlite(path: impl Into<PathBuf>) -> Self {
        Self {
            backend: BackendSelection::Sqlite,
            path: path.into(),
        }
    }
}
