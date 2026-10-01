//! Closed claim owner checks, primary-key operations and bounded SQL writes.

use layerfs_content::filesystem::state::{ClaimAdmission, ClaimKey, ClaimRecord};
use rusqlite::{types::ValueRef, Connection, OptionalExtension, Statement};

use crate::error::{StorageError, StorageResult};

use super::phased::{ClaimPhase, Phased};
use super::profile;

pub(crate) const PRESENT: &str = "SELECT class FROM exclusive_claims WHERE key=?1";
pub(crate) const MAXIMUM: &str = "SELECT key FROM exclusive_claims ORDER BY key DESC LIMIT 1";
pub(crate) const FIRST: &str = "SELECT key,class FROM exclusive_claims ORDER BY key LIMIT ?1";
pub(crate) const AFTER: &str =
    "SELECT key,class FROM exclusive_claims WHERE key>?1 ORDER BY key LIMIT ?2";

fn blob(value: ValueRef<'_>, width: usize) -> StorageResult<&[u8]> {
    match value {
        ValueRef::Blob(bytes) if bytes.len() == width => Ok(bytes),
        _ => Err(StorageError::Integrity("construction scratch claim BLOB")),
    }
}

fn optional_blob(value: ValueRef<'_>, expected: Option<&[u8]>) -> StorageResult<()> {
    match expected {
        Some(bytes) if blob(value, bytes.len())? == bytes => Ok(()),
        None if matches!(value, ValueRef::Null) => Ok(()),
        _ => Err(StorageError::Integrity(
            "construction scratch claim owner field",
        )),
    }
}

pub(crate) fn verify(connection: &Connection, state: &Phased) -> StorageResult<()> {
    let mut statement = connection.prepare("SELECT declared_roots,declared_claims,claim_scope,claim_state,claim_records,claim_remaining,claim_digest,claim_max,claim_after FROM session_owner WHERE id=1")?;
    let mut rows = statement.query([])?;
    let row = rows
        .next()?
        .ok_or(StorageError::Integrity("construction scratch claim owner"))?;
    if row.get::<_, i64>(0)? != state.declared_roots as i64
        || row.get::<_, i64>(1)? != state.declared_claims as i64
        || blob(row.get_ref(2)?, 81)? != state.claims.as_bytes()
        || row.get::<_, i64>(3)? != state.phase as i64
        || row.get::<_, i64>(4)? != state.records as i64
        || row.get::<_, i64>(5)? != state.remaining as i64
    {
        return Err(StorageError::Integrity(
            "construction scratch exact claim owner",
        ));
    }
    optional_blob(
        row.get_ref(6)?,
        state.seal.as_ref().map(|seal| seal.digest().as_slice()),
    )?;
    optional_blob(
        row.get_ref(7)?,
        state.maximum.as_ref().map(|key| key.as_bytes().as_slice()),
    )?;
    optional_blob(
        row.get_ref(8)?,
        state.after.as_ref().map(|key| key.as_bytes().as_slice()),
    )?;
    if matches!(state.phase, ClaimPhase::Sealed | ClaimPhase::Retiring)
        && maximum(connection, state)? != state.maximum
    {
        return Err(StorageError::Integrity(
            "construction scratch immutable claim maximum",
        ));
    }
    if state.phase == ClaimPhase::Retired && !empty(connection)? {
        return Err(StorageError::Integrity(
            "construction scratch retired claims remain",
        ));
    }
    Ok(())
}

pub(crate) fn present(connection: &Connection, key: ClaimKey) -> StorageResult<bool> {
    let mut statement = connection.prepare(PRESENT)?;
    present_prepared(&mut statement, key)
}

fn present_prepared(statement: &mut Statement<'_>, key: ClaimKey) -> StorageResult<bool> {
    let mut rows = statement.query([key.as_bytes().as_slice()])?;
    let Some(row) = rows.next()? else {
        return Ok(false);
    };
    if blob(row.get_ref(0)?, 1)? != [1] {
        return Err(StorageError::Integrity(
            "construction scratch exclusive class",
        ));
    }
    Ok(true)
}

