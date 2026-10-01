//! Closed v3 owner checks and actual indexed site projections.

use layerfs_content::filesystem::state::{SiteKey, SiteRecord, SiteScope};
use rusqlite::{types::ValueRef, Connection, OptionalExtension, Row};

use crate::error::{StorageError, StorageResult};

use super::sites::{SiteStage, Sites};

pub(crate) const GET: &str =
    "SELECT key,flags,point,parent,binding_ordinal FROM binding_sites WHERE key=?1";
pub(crate) const GET_POINT: &str = "SELECT key,flags,point,parent,binding_ordinal FROM binding_sites WHERE parent=?1 AND binding_ordinal=?2";
pub(crate) const MAXIMUM: &str =
    "SELECT key,flags,point,parent,binding_ordinal FROM binding_sites ORDER BY key DESC LIMIT 1";
pub(crate) const FIRST: &str =
    "SELECT key,flags,point,parent,binding_ordinal FROM binding_sites ORDER BY key LIMIT ?1";
pub(crate) const AFTER: &str = "SELECT key,flags,point,parent,binding_ordinal FROM binding_sites WHERE key>?1 ORDER BY key LIMIT ?2";
pub(crate) const BIRTH_FIRST: &str = "SELECT key,flags,point,parent,binding_ordinal FROM binding_sites INDEXED BY site_birth_order ORDER BY parent,binding_ordinal LIMIT ?1";
pub(crate) const BIRTH_AFTER: &str = "SELECT key,flags,point,parent,binding_ordinal FROM binding_sites INDEXED BY site_birth_order WHERE (parent,binding_ordinal)>(?1,?2) ORDER BY parent,binding_ordinal LIMIT ?3";
pub(crate) const PARENT_MAX: &str = "SELECT key,flags,point,parent,binding_ordinal FROM binding_sites INDEXED BY site_existing_parent WHERE (flags&1)=1 AND parent=?1 ORDER BY binding_ordinal DESC LIMIT 1";
pub(crate) const PARENT_FIRST: &str = "SELECT key,flags,point,parent,binding_ordinal FROM binding_sites INDEXED BY site_existing_parent WHERE (flags&1)=1 AND parent=?1 ORDER BY binding_ordinal LIMIT ?2";
pub(crate) const PARENT_AFTER: &str = "SELECT key,flags,point,parent,binding_ordinal FROM binding_sites INDEXED BY site_existing_parent WHERE (flags&1)=1 AND parent=?1 AND binding_ordinal>?2 ORDER BY binding_ordinal LIMIT ?3";

fn blob(value: ValueRef<'_>, width: usize) -> StorageResult<&[u8]> {
    match value {
        ValueRef::Blob(bytes) if bytes.len() == width => Ok(bytes),
        _ => Err(StorageError::Integrity("construction scratch site BLOB")),
    }
}

fn optional_blob(value: ValueRef<'_>, expected: Option<&[u8]>) -> StorageResult<()> {
    match expected {
        Some(bytes) if blob(value, bytes.len())? == bytes => Ok(()),
        None if matches!(value, ValueRef::Null) => Ok(()),
        _ => Err(StorageError::Integrity(
            "construction scratch site owner field",
        )),
    }
}

pub(crate) fn decode(scope: &SiteScope, row: &Row<'_>) -> StorageResult<SiteRecord> {
    let key = blob(row.get_ref(0)?, 25)?;
    let flags: i64 = row.get(1)?;
    let point = blob(row.get_ref(2)?, 28)?;
    if !matches!(flags, 0 | 1 | 3 | 7) {
        return Err(StorageError::Integrity("construction scratch site flags"));
    }
    let mut frame = [0; 60];
    frame[..2].copy_from_slice(&25u16.to_be_bytes());
    frame[2..27].copy_from_slice(key);
    frame[27..31].copy_from_slice(&29u32.to_be_bytes());
    frame[31] = flags as u8;
    frame[32..].copy_from_slice(point);
    let record = SiteRecord::decode(scope, &frame)?;
    if row.get::<_, i64>(3)? != record.point().parent() as i64
        || row.get::<_, i64>(4)? != i64::from(record.point().binding_ordinal())
    {
        return Err(StorageError::Integrity(
            "construction scratch site redundant point",
        ));
    }
    Ok(record)
}

