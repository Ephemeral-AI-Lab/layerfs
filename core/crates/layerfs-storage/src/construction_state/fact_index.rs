//! Borrowed-width checked fact records and exact fixed owner metadata.
use super::fact_state::{Facts, Section};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{BaseFact, FactScope, ParentFact};
use rusqlite::{types::ValueRef, Connection, OptionalExtension};
fn unsigned(value: i64) -> StorageResult<u64> {
    u64::try_from(value).map_err(|_| StorageError::Integrity("fact negative integer"))
}
pub(crate) fn table(code: u8) -> &'static str {
    if code == 17 {
        "base_facts"
    } else {
        "parent_eligibility"
    }
}
pub(crate) fn blob(value: ValueRef<'_>, width: usize) -> StorageResult<&[u8]> {
    match value {
        ValueRef::Blob(bytes) if bytes.len() == width => Ok(bytes),
        _ => Err(StorageError::Integrity("fact BLOB framing")),
    }
}
pub(crate) fn base(
    connection: &Connection,
    scope: &FactScope,
    serial: u64,
) -> StorageResult<Option<BaseFact>> {
    let key = scope.key(serial)?;
    let mut statement = connection.prepare("SELECT value FROM base_facts WHERE key=?1")?;
    let mut rows = statement.query([key.as_slice()])?;
    rows.next()?
        .map(|r| Ok(BaseFact::decode(serial, blob(r.get_ref(0)?, 74)?)?))
        .transpose()
}
pub(crate) fn parent(
    connection: &Connection,
    scope: &FactScope,
    serial: u64,
) -> StorageResult<Option<ParentFact>> {
    let key = scope.key(serial)?;
    let bound: Option<u8> = connection
        .query_row(
            "SELECT bound FROM parent_eligibility WHERE key=?1",
            [key.as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    bound
        .map(|bound| {
            if bound <= 1 {
                Ok(ParentFact {
                    serial,
                    bound: bound == 1,
                })
            } else {
                Err(StorageError::Integrity("parent bound framing"))
            }
        })
        .transpose()
}
pub(crate) fn verify(connection: &Connection, owner: &Facts) -> StorageResult<()> {
    for code in [17, 18] {
        verify_section(connection, code, owner.section(code))?;
    }
    Ok(())
}
fn verify_section(connection: &Connection, code: u8, state: &Section) -> StorageResult<()> {
    let mut statement=connection.prepare("SELECT scope,stage,records,bound,record_bytes,remaining,after_serial,maximum,digest FROM fact_owner WHERE table_id=?1")?;
    let mut rows = statement.query([code])?;
    let row = rows
        .next()?
        .ok_or(StorageError::Integrity("fact fixed owner missing"))?;
    let encoded = state.scope.as_ref().map(|s| s.encode());
    let scope = row.get_ref(0)?;
    let same_scope = match (scope, encoded.as_ref()) {
        (ValueRef::Null, None) => true,
        (ValueRef::Blob(b), Some(e)) => b == e,
        _ => false,
    };
    let digest = row.get_ref(8)?;
    let same_digest = match (digest, state.digest.as_ref()) {
        (ValueRef::Null, None) => true,
        (ValueRef::Blob(b), Some(e)) => b == e,
        _ => false,
    };
    if !same_scope
        || !same_digest
        || row.get::<_, u8>(1)? != state.stage
        || unsigned(row.get::<_, i64>(2)?)? != state.records
        || unsigned(row.get::<_, i64>(3)?)? != state.bound
        || unsigned(row.get::<_, i64>(4)?)? != state.bytes
        || unsigned(row.get::<_, i64>(5)?)? != state.remaining
        || row.get::<_, Option<i64>>(6)?.map(unsigned).transpose()? != state.after
        || row.get::<_, Option<i64>>(7)?.map(unsigned).transpose()? != state.maximum
    {
        return Err(StorageError::Integrity("fact exact owner metadata"));
    }
    Ok(())
}
pub(crate) fn write_owner(
    connection: &Connection,
    table: u8,
    state: &Section,
) -> StorageResult<()> {
    if connection.execute("UPDATE fact_owner SET scope=?1,stage=?2,records=?3,bound=?4,record_bytes=?5,remaining=?6,after_serial=?7 WHERE table_id=?8",
        rusqlite::params![state.scope.as_ref().map(|s|s.encode()).as_ref().map(|b|b.as_slice()),state.stage,state.records as i64,state.bound as i64,state.bytes as i64,state.remaining as i64,state.after.map(|n|n as i64),table])?!=1 {
        return Err(StorageError::Integrity("fact owner acknowledgement"));
    }
    if connection.execute(
        "UPDATE fact_owner SET maximum=?1,digest=?2 WHERE table_id=?3",
        rusqlite::params![
            state.maximum.map(|n| n as i64),
            state.digest.as_ref().map(|d| d.as_slice()),
            table
        ],
    )? != 1
    {
        return Err(StorageError::Integrity(
            "fact owner terminal acknowledgement",
        ));
    }
    Ok(())
}
