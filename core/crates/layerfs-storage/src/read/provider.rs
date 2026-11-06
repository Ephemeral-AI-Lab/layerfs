//! C1's authenticated provider over a two-port Storage handle.

use super::objects::{Decoder, ReadState};
use crate::{error::StorageResult, storage::Storage};
use layerfs_content::{AuthenticatedObjects, ContentResult, ObjectId};
use std::cell::RefCell;

/// One operation's decode arena and bounded group/pooled-value caches.
pub struct Reader<'a> {
    storage: &'a Storage,
    state: RefCell<(Decoder, ReadState)>,
    /// Continues the latest Save's operation (see [`Storage::reader_after_save`]).
    after_save: bool,
}
impl<'a> Reader<'a> {
    pub(crate) fn new(storage: &'a Storage, after_save: bool) -> StorageResult<Self> {
        Ok(Self {
            storage,
            state: RefCell::new((Decoder::new()?, ReadState::new())),
            after_save,
        })
    }
    /// Actual pooled reconstruction work across this operation’s read demands.
    pub fn pooled_read_counters(&self) -> crate::encoding::pool::PoolReadCounters {
        self.state.borrow().1.pooled_read_counters()
    }
    /// Reconstructs and authenticates a bounded demand, in demand order.
    pub fn read_objects(&self, ids: &[ObjectId]) -> StorageResult<Vec<Vec<u8>>> {
        let _serve = self
            .after_save
            .then(|| self.storage.source.serving_published());
        let (workspace, state) = &mut *self.state.borrow_mut();
        state.read(
            workspace,
            &self.storage.source,
            &self.storage.capacities(),
            ids,
        )
    }
    /// Reads a bounded window of saved regular-file lengths in demand order.
    ///
    /// Whole-file length comes from acknowledged trusted Store metadata without
    /// fetching payload. Chunked/empty files authenticate only their small state
    /// object, never the mapping or chunks. This is a metadata fact, not a payload
    /// integrity check. Missing/non-file roots fail explicitly; no fallback read.
    pub fn file_lengths(&self, ids: &[ObjectId]) -> StorageResult<Vec<u64>> {
        super::length::read(self.storage, ids, |states| self.read_objects(states))
    }
}
/// One caller's decode arena and bounded pack, group and pooled-value caches,
/// kept across that caller's demands instead of a fresh [`Reader`]'s per
/// demand. Pack ids come from a counter that never goes back and published
/// packs never change, so a kept body stays valid; every object read through
/// it is authenticated as before. The bounds are the [`Reader`]'s.
pub struct ReadCache(Decoder, KeptReads);
impl ReadCache {
    /// Empty caches and a fresh decode arena.
    pub fn new() -> StorageResult<Self> {
        Ok(Self(Decoder::new()?, KeptReads::new()))
    }
}
/// [`ReadCache`]'s pack, group and pooled-value caches without its decode
/// arena, so an owner on another thread can keep them (`Send`); each demand
/// takes a fresh arena, allocated only if it decodes.
pub struct KeptReads(ReadState);
impl KeptReads {
    /// Most bytes the caches keep without an isolated singleton pack: 2 MiB
    /// of pack bodies, 512 KiB of decoded groups and 512 KiB of pooled values.
    pub const ORDINARY_BYTES: usize = crate::policy::DEPENDENCY_PACK_CACHE_BYTES
        + crate::policy::DECODED_GROUP_CACHE_BYTES
        + crate::policy::POOLED_VALUE_CACHE_BYTES;
    /// Empty caches.
    pub fn new() -> Self {
        Self(ReadState::new())
    }
    /// Bytes the caches keep now; above [`Self::ORDINARY_BYTES`] only while
    /// an isolated singleton is kept.
    pub fn retained_bytes(&self) -> usize {
        self.0.retained_bytes()
    }
}
impl Default for KeptReads {
    fn default() -> Self {
        Self::new()
    }
}
impl Storage {
    /// [`Reader::read_objects`] through caller-kept caches.
    pub fn read_with(
        &self,
        cache: &mut ReadCache,
        ids: &[ObjectId],
    ) -> StorageResult<Vec<Vec<u8>>> {
        let ReadCache(workspace, kept) = cache;
        kept.0
            .read(workspace, &self.source, &self.capacities(), ids)
    }
    /// [`Reader::read_objects`] through kept caches and a fresh arena.
    pub fn read_kept(&self, kept: &mut KeptReads, ids: &[ObjectId]) -> StorageResult<Vec<Vec<u8>>> {
        let mut workspace = Decoder::new()?;
        kept.0
            .read(&mut workspace, &self.source, &self.capacities(), ids)
    }
    /// [`Reader::file_lengths`] through kept caches and a fresh arena.
    pub fn lengths_kept(&self, kept: &mut KeptReads, ids: &[ObjectId]) -> StorageResult<Vec<u64>> {
        super::length::read(self, ids, |states| self.read_kept(kept, states))
    }
}
impl AuthenticatedObjects for Reader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.read_objects(ids).map_err(crate::error::provider_error)
    }
}
