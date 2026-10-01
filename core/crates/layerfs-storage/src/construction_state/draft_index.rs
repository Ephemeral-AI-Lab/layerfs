//! Exact scope/readback and bounded private draft queries.
use super::draft_state::{Drafts, Header};
use crate::error::{StorageError, StorageResult};
use layerfs_content::file::edit::{DraftRecord, DraftScope};
use layerfs_content::{ContentError, ObjectId};
use rusqlite::{Connection, OptionalExtension};

// Exact persisted draft_headers column order; no additional allocation or conversion.
type HeaderRow = (i64, i64, i64, i64, i64, i64, i64, Option<i64>, Vec<u8>);

pub(crate) fn initialize(
    connection: &Connection,
    header: &[u8],
    scope: &DraftScope,
) -> StorageResult<()> {
    connection.execute_batch(include_str!("../../sql/draft_schema.sql"))?;
    if connection.execute(
        "INSERT INTO session_owner VALUES(1,?1,NULL,0,0,0,NULL)",
        [header],
    )? != 1
        || connection.execute(
            "INSERT INTO draft_owner VALUES(1,?1,0,0,0,1,NULL)",
            [scope.as_bytes().as_slice()],
        )? != 1
    {
        return Err(StorageError::Integrity(
            "draft initialization affected count",
        ));
    }
    Ok(())
}
pub(crate) fn verify(connection: &Connection, drafts: &Drafts) -> StorageResult<()> {
    let row: (Vec<u8>, i64, i64, i64, i64, Option<Vec<u8>>) = connection.query_row(
        "SELECT scope,stage,records,bytes,next_job,selected FROM draft_owner WHERE id=1",
        [],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        },
    )?;
    let ledger = drafts.ledger;
    if row.0 != drafts.scope.as_bytes()
        || row.1 != i64::from(ledger.ended)
        || row.2 != ledger.records as i64
        || row.3 != ledger.bytes as i64
        || row.4 != ledger.next_job as i64
        || row.5.as_deref() != ledger.selected.as_ref().map(|id| id.as_bytes().as_slice())
    {
        return Err(StorageError::Integrity("draft exact owner readback"));
    }
    Ok(())
}
pub(crate) fn header(connection: &Connection, id: ObjectId) -> StorageResult<Option<Header>> {
    let row: Option<HeaderRow> = connection.query_row("SELECT form,role,stage,refs,preds,cursor,charge,queued,digest FROM draft_headers WHERE id=?1", [id.as_bytes().as_slice()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?))).optional()?;
    row.map(|row| {
        if !(0..=1).contains(&row.0)
            || !(3..=4).contains(&row.1)
            || !(0..=2).contains(&row.2)
            || !(0..=128).contains(&row.3)
            || !(0..=4).contains(&row.4)
            || !(0..=128).contains(&row.5)
            || row.6 <= 0
            || row.7.is_some_and(|value| value <= 0)
            || row.8.len() != 32
        {
            return Err(StorageError::Integrity("draft header fields"));
        }
        Ok(Header {
            id,
            form: row.0 as u8,
            role: row.1 as u8,
            stage: row.2 as u8,
            references: row.3 as u16,
            predecessors: row.4 as u8,
            cursor: row.5 as u16,
            charge: row.6 as usize,
            queued: row.7.map(|value| value as u64),
            digest: row.8.try_into().unwrap(),
        })
    })
    .transpose()
}
pub(crate) fn count(connection: &Connection, id: ObjectId) -> StorageResult<Option<u64>> {
    let row: Option<Vec<u8>> = connection
        .query_row(
            "SELECT links FROM draft_counts WHERE id=?1",
            [id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    row.map(|value| {
        let bytes: [u8; 8] = value
            .try_into()
            .map_err(|_| StorageError::Integrity("draft count width"))?;
        Ok(u64::from_be_bytes(bytes))
    })
    .transpose()
}
pub(crate) fn body(connection: &Connection, id: ObjectId) -> StorageResult<Vec<u8>> {
    let body: Vec<u8> = connection.query_row(
        "SELECT value FROM draft_bodies WHERE id=?1",
        [id.as_bytes().as_slice()],
        |row| row.get(0),
    )?;
    if body.len() > 8192 || body.capacity() > 8192 {
        return Err(StorageError::Integrity("draft body capacity"));
    }
    Ok(body)
}
pub(crate) fn references(
    connection: &Connection,
    id: ObjectId,
) -> StorageResult<Vec<(ObjectId, bool)>> {
    let mut statement = connection.prepare(
        "SELECT ordinal,value,linked FROM draft_references WHERE id=?1 ORDER BY ordinal LIMIT 129",
    )?;
    let mut rows = statement.query([id.as_bytes().as_slice()])?;
    let mut result = Vec::with_capacity(128);
    while let Some(row) = rows.next()? {
        if result.len() == 128 || row.get::<_, i64>(0)? != result.len() as i64 {
            return Err(StorageError::Integrity("draft reference ordinal"));
        }
        let linked: i64 = row.get(2)?;
        if !(0..=1).contains(&linked) {
            return Err(StorageError::Integrity("draft link flag"));
        }
        result.push((
            ObjectId::from_bytes(&row.get::<_, Vec<u8>>(1)?)?,
            linked == 1,
        ));
    }
    Ok(result)
}
pub(crate) fn predecessors(
    connection: &Connection,
    id: ObjectId,
) -> StorageResult<Vec<(ObjectId, u8)>> {
    let mut statement=connection.prepare("SELECT ordinal,value,provenance FROM draft_predecessors WHERE id=?1 ORDER BY ordinal LIMIT 5")?;
    let mut rows = statement.query([id.as_bytes().as_slice()])?;
    let mut result = Vec::with_capacity(4);
    while let Some(row) = rows.next()? {
        let provenance: i64 = row.get(2)?;
        if result.len() == 4
            || row.get::<_, i64>(0)? != result.len() as i64
            || !(0..=2).contains(&provenance)
        {
            return Err(StorageError::Integrity("draft predecessor ordinal"));
        }
        result.push((
            ObjectId::from_bytes(&row.get::<_, Vec<u8>>(1)?)?,
            provenance as u8,
        ));
    }
    Ok(result)
}
pub(crate) fn digest(
    scope: &DraftScope,
    id: ObjectId,
    form: u8,
    role: u8,
    body: &[u8],
    references: &[(ObjectId, bool)],
    predecessors: &[(ObjectId, u8)],
) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(b"layerfs/private-draft/v6\0");
    hash.update(&scope.as_bytes());
    hash.update(id.as_bytes());
    hash.update(&[form, role]);
    hash.update(&(body.len() as u64).to_be_bytes());
    hash.update(body);
    hash.update(&(references.len() as u16).to_be_bytes());
    for (id, linked) in references {
        hash.update(id.as_bytes());
        hash.update(&[u8::from(*linked)]);
    }
    hash.update(&[predecessors.len() as u8]);
    for (id, provenance) in predecessors {
        hash.update(id.as_bytes());
        hash.update(&[*provenance]);
    }
    *hash.finalize().as_bytes()
}
pub(crate) fn get(
    connection: &Connection,
    scope: &DraftScope,
    id: ObjectId,
) -> StorageResult<Option<DraftRecord>> {
    let Some(header) = header(connection, id)? else {
        return Ok(None);
    };
    if header.stage != 1 {
        return Err(StorageError::Integrity("draft body not Ready"));
    }
    let body = body(connection, id)?;
    let references = references(connection, id)?;
    let predecessors = predecessors(connection, id)?;
    if references.len() != header.references as usize
        || predecessors.len() != header.predecessors as usize
        || digest(
            scope,
            id,
            header.form,
            header.role,
            &body,
            &references,
            &predecessors,
        ) != header.digest
    {
        return Err(StorageError::Integrity("draft body checksum/totals"));
    }
    let record = DraftRecord::from_private(
        header.form,
        header.role,
        body,
        // Borrow the tuple window: consuming it can reuse its40-byte cells as
        // 32-byte IDs, yielding capacity160 from an admitted128-entry window.
        // This distinct bounded ID window preserves the actual capacity gate.
        references.iter().map(|(id, _)| *id).collect(),
        predecessors,
    )?;
    if record.charge() - 71 != header.charge
        || matches!(&record,DraftRecord::Page(object) if object.id()!=id)
    {
        return Err(StorageError::Content(ContentError::InvalidRecord(
            "draft body key/charge",
        )));
    }
    Ok(Some(record))
}
pub(crate) fn resolved(connection: &Connection, id: ObjectId) -> StorageResult<Option<ObjectId>> {
    let value: Option<Vec<u8>> = connection
        .query_row(
            "SELECT canonical FROM draft_committed WHERE id=?1",
            [id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    value
        .map(|value| ObjectId::from_bytes(&value).map_err(Into::into))
        .transpose()
}
pub(crate) fn emission(connection: &Connection, id: ObjectId) -> StorageResult<Option<bool>> {
    let value: Option<i64> = connection
        .query_row(
            "SELECT accepted FROM draft_emissions WHERE canonical=?1",
            [id.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    value
        .map(|value| match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(StorageError::Integrity("draft emission stage")),
        })
        .transpose()
}
