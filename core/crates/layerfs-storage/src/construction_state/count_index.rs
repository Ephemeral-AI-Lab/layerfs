//! Borrowed-width count/zero records and exact current owner verification.
use super::count_state::{Counts, Snapshot};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{BaseFact, CanonicalScope, CountRecord};
use rusqlite::{types::ValueRef, Connection};
pub(crate) fn blob(value: ValueRef<'_>, size: usize) -> StorageResult<&[u8]> {
    match value {
        ValueRef::Blob(bytes) if bytes.len() == size => Ok(bytes),
        _ => Err(StorageError::Integrity("canonical count BLOB width")),
    }
}
fn unsigned(value: i64) -> StorageResult<u64> {
    u64::try_from(value).map_err(|_| StorageError::Integrity("negative count owner metadata"))
}
fn optional(value: Option<i64>) -> StorageResult<Option<u64>> {
    value.map(unsigned).transpose()
}
pub(crate) fn deferred(connection: &Connection) -> StorageResult<()> {
    let row:i64=connection.query_row("SELECT COUNT(*) FROM count_owner WHERE id=1 AND scope IS NULL AND stage=0 AND records=0 AND touched=0 AND maximum IS NULL AND zeros=0 AND zero_maximum IS NULL AND count_remaining=0 AND zero_remaining=0 AND after_count IS NULL AND after_zero IS NULL AND effects IS NULL AND final_seal IS NULL AND seeds IS NULL",[],|r|r.get(0))?;
    if row != 1 || !empty(connection, "canonical_counts")? || !empty(connection, "zero_seeds")? {
        return Err(StorageError::Integrity("deferred count owner"));
    }
    Ok(())
}
pub(crate) fn verify(connection: &Connection, state: &Counts) -> StorageResult<()> {
    let _scalars = state
        .memory
        .reserve(228 + 2 * 294 + 579 + 12 * std::mem::size_of::<u64>())?;
    let mut statement=connection.prepare("SELECT scope,stage,records,touched,maximum,zeros,zero_maximum,count_remaining,zero_remaining,after_count,after_zero,effects,final_seal,seeds FROM count_owner WHERE id=1")?;
    let mut rows = statement.query([])?;
    let row = rows
        .next()?
        .ok_or(StorageError::Integrity("count owner missing"))?;
    let s = &state.snapshot;
    let scope = state.scope.encode();
    let selected = if s.stage == 0 {
        matches!(row.get_ref(0)?, ValueRef::Null)
    } else {
        blob(row.get_ref(0)?, 228)? == scope
    };
    if !selected
        || unsigned(row.get(1)?)? != u64::from(s.stage)
        || unsigned(row.get(2)?)? != s.records
        || unsigned(row.get(3)?)? != s.touched
        || optional(row.get(4)?)? != s.maximum
        || unsigned(row.get(5)?)? != s.zeros
        || optional(row.get(6)?)? != s.zero_maximum
        || unsigned(row.get(7)?)? != s.count_remaining
        || unsigned(row.get(8)?)? != s.zero_remaining
        || optional(row.get(9)?)? != s.after_count
        || optional(row.get(10)?)? != s.after_zero
    {
        return Err(StorageError::Integrity("count exact owner metadata"));
    }
    let effects = s.effects.as_ref().map(|v| v.encode());
    let final_seal = s.final_seal.as_ref().map(|v| v.encode());
    let seeds = s.seeds.as_ref().map(|v| v.encode());
    for (column, expected) in [
        (11, effects.as_ref().map(|v| v.as_slice())),
        (12, final_seal.as_ref().map(|v| v.as_slice())),
        (13, seeds.as_ref().map(|v| v.as_slice())),
    ] {
        match (row.get_ref(column)?, expected) {
            (ValueRef::Null, None) => {}
            (ValueRef::Blob(actual), Some(expected)) if actual == expected => {}
            _ => return Err(StorageError::Integrity("count exact owner seal")),
        }
    }
    if rows.next()?.is_some() {
        return Err(StorageError::Integrity("count owner multiplicity"));
    }
    Ok(())
}
pub(crate) fn write_owner(
    connection: &Connection,
    scope: &CanonicalScope,
    s: &Snapshot,
) -> StorageResult<()> {
    let effects = s.effects.as_ref().map(|s| s.encode());
    let final_seal = s.final_seal.as_ref().map(|s| s.encode());
    let seeds = s.seeds.as_ref().map(|s| s.encode());
    if connection.execute("UPDATE count_owner SET stage=?1,records=?2,touched=?3,maximum=?4,zeros=?5,zero_maximum=?6,count_remaining=?7,zero_remaining=?8 WHERE id=1",rusqlite::params![s.stage,s.records as i64,s.touched as i64,s.maximum.map(|v|v as i64),s.zeros as i64,s.zero_maximum.map(|v|v as i64),s.count_remaining as i64,s.zero_remaining as i64])?!=1 || connection.execute("UPDATE count_owner SET scope=?1,after_count=?2,after_zero=?3,effects=?4,final_seal=?5,seeds=?6 WHERE id=1",rusqlite::params![scope.encode().as_slice(),s.after_count.map(|v|v as i64),s.after_zero.map(|v|v as i64),effects.as_ref().map(|v|v.as_slice()),final_seal.as_ref().map(|v|v.as_slice()),seeds.as_ref().map(|v|v.as_slice())])?!=1 {return Err(StorageError::Integrity("count fixed owner acknowledgement"));}
    Ok(())
}
pub(crate) fn get(
    connection: &Connection,
    scope: &CanonicalScope,
    serial: u64,
) -> StorageResult<Option<CountRecord>> {
    let key = scope.key(serial)?;
    let mut statement = connection.prepare("SELECT value FROM canonical_counts WHERE key=?1")?;
    let mut rows = statement.query([key.as_slice()])?;
    rows.next()?
        .map(|r| Ok(CountRecord::decode(scope, &key, blob(r.get_ref(0)?, 97)?)?))
        .transpose()
}
pub(crate) fn zero(
    connection: &Connection,
    scope: &CanonicalScope,
    serial: u64,
) -> StorageResult<Option<BaseFact>> {
    let key = scope.key(serial)?;
    let mut statement = connection.prepare("SELECT value FROM zero_seeds WHERE key=?1")?;
    let mut rows = statement.query([key.as_slice()])?;
    rows.next()?
        .map(|r| Ok(BaseFact::decode(serial, blob(r.get_ref(0)?, 74)?)?))
        .transpose()
}
pub(crate) fn empty(connection: &Connection, table: &str) -> StorageResult<bool> {
    Ok(connection.query_row(
        &format!("SELECT NOT EXISTS(SELECT 1 FROM {table} LIMIT 1)"),
        [],
        |r| r.get(0),
    )?)
}
