//! Authenticated same-save reads from bounded pending state or sealed bytes.
use super::{state::State, Save};
use crate::{
    error::{StorageError, StorageResult},
    policy::READ_CANONICAL_BYTES_LIMIT,
    read::{CapturedLocators, LocatorMode},
};
use layerfs_content::{AuthenticatedObjects, ContentResult, ObjectId};
use std::sync::Arc;

impl Save<'_> {
    /// Reads accepted pending objects or authenticated registered canonical bytes.
    pub fn read_objects(&self, ids: &[ObjectId]) -> StorageResult<Vec<Vec<u8>>> {
        self.read_with_locators(ids, None)
    }
    fn read_with_locators(
        &self,
        ids: &[ObjectId],
        locators: Option<&Arc<CapturedLocators>>,
    ) -> StorageResult<Vec<Vec<u8>>> {
        if self.terminal.get() {
            return Err(StorageError::Aborted);
        }
        let result = self.read_inner(ids, locators);
        if result.is_err() {
            self.terminal.set(true);
        }
        result
    }
    fn read_inner(
        &self,
        ids: &[ObjectId],
        locators: Option<&Arc<CapturedLocators>>,
    ) -> StorageResult<Vec<Vec<u8>>> {
        let mut state = self.state.borrow_mut();
        let storage = state.storage;
        let _published = storage.source.serving_published();
        let remote_capture = self.captured.get()
            && self.join_metadata_reads
            && storage.source.metadata.batched_chain_reads();
        let _compact_inodes = storage.source.compact_inode_packs(remote_capture);
        let _paired = storage.source.pairing_inode_values(remote_capture);
        let _locators = storage.source.scoped_locators(
            locators
                .filter(|_| remote_capture)
                .map(|cache| LocatorMode::Reuse(cache.clone())),
        );
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
                    && !state.deferred.holds(*id)
                    && state.packer.location(*id).is_none()
                    && !state.resolved.holds(*id)
            })
            .collect();
        if self.join_metadata_reads {
            storage.source.locate_for_save(&stored, &mut state.packs)?;
        } else {
            storage.source.locate(&stored)?;
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
        use crate::location::Source;
        let declared = ids.iter().try_fold(0usize, |bytes, id| {
            let length = if let Some(value) = state
                .pending
                .pending_canonical(*id)
                .or_else(|| state.deferred.canonical(*id))
            {
                value.len()
            } else if let Some(length) = state.resolved.canonical_len(*id) {
                length
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
        if stored.len() > 1 {
            // Walk stored chains only after the demand passes its byte limit.
            // Locate them once for this demand, including the
            // inode leaves prepared below. begin_demand(false) disables
            // hinted locators, so failures from this authoritative walk
            // propagate. Group fetching handles advisory pack hints itself.
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
            crate::read::prefetch_chains(&storage.source, &roots, packs, decode, groups)?;
        }
        let mut out = Vec::with_capacity(ids.len());
        let mut bytes = 0usize;
        for (at, id) in ids.iter().enumerate() {
            let value = if let Some(canonical) = state
                .pending
                .pending_canonical(*id)
                .or_else(|| state.deferred.canonical(*id))
            {
                canonical.to_vec()
            } else {
                state.resolve_demand(*id, &ids[at..])?
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
impl Save<'_> {
    /// Canonical reads for the captured producer, using only positive
    /// metadata from this attempt's exact Workspace reader on a complete hit.
    pub fn read_captured_canonical_batch(
        &self,
        ids: &[ObjectId],
        locators: Option<&Arc<CapturedLocators>>,
    ) -> ContentResult<Vec<Vec<u8>>> {
        self.read_with_locators(ids, locators).map_err(|error| {
            let mapped = crate::error::provider_error_ref(&error);
            self.remember_failure(error);
            mapped
        })
    }
}
impl AuthenticatedObjects for Save<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.read_captured_canonical_batch(ids, None)
    }
}
