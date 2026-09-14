//! Snapshot-owned candidate planning; canonical construction remains in the
//! existing shared output, file-mutation, inode and namespace machinery.
use super::*;
use crate::candidate_capacity::CandidateCapacity;
use crate::correspondence::{self, Description, Descriptor, OriginSequence, Span};
use crate::overlay::{ChangeKey, InodeRecord};
use crate::overlay_budget::Budget;
use crate::overlay_index::{Index, Root as IndexRoot};
use crate::overlay_ranges::Ranges;
use crate::snapshot::Snapshot;
use std::io;
use std::ops::Bound;
use std::sync::{Arc, Mutex};

const CORRESPONDENCE_NODE: u8 = 1;
const CORRESPONDENCE_INODE: u8 = 2;
const DIRTY_NODE: u8 = 1;
const DIRTY_BINDING: u8 = 2;
const PLAN_BATCH: usize = 64;

/// One current published comparison context. The root graph retains descriptors
/// and canonical identity metadata only, never old snapshot payload ranges.
#[derive(Clone)]
pub(crate) struct PublishedCorrespondence {
    index: Index,
    root: IndexRoot,
    pending: IndexRoot,
    pub(crate) canonical_root: ObjectId,
    pub(crate) covered_sequence: u64,
}
impl PublishedCorrespondence {
    pub(crate) fn empty(
        index: &Index,
        canonical_root: ObjectId,
        covered_sequence: u64,
    ) -> Result<Self> {
        Ok(Self {
            index: index.clone(),
            root: index.empty_root()?,
            pending: index.empty_root()?,
            canonical_root,
            covered_sequence,
        })
    }
    pub(crate) fn description(&self, node: NodeId) -> Result<Option<Description>> {
        let key = [vec![CORRESPONDENCE_NODE], node.0.to_be_bytes().to_vec()].concat();
        let Some((header, root)) = self.index.get_linked(&self.root, &key)? else {
            return Ok(None);
        };
        if header.len() != 32 {
            return Err(StorageError::Integrity("published correspondence inode"));
        }
        let description = root
            .map(|root| Description::restore(&self.index, &root))
            .transpose()?;
        if description
            .as_ref()
            .is_some_and(|value| value.inode != node)
        {
            return Err(StorageError::Integrity("published description inode"));
        }
        Ok(description)
    }
    /// One bounded disk set merges pending inode work across published contexts.
    /// Updating C2 visits only its new keys, never scans the previous prefix.
    pub(crate) fn maintenance_nodes(&self, after: Option<NodeId>) -> Result<Vec<NodeId>> {
        let after = after.map(|node| [vec![DIRTY_NODE], node.0.to_be_bytes().to_vec()].concat());
        self.index
            .scan(
                &self.pending,
                after
                    .as_deref()
                    .map_or(Bound::Included(&[DIRTY_NODE][..]), Bound::Excluded),
                Bound::Excluded(&[DIRTY_NODE + 1]),
                PLAN_BATCH,
            )?
            .into_iter()
            .map(|(key, value)| {
                if key.len() != 9 || !value.is_empty() {
                    return Err(StorageError::Integrity("published maintenance key"));
                }
                Ok(NodeId(u64::from_be_bytes(key[1..].try_into().unwrap())))
            })
            .collect()
    }
    pub(crate) fn maintenance_complete(&mut self, node: NodeId) -> Result<()> {
        let key = [vec![DIRTY_NODE], node.0.to_be_bytes().to_vec()].concat();
        self.pending = self.index.remove(&self.pending, &key)?;
        Ok(())
    }
    fn queue_maintenance(&mut self, node: NodeId) -> Result<()> {
        let key = [vec![DIRTY_NODE], node.0.to_be_bytes().to_vec()].concat();
        self.pending = self.index.set(&self.pending, &key, &[])?;
        Ok(())
    }
    fn set(
        &mut self,
        node: NodeId,
        inode: InodeId,
        description: Option<&Description>,
    ) -> Result<()> {
        let key = [vec![CORRESPONDENCE_NODE], node.0.to_be_bytes().to_vec()].concat();
        let reverse = [vec![CORRESPONDENCE_INODE], inode.as_bytes().to_vec()].concat();
        if self
            .index
            .get(&self.root, &key)?
            .is_some_and(|previous| previous.as_slice() != inode.as_bytes())
        {
            return Err(StorageError::Integrity("published canonical inode changed"));
        }
        let description = description.map(Description::persist).transpose()?;
        let root =
            self.index
                .set_linked(&self.root, &key, inode.as_bytes(), description.as_ref())?;
        self.root = self.index.set(&root, &reverse, &node.0.to_be_bytes())?;
        Ok(())
    }
    fn remove_inode(&mut self, inode: InodeId) -> Result<()> {
        let reverse = [vec![CORRESPONDENCE_INODE], inode.as_bytes().to_vec()].concat();
        let Some(node) = self.index.get(&self.root, &reverse)? else {
            return Ok(());
        };
        if node.len() != 8 {
            return Err(StorageError::Integrity("published correspondence node"));
        }
        self.maintenance_complete(NodeId(u64::from_be_bytes(
            node.as_slice().try_into().unwrap(),
        )))?;
        let key = [vec![CORRESPONDENCE_NODE], node].concat();
        let root = self.index.remove(&self.root, &key)?;
        self.root = self.index.remove(&root, &reverse)?;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SnapshotCandidateDiagnostics {
    pub changed_keys: u64,
    pub changed_inodes: u64,
    pub binding_deltas: u64,
    pub file_tasks: u64,
    pub correspondence_visits: u64,
    pub correspondence_fragments: u64,
    pub replacement_bytes: u64,
    pub reused_bytes: u64,
    pub full_comparisons: u64,
    pub full_builds: u64,
    pub cdc_bytes_scanned: u64,
    pub scratch_peak_reserved_bytes: u64,
    pub scratch_peak_allocated_bytes: u64,
    pub scratch_peak_reserved_files: usize,
    pub construction_memory_reservation: u64,
}
pub(crate) struct PreparedSnapshotCommit {
    pub(crate) built: BuiltRoot,
    pub(crate) admission: layerfs_layerstack_store::WorkspaceAdmission,
    pub(crate) correspondence: PublishedCorrespondence,
    pub(crate) covered_sequence: u64,
    pub(crate) diagnostics: SnapshotCandidateDiagnostics,
}
pub(crate) struct SnapshotCandidateInputs<'a> {
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) ranges: &'a Ranges,
    pub(crate) index: &'a Index,
    pub(crate) origins: &'a OriginSequence,
    pub(crate) comparison: &'a PublishedCorrespondence,
    pub(crate) store: &'a layerfs_layerstack_store::LayerStackStore,
    pub(crate) workspace_id: [u8; 16],
    pub(crate) scope: Option<ObjectId>,
    pub(crate) serials: Arc<Mutex<Option<std::ops::Range<u64>>>>,
    pub(crate) policy: layerfs_workspace_core::ResourcePolicy,
    pub(crate) budget: &'a Arc<Budget>,
    pub(crate) spool: &'a std::path::Path,
}

