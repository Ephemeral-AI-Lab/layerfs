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
/// Explicit SQLite completion and durability contract, selected before opening.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SqlitePersistenceProfile {
    /// WAL/FULL with macOS full synchronization; the default profile.
    #[default]
    Durable,
    /// Disk-backed MEMORY/OFF; runtime atomicity without crash durability.
    /// A crash or power loss can corrupt the Store or lose acknowledged data.
    Disposable,
}
/// One complete Store database path and engine selection.
#[derive(Clone, Debug)]
pub struct PersistenceConfig {
    /// Backend requested by the application.
    pub backend: BackendSelection,
    /// Host-local SQLite file; never a network endpoint.
    pub path: PathBuf,
    /// Explicit SQLite profile; never selected by failure or environment.
    pub sqlite_profile: SqlitePersistenceProfile,
}
impl PersistenceConfig {
    /// Selects the embedded durable profile at an explicit path.
    pub fn sqlite(path: impl Into<PathBuf>) -> Self {
        Self {
            backend: BackendSelection::Sqlite,
            path: path.into(),
            sqlite_profile: SqlitePersistenceProfile::Durable,
        }
    }
    /// Selects a profile before creation/open; incompatible Stores are refused.
    pub fn with_sqlite_profile(mut self, profile: SqlitePersistenceProfile) -> Self {
        self.sqlite_profile = profile;
        self
    }
}
