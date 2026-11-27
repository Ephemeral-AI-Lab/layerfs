//! Explicit backend and local database selection.
use std::path::PathBuf;
/// PostgreSQL `synchronous_commit` for every write transaction, applied with
/// `SET LOCAL`. `Local` is the default (owner, 2026-11-17; it was `Off`
/// from 2026-10-29): a server crash loses no acknowledged write.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SynchronousCommit {
    /// Acknowledge before the WAL record is flushed. A server crash can lose
    /// acknowledged writes; an explicit choice only.
    Off,
    /// Wait for the local WAL flush only.
    #[default]
    Local,
    /// Wait for standbys to receive the WAL.
    RemoteWrite,
    /// Wait for the local and synchronous-standby flush.
    On,
    /// Wait for synchronous standbys to apply the WAL.
    RemoteApply,
}
impl SynchronousCommit {
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
/// How much of a PostgreSQL schema an open checks before serving (#63).
/// Both run in one round trip and refuse a wrong binding, catalog,
/// incarnation, identity format or schema version, or a policy row that
/// does not validate, with `Integrity`/`Malformed`, exactly as before.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PostgresCheck {
    /// The whole contract: the exact table set and the live column
    /// definition against the recorded source and definition, read from
    /// `information_schema` in one REPEATABLE READ snapshot. Init, Store
    /// connect and the host's Store service open with it.
    #[default]
    Full,
    /// The schema-version and incarnation check of a session opened after
    /// the schema was fully checked once (a Sandbox daemon's writer, every
    /// Workspace and control reader, the host's Sandbox reach check): the
    /// `history_meta` and policy rows, the recorded source's length and the
    /// acquisition tables' presence by name. It does not scan
    /// `information_schema`, so a column changed by hand after Store
    /// connect is found by the statement that uses it, not at open.
    Light,
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
    /// What an open checks; see [`PostgresCheck`].
    pub check: PostgresCheck,
    /// How long a Workspace reader's connection may have been silent (no
    /// server reply) before a mount that takes it first checks it with one
    /// round trip, in milliseconds ([`READER_IDLE_CHECK_MS`] by default; see
    /// `PostgresStorage::usable`).
    pub reader_idle_check_ms: u32,
}
/// The default silence after which a pooled Workspace reader is checked
/// with an empty query (answer within [`READER_CHECK_TIMEOUT`]) before a
/// mount takes it: 30 s since the server last answered on its connection.
/// A reader whose server answered more recently is checked locally only,
/// with no round trip, so back-to-back per-tool-call mounts send nothing;
/// one silent longer, however short its time among the idle readers, meets
/// the network paths that end an idle connection without telling the
/// client (a load balancer or NAT that resets it at the next packet, a
/// cloud path that drops it silently) while it can still be swapped for a
/// fresh reader.
pub const READER_IDLE_CHECK_MS: u32 = 30_000;
/// How long the idle reader's empty query may take before the reader is
/// dropped as unusable: 5 s.
pub const READER_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
impl PostgresSelection {
    /// A selection with `synchronous_commit=local` and a 30 s lane wait.
    pub fn new(url: impl Into<String>, schema: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            schema: schema.into(),
            synchronous_commit: SynchronousCommit::Local,
            lock_timeout_ms: 30_000,
            check: PostgresCheck::Full,
            reader_idle_check_ms: READER_IDLE_CHECK_MS,
        }
    }
    /// Selects what an open checks.
    pub fn with_check(mut self, check: PostgresCheck) -> Self {
        self.check = check;
        self
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
            .field("check", &self.check)
            .field("reader_idle_check_ms", &self.reader_idle_check_ms)
            .finish()
    }
}
/// Explicit SQLite completion and durability contract, selected before opening.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SqlitePersistenceProfile {
    /// WAL/FULL; macOS additionally requests full synchronization.
    #[default]
    Durable,
    /// WAL/NORMAL: the log is synchronized before each checkpoint and after a
    /// log restart, not at every COMMIT. A kernel or VM crash can lose the
    /// commits since the last synchronized checkpoint. The only profile a
    /// Store is opened with (owner, 2026-11-17; OFF was removed 2026-11-23).
    Normal,
}
impl SqlitePersistenceProfile {
    /// Whether a Store may be opened with this profile: Normal only.
    /// Durable stays disabled.
    pub const fn permitted(self) -> bool {
        matches!(self, Self::Normal)
    }
}
/// Explicit physical layout for a newly created embedded Store.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SqlitePackLayout {
    /// Complete pack BLOBs (schema version 56, or 62 with acquisition tables).
    #[default]
    Monolithic,
    /// Complete encoded groups in independent immutable rows (57 or 63).
    GroupRows,
    /// Group rows with a covering index for bounded mapping validation (58 or 64).
    GroupRowsIndexed,
}
impl SqlitePackLayout {
    pub(crate) const fn uses_units(self) -> bool {
        matches!(self, Self::GroupRows | Self::GroupRowsIndexed)
    }
    /// The stored schema version of this layout without acquisition
    /// tables. Versions 56 to 58 (62 to 64 with them) are schema version 3
    /// (#776 R1): schema version 2's inode value and binding hint blocks
    /// (#91) with each Commit's attempt. Schema version 2 (36 to 38, 42 to
    /// 44) and schema version 1 (16 to 18, 22 to 24) are refused at open.
    pub(crate) const fn version(self) -> i64 {
        match self {
            Self::Monolithic => 56,
            Self::GroupRows => 57,
            Self::GroupRowsIndexed => 58,
        }
    }
}
/// Explicit creation-only selection of the initial-acquisition working tables.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SqliteAcquisitionSchema {
    /// Schema versions 56 to 58: no acquisition tables; acquisition is unavailable.
    #[default]
    Absent,
    /// Schema versions 62 to 64: the same pack layouts with the acquisition tables.
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
