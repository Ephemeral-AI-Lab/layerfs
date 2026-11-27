//! `store upgrade` (#804): the format a Store is written in, read without a
//! full open, and the one-transaction chain of steps that brings an older
//! Store to the latest format in place. Nothing else converts a Store.
pub(crate) mod chain;
pub(crate) mod dispatch;
mod types;

pub(crate) use chain::{run, Step};

pub use dispatch::{
    postgresql_format, postgresql_format_evidence, sqlite_format, sqlite_format_evidence,
    upgrade_postgresql, upgrade_sqlite,
};
pub use types::{
    Format, FormatEvidence, FormatStatus, UpgradeError, UpgradeOutcome, UpgradeReport,
    UpgradeRequest, UpgradeWork, NEWER, POSTGRESQL_LATEST, POSTGRESQL_PHASES, SQLITE_LATEST,
    SQLITE_PHASES,
};
