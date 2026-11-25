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
use postgres::Row;
use std::collections::HashMap;
use std::rc::Rc;

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

/// Packs one [`expect`] reads ahead at most, and the bytes of their rows it
/// asks for in one statement.
const AHEAD_PACKS: usize = 256;
const AHEAD_BYTES: i64 = 8 * 1024 * 1024;
/// Bytes of pack rows a transaction that announced its reads keeps, in two
/// generations: when the young one reaches half of this, the old one is
/// dropped.
const HELD_PACK_BYTES: usize = 32 * 1024 * 1024;

/// One pack row as a scoped read uses it: the descriptor and the head of the
/// body, copied out of the driver's row. The driver cuts its rows from the
/// buffers it received them in, so a kept row would keep the whole buffer.
pub(crate) struct HeldPack {
    info: PackInfo,
    head: Vec<u8>,
}

impl HeldPack {
    /// Checks one fetched row and copies what a scoped read needs of it.
    fn from_row(row: &Row) -> Result<Self, PersistenceError> {
        let info = info(&rows::record_first(row, 4)?)?;
        let head: Vec<u8> = row.try_get(4).map_err(|e| rows::error(&e))?;
        if head.len() != info.length.min(HEAD_BYTES as usize) {
            return Err(PersistenceError::Malformed);
        }
        Ok(Self { info, head })
    }

    /// The bytes this holds, with an allowance for the entry itself.
    fn bytes(&self) -> usize {
        self.head.len() + 128
    }
}

/// The pack rows a transaction holds for its scoped reads: those read ahead
/// ([`expect`]) and, once the caller has announced a walk, every row a
/// scoped read fetched.
///
/// A published pack never changes, so a held row is the row the server
/// would return again. A walk comes back to a pack whenever a later record
/// is a delta on one of its records; the row it already fetched then costs
/// no second round trip and no second transfer.
#[derive(Default)]
pub(crate) struct HeldPacks {
    young: HashMap<i64, Rc<HeldPack>>,
    old: HashMap<i64, Rc<HeldPack>>,
    young_bytes: usize,
    /// Whether the caller announced its reads: only then are rows kept.
    walk: bool,
}

impl HeldPacks {
    fn holds(&self, id: i64) -> bool {
        self.young.contains_key(&id) || self.old.contains_key(&id)
    }

    /// The held row of `id`; one found in the old generation joins the
    /// young one, so a pack in use outlives the next turn.
    fn get(&mut self, id: i64) -> Option<Rc<HeldPack>> {
        if let Some(pack) = self.young.get(&id) {
            return Some(pack.clone());
        }
        let pack = self.old.remove(&id)?;
        self.keep(id, pack.clone());
        Some(pack)
    }

    fn keep(&mut self, id: i64, pack: Rc<HeldPack>) {
        let bytes = pack.bytes();
        if self.young_bytes + bytes > HELD_PACK_BYTES / 2 {
            self.old = std::mem::take(&mut self.young);
            self.young_bytes = 0;
        }
        self.young_bytes += bytes;
        if let Some(replaced) = self.young.insert(id, pack) {
            self.young_bytes -= replaced.bytes();
        }
    }
}

