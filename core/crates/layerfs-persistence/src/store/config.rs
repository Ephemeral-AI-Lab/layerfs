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
/// Explicit physical layout for a newly created embedded Store.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SqlitePackLayout {
    /// Original schema1 complete pack BLOBs.
    #[default]
    Monolithic,
    /// Schema2 complete encoded groups in independent immutable rows.
    GroupRows,
    /// Schema3 group rows with a covering index for bounded mapping validation.
    GroupRowsIndexed,
}
impl SqlitePackLayout {
    pub(crate) const fn uses_units(self) -> bool {
        matches!(self, Self::GroupRows | Self::GroupRowsIndexed)
    }
    pub(crate) const fn version(self) -> i64 {
        match self {
            Self::Monolithic => 1,
            Self::GroupRows => 2,
            Self::GroupRowsIndexed => 3,
        }
    }
}
/// Explicit creation-only selection of the initial-acquisition working tables.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SqliteAcquisitionSchema {
    /// Schema versions 1 to 3: no acquisition tables; acquisition is unavailable.
    #[default]
    Absent,
    /// Schema versions 4 to 6: the same pack layouts with the acquisition tables.
    Tables,
}
impl SqliteAcquisitionSchema {
    pub(crate) const fn version_offset(self) -> i64 {
        match self {
            Self::Absent => 0,
            Self::Tables => 3,
        }
    }
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
    /// Creation-only layout; opens select the declared supported schema version.
    pub sqlite_pack_layout: SqlitePackLayout,
    /// Creation-only acquisition tables; opens follow the stored schema version.
    pub sqlite_acquisition: SqliteAcquisitionSchema,
}
impl PersistenceConfig {
    /// Selects the embedded durable profile at an explicit path.
    pub fn sqlite(path: impl Into<PathBuf>) -> Self {
        Self {
            backend: BackendSelection::Sqlite,
            path: path.into(),
            sqlite_profile: SqlitePersistenceProfile::Durable,
            sqlite_pack_layout: SqlitePackLayout::Monolithic,
            sqlite_acquisition: SqliteAcquisitionSchema::Absent,
        }
    }
    /// Selects the acquisition tables explicitly for creation; never added to an open Store.
    pub fn with_sqlite_acquisition(mut self, acquisition: SqliteAcquisitionSchema) -> Self {
        self.sqlite_acquisition = acquisition;
        self
    }
    /// Selects physical group rows explicitly for creation; never migrates an open Store.
    pub fn with_sqlite_pack_layout(mut self, layout: SqlitePackLayout) -> Self {
        self.sqlite_pack_layout = layout;
        self
    }
    /// Selects a profile before creation/open; incompatible Stores are refused.
    pub fn with_sqlite_profile(mut self, profile: SqlitePersistenceProfile) -> Self {
        self.sqlite_profile = profile;
        self
    }
}
