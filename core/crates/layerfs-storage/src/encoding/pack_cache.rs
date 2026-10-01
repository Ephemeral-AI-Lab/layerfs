//! Actual-capacity pack retention and one separate compatibility current body.
use crate::{
    error::{StorageError, StorageResult},
    policy::{DEPENDENCY_PACK_CACHE_BYTES, SINGLETON_PACK_LIMIT},
    sqlite::lookup,
};
use rusqlite::Connection;
use std::collections::BTreeMap;

#[derive(Debug)]
pub(crate) struct CurrentPack {
    id: i64,
    bytes: Vec<u8>,
}
impl CurrentPack {
    pub(crate) fn capacity(&self) -> usize {
        self.bytes.capacity()
    }
}
fn excess(what: &'static str, limit: usize, actual: usize) -> StorageError {
    StorageError::CapacityExceeded {
        what,
        limit: limit as u64,
        actual: actual as u64,
    }
}
/// Validation precedes provider effects, even when callers supply their own map.
pub(crate) fn retained(packs: &BTreeMap<i64, Vec<u8>>) -> StorageResult<usize> {
    packs.values().try_fold(0usize, |total, body| {
        let actual = total.checked_add(body.capacity()).ok_or_else(|| {
            excess(
                "pack cache capacity",
                DEPENDENCY_PACK_CACHE_BYTES,
                usize::MAX,
            )
        })?;
        if actual > DEPENDENCY_PACK_CACHE_BYTES {
            return Err(excess(
                "pack cache capacity",
                DEPENDENCY_PACK_CACHE_BYTES,
                actual,
            ));
        }
        Ok(actual)
    })
}
/// Pack Vec never escapes this owner. Oversized bodies belong only to current.
pub(crate) fn body<'a>(
    packs: &'a mut BTreeMap<i64, Vec<u8>>,
    current: &'a mut Option<CurrentPack>,
    connection: &Connection,
    id: i64,
) -> StorageResult<(&'a [u8], bool)> {
    let retained = retained(packs)?;
    if packs.contains_key(&id) {
        return Ok((packs.get(&id).unwrap().as_slice(), false));
    }
    if current.as_ref().is_some_and(|body| body.id == id) {
        return Ok((current.as_ref().unwrap().bytes.as_slice(), false));
    }
    // The old large current body is destroyed before the next allocation.
    *current = None;
    let bytes = lookup::pack_bytes(connection, id)?;
    let capacity = bytes.capacity();
    if capacity > SINGLETON_PACK_LIMIT {
        return Err(excess(
            "current pack capacity",
            SINGLETON_PACK_LIMIT,
            capacity,
        ));
    }
    if retained.saturating_add(capacity) > DEPENDENCY_PACK_CACHE_BYTES {
        packs.clear();
    }
    if capacity > DEPENDENCY_PACK_CACHE_BYTES {
        *current = Some(CurrentPack { id, bytes });
        Ok((current.as_ref().unwrap().bytes.as_slice(), true))
    } else {
        packs.insert(id, bytes);
        Ok((packs.get(&id).unwrap().as_slice(), true))
    }
}
