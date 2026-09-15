//! Host-side frozen-generation acquisition for remote (sandbox-owned)
//! workspaces.
//!
//! At Commit the host captures the workspace's frozen generation through the
//! control lane, pulls its changed records and needed payload bytes over the
//! snapshot lane into an owned, bounded input, and feeds the existing
//! single-worker canonical builder. Payload bytes stream from the sandbox's
//! packed local backing on demand; no full host staging copy exists. The
//! materialized input is a stable owned copy, so the build holds no lock the
//! live workspace needs.

use crate::live_backing::RemoteWorkspace;
use crate::WorkspaceResult;
use layerfs_fuse::live_wire::{self as wire, Input};
use layerfs_workspace_core::backing::{BackingId, BackingRef};
use layerfs_workspace_core::{Node, NodeId};
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The capture identity carried by every snapshot-lane frame and completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SnapshotToken {
    pub incarnation: u64,
    pub attempt: u64,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CaptureSummary {
    pub token: SnapshotToken,
    pub frontier_len: u64,
}

/// One completion record: the canonical re-base of a covered node.
#[derive(Clone, Copy)]
pub(crate) struct CompletionRecord {
    pub node: NodeId,
    pub revision: u64,
    pub inode: [u8; 32],
    pub content: [u8; 32],
    pub size: u64,
    pub mode: u32,
    pub links: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub kind: u8,
}

/// The host-side resource behind `Piece::Spool` references in the
/// materialized frozen input: fetches bytes from the sandbox's packed local
/// backing on demand through the snapshot lane. Reads run on builder threads
/// (blocking context); every fetch is bounded to one wire frame.
pub(crate) struct RemoteSegment {
    server: Arc<layerfs_fuse::live_transport::BackingServer>,
    token: SnapshotToken,
    id: BackingId,
    len: AtomicU64,
    /// Bounded read-ahead shared by every piece in the same sandbox segment.
    windows: Arc<RemoteWindows>,
}

/// Read-ahead allowance for one frozen input. The sandbox packs pieces into
/// 1 MiB segments (`LocalSpool::SEGMENT_CAPACITY`), so one bounded fetch per
/// segment serves every piece inside it instead of one round trip per piece.
/// The cache never exceeds this allowance and is released with the frozen
/// input; it is the declared combined transfer/staging buffer, not a payload
/// copy.
pub(crate) const REMOTE_WINDOW_BUDGET: usize = 8 * 1024 * 1024;

/// Largest single fetch: one whole segment, matching the daemon's bound.
const REMOTE_FETCH_MAX: u64 = 1024 * 1024;

/// Bounded shared cache of fetched sandbox segments.
pub(crate) struct RemoteWindows {
    inner: Mutex<RemoteWindowCache>,
    budget: usize,
}

#[derive(Default)]
struct RemoteWindowCache {
    /// Frozen coverage per segment, learned while pulling the records.
    spans: HashMap<BackingId, (u64, u64)>,
    /// Fetched bytes per segment, with the offset they start at.
    windows: HashMap<BackingId, (u64, Vec<u8>)>,
    order: std::collections::VecDeque<BackingId>,
    used: usize,
}

impl RemoteWindows {
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            inner: Mutex::new(RemoteWindowCache::default()),
            budget,
        }
    }

    /// Record one frozen piece range so the whole segment can be fetched in
    /// a single bounded request.
    fn note_span(&self, id: BackingId, offset: u64, len: u64) {
        let Ok(mut cache) = self.inner.lock() else {
            return;
        };
        let end = offset.saturating_add(len);
        match cache.spans.entry(id) {
            std::collections::hash_map::Entry::Occupied(mut span) => {
                let span = span.get_mut();
                span.0 = span.0.min(offset);
                span.1 = span.1.max(end);
            }
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert((offset, end));
            }
        }
    }

    fn span(&self, id: BackingId) -> Option<(u64, u64)> {
        self.inner.lock().ok()?.spans.get(&id).copied()
    }

    fn copy_out(&self, id: BackingId, offset: u64, len: u64) -> Option<Vec<u8>> {
        let cache = self.inner.lock().ok()?;
        let (start, bytes) = cache.windows.get(&id)?;
        let from = offset.checked_sub(*start)? as usize;
        let to = from.checked_add(len as usize)?;
        bytes.get(from..to).map(|bytes| bytes.to_vec())
    }

    fn insert(&self, id: BackingId, start: u64, bytes: Vec<u8>) {
        let Ok(mut cache) = self.inner.lock() else {
            return;
        };
        if bytes.len() > self.budget {
            return;
        }
        if let Some((_, previous)) = cache.windows.remove(&id) {
            cache.used = cache.used.saturating_sub(previous.len());
            cache.order.retain(|entry| *entry != id);
        }
        while cache.used.saturating_add(bytes.len()) > self.budget {
            let Some(oldest) = cache.order.pop_front() else {
                break;
            };
            if let Some((_, evicted)) = cache.windows.remove(&oldest) {
                cache.used = cache.used.saturating_sub(evicted.len());
            }
        }
        cache.used = cache.used.saturating_add(bytes.len());
        cache.order.push_back(id);
        cache.windows.insert(id, (start, bytes));
    }
}

