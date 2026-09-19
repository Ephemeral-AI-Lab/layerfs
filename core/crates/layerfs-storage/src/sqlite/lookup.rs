//! Bounded membership, location and presence queries.
//!
//! Every query is paged: at most [`LOOKUP_PAGE_IDS`] identifiers per statement,
//! with the parameter list built from the actual page width. A presence check
//! never fetches a locator it does not need, and a read never issues one query
//! per object.
//!
//! A locator is row metadata only. The direct base identity is **not** a column:
//! it lives in the packed record, and a caller that needs it reads it from there
//! (`encoding::delta::read::ChainBases`, `encoding::pool::PoolReader`). A
//! metadata query therefore never claims to know a base it has not read.

use rusqlite::types::Value;
use rusqlite::Connection;

use layerfs_content::{ObjectId, ObjectRole};

use crate::error::{StorageError, StorageResult};
use crate::policy::LOOKUP_PAGE_IDS;

/// One stored object's descriptor and physical location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectLocation {
    /// Canonical identity.
    pub object_id: ObjectId,
    /// Logical role code.
    pub role: ObjectRole,
    /// Canonical object length recorded at save time.
    pub canonical_length: usize,
    /// Pack holding the record.
    pub pack_id: i64,
    /// Group ordinal inside the pack.
    pub group_number: usize,
    /// Record ordinal inside the group.
    pub record_number: usize,
}

/// Pages `ids` into bounded groups.
pub fn pages(ids: &[ObjectId]) -> impl Iterator<Item = &[ObjectId]> {
    ids.chunks(LOOKUP_PAGE_IDS)
}

fn placeholders(count: usize, first: usize) -> String {
    (0..count)
        .map(|index| format!("?{}", first + index))
        .collect::<Vec<_>>()
        .join(",")
}

fn id_value(id: ObjectId) -> Value {
    Value::Blob(id.to_bytes().to_vec())
}

/// Returns locations for the identifiers that are present, honouring a ceiling.
pub fn locations(
    connection: &Connection,
    ids: &[ObjectId],
    ceiling: i64,
) -> StorageResult<Vec<ObjectLocation>> {
    let mut found = Vec::new();
    for page in pages(ids) {
        let sql = format!(
            "SELECT object_id, object_role, canonical_length, pack_id, group_number, record_number \
             FROM objects WHERE object_id IN ({}) AND pack_id <= ?{}",
            placeholders(page.len(), 1),
            page.len() + 1
        );
        let mut parameters: Vec<Value> = page.iter().copied().map(id_value).collect();
        parameters.push(Value::Integer(ceiling));
        let mut statement = connection.prepare_cached(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(parameters), |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;
        for row in rows {
            found.push(decode_location(row?)?);
        }
    }
    Ok(found)
}

/// Reads one stored location under a ceiling.
pub fn location(
    connection: &Connection,
    id: ObjectId,
    ceiling: i64,
) -> StorageResult<Option<ObjectLocation>> {
    let mut found = locations(connection, std::slice::from_ref(&id), ceiling)?;
    found.pop().map_or(Ok(None), |location| {
        if location.object_id == id {
            Ok(Some(location))
        } else {
            Err(StorageError::Integrity("locator identity"))
        }
    })
}

type RawRow = (Vec<u8>, i64, i64, i64, i64, i64);

fn decode_location(row: RawRow) -> StorageResult<ObjectLocation> {
    let role = u8::try_from(row.1).map_err(|_| StorageError::Integrity("object role"))?;
    Ok(ObjectLocation {
        object_id: ObjectId::from_bytes(&row.0)?,
        role: ObjectRole::from_code(role)?,
        canonical_length: usize::try_from(row.2)
            .map_err(|_| StorageError::Integrity("canonical length"))?,
        pack_id: row.3,
        group_number: usize::try_from(row.4)
            .map_err(|_| StorageError::Integrity("group number"))?,
        record_number: usize::try_from(row.5)
            .map_err(|_| StorageError::Integrity("record number"))?,
    })
}

/// Returns the subset of `ids` that is present, without fetching locators.
pub fn present(
    connection: &Connection,
    ids: &[ObjectId],
    ceiling: i64,
) -> StorageResult<Vec<ObjectId>> {
    let mut found = Vec::new();
    for page in pages(ids) {
        let sql = format!(
            "SELECT object_id FROM objects WHERE object_id IN ({}) AND pack_id <= ?{}",
            placeholders(page.len(), 1),
            page.len() + 1
        );
        let mut parameters: Vec<Value> = page.iter().copied().map(id_value).collect();
        parameters.push(Value::Integer(ceiling));
        let mut statement = connection.prepare_cached(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(parameters), |row| {
            row.get::<_, Vec<u8>>(0)
        })?;
        for row in rows {
            found.push(ObjectId::from_bytes(&row?)?);
        }
    }
    Ok(found)
}

/// Reads one pack BLOB by primary key.
pub fn pack_bytes(connection: &Connection, pack_id: i64) -> StorageResult<Vec<u8>> {
    connection
        .query_row(
            "SELECT data FROM object_packs WHERE pack_id = ?1",
            [pack_id],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => StorageError::Integrity("pack row is missing"),
            other => StorageError::Engine(other),
        })
}

/// Highest pack identifier currently stored, or zero when the Store is empty.
pub fn highest_pack_id(connection: &Connection) -> StorageResult<i64> {
    Ok(connection.query_row(
        "SELECT COALESCE(MAX(pack_id), 0) FROM object_packs",
        [],
        |row| row.get(0),
    )?)
}