struct CapturedChanges {
    index: Index,
    root: IndexRoot,
    count: u64,
    diagnostics: SnapshotCandidateDiagnostics,
}
impl CapturedChanges {
    fn capture(inputs: &SnapshotCandidateInputs<'_>) -> Result<Self> {
        let mut result = Self {
            index: inputs.index.clone(),
            root: inputs.index.empty_root()?,
            count: 0,
            diagnostics: Default::default(),
        };
        let mut after = None;
        loop {
            let page = inputs
                .snapshot
                .changes(inputs.comparison.covered_sequence, after.as_deref())?;
            if page.is_empty() {
                break;
            }
            for (key, _, changed) in page {
                result.diagnostics.changed_keys += 1;
                match changed {
                    ChangeKey::Inode(node) => {
                        result.add_node(node)?;
                    }
                    ChangeKey::Binding(parent, name) => {
                        result.add_node(parent)?;
                        let key =
                            [vec![DIRTY_BINDING], parent.0.to_be_bytes().to_vec(), name].concat();
                        result.root = result.index.set(&result.root, &key, &[])?;
                        result.diagnostics.binding_deltas += 1;
                    }
                }
                after = Some(key);
            }
        }
        result.diagnostics.changed_inodes = result.count;
        Ok(result)
    }
    fn add_node(&mut self, node: NodeId) -> Result<()> {
        let key = [vec![DIRTY_NODE], node.0.to_be_bytes().to_vec()].concat();
        if self.index.get(&self.root, &key)?.is_none() {
            self.root = self.index.set(&self.root, &key, &[])?;
            self.count += 1;
        }
        Ok(())
    }
    fn nodes(&self) -> CapturedNodes<'_> {
        CapturedNodes {
            changes: self,
            after: None,
            rows: std::collections::VecDeque::new(),
            done: false,
        }
    }
    fn bindings(&self, parent: NodeId, after: Option<&[u8]>) -> Result<Vec<Vec<u8>>> {
        let prefix = [vec![DIRTY_BINDING], parent.0.to_be_bytes().to_vec()].concat();
        let limit = if parent.0 == u64::MAX {
            vec![DIRTY_BINDING + 1]
        } else {
            [vec![DIRTY_BINDING], (parent.0 + 1).to_be_bytes().to_vec()].concat()
        };
        let start = after.map(|name| [prefix.clone(), name.to_vec()].concat());
        self.index
            .scan(
                &self.root,
                start
                    .as_deref()
                    .map_or(Bound::Included(prefix.as_slice()), Bound::Excluded),
                Bound::Excluded(&limit),
                PLAN_BATCH,
            )?
            .into_iter()
            .map(|(key, value)| {
                if key.len() <= 9 || !value.is_empty() {
                    return Err(StorageError::Integrity("captured binding index"));
                }
                Ok(key[9..].to_vec())
            })
            .collect()
    }
}
struct CapturedNodes<'a> {
    changes: &'a CapturedChanges,
    after: Option<Vec<u8>>,
    rows: std::collections::VecDeque<(Vec<u8>, Vec<u8>)>,
    done: bool,
}
impl Iterator for CapturedNodes<'_> {
    type Item = Result<NodeId>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.rows.is_empty() && !self.done {
            let page = self.changes.index.scan(
                &self.changes.root,
                self.after
                    .as_deref()
                    .map_or(Bound::Included(&[DIRTY_NODE][..]), Bound::Excluded),
                Bound::Excluded(&[DIRTY_NODE + 1]),
                PLAN_BATCH,
            );
            match page {
                Ok(rows) => {
                    self.done = rows.len() < PLAN_BATCH;
                    self.after = rows.last().map(|(key, _)| key.clone());
                    self.rows = rows.into()
                }
                Err(error) => {
                    self.done = true;
                    return Some(Err(error.into()));
                }
            }
        }
        self.rows.pop_front().map(|(key, value)| {
            if key.len() != 9 || !value.is_empty() {
                return Err(StorageError::Integrity("captured inode index"));
            }
            Ok(NodeId(u64::from_be_bytes(key[1..].try_into().unwrap())))
        })
    }
}

