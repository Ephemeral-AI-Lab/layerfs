//! Closed bounded primary-key queries and acknowledged append transactions.

use layerfs_content::filesystem::state::{
    PageLimit, StateKey, StatePage, StateRecord, StateScope, StateSeal,
};
use layerfs_content::{ContentError, ObjectId};
use rusqlite::{types::ValueRef, Connection, OptionalExtension};

use crate::error::{StorageError, StorageResult};

use super::profile;

pub(crate) const GET_SQL: &str = "SELECT root FROM directory_roots WHERE key=?1";
pub(crate) const BEFORE_SQL: &str =
    "SELECT ordinal FROM directory_roots WHERE key<=?1 ORDER BY key DESC LIMIT 1";
pub(crate) const PAGE_SQL: &str =
    "SELECT key,ordinal,root FROM directory_roots WHERE key>?1 AND key<=?2 ORDER BY key LIMIT ?3";

fn blob(value: ValueRef<'_>, length: usize) -> StorageResult<&[u8]> {
    match value {
        ValueRef::Blob(bytes) if bytes.len() == length => Ok(bytes),
        _ => Err(StorageError::Integrity(
            "construction scratch BLOB width/type",
        )),
    }
}

pub(crate) fn verify_header(
    connection: &Connection,
    header: &[u8],
    seal: Option<&StateSeal>,
) -> StorageResult<()> {
    let mut statement = connection.prepare(
        "SELECT header,scope,sealed,records,record_bytes,digest FROM session_owner WHERE id=1",
    )?;
    let mut rows = statement.query([])?;
    let row = rows.next()?.ok_or(StorageError::Integrity(
        "construction scratch owner missing",
    ))?;
    if blob(row.get_ref(0)?, header.len())? != header {
        return Err(StorageError::Integrity(
            "construction scratch owner association",
        ));
    }
    if let Some(seal) = seal {
        if blob(row.get_ref(1)?, 81)? != seal.scope().as_bytes()
            || row.get::<_, i64>(2)? != 1
            || row.get::<_, i64>(3)? != seal.records() as i64
            || row.get::<_, i64>(4)? != seal.encoded_bytes() as i64
            || blob(row.get_ref(5)?, 32)? != seal.digest()
        {
            return Err(StorageError::Integrity(
                "construction scratch sealed owner facts",
            ));
        }
    }
    Ok(())
}

fn verify_writable(
    connection: &Connection,
    scope: &StateScope,
    previous: u64,
) -> StorageResult<()> {
    let mut statement = connection
        .prepare("SELECT scope,sealed,records,record_bytes,digest FROM session_owner WHERE id=1")?;
    let mut rows = statement.query([])?;
    let row = rows.next()?.ok_or(StorageError::Integrity(
        "construction scratch owner missing",
    ))?;
    let scope_matches = match row.get_ref(0)? {
        ValueRef::Null => previous == 0,
        value => blob(value, 81)? == scope.as_bytes(),
    };
    if !scope_matches
        || row.get::<_, i64>(1)? != 0
        || row.get::<_, i64>(2)? != previous as i64
        || row.get::<_, i64>(3)? != (previous * profile::RECORD_BYTES) as i64
        || !matches!(row.get_ref(4)?, ValueRef::Null)
    {
        return Err(StorageError::Integrity(
            "construction scratch writable owner facts",
        ));
    }
    Ok(())
}

pub(crate) fn append(
    connection: &Connection,
    engine: Option<&'static crate::engine::EngineGuard>,
    scope: &StateScope,
    records: &[StateRecord],
    previous: u64,
) -> StorageResult<()> {
    if let Some(guard) = engine {
        guard.validate()?;
    }
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        // The admitted batch has already passed its widths/order/capacity checks.
        // Check immutable phase/totals under this transaction before row effects.
        verify_writable(connection, scope, previous)?;
        let mut statement =
            connection.prepare("INSERT INTO directory_roots(key,ordinal,root) VALUES(?1,?2,?3)")?;
        for (index, record) in records.iter().enumerate() {
            let ordinal = previous + index as u64 + 1;
            if statement.execute(rusqlite::params![
                record.key().as_bytes().as_slice(),
                ordinal as i64,
                record.root().as_bytes().as_slice()
            ])? != 1
            {
                return Err(StorageError::Integrity(
                    "construction scratch append cardinality",
                ));
            }
        }
        drop(statement);
        let count = previous + records.len() as u64;
        let bytes = count * profile::RECORD_BYTES;
        if connection.execute("UPDATE session_owner SET scope=?1,records=?2,record_bytes=?3 WHERE id=1 AND sealed=0 AND records=?4",
            rusqlite::params![scope.as_bytes().as_slice(), count as i64, bytes as i64, previous as i64])? != 1
        {
            return Err(StorageError::Integrity("construction scratch append owner totals"));
        }
        Ok(())
    })();
    profile::finish_write_guarded(connection, result, engine)
}

