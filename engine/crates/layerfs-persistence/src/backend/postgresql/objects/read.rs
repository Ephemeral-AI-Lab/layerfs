//! Whole and ranged pack reads; read budgets are checked before any body.
use super::super::session::{rows, transaction::Transaction};
use crate::backend::records::{pack_info, BackendError, Param, Record};
use layerfs_storage::{
    location::PackInfo,
    policy,
    port::{
        AcquiredPackRead, PackRange, PackReadPlan, PersistedPack, PersistedPackRead,
        PersistenceError,
    },
};

fn info(r: &Record) -> Result<PackInfo, BackendError> {
    pack_info(r.get(0)?, r.get(1)?, &r.get::<Vec<u8>>(2)?, r.get(3)?)
}

pub(crate) fn read(
    tx: &Transaction<'_>,
    ids: &[i64],
    out: &mut Vec<PersistedPack>,
) -> Result<(), PersistenceError> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let budget = policy::DEPENDENCY_PACK_CACHE_BYTES as i64;
    let singleton = policy::SINGLETON_PACK_LIMIT as i64;
    // Descriptors and bodies in one statement. The server sends the bodies
    // only when every id is present and their total is within the read
    // budget, so the budget is still checked before any body is read.
    let found = tx.typed(
        &format!(
            "WITH d AS (SELECT count(*) AS n, coalesce(sum(length),0) AS total FROM {s}.pack WHERE pack_id = ANY($1::bigint[])) \
             SELECT p.pack_id,p.domain,CASE WHEN octet_length(p.digest)=32 THEN p.digest END,p.length,\
             CASE WHEN d.n = cardinality($1::bigint[]) AND (d.total <= $2::bigint OR (d.n = 1 AND d.total <= $3::bigint)) AND octet_length(p.body)=p.length AND p.length BETWEEN 32 AND $3::bigint THEN p.body END \
             FROM {s}.pack p, d WHERE p.pack_id = ANY($1::bigint[]) ORDER BY p.pack_id",
            s = tx.schema
        ),
        &[&ids, &budget, &singleton],
        ids.len() as u64 * 8 + 16,
    )?;
    if found.len() != ids.len() {
        return Err(PersistenceError::Missing);
    }
    let mut infos = Vec::with_capacity(found.len());
    let mut total = 0usize;
    for row in &found {
        let descriptor = info(&rows::record_first(row, 4)?)?;
        total = total
            .checked_add(descriptor.length)
            .ok_or(BackendError::Capacity)?;
        infos.push(descriptor);
    }
    if total > policy::DEPENDENCY_PACK_CACHE_BYTES
        && !(ids.len() == 1 && total <= policy::SINGLETON_PACK_LIMIT)
    {
        return Err(BackendError::Capacity.into());
    }
    for (info, row) in infos.into_iter().zip(found) {
        let body: Option<Vec<u8>> = row.try_get(4).map_err(|e| rows::error(&e))?;
        out.push(PersistedPack::authenticate(
            info,
            body.ok_or(PersistenceError::Malformed)?,
        )?);
    }
    Ok(())
}

pub(crate) fn selection(
    tx: &Transaction<'_>,
    id: i64,
    plan: &mut dyn PackReadPlan,
) -> Result<PersistedPackRead, PersistenceError> {
    read_with(tx, id, |descriptor, read_at| {
        PersistedPackRead::acquire(descriptor, plan, read_at)
    })
}

pub(crate) fn scoped(
    tx: &Transaction<'_>,
    id: i64,
    plan: &mut dyn PackReadPlan,
) -> Result<AcquiredPackRead, PersistenceError> {
    read_with(tx, id, |descriptor, read_at| {
        AcquiredPackRead::acquire(descriptor, plan, read_at)
    })
}

type OffsetRead<'a> = dyn FnMut(usize, &mut [u8]) -> Result<(), PersistenceError> + 'a;

/// Bytes from the start of a body fetched with its descriptor: the header
/// and directory reads that open every acquisition then cost no round trip,
/// and a pack up to the pack limit, which this provider takes whole
/// ([`whole_pack_bytes`](layerfs_storage::port::PackPersistence::whole_pack_bytes)),
/// arrives complete in that one statement.
const HEAD_BYTES: i32 = policy::PACK_LIMIT as i32;

/// A pack's descriptor and the head of its body, `$2` bytes long.
const HEAD_COLUMNS: &str = "p.pack_id,p.domain,CASE WHEN octet_length(p.digest)=32 THEN p.digest END,p.length,CASE WHEN octet_length(p.body)=p.length THEN substring(p.body FROM 1 FOR $2) END";

/// Packs one [`expect`] reads ahead, and the longest body it takes: the rows
/// held between two calls stay within their product, 8 MiB.
const AHEAD_PACKS: usize = 256;
const AHEAD_BYTES: i64 = 32 * 1024;