struct BuildView<'a, 'i> {
    inputs: &'a SnapshotCandidateInputs<'i>,
    capacity: &'a CandidateCapacity,
    reader: layerfs_layerstack_store::SnapshotReader,
    table: InodeTableRoot,
    output: Mutex<PublishedCorrespondence>,
    diagnostics: Mutex<SnapshotCandidateDiagnostics>,
    physical_reserved: Arc<std::sync::atomic::AtomicU64>,
    file_hook: Option<&'a (dyn Fn(NodeId) + Sync)>,
}
impl SnapshotCandidateInputs<'_> {
    fn prepare_serials(&self) -> Result<()> {
        let Some(scope) = self.scope else {
            return Ok(());
        };
        let mut serials = self
            .serials
            .lock()
            .map_err(|_| StorageError::Integrity("snapshot serial reservation"))?;
        const WIDTH: u64 = 1 << 32;
        if self.snapshot.root.next_inode > WIDTH {
            return Err(StorageError::InvalidInput("workspace inode serial range"));
        }
        if serials.is_none() {
            *serials = Some(self.store.reserve_inode_serials(scope, WIDTH)?);
        }
        Ok(())
    }
    fn canonical_inode(&self, node: NodeId, record: InodeRecord) -> Result<InodeId> {
        if let Some(inode) = record.canonical {
            return Ok(inode);
        }
        if let Some(serials) = self
            .serials
            .lock()
            .map_err(|_| StorageError::Integrity("snapshot serial reservation"))?
            .as_ref()
        {
            let serial = serials
                .start
                .checked_add(node.0)
                .filter(|id| *id < serials.end)
                .ok_or(StorageError::Integrity(
                    "snapshot inode reservation coverage",
                ))?;
            return Ok(layerfs_content::tree::compact::InodeSerial::new(serial)?.inode_key());
        }
        // Released noncompact formats accept the existing allocated_inode key.
        // Bind it to Workspace + stable NodeId, never a mutable current path.
        let mut seed = [0; 32];
        seed[..16].copy_from_slice(&self.workspace_id);
        seed[16..].copy_from_slice(b"snapshot-inode01");
        Ok(filesystem::allocated_inode(
            seed,
            &CanonicalPath::new(&format!("inode-{}", node.0))?,
        ))
    }
    pub(crate) fn build(&self, worker_limit: usize) -> Result<PreparedSnapshotCommit> {
        self.build_inner(worker_limit, None)
    }
    #[cfg(test)]
    fn build_with_hook(
        &self,
        worker_limit: usize,
        hook: &(dyn Fn(NodeId) + Sync),
    ) -> Result<PreparedSnapshotCommit> {
        self.build_inner(worker_limit, Some(hook))
    }
    fn build_inner(
        &self,
        worker_limit: usize,
        hook: Option<&(dyn Fn(NodeId) + Sync)>,
    ) -> Result<PreparedSnapshotCommit> {
        self.policy
            .check_final_delta(1024)
            .map_err(crate::live_error)?;
        if self.comparison.covered_sequence > self.snapshot.root.sequence {
            return Err(StorageError::Integrity(
                "snapshot precedes published coverage",
            ));
        }
        let started = Instant::now();
        let changes = CapturedChanges::capture(self)?;
        // Private canonical scratch belongs to this Workspace's spool directory,
        // not to the process temporary directory.
        let capacity = CandidateCapacity::acquire_in(self.budget, self.policy, self.spool)?;
        if changes.count != 0 {
            self.prepare_serials()?;
        }
        let reader = self.store.snapshot_reader(self.comparison.canonical_root);
        let namespace =
            filesystem::namespace(&CoreReader(&reader), self.comparison.canonical_root)?;
        if namespace.scope != self.scope {
            return Err(StorageError::Integrity("snapshot namespace scope"));
        }
        let mut output = self.comparison.clone();
        for node in changes.nodes() {
            output.queue_maintenance(node?)?;
        }
        let view = BuildView {
            inputs: self,
            capacity: &capacity,
            reader,
            table: InodeTableRoot(namespace.inode_table_root),
            output: Mutex::new(output),
            diagnostics: Mutex::new(changes.diagnostics),
            physical_reserved: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            file_hook: hook,
        };
        let io_bytes = journal_io_bytes(self.policy.max_final_delta_memory_bytes);
        let frontier_budget = self
            .policy
            .max_final_delta_memory_bytes
            .saturating_sub(4 * io_bytes.saturating_sub(256) as u64);
        let tree_scratch = usize::try_from(frontier_budget.saturating_sub(1024) / 2)
            .unwrap_or(usize::MAX)
            .min(SORTED_TREE_UPDATE_SCRATCH_BYTES);
        let plan = view.file_plan(&changes, io_bytes / 2)?;
        let workers = worker_limit
            .min(plan.count)
            .min(io_bytes / 256)
            .min(if plan.has_predecessor { 1 } else { 8 });
        if workers == 0 && plan.count != 0 {
            return Err(StorageError::InvalidInput("file producer budget"));
        }
        note_commit_phase(WorkspaceCommitPhase::CandidatePlan, started);
        let started = Instant::now();
        let mut objects = ObjectBuffer::bounded_output(Some(&view.reader))?;
        capacity.configure(&mut objects)?;
        let tasks = plan.tasks(io_bytes / 2)?;
        // Reuse the existing private-output driver. Admission's Store-global
        // writer permit is obtained only after all canonical construction ends.
        let completed = objects.construct_files(
            workers,
            plan.count,
            tasks,
            |worker| {
                Ok(FileResultWriter {
                    worker,
                    partitions: workers.max(1),
                    journal: BufWriter::with_capacity(
                        io_bytes / 2 / workers.max(1),
                        scoped_journal(self.spool, Some(&capacity.scratch))?,
                    ),
                    offset: 0,
                    count: 0,
                    counters: Default::default(),
                })
            },
            |worker, ordinal, task, writer| {
                writer.set_small_chain_format(view.reader.supports_small_predecessor_candidates());
                view.produce_file(worker, &plan.file, ordinal, task?, writer)
            },
            FileResultWriter::finish,
        )?;
        let mut files = FileResults::new(plan, completed, io_bytes / 2)?;
        note_commit_phase(WorkspaceCommitPhase::Content, started);
        let mut inodes = FrontierInodes::new(
            self.comparison.canonical_root,
            1 + (frontier_budget.saturating_sub(1024) / 512) as usize,
            tree_scratch,
            self.spool,
        );
        inodes.scratch = Some(capacity.scratch.clone());
        let mut metadata = PortableMetadataCache::default();
        let mut references = ReferenceJournal::new(self.spool, io_bytes / 2);
        references.scratch = Some(capacity.scratch.clone());
        let started = Instant::now();
        for node in changes.nodes() {
            let node = node?;
            let Some((value, _)) = self.snapshot.inode_record(node)? else {
                continue;
            };
            if value.attr.links == 0 {
                continue;
            }
            let inode = self.canonical_inode(node, value)?;
            let before = view.before(inode)?;
            let file = if value.attr.kind == Kind::File {
                Some(files.next(node, self.snapshot.root.sequence, value.attr.size)?)
            } else {
                None
            };
            if let Some((_, stored_before)) = file {
                if stored_before != before {
                    return Err(StorageError::Integrity("snapshot file predecessor changed"));
                }
            }
            let content_root = match value.attr.kind {
                Kind::File => file.unwrap().0,
                Kind::Directory => {
                    let base = match before {
                        Some(record) => DirectoryStateRoot(record.content_root),
                        None => empty_directory(&mut objects)?,
                    };
                    view.directory(
                        &changes,
                        node,
                        &mut objects,
                        base,
                        tree_scratch,
                        &mut references,
                    )?
                    .0
                }
                Kind::Symlink => match before {
                    Some(record) => record.content_root,
                    None => filesystem::symlink_content(
                        &mut objects,
                        self.snapshot.readlink(self.ranges, node)?,
                    )?,
                },
            };
            let inode_kind = match value.attr.kind {
                Kind::File => InodeKind::RegularFile,
                Kind::Directory => InodeKind::Directory,
                Kind::Symlink => InodeKind::Symlink,
            };
            let previous_metadata = before
                .map(|record| {
                    portable_metadata(&CoreReader(&view.reader), record.metadata_root, record.kind)
                })
                .transpose()?;
            let metadata_root = if previous_metadata.is_some_and(|old| {
                old.permission_mode == value.attr.mode
                    && old.mtime_seconds == value.attr.mtime_seconds
                    && old.mtime_nanoseconds == value.attr.mtime_nanoseconds
            }) {
                before.unwrap().metadata_root
            } else {
                metadata
                    .get_or_build(
                        &mut objects,
                        inode_kind,
                        value.attr.mode,
                        value.attr.mtime_seconds,
                        value.attr.mtime_nanoseconds,
                    )?
                    .0
            };
            let record = InodeRecordV1 {
                kind: inode_kind,
                content_root,
                metadata_root,
                namespace_ref_count: before.map_or_else(
                    || {
                        if value.attr.kind == Kind::Directory {
                            1
                        } else {
                            u64::from(value.attr.links)
                        }
                    },
                    |record| record.namespace_ref_count,
                ),
            };
            inodes.set_checkpoint(inode, record, node, content_root)?;
            if value.attr.kind != Kind::File {
                view.output
                    .lock()
                    .map_err(|_| StorageError::Integrity("snapshot result correspondence"))?
                    .set(node, inode, None)?;
            }
        }
        files.finish_read()?;
        let file_counters = files.counters;
        drop(files);
        inodes.apply_references_observed(
            &objects,
            &CoreReader(&view.reader),
            references.finish()?,
            frontier_budget,
            io_bytes / 2,
            &mut |inode| {
                view.output
                    .lock()
                    .map_err(|_| StorageError::Integrity("snapshot result correspondence"))?
                    .remove_inode(inode)
            },
        )?;
        inodes.finish(&mut objects, |objects, _, node, content, record| {
            let (value, _) = self
                .snapshot
                .inode_record(node)?
                .ok_or(StorageError::Integrity("snapshot result inode missing"))?;
            CheckpointJournal::validate_record(objects, &metadata, record, content, value.attr)
        })?;
        note_commit_phase(WorkspaceCommitPhase::Namespace, started);
        let mut built = objects.finish(inodes.root, 0)?;
        add_build_counters(&mut built.counters, file_counters);
        let mut correspondence = view
            .output
            .into_inner()
            .map_err(|_| StorageError::Integrity("snapshot result correspondence"))?;
        correspondence.canonical_root = built.root_id;
        correspondence.covered_sequence = self.snapshot.root.sequence;
        let mut diagnostics = view
            .diagnostics
            .into_inner()
            .map_err(|_| StorageError::Integrity("snapshot candidate diagnostics"))?;
        diagnostics.cdc_bytes_scanned = built.counters.cdc_bytes_scanned;
        let usage = capacity.usage();
        diagnostics.scratch_peak_reserved_bytes = usage.peak_reserved_bytes;
        diagnostics.scratch_peak_allocated_bytes = usage.peak_observed_allocated_bytes;
        diagnostics.scratch_peak_reserved_files = usage.peak_reserved_files;
        diagnostics.construction_memory_reservation = capacity.memory_bytes;
        drop(view.reader);
        let mut admission = self.store.workspace_admission(self.workspace_id)?;
        capacity.retain_admission(&mut admission)?;
        Ok(PreparedSnapshotCommit {
            built,
            admission,
            correspondence,
            covered_sequence: self.snapshot.root.sequence,
            diagnostics,
        })
    }
}
impl BuildView<'_, '_> {
    fn before(&self, inode: InodeId) -> Result<Option<InodeRecordV1>> {
        Ok(layerfs_content::tree::inode::inode_record_lookup(
            &CoreReader(&self.reader),
            self.table,
            inode,
            &mut InodeTableCounters::default(),
        )?)
    }
    fn desired(&self, parent: NodeId, name: &[u8]) -> Result<Option<InodeId>> {
        let value = self
            .inputs
            .snapshot
            .binding(parent, name)?
            .ok_or(StorageError::Integrity("captured binding effect missing"))?;
        value
            .map(|node| {
                let (record, _) = self
                    .inputs
                    .snapshot
                    .inode_record(node)?
                    .ok_or(StorageError::Integrity("captured binding inode missing"))?;
                self.inputs.canonical_inode(node, record)
            })
            .transpose()
    }
    fn file_plan(&self, changes: &CapturedChanges, io_bytes: usize) -> Result<FileTaskPlan> {
        let mut output = BufWriter::with_capacity(
            io_bytes,
            scoped_journal(self.inputs.spool, Some(&self.capacity.scratch))?,
        );
        let mut count = 0_usize;
        let mut has_predecessor = false;
        let removed = self.removed_candidates(changes, (io_bytes / (16 * 1024)).clamp(1, 128))?;
        for node in changes.nodes() {
            let node = node?;
            let Some((record, _)) = self.inputs.snapshot.inode_record(node)? else {
                continue;
            };
            if record.attr.kind != Kind::File || record.attr.links == 0 {
                continue;
            }
            let inode = self.inputs.canonical_inode(node, record)?;
            let before = self.before(inode)?;
            let mut predecessor = before.map(|record| record.content_root);
            if predecessor.is_none() {
                if let Some((_, parent, name)) = self.inputs.snapshot.aliases(node, None)?.first() {
                    if let Some((parent_record, _)) = self.inputs.snapshot.inode_record(*parent)? {
                        if let Some(parent) =
                            self.before(self.inputs.canonical_inode(*parent, parent_record)?)?
                        {
                            if parent.kind == InodeKind::Directory {
                                if let Some(inode) = directory_lookup(
                                    &CoreReader(&self.reader),
                                    DirectoryStateRoot(parent.content_root),
                                    &CanonicalName::from_bytes(name)?,
                                    &mut NamespaceCounters::default(),
                                )? {
                                    predecessor = self
                                        .before(inode)?
                                        .filter(|r| r.kind == InodeKind::RegularFile)
                                        .map(|r| r.content_root);
                                }
                            }
                        }
                    }
                    if predecessor.is_none()
                        && record.attr.size > 0
                        && record.attr.size < content::SMALL_LIMIT as u64
                    {
                        predecessor = removed.find(name);
                    }
                }
            }
            let mut slot = [0; FILE_TASK_BYTES as usize];
            slot[..8].copy_from_slice(&node.0.to_le_bytes());
            if let Some(root) = predecessor {
                slot[32..64].copy_from_slice(root.as_bytes());
                has_predecessor = true;
            }
            if let Some(before) = before {
                let bytes = encode_inode_record(before)?;
                if bytes.len() > 256 {
                    return Err(StorageError::Integrity("snapshot predecessor record size"));
                }
                slot[64..68].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
                slot[68..68 + bytes.len()].copy_from_slice(&bytes);
            }
            count = count
                .checked_add(1)
                .ok_or(StorageError::InvalidInput("snapshot task count"))?;
            if (count as u64).saturating_mul(FILE_TASK_BYTES)
                > self.inputs.policy.overlay.max_scratch_bytes
            {
                return Err(StorageError::InvalidInput("snapshot task journal limit"));
            }
            output.write_all(&slot)?;
        }
        output.flush()?;
        self.diagnostics
            .lock()
            .map_err(|_| StorageError::Integrity("snapshot diagnostics"))?
            .file_tasks = count as u64;
        Ok(FileTaskPlan {
            file: output.into_inner().map_err(|e| e.into_error())?,
            count,
            generation: self.inputs.snapshot.root.sequence,
            has_predecessor,
        })
    }
    fn current_description(&self, node: NodeId, length: u64) -> Result<Description> {
        let tree = self.inputs.snapshot.file_ranges(self.inputs.ranges, node)?;
        let mut cursor = self.inputs.ranges.cursor(&tree, 0, length)?;
        Ok(Description::build(
            self.inputs.index,
            node,
            None,
            length,
            self.inputs.policy.overlay.max_scratch_bytes / 49,
            std::iter::from_fn(|| cursor.next_descriptor().transpose()),
        )?)
    }
    fn initial_description(
        &self,
        node: NodeId,
        before: ObjectId,
        length: u64,
    ) -> Result<Description> {
        let tree = self.inputs.snapshot.file_ranges(self.inputs.ranges, node)?;
        let mut cursor = self.inputs.ranges.cursor(&tree, 0, tree.len())?;
        let mut known = self.inputs.index.empty_root()?;
        while let Some((mut descriptor, base)) = cursor.next_base_descriptor()? {
            let Some((root, offset)) = base else { continue };
            if root != before {
                continue;
            }
            if offset
                .checked_add(descriptor.length)
                .is_none_or(|end| end > length)
            {
                return Err(StorageError::Integrity("initial predecessor range"));
            }
            descriptor.offset = offset;
            let mut bytes = Vec::with_capacity(49);
            bytes.extend_from_slice(&descriptor.length.to_be_bytes());
            bytes.extend_from_slice(
                &descriptor
                    .origin
                    .ok_or(StorageError::Integrity("base origin absent"))?
                    .to_bytes(),
            );
            bytes.extend_from_slice(&descriptor.origin_offset.to_be_bytes());
            if self
                .inputs
                .index
                .get(&known, &offset.to_be_bytes())?
                .is_some()
            {
                return Err(StorageError::Integrity("ambiguous initial base occurrence"));
            }
            known = self
                .inputs
                .index
                .set(&known, &offset.to_be_bytes(), &bytes)?;
        }
        let mut after: Option<Vec<u8>> = None;
        let mut rows: std::collections::VecDeque<(Vec<u8>, Vec<u8>)> =
            std::collections::VecDeque::new();
        let mut position = 0_u64;
        let mut pending: Option<Descriptor> = None;
        let mut done = false;
        let iter = std::iter::from_fn(|| -> Option<io::Result<Descriptor>> {
            if let Some(descriptor) = pending.take() {
                position = descriptor.offset + descriptor.length;
                return Some(Ok(descriptor));
            }
            if rows.is_empty() && !done {
                match self.inputs.index.scan(
                    &known,
                    after.as_deref().map_or(Bound::Unbounded, Bound::Excluded),
                    Bound::Unbounded,
                    PLAN_BATCH,
                ) {
                    Ok(page) => {
                        done = page.len() < PLAN_BATCH;
                        after = page.last().map(|(key, _)| key.clone());
                        rows = page.into();
                    }
                    Err(error) => {
                        done = true;
                        return Some(Err(error));
                    }
                }
            }
            if let Some((key, value)) = rows.pop_front() {
                if key.len() != 8 || value.len() != 40 {
                    return Some(Err(io::Error::other("initial descriptor row")));
                }
                let offset = u64::from_be_bytes(key.as_slice().try_into().unwrap());
                let size = u64::from_be_bytes(value[..8].try_into().unwrap());
                let origin = match crate::correspondence::OriginId::from_bytes(
                    value[8..32].try_into().unwrap(),
                ) {
                    Ok(id) => id,
                    Err(e) => return Some(Err(e)),
                };
                let descriptor = Descriptor {
                    offset,
                    length: size,
                    origin: Some(origin),
                    origin_offset: u64::from_be_bytes(value[32..40].try_into().unwrap()),
                    kind: correspondence::Kind::Base,
                };
                if offset < position {
                    return Some(Err(io::Error::other(
                        "overlapping initial predecessor intervals",
                    )));
                }
                if offset > position {
                    pending = Some(descriptor);
                    let start = position;
                    position = offset;
                    return Some(self.inputs.origins.allocate().map(|origin| Descriptor {
                        offset: start,
                        length: offset - start,
                        origin: Some(origin),
                        origin_offset: 0,
                        kind: correspondence::Kind::Base,
                    }));
                }
                position = offset + size;
                return Some(Ok(descriptor));
            }
            if position < length {
                let start = position;
                position = length;
                return Some(self.inputs.origins.allocate().map(|origin| Descriptor {
                    offset: start,
                    length: length - start,
                    origin: Some(origin),
                    origin_offset: 0,
                    kind: correspondence::Kind::Base,
                }));
            }
            None
        });
        Ok(Description::build(
            self.inputs.index,
            node,
            Some(before),
            length,
            self.inputs.policy.overlay.max_scratch_bytes / 49,
            iter,
        )?)
    }
    fn produce_file(
        &self,
        worker: &mut FileResultWriter,
        index: &File,
        ordinal: usize,
        node: NodeId,
        writer: &mut layerfs_layerstack_store::FinalizedOutputWriter,
    ) -> Result<()> {
        if let Some(hook) = self.file_hook {
            hook(node)
        }
        let (record, _) = self
            .inputs
            .snapshot
            .inode_record(node)?
            .ok_or(StorageError::Integrity("snapshot file input"))?;
        let mut slot = [0; FILE_TASK_BYTES as usize];
        index.read_exact_at(&mut slot, ordinal as u64 * FILE_TASK_BYTES)?;
        if u64::from_le_bytes(slot[..8].try_into().unwrap()) != node.0 {
            return Err(StorageError::Integrity("snapshot file task identity"));
        }
        let before_len = u32::from_le_bytes(slot[64..68].try_into().unwrap()) as usize;
        if before_len > 256 {
            return Err(StorageError::Integrity("snapshot predecessor record"));
        }
        let before = if before_len == 0 {
            None
        } else {
            Some(decode_inode_record(&slot[68..68 + before_len])?)
        };
        let predecessor = if slot[32..64].iter().all(|byte| *byte == 0) {
            None
        } else {
            Some(FileContentRoot(ObjectId::from_bytes(&slot[32..64])?))
        };
        let mut description = self.current_description(node, record.attr.size)?;
        let old = if let Some(before) = before {
            let old = match self.inputs.comparison.description(node)? {
                Some(old) => old,
                None => self.initial_description(
                    node,
                    before.content_root,
                    content::length(
                        &CoreReader(&self.reader),
                        FileContentRoot(before.content_root),
                    )?,
                )?,
            };
            if old.inode != node || old.canonical_root != Some(before.content_root) {
                return Err(StorageError::Integrity(
                    "snapshot canonical predecessor correspondence",
                ));
            }
            Some(old)
        } else {
            None
        };
        let mut plan = old
            .as_ref()
            .map(|old| {
                correspondence::plan_scoped(
                    old,
                    &description,
                    self.inputs.spool,
                    self.inputs.policy.overlay.max_scratch_bytes,
                    Some(&self.capacity.scratch),
                )
            })
            .transpose()?;
        let plan_counters = plan.as_ref().map(|plan| plan.counters);
        let unchanged = plan.as_ref().is_some_and(|plan| {
            plan.counters.replacement_bytes == 0
                && old
                    .as_ref()
                    .is_some_and(|old| old.length == description.length)
                && plan.counters.base_bytes == description.length
                && plan.counters.fragments <= 1
        });
        let (root, counters) = if unchanged {
            (before.unwrap().content_root, Default::default())
        } else if writer.supports_small_content()
            && record.attr.size > 0
            && record.attr.size < content::SMALL_LIMIT as u64
        {
            writer.build_small_file(
                self.file_reader(node, 0, record.attr.size)?,
                record.attr.size,
                predecessor.map(|p| p.0),
            )?
        } else if before.is_none() && predecessor.is_none() {
            self.note_full_build()?;
            writer.build_complete_file(
                self.file_reader(node, 0, record.attr.size)?,
                record.attr.size,
            )?
        } else {
            let mut objects = ObjectBuffer::bounded_output(Some(&self.reader))?;
            objects.diagnostic_file_payloads();
            objects.partition_output(worker.partitions)?;
            self.capacity.configure(&mut objects)?;
            if let Some(predecessor) = predecessor {
                objects.set_physical_predecessor(
                    self.reader.clone(),
                    predecessor,
                    self.physical_reserved.clone(),
                )?;
            }
            let large_before = before
                .map(|record| {
                    content::inspect(
                        &CoreReader(&self.reader),
                        FileContentRoot(record.content_root),
                    )
                    .map(|content| !matches!(content, content::Content::Small { .. }))
                })
                .transpose()?
                .unwrap_or(false);
            let built = if large_before && record.attr.size >= content::SMALL_LIMIT as u64 {
                self.mutate_file(
                    node,
                    record.attr.size,
                    before.unwrap().content_root,
                    plan.take()
                        .ok_or(StorageError::Integrity("snapshot predecessor plan absent"))?,
                    objects,
                )?
            } else {
                self.note_full_build()?;
                objects.build_complete_with_predecessor(
                    self.file_reader(node, 0, record.attr.size)?,
                    record.attr.size,
                )?
            };
            let output = (built.root_id, built.counters);
            writer.send_selected(built.objects)?;
            output
        };
        if let Some(plan) = plan_counters {
            let mut diagnostics = self
                .diagnostics
                .lock()
                .map_err(|_| StorageError::Integrity("snapshot diagnostics"))?;
            diagnostics.correspondence_visits +=
                plan.origin_visits + plan.old_descriptor_visits + plan.new_descriptor_visits;
            diagnostics.correspondence_fragments += plan.fragments;
            diagnostics.replacement_bytes += plan.replacement_bytes;
            diagnostics.reused_bytes += plan.base_bytes;
        }
        description.canonical_root = Some(root);
        self.output
            .lock()
            .map_err(|_| StorageError::Integrity("snapshot result correspondence"))?
            .set(
                node,
                self.inputs.canonical_inode(node, record)?,
                Some(&description),
            )?;
        add_build_counters(&mut worker.counters, counters);
        worker.write_result(index, ordinal, node, record.attr.size, root, before)
    }
    fn note_full_build(&self) -> Result<()> {
        self.diagnostics
            .lock()
            .map_err(|_| StorageError::Integrity("snapshot diagnostics"))?
            .full_builds += 1;
        Ok(())
    }
    fn file_reader(
        &self,
        node: NodeId,
        offset: u64,
        length: u64,
    ) -> Result<SnapshotFileReader<'_, '_>> {
        let end = offset
            .checked_add(length)
            .ok_or(StorageError::InvalidInput("snapshot range overflow"))?;
        Ok(SnapshotFileReader {
            view: self,
            node,
            offset,
            end,
        })
    }
    fn mutate_file(
        &self,
        node: NodeId,
        length: u64,
        root: ObjectId,
        plan: correspondence::Plan,
        mut objects: ObjectBuffer<'_>,
    ) -> Result<BuiltRoot> {
        let original = content::length(&CoreReader(&self.reader), FileContentRoot(root))?;
        let mut batch = FileMutationBatch::new(&mut objects, Some(rope::FileStateRoot(root)))?;
        let (mut old_cursor, mut final_cursor, mut replacement) = (0_u64, 0_u64, 0_u64);
        for span in plan.into_iter()? {
            match span? {
                Span::Replacement { length, .. } => {
                    replacement = replacement
                        .checked_add(length)
                        .ok_or(StorageError::InvalidInput("snapshot replacement length"))?
                }
                Span::Base {
                    old_offset, length, ..
                } => {
                    let delete = old_offset
                        .checked_sub(old_cursor)
                        .ok_or(StorageError::Integrity("snapshot base order"))?;
                    if delete != 0 || replacement != 0 {
                        batch.replace(
                            final_cursor,
                            delete,
                            self.file_reader(node, final_cursor, replacement)?,
                        )?;
                    }
                    final_cursor += replacement + length;
                    replacement = 0;
                    old_cursor = old_offset + length;
                }
            }
        }
        let delete = original
            .checked_sub(old_cursor)
            .ok_or(StorageError::Integrity("snapshot base length"))?;
        if delete != 0 || replacement != 0 {
            batch.replace(
                final_cursor,
                delete,
                self.file_reader(node, final_cursor, replacement)?,
            )?;
        }
        if batch.logical_len()? != length {
            return Err(StorageError::Integrity("snapshot file mutation length"));
        }
        let (root, counters) = batch.finish()?;
        objects.finish(root.0, counters.cdc_bytes_scanned)
    }
}
struct SnapshotFileReader<'a, 'i> {
    view: &'a BuildView<'a, 'i>,
    node: NodeId,
    offset: u64,
    end: u64,
}
impl Read for SnapshotFileReader<'_, '_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.offset == self.end || output.is_empty() {
            return Ok(0);
        }
        let count = output.len().min((self.end - self.offset) as usize);
        let read = self
            .view
            .inputs
            .snapshot
            .read_at(
                self.view.inputs.ranges,
                &self.view.reader,
                self.node,
                self.offset,
                &mut output[..count],
            )
            .map_err(io::Error::other)?;
        if read == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.offset += read as u64;
        Ok(read)
    }
}

