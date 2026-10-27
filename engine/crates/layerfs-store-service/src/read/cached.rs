//! One call's storage read caches over its Store ports (#352 F-035): the
//! pack bodies, decoded groups and decode arena a demand leaves behind serve
//! the call's next demands, instead of each demand starting cold. The caches
//! keep the storage reader's byte bounds and are dropped with the call.
use crate::history::lookup as args;
use crate::store::{PortError, StorePorts};
use layerfs_bridge::control::SdkRefusal;
use layerfs_content::{
    file::{mapping::FILE_STATE_CANONICAL_BYTES, FileView},
    policy::WHOLE_FILE_CANONICAL_OVERHEAD,
    AuthenticatedObjects, ContentError, ContentResult, ObjectId,
};
use layerfs_storage::{ReadCache, StorageError};
use std::{cell::RefCell, collections::HashMap};

pub struct Cached<'a> {
    ports: &'a StorePorts,
    cache: RefCell<ReadCache>,
}

impl<'a> Cached<'a> {
    pub fn new(ports: &'a StorePorts) -> Result<Self, SdkRefusal> {
        Ok(Self {
            ports,
            cache: RefCell::new(ReadCache::new().map_err(args::failed)?),
        })
    }

    /// The regular files `ids` opened as views, their roots read [`ROOTS`] at
    /// a time in demands of at most [`ROOT_BYTES`], so their delta chains are
    /// resolved level by level together instead of file by file. A root's
    /// canonical size is bounded from its file length, read first: a
    /// whole-file root is its payload plus a fixed overhead, a chunked or
    /// empty file's root is its fixed-size state.
    pub fn roots(&self, ids: &[ObjectId]) -> Result<HashMap<ObjectId, FileView>, SdkRefusal> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let lengths = self.ports.file_lengths(ids).map_err(args::failed)?;
        if lengths.len() != ids.len() {
            return Err(args::failed("file length cardinality"));
        }
        let mut views = HashMap::with_capacity(ids.len());
        for batch in batches(&lengths) {
            self.open(&ids[batch], &mut views)?;
        }
        Ok(views)
    }

    fn open(
        &self,
        ids: &[ObjectId],
        views: &mut HashMap<ObjectId, FileView>,
    ) -> Result<(), SdkRefusal> {
        let canonical = crate::telemetry::timed("sdk.lines", |timing| {
            timing
                .child("content.read")
                .run(|_| self.read_canonical_batch(ids))
        })
        .map_err(args::content)?;
        if canonical.len() != ids.len() {
            return Err(args::failed("root cardinality"));
        }
        for (id, canonical) in ids.iter().zip(canonical) {
            views.insert(
                *id,
                FileView::from_canonical(*id, canonical).map_err(args::content)?,
            );
        }
        Ok(())
    }
}

/// Roots read in one demand, and their canonical bytes at most: an eighth
/// of the storage reader's per-demand limit, so a batch holds little.
pub const ROOTS: usize = 64;
pub const ROOT_BYTES: usize = 4 * 1024 * 1024;

/// The most canonical bytes a regular file's root of `length` has.
pub fn root_bound(length: u64) -> usize {
    usize::try_from(length)
        .unwrap_or(usize::MAX)
        .saturating_add(WHOLE_FILE_CANONICAL_OVERHEAD)
        .max(FILE_STATE_CANONICAL_BYTES)
}

/// Consecutive runs of files, by their lengths, read one demand each: at
/// most [`ROOTS`] files and [`ROOT_BYTES`] of bounded root bytes, except a
/// single file over that bound, which goes alone.
pub fn batches(lengths: &[u64]) -> Vec<std::ops::Range<usize>> {
    let (mut runs, mut from, mut bytes) = (Vec::new(), 0, 0usize);
    for (at, &length) in lengths.iter().enumerate() {
        let size = root_bound(length);
        if at > from && (at - from == ROOTS || bytes.saturating_add(size) > ROOT_BYTES) {
            runs.push(from..at);
            (from, bytes) = (at, 0);
        }
        bytes = bytes.saturating_add(size);
    }
    if from < lengths.len() {
        runs.push(from..lengths.len());
    }
    runs
}

impl AuthenticatedObjects for Cached<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.ports
            .objects_with(&mut self.cache.borrow_mut(), ids)
            .map_err(|error| match error.as_ref() {
                PortError::Storage(StorageError::ObjectMissing(_)) => ContentError::MissingObject,
                PortError::Storage(StorageError::Content(error)) => error.clone(),
                _ => ContentError::ProviderFailure {
                    what: "Store object demand",
                },
            })
    }
}