pub(crate) fn verify(connection: &Connection, state: &Sites) -> StorageResult<()> {
    let declarations: (i64, i64) = connection.query_row(
        "SELECT declared_roots,declared_sites FROM session_owner WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if declarations != (state.declared_roots as i64, state.declared_sites as i64) {
        return Err(StorageError::Integrity(
            "construction scratch site declarations",
        ));
    }
    let mut statement = connection.prepare("SELECT scope,stage,records,remaining,birth_digest,birth_max,final_digest,final_max,after_key FROM site_owner WHERE id=1")?;
    let mut rows = statement.query([])?;
    let row = rows
        .next()?
        .ok_or(StorageError::Integrity("construction scratch site owner"))?;
    if blob(row.get_ref(0)?, 89)? != state.scope.as_bytes()
        || row.get::<_, i64>(1)? != state.stage as i64
        || row.get::<_, i64>(2)? != state.records as i64
        || row.get::<_, i64>(3)? != state.remaining as i64
    {
        return Err(StorageError::Integrity(
            "construction scratch exact site owner",
        ));
    }
    optional_blob(
        row.get_ref(4)?,
        state
            .membership
            .as_ref()
            .map(|members| members.birth().digest().as_slice()),
    )?;
    let maximum = state.maximum();
    optional_blob(
        row.get_ref(5)?,
        maximum.as_ref().map(|key| key.as_bytes().as_slice()),
    )?;
    optional_blob(
        row.get_ref(6)?,
        state.seal.as_ref().map(|seal| seal.digest().as_slice()),
    )?;
    optional_blob(
        row.get_ref(7)?,
        state
            .seal
            .as_ref()
            .and(maximum.as_ref())
            .map(|key| key.as_bytes().as_slice()),
    )?;
    optional_blob(
        row.get_ref(8)?,
        state.after.as_ref().map(|key| key.as_bytes().as_slice()),
    )?;
    if matches!(
        state.stage,
        SiteStage::Facts | SiteStage::FinalSealed | SiteStage::Retiring
    ) && self::maximum(connection, &state.scope)? != maximum
    {
        return Err(StorageError::Integrity(
            "construction scratch immutable site maximum",
        ));
    }
    if state.stage == SiteStage::Retired && !empty(connection)? {
        return Err(StorageError::Integrity(
            "construction scratch retired sites remain",
        ));
    }
    Ok(())
}

pub(crate) fn get(
    connection: &Connection,
    scope: &SiteScope,
    key: SiteKey,
) -> StorageResult<Option<SiteRecord>> {
    let mut statement = connection.prepare(GET)?;
    let mut rows = statement.query([key.as_bytes().as_slice()])?;
    rows.next()?.map(|row| decode(scope, row)).transpose()
}

pub(crate) fn maximum(
    connection: &Connection,
    scope: &SiteScope,
) -> StorageResult<Option<SiteKey>> {
    let mut statement = connection.prepare(MAXIMUM)?;
    let mut rows = statement.query([])?;
    rows.next()?
        .map(|row| Ok(decode(scope, row)?.key()))
        .transpose()
}

pub(crate) fn birth_maximum(
    connection: &Connection,
    scope: &SiteScope,
) -> StorageResult<Option<(u64, u32)>> {
    let mut statement = connection.prepare("SELECT key,flags,point,parent,binding_ordinal FROM binding_sites INDEXED BY site_birth_order ORDER BY parent DESC,binding_ordinal DESC LIMIT 1")?;
    let mut rows = statement.query([])?;
    rows.next()?
        .map(|row| {
            let record = decode(scope, row)?;
            Ok((record.point().parent(), record.point().binding_ordinal()))
        })
        .transpose()
}

fn output(count: usize) -> StorageResult<Vec<SiteRecord>> {
    if count > 128 {
        return Err(StorageError::Integrity("construction scratch site window"));
    }
    let mut output = Vec::new();
    output.try_reserve_exact(count).map_err(|_| {
        StorageError::Content(layerfs_content::ContentError::ResourceUnavailable {
            what: "construction scratch site page",
        })
    })?;
    if output.capacity() > 128 {
        return Err(StorageError::Integrity(
            "construction scratch site page capacity",
        ));
    }
    Ok(output)
}

