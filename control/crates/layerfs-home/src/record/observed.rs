//! When a thing's state was last observed (call surface 2.0, review fix 1):
//! `observed.json` beside its record, written by a live read (`machine get
//! --with live`) each time its probe answers. It is not a record: writing
//! it never moves the home's [`generation`](super::generation), so a client
//! that polls a live read is not told to read every record again. A
//! records-only read answers it as `observed_at`.
use super::file::{read_record, with_field, write_record};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// The observation file of the thing whose record is at `record`:
/// `observed.json` in the record's folder.
pub fn file(record: &Path) -> PathBuf {
    record.with_file_name("observed.json")
}

/// When the thing whose record is at `record` was last observed, in
/// milliseconds since 1970; none before its first live read, and none when
/// the file cannot be read (an observation is never a refusal).
pub fn read(record: &Path) -> Option<i64> {
    read_record(&file(record)).ok()??.get_int("at")
}

/// Notes that the thing whose record is at `record` was observed at `ms`,
/// keeping what else the file holds (a PostgreSQL Store's reach
/// observations, spec v23). The generation does not move.
pub fn write(record: &Path, ms: i64) -> Result<(), SdkRefusal> {
    merge(record, "at", SdkValue::Int(ms))
}

/// The whole observation file of the thing whose record is at `record`;
/// an empty map when there is none or it cannot be read.
pub fn fields(record: &Path) -> SdkValue {
    match read_record(&file(record)) {
        Ok(Some(value @ SdkValue::Map(_))) => value,
        _ => SdkValue::Map(Vec::new()),
    }
}

/// Sets `key` of the observation file of the thing whose record is at
/// `record` to `value`, keeping its other keys. Writers of one file that
/// may race take that thing's lock first.
pub fn merge(record: &Path, key: &str, value: SdkValue) -> Result<(), SdkRefusal> {
    let fields = with_field(fields(record), key, value);
    write_record(&file(record), &fields)
}

/// When LayerFS last knew the state of the thing whose record is at
/// `record`: its last observation, else when its record was last written;
/// what an `unknown` state answers as `observed_at`.
pub fn last_known(record: &Path) -> Option<i64> {
    read(record).or_else(|| {
        let written = std::fs::metadata(record).ok()?.modified().ok()?;
        let since = written.duration_since(UNIX_EPOCH).ok()?;
        i64::try_from(since.as_millis()).ok()
    })
}