impl BuildView<'_, '_> {
    fn directory(
        &self,
        changes: &CapturedChanges,
        parent: NodeId,
        objects: &mut ObjectBuffer<'_>,
        root: DirectoryStateRoot,
        scratch: usize,
        references: &mut ReferenceJournal<'_>,
    ) -> Result<DirectoryStateRoot> {
        let checkpoint = references.count;
        let (mut source_error, mut edge_error) = (None, None);
        let deltas = BindingDeltas::new(self, changes, parent).map(|next| {
            next.map_err(|error| {
                source_error = Some(error);
                layerfs_content::CoreError::InvalidRecord("snapshot directory delta")
            })
        });
        let sorted =
            directory_apply_sorted_observed(objects, root, deltas, scratch, |before, after| {
                let result = (|| {
                    let after = match after {
                        Some(inode) if self.before(inode)?.is_some() => Some(inode),
                        _ => None,
                    };
                    references.push(before, after)
                })();
                result.map_err(|error| {
                    edge_error = Some(error);
                    layerfs_content::CoreError::Io
                })
            });
        if let Some(error) = source_error.or(edge_error) {
            return Err(error);
        }
        match sorted {
            Ok((root, _)) => Ok(root),
            Err(
                layerfs_content::CoreError::ObjectLimitExceeded
                | layerfs_content::CoreError::Unsupported,
            ) => {
                references.rewind(checkpoint)?;
                let mut current = root;
                let limit =
                    (self.inputs.policy.max_final_delta_memory_bytes / 4096).clamp(1, 128) as usize;
                let mut batch = Vec::with_capacity(limit);
                for delta in BindingDeltas::new(self, changes, parent) {
                    let (name, after) = delta?;
                    let before =
                        directory_lookup(objects, root, &name, &mut NamespaceCounters::default())?;
                    let existing = match after {
                        Some(inode) if self.before(inode)?.is_some() => Some(inode),
                        _ => None,
                    };
                    references.push(before, existing)?;
                    batch.push((name, after));
                    if batch.len() == limit {
                        current =
                            filesystem::apply_directory_changes(objects, current, batch.drain(..))?
                                .0;
                    }
                }
                if !batch.is_empty() {
                    current = filesystem::apply_directory_changes(objects, current, batch)?.0;
                }
                Ok(current)
            }
            Err(error) => Err(error.into()),
        }
    }
    fn removed_candidates(
        &self,
        changes: &CapturedChanges,
        limit: usize,
    ) -> Result<RemovedSmallCandidates> {
        if self.inputs.policy.max_final_delta_memory_bytes < 4 * 1024 * 1024
            || !self.reader.supports_small_predecessor_candidates()
            || changes.count > 16384
        {
            return Ok(RemovedSmallCandidates::default());
        }
        let mut discovery = RemovedSmallDiscovery {
            candidates: RemovedSmallCandidates {
                entries: Vec::with_capacity(RemovedSmallCandidates::ENTRY_LIMIT),
            },
            directories: Vec::with_capacity(RemovedSmallCandidates::ENTRY_LIMIT),
            name_bytes: 0,
            entries: 0,
            calls: 0,
        };
        let mut pending = Vec::with_capacity(limit);
        let mut count = 0;
        for node in changes.nodes() {
            let node = node?;
            let Some((record, _)) = self.inputs.snapshot.inode_record(node)? else {
                continue;
            };
            if record.attr.kind != Kind::Directory {
                continue;
            }
            let Some(before) = self.before(self.inputs.canonical_inode(node, record)?)? else {
                continue;
            };
            let mut after = None;
            loop {
                let names = changes.bindings(node, after.as_deref())?;
                if names.is_empty() {
                    break;
                }
                for name in names {
                    count += 1;
                    if count > 32768 {
                        return Ok(RemovedSmallCandidates::default());
                    }
                    if self.inputs.snapshot.binding(node, &name)? == Some(None) {
                        pending.push((
                            DirectoryStateRoot(before.content_root),
                            CanonicalName::from_bytes(&name)?,
                        ));
                    }
                    after = Some(name);
                    if pending.len() == limit {
                        if !discovery.bindings(&self.reader, self.table, &pending, limit)? {
                            return Ok(RemovedSmallCandidates::default());
                        }
                        pending.clear();
                    }
                }
            }
        }
        if !pending.is_empty() && !discovery.bindings(&self.reader, self.table, &pending, limit)? {
            return Ok(RemovedSmallCandidates::default());
        }
        drop(pending);
        let mut visits = 0;
        while let Some((root, depth)) = discovery.directories.pop() {
            visits += 1;
            if visits > RemovedSmallCandidates::ENTRY_LIMIT {
                return Ok(RemovedSmallCandidates::default());
            }
            let mut after = None;
            loop {
                if !discovery.charge(0, 1) {
                    return Ok(RemovedSmallCandidates::default());
                }
                let page = directory_page_after(
                    &CoreReader(&self.reader),
                    root,
                    after.as_ref(),
                    limit,
                    limit * 512,
                    &mut NamespaceCounters::default(),
                )?;
                if !discovery.charge(page.entries.len(), 0)
                    || !discovery.records(&self.reader, self.table, &page.entries, depth, limit)?
                {
                    return Ok(RemovedSmallCandidates::default());
                }
                after = page.continuation;
                if after.is_none() {
                    break;
                }
            }
        }
        discovery.candidates.entries.sort_unstable();
        discovery.candidates.entries.dedup();
        Ok(discovery.candidates)
    }
}
struct BindingDeltas<'a, 'i> {
    view: &'a BuildView<'a, 'i>,
    changes: &'a CapturedChanges,
    parent: NodeId,
    after: Option<Vec<u8>>,
    rows: std::collections::VecDeque<Vec<u8>>,
    done: bool,
}
impl<'a, 'i> BindingDeltas<'a, 'i> {
    fn new(view: &'a BuildView<'a, 'i>, changes: &'a CapturedChanges, parent: NodeId) -> Self {
        Self {
            view,
            changes,
            parent,
            after: None,
            rows: Default::default(),
            done: false,
        }
    }
}
impl Iterator for BindingDeltas<'_, '_> {
    type Item = Result<(CanonicalName, Option<InodeId>)>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.rows.is_empty() && !self.done {
            match self.changes.bindings(self.parent, self.after.as_deref()) {
                Ok(names) => {
                    self.done = names.len() < PLAN_BATCH;
                    self.after = names.last().cloned();
                    self.rows = names.into()
                }
                Err(error) => {
                    self.done = true;
                    return Some(Err(error));
                }
            }
        }
        self.rows.pop_front().map(|name| {
            Ok((
                CanonicalName::from_bytes(&name)?,
                self.view.desired(self.parent, &name)?,
            ))
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::host_overlay::HostOverlay;
    use crate::overlay_budget::{Budget, HostAdmission, HostLimits};
    use layerfs_fuse::live_runtime::LiveRuntime;
    use layerfs_layerstack_store::{
        BranchRecord, EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
        WorkspacePublicationAttempt,
    };
    use layerfs_workspace_core::ResourcePolicy;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Duration;

    pub(crate) struct Fixture {
        pub(crate) directory: PathBuf,
        pub(crate) host: HostOverlay,
        pub(crate) store: LayerStackStore,
        branch: BranchRecord,
        workspace: [u8; 16],
        scope: Option<ObjectId>,
        serials: Arc<Mutex<Option<std::ops::Range<u64>>>>,
        pub(crate) comparison: PublishedCorrespondence,
        keep_directory: bool,
        _runtime: LiveRuntime,
    }
    impl Fixture {
        pub(crate) fn new(length: u64) -> Self {
            let id = crate::WorkspaceId::new();
            let directory = std::env::temp_dir().join(format!("snapshot-candidate-{id}"));
            let source = directory.join("source");
            fs::create_dir_all(&source).unwrap();
            std::fs::File::create(source.join("file"))
                .unwrap()
                .set_len(length)
                .unwrap();
            let store = LayerStackStore::create(directory.join("store.sqlite")).unwrap();
            let layer = store
                .initialize_layerstack(
                    EntityName::new("candidate").unwrap(),
                    LayerStackInitialization::Directory(source.clone()),
                )
                .unwrap()
                .genesis_layer_id;
            let branch = store
                .fork_branch(
                    EntityName::new("main").unwrap(),
                    LocalForkSource::Layer { layer_id: layer },
                )
                .unwrap();
            let pinned = store.pin_branch(branch).unwrap();
            let scope = filesystem::namespace(&CoreReader(&pinned.reader), pinned.root)
                .unwrap()
                .scope;
            let policy = ResourcePolicy::default();
            let runtime = LiveRuntime::new().unwrap();
            let host_admission =
                HostAdmission::new(runtime.scheduler(), HostLimits::default()).unwrap();
            let resources = Budget::open(&directory, policy, host_admission).unwrap();
            let comparison =
                PublishedCorrespondence::empty(&resources.index, pinned.root, 0).unwrap();
            let host = HostOverlay::new(pinned.reader, pinned.root, id.bytes(), policy, resources)
                .unwrap();
            fs::remove_dir_all(source).unwrap();
            Self {
                directory,
                host,
                store,
                branch: pinned.branch,
                workspace: id.bytes(),
                scope,
                serials: Default::default(),
                comparison,
                keep_directory: false,
                _runtime: runtime,
            }
        }
        pub(crate) fn inputs<'a>(&'a self, snapshot: &'a Snapshot) -> SnapshotCandidateInputs<'a> {
            SnapshotCandidateInputs {
                snapshot,
                ranges: &self.host.ranges,
                index: &self.host.index,
                origins: self.host.origins(),
                comparison: &self.comparison,
                store: &self.store,
                workspace_id: self.workspace,
                scope: self.scope,
                serials: self.serials.clone(),
                policy: self.host.policy,
                budget: &self.host.budget,
                spool: &self.directory,
            }
        }
        pub(crate) fn publish(
            &mut self,
            prepared: PreparedSnapshotCommit,
        ) -> (ObjectId, SnapshotCandidateDiagnostics) {
            let attempt = WorkspacePublicationAttempt::new(
                self.workspace,
                &self.branch,
                self.comparison.canonical_root,
                prepared.built.root_id,
                self.branch.base_layer_id,
                prepared.covered_sequence,
            );
            self.store
                .commit_workspace_candidate_retained(&attempt, prepared.built, prepared.admission)
                .unwrap();
            self.comparison = prepared.correspondence;
            self.branch = self.store.pin_branch(self.branch.id).unwrap().branch;
            self.store
                .acknowledge_workspace_publication(&attempt)
                .unwrap();
            (attempt.candidate_root, prepared.diagnostics)
        }
        fn resolved(&self, root: ObjectId, path: &str) -> filesystem::Resolved {
            let reader = self.store.snapshot_reader(root);
            filesystem::resolve(
                &CoreReader(&reader),
                root,
                &CanonicalPath::new(path).unwrap(),
                &mut LogicalCounters::default(),
            )
            .unwrap()
        }
        fn bytes(&self, root: ObjectId, path: &str, offset: u64, length: u64) -> Vec<u8> {
            canonical_bytes(&self.store, root, path, offset, length)
        }
        fn reopen(mut self, check: impl FnOnce(&LayerStackStore)) {
            self.keep_directory = true;
            let directory = self.directory.clone();
            drop(self);
            let store = LayerStackStore::connect(directory.join("store.sqlite")).unwrap();
            check(&store);
            drop(store);
            fs::remove_dir_all(directory).unwrap();
        }
    }
    fn canonical_bytes(
        store: &LayerStackStore,
        root: ObjectId,
        path: &str,
        offset: u64,
        length: u64,
    ) -> Vec<u8> {
        let reader = store.snapshot_reader(root);
        let resolved = filesystem::resolve(
            &CoreReader(&reader),
            root,
            &CanonicalPath::new(path).unwrap(),
            &mut LogicalCounters::default(),
        )
        .unwrap();
        let mut bytes = Vec::new();
        content::read_range(
            &CoreReader(&reader),
            FileContentRoot(resolved.record.content_root),
            offset..offset + length,
            &mut bytes,
        )
        .unwrap();
        bytes
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if !self.keep_directory {
                fs::remove_dir_all(&self.directory).unwrap();
            }
        }
    }

    #[test]
    fn shared_builder_holds_owned_c1_cut_and_reuses_c1_for_localized_c2() {
        const LENGTH: u64 = 16 * 1024 * 1024;
        let mut fixture = Fixture::new(LENGTH);
        let file = fixture.host.lookup(ROOT, b"file").unwrap().node;
        fixture.host.write(file, 0, b"X").unwrap();
        let captured = fixture.host.snapshot().unwrap();
        let (started, at_build) = mpsc::channel();
        let (resume, continue_build) = mpsc::channel();
        let continue_build = Mutex::new(continue_build);
        let built = std::thread::scope(|scope| {
            let shared = &fixture;
            let snapshot = &captured;
            let building = scope.spawn(move || {
                shared.inputs(snapshot).build_with_hook(1, &|_| {
                    started.send(()).unwrap();
                    continue_build.lock().unwrap().recv().unwrap();
                })
            });
            at_build.recv_timeout(Duration::from_secs(10)).unwrap();
            fixture.host.write(file, LENGTH / 2, b"Y").unwrap();
            let mut live = [0; 1];
            fixture.host.read_into(file, LENGTH / 2, &mut live).unwrap();
            assert_eq!(live, [b'Y']);
            // The Store admission permit is available while canonical build is held.
            let (permit, available) = mpsc::channel();
            scope.spawn(move || {
                let held = shared.store.workspace_admission([0xA7; 16]);
                permit.send(held.is_ok()).unwrap();
            });
            let available = available.recv_timeout(Duration::from_secs(3));
            resume.send(()).unwrap();
            let result = building.join().unwrap().unwrap();
            assert!(available.unwrap());
            result
        });
        let (first, d1) = fixture.publish(built);
        assert_eq!(fixture.bytes(first, "file", 0, 1), b"X");
        assert_eq!(fixture.bytes(first, "file", LENGTH / 2, 1), [0]);
        assert_eq!(d1.full_comparisons, 0);
        assert_eq!(d1.full_builds, 0);
        assert_eq!(d1.replacement_bytes, 1);
        assert_eq!(d1.reused_bytes, LENGTH - 1);
        assert!(d1.cdc_bytes_scanned < LENGTH / 4, "{d1:?}");
        assert_eq!(
            captured.root.base_root,
            fixture.host.snapshot().unwrap().root.base_root
        );
        let next = fixture.host.snapshot().unwrap();
        let prepared = fixture.inputs(&next).build(1).unwrap();
        let (second, d2) = fixture.publish(prepared);
        assert_eq!(fixture.bytes(second, "file", 0, 1), b"X");
        assert_eq!(fixture.bytes(second, "file", LENGTH / 2, 1), b"Y");
        assert_eq!(d2.full_comparisons, 0);
        assert_eq!(d2.full_builds, 0);
        assert_eq!(d2.replacement_bytes, 1);
        assert_eq!(d2.reused_bytes, LENGTH - 1);
        assert!(d2.cdc_bytes_scanned < LENGTH / 4, "{d2:?}");
        assert_eq!(
            fixture
                .comparison
                .description(file)
                .unwrap()
                .unwrap()
                .records,
            4
        );
        println!("C1 {d1:?}; C2 {d2:?}; logical_length={LENGTH}");
        fixture.reopen(|store| {
            assert_eq!(canonical_bytes(store, first, "file", LENGTH / 2, 1), [0]);
            assert_eq!(canonical_bytes(store, second, "file", LENGTH / 2, 1), b"Y");
        });
    }

    #[test]
    fn published_new_inode_identity_binding_suffix_and_pending_set_survive_c2() {
        let mut fixture = Fixture::new(0);
        let directory = fixture.host.mkdir(ROOT, b"dir", 0o755).unwrap().node;
        let file = fixture
            .host
            .create_file(ROOT, b"created", 0o644)
            .unwrap()
            .node;
        fixture
            .host
            .write(file, 0, &vec![b'a'; 128 * 1024])
            .unwrap();
        let first_input = fixture.host.snapshot().unwrap();
        let first = fixture.inputs(&first_input).build(2).unwrap();
        let (first, d1) = fixture.publish(first);
        assert_eq!(fixture.resolved(first, "dir").record.namespace_ref_count, 1);
        let first_inode = fixture.resolved(first, "created").inode;
        fixture
            .host
            .rename(ROOT, b"created", directory, b"moved", false)
            .unwrap();
        fixture.host.write(file, 17, b"B").unwrap();
        let second_input = fixture.host.snapshot().unwrap();
        assert!(second_input
            .inode_record(file)
            .unwrap()
            .unwrap()
            .0
            .canonical
            .is_none());
        let second = fixture.inputs(&second_input).build(2).unwrap();
        let (second, d2) = fixture.publish(second);
        assert_eq!(fixture.resolved(second, "dir/moved").inode, first_inode);
        assert_eq!(fixture.bytes(second, "dir/moved", 16, 3), b"aBa");
        assert_eq!(d2.binding_deltas, 2, "C2 must omit captured C1 bindings");
        assert_eq!(d2.replacement_bytes, 1);
        let pending = fixture.comparison.maintenance_nodes(None).unwrap();
        assert_eq!(pending.iter().filter(|node| **node == file).count(), 1);
        assert!(pending.contains(&directory));
        fixture.comparison.maintenance_complete(directory).unwrap();
        assert!(!fixture
            .comparison
            .maintenance_nodes(None)
            .unwrap()
            .contains(&directory));
        fixture.host.unlink(directory, b"moved", false).unwrap();
        fixture.host.unlink(ROOT, b"dir", true).unwrap();
        let third_input = fixture.host.snapshot().unwrap();
        let third = fixture.inputs(&third_input).build(1).unwrap();
        fixture.publish(third);
        assert!(fixture.comparison.description(file).unwrap().is_none());
        assert!(!fixture
            .comparison
            .maintenance_nodes(None)
            .unwrap()
            .contains(&file));
        println!("C1 {d1:?}; C2 {d2:?}; stable_new_inode={first_inode:?}");
        fixture.reopen(|store| {
            assert_eq!(canonical_bytes(store, first, "created", 16, 3), b"aaa");
            assert_eq!(canonical_bytes(store, second, "dir/moved", 16, 3), b"aBa");
        });
    }
    #[test]
    fn zero_gap_deletion_reuses_large_zero_span_and_insertion_reads_only_new_byte() {
        const ZERO: u64 = 16 * 1024 * 1024;
        let mut fixture = Fixture::new(0);
        let file = fixture.host.lookup(ROOT, b"file").unwrap().node;
        fixture.host.write(file, 0, b"A").unwrap();
        fixture
            .host
            .edit_many(file, &[(1, 0, crate::WorkspaceFileReplacement::Zero(ZERO))])
            .unwrap();
        fixture.host.append(file, b"B").unwrap();
        let first = fixture.host.snapshot().unwrap();
        let prepared = fixture.inputs(&first).build(1).unwrap();
        let (c1, d1) = fixture.publish(prepared);
        assert_eq!(
            fixture
                .comparison
                .description(file)
                .unwrap()
                .unwrap()
                .records,
            3
        );
        fixture
            .host
            .edit_many(file, &[(0, 1, crate::WorkspaceFileReplacement::Zero(0))])
            .unwrap();
        let second = fixture.host.snapshot().unwrap();
        let prepared = fixture.inputs(&second).build(1).unwrap();
        let (c2, d2) = fixture.publish(prepared);
        assert_eq!(d2.replacement_bytes, 0);
        assert_eq!(d2.reused_bytes, ZERO + 1);
        assert_eq!(
            d2.cdc_bytes_scanned, 0,
            "unchanged sparse zero region must not enter CDC"
        );
        assert_eq!(d2.full_builds, 0);
        fixture
            .host
            .edit_many(file, &[(0, 0, crate::WorkspaceFileReplacement::Zero(1))])
            .unwrap();
        let third = fixture.host.snapshot().unwrap();
        let prepared = fixture.inputs(&third).build(1).unwrap();
        let (c3, d3) = fixture.publish(prepared);
        assert_eq!(d3.replacement_bytes, 1);
        assert_eq!(d3.reused_bytes, ZERO + 1);
        assert_eq!(d3.cdc_bytes_scanned, 1);
        assert_eq!(d3.full_builds, 0);
        println!(
            "explicit_zero C1 {d1:?}; C2 deletion {d2:?}; C3 insertion {d3:?}; zero_bytes={ZERO}"
        );
        fixture.reopen(|store| {
            assert_eq!(canonical_bytes(store, c1, "file", 0, 2), [b'A', 0]);
            assert_eq!(canonical_bytes(store, c2, "file", ZERO - 1, 2), [0, b'B']);
            assert_eq!(canonical_bytes(store, c3, "file", ZERO, 2), [0, b'B']);
        });
    }
}