pub(crate) fn batch(
    connection: &Connection,
    engine: Option<&'static crate::engine::EngineGuard>,
    state: &Phased,
    keys: &[ClaimKey],
) -> StorageResult<ClaimAdmission> {
    if let Some(guard) = engine {
        guard.validate()?;
    }
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        verify(connection, state)?;
        // Every existing-key check precedes the first INSERT. The whole current
        // batch remains unacknowledged when a duplicate or bound is discovered.
        let mut lookup = connection.prepare(PRESENT)?;
        let mut duplicate = false;
        for key in keys {
            if present_prepared(&mut lookup, *key)? {
                duplicate = true;
            }
        }
        drop(lookup);
        if duplicate {
            return Ok(ClaimAdmission::Duplicate);
        }
        let count = state
            .records
            .checked_add(keys.len() as u64)
            .ok_or(StorageError::Integrity("construction scratch claim count"))?;
        if count > state.declared_claims {
            return Err(StorageError::CapacityExceeded {
                what: "construction scratch claim rows",
                limit: state.declared_claims,
                actual: count,
            });
        }
        let mut statement =
            connection.prepare("INSERT INTO exclusive_claims(key,class) VALUES(?1,x'01')")?;
        for key in keys {
            if statement.execute([key.as_bytes().as_slice()])? != 1 {
                return Err(StorageError::Integrity(
                    "construction scratch claim insertion",
                ));
            }
        }
        drop(statement);
        if connection.execute("UPDATE session_owner SET claim_records=?1 WHERE id=1 AND claim_state=0 AND claim_records=?2 AND claim_scope=?3",
            rusqlite::params![count as i64, state.records as i64, state.claims.as_bytes().as_slice()])? != 1 {
            return Err(StorageError::Integrity("construction scratch claim acknowledgement"));
        }
        Ok(ClaimAdmission::Fresh)
    })();
    match result {
        Ok(ClaimAdmission::Fresh) => {
            profile::finish_write_guarded(connection, Ok(()), engine)?;
            Ok(ClaimAdmission::Fresh)
        }
        Ok(ClaimAdmission::Duplicate) => {
            if let Some(guard) = engine {
                guard
                    .validate()
                    .map_err(|original| StorageError::UnknownOutcome {
                        original: Box::new(original),
                    })?;
            }
            crate::sqlite::write::rollback(connection)?;
            Ok(ClaimAdmission::Duplicate)
        }
        Err(error) => {
            profile::finish_write_guarded(connection, Err(error), engine)?;
            unreachable!()
        }
    }
}

pub(crate) fn maximum(connection: &Connection, state: &Phased) -> StorageResult<Option<ClaimKey>> {
    let mut statement = connection.prepare(MAXIMUM)?;
    let mut rows = statement.query([])?;
    rows.next()?
        .map(|row| Ok(ClaimKey::decode(&state.claims, blob(row.get_ref(0)?, 25)?)?))
        .transpose()
}

/// One actual bounded owner, with no insertion ordinal or repeated rank query.
pub(crate) fn read(
    connection: &Connection,
    state: &Phased,
    after: Option<ClaimKey>,
    count: usize,
) -> StorageResult<Vec<ClaimRecord>> {
    let mut output = Vec::new();
    output.try_reserve_exact(count).map_err(|_| {
        StorageError::Content(layerfs_content::ContentError::ResourceUnavailable {
            what: "construction scratch claim page",
        })
    })?;
    if count == 0 {
        return Ok(output);
    }
    let mut statement = connection.prepare(if after.is_some() { AFTER } else { FIRST })?;
    let mut rows = match after {
        Some(key) => statement.query(rusqlite::params![key.as_bytes().as_slice(), count as i64])?,
        None => statement.query([count as i64])?,
    };
    let mut previous = after;
    while let Some(row) = rows.next()? {
        let key = ClaimKey::decode(&state.claims, blob(row.get_ref(0)?, 25)?)?;
        if blob(row.get_ref(1)?, 1)? != [1]
            || previous.is_some_and(|last| last >= key)
            || output.len() == count
        {
            return Err(StorageError::Integrity(
                "construction scratch claim page order/class",
            ));
        }
        output.push(ClaimRecord::new(key));
        previous = Some(key);
    }
    Ok(output)
}

pub(crate) fn empty(connection: &Connection) -> StorageResult<bool> {
    Ok(connection
        .query_row("SELECT 1 FROM exclusive_claims LIMIT 1", [], |_| Ok(()))
        .optional()?
        .is_none())
}