impl RemoteSegment {
    pub(crate) fn new(
        server: Arc<layerfs_fuse::live_transport::BackingServer>,
        token: SnapshotToken,
        id: BackingId,
        windows: Arc<RemoteWindows>,
    ) -> Self {
        Self {
            server,
            token,
            id,
            len: AtomicU64::new(u64::MAX),
            windows,
        }
    }

    pub(crate) fn len(&self) -> u64 {
        self.len.load(Ordering::Acquire)
    }

    fn fetch(&self, offset: u64, len: u64) -> WorkspaceResult<Vec<u8>> {
        let mut request = vec![wire::SNAP_READ];
        wire::u64_out(&mut request, self.token.incarnation);
        wire::u64_out(&mut request, self.token.attempt);
        wire::u64_out(&mut request, self.id.0);
        wire::u64_out(&mut request, offset);
        request.extend_from_slice(&(len as u32).to_be_bytes());
        let reply = self
            .server
            .snapshot_request(&request)
            .map_err(|_| crate::WorkspaceError::InvalidExecution)?;
        if reply.len() as u64 != len {
            return Err(crate::WorkspaceError::InvalidExecution);
        }
        Ok(reply)
    }

    /// Read `len` bytes at `offset`, served from the shared bounded window
    /// when the segment was already fetched.
    pub(crate) fn read(&self, offset: u64, len: u64) -> WorkspaceResult<Vec<u8>> {
        if offset.checked_add(len).is_none_or(|end| end > self.len()) {
            return Err(crate::WorkspaceError::InvalidExecution);
        }
        if len == 0 {
            return Ok(Vec::new());
        }
        if let Some(bytes) = self.windows.copy_out(self.id, offset, len) {
            return Ok(bytes);
        }
        // One bounded request per segment: the pulled records already told us
        // which ranges of this segment the frozen generation references, so
        // every piece in it is served by the same fetch. A range outside that
        // coverage (unreachable for a pulled input) falls back to the exact
        // request.
        let (fetch_start, fetch_len) = match self.windows.span(self.id) {
            Some((start, end))
                if start <= offset
                    && offset
                        .checked_add(len)
                        .is_some_and(|requested| requested <= end) =>
            {
                (start, end - start)
            }
            _ => (offset, len),
        };
        if fetch_len == 0 || fetch_len > REMOTE_FETCH_MAX {
            return Err(crate::WorkspaceError::InvalidExecution);
        }
        let bytes = self.fetch(fetch_start, fetch_len)?;
        let from = (offset - fetch_start) as usize;
        let out = bytes
            .get(from..from + len as usize)
            .ok_or(crate::WorkspaceError::InvalidExecution)?
            .to_vec();
        self.windows.insert(self.id, fetch_start, bytes);
        Ok(out)
    }
}

pub(crate) struct FrozenRemoteInput {
    pub nodes: HashMap<NodeId, Node>,
    pub dirty: BTreeSet<NodeId>,
    pub mutation_generation: u64,
}

impl RemoteWorkspace {
    /// Capture the frozen generation. Requires no active attempt; the caller
    /// serializes commits per workspace.
    pub(crate) fn capture(&self) -> WorkspaceResult<CaptureSummary> {
        let reply = self
            .server
            .request(&[wire::CAPTURE])
            .map_err(|_| crate::WorkspaceError::InvalidExecution)?;
        let mut input = Input(&reply);
        let incarnation = input.u64().map_err(invalid)?;
        let attempt = input.u64().map_err(invalid)?;
        let generation = input.u64().map_err(invalid)?;
        let frontier_len = input.u64().map_err(invalid)?;
        let _base_root = input.object().map_err(invalid)?;
        let _head = input.head().map_err(invalid)?;
        input.done().map_err(invalid)?;
        Ok(CaptureSummary {
            token: SnapshotToken {
                incarnation,
                attempt,
                generation,
            },
            frontier_len,
        })
    }