/// Reads ahead, in one statement, the rows that scoped reads of `ids` would
/// each fetch on their own: the packs from the front of `ids` that are not
/// held yet, in order, as long as their rows together stay within
/// [`AHEAD_BYTES`].
///
/// A pack past the byte bound, or one this call was not told about, is read
/// by its own statement as before, so the hint changes round trips and
/// nothing else. Inside a transaction only: an autocommit statement carries
/// a generation fence this one lacks.
pub(crate) fn expect(tx: &Transaction<'_>, ids: &[i64]) -> Result<(), PersistenceError> {
    if ids.is_empty() || !tx.unfenced() {
        return Ok(());
    }
    let ids: Vec<i64> = {
        let mut held = tx.packs.borrow_mut();
        held.walk = true;
        ids.iter()
            .take(AHEAD_PACKS)
            .copied()
            .filter(|id| !held.holds(*id))
            .collect()
    };
    if ids.is_empty() {
        return Ok(());
    }
    // The running sum is over the bytes each row would carry, in the order
    // the packs were announced; a body is fetched only for a row it admits.
    let rows = tx.typed(
        &format!(
            "SELECT {HEAD_COLUMNS} FROM (SELECT q.*,sum(least(q.length,($2::int4)::bigint)) OVER (ORDER BY i.n) AS held \
             FROM unnest($1::bigint[]) WITH ORDINALITY AS i(pack_id,n) JOIN {}.pack q ON q.pack_id=i.pack_id) p \
             WHERE p.held <= $3::bigint",
            tx.schema
        ),
        &[&ids, &HEAD_BYTES, &AHEAD_BYTES],
        ids.len() as u64 * 8 + 12,
    )?;
    let mut held = tx.packs.borrow_mut();
    for row in &rows {
        // A row that does not check is left for the scoped read of its
        // pack, which fetches it again and refuses it there.
        if let Ok(pack) = HeldPack::from_row(row) {
            held.keep(pack.info.pack_id, Rc::new(pack));
        }
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
    // A held row of this pack is the row the statement below returns.
    let held = tx.packs.borrow_mut().get(id);
    let pack = match held {
        Some(pack) => pack,
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
            let row = found.pop().ok_or(PersistenceError::Missing)?;
            let pack = Rc::new(HeldPack::from_row(&row)?);
            let mut held = tx.packs.borrow_mut();
            if held.walk {
                held.keep(id, pack.clone());
            }
            pack
        }
    };
    let descriptor = pack.info;
    if descriptor.pack_id != id {
        return Err(PersistenceError::Malformed);
    }
    let head = &pack.head;
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

/// The widest gap between two wanted ranges of one pack that the server
/// reads through rather than skips. Each separate piece of a body costs the
/// server a descent of the body's index; the stored pieces between two
/// nearby ranges come from the pages that descent already reached. The bytes
/// of a gap stay in the server: only the wanted ranges are sent.
const SPAN_FILL: usize = 8 * 1024;

/// The bytes of selected ranges of several packs in one statement: for each
/// `(pack, ranges)` of `requests`, in order, the bytes of each range, in
/// order. A pack's ranges are sorted and disjoint. A pack that is absent is
/// `Missing`; a range that reaches past its body comes back short and is
/// `Malformed`.
///
/// The body is stored uncompressed out of line, so `substring` fetches only
/// the stored pieces it covers. Ranges of one pack no further apart than
/// [`SPAN_FILL`] are fetched as one span, once, and cut from it inside the
/// statement: a pack whose record headers are all wanted costs one fetch of
/// its body instead of one per header, and still returns only the headers.
pub(crate) fn ranges(
    tx: &Transaction<'_>,
    requests: &[(i64, Vec<PackRange>)],
    out: &mut Vec<Vec<Vec<u8>>>,
) -> Result<(), PersistenceError> {
    let total = requests.iter().map(|(_, ranges)| ranges.len()).sum();
    let int = |value: usize| i32::try_from(value).map_err(|_| PersistenceError::Malformed);
    // One row per span: its pack, 1-based start, length, and the 1-based
    // position and count of its ranges in the two range arrays.
    let (mut ids, mut starts, mut lengths) = (Vec::new(), Vec::new(), Vec::new());
    let (mut firsts, mut counts) = (Vec::new(), Vec::new());
    // One row per range: its 1-based start inside its span, and its length.
    let mut from = Vec::with_capacity(total);
    let mut count = Vec::with_capacity(total);
    for (id, ranges) in requests {
        if *id <= 0 {
            return Err(PersistenceError::Malformed);
        }
        // The open span of this pack: its start and end offsets.
        let mut span: Option<(usize, usize)> = None;
        let mut end = 0;
        for range in ranges {
            let stop = range
                .offset
                .checked_add(range.length)
                .filter(|_| range.length > 0 && range.offset >= end)
                .ok_or(PersistenceError::Malformed)?;
            end = stop;
            let start = match span {
                Some((start, reach)) if range.offset <= reach + SPAN_FILL => start,
                _ => {
                    ids.push(*id);
                    starts.push(int(range.offset + 1)?);
                    lengths.push(0);
                    firsts.push(int(from.len() + 1)?);
                    counts.push(0);
                    range.offset
                }
            };
            span = Some((start, stop));
            *lengths.last_mut().ok_or(PersistenceError::Malformed)? = int(stop - start)?;
            *counts.last_mut().ok_or(PersistenceError::Malformed)? += 1;
            from.push(int(range.offset - start + 1)?);
            count.push(int(range.length)?);
        }
    }
    let rows = if total == 0 {
        Vec::new()
    } else {
        tx.typed(
            &ranges_sql(tx.schema),
            &[&ids, &starts, &lengths, &firsts, &counts, &from, &count],
            ids.len() as u64 * 24 + total as u64 * 8,
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

/// The multi-range reader's text ([`ranges`]): `OFFSET 0` keeps the span a
/// value computed once per span row; the ranges are then cut from that value
/// in memory.
pub(crate) fn ranges_sql(schema: &str) -> String {
    format!(
        "SELECT substring(s.span FROM ($6::int4[])[k] FOR ($7::int4[])[k]) \
         FROM unnest($1::bigint[],$2::int4[],$3::int4[],$4::int4[],$5::int4[]) AS g(pack_id,o,l,first,n) \
         JOIN {schema}.pack p ON p.pack_id=g.pack_id \
         CROSS JOIN LATERAL (SELECT substring(p.body FROM g.o FOR g.l) AS span OFFSET 0) s \
         CROSS JOIN LATERAL generate_series(g.first,g.first+g.n-1) AS k ORDER BY k"
    )
}
