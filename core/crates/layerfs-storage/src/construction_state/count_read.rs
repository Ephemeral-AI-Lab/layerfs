//! Indexed exact bounded immutable count and carried-zero pages.
use super::{count_index, ScratchSession};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    BaseFact, CanonicalScope, CountEpoch, CountPage, CountRecord, CountSeal, ZeroPage, ZeroSeal,
    COUNT_PAGE_HEADER_BYTES, ZERO_PAGE_HEADER_BYTES,
};
use rusqlite::Connection;
fn window(
    records: usize,
    bytes: usize,
    header: usize,
    width: usize,
    empty: bool,
) -> StorageResult<usize> {
    if records == 0
        || records > 128
        || bytes > 65536
        || bytes < header
        || (!empty && bytes - header < width)
    {
        return Err(StorageError::Integrity("canonical count page window"));
    }
    Ok(records.min((bytes - header) / width))
}
// The sealed cursor bounds and decode/consume callbacks remain explicit without added owners.
#[allow(clippy::too_many_arguments)]
pub(crate) fn visit<T>(
    connection: &Connection,
    table: &str,
    scope: &CanonicalScope,
    after: Option<u64>,
    maximum: Option<u64>,
    count: usize,
    decode: impl Fn(&[u8], &[u8]) -> StorageResult<T>,
    mut consume: impl FnMut(T) -> StorageResult<()>,
) -> StorageResult<(usize, Option<u64>, bool)> {
    let mut end = connection.prepare(&format!(
        "SELECT key FROM {table} ORDER BY key DESC LIMIT 1"
    ))?;
    let mut maximum_rows = end.query([])?;
    let actual = maximum_rows
        .next()?
        .map(|r| -> StorageResult<u64> { Ok(scope.scalar(count_index::blob(r.get_ref(0)?, 25)?)?) })
        .transpose()?;
    if actual != maximum || after.is_some_and(|after| maximum.is_none_or(|m| after > m)) {
        return Err(StorageError::Integrity("count sealed maximum/cursor"));
    }
    if maximum.is_none() || after == maximum {
        return Ok((0, after, true));
    }
    let lower = after.map(|s| scope.key(s)).transpose()?;
    let upper = scope.key(maximum.unwrap())?;
    let mut statement = connection.prepare(&format!(
        "SELECT key,value FROM {table} WHERE key>?1 AND key<=?2 ORDER BY key LIMIT ?3"
    ))?;
    let mut rows = statement.query(rusqlite::params![
        lower.as_ref().map_or(&[][..], |v| v.as_slice()),
        upper.as_slice(),
        count as i64
    ])?;
    let mut last = after;
    let mut read = 0;
    while let Some(row) = rows.next()? {
        let key = count_index::blob(row.get_ref(0)?, 25)?;
        let serial = scope.scalar(key)?;
        if last.is_some_and(|old| old >= serial) {
            return Err(StorageError::Integrity("count page advance"));
        }
        let value = match row.get_ref(1)? {
            rusqlite::types::ValueRef::Blob(v) => v,
            _ => return Err(StorageError::Integrity("count page value")),
        };
        consume(decode(key, value)?)?;
        read += 1;
        last = Some(serial);
    }
    let eof = last == maximum;
    if read == 0 && !eof {
        return Err(StorageError::Integrity("count page nonterminal empty"));
    }
    Ok((read, last, eof))
}
pub(crate) fn read<T>(
    connection: &Connection,
    table: &str,
    scope: &CanonicalScope,
    after: Option<u64>,
    maximum: Option<u64>,
    count: usize,
    decode: impl Fn(&[u8], &[u8]) -> StorageResult<T>,
) -> StorageResult<(Vec<T>, Option<u64>, bool)> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| StorageError::Integrity("count page allocation"))?;
    if values.capacity() != count {
        return Err(StorageError::Integrity("count page actual capacity"));
    }
    let (_, last, eof) = visit(
        connection,
        table,
        scope,
        after,
        maximum,
        count,
        decode,
        |row| {
            values.push(row);
            Ok(())
        },
    )?;
    Ok((values, last, eof))
}
impl ScratchSession {
    /// Read one advancing immutable Count page under this exact sealed epoch.
    /// The header-inclusive window permits1..=128 records and at most64KiB.
    /// Its actual Vec/owner credit stays with the returned page until drop.
    pub fn count_page(
        &mut self,
        seal: &CountSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> StorageResult<CountPage> {
        self.count_context(&seal.scope)?;
        self.finish((|| {
            let r = self.count_context(&seal.scope)?;
            let state = r.counts.as_ref().unwrap();
            let exact = match seal.epoch {
                CountEpoch::Effects => {
                    state.snapshot.stage == 2 && state.snapshot.effects.as_ref() == Some(seal)
                }
                CountEpoch::Final => {
                    state.snapshot.stage == 4 && state.snapshot.final_seal.as_ref() == Some(seal)
                }
            };
            if !exact {
                return Err(StorageError::Integrity("count stale sealed epoch"));
            }
            let empty = seal.maximum.is_none() || after == seal.maximum;
            let count = window(records, bytes, COUNT_PAGE_HEADER_BYTES, 128, empty)?;
            let memory = state.memory.reserve(
                std::mem::size_of::<CountPage>() + count * std::mem::size_of::<CountRecord>(),
            )?;
            let (values, last, eof) = read(
                r.verify()?,
                "canonical_counts",
                &seal.scope,
                after,
                seal.maximum,
                count,
                |key, value| Ok(CountRecord::decode(&seal.scope, key, value)?),
            )?;
            Ok(CountPage::new(memory, seal.clone(), values, last, eof)?)
        })())
    }
    /// Read one advancing carried-base page under this exact known zero epoch.
    /// The header-inclusive window permits1..=128 records and at most64KiB.
    /// Its actual Vec/owner credit stays with the returned page until drop.
    pub fn zero_page(
        &mut self,
        seal: &ZeroSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> StorageResult<ZeroPage> {
        self.count_context(&seal.counts.scope)?;
        self.finish((|| {
            let r = self.count_context(&seal.counts.scope)?;
            let state = r.counts.as_ref().unwrap();
            if !matches!(state.snapshot.stage, 2..=4)
                || state.snapshot.seeds.as_ref() != Some(seal)
                || seal.scope != seal.counts.scope.zeros()?
            {
                return Err(StorageError::Integrity("zero selected sealed epoch"));
            }
            let empty = seal.maximum.is_none() || after == seal.maximum;
            let count = window(records, bytes, ZERO_PAGE_HEADER_BYTES, 105, empty)?;
            let memory = state.memory.reserve(
                std::mem::size_of::<ZeroPage>() + count * std::mem::size_of::<BaseFact>(),
            )?;
            let (values, last, eof) = read(
                r.verify()?,
                "zero_seeds",
                &seal.scope,
                after,
                seal.maximum,
                count,
                |key, value| Ok(BaseFact::decode(seal.scope.scalar(key)?, value)?),
            )?;
            Ok(ZeroPage::new(memory, seal.clone(), values, last, eof)?)
        })())
    }
}
