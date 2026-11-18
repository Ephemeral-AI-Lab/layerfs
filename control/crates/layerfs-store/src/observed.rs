//! When a Store's state was last observed (call surface 2.0, review fix 1):
//! `observed.json` beside its record, written by a live read (`store get
//! --with live`) each time the Store answers. It is not a record: writing
//! it never moves the home's generation, so a client that polls a live
//! read is not told to read every record again. A records-only read
//! answers it as `observed_at`.
//!
//! **The seam with package F-machine.** That package adds the same helper
//! for every thing's record as `layerfs_home::home::observed` (same file
//! name beside the record, same `{at}` shape). [`read`], [`write`] and
//! [`last_known`] are the only functions the Store code calls; they switch
//! to that module once it lands, and nothing else here changes then.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{paths, read_record, write_record};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Store `store`'s observation file: `observed.json` beside its record.
fn file(home: &Path, store: &str) -> PathBuf {
    paths::store_record(home, store).with_file_name("observed.json")
}

/// When Store `store` was last observed, in milliseconds since 1970; none
/// before its first live read, and none when the file cannot be read (an
/// observation is never a refusal).
pub fn read(home: &Path, store: &str) -> Option<i64> {
    read_record(&file(home, store)).ok()??.get_int("at")
}

/// Notes that Store `store` was observed at `ms`. The generation does not
/// move.
pub fn write(home: &Path, store: &str, ms: i64) -> Result<(), SdkRefusal> {
    write_record(
        &file(home, store),
        &SdkValue::map([("at", SdkValue::Int(ms))]),
    )
}

/// When LayerFS last knew Store `store`'s state: its last observation,
/// else when its record was last written; what an `unknown` state answers
/// as `observed_at`.
pub fn last_known(home: &Path, store: &str) -> Option<i64> {
    read(home, store).or_else(|| {
        let written = std::fs::metadata(paths::store_record(home, store)).ok()?;
        let since = written.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
        i64::try_from(since.as_millis()).ok()
    })
}
