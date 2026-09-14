use crate::cow_tree::{Data, FileData, Node, NodeId, Workspace};
#[cfg(test)]
use crate::file_edit::MAX_INLINE_PER_WORKSPACE;
use crate::file_edit::{Piece, PieceTree, SpoolSlice};
use layerfs_content::file::content::read_range;
use layerfs_layerstack_store::{CoreReader, Result, SnapshotReader, StoreError};
use layerfs_workspace_core::backing::{BackingId, BackingRef};
use layerfs_workspace_core::ReadSource;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
#[cfg(test)]
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

/// Event-boundary observations of owned regular spool inode allocation, not
/// logical file lengths or a continuous filesystem allocator peak. Kept outside
/// edit checkpoints so failed writes cannot erase resources they actually used.
#[derive(Default, Debug)]
pub(crate) struct PhysicalSpoolMetrics {
    allocated: HashMap<u64, u64>,
    current: u64,
    peak: u64,
    errors: u64,
    observations: u64,
}

impl PhysicalSpoolMetrics {
    pub(crate) fn observe(&mut self, node: u64, metadata: &std::fs::Metadata) {
        use std::os::unix::fs::MetadataExt;
        self.observations = self.observations.saturating_add(1);
        let previous = self.allocated.get(&node).copied().unwrap_or(0);
        let Some(allocated) = metadata.blocks().checked_mul(512) else {
            self.error();
            return;
        };
        let Some(current) = self
            .current
            .checked_sub(previous)
            .and_then(|n| n.checked_add(allocated))
        else {
            self.error();
            return;
        };
        self.allocated.insert(node, allocated);
        self.current = current;
        self.peak = self.peak.max(current);
    }

    pub(crate) fn removed(&mut self, node: u64) {
        if let Some(bytes) = self.allocated.remove(&node) {
            if let Some(current) = self.current.checked_sub(bytes) {
                self.current = current;
            } else {
                self.error();
            }
        }
    }

    pub(crate) fn error(&mut self) {
        self.errors = self.errors.saturating_add(1);
    }

