//! Exact typed scalar keys, indexed facts/jobs and fixed owner verification.
use super::alias_state::Aliases;
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{AliasCurrent, AliasFact, AliasProgress, SiteScope};
use rusqlite::{types::ValueRef, Connection, OptionalExtension};

pub(crate) fn unsigned(value: i64) -> StorageResult<u64> {
    u64::try_from(value).map_err(|_| StorageError::Integrity("alias negative integer"))
}
pub(crate) fn blob(value: ValueRef<'_>, width: usize) -> StorageResult<&[u8]> {
    match value {
        ValueRef::Blob(bytes) if bytes.len() == width => Ok(bytes),
        _ => Err(StorageError::Integrity("alias BLOB framing")),
    }
}

pub(crate) fn key(scope: &SiteScope, table: u8, scalar: u64) -> [u8; 25] {
    let mut key = [0; 25];
    key[..8].copy_from_slice(&scope.state().selection().token().to_be_bytes());
    key[8..16].copy_from_slice(&scope.state().phase().to_be_bytes());
    key[16] = table;
    key[17..].copy_from_slice(&scalar.to_be_bytes());
    key
}
pub(crate) fn scalar(scope: &SiteScope, table: u8, bytes: &[u8]) -> StorageResult<u64> {
    if bytes.len() != 25 {
        return Err(StorageError::Integrity("alias key length"));
    }
    let scalar = u64::from_be_bytes(bytes[17..].try_into().unwrap());
    if scalar == 0 || scalar > i64::MAX as u64 || bytes != key(scope, table, scalar) {
        return Err(StorageError::Integrity("alias selected key"));
    }
    Ok(scalar)
}
pub(crate) fn fact(
    connection: &Connection,
    state: &Aliases,
    serial: u64,
) -> StorageResult<Option<AliasFact>> {
    let key = key(&state.scope, 6, serial);
    let found: Option<(i64, u8)> = connection
        .query_row(
            "SELECT sequence,status FROM alias_facts WHERE key=?1",
            [key.as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    found
        .map(|(sequence, status)| {
            let fact = AliasFact {
                serial,
                sequence: unsigned(sequence)?,
                status,
            };
            fact.check()?;
            Ok(fact)
        })
        .transpose()
}
pub(crate) fn verify(connection: &Connection, state: &Aliases) -> StorageResult<()> {
    let mut statement=connection.prepare("SELECT scope,members,stage,sequence,facts,jobs,expanded,current_serial,current_sequence,progress,remaining,after_serial FROM alias_owner WHERE id=1")?;
    let mut rows = statement.query([])?;
    let row = rows
        .next()?
        .ok_or(StorageError::Integrity("alias owner missing"))?;
    let scope = blob(row.get_ref(0)?, 89)?;
    let encoded_members = state.members.as_ref().map(|m| m.encode());
    let members = row.get_ref(1)?;
    let members_match = match (members, encoded_members.as_ref()) {
        (rusqlite::types::ValueRef::Null, None) => true,
        (rusqlite::types::ValueRef::Blob(bytes), Some(expected)) => bytes == expected,
        _ => false,
    };
    let current = match (
        row.get::<_, Option<i64>>(7)?.map(unsigned).transpose()?,
        row.get::<_, Option<i64>>(8)?.map(unsigned).transpose()?,
    ) {
        (Some(serial), Some(sequence)) => Some(AliasCurrent { serial, sequence }),
        (None, None) => None,
        _ => return Err(StorageError::Integrity("alias current framing")),
    };
    let progress = AliasProgress::decode(blob(row.get_ref(9)?, 264)?)?;
    let t = state.totals;
    if scope != state.scope.as_bytes()
        || !members_match
        || row.get::<_, u8>(2)? != state.stage
        || unsigned(row.get::<_, i64>(3)?)? != t.sequence
        || unsigned(row.get::<_, i64>(4)?)? != t.facts
        || unsigned(row.get::<_, i64>(5)?)? != t.jobs
        || unsigned(row.get::<_, i64>(6)?)? != t.expanded
        || current != state.current
        || progress != state.progress
        || unsigned(row.get::<_, i64>(10)?)? != t.remaining
        || row.get::<_, Option<i64>>(11)?.map(unsigned).transpose()? != t.after
    {
        return Err(StorageError::Integrity("alias exact owner metadata"));
    }
    if rows.next()?.is_some() {
        return Err(StorageError::Integrity("alias owner multiplicity"));
    }
    Ok(())
}
pub(crate) fn empty(connection: &Connection, table: &str) -> StorageResult<bool> {
    Ok(connection.query_row(
        &format!("SELECT NOT EXISTS(SELECT 1 FROM {table} LIMIT 1)"),
        [],
        |r| r.get(0),
    )?)
}
