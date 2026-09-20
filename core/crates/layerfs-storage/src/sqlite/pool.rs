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

/// One raw catalogue row in the fixed selected column order.
type CatalogueRow = (i64, i64, i64, i64, Vec<u8>);

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

/// Exclusive end of the assigned ordinal space, as an integer wide enough to hold
/// the end of a full catalogue (`2^32`).
///
/// Only rows whose start is within one group width of the maximum can reach the
/// end, so the exact `MAX(first_ordinal + count)` is found without scanning the
/// catalogue; the schema bounds every count to one group width. This is the
/// *cursor* reading: a synchronizing reader needs the end of the space even when
/// no further ordinal can be assigned, so the ceiling is not an error here.
pub fn ordinal_end(connection: &Connection) -> StorageResult<u64> {
    let next: i64 = connection.query_row(
        "SELECT COALESCE(MAX(first_ordinal + count), ?1) FROM metadata_value_groups \
         WHERE first_ordinal > (SELECT MAX(first_ordinal) FROM metadata_value_groups) - ?2",
        rusqlite::params![FIRST_ORDINAL, VALUES_PER_GROUP as i64],
        |row| row.get(0),
    )?;
    if !(i64::from(FIRST_ORDINAL)..=i64::from(u32::MAX) + 1).contains(&next) {
        return Err(StorageError::Integrity("metadata ordinal maximum"));
    }
    Ok(next as u64)
}

/// Ordinal a new value group may start at, refusing an exhausted space.
///
/// The ordinal column is a `u32`, so the exclusive end of a full catalogue
/// (`2^32`) is not assignable. Naming that here makes the ceiling report itself:
/// before this check the end value was truncated to `0` by the cast, and the
/// caller stored a group at ordinal zero, which failed later as an unrelated
/// "metadata group row" or "metadata index chronology" integrity error.
pub fn next_ordinal(connection: &Connection) -> StorageResult<u32> {
    u32::try_from(ordinal_end(connection)?)
        .map_err(|_| StorageError::Integrity("metadata ordinal maximum"))
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
    let mut statement = connection.prepare_cached(
        "SELECT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest \
         FROM metadata_value_groups g JOIN object_packs p USING(pack_id) JOIN saves s USING(save_id),temp.layerfs_read_scope r \
         WHERE g.first_ordinal = (SELECT MAX(first_ordinal) FROM metadata_value_groups WHERE first_ordinal <= ?1) \
         AND (p.save_id = r.save_id OR s.publication <= r.publication)",
    )?;
    let row = statement
        .query_row([i64::from(ordinal)], decode_group_row)
        .map(Some)
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(StorageError::Engine(other)),
        })?;
    let Some(row) = row else {
        return Ok(None);
    };
    let group = checked_group_row(row)?;
    if ordinal < group.first_ordinal
        || u64::from(ordinal) >= u64::from(group.first_ordinal) + group.count as u64
        || group.count == 0
        || group.count > VALUES_PER_GROUP
    {
        return Err(StorageError::Integrity("metadata ordinal range"));
    }
    Ok(Some(group))
}

/// Decodes one catalogue row selected in the fixed `first_ordinal, count,
/// pack_id, group_number, digest` order.
fn decode_group_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CatalogueRow> {
    Ok((
        row.get::<_, i64>(0)?,
        row.get::<_, i64>(1)?,
        row.get::<_, i64>(2)?,
        row.get::<_, i64>(3)?,
        row.get::<_, Vec<u8>>(4)?,
    ))
}

/// Converts one selected catalogue row into its checked form.
fn checked_group_row(row: CatalogueRow) -> StorageResult<ValueGroupRow> {
    Ok(ValueGroupRow {
        first_ordinal: u32::try_from(row.0)
            .map_err(|_| StorageError::Integrity("metadata first ordinal"))?,
        count: usize::try_from(row.1).map_err(|_| StorageError::Integrity("metadata count"))?,
        pack_id: row.2,
        group_number: usize::try_from(row.3)
            .map_err(|_| StorageError::Integrity("metadata group number"))?,
        digest: ObjectId::from_bytes(&row.4)?,
    })
}

/// Visits every catalogue row in ordinal order, starting at `from` when given.
///
/// The statement is streamed: one row is decoded and handed to the visitor at a
/// time, so a caller that only needs the eviction recurrence holds no
/// size-proportional transient - the catalogue can be far larger than the retained
/// window, and nothing here materialises it.
pub fn for_each_group(
    connection: &Connection,
    from: Option<u32>,
    mut visit: impl FnMut(ValueGroupRow) -> StorageResult<()>,
) -> StorageResult<()> {
    let mut statement = connection.prepare_cached(
        "SELECT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest \
         FROM metadata_value_groups g JOIN object_packs p USING(pack_id) JOIN saves s USING(save_id),temp.layerfs_read_scope r \
         WHERE g.first_ordinal >= ?1 AND (p.save_id = r.save_id OR s.publication <= r.publication) ORDER BY g.first_ordinal",
    )?;
    let mut rows = statement.query([i64::from(from.unwrap_or(FIRST_ORDINAL))])?;
    while let Some(row) = rows.next()? {
        visit(checked_group_row(decode_group_row(row)?)?)?;
    }
    Ok(())
}

/// Number of stored catalogue rows.
pub fn group_count(connection: &Connection) -> StorageResult<i64> {
    Ok(
        connection.query_row("SELECT COUNT(*) FROM metadata_value_groups", [], |row| {
            row.get(0)
        })?,
    )
}

/// First ordinal reserved in the bounded current candidate window.
pub fn window_start(connection: &Connection) -> StorageResult<u32> {
    Ok(connection.query_row(
        "SELECT metadata_window_start FROM store_policy WHERE id = 1",
        [],
        |row| row.get(0),
    )?)
}