/// Reads ahead, in one statement, the rows that scoped reads of `ids` would
/// each fetch on their own, for the packs short enough to arrive whole.
///
/// The rows replace any still held from the call before. A longer pack, or
/// one this call was not told about, is read by its own statement as before,
/// so the hint changes round trips and nothing else. Inside a transaction
/// only: an autocommit statement carries a generation fence this one lacks.
pub(crate) fn expect(tx: &Transaction<'_>, ids: &[i64]) -> Result<(), PersistenceError> {
    tx.ahead.borrow_mut().clear();
    if ids.is_empty() || !tx.unfenced() {
        return Ok(());
    }
    let ids = &ids[..ids.len().min(AHEAD_PACKS)];
    let rows = tx.typed(
        &format!(
            "SELECT {HEAD_COLUMNS} FROM {}.pack p WHERE p.pack_id = ANY($1::bigint[]) AND p.length <= $3::bigint",
            tx.schema
        ),
        &[&ids, &HEAD_BYTES, &AHEAD_BYTES],
        ids.len() as u64 * 8 + 12,
    )?;
    let mut ahead = tx.ahead.borrow_mut();
    for row in rows {
        let id: i64 = row.try_get(0).map_err(|e| rows::error(&e))?;
        ahead.insert(id, row);
    }
    Ok(())
}

