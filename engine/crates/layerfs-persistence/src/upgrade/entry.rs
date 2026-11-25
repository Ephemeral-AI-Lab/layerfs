//! The upgrade's entry points (skeleton; W1 implements).
use super::types::{Format, FormatEvidence, UpgradeError, UpgradeReport, UpgradeRequest};
use std::path::Path;

/// The SQLite Store's format at `path`, from its header on a raw read-only
/// connection: starts nothing, and works on a Store this build refuses.
pub fn sqlite_format(path: &Path) -> Result<Format, UpgradeError> {
    sqlite_format_evidence(path).map(|evidence| evidence.format)
}

/// [`sqlite_format`] with the doctor's evidence (raw `user_version`,
/// recorded-source variant).
pub fn sqlite_format_evidence(path: &Path) -> Result<FormatEvidence, UpgradeError> {
    let _ = path;
    Err(UpgradeError::Failed("not implemented".to_owned()))
}

/// The PostgreSQL Store's format in `schema` at `url`: one connect, one `SELECT`.
pub fn postgresql_format(url: &str, schema: &str) -> Result<Format, UpgradeError> {
    postgresql_format_evidence(url, schema).map(|evidence| evidence.format)
}

/// [`postgresql_format`] with the doctor's evidence.
pub fn postgresql_format_evidence(url: &str, schema: &str) -> Result<FormatEvidence, UpgradeError> {
    let _ = (url, schema);
    Err(UpgradeError::Failed("not implemented".to_owned()))
}

/// Upgrades the closed-to-others SQLite Store at `path` in place, in one
/// transaction, to [`super::SQLITE_LATEST`]; `phase` is called as each of
/// [`super::SQLITE_PHASES`] starts.
pub fn upgrade_sqlite(
    path: &Path,
    request: &UpgradeRequest<'_>,
    phase: &mut dyn FnMut(&'static str),
) -> Result<UpgradeReport, UpgradeError> {
    let _ = (path, request, phase);
    Err(UpgradeError::Failed("not implemented".to_owned()))
}

/// Upgrades the PostgreSQL Store in `schema` at `url` in place: connect and
/// two messages (§5.2); `phase` is called as each of
/// [`super::POSTGRESQL_PHASES`] starts.
pub fn upgrade_postgresql(
    url: &str,
    schema: &str,
    request: &UpgradeRequest<'_>,
    phase: &mut dyn FnMut(&'static str),
) -> Result<UpgradeReport, UpgradeError> {
    let _ = (url, schema, request, phase);
    Err(UpgradeError::Failed("not implemented".to_owned()))
}