pub(crate) fn seal(
    connection: &Connection,
    engine: Option<&'static crate::engine::EngineGuard>,
    seal: &StateSeal,
) -> StorageResult<()> {
    if let Some(guard) = engine {
        guard.validate()?;
    }
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        verify_writable(connection, seal.scope(), seal.records())?;
        if connection.execute("UPDATE session_owner SET scope=?1,sealed=1,digest=?2 WHERE id=1 AND sealed=0 AND records=?3 AND record_bytes=?4",
            rusqlite::params![seal.scope().as_bytes().as_slice(), seal.digest().as_slice(),
                seal.records() as i64, seal.encoded_bytes() as i64])? != 1
        {
            return Err(StorageError::Integrity("construction scratch seal owner totals"));
        }
        Ok(())
    })();
    profile::finish_write_guarded(connection, result, engine)
}

pub(crate) fn get(
    connection: &Connection,
    seal: &StateSeal,
    key: StateKey,
) -> StorageResult<Option<StateRecord>> {
    StateKey::decode(seal.scope(), key.as_bytes())?;
    let mut statement = connection.prepare(GET_SQL)?;
    let mut rows = statement.query([key.as_bytes().as_slice()])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let root = ObjectId::from_bytes(blob(row.get_ref(0)?, 32)?)?;
    Ok(Some(StateRecord::new(key, root)))
}

pub(crate) fn page(
    connection: &Connection,
    seal: &StateSeal,
    after: Option<StateKey>,
    limit: PageLimit,
) -> StorageResult<StatePage> {
    let mut lower = *StateKey::directory_root(seal.scope(), 1)?.as_bytes();
    lower[17..].fill(0);
    if let Some(key) = after {
        StateKey::decode(seal.scope(), key.as_bytes())?;
        lower = *key.as_bytes();
    }
    let upper = StateKey::directory_root(seal.scope(), i64::MAX as u64)?;
    let previous: Option<i64> = connection
        .query_row(BEFORE_SQL, [lower.as_slice()], |row| row.get(0))
        .optional()?;
    let previous = previous.unwrap_or(0);
    if previous < 0 || previous as u64 > seal.records() {
        return Err(StorageError::Integrity("construction scratch page ordinal"));
    }
    let remaining = seal.records() - previous as u64;
    let count = limit.fitting_records().min(remaining as usize);
    if remaining != 0 && count == 0 {
        limit.check_records(1)?;
        return Err(StorageError::Integrity(
            "construction scratch page failed to advance",
        ));
    }
    let mut output = Vec::new();
    output.try_reserve_exact(count).map_err(|_| {
        StorageError::Content(ContentError::ResourceUnavailable {
            what: "construction scratch page owner",
        })
    })?;
    if count != 0 {
        let mut statement = connection.prepare(PAGE_SQL)?;
        let mut rows = statement.query(rusqlite::params![
            lower.as_slice(),
            upper.as_bytes().as_slice(),
            count as i64
        ])?;
        let mut last = after;
        while let Some(row) = rows.next()? {
            // Borrow and check both BLOB sizes before copying their fixed fields.
            let key_bytes = blob(row.get_ref(0)?, 25)?;
            let root_bytes = blob(row.get_ref(2)?, 32)?;
            let key = StateKey::decode(seal.scope(), key_bytes)?;
            let ordinal: i64 = row.get(1)?;
            if ordinal != previous + output.len() as i64 + 1
                || last.is_some_and(|last| last >= key)
                || output.len() >= count
            {
                return Err(StorageError::Integrity("construction scratch ordered page"));
            }
            output.push(StateRecord::new(key, ObjectId::from_bytes(root_bytes)?));
            last = Some(key);
        }
        if output.len() != count {
            return Err(StorageError::Integrity(
                "construction scratch sealed page EOF",
            ));
        }
    }
    let page = StatePage::after(seal.clone(), after, output, count as u64 == remaining)?;
    page.check_limit(limit)?;
    Ok(page)
}