    /// Pull every frozen record into an owned input. Spool pieces reference
    /// `RemoteSegment` resources that stream bytes from the sandbox backing.
    pub(crate) fn pull_frozen_input(
        &self,
        summary: &CaptureSummary,
    ) -> WorkspaceResult<FrozenRemoteInput> {
        let token = summary.token;
        let mut nodes = HashMap::new();
        let mut dirty = BTreeSet::new();
        let mut after = 0u64;
        let windows = Arc::new(RemoteWindows::new(REMOTE_WINDOW_BUDGET));
        loop {
            let mut request = vec![wire::SNAP_RECORDS];
            wire::u64_out(&mut request, token.incarnation);
            wire::u64_out(&mut request, token.attempt);
            wire::u64_out(&mut request, after);
            let page = self
                .server
                .snapshot_request(&request)
                .map_err(|_| crate::WorkspaceError::InvalidExecution)?;
            let mut input = Input(&page);
            let done = input.byte().map_err(invalid)?;
            let _generation = input.u64().map_err(invalid)?;
            let count = input.u64().map_err(invalid)?;
            after = input.u64().map_err(invalid)?;
            for _ in 0..count {
                let record = input.bytes().map_err(invalid)?;
                let (id, node) = wire::node_in(record, |id, offset, len| {
                    // Resolve every spool reference to a bounded remote
                    // fetcher and record the range so the whole segment can
                    // be fetched once. Range validation happens on the daemon
                    // (segment high-water) and on the reply length; the host
                    // does not bound the segment itself.
                    windows.note_span(id, offset, len);
                    let segment =
                        RemoteSegment::new(self.server.clone(), token, id, windows.clone());
                    Ok(BackingRef::new(id, segment))
                })
                .map_err(invalid)?;
                if nodes.insert(id, node).is_some() {
                    return Err(crate::WorkspaceError::InvalidExecution);
                }
                dirty.insert(id);
            }
            input.done().map_err(invalid)?;
            if done == 1 || count == 0 {
                break;
            }
        }
        Ok(FrozenRemoteInput {
            nodes,
            dirty,
            mutation_generation: token.generation,
        })
    }

    /// Abandon an unresolved attempt before publication.
    pub(crate) fn cancel_snapshot(&self, token: SnapshotToken) -> WorkspaceResult<()> {
        let mut request = vec![wire::SNAP_CANCEL];
        wire::u64_out(&mut request, token.incarnation);
        wire::u64_out(&mut request, token.attempt);
        self.server
            .request(&request)
            .map_err(|_| crate::WorkspaceError::InvalidExecution)?;
        Ok(())
    }

    /// Deliver the exact publication outcome for the captured generation:
    /// per-node canonical re-bases guarded by each node's captured revision.
    /// Idempotent; a lost reply is retried with the same records.
    pub(crate) fn complete_generation(
        &self,
        token: SnapshotToken,
        root: layerfs_content::ObjectId,
        head: Option<[u8; 33]>,
        records: &[CompletionRecord],
    ) -> WorkspaceResult<(u64, u64)> {
        let mut begin = vec![wire::COMPLETE_BEGIN];
        wire::u64_out(&mut begin, token.incarnation);
        wire::u64_out(&mut begin, token.attempt);
        begin.extend_from_slice(root.as_bytes());
        wire::bytes_out(
            &mut begin,
            head.as_ref().map_or(&[][..], |bytes| &bytes[..]),
        )
        .map_err(invalid)?;
        self.server
            .request(&begin)
            .map_err(|_| crate::WorkspaceError::InvalidExecution)?;

        for page in records.chunks(wire::COMPLETE_PAGE_RECORDS) {
            let mut frame = vec![wire::COMPLETE_NODE];
            wire::u64_out(&mut frame, token.incarnation);
            wire::u64_out(&mut frame, token.attempt);
            for record in page {
                wire::u64_out(&mut frame, record.node.0);
                wire::u64_out(&mut frame, record.revision);
                frame.extend_from_slice(&record.inode);
                frame.extend_from_slice(&record.content);
                wire::u64_out(&mut frame, record.size);
                frame.extend_from_slice(&record.mode.to_be_bytes());
                frame.extend_from_slice(&record.links.to_be_bytes());
                wire::u64_out(&mut frame, record.mtime_seconds as u64);
                frame.extend_from_slice(&record.mtime_nanoseconds.to_be_bytes());
                frame.push(record.kind);
            }
            self.server
                .request(&frame)
                .map_err(|_| crate::WorkspaceError::InvalidExecution)?;
        }

        let mut end = vec![wire::COMPLETE_END];
        wire::u64_out(&mut end, token.incarnation);
        wire::u64_out(&mut end, token.attempt);
        wire::u64_out(&mut end, token.generation);
        wire::u64_out(&mut end, records.len() as u64);
        let reply = self
            .server
            .request(&end)
            .map_err(|_| crate::WorkspaceError::InvalidExecution)?;
        let mut input = Input(&reply);
        let applied = input.u64().map_err(invalid)?;
        let skipped = input.u64().map_err(invalid)?;
        input.done().map_err(invalid)?;
        Ok((applied, skipped))
    }
}

fn invalid(_: std::io::Error) -> crate::WorkspaceError {
    crate::WorkspaceError::InvalidExecution
}
