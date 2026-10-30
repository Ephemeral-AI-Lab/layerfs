//! Explicit backend and local database selection.
use std::path::PathBuf;
/// Explicit persistence engine; no error-driven fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendSelection {
    /// Embedded local SQLite.
    Sqlite,
    /// A PostgreSQL Store (#506), opened through `PostgresHandles` when the
    /// `postgresql` feature is built. Init stays SQLite only: `Handles`
    /// refuses this selection with `BackendUnavailable`.
    Postgres(PostgresSelection),
}
/// PostgreSQL `synchronous_commit` for every write transaction, applied with
/// `SET LOCAL`. `Off` is the default (owner, 2026-10-29).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SynchronousCommit {
    /// Acknowledge before the WAL record is flushed.
    #[default]
    Off,
    /// Wait for the local WAL flush only.
    Local,
    /// Wait for standbys to receive the WAL.
    RemoteWrite,
    /// Wait for the local and synchronous-standby flush.
    On,
    /// Wait for synchronous standbys to apply the WAL.
    RemoteApply,
}
impl SynchronousCommit {
    #[cfg_attr(not(feature = "postgresql"), allow(dead_code))]
    pub(crate) const fn setting(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Local => "local",
            Self::RemoteWrite => "remote_write",
            Self::On => "on",
            Self::RemoteApply => "remote_apply",
        }
    }
}
/// One PostgreSQL Store: server, schema and write settings.
#[derive(Clone, Eq, PartialEq)]
pub struct PostgresSelection {
    /// libpq-style connection string or URL. It may hold a password, so it
    /// is never printed by `Debug`.
    pub url: String,
    /// Schema holding the Store's tables: `[a-z_][a-z0-9_]*`, at most 63 bytes.
    pub schema: String,
    /// `synchronous_commit` of every write transaction.
    pub synchronous_commit: SynchronousCommit,
    /// `lock_timeout` while a write waits for the publication lane, in
    /// milliseconds; a wait that reaches it fails `Busy`, never retried.
    pub lock_timeout_ms: u32,
}
impl PostgresSelection {
    /// A selection with `synchronous_commit=off` and a 30 s lane wait.
    pub fn new(url: impl Into<String>, schema: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            schema: schema.into(),
            synchronous_commit: SynchronousCommit::Off,
            lock_timeout_ms: 30_000,
        }
    }
    /// Selects `synchronous_commit` for every write transaction.
    pub fn with_synchronous_commit(mut self, setting: SynchronousCommit) -> Self {
        self.synchronous_commit = setting;
        self
    }
}
impl std::fmt::Debug for PostgresSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresSelection")
            .field("url", &"<redacted>")
            .field("schema", &self.schema)
            .field("synchronous_commit", &self.synchronous_commit)
            .field("lock_timeout_ms", &self.lock_timeout_ms)
            .finish()
    }
}
/// Explicit SQLite completion and durability contract, selected before opening.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SqlitePersistenceProfile {
    /// WAL/FULL; macOS additionally requests full synchronization.
    #[default]
    Durable,
    /// WAL/OFF; survives process termination, without kernel-crash durability.
    /// A kernel or VM crash can corrupt the Store or lose acknowledged data.
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
            Self::Monolithic => 16,
            Self::GroupRows => 17,
            Self::GroupRowsIndexed => 18,
        }
    }
}
/// Explicit creation-only selection of the initial-acquisition working tables.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SqliteAcquisitionSchema {
    /// Schema versions 16 to 18: no acquisition tables; acquisition is unavailable.
    #[default]
    Absent,
    /// Schema versions 22 to 24: the same pack layouts with the acquisition tables.
    Tables,
}
impl SqliteAcquisitionSchema {
    pub(crate) const fn version_offset(self) -> i64 {
        match self {
            Self::Absent => 0,
            Self::Tables => 6,
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