pub(crate) fn read(
    connection: &Connection,
    scope: &SiteScope,
    after: Option<SiteKey>,
    count: usize,
) -> StorageResult<Vec<SiteRecord>> {
    let mut output = output(count)?;
    if count == 0 {
        return Ok(output);
    }
    let mut statement = connection.prepare(if after.is_some() { AFTER } else { FIRST })?;
    let mut rows = match after {
        Some(key) => statement.query(rusqlite::params![key.as_bytes().as_slice(), count as i64])?,
        None => statement.query([count as i64])?,
    };
    let mut last = after;
    while let Some(row) = rows.next()? {
        let record = decode(scope, row)?;
        if output.len() == count || last.is_some_and(|key| key >= record.key()) {
            return Err(StorageError::Integrity(
                "construction scratch site primary order",
            ));
        }
        last = Some(record.key());
        output.push(record);
    }
    Ok(output)
}

pub(crate) fn read_birth(
    connection: &Connection,
    scope: &SiteScope,
    after: Option<(u64, u32)>,
    count: usize,
) -> StorageResult<Vec<SiteRecord>> {
    let mut output = output(count)?;
    if count == 0 {
        return Ok(output);
    }
    let mut statement = connection.prepare(if after.is_some() {
        BIRTH_AFTER
    } else {
        BIRTH_FIRST
    })?;
    let mut rows = match after {
        Some((parent, ordinal)) => statement.query(rusqlite::params![
            parent as i64,
            i64::from(ordinal),
            count as i64
        ])?,
        None => statement.query([count as i64])?,
    };
    let mut last = after;
    while let Some(row) = rows.next()? {
        let record = decode(scope, row)?;
        let tuple = (record.point().parent(), record.point().binding_ordinal());
        if output.len() == count || last.is_some_and(|prior| prior >= tuple) {
            return Err(StorageError::Integrity(
                "construction scratch site birth order",
            ));
        }
        last = Some(tuple);
        output.push(record);
    }
    Ok(output)
}

pub(crate) fn parent_maximum(
    connection: &Connection,
    scope: &SiteScope,
    parent: u64,
) -> StorageResult<Option<u32>> {
    let mut statement = connection.prepare(PARENT_MAX)?;
    let mut rows = statement.query([parent as i64])?;
    rows.next()?
        .map(|row| {
            let record = decode(scope, row)?;
            if !record.has_base() || record.point().parent() != parent {
                return Err(StorageError::Integrity(
                    "construction scratch site parent maximum",
                ));
            }
            Ok(record.point().binding_ordinal())
        })
        .transpose()
}

pub(crate) fn read_parent(
    connection: &Connection,
    scope: &SiteScope,
    parent: u64,
    after: Option<u32>,
    count: usize,
) -> StorageResult<Vec<SiteRecord>> {
    let mut output = output(count)?;
    if count == 0 {
        return Ok(output);
    }
    let mut statement = connection.prepare(if after.is_some() {
        PARENT_AFTER
    } else {
        PARENT_FIRST
    })?;
    let mut rows = match after {
        Some(ordinal) => statement.query(rusqlite::params![
            parent as i64,
            i64::from(ordinal),
            count as i64
        ])?,
        None => statement.query(rusqlite::params![parent as i64, count as i64])?,
    };
    let mut last = after;
    while let Some(row) = rows.next()? {
        let record = decode(scope, row)?.birth_projection();
        let ordinal = record.point().binding_ordinal();
        if output.len() == count
            || !record.has_base()
            || record.point().parent() != parent
            || last.is_some_and(|prior| prior >= ordinal)
        {
            return Err(StorageError::Integrity(
                "construction scratch site parent order",
            ));
        }
        last = Some(ordinal);
        output.push(record);
    }
    Ok(output)
}

pub(crate) fn empty(connection: &Connection) -> StorageResult<bool> {
    for sql in [
        "SELECT 1 FROM binding_sites LIMIT 1",
        "SELECT 1 FROM binding_sites INDEXED BY site_birth_order LIMIT 1",
        "SELECT 1 FROM binding_sites INDEXED BY site_existing_parent WHERE (flags&1)=1 LIMIT 1",
    ] {
        if connection
            .query_row(sql, [], |_| Ok(()))
            .optional()?
            .is_some()
        {
            return Ok(false);
        }
    }
    Ok(true)
}
