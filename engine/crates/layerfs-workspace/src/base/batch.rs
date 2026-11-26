//! One window of base files a Commit opens: their provided lengths in one
//! demand and their content roots fetched in bounded batches (#810 F5).
use super::client::{CanonicalClient, CANONICAL_BYTES, DEMAND_BYTES, DEMAND_IDS};
use crate::{BaseView, WorkspaceResult};
use layerfs_content::file::mapping::FILE_STATE_CANONICAL_BYTES;
use layerfs_content::policy::WHOLE_FILE_CANONICAL_OVERHEAD;
use layerfs_content::{ContentError, ObjectId};

/// Most canonical bytes one prefetch batch names, by the upper bound the
/// provided lengths give: a bounded transient copy, and with the cache's
/// allowance at least four times larger every root of a batch stays cached
/// until its file is opened.
const PREFETCH_BYTES: usize = 4 * 1024 * 1024;
// One batch is one upstream demand within its byte bound.
const _: () = assert!(PREFETCH_BYTES <= DEMAND_BYTES);

/// The provider's own lengths of one window's base file content roots,
/// sorted by root: what `BaseView::captured_file` would ask one file at a
/// time. It holds one captured inode page's files at most.
#[derive(Default)]
pub struct BaseFiles {
    lengths: Vec<(ObjectId, u64)>,
    /// Object prefetch batches made for this window.
    pub(crate) prefetch_batches: u64,
}
impl BaseFiles {
    /// The provided length of `root`, if this window asked for it.
    pub(crate) fn length(&self, root: ObjectId) -> Option<u64> {
        self.lengths
            .binary_search_by_key(&root, |(id, _)| *id)
            .ok()
            .map(|at| self.lengths[at].1)
    }
}
impl CanonicalClient {
    /// The remembered length of `id`, with no demand. It is a pure function
    /// of the immutable root: remembered only from the provider's own
    /// answers and from a published Commit's own whole-file objects.
    pub(crate) fn remembered_length(&self, id: ObjectId) -> Option<u64> {
        self.state().ok()?.cache.length(id)
    }
    /// The provider's lengths of `ids`, in order, asked in one demand per
    /// [`DEMAND_IDS`] and remembered as [`Self::provide_lengths`] does.
    pub(crate) fn provided_lengths(&self, ids: &[ObjectId]) -> WorkspaceResult<Vec<u64>> {
        let provider = self
            .lengths
            .as_ref()
            .ok_or(crate::WorkspaceError::MissingLengthProvider)?;
        let mut lengths = Vec::with_capacity(ids.len());
        for window in ids.chunks(DEMAND_IDS) {
            let values = provider.file_lengths(window)?;
            if values.len() != window.len() {
                return Err(ContentError::BatchCardinality {
                    requested: window.len(),
                    returned: values.len(),
                }
                .into());
            }
            let mut state = self.state()?;
            for (id, length) in window.iter().zip(&values) {
                let evictions = state.cache.remember_length(*id, *length).unwrap_or(0);
                state.work.evictions = state.work.evictions.saturating_add(evictions);
            }
            drop(state);
            lengths.extend(values);
        }
        Ok(lengths)
    }
}
impl BaseView {
    /// One window of base files to open, with the `lengths` the provider
    /// gave for their content roots in the caller's one demand: the roots
    /// are placed in the cache in batches of at most [`PREFETCH_BYTES`]
    /// (and a quarter of the cache's allowance) by the canonical size each
    /// length bounds. A root larger than a batch, or than the cache keeps,
    /// is left to its own demand when the file is opened. A memory-only
    /// client fetches nothing. One attempt: a failed batch fails the window
    /// with the provider's original cause.
    pub(crate) fn base_files(
        &self,
        mut lengths: Vec<(ObjectId, u64)>,
        whole_file_raw_limit: usize,
    ) -> WorkspaceResult<BaseFiles> {
        let client = self.client();
        lengths.sort_unstable();
        lengths.dedup_by_key(|(root, _)| *root);
        let mut files = BaseFiles {
            lengths,
            prefetch_batches: 0,
        };
        if files.lengths.is_empty() || client.resident {
            return Ok(files);
        }
        let budget = {
            let state = client.state()?;
            PREFETCH_BYTES.min(state.cache.capacity() / 4)
        };
        let mut batch = Vec::new();
        let mut bytes = 0_usize;
        for (root, length) in &files.lengths {
            // A whole file's root is its payload plus a fixed overhead; a
            // chunked or empty file's root is its small state object.
            let bound = match usize::try_from(*length) {
                Ok(length) if length <= whole_file_raw_limit => length
                    .saturating_add(WHOLE_FILE_CANONICAL_OVERHEAD)
                    .max(FILE_STATE_CANONICAL_BYTES),
                _ => FILE_STATE_CANONICAL_BYTES,
            };
            if bound > budget || bound > CANONICAL_BYTES {
                continue;
            }
            if bytes + bound > budget || batch.len() == DEMAND_IDS {
                client.fetch_ahead(&batch)?;
                files.prefetch_batches += 1;
                batch.clear();
                bytes = 0;
            }
            batch.push(*root);
            bytes += bound;
        }
        if !batch.is_empty() {
            client.fetch_ahead(&batch)?;
            files.prefetch_batches += 1;
        }
        Ok(files)
    }
}