    pub(crate) fn snapshot(&self) -> (Option<u64>, Option<u64>, u64, u64) {
        (
            (self.errors == 0).then_some(self.current),
            (self.errors == 0).then_some(self.peak),
            self.errors,
            self.observations,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SpoolWriteMetrics {
    pub(crate) write_bytes: u64,
    pub(crate) write_open_count: u64,
    pub(crate) write_ns: u64,
    pub(crate) fence_count: u64,
    pub(crate) fence_ns: u64,
}

pub(crate) struct HostSpool {
    pub(crate) segments: HashMap<u64, BackingRef>,
    pub(crate) current: Option<u64>,
    pub(crate) next_id: u64,
    pub(crate) bytes: u64,
    pub(crate) physical: Arc<Mutex<PhysicalSpoolMetrics>>,
    pub(crate) metrics: SpoolWriteMetrics,
}

impl Default for HostSpool {
    fn default() -> Self {
        Self {
            segments: HashMap::new(),
            current: None,
            next_id: 1,
            bytes: 0,
            physical: Default::default(),
            metrics: SpoolWriteMetrics::default(),
        }
    }
}

// Physical segments are shared by immutable piece/read-plan references. Only the
// existing exclusive Workspace writer appends or rolls back an unpublished tail.
const SPOOL_SEGMENT_BYTES: u64 = 1024 * 1024;

#[derive(Debug)]
pub(crate) struct SpoolSegment {
    pub(crate) file: File,
    id: u64,
    pub(crate) len: AtomicU64,
    pub(crate) capacity: u64,
    physical: Arc<Mutex<PhysicalSpoolMetrics>>,
}

impl SpoolSegment {
    fn new(
        directory: &Path,
        id: u64,
        capacity: u64,
        physical: Arc<Mutex<PhysicalSpoolMetrics>>,
    ) -> Result<Self> {
        let path = directory.join(format!("segment-{}", crate::WorkspaceId::new()));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        // Access uses the exclusively owned descriptor; no mutable path can
        // replace this backing, and no per-logical-file unlink remains at Commit.
        std::fs::remove_file(path)?;
        let segment = Self {
            file,
            id,
            len: AtomicU64::new(0),
            capacity,
            physical,
        };
        segment.observe();
        Ok(segment)
    }

    pub(crate) fn observe(&self) {
        let metadata = self.file.metadata();
        if let Ok(mut physical) = self.physical.lock() {
            match metadata {
                Ok(metadata) => physical.observe(self.id, &metadata),
                Err(_) => physical.error(),
            }
        }
    }

    pub(crate) fn check(&self) -> Result<()> {
        let metadata = self.file.metadata().inspect_err(|_| {
            if let Ok(mut physical) = self.physical.lock() {
                physical.error();
            }
        })?;
        if metadata.len() != self.len.load(Ordering::Relaxed) {
            if let Ok(mut physical) = self.physical.lock() {
                physical.error();
            }
            return Err(StoreError::Integrity("spool segment high-water"));
        }
        Ok(())
    }
}

impl Drop for SpoolSegment {
    fn drop(&mut self) {
        if let Ok(mut physical) = self.physical.lock() {
            physical.removed(self.id);
        }
    }
}

pub(crate) fn spool_segment(backing: &BackingRef) -> Result<&SpoolSegment> {
    backing
        .resource()
        .ok_or(StoreError::Integrity("host spool backing"))
}

/// Read an exact range from a piece backing: host-owned segments read the
/// open descriptor directly; sandbox-owned segments (frozen remote input)
/// fetch through the bounded remote window. The reply length is checked by
/// the transport, so a short remote read surfaces as an integrity error.
pub(crate) fn read_backing_exact(
    segment: &BackingRef,
    output: &mut [u8],
    offset: u64,
) -> Result<()> {
    if let Some(host) = segment.resource::<SpoolSegment>() {
        return read_exact_at(&host.file, output, offset);
    }
    if let Some(remote) = segment.resource::<crate::snapshot_input::RemoteSegment>() {
        let bytes = remote
            .read(offset, output.len() as u64)
            .map_err(|_| StoreError::Integrity("remote segment read"))?;
        if bytes.len() != output.len() {
            return Err(StoreError::Integrity("remote segment read"));
        }
        output.copy_from_slice(&bytes);
        return Ok(());
    }
    Err(StoreError::Integrity("host spool backing"))
}

pub(crate) struct EditCheckpoint {
    node: NodeId,
    value: crate::cow_tree::Node,
    dirty: bool,
    spool_bytes: u64,
    spool_bytes_peak: u64,
    inline_bytes: u64,
    piece_allocation_bytes: u64,
    spool_write_metrics: SpoolWriteMetrics,
    mutation_generation: u64,
    mutation_paths: std::collections::BTreeMap<String, u64>,
}

pub struct ReadPlan {
    reader: SnapshotReader,
    plan: layerfs_workspace_core::ReadPlan,
}

impl ReadPlan {
    pub(crate) fn for_file(
        reader: SnapshotReader,
        data: &FileData,
        offset: u64,
        size: usize,
    ) -> Result<Self> {
        let plan = layerfs_workspace_core::ReadPlan::for_file(data, offset, size)
            .map_err(crate::live_error)?;
        layerfs_layerstack_store::note_workspace_commit_tree_visits(plan.tree_visits as u64);
        Ok(Self { reader, plan })
    }

    pub fn read(self) -> Result<Vec<u8>> {
        let started = std::time::Instant::now();
        let reader = self.reader.clone();
        let output = match self.plan.source {
            ReadSource::Base(root, start, end) => read_base(&self.reader, root, start, end),
            ReadSource::Edited(pieces) => {
                let mut output = Vec::with_capacity(as_usize(self.plan.requested)?);
                for piece in pieces {
                    match piece {
                        Piece::Base { root, offset, len } => {
                            output.extend(read_base(&self.reader, root, offset, offset + len)?)
                        }
                        Piece::Inline { bytes, offset, len } => output
                            .extend_from_slice(&bytes[as_usize(offset)?..as_usize(offset + len)?]),
                        Piece::Zero { len } => output.resize(output.len() + as_usize(len)?, 0),
                        Piece::Spool {
                            segment,
                            offset,
                            len,
                        } => {
                            let start = output.len();
                            output.resize(start + as_usize(len)?, 0);
                            read_backing_exact(&segment, &mut output[start..], offset)?;
                        }
                    }
                }
                Ok(output)
            }
        }?;
        reader.note_workspace_read(
            self.plan.requested,
            output.len() as u64,
            elapsed_ns(started),
        )?;
        Ok(output)
    }
}

impl Workspace {
    pub(crate) fn note_commit_edit_state(&self) -> Result<()> {
        Self::note_edit_state_over(
            &self.live.nodes,
            &self.live.dirty,
            self.live.spool_bytes,
            self.live.spool_bytes_peak,
        )?;
        let (current, peak, errors, observations) = self.physical_spool_snapshot();
        layerfs_layerstack_store::note_workspace_physical_spool(
            current,
            peak,
            errors,
            observations,
        );
        Ok(())
    }

    /// Edit-state telemetry over one node table. Remote workspaces call this
    /// with their materialized frozen input; the sandbox owns the live state.
    pub(crate) fn note_edit_state_over(
        nodes: &std::collections::HashMap<NodeId, Node>,
        dirty: &std::collections::BTreeSet<NodeId>,
        spool_bytes: u64,
        spool_peak: u64,
    ) -> Result<()> {
        let mut edits = 0_u64;
        let mut pieces = 0_u64;
        let mut height = 0_u64;
        let mut charge = 0_u64;
        let mut spool_live = 0_u64;
        let mut metric_nodes_scanned = 0_u64;
        let (nodes, dirty, spool_bytes, spool_peak) = (nodes, dirty, spool_bytes, spool_peak);
        for node in dirty.iter().filter_map(|node| nodes.get(node)) {
            metric_nodes_scanned = metric_nodes_scanned.saturating_add(1);
            if let Data::File(FileData::Edited {
                pieces: tree,
                edits: file_edits,
                ..
            }) = &node.data
            {
                edits = edits.saturating_add(*file_edits);
                pieces = pieces.saturating_add(tree.count() as u64);
                height = height.max(tree.height() as u64);
                charge = charge.saturating_add(
                    tree.logical_allocation_charge()
                        .map_err(crate::live_error)?,
                );
                spool_live = spool_live.saturating_add(tree.spool_len());
            }
        }
        layerfs_layerstack_store::note_workspace_commit_edit_state(
            edits,
            pieces,
            height,
            charge,
            spool_bytes,
            spool_peak,
            spool_live,
            spool_bytes.saturating_sub(spool_live),
            metric_nodes_scanned,
        );
        Ok(())
    }

    pub(crate) fn edit_checkpoint(&self, node: NodeId) -> Result<EditCheckpoint> {
        Ok(EditCheckpoint {
            node,
            value: self
                .live
                .nodes
                .get(&node)
                .ok_or(StoreError::NotFound("node"))?
                .clone(),
            dirty: self.live.dirty.contains(&node),
            spool_bytes: self.live.spool_bytes,
            spool_bytes_peak: self.live.spool_bytes_peak,
            inline_bytes: self.live.inline_bytes,
            piece_allocation_bytes: self.live.piece_allocation_bytes,
            spool_write_metrics: self.backing.metrics,
            mutation_generation: self.live.mutation_generation,
            mutation_paths: self.live.mutation_paths.clone(),
        })
    }

    pub(crate) fn restore_edit(&mut self, checkpoint: EditCheckpoint) -> Result<()> {
        if matches!(checkpoint.value.data, Data::File(FileData::Base { .. })) {
            self.live.edited_nodes.remove(&checkpoint.node);
        } else {
            self.live.edited_nodes.insert(checkpoint.node);
        }
        self.live.nodes.insert(checkpoint.node, checkpoint.value);
        if checkpoint.dirty {
            self.live.dirty.insert(checkpoint.node);
        } else {
            self.live.dirty.remove(&checkpoint.node);
        }
        self.live.spool_bytes = checkpoint.spool_bytes;
        self.live.spool_bytes_peak = checkpoint.spool_bytes_peak;
        self.live.inline_bytes = checkpoint.inline_bytes;
        self.live.piece_allocation_bytes = checkpoint.piece_allocation_bytes;
        self.backing.metrics = checkpoint.spool_write_metrics;
        self.live.mutation_generation = checkpoint.mutation_generation;
        self.live.mutation_paths = checkpoint.mutation_paths;
        self.retire_spool_segments();
        Ok(())
    }

    pub fn read(&self, node: NodeId, offset: u64, size: usize) -> Result<Vec<u8>> {
        self.read_plan(node, offset, size)?.read()
    }
    pub fn read_plan(&self, node: NodeId, offset: u64, size: usize) -> Result<ReadPlan> {
        match &self
            .live
            .nodes
            .get(&node)
            .ok_or(StoreError::NotFound("node"))?
            .data
        {
            Data::File(data) => ReadPlan::for_file(self.reader.clone(), data, offset, size),
            _ => Err(StoreError::InvalidInput("read")),
        }
    }

    pub fn write(&mut self, node: NodeId, offset: u64, bytes: &[u8]) -> Result<usize> {
        self.write_inner(node, offset, bytes.len(), Some(bytes))
    }
    fn write_inner(
        &mut self,
        node: NodeId,
        offset: u64,
        byte_len: usize,
        bytes: Option<&[u8]>,
    ) -> Result<usize> {
        self.ensure_active()?;
        if byte_len == 0 {
            return Ok(0);
        }
        let old_len = self.attr(node)?.size;
        let end = offset
            .checked_add(byte_len as u64)
            .ok_or(StoreError::InvalidInput("write length"))?;
        let physical = if bytes.is_some() {
            Some(self.append_segment(byte_len as u64)?)
        } else {
            None
        };
        let prepared = self
            .live
            .prepare_write(
                node,
                offset,
                byte_len,
                physical.as_ref().map(|(segment, offset)| SpoolSlice {
                    segment: segment.clone(),
                    offset: *offset,
                    len: byte_len as u64,
                }),
            )
            .map_err(crate::live_error)?;
        #[cfg(feature = "test-instrumentation")]
        let appended = bytes.map_or(0, |bytes| bytes.len() as u64);
        #[cfg(feature = "test-instrumentation")]
        if appended > 0
            && crate::lifecycle::consume_verification_fault(
                self.branch_id,
                crate::lifecycle::VerificationFault::NoSpace,
                self.live.spool_bytes,
            )
        {
            let mut lowered = self.live.policy;
            lowered.max_spool_bytes = self.live.spool_bytes;
            lowered
                .check(
                    self.live
                        .spool_bytes
                        .checked_add(appended)
                        .ok_or(StoreError::InvalidInput("workspace spool limit"))?,
                )
                .map_err(crate::live_error)?;
        }
        if let Some(bytes) = bytes {
            #[cfg(feature = "test-instrumentation")]
            let inject_short = crate::lifecycle::consume_verification_fault(
                self.branch_id,
                crate::lifecycle::VerificationFault::ShortAppend,
                self.live.spool_bytes,
            );
            let (backing, physical_start) = physical.as_ref().unwrap();
            self.backing.append(
                backing,
                *physical_start,
                bytes,
                #[cfg(feature = "test-instrumentation")]
                inject_short,
            )?;
        }
        #[cfg(test)]
        if INJECT_STALE_WRITE.with(|inject| inject.replace(false)) {
            self.live
                .set_mtime(node, 123, 0)
                .map_err(crate::live_error)?;
        }
        self.live.apply_edit(prepared).map_err(crate::live_error)?;
        if let Some(bytes) = bytes {
            self.capture_write(node, offset, old_len, bytes);
        }
        debug_assert_eq!(self.attr(node)?.size, old_len.max(end));
        Ok(byte_len)
    }

    pub(crate) fn edit_many(
        &mut self,
        node: NodeId,
        edits: Vec<(u64, u64, crate::WorkspaceFileReplacement)>,
    ) -> Result<()> {
        self.ensure_active()?;
        let replacements = edits
            .into_iter()
            .map(|(start, delete_len, replacement)| {
                let piece = match replacement {
                    crate::WorkspaceFileReplacement::Inline(bytes) => {
                        (!bytes.is_empty()).then(|| Piece::Inline {
                            len: bytes.len() as u64,
                            bytes: Arc::from(bytes),
                            offset: 0,
                        })
                    }
                    crate::WorkspaceFileReplacement::Zero(len) => {
                        (len != 0).then_some(Piece::Zero { len })
                    }
                };
                (start, delete_len, piece)
            })
            .collect();
        let prepared = self
            .live
            .prepare_splices(node, replacements)
            .map_err(crate::live_error)?;
        self.invalidate_capture();
        self.live
            .apply_edit(prepared)
            .map(drop)
            .map_err(crate::live_error)
    }

    pub fn truncate(&mut self, node: NodeId, size: u64) -> Result<()> {
        self.invalidate_capture();
        self.ensure_active()?;
        let Some(prepared) = self
            .live
            .prepare_truncate(node, size)
            .map_err(crate::live_error)?
        else {
            return Ok(());
        };
        for range in prepared.backing_ranges() {
            spool_segment(&range.segment)?.check()?;
        }
        self.live
            .apply_edit(prepared)
            .map(|_| ())
            .map_err(crate::live_error)
    }

    #[cfg(test)]
    pub fn fsync(&mut self, node: Option<NodeId>) -> Result<()> {
        let started = std::time::Instant::now();
        let mut segments = std::collections::BTreeMap::new();
        if let Some(node) = node {
            if let Data::File(FileData::Edited { pieces, .. }) = &self
                .live
                .nodes
                .get(&node)
                .ok_or(StoreError::NotFound("node"))?
                .data
            {
                for piece in pieces.pieces() {
                    if let Piece::Spool { segment, .. } = piece {
                        segments.insert(segment.id(), segment);
                    }
                }
            }
            for segment in segments.values() {
                spool_segment(segment)?.check()?;
                spool_segment(segment)?.observe();
            }
        } else {
            for segment in self.backing.segments.values() {
                spool_segment(segment)?.check()?;
                spool_segment(segment)?.observe();
            }
        }
        self.finish_capture(node);
        self.backing.metrics.fence_count = self.backing.metrics.fence_count.saturating_add(1);
        self.backing.metrics.fence_ns = self
            .backing
            .metrics
            .fence_ns
            .saturating_add(elapsed_ns(started));
        Ok(())
    }
    /// Test-only pending-workspace charge snapshot for capacity diagnostics.
    #[cfg(test)]
    pub(crate) fn pending_charge_snapshot(&self) -> (u64, u64, u64, usize, usize) {
        (
            self.live.piece_allocation_bytes,
            self.live.inline_bytes,
            self.live.spool_bytes,
            self.live.dirty.len(),
            self.live.nodes.len(),
        )
    }

    pub(crate) fn take_spool_write_metrics(&mut self) -> SpoolWriteMetrics {
        std::mem::take(&mut self.backing.metrics)
    }
    pub(crate) fn clear_spool(&mut self) -> Result<()> {
        self.invalidate_capture();
        for id in &self.live.edited_nodes {
            if let Some(Node {
                data: Data::File(FileData::Edited { pieces, .. }),
                ..
            }) = self.live.nodes.get_mut(id)
            {
                *pieces = PieceTree::empty();
            }
        }
        self.live.edited_nodes.clear();
        self.backing.current = None;
        self.backing.segments.clear();
        self.backing.bytes = 0;
        self.live.spool_bytes = 0;
        self.live.spool_bytes_peak = 0;
        self.live.inline_bytes = 0;
        self.live.piece_allocation_bytes = 0;
        Ok(())
    }
    pub(crate) fn physical_spool_snapshot(&self) -> (Option<u64>, Option<u64>, u64, u64) {
        // The remote workspace's physical spool lives in the sandbox; its
        // bytes are observed through the daemon (OBSERVE) rather than here.
        if self.remote.is_some() {
            return (None, None, 0, 0);
        }
        self.backing
            .physical
            .lock()
            .map_or((None, None, 1, 0), |metrics| metrics.snapshot())
    }

    fn append_segment(&mut self, bytes: u64) -> Result<(BackingRef, u64)> {
        self.backing
            .reserve_append(&self.spool, bytes, self.live.spool_bytes, self.live.policy)
    }

    pub(crate) fn retire_spool_segments(&mut self) {
        self.backing.retire();
    }
}

fn elapsed_ns(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
fn append_spool(file: &File, bytes: &[u8], offset: u64) -> std::io::Result<()> {
    #[cfg(test)]
    if INJECT_SHORT_APPEND.with(|inject| inject.replace(false)) {
        file.write_all_at(&bytes[..bytes.len() / 2], offset)?;
        return Err(std::io::Error::other("injected short spool append"));
    }
    file.write_all_at(bytes, offset)
}

#[cfg(test)]
thread_local! {
    static INJECT_SHORT_APPEND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static INJECT_APPEND_CLEANUP_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static INJECT_STALE_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
fn read_base(
    reader: &SnapshotReader,
    root: layerfs_content::file::content::FileContentRoot,
    start: u64,
    end: u64,
) -> Result<Vec<u8>> {
    if start >= end {
        return Ok(Vec::new());
    }
    let mut bytes = Vec::with_capacity(as_usize(end - start)?);
    let counters = read_range(&CoreReader(reader), root, start..end, &mut bytes)?;
    reader.note_rope_read(counters)?;
    Ok(bytes)
}
fn read_exact_at(file: &File, mut output: &mut [u8], mut offset: u64) -> Result<()> {
    while !output.is_empty() {
        let read = file.read_at(output, offset)?;
        if read == 0 {
            return Err(StoreError::Integrity("spool eof"));
        }
        offset += read as u64;
        output = &mut output[read..];
    }
    Ok(())
}

fn as_usize(value: u64) -> Result<usize> {
    usize::try_from(value).map_err(|_| StoreError::InvalidInput("file range"))
}

impl HostSpool {
    pub(crate) fn reserve_append(
        &mut self,
        directory: &Path,
        bytes: u64,
        logical_bytes: u64,
        policy: crate::ResourcePolicy,
    ) -> Result<(BackingRef, u64)> {
        if self.bytes.saturating_add(bytes) > policy.max_spool_bytes {
            self.retire();
        }
        policy
            .check(
                logical_bytes
                    .max(self.bytes)
                    .checked_add(bytes)
                    .ok_or(StoreError::InvalidInput("workspace spool limit"))?,
            )
            .map_err(crate::live_error)?;
        if let Some(segment) = self.current.and_then(|id| self.segments.get(&id)) {
            let physical = spool_segment(segment)?;
            let offset = physical.len.load(Ordering::Relaxed);
            if offset.saturating_add(bytes) <= physical.capacity {
                return Ok((segment.clone(), offset));
            }
        }
        self.retire();
        let started = std::time::Instant::now();
        let id = self.next_id;
        self.next_id = id
            .checked_add(1)
            .ok_or(StoreError::Integrity("spool segment identity"))?;
        let segment = BackingRef::new(
            BackingId(id),
            SpoolSegment::new(
                directory,
                id,
                SPOOL_SEGMENT_BYTES.max(bytes).min(policy.max_spool_bytes),
                self.physical.clone(),
            )?,
        );
        self.segments.insert(id, segment.clone());
        self.current = Some(id);
        self.note_open(elapsed_ns(started));
        Ok((segment, 0))
    }

    pub(crate) fn retire(&mut self) {
        let started = std::time::Instant::now();
        let mut scan_ns = 0_u64;
        let mut retired = 0_u64;
        self.segments.retain(|id, segment| {
            let scan_started = std::time::Instant::now();
            let keep = !segment.is_unique();
            if !keep {
                self.bytes = self.bytes.saturating_sub(
                    spool_segment(segment)
                        .expect("host segment registry")
                        .len
                        .load(Ordering::Relaxed),
                );
                if self.current == Some(*id) {
                    self.current = None;
                }
                retired += 1;
            }
            scan_ns = scan_ns.saturating_add(elapsed_ns(scan_started));
            keep
        });
        layerfs_layerstack_store::note_workspace_spool_retirement(
            elapsed_ns(started),
            scan_ns,
            retired,
        );
    }
    fn note_open(&mut self, ns: u64) {
        self.metrics.write_open_count = self.metrics.write_open_count.saturating_add(1);
        self.metrics.write_ns = self.metrics.write_ns.saturating_add(ns);
    }
}

impl HostSpool {
    pub(crate) fn append(
        &mut self,
        backing: &BackingRef,
        physical_start: u64,
        bytes: &[u8],
        #[cfg(feature = "test-instrumentation")] inject_short: bool,
    ) -> Result<()> {
        let started = std::time::Instant::now();
        let appended = bytes.len() as u64;
        if self.segments.get(&backing.id().0) != Some(backing) {
            return Err(StoreError::Integrity("spool append ownership"));
        }
        let segment = spool_segment(backing)?;
        if physical_start != segment.len.load(Ordering::Relaxed)
            || physical_start
                .checked_add(appended)
                .is_none_or(|end| end > segment.capacity)
        {
            return Err(StoreError::Integrity("spool append range"));
        }

        segment.check()?;
        let file = &segment.file;
        #[cfg(feature = "test-instrumentation")]
        let append = if inject_short {
            file.write_all_at(&bytes[..bytes.len() / 2], physical_start)
                .and_then(|_| Err(std::io::Error::other("injected short spool append")))
        } else {
            append_spool(file, bytes, physical_start)
        };
        #[cfg(not(feature = "test-instrumentation"))]
        let append = append_spool(file, bytes, physical_start);
        segment.observe();
        if let Err(error) = append {
            // No visible range references this tail; other files' earlier
            // bytes in the shared segment must never be truncated.
            #[cfg(test)]
            let cleanup = if INJECT_APPEND_CLEANUP_FAILURE.with(|inject| inject.replace(false)) {
                Err(std::io::Error::other("injected spool rollback failure"))
            } else {
                file.set_len(physical_start)
            };
            #[cfg(not(test))]
            let cleanup = file.set_len(physical_start);
            segment.observe();
            if cleanup.is_err() {
                let retained = file.metadata()?.len();
                let extra = retained
                    .checked_sub(physical_start)
                    .ok_or(StoreError::Integrity("spool append cleanup length"))?;
                segment.len.store(retained, Ordering::Relaxed);
                self.bytes = self
                    .bytes
                    .checked_add(extra)
                    .ok_or(StoreError::Integrity("spool segment charge"))?;
                return Err(StoreError::Integrity("spool append cleanup failure"));
            }
            return Err(error.into());
        }
        segment
            .len
            .store(physical_start + appended, Ordering::Relaxed);
        self.bytes += appended;
        self.metrics.write_bytes = self.metrics.write_bytes.saturating_add(appended);
        self.metrics.write_ns = self.metrics.write_ns.saturating_add(elapsed_ns(started));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ROOT;
    use layerfs_layerstack_store::{
        EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
    };

    #[test]
    fn backing_service_appends_only_its_exact_reserved_range_without_live_state() {
        let root =
            std::env::temp_dir().join(format!("layerfs-host-spool-{}", crate::WorkspaceId::new()));
        std::fs::create_dir(&root).unwrap();
        let mut spool = HostSpool::default();
        let mut foreign = HostSpool::default();
        let (backing, start) = spool
            .reserve_append(&root, 3, 0, crate::ResourcePolicy::default())
            .unwrap();
        assert!(foreign
            .append(
                &backing,
                start,
                b"bad",
                #[cfg(feature = "test-instrumentation")]
                false,
            )
            .is_err());
        spool
            .append(
                &backing,
                start,
                b"abc",
                #[cfg(feature = "test-instrumentation")]
                false,
            )
            .unwrap();
        assert!(spool
            .append(
                &backing,
                start,
                b"bad",
                #[cfg(feature = "test-instrumentation")]
                false,
            )
            .is_err());
        assert_eq!(spool.bytes, 3);
        let mut bytes = [0; 3];
        read_exact_at(&spool_segment(&backing).unwrap().file, &mut bytes, 0).unwrap();
        assert_eq!(&bytes, b"abc");
        spool.retire();
        assert_eq!(spool.bytes, 3);
        drop(backing);
        spool.retire();
        assert_eq!(spool.bytes, 0);
        assert!(spool.segments.is_empty());
        drop(spool);
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn stale_prepared_append_is_not_repeated_and_keeps_its_physical_charge() {
        let (root, mut workspace) = workspace("stale-prepared-append");
        let file = workspace.create_file(ROOT, b"file", 0o600).unwrap().node;
        workspace.live.policy.max_spool_bytes = 9;
        workspace.write(file, 0, b"hello").unwrap();
        let held = workspace.read_plan(file, 0, 5).unwrap();
        INJECT_STALE_WRITE.with(|inject| inject.set(true));
        assert!(matches!(
            workspace.write(file, 0, b"bad"),
            Err(StoreError::Integrity("stale prepared write"))
        ));
        assert_eq!(workspace.read(file, 0, 5).unwrap(), b"hello");
        assert_eq!(workspace.live.spool_bytes, 5);
        assert_eq!(workspace.backing.bytes, 8);
        assert_eq!(workspace.backing.metrics.write_bytes, 8);
        assert!(workspace.write(file, 0, b"no").is_err());
        workspace.commit().unwrap();
        assert_eq!(
            workspace.backing.bytes, 8,
            "old reader retains the whole segment"
        );
        assert_eq!(held.read().unwrap(), b"hello");
        workspace.commit().unwrap();
        assert_eq!(workspace.backing.bytes, 0);
        workspace.write(file, 0, b"ok").unwrap();
        assert_eq!(workspace.read(file, 0, 5).unwrap(), b"okllo");
        workspace.discard().unwrap();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn workspace(label: &str) -> (std::path::PathBuf, Workspace) {
        let root = std::env::temp_dir().join(format!(
            "layerfs-file-edit-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Empty,
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let workspace = Workspace::open(store, branch, root.join("spool")).unwrap();
        (root, workspace)
    }

    fn workspace_with_file(
        label: &str,
    ) -> (
        std::path::PathBuf,
        Workspace,
        layerfs_layerstack_store::BranchId,
    ) {
        let root = std::env::temp_dir().join(format!(
            "layerfs-file-edit-source-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("file"), b"abcdefghij").unwrap();
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Directory(source),
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let workspace = Workspace::open(store, branch, root.join("spool")).unwrap();
        (root, workspace, branch)
    }

    #[test]
    fn failed_tail_cleanup_keeps_discarded_physical_bytes_charged() {
        let (root, mut workspace) = workspace("failed-tail-cleanup");
        workspace.live.policy.max_spool_bytes = 6;
        let file = workspace.create_file(ROOT, b"file", 0o600).unwrap().node;
        workspace.write(file, 0, b"ok").unwrap();
        INJECT_SHORT_APPEND.with(|inject| inject.set(true));
        INJECT_APPEND_CLEANUP_FAILURE.with(|inject| inject.set(true));
        assert!(matches!(
            workspace.write(file, 2, b"bad!"),
            Err(StoreError::Integrity("spool append cleanup failure"))
        ));
        assert_eq!(workspace.read(file, 0, 2).unwrap(), b"ok");
        assert_eq!(workspace.live.spool_bytes, 2);
        assert_eq!(workspace.backing.bytes, 4);
        workspace.live.policy.max_spool_bytes = 4;
        assert!(workspace.write(file, 2, b"x").is_err());
        workspace.commit().unwrap();
        assert_eq!(workspace.backing.bytes, 0);
        workspace.write(file, 2, b"x").unwrap();
        assert_eq!(workspace.read(file, 0, 3).unwrap(), b"okx");
        workspace.discard().unwrap();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shared_segments_keep_interleaved_rollback_reads_and_open_unlinked_lifetime() {
        let (root, mut workspace) = workspace("shared-segment-lifetime");
        let a = workspace.create_file(ROOT, b"a", 0o600).unwrap().node;
        let b = workspace.create_file(ROOT, b"b", 0o600).unwrap().node;
        workspace.write(a, 0, b"abc").unwrap();
        workspace.write(b, 0, b"XYZ").unwrap();
        workspace.write(a, 3, b"def").unwrap();
        assert_eq!(workspace.backing.segments.len(), 1);
        let held = workspace.read_plan(a, 0, 6).unwrap();
        workspace.write(a, 0, b"Q").unwrap();
        workspace.truncate(b, 2).unwrap();
        workspace.write(b, 5, b"R").unwrap();
        let end = workspace.backing.bytes;
        INJECT_SHORT_APPEND.with(|inject| inject.set(true));
        assert!(workspace.write(a, 6, b"failed tail").is_err());
        assert_eq!(workspace.backing.bytes, end);
        assert_eq!(workspace.read(a, 0, 6).unwrap(), b"Qbcdef");
        assert_eq!(workspace.read(b, 0, 6).unwrap(), b"XY\0\0\0R");
        workspace.pin(a, false).unwrap();
        workspace.unlink(ROOT, b"a", false).unwrap();
        let c = workspace.create_file(ROOT, b"c", 0o600).unwrap().node;
        workspace
            .write(c, 0, &vec![7; SPOOL_SEGMENT_BYTES as usize])
            .unwrap();
        assert_eq!(workspace.backing.segments.len(), 2);
        workspace.fsync(None).unwrap();
        workspace.commit().unwrap();
        assert_eq!(workspace.backing.segments.len(), 1);
        assert_eq!(
            workspace.backing.bytes, end,
            "retained segment dead bytes remain charged"
        );
        assert_eq!(workspace.read(a, 0, 6).unwrap(), b"Qbcdef");
        workspace.chmod(c, 0o640).unwrap();
        workspace.commit().unwrap();
        workspace.unpin(a).unwrap();
        assert_eq!(
            workspace.backing.segments.len(),
            1,
            "prepared read owns old extents"
        );
        assert_eq!(held.read().unwrap(), b"abcdef");
        workspace.commit().unwrap();
        assert!(workspace.backing.segments.is_empty());
        assert_eq!(workspace.backing.bytes, 0);
        assert_eq!(workspace.physical_spool_snapshot().0, Some(0));
        workspace.end_clean().unwrap();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn edit_limit_and_sparse_physical_charge_are_exact() {
        let (root, mut workspace) = workspace("limits");
        let sparse = workspace.create_file(ROOT, b"sparse", 0o600).unwrap().node;
        workspace.write(sparse, 60 * 1024, b"x").unwrap();
        assert_eq!(workspace.attr(sparse).unwrap().size, 60 * 1024 + 1);
        assert_eq!(workspace.live.spool_bytes, 1);
        assert_eq!(
            workspace
                .backing
                .segments
                .values()
                .map(|segment| spool_segment(segment)
                    .unwrap()
                    .file
                    .metadata()
                    .unwrap()
                    .len())
                .sum::<u64>(),
            1
        );

        let file = workspace.create_file(ROOT, b"limit", 0o600).unwrap().node;
        // Rewriting one byte in place coalesces to a single piece, so the
        // cumulative edit count grows past the old 4,096 counter without any
        // piece, inline, spool or allocation budget being reached. The last
        // write still succeeds and the file keeps its exact contents.
        let rewrites = 5_000u32;
        for value in 0..rewrites {
            workspace.write(file, 0, &[(value & 0xff) as u8]).unwrap();
        }
        assert_eq!(
            workspace.read(file, 0, 1).unwrap(),
            vec![((rewrites - 1) & 0xff) as u8]
        );
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// #116 Phase 1: locate the pending-workspace ceiling for many distinct
    /// changed files with the workload's real 4 KiB inline replacement.
    /// Explicit selection only.
    #[test]
    #[ignore = "issue116 phase-1 capacity probe"]
    fn issue116_many_changed_files_charge_probe() {
        let (root, mut workspace) = workspace("many-changed-files");
        let payload = vec![7u8; 4096];
        let mut accepted = 0u32;
        for index in 0..20_000u32 {
            let name = format!("f{index}");
            let node = workspace
                .create_file(ROOT, name.as_bytes(), 0o600)
                .unwrap()
                .node;
            match workspace.write(node, 0, &payload) {
                Ok(_) => {
                    accepted += 1;
                    if accepted % 500 == 0 {
                        let (charge, inline, spool, dirty, nodes) =
                            workspace.pending_charge_snapshot();
                        println!(
                            "PROBE CHG accepted={accepted} piece_charge={charge} inline={inline} spool={spool} dirty={dirty} nodes={nodes}"
                        );
                    }
                }
                Err(error) => {
                    let (charge, inline, spool, dirty, nodes) = workspace.pending_charge_snapshot();
                    println!(
                        "PROBE CHG rejected_at={index} accepted={accepted} error={error:?} piece_charge={charge} inline={inline} spool={spool} dirty={dirty} nodes={nodes}"
                    );
                    break;
                }
            }
        }
        let (charge, inline, spool, dirty, nodes) = workspace.pending_charge_snapshot();
        println!(
            "PROBE CHG end accepted={accepted} piece_charge={charge} inline={inline} spool={spool} dirty={dirty} nodes={nodes}"
        );
        drop(workspace);
        let _ = std::fs::remove_dir_all(root);
    }

    /// #116 Phase 1: locate the pending-workspace ceiling for the exact
    /// sequence edit shape (a small inline splice inside a larger base file).
    /// Explicit selection only.
    #[test]
    #[ignore = "issue116 phase-1 capacity probe"]
    fn issue116_splice_shape_charge_probe() {
        let (root, mut workspace) = workspace("splice-shape");
        let base = vec![5u8; 49_152];
        let mut accepted = 0u32;
        for index in 0..20_000u32 {
            let name = format!("f{index}");
            let node = workspace
                .create_file(ROOT, name.as_bytes(), 0o600)
                .unwrap()
                .node;
            workspace.write(node, 0, &base).unwrap();
            match workspace.write(node, 1000, b"C6000000001") {
                Ok(_) => {
                    accepted += 1;
                    if accepted % 250 == 0 {
                        let (charge, inline, spool, dirty, nodes) =
                            workspace.pending_charge_snapshot();
                        println!(
                            "PROBE SPLICE accepted={accepted} piece_charge={charge} inline={inline} spool={spool} dirty={dirty} nodes={nodes}"
                        );
                    }
                }
                Err(error) => {
                    let (charge, inline, spool, dirty, nodes) = workspace.pending_charge_snapshot();
                    println!(
                        "PROBE SPLICE rejected_at={index} accepted={accepted} error={error:?} piece_charge={charge} inline={inline} spool={spool} dirty={dirty} nodes={nodes}"
                    );
                    break;
                }
            }
        }
        let (charge, inline, spool, dirty, nodes) = workspace.pending_charge_snapshot();
        println!(
            "PROBE SPLICE end accepted={accepted} piece_charge={charge} inline={inline} spool={spool} dirty={dirty} nodes={nodes}"
        );
        drop(workspace);
        let _ = std::fs::remove_dir_all(root);
    }

    /// #116 Phase 1: per-file pending piece charge for the sequence edit shape.
    #[test]
    #[ignore = "issue116 phase-1 capacity probe"]
    fn issue116_per_file_charge_probe() {
        let (root, mut workspace) = workspace("per-file-charge");
        for (label, size, offset) in [
            ("tiny", 4096u64, 1024u64),
            ("medium", 49_152, 1000),
            ("anchor", 200_000_000, 1000),
        ] {
            let base = vec![5u8; size as usize];
            let node = workspace
                .create_file(ROOT, label.as_bytes(), 0o600)
                .unwrap()
                .node;
            workspace.write(node, 0, &base).unwrap();
            let (before, _, _, _, _) = workspace.pending_charge_snapshot();
            workspace.write(node, offset, b"C6000000001").unwrap();
            let (after, _, _, _, _) = workspace.pending_charge_snapshot();
            println!(
                "PROBE PERFILE label={label} size={size} delta={} cumulative={after}",
                after - before
            );
        }
        drop(workspace);
        let _ = std::fs::remove_dir_all(root);
    }

    /// #116 RCA: exact per-file pending structure and charge decomposition.
    #[test]
    #[ignore = "issue116 root-cause probe"]
    fn issue116_piece_charge_rca() {
        use layerfs_workspace_core::file_edit::{Piece, PieceTree};
        println!(
            "RCA size_of Piece={} ObjectId={} ArcSlice={} OptionArcSpoolSlice={} BTreeSet<String>={} Data={} FileData={} Node={}",
            std::mem::size_of::<Piece>(),
            std::mem::size_of::<layerfs_content::ObjectId>(),
            std::mem::size_of::<std::sync::Arc<[u8]>>(),
            std::mem::size_of::<Option<std::sync::Arc<SpoolSlice>>>(),
            std::mem::size_of::<std::collections::BTreeSet<String>>(),
            std::mem::size_of::<Data>(),
            std::mem::size_of::<FileData>(),
            std::mem::size_of::<layerfs_workspace_core::Node>(),
        );

        let (root, mut workspace) = workspace("piece-rca");
        let base = vec![5u8; 49_152];
        let node = workspace.create_file(ROOT, b"f0", 0o600).unwrap().node;
        workspace.write(node, 0, &base).unwrap();
        let (charge, _, _, _, _) = workspace.pending_charge_snapshot();
        println!("RCA after base write: charge={charge}");
        let _ = workspace.write(node, 1000, b"C6000000001").unwrap();
        let (charge, inline, spool, _, _) = workspace.pending_charge_snapshot();
        println!("RCA after 12-byte splice: charge={charge} inline={inline} spool={spool}");
        {
            let Data::File(FileData::Edited { pieces, edits, .. }) =
                &workspace.live.nodes[&node].data
            else {
                unreachable!()
            };
            println!(
                "RCA tree count={} len={} inline_len={} spool_len={} height={} edits={edits} charge={}",
                pieces.count(),
                pieces.len(),
                pieces.inline_len(),
                pieces.spool_len(),
                pieces.height(),
                pieces.logical_allocation_charge().unwrap()
            );
            for (index, piece) in pieces.pieces().iter().enumerate() {
                println!("RCA piece[{index}] = {piece:?}");
            }
        }

        // Second splice at a different offset in the same file.
        let _ = workspace.write(node, 20_000, b"C6000000002").unwrap();
        let (charge2, _, _, _, _) = workspace.pending_charge_snapshot();
        {
            let Data::File(FileData::Edited { pieces, edits, .. }) =
                &workspace.live.nodes[&node].data
            else {
                unreachable!()
            };
            println!(
                "RCA after second splice: charge={charge2} count={} edits={edits}",
                pieces.count()
            );
            for (index, piece) in pieces.pieces().iter().enumerate() {
                println!("RCA piece2[{index}] = {piece:?}");
            }
        }

        // Re-splicing the exact same range repeatedly must not grow the tree.
        for index in 0..5u32 {
            let _ = workspace
                .write(node, 1000, format!("C600000{index:04}").as_bytes())
                .unwrap();
        }
        let (charge3, _, _, _, _) = workspace.pending_charge_snapshot();
        {
            let Data::File(FileData::Edited { pieces, edits, .. }) =
                &workspace.live.nodes[&node].data
            else {
                unreachable!()
            };
            println!(
                "RCA after 5 same-range resplices: charge={charge3} count={} edits={edits}",
                pieces.count()
            );
        }

        // A whole-file inline replacement cannot stay compact: confirm the shape.
        let node2 = workspace.create_file(ROOT, b"f1", 0o600).unwrap().node;
        workspace.write(node2, 0, &base).unwrap();
        let before = workspace.pending_charge_snapshot().0;
        let _ = workspace.write(node2, 0, &vec![9u8; 49_152]).unwrap();
        let after = workspace.pending_charge_snapshot().0;
        {
            let Data::File(FileData::Edited { pieces, .. }) = &workspace.live.nodes[&node2].data
            else {
                unreachable!()
            };
            println!(
                "RCA whole-file inline replace: delta={} count={} pieces={:?}",
                after - before,
                pieces.count(),
                pieces.pieces().len()
            );
        }
        drop(workspace);
        let _ = std::fs::remove_dir_all(root);
    }

    /// #116 RCA: which budget binds next, using the product's own fact-charge
    /// formula (live_wire::node_encoded_bound, charged 8x + 1024 per node).
    #[test]
    #[ignore = "issue116 root-cause probe"]
    fn issue116_fact_charge_probe() {
        use layerfs_workspace_core::{Data, FileData};
        fn encoded_bound(node: &layerfs_workspace_core::Node) -> usize {
            let paths = node.paths.iter().map(|p| 4 + p.len()).sum::<usize>();
            let data = match &node.data {
                Data::File(FileData::Edited { pieces, .. }) => {
                    pieces.count() * 49 + usize::try_from(pieces.inline_len()).unwrap()
                }
                Data::Directory(d) => d.changes.keys().map(|n| 12 + n.len()).sum(),
                Data::Symlink(target) => target.len(),
                Data::File(_) => 0,
            };
            128 + paths + data
        }
        let (root, mut workspace) = workspace("fact-charge");
        let base = vec![5u8; 49_152];
        let mut previous = 0u64;
        let mut directories = std::collections::BTreeMap::new();
        for index in 0..12_000u32 {
            let group = index / 100;
            let directory = match directories.get(&group) {
                Some(node) => *node,
                None => {
                    let name = format!("d{group:04}");
                    let node = workspace.mkdir(ROOT, name.as_bytes(), 0o750).unwrap().node;
                    directories.insert(group, node);
                    node
                }
            };
            let name = format!("f{index:06}");
            let node = match workspace.create_file(directory, name.as_bytes(), 0o600) {
                Ok(attr) => attr.node,
                Err(error) => {
                    println!("RCA FACT create_rejected_at={index} error={error:?}");
                    break;
                }
            };
            workspace.write(node, 0, &base).unwrap();
            if let Err(error) = workspace.write(node, 1000, b"C6000000001") {
                println!("RCA FACT write_rejected_at={index} error={error:?}");
                break;
            }
            if index % 1_000 == 0 || index == 11_999 {
                let mut fact = 0u64;
                let mut files = 0u64;
                for id in workspace.live.dirty.iter() {
                    let Some(node) = workspace.live.nodes.get(id) else {
                        continue;
                    };
                    if matches!(node.data, Data::File(FileData::Edited { .. })) {
                        files += 1;
                        fact += (encoded_bound(node) as u64) * 8 + 1024;
                    }
                }
                let (charge, _, _, dirty, _) = workspace.pending_charge_snapshot();
                println!(
                    "RCA FACT files={files} piece_charge={charge} fact_charge={fact} fact_per_file={} dirty={dirty} delta_since_last={}",
                    fact / files.max(1),
                    fact - previous
                );
                previous = fact;
            }
        }
        drop(workspace);
        let _ = std::fs::remove_dir_all(root);
    }

    /// #116 RCA: real resident cost of a large pending set versus the charged
    /// budgets, so the ceilings can be compared with actual memory.
    #[test]
    #[ignore = "issue116 root-cause probe"]
    fn issue116_pending_set_rss_probe() {
        use layerfs_workspace_core::{Data, FileData};
        fn rss_bytes() -> u64 {
            // macOS has no /proc; ask ps for this process's resident set size.
            let output = std::process::Command::new("ps")
                .args(["-o", "rss=", "-p", &std::process::id().to_string()])
                .output();
            match output {
                Ok(output) => String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<u64>()
                    .map(|kib| kib * 1024)
                    .unwrap_or(0),
                Err(_) => 0,
            }
        }
        fn encoded_bound(node: &layerfs_workspace_core::Node) -> usize {
            let paths = node.paths.iter().map(|p| 4 + p.len()).sum::<usize>();
            let data = match &node.data {
                Data::File(FileData::Edited { pieces, .. }) => {
                    pieces.count() * 49 + usize::try_from(pieces.inline_len()).unwrap()
                }
                Data::Directory(d) => d.changes.keys().map(|n| 12 + n.len()).sum(),
                Data::Symlink(target) => target.len(),
                Data::File(_) => 0,
            };
            128 + paths + data
        }
        let (root, mut workspace) = workspace("rss");
        let base = vec![5u8; 49_152];
        println!("RCA RSS baseline peak={} B", rss_bytes());
        let mut directories = std::collections::BTreeMap::new();
        for index in 0..20_000u32 {
            let group = index / 100;
            let directory = match directories.get(&group) {
                Some(node) => *node,
                None => {
                    let name = format!("d{group:04}");
                    let node = workspace.mkdir(ROOT, name.as_bytes(), 0o750).unwrap().node;
                    directories.insert(group, node);
                    node
                }
            };
            let name = format!("f{index:06}");
            let node = workspace
                .create_file(directory, name.as_bytes(), 0o600)
                .unwrap()
                .node;
            workspace.write(node, 0, &base).unwrap();
            if let Err(error) = workspace.write(node, 1000, b"C6000000001") {
                println!("RCA RSS rejected_at={index} error={error:?}");
                break;
            }
            if index % 1_000 == 0 {
                let mut fact = 0u64;
                let mut files = 0u64;
                let mut nodes = 0u64;
                for id in workspace.live.dirty.iter() {
                    let Some(node) = workspace.live.nodes.get(id) else {
                        continue;
                    };
                    nodes += 1;
                    if matches!(node.data, Data::File(FileData::Edited { .. })) {
                        files += 1;
                        fact += (encoded_bound(node) as u64) * 8 + 1024;
                    }
                }
                let (charge, _, _, dirty, _) = workspace.pending_charge_snapshot();
                println!(
                    "RCA RSS files={files} dirty_nodes={nodes} piece_charge={charge} fact_charge={fact} peak_rss={} bytes_per_file={}",
                    rss_bytes(),
                    rss_bytes() / files.max(1)
                );
            }
        }
        drop(workspace);
        let _ = std::fs::remove_dir_all(root);
    }

    /// #116 RCA: how far the compact single-range form scales, to contrast the
    /// 5,461 splice ceiling with the other per-file shapes.
    #[test]
    #[ignore = "issue116 root-cause probe"]
    fn issue116_compact_form_scaling_probe() {
        let (root, mut workspace) = workspace("compact-scaling");
        let payload = vec![7u8; 4096];
        let mut accepted = 0u32;
        for index in 0..400_000u32 {
            let name = format!("f{index}");
            let node = match workspace.create_file(ROOT, name.as_bytes(), 0o600) {
                Ok(attr) => attr.node,
                Err(error) => {
                    println!("RCA COMPACT create_rejected_at={index} error={error:?}");
                    break;
                }
            };
            // One contiguous write: the whole file stays a single spool range.
            match workspace.write(node, 0, &payload) {
                Ok(_) => {
                    accepted += 1;
                    if accepted % 50_000 == 0 {
                        let (charge, _, spool, dirty, nodes) = workspace.pending_charge_snapshot();
                        println!(
                            "RCA COMPACT accepted={accepted} piece_charge={charge} spool={spool} dirty={dirty} nodes={nodes}"
                        );
                    }
                }
                Err(error) => {
                    let (charge, _, _, _, _) = workspace.pending_charge_snapshot();
                    println!(
                        "RCA COMPACT rejected_at={index} accepted={accepted} error={error:?} piece_charge={charge}"
                    );
                    break;
                }
            }
        }
        println!("RCA COMPACT end accepted={accepted}");
        drop(workspace);
        let _ = std::fs::remove_dir_all(root);
    }

    /// #116 RCA: the fact charge counts pieces too, so a cheaper per-file
    /// representation lowers both budgets. Measures the compact shape's real
    /// fact charge and the ceiling each budget implies.
    #[test]
    #[ignore = "issue116 root-cause probe"]
    fn issue116_fact_ceiling_probe() {
        use layerfs_workspace_core::{Data, FileData};
        fn fact_charge(node: &layerfs_workspace_core::Node) -> u64 {
            let paths = node.paths.iter().map(|p| 4 + p.len()).sum::<usize>();
            let (pieces, inline) = match &node.data {
                Data::File(FileData::Edited { pieces, .. }) => (
                    pieces.count(),
                    usize::try_from(pieces.inline_len()).unwrap(),
                ),
                _ => (0, 0),
            };
            let bound = 128 + paths + pieces * 49 + inline;
            ((bound - inline) as u64) * 8 + 1024
        }
        let (root, mut ws) = workspace("fact-ceiling");
        let base = vec![5u8; 49_152];
        // Shape A: one contiguous write per file.
        for index in 0..4_000u32 {
            let name = format!("a{index:06}");
            let node = ws.create_file(ROOT, name.as_bytes(), 0o600).unwrap().node;
            ws.write(node, 0, &base).unwrap();
            if index % 1_000 == 999 {
                let mut total = 0u64;
                let mut files = 0u64;
                let mut pieces = 0u64;
                for id in ws.live.dirty.iter() {
                    let Some(node) = ws.live.nodes.get(id) else {
                        continue;
                    };
                    if let Data::File(FileData::Edited { pieces: tree, .. }) = &node.data {
                        files += 1;
                        pieces += tree.count() as u64;
                        total += fact_charge(node);
                    }
                }
                println!(
                    "RCA FACTCEIL shape=compact files={files} pieces={pieces} fact_charge={total} per_file={} charge={}",
                    total / files.max(1),
                    ws.pending_charge_snapshot().0
                );
            }
        }
        // Shape B: one contiguous write plus one small splice per file.
        for index in 0..4_000u32 {
            let name = format!("g{index:06}");
            let node = ws.create_file(ROOT, name.as_bytes(), 0o600).unwrap().node;
            ws.write(node, 0, &base).unwrap();
            ws.write(node, 1000, b"C6000000001").unwrap();
            if index % 1_000 == 999 {
                let mut total = 0u64;
                let mut files = 0u64;
                let mut pieces = 0u64;
                for id in ws.live.dirty.iter() {
                    let Some(node) = ws.live.nodes.get(id) else {
                        continue;
                    };
                    if let Data::File(FileData::Edited { pieces: tree, .. }) = &node.data {
                        files += 1;
                        pieces += tree.count() as u64;
                        total += fact_charge(node);
                    }
                }
                println!(
                    "RCA FACTCEIL shape=splice files={files} pieces={pieces} fact_charge={total} per_file={} charge={}",
                    total / files.max(1),
                    ws.pending_charge_snapshot().0
                );
            }
        }
        drop(ws);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn short_spool_append_restores_high_water_and_piece_root() {
        let (root, mut workspace) = workspace("short-append");
        let file = workspace.create_file(ROOT, b"file", 0o600).unwrap().node;
        workspace.write(file, 0, b"base").unwrap();
        let before = workspace.live.nodes[&file].clone();
        let before_charge = workspace.live.spool_bytes;
        let before_peak = workspace.live.spool_bytes_peak;
        INJECT_SHORT_APPEND.with(|inject| inject.set(true));
        assert!(workspace.write(file, 4, b"failure").is_err());
        assert_eq!(workspace.live.nodes[&file], before);
        assert_eq!(workspace.live.spool_bytes, before_charge);
        assert_eq!(workspace.live.spool_bytes_peak, before_peak);
        assert_eq!(
            workspace
                .backing
                .segments
                .values()
                .map(|segment| spool_segment(segment)
                    .unwrap()
                    .file
                    .metadata()
                    .unwrap()
                    .len())
                .sum::<u64>(),
            before_charge
        );
        assert_eq!(workspace.read(file, 0, 16).unwrap(), b"base");
        workspace.live.policy.max_spool_bytes = before_charge;
        assert!(workspace.write(file, 4, b"x").is_err());
        assert_eq!(workspace.live.spool_bytes, before_charge);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn physical_spool_peak_survives_short_append_reclaim_and_discard() {
        let (root, mut workspace) = workspace("physical-spool-accounting");
        let first = workspace.create_file(ROOT, b"first", 0o600).unwrap().node;
        let second = workspace.create_file(ROOT, b"second", 0o600).unwrap().node;
        workspace.write(first, 0, b"a").unwrap();
        workspace.write(second, 0, &[0x53; 8192]).unwrap();
        let actual = || {
            workspace
                .backing
                .segments
                .values()
                .map(|f| spool_segment(f).unwrap().file.metadata().unwrap().blocks() * 512)
                .sum::<u64>()
        };
        assert_eq!(workspace.physical_spool_snapshot().0, Some(actual()));
        assert_eq!(
            workspace.backing.segments.len(),
            1,
            "two files share one physical segment"
        );
        let initial_peak = workspace.physical_spool_snapshot().1.unwrap();
        let failed = workspace.create_file(ROOT, b"failed", 0o600).unwrap().node;
        let logical_before = (workspace.live.spool_bytes, workspace.live.spool_bytes_peak);
        INJECT_SHORT_APPEND.with(|inject| inject.set(true));
        assert!(workspace.write(failed, 0, &vec![0xa5; 256 * 1024]).is_err());
        assert_eq!(
            (workspace.live.spool_bytes, workspace.live.spool_bytes_peak),
            logical_before
        );
        assert_eq!(workspace.attr(failed).unwrap().size, 0);
        assert_eq!(
            workspace
                .backing
                .segments
                .values()
                .map(|segment| spool_segment(segment)
                    .unwrap()
                    .file
                    .metadata()
                    .unwrap()
                    .len())
                .sum::<u64>(),
            logical_before.0
        );
        let (current, peak, errors, count) = workspace.physical_spool_snapshot();
        let actual = workspace
            .backing
            .segments
            .values()
            .map(|f| spool_segment(f).unwrap().file.metadata().unwrap().blocks() * 512)
            .sum::<u64>();
        assert_eq!(current, Some(actual));
        assert!(
            peak.unwrap() > initial_peak,
            "partial append allocation must be observed before truncation"
        );
        assert_eq!(errors, 0);
        assert!(count > 0);
        let capture = layerfs_layerstack_store::capture_workspace_commit_diagnostics().unwrap();
        {
            let _timer = layerfs_layerstack_store::begin_workspace_commit(
                layerfs_layerstack_store::CaptureMode::Materialized,
            )
            .unwrap();
            workspace.note_commit_edit_state().unwrap();
        }
        let diagnostic = layerfs_layerstack_store::take_workspace_commit_diagnostics()
            .pop()
            .unwrap();
        assert_eq!(diagnostic.physical_spool_allocated_bytes, current);
        assert_eq!(diagnostic.physical_spool_peak_bytes, peak);
        assert_eq!(diagnostic.physical_spool_observation_errors, 0);
        drop(capture);

        workspace.pin(first, false).unwrap();
        workspace.unlink(ROOT, b"first", false).unwrap();
        assert_eq!(
            workspace.physical_spool_snapshot().0,
            current,
            "open unlinked spool stays charged"
        );
        workspace.unpin(first).unwrap();
        assert_eq!(workspace.physical_spool_snapshot().0, Some(actual));
        assert_eq!(workspace.physical_spool_snapshot().1, peak);

        workspace.commit().unwrap();
        assert_eq!(workspace.physical_spool_snapshot().0, Some(0));
        assert_eq!(
            workspace.physical_spool_snapshot().1,
            peak,
            "Commit checkpoint keeps lifetime allocation evidence"
        );
        let last = workspace.create_file(ROOT, b"last", 0o600).unwrap().node;
        workspace.write(last, 0, b"later").unwrap();
        workspace.discard().unwrap();
        assert_eq!(workspace.physical_spool_snapshot().0, Some(0));
        assert_eq!(workspace.physical_spool_snapshot().1, peak);
        workspace.backing.physical.lock().unwrap().error();
        assert_eq!(workspace.physical_spool_snapshot().0, None);
        assert_eq!(workspace.physical_spool_snapshot().1, None);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workspace_inline_limit_accepts_eight_mib_and_rejects_the_next_byte() {
        let (root, mut workspace) = workspace("inline-limit");
        for index in 0..8 {
            let name = format!("file-{index}");
            let file = workspace
                .create_file(ROOT, name.as_bytes(), 0o600)
                .unwrap()
                .node;
            workspace
                .edit_many(
                    file,
                    vec![(
                        0,
                        0,
                        crate::WorkspaceFileReplacement::Inline(vec![index; 1024 * 1024]),
                    )],
                )
                .unwrap();
        }
        assert_eq!(workspace.live.inline_bytes, MAX_INLINE_PER_WORKSPACE);
        let extra = workspace.create_file(ROOT, b"extra", 0o600).unwrap().node;
        assert!(workspace
            .edit_many(
                extra,
                vec![(0, 0, crate::WorkspaceFileReplacement::Inline(vec![0]),)],
            )
            .is_err());
        assert_eq!(workspace.live.inline_bytes, MAX_INLINE_PER_WORKSPACE);
        assert_eq!(workspace.attr(extra).unwrap().size, 0);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generation_overflow_rejects_without_logical_or_physical_change() {
        let (root, mut workspace, _) = workspace_with_file("generation-overflow");
        let file = workspace.lookup(ROOT, b"file").unwrap().node;
        workspace.live.mutation_generation = u64::MAX;
        let before = workspace.live.nodes[&file].clone();
        let charges = (
            workspace.live.spool_bytes,
            workspace.live.inline_bytes,
            workspace.live.piece_allocation_bytes,
        );
        assert!(workspace
            .edit_many(
                file,
                vec![(0, 0, crate::WorkspaceFileReplacement::Inline(b"P".to_vec()),)],
            )
            .is_err());
        assert_eq!(workspace.live.nodes[&file], before);
        assert_eq!(
            (
                workspace.live.spool_bytes,
                workspace.live.inline_bytes,
                workspace.live.piece_allocation_bytes,
            ),
            charges
        );
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discard_reclaims_one_live_base_inline_zero_spool_composition() {
        let (root, mut workspace, branch) = workspace_with_file("mixed-discard");
        let branch_root = workspace.store.pin_branch(branch).unwrap().root;
        let file = workspace.lookup(ROOT, b"file").unwrap().node;
        workspace
            .edit_many(
                file,
                vec![
                    (1, 2, crate::WorkspaceFileReplacement::Inline(b"X".to_vec())),
                    (2, 0, crate::WorkspaceFileReplacement::Zero(2)),
                ],
            )
            .unwrap();
        let end = workspace.attr(file).unwrap().size;
        workspace.write(file, end, b"S").unwrap();
        let Data::File(FileData::Edited { pieces, .. }) = &workspace.live.nodes[&file].data else {
            panic!("edited file")
        };
        let variants = pieces.pieces();
        assert!(variants
            .iter()
            .any(|piece| matches!(piece, Piece::Base { .. })));
        assert!(variants
            .iter()
            .any(|piece| matches!(piece, Piece::Inline { .. })));
        assert!(variants
            .iter()
            .any(|piece| matches!(piece, Piece::Zero { .. })));
        assert!(variants
            .iter()
            .any(|piece| matches!(piece, Piece::Spool { .. })));
        drop(variants);
        assert_eq!(workspace.backing.segments.len(), 1);
        workspace.discard().unwrap();
        assert_eq!(
            workspace.store.pin_branch(branch).unwrap().root,
            branch_root
        );
        assert_eq!(workspace.live.spool_bytes, 0);
        assert_eq!(workspace.live.inline_bytes, 0);
        assert_eq!(workspace.live.piece_allocation_bytes, 0);
        assert!(workspace.backing.segments.is_empty());
        assert_eq!(workspace.physical_spool_snapshot().0, Some(0));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }
}