/// Exact ranges of one body: the body is stored uncompressed out of line
/// (`STORAGE EXTERNAL`), so `substring` fetches only the chunks it covers.
/// The descriptor and the body's head come in one statement; a read the
/// head covers is copied from it, any other is one ranged statement.
fn read_with<T>(
    tx: &Transaction<'_>,
    id: i64,
    acquire: impl FnOnce(PackInfo, &mut OffsetRead<'_>) -> Result<T, PersistenceError>,
) -> Result<T, PersistenceError> {
    if id <= 0 {
        return Err(PersistenceError::Malformed);
    }
    // A row read ahead for this pack is the row the statement below returns.
    let ahead = tx.ahead.borrow_mut().remove(&id);
    let row = match ahead {
        Some(row) => row,
        None => {
            let mut found = tx.typed(
                &format!(
                    "SELECT {HEAD_COLUMNS} FROM {}.pack p WHERE p.pack_id=$1",
                    tx.schema
                ),
                &[&id, &HEAD_BYTES],
                12,
            )?;
            if found.len() > 1 {
                return Err(PersistenceError::Malformed);
            }
            found.pop().ok_or(PersistenceError::Missing)?
        }
    };
    let descriptor = info(&rows::record_first(&row, 4)?)?;
    if descriptor.pack_id != id {
        return Err(PersistenceError::Malformed);
    }
    let head: Vec<u8> = row.try_get(4).map_err(|e| rows::error(&e))?;
    if head.len() != descriptor.length.min(HEAD_BYTES as usize) {
        return Err(PersistenceError::Malformed);
    }
    let ranged = format!(
        "SELECT substring(body FROM $2 FOR $3) FROM {}.pack WHERE pack_id=$1",
        tx.schema
    );
    acquire(descriptor, &mut |offset, bytes| {
        if let Some(held) = offset
            .checked_add(bytes.len())
            .and_then(|end| head.get(offset..end))
        {
            bytes.copy_from_slice(held);
            return Ok(());
        }
        let from = i32::try_from(offset + 1).map_err(|_| PersistenceError::Malformed)?;
        let count = i32::try_from(bytes.len()).map_err(|_| PersistenceError::Malformed)?;
        let rows = tx.typed(&ranged, &[&id, &from, &count], 16)?;
        let slice: Vec<u8> = rows
            .first()
            .ok_or(PersistenceError::Missing)?
            .try_get(0)
            .map_err(|e| PersistenceError::from(rows::error(&e)))?;
        if slice.len() != bytes.len() {
            return Err(PersistenceError::Malformed);
        }
        bytes.copy_from_slice(&slice);
        Ok(())
    })
}

/// The bounded control/directory prefix of one pack and its declared length.
///
/// `substring` on an externally stored bytea fetches only the requested range,
/// so a metadata-only pack scan never reads the bodies behind the directory.
pub(crate) fn prefix(
    tx: &Transaction<'_>,
    id: i64,
    limit: usize,
) -> Result<Option<(usize, Vec<u8>)>, PersistenceError> {
    if id <= 0 || limit == 0 {
        return Err(PersistenceError::Malformed);
    }
    let limit = i32::try_from(limit).map_err(|_| PersistenceError::Malformed)?;
    let rows = tx.query(
        &format!(
            "SELECT length,substring(body FROM 1 FOR $2) FROM {}.pack WHERE pack_id=$1",
            tx.schema
        ),
        vec![Param::I64(id), Param::I64(i64::from(limit))],
    )?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let length = row.get::<i64>(0).map_err(|_| PersistenceError::Malformed)?;
    let body = row
        .get::<Vec<u8>>(1)
        .map_err(|_| PersistenceError::Malformed)?;
    let length = usize::try_from(length).map_err(|_| PersistenceError::Malformed)?;
    if body.len() != length.min(limit as usize) {
        return Err(PersistenceError::Malformed);
    }
    Ok(Some((length, body)))
}

/// [`prefix`] for several packs in one statement, in `ids` order. A pack that
/// is absent is an error.
pub(crate) fn prefixes(
    tx: &Transaction<'_>,
    ids: &[i64],
    limit: usize,
    out: &mut Vec<(usize, Vec<u8>)>,
) -> Result<(), PersistenceError> {
    if limit == 0 || ids.iter().any(|id| *id <= 0) {
        return Err(PersistenceError::Malformed);
    }
    if ids.is_empty() {
        return Ok(());
    }
    let limit = i32::try_from(limit).map_err(|_| PersistenceError::Malformed)?;
    let ids = ids.to_vec();
    let rows = tx.typed(
        &format!(
            "SELECT p.length,substring(p.body FROM 1 FOR $2) \
             FROM unnest($1::bigint[]) WITH ORDINALITY AS i(pack_id,n) \
             JOIN {}.pack p ON p.pack_id=i.pack_id ORDER BY i.n",
            tx.schema
        ),
        &[&ids, &limit],
        ids.len() as u64 * 8 + 4,
    )?;
    if rows.len() != ids.len() {
        return Err(PersistenceError::Missing);
    }
    for row in &rows {
        let length: i64 = row.try_get(0).map_err(|e| rows::error(&e))?;
        let body: Vec<u8> = row.try_get(1).map_err(|e| rows::error(&e))?;
        let length = usize::try_from(length).map_err(|_| PersistenceError::Malformed)?;
        if body.len() != length.min(limit as usize) {
            return Err(PersistenceError::Malformed);
        }
        out.push((length, body));
    }
    Ok(())
}

/// The bytes of selected ranges of several packs in one statement: for each
/// `(pack, ranges)` of `requests`, in order, the bytes of each range, in
/// order. A pack's ranges are sorted and disjoint. The body is stored
/// uncompressed out of line, so each `substring` fetches only the chunks it
/// covers. A pack that is absent is `Missing`; a range that reaches past its
/// body comes back short and is `Malformed`.
pub(crate) fn ranges(
    tx: &Transaction<'_>,
    requests: &[(i64, Vec<PackRange>)],
    out: &mut Vec<Vec<Vec<u8>>>,
) -> Result<(), PersistenceError> {
    let total = requests.iter().map(|(_, ranges)| ranges.len()).sum();
    let mut ids = Vec::with_capacity(total);
    let mut from = Vec::with_capacity(total);
    let mut count = Vec::with_capacity(total);
    for (id, ranges) in requests {
        if *id <= 0 {
            return Err(PersistenceError::Malformed);
        }
        let mut end = 0;
        for range in ranges {
            let stop = range
                .offset
                .checked_add(range.length)
                .filter(|_| range.length > 0 && range.offset >= end)
                .ok_or(PersistenceError::Malformed)?;
            end = stop;
            ids.push(*id);
            from.push(i32::try_from(range.offset + 1).map_err(|_| PersistenceError::Malformed)?);
            count.push(i32::try_from(range.length).map_err(|_| PersistenceError::Malformed)?);
        }
    }
    let rows = if total == 0 {
        Vec::new()
    } else {
        tx.typed(
            &format!(
                "SELECT substring(p.body FROM r.o FOR r.l) \
                 FROM unnest($1::bigint[],$2::int4[],$3::int4[]) WITH ORDINALITY AS r(pack_id,o,l,n) \
                 JOIN {}.pack p ON p.pack_id=r.pack_id ORDER BY r.n",
                tx.schema
            ),
            &[&ids, &from, &count],
            total as u64 * 16,
        )?
    };
    if rows.len() != total {
        return Err(PersistenceError::Missing);
    }
    let mut rows = rows.iter();
    for (_, ranges) in requests {
        let mut bodies = Vec::with_capacity(ranges.len());
        for (range, row) in ranges.iter().zip(rows.by_ref()) {
            let body: Vec<u8> = row.try_get(0).map_err(|e| rows::error(&e))?;
            if body.len() != range.length {
                return Err(PersistenceError::Malformed);
            }
            bodies.push(body);
        }
        out.push(bodies);
    }
    Ok(())
}
