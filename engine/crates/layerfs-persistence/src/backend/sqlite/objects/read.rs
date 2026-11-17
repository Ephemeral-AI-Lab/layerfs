//! Read budgets are checked before any BLOB is acquired into the result buffer.
use crate::backend::sqlite::session::rows;
use crate::{
    backend::metadata_locations::info,
    backend::{
        records::{BackendError, Param},
        Transaction,
    },
};
use layerfs_storage::{policy, port::*};
use std::collections::BTreeSet;
use std::time::Instant;
pub(crate) fn read(
    tx: &Transaction<'_>,
    ids: &[i64],
    out: &mut Vec<PersistedPack>,
) -> Result<(), PersistenceError> {
    if ids.len() > policy::READ_OBJECT_LIMIT || ids.iter().any(|id| *id <= 0) {
        return Err(PersistenceError::Malformed);
    }
    let ids: Vec<_> = ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut descriptors = Vec::new();
    let mut total = 0usize;
    for page in ids.chunks(tx.input_limit(1, 128, 2)?) {
        let sql = format!(
            "SELECT pack_id,domain,CASE WHEN (CASE WHEN typeof(digest)='blob' THEN length(CAST(digest AS BLOB)) ELSE NULL END)=32 THEN digest END,length FROM pack WHERE pack_id IN({}) ORDER BY pack_id",
            std::iter::repeat_n("?", page.len())
                .collect::<Vec<_>>()
                .join(",")
        );
        for r in tx.query(&sql, page.iter().map(|id| Param::I64(*id)).collect())? {
            let descriptor = info(&r, 0)?;
            total = total
                .checked_add(descriptor.length)
                .ok_or(BackendError::Capacity)?;
            descriptors.push(descriptor);
        }
    }
    if descriptors.len() != ids.len() {
        return Err(PersistenceError::Missing);
    }
    if total > policy::DEPENDENCY_PACK_CACHE_BYTES
        && !(ids.len() == 1 && total <= policy::SINGLETON_PACK_LIMIT)
    {
        return Err(BackendError::Capacity.into());
    }
    for info in descriptors {
        if tx.layout().uses_units() {
            out.push(super::units_read::whole(tx, info)?);
            continue;
        }
        let mut rows = tx.query(
            "SELECT CASE WHEN (CASE WHEN typeof(body)='blob' THEN length(CAST(body AS BLOB)) ELSE NULL END)=?2 THEN body END FROM pack WHERE pack_id=?1",
            vec![Param::I64(info.pack_id), Param::I64(info.length as i64)],
        )?;
        let body: Vec<u8> = rows
            .first_mut()
            .ok_or(PersistenceError::Missing)?
            .take_bytes(0)?;
        out.push(PersistedPack::authenticate(info, body)?);
    }
    Ok(())
}

