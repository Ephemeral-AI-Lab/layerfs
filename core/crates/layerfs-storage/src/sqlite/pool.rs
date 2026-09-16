//! Bounded value-group catalogue access.
//!
//! The catalogue is the only persisted index of pooled metadata: one row per
//! stored value group, carrying its ordinal span, its physical location and the
//! digest of the decoded group body. Values themselves live only inside the group
//! packs, so a reader authenticates the group before it trusts any ordinal.

use rusqlite::Connection;

use layerfs_content::ObjectId;

use crate::error::{StorageError, StorageResult};
use crate::policy::VALUES_PER_GROUP;

/// One catalogue row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueGroupRow {
    /// First ordinal the group covers.
    pub first_ordinal: u32,
    /// Number of values in the group.
    pub count: usize,
    /// Pack holding the group.
    pub pack_id: i64,
    /// Group ordinal inside the pack.
    pub group_number: usize,
    /// Digest of the decoded group body.
    pub digest: ObjectId,
}

/// Next ordinal a new value group starts at when the Store holds no group.
const FIRST_ORDINAL: u32 = 1;

/// Highest ordinal already assigned plus one.
///
/// Only rows whose start is within one group width of the maximum can reach the
/// end, so the exact `MAX(first_ordinal + count)` is found without scanning the
/// catalogue; the schema bounds every count to one group width.
pub fn next_ordinal(connection: &Connection) -> StorageResult<u32> {
    let next: i64 = connection.query_row(
        "SELECT COALESCE(MAX(first_ordinal + count), ?1) FROM metadata_value_groups \
         WHERE first_ordinal > (SELECT MAX(first_ordinal) FROM metadata_value_groups) - ?2",
        rusqlite::params![FIRST_ORDINAL, VALUES_PER_GROUP as i64],
        |row| row.get(0),
    )?;
    if !(i64::from(FIRST_ORDINAL)..=i64::from(u32::MAX) + 1).contains(&next) {
        return Err(StorageError::Integrity("metadata ordinal maximum"));
    }
    Ok(next as u32)
}

/// Inserts one catalogue row inside the caller's open transaction.
pub fn insert_group(connection: &Connection, row: &ValueGroupRow) -> StorageResult<()> {
    if row.count == 0 || row.count > VALUES_PER_GROUP || row.first_ordinal == 0 {
        return Err(StorageError::Integrity("metadata group row"));
    }
    let affected = connection.execute(
        "INSERT INTO metadata_value_groups \
         (first_ordinal, count, pack_id, group_number, digest) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            i64::from(row.first_ordinal),
            row.count as i64,
            row.pack_id,
            row.group_number as i64,
            row.digest.to_bytes().to_vec(),
        ],
    )?;
    if affected != 1 {
        return Err(StorageError::Integrity("metadata group insert cardinality"));
    }
    Ok(())
}

/// Catalogue row covering `ordinal`, when one exists.
pub fn group_for(connection: &Connection, ordinal: u32) -> StorageResult<Option<ValueGroupRow>> {
    if ordinal < FIRST_ORDINAL {
        return Err(StorageError::Integrity("metadata ordinal"));
    }
    let row = connection
        .query_row(
            "SELECT first_ordinal, count, pack_id, group_number, digest \
             FROM metadata_value_groups WHERE first_ordinal <= ?1 \
             ORDER BY first_ordinal DESC LIMIT 1",
            [i64::from(ordinal)],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                ))
            },
        )
        .map(Some)
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(StorageError::Engine(other)),
        })?;
    let Some(row) = row else {
        return Ok(None);
    };
    let first_ordinal =
        u32::try_from(row.0).map_err(|_| StorageError::Integrity("metadata first ordinal"))?;
    let count = usize::try_from(row.1).map_err(|_| StorageError::Integrity("metadata count"))?;
    if ordinal < first_ordinal
        || u64::from(ordinal) >= u64::from(first_ordinal) + count as u64
        || count == 0
        || count > VALUES_PER_GROUP
    {
        return Err(StorageError::Integrity("metadata ordinal range"));
    }
    Ok(Some(ValueGroupRow {
        first_ordinal,
        count,
        pack_id: row.2,
        group_number: usize::try_from(row.3)
            .map_err(|_| StorageError::Integrity("metadata group number"))?,
        digest: ObjectId::from_bytes(&row.4)?,
    }))
}

/// Every catalogue row in ordinal order, starting at `from` when given.
pub fn catalogue(connection: &Connection, from: Option<u32>) -> StorageResult<Vec<ValueGroupRow>> {
    let mut statement = connection.prepare(
        "SELECT first_ordinal, count, pack_id, group_number, digest \
         FROM metadata_value_groups WHERE first_ordinal >= ?1 ORDER BY first_ordinal",
    )?;
    let rows = statement.query_map([i64::from(from.unwrap_or(FIRST_ORDINAL))], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Vec<u8>>(4)?,
        ))
    })?;
    let mut groups = Vec::new();
    for row in rows {
        let row = row?;
        groups.push(ValueGroupRow {
            first_ordinal: u32::try_from(row.0)
                .map_err(|_| StorageError::Integrity("metadata first ordinal"))?,
            count: usize::try_from(row.1).map_err(|_| StorageError::Integrity("metadata count"))?,
            pack_id: row.2,
            group_number: usize::try_from(row.3)
                .map_err(|_| StorageError::Integrity("metadata group number"))?,
            digest: ObjectId::from_bytes(&row.4)?,
        });
    }
    Ok(groups)
}

/// Number of stored catalogue rows.
pub fn group_count(connection: &Connection) -> StorageResult<i64> {
    Ok(
        connection.query_row("SELECT COUNT(*) FROM metadata_value_groups", [], |row| {
            row.get(0)
        })?,
    )
}
