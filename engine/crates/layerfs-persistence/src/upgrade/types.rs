//! The upgrade's public types.
use std::sync::atomic::AtomicBool;

/// The SQLite format this build writes.
pub const SQLITE_LATEST: i64 = 3;
/// The PostgreSQL format this build writes.
pub const POSTGRESQL_LATEST: i64 = 4;
/// The words every refusal of a Store written by a newer build carries
/// (never "outdated"); front ends tell a newer Store by them.
pub const NEWER: &str = "newer than this build";
/// A SQLite upgrade's phases, in order; one event as each starts.
pub const SQLITE_PHASES: [&str; 4] = ["check", "convert", "verify", "commit"];
/// A PostgreSQL upgrade's phases, in order.
pub const POSTGRESQL_PHASES: [&str; 2] = ["check", "convert"];

/// How a Store's format stands against this build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatStatus {
    /// The latest format.
    Current,
    /// Older, and an upgrade path exists.
    Outdated,
    /// Older with no upgrade path (below the floor, transient, unrecognized).
    Unsupported,
    /// Written by a newer build than this one.
    Newer,
}

impl FormatStatus {
    /// `current`, `outdated`, `unsupported` or `newer`.
    pub fn word(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Outdated => "outdated",
            Self::Unsupported => "unsupported",
            Self::Newer => "newer",
        }
    }
}

/// A Store's format: one integer per engine, with the latest this build
/// writes and the status word.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Format {
    /// The format, `None` when the Store is not one this build recognizes.
    pub version: Option<i64>,
    /// The latest format this build writes for the engine.
    pub latest: i64,
    /// The status word.
    pub status: FormatStatus,
}

/// [`Format`] with the doctor's evidence: the raw encoding and the
/// recorded-source variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatEvidence {
    /// The format.
    pub format: Format,
    /// SQLite `PRAGMA user_version`, or PostgreSQL `history_meta.schema_version`.
    pub raw: i64,
    /// The recorded-source variant a step recognizes (`"d"`, …), if any.
    pub variant: Option<String>,
}

/// What one upgrade call is given.
pub struct UpgradeRequest<'a> {
    /// Set by a cancel (Ctrl-C, SIGTERM, a worker `store.cancel`); read until
    /// `commit` starts, never after.
    pub cancel: &'a AtomicBool,
    /// Test-only stop point (stop after statement `n`, as compaction's
    /// failpoints); production callers leave it `None`.
    pub failpoint: Option<u32>,
}

/// Whether the call changed the Store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpgradeOutcome {
    /// Brought to the latest format.
    Upgraded,
    /// Already at the latest format; nothing changed.
    Current,
}

impl UpgradeOutcome {
    /// `upgraded` or `current`.
    pub fn word(self) -> &'static str {
        match self {
            Self::Upgraded => "upgraded",
            Self::Current => "current",
        }
    }
}

/// The upgrade's own work counters (session counters do not see its raw
/// client).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UpgradeWork {
    /// Connections made (PostgreSQL; 0 on SQLite).
    pub connects: u32,
    /// Client waits for a server reply (PostgreSQL; 0 on SQLite).
    pub round_trips: u32,
    /// Write transactions committed.
    pub write_transactions: u32,
}

/// An upgrade's answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeReport {
    /// Upgraded or current.
    pub outcome: UpgradeOutcome,
    /// `sqlite` or `postgresql`.
    pub engine: &'static str,
    /// The format before.
    pub format_before: i64,
    /// The format after.
    pub format_after: i64,
    /// The steps applied, in order, as `"2->3"`.
    pub steps: Vec<String>,
    /// Things after COMMIT that did not go as planned (§6); never a failure.
    pub warnings: Vec<String>,
    /// The work counters (§9).
    pub work: UpgradeWork,
}

/// Why an upgrade changed nothing, or whether it did is not known.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpgradeError {
    /// Someone else has the Store open (SQLite's exclusive lock refused).
    InUse(String),
    /// A lane, lock or GC record was not granted at once, or the Store
    /// changed between the check and the conversion.
    Busy(String),
    /// Not a format this build upgrades: unrecognized, no step, or newer
    /// (said as such).
    Unsupported(String),
    /// A check before COMMIT failed; the message names it.
    Failed(String),
    /// Cancelled before COMMIT; nothing changed.
    Cancelled,
    /// The outcome is not known (a lost reply, no SQLSTATE).
    Unknown(String),
}
