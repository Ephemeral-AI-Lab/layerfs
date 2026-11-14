// The Windows stub of the call log (#659): the log is off, append is a
// no-op, and the reads answer Unsupported - the per-call file semantics
// (O_APPEND rotation under flock) are the port gap, disclosed. The
// calls themselves are unaffected; only the log face refuses.
use crate::outcome::{refused, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

/// Which lines `logs.list` answers; every field given must match.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// `family` or `family.call`.
    pub op: Option<String>,
    /// A project the line's `argv` names.
    pub project: Option<String>,
    /// `cli`, `node`, `python` or `rust`.
    pub client: Option<String>,
    /// An exact tag, or a prefix ending in `:`.
    pub tag: Option<String>,
    /// `ok` or a refusal code.
    pub outcome: Option<String>,
    /// Lines whose `at` is at or after this time (ms).
    pub since: Option<i64>,
}

/// Appends one line: a no-op (the log is off on Windows).
pub fn append(_home: &Path, _line: &SdkValue) {}

/// Clears the call log: Unsupported until the port.
pub fn reset(_home: &Path) -> Result<u64, SdkRefusal> {
    Err(refused(
        "Unsupported",
        "the call log is not ported to Windows yet (#659)",
    ))
}

/// Reads the call log: Unsupported until the port.
pub fn read(
    _home: &Path,
    _filter: &Filter,
    _cursor: Option<&str>,
    _limit: Option<i64>,
) -> Outcome {
    Err(refused(
        "Unsupported",
        "the call log is not ported to Windows yet (#659)",
    ))
}