/// The bounded control/directory prefix of one pack and its declared length.
///
/// The pack directory sits at the head of the body, so a metadata-only scan
/// reads at most `limit` bytes instead of the rest of the pack. A pack shorter
/// than `limit` comes whole; the declared length is returned separately so the
/// caller can still parse the header against it.
pub(crate) fn prefix(
    tx: &Transaction<'_>,
    id: i64,
    limit: usize,
) -> Result<Option<(usize, Vec<u8>)>, PersistenceError> {
    if id <= 0 || limit == 0 {
        return Err(PersistenceError::Malformed);
    }
    if tx.layout().uses_units() {
        // A unit-layout pack keeps its header and directory in `control`; that
        // column is exactly the bounded prefix, so no body range is read.
        let mut rows = tx.query(
            "SELECT length,control FROM pack WHERE pack_id=?",
            vec![Param::I64(id)],
        )?;
        let Some(row) = rows.pop() else {
            return Ok(None);
        };
        let length = row.get::<i64>(0).map_err(|_| PersistenceError::Malformed)?;
        let control = row
            .get::<Vec<u8>>(1)
            .map_err(|_| PersistenceError::Malformed)?;
        let length = usize::try_from(length).map_err(|_| PersistenceError::Malformed)?;
        if control.len() > limit {
            return Err(PersistenceError::Malformed);
        }
        return Ok(Some((length, control)));
    }
    // Incremental BLOB I/O, not `substr(body,1,?)`: SQLite loads a column's
    // whole overflow chain before a function sees it, so that statement read
    // every pack body to return its first bytes.
    let mut rows = tx.query(
        "SELECT length FROM pack WHERE pack_id=?",
        vec![Param::I64(id)],
    )?;
    let Some(row) = rows.pop() else {
        return Ok(None);
    };
    let length = row.get::<i64>(0).map_err(|_| PersistenceError::Malformed)?;
    let length = usize::try_from(length).map_err(|_| PersistenceError::Malformed)?;
    tx.work.borrow_mut().blob_open_calls += 1;
    let blob = tx
        .connection
        .blob_open("main", "pack", "body", id, true)
        .map_err(rows::error)?;
    let read = if blob.len() != length {
        Err(PersistenceError::Malformed)
    } else {
        head(tx, &blob, limit)
    };
    closed(tx, blob, read).map(|body| Some((length, body)))
}

/// The first `limit` bytes of the body `blob` is open on.
fn head(
    tx: &Transaction<'_>,
    blob: &rusqlite::blob::Blob<'_>,
    limit: usize,
) -> Result<Vec<u8>, PersistenceError> {
    let mut body = vec![0; blob.len().min(limit)];
    {
        let mut work = tx.work.borrow_mut();
        work.blob_read_calls += 1;
        work.blob_requested_bytes += body.len() as u64;
    }
    let start = Instant::now();
    let read = blob.read_at_exact(&mut body, 0).map_err(rows::error);
    let mut work = tx.work.borrow_mut();
    work.blob_read_ns += start.elapsed().as_nanos() as u64;
    read?;
    work.blob_read_bytes += body.len() as u64;
    Ok(body)
}

/// Closes `blob` and keeps an unproven outcome ahead of any other failure.
fn closed<T>(
    tx: &Transaction<'_>,
    blob: rusqlite::blob::Blob<'_>,
    read: Result<T, PersistenceError>,
) -> Result<T, PersistenceError> {
    tx.work.borrow_mut().blob_close_calls += 1;
    let close: Result<(), PersistenceError> = blob.close().map_err(rows::error).map_err(Into::into);
    match (read, close) {
        (Err(PersistenceError::Uncertain), _) | (_, Err(PersistenceError::Uncertain)) => {
            Err(PersistenceError::Uncertain)
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}

/// The bounded prefixes of several packs, in `ids` order, through one BLOB
/// handle moved from row to row: no statement and no handle per pack.
pub(crate) fn prefixes(
    tx: &Transaction<'_>,
    ids: &[i64],
    limit: usize,
    out: &mut Vec<(usize, Vec<u8>)>,
) -> Result<(), PersistenceError> {
    if limit == 0 || ids.iter().any(|id| *id <= 0) {
        return Err(PersistenceError::Malformed);
    }
    let Some(first) = ids.first() else {
        return Ok(());
    };
    if tx.layout().uses_units() {
        for id in ids {
            out.push(prefix(tx, *id, limit)?.ok_or(PersistenceError::Missing)?);
        }
        return Ok(());
    }
    tx.work.borrow_mut().blob_open_calls += 1;
    let mut blob = tx
        .connection
        .blob_open("main", "pack", "body", *first, true)
        .map_err(rows::error)?;
    let read = (|| {
        for (index, id) in ids.iter().enumerate() {
            if index > 0 {
                tx.work.borrow_mut().blob_reopen_calls += 1;
                blob.reopen(*id).map_err(rows::error)?;
            }
            out.push((blob.len(), head(tx, &blob, limit)?));
        }
        Ok(())
    })();
    closed(tx, blob, read)
}
