//! Authenticated same-save reads from bounded pending state or sealed bytes.
use super::{state::State, Save};
use crate::{
    error::{StorageError, StorageResult},
    policy::READ_CANONICAL_BYTES_LIMIT,
};
use layerfs_content::{AuthenticatedObjects, ContentResult, ObjectId};
impl Save<'_> {
    /// Reads accepted pending objects or authenticated registered canonical bytes.
    pub fn read_objects(&self, ids: &[ObjectId]) -> StorageResult<Vec<Vec<u8>>> {
        if self.terminal.get() {
            return Err(StorageError::Aborted);
        }
        let result = self.read_inner(ids);
        if result.is_err() {
            self.terminal.set(true);
        }
        result
    }
    fn read_inner(&self, ids: &[ObjectId]) -> StorageResult<Vec<Vec<u8>>> {
        let mut state = self.state.borrow_mut();
        let storage = state.storage;
        let _published = storage.source.serving_published();
        let capacities = state.storage.capacities();
        if ids.len() > capacities.read_objects {
            return Err(StorageError::CapacityExceeded {
                what: "storage.read_objects",
                limit: capacities.read_objects as u64,
                actual: ids.len() as u64,
            });
        }
        state.storage.source.begin_demand(false);
        // This Save's own pending and sealed objects are read from it; only
        // the rest are located in the Store.
        let stored: Vec<ObjectId> = ids
            .iter()
            .copied()
            .filter(|id| {
                !state.packer.pending(*id)
                    && state.pending.pending_canonical(*id).is_none()
                    && state.packer.location(*id).is_none()
            })
            .collect();
        state.storage.source.locate(&stored)?;
        if stored.len() > 1 {
            // The chains of a grouped demand's stored objects are located a
            // level at a time, one locate per level for all of them, as the
            // Store's own reads do (`read::prefetch`, #810 F3); rebuilding
            // each object on its own would locate every chain step apart.
            let roots = stored
                .iter()
                .map(|id| {
                    state
                        .storage
                        .source
                        .location(*id, i64::MAX)?
                        .ok_or(StorageError::ObjectMissing(*id))
                })
                .collect::<StorageResult<Vec<_>>>()?;
            let State {
                storage,
                packs,
                decode,
                groups,
                ..
            } = &mut *state;
            let source = &storage.source;
            let mut walk = || crate::read::prefetch::chains(source, &roots, packs, decode, groups);
            // A walk a stale hint broke runs once more without hints, as the
            // Store's reads and each object's own resolution do (#758).
            match walk() {
                Err(error) if crate::read::hint_failure(&error) && source.drop_hints(&[])? => {
                    walk()?;
                }
                other => {
                    other?;
                }
            }
        }
        if ids.iter().any(|id| state.packer.pending(*id)) {
            let required = state.packer.finish_pack_bound();
            state.reserve_packs(required)?;
            let State {
                packer,
                compression,
                next_pack,
                pack_end,
                ..
            } = &mut *state;
            packer.seal_pending(ids, compression, next_pack, *pack_end)?;
        }
        use crate::source::Source;
        let declared = ids.iter().try_fold(0usize, |bytes, id| {
            let length = if let Some(value) = state.pending.pending_canonical(*id) {
                value.len()
            } else {
                match state.packer.location(*id) {
                    Some(location) => location,
                    None => state
                        .storage
                        .source
                        .location(*id, i64::MAX)?
                        .ok_or(StorageError::ObjectMissing(*id))?,
                }
                .canonical_length
            };
            bytes
                .checked_add(length)
                .ok_or(StorageError::Integrity("read bytes"))
        })?;
        if declared > READ_CANONICAL_BYTES_LIMIT {
            return Err(StorageError::CapacityExceeded {
                what: "storage.read_canonical_bytes",
                limit: READ_CANONICAL_BYTES_LIMIT as u64,
                actual: declared as u64,
            });
        }
        let mut out = Vec::with_capacity(ids.len());
        let mut bytes = 0usize;
        for id in ids {
            let value = if let Some(canonical) = state.pending.pending_canonical(*id) {
                canonical.to_vec()
            } else {
                state.resolve(*id)?
            };
            bytes = bytes
                .checked_add(value.len())
                .ok_or(StorageError::Integrity("read bytes"))?;
            if bytes > READ_CANONICAL_BYTES_LIMIT {
                return Err(StorageError::CapacityExceeded {
                    what: "storage.read_canonical_bytes",
                    limit: READ_CANONICAL_BYTES_LIMIT as u64,
                    actual: bytes as u64,
                });
            }
            out.push(value);
        }
        Ok(out)
    }
}
impl AuthenticatedObjects for Save<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.read_objects(ids).map_err(|error| {
            let mapped = crate::error::provider_error_ref(&error);
            self.remember_failure(error);
            mapped
        })
    }
}
