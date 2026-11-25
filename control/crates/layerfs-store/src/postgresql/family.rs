//! The PostgreSQL Store family's exclusive operation (#804 §5.2 step 1):
//! the counterpart of `sqlite::family::begin` and `settle` for a
//! PostgreSQL record. Under the home lock and the Store's lock the record
//! is re-read, the caller's `check` refuses or answers, the Store is found
//! idle and `op` is written, which refuses every later lease; then, past
//! the locks, this process's own open session of the Store is closed and,
//! under the locks once more, the Store is found idle again and this
//! process's lease is taken. The locks are held only for those checks and
//! record writes, never across a round trip.
use crate::lease;
use crate::record::{self, StoreKind, StoreRecord};
use crate::Related;
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::home;
use layerfs_home::op::{self, Op};
use layerfs_home::outcome::refused;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Writes `op` into PostgreSQL Store `name`'s record once `check` lets it
/// and nothing uses the Store; answers `check`'s answer. A sqlite record
/// is `InvalidInput`.
pub fn begin<T>(
    home_dir: &Path,
    name: &str,
    op: &Op,
    related: Related,
    check: impl FnOnce(&StoreRecord) -> Result<T>,
) -> Result<T> {
    let _lock = home::lock::take(home_dir)?;
    let _store = record::lock(home_dir, name)?;
    let record = StoreRecord::of(home_dir, name)?;
    if record.kind()? != StoreKind::Postgresql {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is not a PostgreSQL Store"),
        ));
    }
    let answer = check(&record)?;
    (related.store_idle)(home_dir, name)?;
    record
        .with(op::FIELD, op.to_record().clone())
        .write(home_dir)?;
    Ok(answer)
}

/// After `op` is written: closes this process's own open session of
/// PostgreSQL Store `record` (`InUse` while a call runs on it), then, under
/// the locks, finds the Store idle once more and answers this process's
/// lease, taken there.
pub fn settle(home_dir: &Path, record: &StoreRecord, related: Related) -> Result<lease::Lease> {
    let name = record.name();
    crate::open::close(&record.postgresql_key(home_dir, name))?;
    let _lock = home::lock::take(home_dir)?;
    let _store = record::lock(home_dir, name)?;
    if lease::held_here(home_dir, name) {
        return Err(refused(
            "InUse",
            format!("Store {name} is in use in this process"),
        ));
    }
    (related.store_idle)(home_dir, name)?;
    lease::hold_locked(home_dir, name)
}
