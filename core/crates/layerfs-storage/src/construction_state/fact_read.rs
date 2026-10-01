//! Leased bounded selected pages; lengths are checked before value allocation.
use super::{fact_index, ScratchSession};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    BaseFact, FactPage, FactScope, FactSeal, ParentFact, ParentSeal, BASE_FACT_BYTES,
    FACT_PAGE_HEADER_BYTES, PARENT_ELIGIBILITY_BYTES,
};

impl ScratchSession {
    /// One exact bounded sealed BaseFacts page with consumer-held credit.
    pub fn fact_page(
        &mut self,
        seal: &FactSeal,
        after: Option<u64>,
        count: usize,
        bytes: usize,
    ) -> StorageResult<FactPage<BaseFact>> {
        self.fact_selected(&seal.scope)?;
        let result = (|| {
            let r = self.resource.as_ref().unwrap();
            let owner = r.facts.as_ref().unwrap();
            if owner.base_seal.as_ref() != Some(seal) || owner.base.stage != 2 {
                return Err(StorageError::Integrity("base fact selected seal"));
            }
            read_page(
                r.verify()?,
                &seal.scope,
                seal,
                after,
                count,
                bytes,
                BASE_FACT_BYTES,
                owner.memory.clone(),
                |row, scope| {
                    let serial = scope.serial(fact_index::blob(row.get_ref(0)?, 25)?)?;
                    Ok(BaseFact::decode(
                        serial,
                        fact_index::blob(row.get_ref(1)?, 74)?,
                    )?)
                },
            )
        })();
        self.finish(result)
    }
    /// One exact bounded sealed ParentEligibility page with consumer-held credit.
    pub fn parent_page(
        &mut self,
        seal: &ParentSeal,
        after: Option<u64>,
        count: usize,
        bytes: usize,
    ) -> StorageResult<FactPage<ParentFact>> {
        self.fact_selected(&seal.facts.scope)?;
        let result = (|| {
            let r = self.resource.as_ref().unwrap();
            let owner = r.facts.as_ref().unwrap();
            if owner.parent_seal.as_ref() != Some(seal) || owner.parent.stage != 3 {
                return Err(StorageError::Integrity("parent selected seal"));
            }
            read_page(
                r.verify()?,
                &seal.facts.scope,
                &seal.facts,
                after,
                count,
                bytes,
                PARENT_ELIGIBILITY_BYTES,
                owner.memory.clone(),
                |row, scope| {
                    let serial = scope.serial(fact_index::blob(row.get_ref(0)?, 25)?)?;
                    let bound: u8 = row.get(1)?;
                    if bound > 1 {
                        return Err(StorageError::Integrity("parent bound framing"));
                    }
                    Ok(ParentFact {
                        serial,
                        bound: bound == 1,
                    })
                },
            )
        })();
        self.finish(result)
    }
    /// Exact selected sealed parent row; unknown is outside the declaration set.
    pub fn parent_get(
        &mut self,
        seal: &ParentSeal,
        serial: u64,
    ) -> StorageResult<Option<ParentFact>> {
        self.fact_selected(&seal.facts.scope)?;
        self.finish((|| {
            let r = self.resource.as_ref().unwrap();
            let owner = r.facts.as_ref().unwrap();
            if owner.parent_seal.as_ref() != Some(seal) || owner.parent.stage != 3 {
                return Err(StorageError::Integrity("parent point selected seal"));
            }
            fact_index::parent(r.verify()?, &seal.facts.scope, serial)
        })())
    }
}
// Scope, seal, window and lease are checked together before any page allocation.
#[allow(clippy::too_many_arguments)]
fn read_page<T>(
    connection: &rusqlite::Connection,
    scope: &FactScope,
    seal: &FactSeal,
    after: Option<u64>,
    count: usize,
    bytes: usize,
    width: u64,
    memory: layerfs_content::filesystem::state::GraphMemory,
    decode: impl Fn(&rusqlite::Row<'_>, &FactScope) -> StorageResult<T>,
) -> StorageResult<FactPage<T>> {
    if count == 0 || count > 128 || !(FACT_PAGE_HEADER_BYTES..=65536).contains(&bytes) {
        return Err(StorageError::Integrity("fact page limit"));
    }
    if after == seal.maximum {
        let sql = format!(
            "SELECT NOT EXISTS(SELECT 1 FROM {} WHERE key>?1 LIMIT 1)",
            fact_index::table(scope.state().table().code())
        );
        let lower = after.map(|a| scope.key(a)).transpose()?;
        let empty: bool = connection.query_row(
            &sql,
            [lower.as_ref().map_or(&[][..], |k| k.as_slice())],
            |r| r.get(0),
        )?;
        if !empty {
            return Err(StorageError::Integrity("fact terminal extra record"));
        }
        let lease = memory.reserve(std::mem::size_of::<FactPage<T>>())?;
        return Ok(FactPage::new(lease, seal.clone(), Vec::new(), after, true)?);
    }
    let fits = (bytes - FACT_PAGE_HEADER_BYTES) / (width as usize);
    let count = count.min(fits);
    if count == 0 {
        return Err(StorageError::CapacityExceeded {
            what: "fact first record page",
            limit: bytes as u64,
            actual: FACT_PAGE_HEADER_BYTES as u64 + width,
        });
    }
    if after.is_some_and(|a| seal.maximum.is_none_or(|m| a > m)) {
        return Err(StorageError::Integrity("fact page continuation maximum"));
    }
    let lease =
        memory.reserve(std::mem::size_of::<FactPage<T>>() + count * std::mem::size_of::<T>())?;
    let mut records = Vec::new();
    records.try_reserve_exact(count).map_err(|_| {
        StorageError::Content(layerfs_content::ContentError::ResourceUnavailable {
            what: "fact.page",
        })
    })?;
    if records.capacity() > count {
        return Err(StorageError::Integrity("fact page Vec capacity"));
    }
    let column = if scope.state().table().code() == 17 {
        "value"
    } else {
        "bound"
    };
    let sql = format!(
        "SELECT key,{column} FROM {} WHERE key>?1 ORDER BY key LIMIT ?2",
        fact_index::table(scope.state().table().code())
    );
    let lower = after.map(|a| scope.key(a)).transpose()?;
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query(rusqlite::params![
        lower.as_ref().map_or(&[][..], |k| k.as_slice()),
        count as i64
    ])?;
    let mut last = after;
    while let Some(row) = rows.next()? {
        let serial = scope.serial(fact_index::blob(row.get_ref(0)?, 25)?)?;
        if last.is_some_and(|old| old >= serial) || seal.maximum.is_none_or(|max| serial > max) {
            return Err(StorageError::Integrity("fact page serial order"));
        }
        records.push(decode(row, scope)?);
        last = Some(serial);
    }
    let eof = last == seal.maximum;
    if records.is_empty() && !eof {
        return Err(StorageError::Integrity("fact stalled page"));
    }
    Ok(FactPage::new(lease, seal.clone(), records, last, eof)?)
}
