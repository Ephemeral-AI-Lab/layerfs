#[path = "snapshot_candidate.rs"]
mod snapshot_candidate;
#[cfg(test)]
pub(crate) use snapshot_candidate::tests::Fixture as SnapshotCandidateFixture;
pub(crate) use snapshot_candidate::{
    PreparedSnapshotCommit, PublishedCorrespondence, SnapshotCandidateDiagnostics,
    SnapshotCandidateInputs,
};

use crate::cow_tree::{portable_metadata, Attr, Data, FileData, Kind, NodeId, Workspace, ROOT};
use layerfs_content::file::content::{self, FileContentRoot};
use layerfs_content::file::rope::{self, FileMutationBatch, ObjectStore, RopeCounters};
use layerfs_content::filesystem::{self, InodeMutation, LogicalCounters, PortableMetadataCache};
use layerfs_content::object::access::ObjectRead;
use layerfs_content::object::{ContentDigestWriter, ObjectId};
use layerfs_content::tree::batch::{
    directory_apply_sorted_observed, inode_table_apply_sorted_with_budget,
    SORTED_TREE_UPDATE_SCRATCH_BYTES,
};
use layerfs_content::tree::directory::codec::encode_namespace_root;
use layerfs_content::tree::directory::{
    directory_lookup, directory_page_after, empty_directory, DirectoryStateRoot, NamespaceCounters,
};
use layerfs_content::tree::inode::codec::{decode_inode_record, encode_inode_record};
use layerfs_content::tree::inode::InodeTableCounters;
use layerfs_content::tree::inode::{InodeId, InodeKind, InodeRecordV1, InodeTableRoot};
use layerfs_content::tree::NamespaceRootV1;
use layerfs_content::{CanonicalName, CanonicalPath};
use layerfs_layerstack_store::{
    BuiltRoot, CoreReader, ObjectBuffer, Result, StoreError as StorageError, WorkspaceCommitPhase,
};
use layerfs_layerstack_store::{ScratchBudget, ScratchFile as File};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::FileExt;
use std::time::Instant;

#[derive(Clone, Copy)]
struct BaseEntry {
    record: InodeRecordV1,
}

#[derive(Clone, Copy)]
struct FinalEntry {
    node: NodeId,
    attr: Attr,
}

fn anonymous_journal(directory: &std::path::Path) -> Result<File> {
    scoped_journal(directory, None)
}
fn scoped_journal(
    directory: &std::path::Path,
    budget: Option<&std::sync::Arc<ScratchBudget>>,
) -> Result<File> {
    let path = directory.join(format!("candidate-{}", crate::WorkspaceId::new()));
    let (file, allocation) = File::create(&path, budget)?;
    std::fs::remove_file(path)?;
    drop(allocation); // the unlinked file's descriptors now own its allocation
    Ok(file)
}

fn journal_io_bytes(budget: u64) -> usize {
    (budget.saturating_sub(1024) / 64).clamp(256, 64 * 1024) as usize
}

// Original/final edge facts from the existing sorted directory leaf merge.
// New inode reference counts are already initialized from their final bindings.
struct ReferenceJournal<'a> {
    directory: &'a std::path::Path,
    io_bytes: usize,
    writer: Option<BufWriter<File>>,
    count: u64,
    scratch: Option<std::sync::Arc<ScratchBudget>>,
}

impl<'a> ReferenceJournal<'a> {
    fn new(directory: &'a std::path::Path, io_bytes: usize) -> Self {
        Self {
            directory,
            io_bytes,
            writer: None,
            count: 0,
            scratch: None,
        }
    }

    fn push(&mut self, before: Option<InodeId>, after: Option<InodeId>) -> Result<()> {
        if before == after {
            return Ok(());
        }
        if self.writer.is_none() {
            self.writer = Some(BufWriter::with_capacity(
                self.io_bytes,
                scoped_journal(self.directory, self.scratch.as_ref())?,
            ));
        }
        let mut row = [0; 65];
        if let Some(inode) = before {
            row[0] |= 1;
            row[1..33].copy_from_slice(inode.as_bytes());
        }
        if let Some(inode) = after {
            row[0] |= 2;
            row[33..65].copy_from_slice(inode.as_bytes());
        }
        self.writer.as_mut().unwrap().write_all(&row)?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(StorageError::Integrity("reference journal count"))?;
        Ok(())
    }

    fn rewind(&mut self, count: u64) -> Result<()> {
        if let Some(writer) = &mut self.writer {
            writer.flush()?;
            let end = count
                .checked_mul(65)
                .ok_or(StorageError::Integrity("reference journal length"))?;
            writer.get_ref().set_len(end)?;
            writer.seek(SeekFrom::Start(end))?;
        }
        self.count = count;
        Ok(())
    }

    fn finish(self) -> Result<Option<(File, u64)>> {
        self.writer
            .map(|mut writer| {
                writer.flush()?;
                Ok((
                    writer.into_inner().map_err(|error| error.into_error())?,
                    self.count,
                ))
            })
            .transpose()
    }
}

#[derive(Clone, Copy)]
pub(crate) enum CandidatePurpose {
    Commit,
    Preview,
}

// Mutable identities stay with Workspace; Store receives only the canonical candidate.
pub(crate) struct PreparedCommit {
    pub(crate) built: BuiltRoot,
    pub(crate) checkpoint: Checkpoint,
    pub(crate) admission: Option<layerfs_layerstack_store::WorkspaceAdmission>,
}

pub(crate) struct Checkpoint {
    pub(crate) root: ObjectId,
    pub(crate) generation: u64,
    file: File,
    count: u64,
    io_bytes: usize,
}

struct CheckpointJournal {
    writer: std::io::BufWriter<File>,
    count: u64,
    byte_limit: u64,
}

impl CheckpointJournal {
    fn new(workspace: &CandidateInputs<'_>) -> Result<Self> {
        let file = anonymous_journal(workspace.spool)?;
        Ok(Self {
            writer: BufWriter::with_capacity(
                journal_io_bytes(workspace.live.policy.max_final_delta_memory_bytes),
                file,
            ),
            count: 0,
            // Metadata facts are candidate scratch, not payload spool bytes.
            byte_limit: (workspace.live.nodes.len() as u64).saturating_mul(104),
        })
    }

    fn push(
        &mut self,
        node: NodeId,
        inode: InodeId,
        content: ObjectId,
        attr: Attr,
        checked_file_len: Option<u64>,
    ) -> Result<()> {
        if attr.kind == Kind::File && checked_file_len != Some(attr.size) {
            return Err(StorageError::Integrity("checkpoint file length"));
        }
        if self.count.saturating_add(1).saturating_mul(104) > self.byte_limit {
            return Err(StorageError::InvalidInput(
                "workspace checkpoint journal limit",
            ));
        }
        let mut bytes = [0; 104];
        bytes[..8].copy_from_slice(&node.0.to_le_bytes());
        bytes[8..40].copy_from_slice(inode.as_bytes());
        bytes[40..72].copy_from_slice(content.as_bytes());
        bytes[72..80].copy_from_slice(&attr.size.to_le_bytes());
        bytes[80..84].copy_from_slice(&attr.mode.to_le_bytes());
        bytes[84..88].copy_from_slice(&attr.links.to_le_bytes());
        bytes[88..96].copy_from_slice(&attr.mtime_seconds.to_le_bytes());
        bytes[96..100].copy_from_slice(&attr.mtime_nanoseconds.to_le_bytes());
        bytes[100] = match attr.kind {
            Kind::File => 1,
            Kind::Directory => 2,
            Kind::Symlink => 3,
        };
        self.writer.write_all(&bytes)?;
        self.count += 1;
        Ok(())
    }

    #[cfg(test)]
    fn validate(
        &mut self,
        objects: &ObjectBuffer<'_>,
        metadata_cache: &PortableMetadataCache,
        mut record: impl FnMut(InodeId) -> Result<InodeRecordV1>,
    ) -> Result<()> {
        self.writer.flush()?;
        Checkpoint::visit_file(
            self.writer.get_ref(),
            self.count,
            self.writer.capacity(),
            |_, inode, content, attr| {
                let final_record = record(inode)?;
                Self::validate_record(objects, metadata_cache, final_record, content, attr)
            },
        )
    }

    fn validate_record(
        objects: &ObjectBuffer<'_>,
        metadata_cache: &PortableMetadataCache,
        final_record: InodeRecordV1,
        content: ObjectId,
        attr: Attr,
    ) -> Result<()> {
        let metadata =
            match metadata_cache.get_by_root(final_record.kind, final_record.metadata_root) {
                Some(metadata) => metadata,
                None => portable_metadata(objects, final_record.metadata_root, final_record.kind)?,
            };
        let size = match final_record.kind {
            InodeKind::RegularFile => attr.size, // checked at the completed-file handoff
            InodeKind::Directory => 0,
            InodeKind::Symlink => ObjectRead::with_authenticated_canonical(
                objects,
                content,
                layerfs_content::tree::directory::codec::decode_symlink,
            )?
            .target
            .len() as u64,
        };
        if final_record.content_root != content
            || kind(final_record.kind) != attr.kind
            || size != attr.size
            || metadata.permission_mode != attr.mode
            || metadata.mtime_seconds != attr.mtime_seconds
            || metadata.mtime_nanoseconds != attr.mtime_nanoseconds
            || (attr.kind != Kind::Directory
                && final_record.namespace_ref_count != u64::from(attr.links))
        {
            return Err(StorageError::Integrity("candidate checkpoint record"));
        }
        Ok(())
    }

    fn finish(mut self, built: BuiltRoot, generation: u64) -> Result<PreparedCommit> {
        self.writer.flush()?;
        let io_bytes = self.writer.capacity();
        let file = self
            .writer
            .into_inner()
            .map_err(|error| error.into_error())?;
        Ok(PreparedCommit {
            admission: None,
            checkpoint: Checkpoint {
                root: built.root_id,
                generation,
                file,
                count: self.count,
                io_bytes,
            },
            built,
        })
    }
}

impl Checkpoint {
    pub(crate) fn visit(
        &self,
        mut visitor: impl FnMut(NodeId, InodeId, ObjectId, Attr) -> Result<()>,
    ) -> Result<()> {
        Self::visit_file(&self.file, self.count, self.io_bytes, &mut visitor)
    }

    fn visit_file(
        file: &File,
        count: u64,
        io_bytes: usize,
        mut visitor: impl FnMut(NodeId, InodeId, ObjectId, Attr) -> Result<()>,
    ) -> Result<()> {
        let mut reader = BufReader::with_capacity(io_bytes, file);
        reader.seek(SeekFrom::Start(0))?;
        for _ in 0..count {
            let mut bytes = [0; 104];
            reader.read_exact(&mut bytes)?;
            let node = NodeId(u64::from_le_bytes(bytes[..8].try_into().unwrap()));
            let attr = Attr {
                node,
                size: u64::from_le_bytes(bytes[72..80].try_into().unwrap()),
                mode: u32::from_le_bytes(bytes[80..84].try_into().unwrap()),
                links: u32::from_le_bytes(bytes[84..88].try_into().unwrap()),
                mtime_seconds: i64::from_le_bytes(bytes[88..96].try_into().unwrap()),
                mtime_nanoseconds: u32::from_le_bytes(bytes[96..100].try_into().unwrap()),
                kind: match bytes[100] {
                    1 => Kind::File,
                    2 => Kind::Directory,
                    3 => Kind::Symlink,
                    _ => return Err(StorageError::Integrity("checkpoint kind")),
                },
            };
            visitor(
                node,
                InodeId(bytes[8..40].try_into().unwrap()),
                ObjectId::from_bytes(&bytes[40..72])?,
                attr,
            )?;
        }
        if reader.read(&mut [0; 1])? != 0 {
            return Err(StorageError::Integrity("checkpoint journal length"));
        }
        Ok(())
    }
}

impl Workspace {
    pub(crate) fn build_candidate(&mut self, purpose: CandidatePurpose) -> Result<PreparedCommit> {
        #[cfg(any(debug_assertions, feature = "test-instrumentation"))]
        if INJECT_CANDIDATE_FAILURE.with(|inject| inject.replace(false)) {
            return Err(StorageError::Integrity(
                "injected Workspace candidate failure",
            ));
        }
        self.build_frontier_candidate(purpose)
    }

    // Directory overlays already are the final binding delta. Applying their inode
    // edges directly preserves untouched subtrees, including a renamed directory,
    // without building either complete namespace manifest.
    fn build_frontier_candidate(&mut self, purpose: CandidatePurpose) -> Result<PreparedCommit> {
        let workers = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
            .min(8);
        // CandidateInputs further caps workers by eligible tasks and partitions
        // the existing aggregate journal, candidate and spill allowances.
        self.build_frontier_candidate_with_workers(purpose, workers)
    }

    fn build_frontier_candidate_with_workers(
        &mut self,
        purpose: CandidatePurpose,
        worker_limit: usize,
    ) -> Result<PreparedCommit> {
        if let Some(remote) = &self.remote {
            let backing = remote
                .backing
                .lock()
                .map_err(|_| StorageError::Integrity("live backing lock"))?;
            backing.frozen_generation()?;
            let canonical_nodes = backing
                .facts
                .iter()
                .filter_map(|(id, node)| node.canonical.map(|inode| (inode, *id)))
                .collect();
            return CandidateInputs {
                scope: self.inode_scope,
                serials: self.inode_serials.clone(),
                live: layerfs_workspace_core::FrozenWorkspaceChanges {
                    nodes: &backing.facts,
                    dirty: &backing.dirty,
                    canonical_nodes: &canonical_nodes,
                    base_root: self.base_root,
                    mutation_generation: backing.generation,
                    policy: self.live.policy,
                },
                store: &self.store,
                workspace_id: self.workspace_id,
                reader: self.reader.clone(),
                base_inodes: self.base_inodes,
                spool: &self.spool,
            }
            .build(purpose, worker_limit, None);
        }
        let captured = self.take_capture();
        self.candidate_inputs()
            .build(purpose, worker_limit, captured)
    }

    fn candidate_inputs(&self) -> CandidateInputs<'_> {
        CandidateInputs {
            scope: self.inode_scope,
            serials: self.inode_serials.clone(),
            live: self.live.frozen_changes(),
            store: &self.store,
            workspace_id: self.workspace_id,
            reader: self.reader.clone(),
            base_inodes: self.base_inodes,
            spool: &self.spool,
        }
    }

    pub(crate) fn resolution_fingerprint(
        &mut self,
        affected_paths: &[CanonicalPath],
    ) -> Result<[u8; 32]> {
        let base = self.base_manifest()?;
        let final_view = self.final_manifest(manifest_charge(&base))?;
        let mut affected = affected_paths.iter().collect::<Vec<_>>();
        affected.sort();
        let mut digest = ContentDigestWriter::new();
        digest.write_all(b"layerfs/workspace-resolution/v2\0")?;
        for affected_path in affected {
            digest.write_all(b"A")?;
            frame(&mut digest, affected_path.as_bytes())?;
            for (path, entry) in final_view
                .iter()
                .filter(|(path, _)| path_intersects(path, affected_path.as_str()))
            {
                digest.write_all(b"E")?;
                frame(&mut digest, path.as_bytes())?;
                digest.write_all(&[match entry.attr.kind {
                    Kind::File => 1,
                    Kind::Directory => 2,
                    Kind::Symlink => 3,
                }])?;
                digest.write_all(&entry.attr.size.to_be_bytes())?;
                digest.write_all(&entry.attr.mode.to_be_bytes())?;
                digest.write_all(&entry.attr.links.to_be_bytes())?;
                digest.write_all(&entry.attr.mtime_seconds.to_be_bytes())?;
                digest.write_all(&entry.attr.mtime_nanoseconds.to_be_bytes())?;
                let node = self
                    .live
                    .nodes
                    .get(&entry.node)
                    .ok_or(StorageError::Integrity("resolution node"))?;
                digest.write_all(&(node.paths.len() as u64).to_be_bytes())?;
                for alias in &node.paths {
                    frame(&mut digest, alias.as_bytes())?;
                }
                match entry.attr.kind {
                    Kind::File => {
                        let mut reader = WorkspaceFileReader::new(self, entry.node)?;
                        std::io::copy(&mut reader, &mut digest)?;
                    }
                    Kind::Symlink => frame(&mut digest, &self.readlink(entry.node)?)?,
                    Kind::Directory => {}
                }
            }
            digest.write_all(b"Z")?;
        }
        Ok(digest.finish())
    }

    fn base_manifest(&self) -> Result<BTreeMap<String, BaseEntry>> {
        let reader = CoreReader(&self.reader);
        let mut output = BTreeMap::new();
        let mut charge = 0_u64;
        let mut pending = vec![CanonicalPath::root()];
        while let Some(directory) = pending.pop() {
            let mut after = None;
            loop {
                let (page, _) = filesystem::list(
                    &reader,
                    self.base_root,
                    &directory,
                    after.as_ref(),
                    128,
                    256 * 1024,
                )?;
                layerfs_layerstack_store::note_workspace_namespace_visits(
                    page.entries.len() as u64,
                    0,
                    0,
                    0,
                    0,
                );
                for (name, _) in page.entries {
                    let path = join(&directory, name.as_str())?;
                    let resolved = filesystem::resolve(
                        &reader,
                        self.base_root,
                        &path,
                        &mut LogicalCounters::default(),
                    )?;
                    if resolved.record.kind == InodeKind::Directory {
                        pending.push(path.clone());
                    }
                    let path = path.as_str().to_owned();
                    charge = charge.saturating_add(path_charge(&path));
                    output.insert(
                        path,
                        BaseEntry {
                            record: resolved.record,
                        },
                    );
                    self.live
                        .policy
                        .check_final_delta(charge)
                        .map_err(crate::live_error)?;
                }
                let Some(next) = page.continuation else { break };
                after = Some(next);
            }
        }
        Ok(output)
    }

    fn final_manifest(&mut self, base_charge: u64) -> Result<BTreeMap<String, FinalEntry>> {
        let mut output = BTreeMap::new();
        let mut charge = base_charge;
        let mut pending = vec![(ROOT, String::new())];
        while let Some((directory, prefix)) = pending.pop() {
            for (name, node) in self.directory_entries(directory)? {
                let dirty = self.live.dirty.contains(&node);
                layerfs_layerstack_store::note_workspace_namespace_visits(
                    0,
                    1,
                    u64::from(dirty),
                    u64::from(!dirty),
                    0,
                );
                let name = std::str::from_utf8(&name)
                    .map_err(|_| StorageError::Integrity("Workspace path"))?;
                let path = if prefix.is_empty() {
                    name.to_owned()
                } else {
                    format!("{prefix}/{name}")
                };
                let attr = self.attr(node)?;
                charge = charge.saturating_add(path_charge(&path));
                output.insert(path.clone(), FinalEntry { node, attr });
                self.live
                    .policy
                    .check_final_delta(charge)
                    .map_err(crate::live_error)?;
                if attr.kind == Kind::Directory {
                    pending.push((node, path));
                }
            }
        }
        Ok(output)
    }
}

// Host construction needs immutable inputs and backing, never a live Workspace.
struct CandidateInputs<'a> {
    scope: Option<ObjectId>,
    serials: std::sync::Arc<std::sync::Mutex<Option<std::ops::Range<u64>>>>,
    live: layerfs_workspace_core::FrozenWorkspaceChanges<'a>,
    store: &'a layerfs_layerstack_store::LayerStackStore,
    workspace_id: [u8; 16],
    reader: layerfs_layerstack_store::SnapshotReader,
    base_inodes: InodeTableRoot,
    spool: &'a std::path::Path,
}

impl CandidateInputs<'_> {
    fn prepare_inode_serials(&self) -> Result<()> {
        let mut serials = self
            .serials
            .lock()
            .map_err(|_| StorageError::Integrity("workspace inode reservation lock"))?;
        if serials.is_some() {
            return Ok(());
        }
        let Some(scope) = self.scope else {
            return Ok(());
        };
        let Some(maximum) = self
            .live
            .nodes
            .iter()
            .filter(|(_, node)| node.canonical.is_none() && !node.paths.is_empty())
            .map(|(id, _)| id.0)
            .max()
        else {
            return Ok(());
        };
        // ponytail: one 32-bit NodeId range per mutable Workspace; chunked
        // reservations are needed only for a lifetime exceeding 2^32 nodes.
        const WIDTH: u64 = 1u64 << 32;
        if maximum >= WIDTH {
            return Err(StorageError::InvalidInput("workspace inode serial range"));
        }
        *serials = Some(self.store.reserve_inode_serials(scope, WIDTH)?);
        Ok(())
    }

    fn build(
        &self,
        purpose: CandidatePurpose,
        worker_limit: usize,
        captured: Option<crate::capture::CapturedFile>,
    ) -> Result<PreparedCommit> {
        // Reserve before content admission opens its coalesced transaction.
        // Every use of a new live NodeId in this candidate shares the same serial.
        self.prepare_inode_serials()?;
        let started = Instant::now();
        self.live
            .policy
            .check_final_delta(1024)
            .map_err(crate::live_error)?;
        let batch_size =
            (self.live.policy.max_final_delta_memory_bytes / 4096).clamp(1, 128) as usize;
        let io_bytes = journal_io_bytes(self.live.policy.max_final_delta_memory_bytes);
        // Task-index and all worker-result buffers split one existing journal allowance.
        let frontier_budget = self
            .live
            .policy
            .max_final_delta_memory_bytes
            .saturating_sub(4 * io_bytes.saturating_sub(256) as u64);
        let tree_scratch = usize::try_from(frontier_budget.saturating_sub(1024) / 2)
            .unwrap_or(usize::MAX)
            .min(SORTED_TREE_UPDATE_SCRATCH_BYTES);
        if let Some(captured) = &captured {
            layerfs_layerstack_store::note_workspace_capture(1, captured.len);
        }
        let inputs = StableFileInputs {
            nodes: self.live.nodes,
            dirty: self.live.dirty,
            reader: self.reader.clone(),
            base_inodes: self.base_inodes,
            base_root: self.live.base_root,
            correspondence_reserved: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            generation: self.live.mutation_generation,
            spool: self.spool,
            io_bytes: io_bytes / 2,
            planning_bytes: self.live.policy.max_final_delta_memory_bytes,
            captured: std::sync::Mutex::new(captured),
        };
        note_commit_phase(WorkspaceCommitPhase::CandidatePlan, started);
        let content_started = Instant::now();
        let mut objects = ObjectBuffer::bounded_output(Some(&self.reader))?;
        let plan = inputs.prepare()?;
        // ponytail: predecessor plans use one producer so their cursor fits the
        // existing 1-MiB budget; share bounded correspondence scratch before widening.
        let workers = worker_limit
            .min(plan.count)
            .min(io_bytes / 256)
            .min(if plan.has_predecessor { 1 } else { 8 });
        if workers == 0 && plan.count != 0 {
            return Err(StorageError::InvalidInput("file producer budget"));
        }
        let tasks = plan.tasks(io_bytes / 2)?;
        let initialize = |worker| inputs.worker(worker, workers.max(1));
        let step =
            |worker: &mut FileResultWriter,
             ordinal,
             task: Result<NodeId>,
             writer: &mut layerfs_layerstack_store::FinalizedOutputWriter| {
                inputs.produce_file(worker, &plan.file, ordinal, task?, writer)
            };
        let finish = |worker: FileResultWriter| worker.finish();
        let (workers, admission) = match purpose {
            CandidatePurpose::Commit => {
                let (workers, admission) = self.store.construct_workspace_files(
                    self.workspace_id,
                    workers,
                    plan.count,
                    tasks,
                    initialize,
                    step,
                    finish,
                )?;
                (workers, Some(admission))
            }
            CandidatePurpose::Preview => (
                objects.construct_files(workers, plan.count, tasks, initialize, step, finish)?,
                None,
            ),
        };
        let mut files = FileResults::new(plan, workers, io_bytes / 2)?;
        drop(inputs);
        note_commit_phase(WorkspaceCommitPhase::Content, content_started);
        let started = Instant::now();
        let mut inodes = FrontierInodes::new(
            self.live.base_root,
            // The fixed 1 KiB allowance includes the first 256-byte map entry.
            1 + (frontier_budget.saturating_sub(1024) / 512) as usize,
            tree_scratch,
            self.spool,
        );
        #[cfg(test)]
        LAST_FRONTIER_STATS.with(|stats| {
            stats.set(Some(SpillStatCounters {
                batch_size: inodes.batch_size as u64,
                ..SpillStatCounters::default()
            }))
        });
        let mut metadata_cache = PortableMetadataCache::default();
        let mut references = ReferenceJournal::new(self.spool, io_bytes / 2);
        note_commit_phase(WorkspaceCommitPhase::CandidatePlan, started);
        let started = Instant::now();
        for &node in self.live.dirty {
            let value = self
                .live
                .nodes
                .get(&node)
                .ok_or(StorageError::Integrity("dirty node"))?;
            layerfs_layerstack_store::note_workspace_namespace_visits(0, 0, 0, 0, 1);
            if value.paths.is_empty() && value.links == 0 {
                continue;
            }
            let inode = self.frontier_inode(node)?;
            let file_result = if matches!(value.data, Data::File(_)) {
                Some(files.next(
                    node,
                    self.live.mutation_generation,
                    self.live.attr(node).map_err(crate::live_error)?.size,
                )?)
            } else {
                None
            };
            let before = if let Some((_, before)) = file_result {
                before
            } else {
                match value.canonical {
                    Some(_) => Some(inodes.record(&objects, inode)?),
                    None => None,
                }
            };
            let attr = self.live.attr(node).map_err(crate::live_error)?;
            let inode_kind = match attr.kind {
                Kind::File => InodeKind::RegularFile,
                Kind::Directory => InodeKind::Directory,
                Kind::Symlink => InodeKind::Symlink,
            };
            let content_root = match &value.data {
                Data::Directory(directory) => {
                    let mut content = match directory.base {
                        Some(base) => base,
                        None => empty_directory(&mut objects)?,
                    };
                    for desired in directory.changes.values() {
                        layerfs_layerstack_store::note_workspace_namespace_visits(
                            0,
                            u64::from(desired.is_some()),
                            0,
                            0,
                            0,
                        );
                    }
                    content = self.apply_frontier_directory(
                        &mut objects,
                        content,
                        &directory.changes,
                        batch_size,
                        tree_scratch,
                        &mut references,
                        !value.paths.is_empty(),
                    )?;
                    content.0
                }
                Data::Symlink(target) => match before {
                    Some(record) => record.content_root,
                    None => filesystem::symlink_content(&mut objects, target.clone())?,
                },
                Data::File(_) => file_result.expect("prepared file result").0,
            };
            let old_metadata = before
                .map(|record| {
                    portable_metadata(&CoreReader(&self.reader), record.metadata_root, record.kind)
                })
                .transpose()?;
            let metadata_root = if old_metadata.is_some_and(|metadata| {
                metadata.permission_mode == attr.mode
                    && metadata.mtime_seconds == attr.mtime_seconds
                    && metadata.mtime_nanoseconds == attr.mtime_nanoseconds
            }) {
                before.unwrap().metadata_root
            } else {
                metadata_cache
                    .get_or_build(
                        &mut objects,
                        inode_kind,
                        attr.mode,
                        attr.mtime_seconds,
                        attr.mtime_nanoseconds,
                    )?
                    .0
            };
            let record = InodeRecordV1 {
                kind: inode_kind,
                content_root,
                metadata_root,
                // New inodes have every final binding materialized. Existing
                // inodes can have unseen aliases and retain their stored count.
                namespace_ref_count: before.map_or(value.paths.len() as u64, |record| {
                    record.namespace_ref_count
                }),
            };
            layerfs_layerstack_store::note_workspace_namespace_visits(
                0,
                0,
                u64::from(before != Some(record)),
                u64::from(before == Some(record)),
                0,
            );
            inodes.set_checkpoint(inode, record, node, content_root)?;
        }
        files.finish_read()?;
        note_commit_phase(WorkspaceCommitPhase::Content, started);
        let started = Instant::now();
        let file_counters = files.counters;
        drop(files);
        inodes.apply_references(
            &objects,
            &CoreReader(&self.reader),
            references.finish()?,
            frontier_budget,
            io_bytes / 2,
        )?;
        let mut checkpoint = CheckpointJournal::new(self)?;
        inodes.finish(&mut objects, |objects, inode, node, content, record| {
            let attr = self.live.attr(node).map_err(crate::live_error)?;
            CheckpointJournal::validate_record(objects, &metadata_cache, record, content, attr)?;
            checkpoint.push(
                node,
                inode,
                content,
                attr,
                (attr.kind == Kind::File).then_some(attr.size),
            )
        })?;
        #[cfg(test)]
        LAST_FRONTIER_STATS.with(|stats| {
            stats.set(Some(inodes.stats.snapshot(inodes.batch_size)));
        });
        note_commit_phase(WorkspaceCommitPhase::Namespace, started);
        let started = Instant::now();
        let mut built = objects.finish(inodes.root, 0)?;
        add_build_counters(&mut built.counters, file_counters);
        let built = checkpoint
            .finish(built, self.live.mutation_generation)
            .map(|mut prepared| {
                prepared.admission = admission;
                prepared
            });
        note_commit_phase(WorkspaceCommitPhase::CandidateFinish, started);
        built
    }

    // Keep the existing frontier inputs explicit without adding a wrapper type.
    #[allow(clippy::too_many_arguments)]
    fn apply_frontier_directory(
        &self,
        objects: &mut ObjectBuffer<'_>,
        root: DirectoryStateRoot,
        changes: &BTreeMap<Vec<u8>, Option<NodeId>>,
        batch_size: usize,
        scratch_limit: usize,
        references: &mut ReferenceJournal<'_>,
        record_edges: bool,
    ) -> Result<DirectoryStateRoot> {
        let checkpoint = references.count;
        let mut edge_error = None;
        let mut source_error = None;
        let deltas = changes.iter().map(|(name, desired)| {
            let result: Result<_> = (|| {
                Ok((
                    CanonicalName::from_bytes(name)?,
                    desired
                        .map(|child| self.frontier_inode(child))
                        .transpose()?,
                ))
            })();
            match result {
                Ok(delta) => Ok(delta),
                Err(error) => {
                    source_error = Some(error);
                    Err(layerfs_content::CoreError::InvalidRecord(
                        "Workspace directory delta",
                    ))
                }
            }
        });
        let sorted = directory_apply_sorted_observed(
            objects,
            root,
            deltas,
            scratch_limit,
            |before, after| {
                if !record_edges {
                    return Ok(());
                }
                layerfs_layerstack_store::note_workspace_namespace_visits(
                    u64::from(before.is_some()),
                    u64::from(after.is_some()),
                    0,
                    0,
                    0,
                );
                // Only existing inodes need an added reference; new records already
                // carry every final alias. Original bindings always belong to base.
                let after = after.filter(|inode| self.live.canonical_nodes.contains_key(inode));
                references.push(before, after).map_err(|error| {
                    edge_error = Some(error);
                    layerfs_content::CoreError::Io
                })
            },
        );
        if let Some(error) = edge_error {
            return Err(error);
        }
        if let Some(error) = source_error {
            return Err(error);
        }
        match sorted {
            Ok((root, _)) => Ok(root),
            Err(
                layerfs_content::CoreError::ObjectLimitExceeded
                | layerfs_content::CoreError::Unsupported,
            ) => {
                references.rewind(checkpoint)?;
                let original = root;
                let mut root = root;
                let mut batch = Vec::with_capacity(batch_size);
                for (name, desired) in changes {
                    let name = CanonicalName::from_bytes(name)?;
                    let after = desired
                        .map(|child| self.frontier_inode(child))
                        .transpose()?;
                    if record_edges {
                        let before = directory_lookup(
                            objects,
                            original,
                            &name,
                            &mut NamespaceCounters::default(),
                        )?;
                        references.push(
                            before,
                            after.filter(|inode| self.live.canonical_nodes.contains_key(inode)),
                        )?;
                    }
                    batch.push((name, after));
                    if batch.len() == batch_size {
                        root =
                            filesystem::apply_directory_changes(objects, root, batch.drain(..))?.0;
                    }
                }
                if !batch.is_empty() {
                    root = filesystem::apply_directory_changes(objects, root, batch)?.0;
                }
                Ok(root)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn frontier_inode(&self, node: NodeId) -> Result<InodeId> {
        let value = self
            .live
            .nodes
            .get(&node)
            .ok_or(StorageError::Integrity("frontier node"))?;
        if let Some(inode) = value.canonical {
            return Ok(inode);
        }
        let path = value
            .paths
            .first()
            .ok_or(StorageError::Integrity("frontier path"))?;
        let batch_allowance =
            (self.live.policy.max_final_delta_memory_bytes / 4096).clamp(1, 128) * 1024;
        self.live
            .policy
            .check_final_delta(batch_allowance.saturating_add(path_charge(path)))
            .map_err(crate::live_error)?;
        layerfs_layerstack_store::note_workspace_namespace_visits(0, 1, 0, 0, 0);
        self.prepare_inode_serials()?;
        if let Some(range) = self
            .serials
            .lock()
            .map_err(|_| StorageError::Integrity("workspace inode reservation lock"))?
            .as_ref()
        {
            let serial = range
                .start
                .checked_add(node.0)
                .filter(|serial| *serial < range.end)
                .ok_or(StorageError::Integrity(
                    "workspace inode reservation coverage",
                ))?;
            return Ok(layerfs_content::tree::compact::InodeSerial::new(serial)?.inode_key());
        }
        // Bind new identity to this base snapshot: replacing one alias must not
        // accidentally reuse the still-live inode originally allocated at its path.
        Ok(filesystem::allocated_inode(
            self.live.base_root.to_bytes(),
            &CanonicalPath::new(path)?,
        ))
    }
}

struct StableFileInputs<'a> {
    nodes: &'a std::collections::HashMap<NodeId, crate::cow_tree::Node>,
    dirty: &'a BTreeSet<NodeId>,
    reader: layerfs_layerstack_store::SnapshotReader,
    base_inodes: InodeTableRoot,
    generation: u64,
    spool: &'a std::path::Path,
    io_bytes: usize,
    planning_bytes: u64,
    captured: std::sync::Mutex<Option<crate::capture::CapturedFile>>,
    base_root: ObjectId,
    correspondence_reserved: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

// Fixed task slots restore ordinal order without an in-memory completion map.
// Each slot holds NodeId, worker+1, journal offset and encoded result length.
const FILE_TASK_BYTES: u64 = 324;
struct FileTaskPlan {
    file: File,
    count: usize,
    generation: u64,
    has_predecessor: bool,
}
struct FileTasks {
    reader: BufReader<File>,
    remaining: usize,
}
impl Iterator for FileTasks {
    type Item = Result<NodeId>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let mut slot = [0; FILE_TASK_BYTES as usize];
        Some(
            self.reader
                .read_exact(&mut slot)
                .map(|()| NodeId(u64::from_le_bytes(slot[..8].try_into().unwrap())))
                .map_err(Into::into),
        )
    }
}
impl FileTaskPlan {
    fn tasks(&self, io_bytes: usize) -> Result<FileTasks> {
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        Ok(FileTasks {
            reader: BufReader::with_capacity(io_bytes, file),
            remaining: self.count,
        })
    }
}
struct FileResultWriter {
    worker: usize,
    partitions: usize,
    journal: BufWriter<File>,
    offset: u64,
    count: u64,
    counters: layerfs_layerstack_store::BuildCounters,
}
struct WorkerFileResults {
    worker: usize,
    file: File,
    io_bytes: usize,
    count: u64,
    counters: layerfs_layerstack_store::BuildCounters,
}
impl FileResultWriter {
    fn write_result(
        &mut self,
        index: &File,
        ordinal: usize,
        id: NodeId,
        len: u64,
        root: ObjectId,
        before: Option<InodeRecordV1>,
    ) -> Result<()> {
        let before = before.map(encode_inode_record).transpose()?;
        let mut record = Vec::with_capacity(308);
        record.extend_from_slice(&id.0.to_le_bytes());
        record.extend_from_slice(&len.to_le_bytes());
        record.extend_from_slice(root.as_bytes());
        record.extend_from_slice(&(before.as_ref().map_or(0, Vec::len) as u32).to_le_bytes());
        if let Some(before) = before {
            record.extend_from_slice(&before);
        }
        self.journal.write_all(&record)?;
        let mut location = [0; 24];
        location[..8].copy_from_slice(&(self.worker as u64 + 1).to_le_bytes());
        location[8..16].copy_from_slice(&self.offset.to_le_bytes());
        location[16..].copy_from_slice(&(record.len() as u64).to_le_bytes());
        let offset = (ordinal as u64)
            .checked_mul(FILE_TASK_BYTES)
            .and_then(|v| v.checked_add(8))
            .ok_or(StorageError::Integrity("file task offset"))?;
        index.write_all_at(&location, offset)?;
        self.offset = self
            .offset
            .checked_add(record.len() as u64)
            .ok_or(StorageError::Integrity("file result offset"))?;
        self.count += 1;
        Ok(())
    }

    fn finish(mut self) -> Result<WorkerFileResults> {
        self.journal.flush()?;
        let io_bytes = self.journal.capacity();
        let mut file = self
            .journal
            .into_inner()
            .map_err(|error| error.into_error())?;
        file.seek(SeekFrom::Start(0))?;
        Ok(WorkerFileResults {
            worker: self.worker,
            file,
            io_bytes,
            count: self.count,
            counters: self.counters,
        })
    }
}
struct FileResults {
    index: BufReader<File>,
    files: Vec<BufReader<File>>,
    offsets: Vec<u64>,
    remaining: u64,
    generation: u64,
    counters: layerfs_layerstack_store::BuildCounters,
}

fn add_build_counters(
    total: &mut layerfs_layerstack_store::BuildCounters,
    next: layerfs_layerstack_store::BuildCounters,
) {
    total.cdc_bytes_scanned = total
        .cdc_bytes_scanned
        .saturating_add(next.cdc_bytes_scanned);
    total.encode_hash_invocations = total
        .encode_hash_invocations
        .saturating_add(next.encode_hash_invocations);
    total.first_store_write_bytes = total
        .first_store_write_bytes
        .saturating_add(next.first_store_write_bytes);
    total.reachable_copy_write_bytes = total
        .reachable_copy_write_bytes
        .saturating_add(next.reachable_copy_write_bytes);
    total.spill_peak_bytes = total.spill_peak_bytes.max(next.spill_peak_bytes);
    total.spill_count = total.spill_count.saturating_add(next.spill_count);
}

// ponytail: unique removed basenames only; add similarity ranking only if measured
// remaining FULL bytes justify it. This catalogue never owns file payloads.
#[derive(Default)]
struct RemovedSmallCandidates {
    entries: Vec<(Vec<u8>, ObjectId)>,
}

impl RemovedSmallCandidates {
    const ENTRY_LIMIT: usize = 4096;
    const MEMORY_LIMIT: usize = 1024 * 1024;

    fn find(&self, name: &[u8]) -> Option<ObjectId> {
        let index = self
            .entries
            .partition_point(|entry| entry.0.as_slice() < name);
        let entry = self.entries.get(index)?;
        if entry.0 != name
            || self
                .entries
                .get(index + 1)
                .is_some_and(|next| next.0 == name)
        {
            return None;
        }
        Some(entry.1)
    }

    fn discover(inputs: &StableFileInputs<'_>, limit: usize) -> Result<Self> {
        if inputs.planning_bytes < 4 * 1024 * 1024
            || !inputs.reader.supports_small_predecessor_candidates()
            || inputs.dirty.len() > 16384
        {
            return Ok(Self::default());
        }
        let mut discovery = RemovedSmallDiscovery {
            candidates: Self {
                entries: Vec::with_capacity(Self::ENTRY_LIMIT),
            },
            directories: Vec::with_capacity(Self::ENTRY_LIMIT),
            name_bytes: 0,
            entries: 0,
            calls: 0,
        };
        let core = CoreReader(&inputs.reader);
        let mut changes = 0;
        let mut pending = Vec::with_capacity(limit);
        for id in inputs.dirty {
            let node = inputs
                .nodes
                .get(id)
                .ok_or(StorageError::Integrity("frozen removal node"))?;
            let Data::Directory(directory) = &node.data else {
                continue;
            };
            let Some(base) = directory.base else {
                continue;
            };
            for (name, value) in &directory.changes {
                changes += 1;
                if changes > 32768 {
                    return Ok(Self::default());
                }
                if value.is_some() {
                    continue;
                }
                pending.push((base, CanonicalName::from_bytes(name)?));
                if pending.len() == limit {
                    if !discovery.bindings(&inputs.reader, inputs.base_inodes, &pending, limit)? {
                        return Ok(Self::default());
                    }
                    pending.clear();
                }
            }
        }
        if !pending.is_empty()
            && !discovery.bindings(&inputs.reader, inputs.base_inodes, &pending, limit)?
        {
            return Ok(Self::default());
        }
        drop(pending);
        let mut visits = 0;
        while let Some((root, depth)) = discovery.directories.pop() {
            visits += 1;
            if visits > Self::ENTRY_LIMIT {
                return Ok(Self::default());
            }
            let mut after = None;
            loop {
                if !discovery.charge(0, 1) {
                    return Ok(Self::default());
                }
                let page = directory_page_after(
                    &core,
                    root,
                    after.as_ref(),
                    limit,
                    limit * 512,
                    &mut NamespaceCounters::default(),
                )?;
                if !discovery.charge(page.entries.len(), 0)
                    || !discovery.records(
                        &inputs.reader,
                        inputs.base_inodes,
                        &page.entries,
                        depth,
                        limit,
                    )?
                {
                    return Ok(Self::default());
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

struct RemovedSmallDiscovery {
    candidates: RemovedSmallCandidates,
    directories: Vec<(DirectoryStateRoot, usize)>,
    name_bytes: usize,
    entries: usize,
    calls: usize,
}

impl RemovedSmallDiscovery {
    fn charge(&mut self, entries: usize, calls: usize) -> bool {
        self.entries += entries;
        self.calls += calls;
        self.entries <= RemovedSmallCandidates::ENTRY_LIMIT && self.calls <= 8192
    }

    fn bindings(
        &mut self,
        reader: &layerfs_layerstack_store::SnapshotReader,
        base_inodes: InodeTableRoot,
        keys: &[(DirectoryStateRoot, CanonicalName)],
        limit: usize,
    ) -> Result<bool> {
        if !self.charge(keys.len(), 1) {
            return Ok(false);
        }
        let found = layerfs_content::tree::directory::directory_lookup_many(
            &CoreReader(reader),
            keys,
            &mut NamespaceCounters::default(),
        )?;
        let records: Vec<_> = keys
            .iter()
            .zip(found)
            .filter_map(|((_, name), inode)| inode.map(|id| (name.clone(), id)))
            .collect();
        self.records(reader, base_inodes, &records, 0, limit)
    }

    fn records(
        &mut self,
        reader: &layerfs_layerstack_store::SnapshotReader,
        base_inodes: InodeTableRoot,
        names: &[(CanonicalName, InodeId)],
        depth: usize,
        limit: usize,
    ) -> Result<bool> {
        if names.is_empty() {
            return Ok(true);
        }
        if !self.charge(0, 1) {
            return Ok(false);
        }
        let ids: Vec<_> = names.iter().map(|(_, id)| *id).collect();
        let records = FrontierInodes::base_records(&CoreReader(reader), base_inodes, &ids, limit)?;
        for ((name, _), record) in names.iter().zip(records) {
            match record.kind {
                InodeKind::RegularFile => {
                    let fixed = self.candidates.entries.capacity()
                        * std::mem::size_of::<(Vec<u8>, ObjectId)>()
                        + self.directories.capacity()
                            * std::mem::size_of::<(DirectoryStateRoot, usize)>();
                    if self.candidates.entries.len() == self.candidates.entries.capacity()
                        || fixed + self.name_bytes + name.as_bytes().len()
                            > RemovedSmallCandidates::MEMORY_LIMIT
                    {
                        return Ok(false);
                    }
                    let name = name.as_bytes().to_vec();
                    self.name_bytes += name.capacity();
                    if fixed + self.name_bytes > RemovedSmallCandidates::MEMORY_LIMIT {
                        return Ok(false);
                    }
                    self.candidates.entries.push((name, record.content_root));
                }
                InodeKind::Directory => {
                    if depth == 64 || self.directories.len() == self.directories.capacity() {
                        return Ok(false);
                    }
                    self.directories
                        .push((DirectoryStateRoot(record.content_root), depth + 1));
                }
                _ => {}
            }
        }
        Ok(true)
    }
}

impl StableFileInputs<'_> {
    fn prepare(&self) -> Result<FileTaskPlan> {
        let mut writer = BufWriter::with_capacity(self.io_bytes, anonymous_journal(self.spool)?);
        let mut count = 0_usize;
        let mut has_predecessor = false;
        // One 8-KiB canonical directory page per lookup plus decoded/request
        // ownership; callbacks discard each decoded node before the next one.
        let limit = (self.io_bytes / (16 * 1024)).clamp(1, 128);
        let removed = RemovedSmallCandidates::discover(self, limit)?;
        let mut page = Vec::with_capacity(limit);
        for &id in self.dirty {
            let node = self
                .nodes
                .get(&id)
                .ok_or(StorageError::Integrity("frozen file node"))?;
            if !matches!(node.data, Data::File(_)) || (node.paths.is_empty() && node.links == 0) {
                continue;
            }
            page.push(id);
            count = count
                .checked_add(1)
                .ok_or(StorageError::Integrity("file task count"))?;
            if page.len() == limit {
                has_predecessor |= self.prepare_page(&page, &removed, &mut writer)?;
                page.clear();
            }
        }
        if !page.is_empty() {
            has_predecessor |= self.prepare_page(&page, &removed, &mut writer)?;
        }
        writer.flush()?;
        Ok(FileTaskPlan {
            file: writer.into_inner().map_err(|error| error.into_error())?,
            count,
            generation: self.generation,
            has_predecessor,
        })
    }

    fn prepare_page(
        &self,
        page: &[NodeId],
        removed: &RemovedSmallCandidates,
        writer: &mut impl Write,
    ) -> Result<bool> {
        let core = CoreReader(&self.reader);
        let mut before = vec![None; page.len()];
        let known: Vec<_> = page
            .iter()
            .enumerate()
            .filter_map(|(slot, id)| self.nodes[id].canonical.map(|inode| (slot, inode)))
            .collect();
        let keys: Vec<_> = known.iter().map(|(_, inode)| *inode).collect();
        if !keys.is_empty() {
            for ((slot, _), record) in known.into_iter().zip(FrontierInodes::base_records(
                &core,
                self.base_inodes,
                &keys,
                page.len(),
            )?) {
                before[slot] = Some(record);
            }
        }
        let mut prior = before.clone();
        let mut paths: Vec<_> = page
            .iter()
            .enumerate()
            .filter_map(|(slot, id)| {
                if before[slot].is_some() {
                    return None;
                }
                self.nodes[id]
                    .paths
                    .first()
                    .map(|path| (slot, path.split('/')))
            })
            .collect();
        if !paths.is_empty() {
            let namespace = filesystem::namespace(&core, self.base_root)?;
            let root = FrontierInodes::base_records(
                &core,
                self.base_inodes,
                &[namespace.root_directory_inode],
                1,
            )?[0];
            for (slot, _) in &paths {
                prior[*slot] = Some(root);
            }
            // Directory states/nodes and inode records use bounded reader waves
            // at each path depth, including requests spanning distinct directories.
            loop {
                let mut lookups = Vec::with_capacity(page.len());
                for (slot, components) in &mut paths {
                    let Some(record) = prior[*slot] else {
                        continue;
                    };
                    let Some(component) = components.next() else {
                        continue;
                    };
                    if record.kind != InodeKind::Directory {
                        prior[*slot] = None;
                        continue;
                    }
                    lookups.push((*slot, record.content_root, component));
                }
                if lookups.is_empty() {
                    break;
                }
                lookups.sort_unstable_by_key(|(_, root, name)| (*root, *name));
                let mut slots = Vec::with_capacity(lookups.len());
                let mut keys = Vec::with_capacity(lookups.len());
                let directory_keys = lookups
                    .iter()
                    .map(|(_, root, component)| {
                        Ok((
                            DirectoryStateRoot(*root),
                            CanonicalName::from_bytes(component.as_bytes())?,
                        ))
                    })
                    .collect::<layerfs_content::CoreResult<Vec<_>>>()?;
                let found = layerfs_content::tree::directory::directory_lookup_many(
                    &core,
                    &directory_keys,
                    &mut NamespaceCounters::default(),
                )?;
                for ((slot, _, _), inode) in lookups.into_iter().zip(found) {
                    match inode {
                        Some(inode) => {
                            slots.push(slot);
                            keys.push(inode);
                        }
                        None => prior[slot] = None,
                    }
                }
                if !keys.is_empty() {
                    for (slot, record) in slots.into_iter().zip(FrontierInodes::base_records(
                        &core,
                        self.base_inodes,
                        &keys,
                        page.len(),
                    )?) {
                        prior[slot] = Some(record);
                    }
                }
            }
        }
        let mut has_predecessor = false;
        for (slot, id) in page.iter().enumerate() {
            let mut encoded = [0; FILE_TASK_BYTES as usize];
            encoded[..8].copy_from_slice(&id.0.to_le_bytes());
            let base = prior[slot]
                .filter(|record| record.kind == InodeKind::RegularFile)
                .map(|record| record.content_root)
                .or_else(|| {
                    let node = &self.nodes[id];
                    let size = node.attr(*id).size;
                    if before[slot].is_some() || size == 0 || size >= content::SMALL_LIMIT as u64 {
                        return None;
                    }
                    removed.find(node.paths.first()?.rsplit('/').next()?.as_bytes())
                });
            if let Some(base) = base {
                encoded[32..64].copy_from_slice(base.as_bytes());
                has_predecessor = true;
            }
            if let Some(record) = before[slot] {
                let record = encode_inode_record(record)?;
                if record.len() > 256 {
                    return Err(StorageError::Integrity("file predecessor record size"));
                }
                encoded[64..68].copy_from_slice(&(record.len() as u32).to_le_bytes());
                encoded[68..68 + record.len()].copy_from_slice(&record);
            }
            writer.write_all(&encoded)?;
        }
        Ok(has_predecessor)
    }

    fn worker(&self, worker: usize, workers: usize) -> Result<FileResultWriter> {
        Ok(FileResultWriter {
            worker,
            partitions: workers,
            journal: BufWriter::with_capacity(
                self.io_bytes / workers,
                anonymous_journal(self.spool)?,
            ),
            offset: 0,
            count: 0,
            counters: Default::default(),
        })
    }

    fn produce_file(
        &self,
        worker: &mut FileResultWriter,
        index: &File,
        ordinal: usize,
        id: NodeId,
        writer: &mut layerfs_layerstack_store::FinalizedOutputWriter,
    ) -> Result<()> {
        let node = self
            .nodes
            .get(&id)
            .ok_or(StorageError::Integrity("frozen file node"))?;
        let input = FrozenFile::from_node(&self.reader, node)?;
        let mut prepared = [0; FILE_TASK_BYTES as usize];
        index.read_exact_at(&mut prepared, ordinal as u64 * FILE_TASK_BYTES)?;
        if u64::from_le_bytes(prepared[..8].try_into().unwrap()) != id.0 {
            return Err(StorageError::Integrity("file predecessor task"));
        }
        let before_len = u32::from_le_bytes(prepared[64..68].try_into().unwrap()) as usize;
        if before_len > 256 {
            return Err(StorageError::Integrity("file predecessor record"));
        }
        let before = if before_len == 0 {
            None
        } else {
            Some(decode_inode_record(&prepared[68..68 + before_len])?)
        };
        let predecessor = if prepared[32..64].iter().all(|byte| *byte == 0) {
            None
        } else {
            Some(FileContentRoot(ObjectId::from_bytes(&prepared[32..64])?))
        };
        let captured = {
            let mut captured = self
                .captured
                .lock()
                .map_err(|_| StorageError::Integrity("captured file input"))?;
            if captured
                .as_ref()
                .is_some_and(|captured| captured.node == id)
            {
                captured.take()
            } else {
                None
            }
        };
        let (root, counters) = if writer.supports_small_content()
            && input.len > 0
            && input.len < content::SMALL_LIMIT as u64
            && captured.is_none()
        {
            if let Some(record) = before.filter(
                |r| matches!(&input.data, FileData::Base { root, .. } if root.0 == r.content_root),
            ) {
                (record.content_root, Default::default())
            } else {
                writer.build_small_file(
                    input.reader(),
                    input.len,
                    predecessor.map(|r| r.0).or(before.map(|r| r.content_root)),
                )?
            }
        } else if before.is_none() && predecessor.is_none() && captured.is_none() {
            // Complete new-file prefixes are final; keep the root private until
            // EOF/length validation and the enclosing task coverage both succeed.
            writer.build_complete_file(input.reader(), input.len)?
        } else {
            let built = input.build(
                before,
                predecessor,
                self.correspondence_reserved.clone(),
                captured,
                worker.partitions,
            )?;
            writer.send_selected(built.objects)?;
            (built.root_id, built.counters)
        };
        add_build_counters(&mut worker.counters, counters);
        worker.write_result(index, ordinal, id, input.len, root, before)
    }
}

impl FileResults {
    fn new(
        mut plan: FileTaskPlan,
        workers: Vec<WorkerFileResults>,
        io_bytes: usize,
    ) -> Result<Self> {
        let mut counters = layerfs_layerstack_store::BuildCounters::default();
        let mut count = 0_u64;
        let mut files = Vec::with_capacity(workers.len());
        for worker in workers {
            if worker.worker != files.len() {
                return Err(StorageError::Integrity("file result worker order"));
            }
            count += worker.count;
            // Sum worker-local spill peaks as an upper bound on concurrent spill.
            let peak = counters
                .spill_peak_bytes
                .saturating_add(worker.counters.spill_peak_bytes);
            add_build_counters(&mut counters, worker.counters);
            counters.spill_peak_bytes = peak;
            files.push(BufReader::with_capacity(worker.io_bytes, worker.file));
        }
        if count != plan.count as u64 {
            return Err(StorageError::Integrity("file result task coverage"));
        }
        plan.file.seek(SeekFrom::Start(0))?;
        Ok(Self {
            index: BufReader::with_capacity(io_bytes, plan.file),
            offsets: vec![0; files.len()],
            files,
            remaining: count,
            generation: plan.generation,
            counters,
        })
    }

    fn next(
        &mut self,
        node: NodeId,
        generation: u64,
        len: u64,
    ) -> Result<(ObjectId, Option<InodeRecordV1>)> {
        if generation != self.generation || self.remaining == 0 {
            return Err(StorageError::Integrity("file result generation"));
        }
        let mut slot = [0; FILE_TASK_BYTES as usize];
        self.index.read_exact(&mut slot)?;
        let worker = u64::from_le_bytes(slot[8..16].try_into().unwrap())
            .checked_sub(1)
            .and_then(|worker| usize::try_from(worker).ok())
            .ok_or(StorageError::Integrity("missing file result"))?;
        let offset = u64::from_le_bytes(slot[16..24].try_into().unwrap());
        let encoded_len = u64::from_le_bytes(slot[24..32].try_into().unwrap());
        if u64::from_le_bytes(slot[..8].try_into().unwrap()) != node.0
            || self.offsets.get(worker) != Some(&offset)
            || !(52..=308).contains(&encoded_len)
        {
            return Err(StorageError::Integrity("file result task identity"));
        }
        let file = &mut self.files[worker];
        let mut header = [0; 52];
        file.read_exact(&mut header)?;
        if u64::from_le_bytes(header[..8].try_into().unwrap()) != node.0
            || u64::from_le_bytes(header[8..16].try_into().unwrap()) != len
        {
            return Err(StorageError::Integrity("file result identity"));
        }
        let root = ObjectId::from_bytes(&header[16..48])?;
        let size = u32::from_le_bytes(header[48..52].try_into().unwrap()) as usize;
        let mut record = [0; 256];
        if size > record.len() || encoded_len != 52 + size as u64 {
            return Err(StorageError::Integrity("file result record"));
        }
        file.read_exact(&mut record[..size])?;
        self.offsets[worker] += encoded_len;
        self.remaining -= 1;
        Ok((
            root,
            if size == 0 {
                None
            } else {
                Some(decode_inode_record(&record[..size])?)
            },
        ))
    }
    fn finish_read(&mut self) -> Result<()> {
        if self.remaining != 0 || self.index.read(&mut [0; 1])? != 0 {
            return Err(StorageError::Integrity("file result coverage"));
        }
        for file in &mut self.files {
            if file.read(&mut [0; 1])? != 0 {
                return Err(StorageError::Integrity("file result journal coverage"));
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct FrozenFile {
    reader: layerfs_layerstack_store::SnapshotReader,
    data: FileData,
    len: u64,
}

impl FrozenFile {
    fn from_node(
        reader: &layerfs_layerstack_store::SnapshotReader,
        node: &crate::cow_tree::Node,
    ) -> Result<Self> {
        let Data::File(data) = &node.data else {
            return Err(StorageError::InvalidInput("file input"));
        };
        let len = match data {
            FileData::Base { len, .. } => *len,
            FileData::Edited { pieces, .. } => pieces.len(),
        };
        Ok(Self {
            reader: reader.clone(),
            data: data.clone(),
            len,
        })
    }

    fn read(&self, offset: u64, size: usize) -> Result<Vec<u8>> {
        crate::file_io::ReadPlan::for_file(self.reader.clone(), &self.data, offset, size)?.read()
    }

    fn reader(&self) -> WorkspaceFileReader {
        let source = match &self.data {
            FileData::Edited {
                base: None, pieces, ..
            } if pieces.compact_spool().is_some() => {
                let slice = pieces.compact_spool().unwrap();
                WorkspaceFileSource::Direct(slice.segment.clone(), slice.offset)
            }
            _ => WorkspaceFileSource::Mixed(self.clone()),
        };
        WorkspaceFileReader {
            source,
            offset: 0,
            len: self.len,
        }
    }
    fn build(
        &self,
        before: Option<InodeRecordV1>,
        predecessor: Option<FileContentRoot>,
        correspondence_reserved: std::sync::Arc<std::sync::atomic::AtomicU64>,
        captured: Option<crate::capture::CapturedFile>,
        partitions: usize,
    ) -> Result<BuiltRoot> {
        let (mut objects, captured_root) = match captured {
            Some(captured) => {
                if captured.len != self.len {
                    return Err(StorageError::Integrity("captured file length"));
                }
                (
                    ObjectBuffer::resume_prevalidated(&self.reader, captured.objects),
                    Some((captured.root, captured.counters)),
                )
            }
            None => (ObjectBuffer::bounded_output(Some(&self.reader))?, None),
        };
        objects.diagnostic_file_payloads();
        objects.partition_output(partitions)?;
        if let Some(predecessor) = predecessor {
            objects.set_physical_predecessor(
                self.reader.clone(),
                predecessor,
                correspondence_reserved,
            )?;
        }

        let small_result = ObjectStore::small_content_format(&objects)
            && self.len > 0
            && self.len < content::SMALL_LIMIT as u64;
        let small_base = if let Some(record) = before {
            matches!(
                content::inspect(
                    &CoreReader(&self.reader),
                    FileContentRoot(record.content_root)
                )?,
                content::Content::Small { .. }
            )
        } else {
            false
        };
        if captured_root.is_none() && (small_result || small_base) {
            if let Some(record) = before {
                if !self.file_may_differ(record.content_root)? {
                    return objects.finish(record.content_root, 0);
                }
            }
            return objects.build_complete_with_predecessor(self.reader(), self.len);
        }
        let (root, counters) = if let Some(captured) = captured_root {
            captured
        } else if let Some(record) = before {
            if !self.file_may_differ(record.content_root)? {
                (
                    FileContentRoot(record.content_root),
                    RopeCounters::default(),
                )
            } else {
                match self.mutate_existing_file(&mut objects, BaseEntry { record })? {
                    Some(changed) => changed,
                    None if self.incremental_file_supported(record.content_root) => (
                        FileContentRoot(record.content_root),
                        RopeCounters::default(),
                    ),
                    None => {
                        return objects.build_complete_with_predecessor(self.reader(), self.len)
                    }
                }
            }
        } else {
            return objects.build_complete_with_predecessor(self.reader(), self.len);
        };
        if content::length(&objects, root)? != self.len {
            return Err(StorageError::Integrity("completed file length"));
        }
        objects.finish(root.0, counters.cdc_bytes_scanned)
    }

    fn file_may_differ(&self, base: ObjectId) -> Result<bool> {
        match &self.data {
            FileData::Base { root, .. } if root.0 == base => Ok(false),
            FileData::Edited {
                base: Some((root, base_len)),
                pieces,
                ..
            } if root.0 == base => Ok(pieces.len() != *base_len
                || !matches!(pieces.pieces().as_slice(), [crate::file_edit::Piece::Base { root: piece_root, offset: 0, len }] if *piece_root == *root && *len == *base_len)),
            _ => Ok(!self.file_matches(base)?),
        }
    }

    fn incremental_file_supported(&self, base: ObjectId) -> bool {
        matches!(
            &self.data,
            FileData::Edited {
                base: Some((root, _)),
                ..
            } if root.0 == base
        )
    }

    fn mutate_existing_file(
        &self,
        objects: &mut ObjectBuffer<'_>,
        base: BaseEntry,
    ) -> Result<Option<(FileContentRoot, RopeCounters)>> {
        let FileData::Edited {
            base: Some((file_root, _)),
            pieces,
            ..
        } = &self.data
        else {
            return Ok(None);
        };
        if file_root.0 != base.record.content_root {
            return Ok(None);
        }
        let (file_root, pieces) = (*file_root, pieces.pieces());
        let mut batch = FileMutationBatch::new(objects, Some(rope::FileStateRoot(file_root.0)))?;
        let mut changed = false;
        let original_len = layerfs_content::file::rope::state(
            &CoreReader(&self.reader),
            rope::FileStateRoot(file_root.0),
            &mut RopeCounters::default(),
        )?
        .logical_len;
        let mut base_cursor = 0_u64;
        let mut final_cursor = 0_u64;
        let mut replacement_len = 0_u64;
        for piece in pieces {
            match piece {
                crate::file_edit::Piece::Base { root, offset, len } => {
                    if root != file_root || offset < base_cursor {
                        return Err(StorageError::Integrity("Workspace base piece order"));
                    }
                    let delete_len = offset - base_cursor;
                    if (delete_len != 0 || replacement_len != 0)
                        && (delete_len != replacement_len
                            || final_cursor != base_cursor
                            || !self.workspace_range_matches_base(
                                file_root,
                                final_cursor,
                                final_cursor + replacement_len,
                            )?)
                    {
                        batch.replace(
                            final_cursor,
                            delete_len,
                            WorkspaceRangeReader::new(self, final_cursor, replacement_len)?,
                        )?;
                        changed = true;
                    }
                    final_cursor += replacement_len + len;
                    replacement_len = 0;
                    base_cursor = offset + len;
                }
                piece => {
                    replacement_len = replacement_len
                        .checked_add(piece.len())
                        .ok_or(StorageError::InvalidInput("file length"))?
                }
            }
        }
        let delete_len = original_len
            .checked_sub(base_cursor)
            .ok_or(StorageError::Integrity("Workspace base piece order"))?;
        if (delete_len != 0 || replacement_len != 0)
            && (delete_len != replacement_len
                || final_cursor != base_cursor
                || !self.workspace_range_matches_base(
                    file_root,
                    final_cursor,
                    final_cursor + replacement_len,
                )?)
        {
            batch.replace(
                final_cursor,
                delete_len,
                WorkspaceRangeReader::new(self, final_cursor, replacement_len)?,
            )?;
            changed = true;
        }
        if batch.logical_len()? != self.len {
            return Err(StorageError::Integrity("Workspace file mutation length"));
        }
        if !changed {
            return Ok(None);
        }
        let (root, counters) = batch.finish()?;
        Ok(Some((FileContentRoot(root.0), counters)))
    }

    fn workspace_range_matches_base(
        &self,
        base: FileContentRoot,
        start: u64,
        end: u64,
    ) -> Result<bool> {
        let mut offset = start;
        while offset < end {
            let count = (end - offset).min(64 * 1024) as usize;
            let final_bytes = self.read(offset, count)?;
            let mut base_bytes = Vec::with_capacity(count);
            content::read_range(
                &CoreReader(&self.reader),
                base,
                offset..offset + count as u64,
                &mut base_bytes,
            )?;
            if final_bytes != base_bytes {
                return Ok(false);
            }
            offset += count as u64;
        }
        Ok(true)
    }

    fn file_matches(&self, base: ObjectId) -> Result<bool> {
        match &self.data {
            FileData::Base { root, .. } if root.0 == base => return Ok(true),
            FileData::Edited {
                base: Some((root, base_len)),
                pieces,
                ..
            } if root.0 == base
                && pieces.len() == *base_len
                && matches!(pieces.pieces().as_slice(), [crate::file_edit::Piece::Base { root: piece_root, offset: 0, len }] if *piece_root == *root && *len == *base_len) =>
            {
                return Ok(true)
            }
            _ => {}
        }
        let mut final_digest = ContentDigestWriter::new();
        let mut input = self.reader();
        std::io::copy(&mut input, &mut final_digest)?;
        let mut base_digest = ContentDigestWriter::new();
        content::read_all(
            &CoreReader(&self.reader),
            FileContentRoot(base),
            &mut base_digest,
        )?;
        Ok(final_digest.finish() == base_digest.finish())
    }
}

#[cfg(any(debug_assertions, feature = "test-instrumentation"))]
thread_local! {
    static INJECT_CANDIDATE_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(any(debug_assertions, feature = "test-instrumentation"))]
pub(crate) fn inject_candidate_failure_once() {
    INJECT_CANDIDATE_FAILURE.with(|inject| inject.set(true));
}

#[cfg(test)]
thread_local! {
    static INJECT_INODE_MERGE_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    // Fail after this many successful row writes; exercises partial output and
    // later carry failures without filling the host's disk.
    static INJECT_SPILL_WRITE_FAILURE: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
    // Stage 3 proof hook: the counters of the most recent real candidate build.
    static LAST_FRONTIER_STATS: std::cell::Cell<Option<SpillStatCounters>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
impl Workspace {
    /// Deterministic spill counters of the most recent candidate build on this
    /// thread. Test-only; the release product carries no spill telemetry.
    fn frontier_stats(&self) -> SpillStatCounters {
        LAST_FRONTIER_STATS
            .with(|stats| stats.get())
            .expect("no candidate build recorded spill counters on this thread")
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct SpillStatCounters {
    record_reads: u64,
    record_writes: u64,
    bytes_read: u64,
    bytes_written: u64,
    batch_flushes: u64,
    merges: u64,
    merge_levels: u64,
    peak_live_runs: u64,
    peak_open_files: u64,
    peak_scratch_bytes: u64,
    peak_live_bytes: u64,
    peak_allocated_bytes: u64,
    peak_merge_read_bytes: u64,
    spill_keys: u64,
    batch_size: u64,
    tree_attempts: u64,
    tree_fallbacks: u64,
    fallback_inodes: u64,
}

// Records are instrumented on `&self` paths, so the counters use interior
// mutability; a frontier is built on one thread and never shared.
#[cfg(test)]
#[derive(Default, Debug)]
struct SpillStats {
    record_reads: std::cell::Cell<u64>,
    record_writes: std::cell::Cell<u64>,
    bytes_read: std::cell::Cell<u64>,
    bytes_written: std::cell::Cell<u64>,
    batch_flushes: std::cell::Cell<u64>,
    merges: std::cell::Cell<u64>,
    merge_levels: std::cell::Cell<u64>,
    peak_live_runs: std::cell::Cell<u64>,
    peak_open_files: std::cell::Cell<u64>,
    peak_scratch_bytes: std::cell::Cell<u64>,
    peak_live_bytes: std::cell::Cell<u64>,
    peak_allocated_bytes: std::cell::Cell<u64>,
    peak_merge_read_bytes: std::cell::Cell<u64>,
    spill_keys: std::cell::Cell<u64>,
    tree_attempts: std::cell::Cell<u64>,
    tree_fallbacks: std::cell::Cell<u64>,
    fallback_inodes: std::cell::Cell<u64>,
}

#[cfg(test)]
impl SpillStats {
    fn snapshot(&self, batch_size: usize) -> SpillStatCounters {
        SpillStatCounters {
            record_reads: self.record_reads.get(),
            record_writes: self.record_writes.get(),
            bytes_read: self.bytes_read.get(),
            bytes_written: self.bytes_written.get(),
            batch_flushes: self.batch_flushes.get(),
            merges: self.merges.get(),
            merge_levels: self.merge_levels.get(),
            peak_live_runs: self.peak_live_runs.get(),
            peak_open_files: self.peak_open_files.get(),
            peak_scratch_bytes: self.peak_scratch_bytes.get(),
            peak_live_bytes: self.peak_live_bytes.get(),
            peak_allocated_bytes: self.peak_allocated_bytes.get(),
            peak_merge_read_bytes: self.peak_merge_read_bytes.get(),
            spill_keys: self.spill_keys.get(),
            batch_size: batch_size as u64,
            tree_attempts: self.tree_attempts.get(),
            tree_fallbacks: self.tree_fallbacks.get(),
            fallback_inodes: self.fallback_inodes.get(),
        }
    }
}

#[cfg(test)]
macro_rules! spill_note {
    ($this:expr, $field:ident, $value:expr) => {
        $this
            .stats
            .$field
            .set($this.stats.$field.get().saturating_add($value as u64));
    };
}
#[cfg(not(test))]
macro_rules! spill_note {
    ($this:expr, $field:ident, $value:expr) => {{
        let _ = &$this;
        let _ = $value;
    }};
}

#[cfg(test)]
macro_rules! spill_peak {
    ($this:expr, $field:ident, $value:expr) => {
        $this
            .stats
            .$field
            .set($this.stats.$field.get().max($value as u64));
    };
}
#[cfg(not(test))]
macro_rules! spill_peak {
    ($this:expr, $field:ident, $value:expr) => {{
        let _ = &$this;
        let _ = $value;
    }};
}

#[derive(Clone, Copy)]
struct FrontierValue {
    record: Option<InodeRecordV1>,
    checkpoint: Option<(NodeId, ObjectId)>,
}

// One immutable sorted journal of the fixed 192-byte frontier records.
// Runs are only ever appended or replaced whole, so a failed write leaves the
// previous run valid until the replacement has been flushed and installed.
struct SpillRun {
    file: File,
    count: u64,
    first: InodeId,
    last: InodeId,
}

impl SpillRun {
    fn contains(&self, inode: InodeId) -> bool {
        self.first <= inode && inode <= self.last
    }
}

// Final inode changes are coalesced before touching the immutable base table.
// Level i contains 2^i batches (possibly fewer rows after deduplication). Carries
// never saturate: a checked u64 batch count bounds the levels and descriptors.
// Lower occupied levels are newer; all updates enter the bounded pending map.
struct FrontierInodes {
    root: ObjectId,
    pending: BTreeMap<InodeId, FrontierValue>,
    batch_size: usize,
    scratch_limit: usize,
    directory: std::path::PathBuf,
    // Consolidated sorted journal, installed by `finish` (or by the test hook).
    spill: Option<File>,
    spilled_count: u64,
    runs: Vec<Option<SpillRun>>,
    generation: u64,
    // Presence prefilter over every spilled key: no false negatives, so a miss
    // skips the tier scan entirely without changing which records are visible.
    filter: Vec<u64>,
    filter_bits: u64,
    scratch: Option<std::sync::Arc<ScratchBudget>>,
    #[cfg(test)]
    stats: SpillStats,
}

const RUN_ROW_BYTES: usize = 192;
const MAX_MERGE_BUFFER_BYTES: usize = 16 * 1024;

fn init_filter(batch_size: usize) -> (Vec<u64>, u64) {
    // Charge the same reservation the map uses so the prefilter cannot become a
    // second frontier-sized allocation: 1 byte per reserved entry, rounded up.
    let bytes = batch_size
        .checked_next_power_of_two()
        .unwrap_or(usize::MAX)
        .clamp(8, 4 * 1024 * 1024);
    (vec![0_u64; bytes / 8], (bytes * 8) as u64)
}

// Two independent probes; the second is derived from the first so one pass over
// the 32-byte key feeds the whole filter.
fn filter_probe(key: &[u8; 32], odd: u64) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in key {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    if odd == 1 {
        let mut mixed = u64::from_le_bytes(key[..8].try_into().unwrap());
        for &byte in &key[8..] {
            mixed ^= u64::from(byte);
            mixed = mixed.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash = mixed;
    }
    hash
}

impl FrontierInodes {
    fn new(
        root: ObjectId,
        batch_size: usize,
        scratch_limit: usize,
        directory: &std::path::Path,
    ) -> Self {
        let batch_size = batch_size.max(1);
        let (filter, filter_bits) = init_filter(batch_size);
        Self {
            root,
            pending: BTreeMap::new(),
            batch_size,
            scratch_limit,
            directory: directory.to_owned(),
            spill: None,
            spilled_count: 0,
            runs: Vec::new(),
            generation: 0,
            filter,
            filter_bits,
            scratch: None,
            #[cfg(test)]
            stats: SpillStats::default(),
        }
    }

    fn live_runs(&self) -> u64 {
        self.runs.iter().flatten().count() as u64
    }

    fn live_run_bytes(&self) -> u64 {
        self.runs
            .iter()
            .flatten()
            .map(|run| run.count * RUN_ROW_BYTES as u64)
            .sum::<u64>()
            + self.spilled_count * RUN_ROW_BYTES as u64
    }

    #[cfg(test)]
    fn run_record_count(&self) -> u64 {
        self.runs.iter().flatten().map(|run| run.count).sum()
    }

    // Read one row from a run and charge the I/O it actually caused.
    fn read_next(
        &self,
        rows: &mut RunRows,
        file: &File,
        remaining: &mut u64,
    ) -> Result<Option<(InodeId, FrontierValue)>> {
        let next = rows.next(file, remaining)?;
        if let Some((_, read)) = &next {
            spill_note!(self, record_reads, 1);
            spill_note!(self, bytes_read, *read);
        }
        Ok(next.map(|(row, _)| row))
    }

    fn spill_memory_bytes(&self) -> usize {
        self.runs.capacity() * std::mem::size_of::<Option<SpillRun>>()
            + self.filter.capacity() * std::mem::size_of::<u64>()
    }

    fn merge_buffer_bytes(&self) -> Result<usize> {
        // The other 256 bytes per reserved map entry and the fixed allowance
        // fund spill metadata and three buffers. Retain the existing map charge;
        // BTreeMap's allocator overhead is not an observable Vec capacity.
        let available = self
            .batch_size
            .saturating_mul(256)
            .saturating_add(512)
            .saturating_sub(self.spill_memory_bytes());
        let bytes = (self.io_bytes() / 4)
            .clamp(RUN_ROW_BYTES, MAX_MERGE_BUFFER_BYTES)
            .min((available / 3).max(RUN_ROW_BYTES));
        if available < 2 * RUN_ROW_BYTES {
            return Err(StorageError::InvalidInput("frontier spill memory budget"));
        }
        Ok(bytes / RUN_ROW_BYTES * RUN_ROW_BYTES)
    }

    fn filter_bits_of(&self, inode: &InodeId, bits: &mut [u64; 2]) {
        bits[0] = filter_probe(inode.as_bytes(), 0) % self.filter_bits;
        bits[1] = filter_probe(inode.as_bytes(), 1) % self.filter_bits;
    }

    fn filter_maybe_contains(&self, inode: &InodeId) -> bool {
        let mut bits = [0_u64; 2];
        self.filter_bits_of(inode, &mut bits);
        bits.into_iter()
            .all(|bit| self.filter[(bit / 64) as usize] & (1_u64 << (bit % 64)) != 0)
    }

    fn spill_run(&self, level: usize) -> Option<&SpillRun> {
        self.runs.get(level).and_then(Option::as_ref)
    }

    // Immutable sorted keys; account key probes separately from decoded rows.
    fn run_row(&self, level: usize, inode: InodeId) -> Result<Option<u64>> {
        let Some(run) = self.spill_run(level) else {
            return Ok(None);
        };
        if !run.contains(inode) {
            return Ok(None);
        }
        let (mut start, mut end) = (0_u64, run.count);
        while start < end {
            let middle = start + (end - start) / 2;
            let mut key = [0; 32];
            run.file
                .read_exact_at(&mut key, middle * RUN_ROW_BYTES as u64)?;
            spill_note!(self, bytes_read, key.len());
            match InodeId(key).cmp(&inode) {
                std::cmp::Ordering::Equal => return Ok(Some(middle)),
                std::cmp::Ordering::Less => start = middle + 1,
                std::cmp::Ordering::Greater => end = middle,
            }
        }
        Ok(None)
    }

    fn spilled(&mut self, inode: InodeId) -> Result<Option<(usize, u64, FrontierValue)>> {
        if self.runs.iter().all(Option::is_none) || !self.filter_maybe_contains(&inode) {
            return Ok(None);
        }
        // Level 0 is the newest data, so the first level that holds the key wins.
        for level in 0..self.runs.len() {
            if let Some(row) = self.run_row(level, inode)? {
                let value = self.read_row(level, row)?.1;
                spill_note!(self, record_reads, 1);
                spill_note!(self, bytes_read, RUN_ROW_BYTES);
                return Ok(Some((level, row, value)));
            }
        }
        Ok(None)
    }
}

impl FrontierInodes {
    #[cfg(test)]
    fn read(file: &File, index: u64) -> Result<(InodeId, FrontierValue)> {
        let mut bytes = [0; RUN_ROW_BYTES];
        file.read_exact_at(&mut bytes, index * RUN_ROW_BYTES as u64)?;
        Self::decode(&bytes)
    }

    fn decode(bytes: &[u8; 192]) -> Result<(InodeId, FrontierValue)> {
        Ok((
            InodeId(bytes[..32].try_into().unwrap()),
            FrontierValue {
                record: if bytes[32] == 0 {
                    None
                } else {
                    Some(InodeRecordV1 {
                        kind: InodeKind::try_from(bytes[32])?,
                        namespace_ref_count: u64::from_le_bytes(bytes[40..48].try_into().unwrap()),
                        content_root: ObjectId::from_bytes(&bytes[48..80])?,
                        metadata_root: ObjectId::from_bytes(&bytes[80..112])?,
                    })
                },
                checkpoint: if bytes[112] == 0 {
                    None
                } else {
                    Some((
                        NodeId(u64::from_le_bytes(bytes[120..128].try_into().unwrap())),
                        ObjectId::from_bytes(&bytes[128..160])?,
                    ))
                },
            },
        ))
    }

    fn encode(inode: InodeId, value: FrontierValue) -> [u8; RUN_ROW_BYTES] {
        let mut bytes = [0; 192];
        bytes[..32].copy_from_slice(inode.as_bytes());
        if let Some(record) = value.record {
            bytes[32] = record.kind as u8;
            bytes[40..48].copy_from_slice(&record.namespace_ref_count.to_le_bytes());
            bytes[48..80].copy_from_slice(record.content_root.as_bytes());
            bytes[80..112].copy_from_slice(record.metadata_root.as_bytes());
        }
        if let Some((node, content)) = value.checkpoint {
            bytes[112] = 1;
            bytes[120..128].copy_from_slice(&node.0.to_le_bytes());
            bytes[128..160].copy_from_slice(content.as_bytes());
        }
        bytes
    }

    fn io_bytes(&self) -> usize {
        // The unused half of each pending-entry reservation funds run I/O.
        self.batch_size
            .saturating_sub(1)
            .saturating_mul(128)
            .clamp(128, 64 * 1024)
    }

    fn record(&mut self, objects: &ObjectBuffer<'_>, inode: InodeId) -> Result<InodeRecordV1> {
        self.record_with_base(objects, inode, None)
    }

    fn record_with_base(
        &mut self,
        objects: &ObjectBuffer<'_>,
        inode: InodeId,
        base: Option<InodeRecordV1>,
    ) -> Result<InodeRecordV1> {
        if let Some(record) = self.pending.get(&inode) {
            return record
                .record
                .ok_or(StorageError::Integrity("released frontier inode"));
        }
        if let Some((_, _, record)) = self.spilled(inode)? {
            return record
                .record
                .ok_or(StorageError::Integrity("released frontier inode"));
        }
        self.base_record(objects, inode, base)
    }

    fn base_record(
        &self,
        objects: &ObjectBuffer<'_>,
        inode: InodeId,
        base: Option<InodeRecordV1>,
    ) -> Result<InodeRecordV1> {
        if let Some(record) = base {
            return Ok(record);
        }
        let namespace = filesystem::namespace(objects, self.root)?;
        layerfs_content::tree::inode::inode_record_lookup(
            objects,
            InodeTableRoot(namespace.inode_table_root),
            inode,
            &mut InodeTableCounters::default(),
        )?
        .ok_or(StorageError::Integrity("frontier inode record"))
    }

    #[cfg(test)]
    fn set(&mut self, inode: InodeId, record: Option<InodeRecordV1>) -> Result<()> {
        self.set_value(inode, record, None)
    }

    fn set_checkpoint(
        &mut self,
        inode: InodeId,
        record: InodeRecordV1,
        node: NodeId,
        content: ObjectId,
    ) -> Result<()> {
        self.set_value(inode, Some(record), Some((node, content)))
    }

    fn set_value(
        &mut self,
        inode: InodeId,
        record: Option<InodeRecordV1>,
        checkpoint: Option<(NodeId, ObjectId)>,
    ) -> Result<()> {
        if let Some(pending) = self.pending.get_mut(&inode) {
            pending.record = record;
            pending.checkpoint = checkpoint.or(pending.checkpoint);
        } else if let Some((_, _, mut value)) = self.spilled(inode)? {
            value.record = record;
            value.checkpoint = checkpoint.or(value.checkpoint);
            self.insert_new(inode, value)?;
        } else {
            self.insert_new(inode, FrontierValue { record, checkpoint })?;
        }
        Ok(())
    }

    fn insert_new(&mut self, inode: InodeId, value: FrontierValue) -> Result<()> {
        if self.pending.len() == self.batch_size {
            self.merge_pending()?;
        }
        self.pending.insert(inode, value);
        Ok(())
    }

    fn change_references(
        &mut self,
        objects: &ObjectBuffer<'_>,
        inode: InodeId,
        base: Option<InodeRecordV1>,
        amount: u64,
        additions: bool,
    ) -> Result<InodeRecordV1> {
        let adjust = |value: &mut FrontierValue| -> Result<InodeRecordV1> {
            let mut record = value
                .record
                .ok_or(StorageError::Integrity("released frontier inode"))?;
            record.namespace_ref_count = if additions {
                record
                    .namespace_ref_count
                    .checked_add(amount)
                    .ok_or(StorageError::Integrity("namespace reference overflow"))?
            } else {
                record
                    .namespace_ref_count
                    .checked_sub(amount)
                    .ok_or(StorageError::Integrity("namespace reference underflow"))?
            };
            value.record = (record.namespace_ref_count != 0).then_some(record);
            Ok(record)
        };
        if let Some(value) = self.pending.get_mut(&inode) {
            return adjust(value);
        }
        if let Some((_, _, mut value)) = self.spilled(inode)? {
            let record = adjust(&mut value)?;
            self.insert_new(inode, value)?;
            return Ok(record);
        }
        let mut value = FrontierValue {
            record: Some(self.base_record(objects, inode, base)?),
            checkpoint: None,
        };
        let record = adjust(&mut value)?;
        self.insert_new(inode, value)?;
        Ok(record)
    }

    // Keep every original until the entire carry succeeds. A partial output is
    // anonymous and closes on error; retry sees exactly the previous frontier.
    fn merge_pending(&mut self) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(StorageError::Integrity("frontier spill generation"))?;
        generation
            .checked_mul(self.batch_size as u64)
            .filter(|rows| *rows <= u64::MAX / (3 * RUN_ROW_BYTES as u64))
            .ok_or(StorageError::Integrity("frontier spill bytes"))?;
        let level = self
            .runs
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.runs.len());
        if level >= u64::BITS as usize {
            return Err(StorageError::Integrity("frontier spill levels"));
        }
        if level == self.runs.len() {
            let bytes = (level + 1) * std::mem::size_of::<Option<SpillRun>>()
                + self.filter.capacity() * std::mem::size_of::<u64>()
                + 2 * RUN_ROW_BYTES;
            let relocation =
                self.spill_memory_bytes() + (level + 1) * std::mem::size_of::<Option<SpillRun>>();
            if bytes.max(relocation) > self.batch_size.saturating_mul(256).saturating_add(512) {
                return Err(StorageError::InvalidInput("frontier spill memory budget"));
            }
            let old_allocation = self.runs.capacity() * std::mem::size_of::<Option<SpillRun>>();
            self.runs
                .try_reserve_exact(1)
                .map_err(|_| StorageError::InvalidInput("frontier spill memory budget"))?;
            self.runs.push(None);
            // Include a realloc implementation that briefly retains the old block.
            spill_peak!(
                self,
                peak_scratch_bytes,
                self.spill_memory_bytes() + old_allocation
            );
        }
        let mut run = self.write_batch()?;
        #[cfg(test)]
        if INJECT_INODE_MERGE_FAILURE.with(|inject| inject.replace(false)) {
            return Err(StorageError::Integrity("injected inode merge failure"));
        }
        for previous in self.runs[..level].iter().flatten() {
            run = self.merge_runs(previous, &run, true)?;
        }
        for previous in &mut self.runs[..level] {
            *previous = None;
        }
        self.runs[level] = Some(run);
        self.generation = generation;
        for inode in self.pending.keys() {
            let mut bits = [0_u64; 2];
            self.filter_bits_of(inode, &mut bits);
            for bit in bits {
                self.filter[(bit / 64) as usize] |= 1_u64 << (bit % 64);
            }
        }
        self.pending.clear();
        spill_peak!(self, merge_levels, level);
        Ok(())
    }

    #[cfg(test)]
    fn note_spill_allocation(&self, output: &File, extra: Option<&File>) -> Result<()> {
        use std::os::unix::fs::MetadataExt;
        let mut bytes = output.metadata()?.blocks() * 512;
        for file in self.runs.iter().flatten().map(|run| &run.file).chain(extra) {
            bytes += file.metadata()?.blocks() * 512;
        }
        spill_peak!(self, peak_allocated_bytes, bytes);
        Ok(())
    }

    fn write_spill_row(
        &self,
        writer: &mut BufWriter<File>,
        inode: InodeId,
        value: FrontierValue,
    ) -> Result<()> {
        #[cfg(test)]
        if INJECT_SPILL_WRITE_FAILURE.with(|inject| match inject.get() {
            Some(0) => {
                inject.set(None);
                true
            }
            Some(rows) => {
                inject.set(Some(rows - 1));
                false
            }
            None => false,
        }) {
            writer.flush()?;
            self.note_spill_allocation(writer.get_ref(), None)?;
            return Err(std::io::Error::from_raw_os_error(28).into());
        }
        writer.write_all(&Self::encode(inode, value))?;
        spill_note!(self, record_writes, 1);
        spill_note!(self, bytes_written, RUN_ROW_BYTES);
        Ok(())
    }

    fn write_batch(&self) -> Result<SpillRun> {
        let buffer = self.merge_buffer_bytes()?;
        let file = scoped_journal(&self.directory, self.scratch.as_ref())?;
        let mut writer = BufWriter::with_capacity(buffer, file);
        let count = self.pending.len() as u64;
        spill_peak!(
            self,
            peak_scratch_bytes,
            self.spill_memory_bytes() + writer.capacity()
        );
        spill_peak!(self, peak_live_runs, self.live_runs() + 1);
        spill_peak!(self, peak_open_files, self.live_runs() + 1);
        spill_peak!(
            self,
            peak_live_bytes,
            self.live_run_bytes() + count * RUN_ROW_BYTES as u64
        );
        for (inode, value) in &self.pending {
            self.write_spill_row(&mut writer, *inode, *value)?;
        }
        writer.flush()?;
        #[cfg(test)]
        self.note_spill_allocation(writer.get_ref(), None)?;
        let file = writer.into_inner().map_err(|error| error.into_error())?;
        spill_note!(self, batch_flushes, 1);
        Ok(SpillRun {
            file,
            count,
            first: *self.pending.first_key_value().unwrap().0,
            last: *self.pending.last_key_value().unwrap().0,
        })
    }

    // Borrow originals until the caller installs the complete output. A carry
    // owns one additional input; retained originals + carry + output use <= 3S
    // bytes for S total rows submitted in batches, and <= 64 + 2 descriptors.
    fn merge_runs(
        &self,
        older: &SpillRun,
        newer: &SpillRun,
        extra_input: bool,
    ) -> Result<SpillRun> {
        let input_count = older
            .count
            .checked_add(newer.count)
            .filter(|count| *count <= u64::MAX / RUN_ROW_BYTES as u64)
            .ok_or(StorageError::Integrity("frontier spill bytes"))?;
        let buffer = self.merge_buffer_bytes()?;
        let file = scoped_journal(&self.directory, self.scratch.as_ref())?;
        // One-row reads use direct output writes under very small policies.
        let mut writer =
            BufWriter::with_capacity(if buffer == RUN_ROW_BYTES { 0 } else { buffer }, file);
        let mut old_rows = RunRows::new(buffer);
        let mut new_rows = RunRows::new(buffer);
        spill_peak!(
            self,
            peak_scratch_bytes,
            self.spill_memory_bytes()
                + writer.capacity()
                + old_rows.buffer.capacity()
                + new_rows.buffer.capacity()
        );
        spill_peak!(
            self,
            peak_open_files,
            self.live_runs() + 1 + u64::from(extra_input)
        );
        spill_peak!(
            self,
            peak_live_runs,
            self.live_runs() + 1 + u64::from(extra_input)
        );
        spill_peak!(
            self,
            peak_live_bytes,
            self.live_run_bytes()
                + (input_count + if extra_input { newer.count } else { 0 }) * RUN_ROW_BYTES as u64
        );
        spill_peak!(
            self,
            peak_merge_read_bytes,
            input_count * RUN_ROW_BYTES as u64
        );
        let (mut old_remaining, mut new_remaining) = (older.count, newer.count);
        let mut old = self.read_next(&mut old_rows, &older.file, &mut old_remaining)?;
        let mut new = self.read_next(&mut new_rows, &newer.file, &mut new_remaining)?;
        let mut count = 0_u64;
        let mut first = None;
        let mut last = None;
        while old.is_some() || new.is_some() {
            let take_new = match (&old, &new) {
                (Some(old), Some(new)) => new.0 <= old.0,
                (Some(_), None) => false,
                (None, Some(_)) => true,
                (None, None) => break,
            };
            let (inode, value) = if take_new {
                let row = new.take().expect("newer spill row");
                if old.as_ref().is_some_and(|entry| entry.0 == row.0) {
                    old = self.read_next(&mut old_rows, &older.file, &mut old_remaining)?;
                }
                new = self.read_next(&mut new_rows, &newer.file, &mut new_remaining)?;
                row
            } else {
                let row = old.take().expect("older spill row");
                old = self.read_next(&mut old_rows, &older.file, &mut old_remaining)?;
                row
            };
            let written = self.write_spill_row(&mut writer, inode, value);
            #[cfg(test)]
            if written.is_err() {
                self.note_spill_allocation(writer.get_ref(), extra_input.then_some(&newer.file))?;
            }
            written?;
            first.get_or_insert(inode);
            last = Some(inode);
            count += 1; // checked input_count bounds the output and byte offsets
        }
        writer.flush()?;
        #[cfg(test)]
        self.note_spill_allocation(writer.get_ref(), extra_input.then_some(&newer.file))?;
        let file = writer.into_inner().map_err(|error| error.into_error())?;
        spill_note!(self, merges, 1);
        Ok(SpillRun {
            file,
            count,
            first: first.ok_or(StorageError::Integrity("frontier spill rows"))?,
            last: last.ok_or(StorageError::Integrity("frontier spill rows"))?,
        })
    }

    fn read_row(&self, level: usize, row: u64) -> Result<(InodeId, FrontierValue)> {
        let file = &self.runs[level].as_ref().unwrap().file;
        let mut bytes = [0; RUN_ROW_BYTES];
        file.read_exact_at(&mut bytes, row * RUN_ROW_BYTES as u64)?;
        Self::decode(&bytes)
    }

    fn finalize(&mut self) -> Result<()> {
        if self.spill.is_some() {
            return Ok(());
        }
        self.merge_pending()?;
        let mut sources = self.runs.iter().flatten();
        let Some(first) = sources.next() else {
            return Ok(());
        };
        let mut combined = None;
        // Ascending binary tiers: each original batch participates in at most
        // log2(F) final merges, even when deduplication changes the row counts.
        // Only two readers are allocated, independent of the number of tiers.
        for older in sources {
            combined = Some(self.merge_runs(
                older,
                combined.as_ref().unwrap_or(first),
                combined.is_some(),
            )?);
        }
        let run = match combined {
            Some(run) => run,
            None => self.runs.iter_mut().find_map(Option::take).unwrap(),
        };
        self.spilled_count = run.count;
        self.spill = Some(run.file);
        self.runs.clear();
        spill_note!(self, spill_keys, self.spilled_count);
        Ok(())
    }
}

// Sequential 192-byte reader over one immutable run. A run is written with a
// single sequential pass, so a private buffer bounds the reads per row.
struct RunRows {
    offset: u64,
    buffer: Vec<u8>,
    filled: usize,
    consumed: usize,
}

impl RunRows {
    fn new(buffer_bytes: usize) -> Self {
        // Whole rows only, so no row straddles a refill and the read size always
        // matches the remaining file length.
        let rows = (buffer_bytes / RUN_ROW_BYTES).max(1);
        Self {
            offset: 0,
            buffer: vec![0; rows * RUN_ROW_BYTES],
            filled: 0,
            consumed: 0,
        }
    }

    // Returns the next row and the bytes actually read from the file for it, so
    // the caller can charge real I/O rather than a notional row size.
    fn next(
        &mut self,
        file: &File,
        remaining: &mut u64,
    ) -> Result<Option<((InodeId, FrontierValue), u64)>> {
        if *remaining == 0 {
            return Ok(None);
        }
        let mut read = 0_u64;
        // The buffer is a whole number of rows, so a row never straddles a refill:
        // the next buffer starts exactly where the previous one ended.
        if self.consumed == self.filled {
            let rows = (*remaining).min((self.buffer.len() / RUN_ROW_BYTES) as u64) as usize;
            let filled = rows * RUN_ROW_BYTES;
            file.read_exact_at(&mut self.buffer[..filled], self.offset)?;
            self.consumed = 0;
            self.filled = filled;
            read = filled as u64;
        }
        let bytes: &[u8; RUN_ROW_BYTES] = self.buffer[self.consumed..self.consumed + RUN_ROW_BYTES]
            .try_into()
            .unwrap();
        let row = FrontierInodes::decode(bytes)?;
        self.consumed += RUN_ROW_BYTES;
        self.offset += RUN_ROW_BYTES as u64;
        *remaining -= 1;
        Ok(Some((row, read)))
    }
}

impl FrontierInodes {
    fn finish(
        &mut self,
        objects: &mut ObjectBuffer<'_>,
        mut checkpoint: impl FnMut(
            &ObjectBuffer<'_>,
            InodeId,
            NodeId,
            ObjectId,
            InodeRecordV1,
        ) -> Result<()>,
    ) -> Result<()> {
        let mut encode = |objects: &mut ObjectBuffer<'_>,
                          inode: InodeId,
                          value: FrontierValue|
         -> Result<Option<ObjectId>> {
            if let Some((node, content)) = value.checkpoint {
                checkpoint(
                    objects,
                    inode,
                    node,
                    content,
                    value
                        .record
                        .ok_or(StorageError::Integrity("released checkpoint inode"))?,
                )?;
            }
            value
                .record
                .map(|record| {
                    objects
                        .put_owned(encode_inode_record(record)?)
                        .map_err(Into::into)
                })
                .transpose()
        };
        let mut memory = Vec::new();
        if self.spill.is_none() && self.runs.iter().all(Option::is_none) {
            // Consume the bounded map: memory-resident final changes need no journal.
            memory.reserve_exact(self.pending.len());
            while let Some((inode, record)) = self.pending.pop_first() {
                let id = encode(objects, inode, record)?;
                memory.push((inode, id));
            }
        } else {
            self.finalize()?;
            let file = self.spill.as_ref().unwrap();
            // Encode once and attach IDs in sequential blocks, preserving raw
            // records for diagnostics and the same sorted-run representation.
            let rows_per_block = (self.io_bytes() / 192).max(1);
            let mut bytes = vec![0; rows_per_block * 192];
            let mut first = 0;
            while first < self.spilled_count {
                let rows = (self.spilled_count - first).min(rows_per_block as u64) as usize;
                let block = &mut bytes[..rows * 192];
                file.read_exact_at(block, first * 192)?;
                for row in block.chunks_exact_mut(192) {
                    let (inode, value) = Self::decode((&*row).try_into().unwrap())?;
                    if let Some(id) = encode(objects, inode, value)? {
                        row[160..192].copy_from_slice(id.as_bytes());
                    }
                }
                file.write_all_at(block, first * 192)?;
                first += rows as u64;
            }
        }
        let count = if self.spill.is_some() {
            self.spilled_count
        } else {
            memory.len() as u64
        };
        if count == 0 {
            return Ok(());
        }
        let namespace = filesystem::namespace(objects, self.root)?;
        let mut reader = self
            .spill
            .as_ref()
            .map(|file| BufReader::with_capacity(self.io_bytes(), file));
        if let Some(reader) = &mut reader {
            reader.seek(SeekFrom::Start(0))?;
        }
        let mut source_error = None;
        let deltas = (0..count).map(|index| {
            let result = Self::final_delta(&mut reader, &memory, index);
            result.map_err(|error| {
                source_error = Some(error);
                layerfs_content::CoreError::InvalidRecord("Workspace inode delta")
            })
        });
        spill_note!(self, tree_attempts, 1);
        let sorted = inode_table_apply_sorted_with_budget(
            objects,
            InodeTableRoot(namespace.inode_table_root),
            deltas,
            self.scratch_limit,
        );
        if let Some(error) = source_error {
            return Err(error);
        }
        self.root = match sorted {
            Ok((table, _)) if table.0 == namespace.inode_table_root => self.root,
            Ok((table, _)) => objects.put_owned(encode_namespace_root(NamespaceRootV1 {
                inode_table_root: table.0,
                ..namespace
            })?)?,
            Err(
                layerfs_content::CoreError::ObjectLimitExceeded
                | layerfs_content::CoreError::Unsupported,
            ) => {
                spill_note!(self, tree_fallbacks, 1);
                spill_note!(self, fallback_inodes, count);
                if let Some(reader) = &mut reader {
                    reader.seek(SeekFrom::Start(0))?;
                }
                let mut root = self.root;
                for index in 0..count {
                    let (inode, id) = Self::final_delta(&mut reader, &memory, index)?;
                    let mutation = match id {
                        Some(id) => InodeMutation::Upsert {
                            inode,
                            record: ObjectStore::with_authenticated_canonical(
                                objects,
                                id,
                                decode_inode_record,
                            )?,
                        },
                        None => InodeMutation::Remove { inode },
                    };
                    root = filesystem::apply_inode_mutations(objects, root, [mutation])?.root();
                }
                root
            }
            Err(error) => return Err(error.into()),
        };
        Ok(())
    }

    fn final_delta(
        reader: &mut Option<BufReader<&File>>,
        memory: &[(InodeId, Option<ObjectId>)],
        index: u64,
    ) -> Result<(InodeId, Option<ObjectId>)> {
        if let Some(reader) = reader {
            let mut bytes = [0; 192];
            reader.read_exact(&mut bytes)?;
            Ok((
                InodeId(bytes[..32].try_into().unwrap()),
                if bytes[32] == 0 {
                    None
                } else {
                    Some(ObjectId::from_bytes(&bytes[160..192])?)
                },
            ))
        } else {
            Ok(memory[index as usize])
        }
    }

    fn base_records(
        base: &CoreReader<'_>,
        table: InodeTableRoot,
        keys: &[InodeId],
        lookup_limit: usize,
    ) -> Result<Vec<InodeRecordV1>> {
        let mut records = Vec::with_capacity(keys.len());
        for keys in keys.chunks(lookup_limit) {
            for record in layerfs_content::tree::inode::inode_record_lookup_many(
                base,
                table,
                keys,
                &mut InodeTableCounters::default(),
            )? {
                records.push(record.ok_or(StorageError::Integrity("referenced inode record"))?);
            }
        }
        Ok(records)
    }

    fn apply_references(
        &mut self,
        objects: &ObjectBuffer<'_>,
        base: &CoreReader<'_>,
        journal: Option<(File, u64)>,
        budget: u64,
        io_bytes: usize,
    ) -> Result<()> {
        self.apply_references_observed(objects, base, journal, budget, io_bytes, &mut |_| Ok(()))
    }

    fn apply_references_observed(
        &mut self,
        objects: &ObjectBuffer<'_>,
        base: &CoreReader<'_>,
        journal: Option<(File, u64)>,
        budget: u64,
        io_bytes: usize,
        removed: &mut impl FnMut(InodeId) -> Result<()>,
    ) -> Result<()> {
        let Some((file, count)) = journal else {
            return Ok(());
        };
        let namespace = filesystem::namespace(objects, self.root)?;
        // Complete additions before any zero-reference traversal. Within each
        // bounded page aliases coalesce, while current changes override prefetch.
        for additions in [true, false] {
            let mut reader = BufReader::with_capacity(io_bytes, &file);
            reader.seek(SeekFrom::Start(0))?;
            let mut remaining = count;
            while remaining != 0 {
                let held = self.pending.len() as u64 * 256;
                let limit =
                    (budget.saturating_sub(held + 1024) / (32 * 1024 + 512)).clamp(1, 128) as usize;
                let mut changes = BTreeMap::<InodeId, u64>::new();
                for _ in 0..remaining.min(limit as u64) {
                    let mut row = [0; 65];
                    reader.read_exact(&mut row)?;
                    if row[0] == 0 || row[0] > 3 {
                        return Err(StorageError::Integrity("reference journal flags"));
                    }
                    let (flag, offset) = if additions { (2, 33) } else { (1, 1) };
                    if row[0] & flag != 0 {
                        *changes
                            .entry(InodeId(row[offset..offset + 32].try_into().unwrap()))
                            .or_default() += 1;
                    }
                    remaining -= 1;
                }
                if changes.is_empty() {
                    continue;
                }
                let started = Instant::now();
                let keys = changes.keys().copied().collect::<Vec<_>>();
                let records = Self::base_records(
                    base,
                    InodeTableRoot(namespace.inode_table_root),
                    &keys,
                    limit,
                )?;
                note_commit_phase(WorkspaceCommitPhase::DeletionRecords, started);
                let retained = keys.len() as u64 * 512;
                drop(keys);
                for ((inode, amount), record) in changes.into_iter().zip(records) {
                    if additions {
                        self.change_references(objects, inode, Some(record), amount, true)?;
                    } else {
                        self.release(
                            objects,
                            base,
                            inode,
                            Some(record),
                            amount,
                            budget.saturating_sub(retained),
                            removed,
                        )?;
                    }
                }
            }
            if reader.read(&mut [0; 1])? != 0 {
                return Err(StorageError::Integrity("reference journal coverage"));
            }
        }
        Ok(())
    }

    fn release(
        &mut self,
        objects: &ObjectBuffer<'_>,
        base: &CoreReader<'_>,
        inode: InodeId,
        prefetched: Option<InodeRecordV1>,
        amount: u64,
        budget: u64,
        removed: &mut impl FnMut(InodeId) -> Result<()>,
    ) -> Result<()> {
        struct Cursor {
            root: DirectoryStateRoot,
            after: Option<CanonicalName>,
            children: std::collections::VecDeque<(InodeId, InodeRecordV1)>,
            finished: bool,
        }
        let namespace = filesystem::namespace(objects, self.root)?;
        let mut directories: Vec<Cursor> = Vec::new();
        let mut next = Some((inode, prefetched, amount));
        loop {
            if let Some((inode, prefetched, amount)) = next.take() {
                let started = Instant::now();
                // Earlier additions/releases override authenticated page prefetch.
                let record = self.change_references(objects, inode, prefetched, amount, false)?;
                note_commit_phase(WorkspaceCommitPhase::DeletionRecords, started);
                if record.namespace_ref_count == 0 {
                    removed(inode)?;
                }
                if record.namespace_ref_count == 0 && record.kind == InodeKind::Directory {
                    directories.push(Cursor {
                        root: DirectoryStateRoot(record.content_root),
                        after: None,
                        children: Default::default(),
                        finished: false,
                    });
                }
            }
            let held = self.pending.len() as u64 * 256
                + directories
                    .iter()
                    .map(|cursor| 512 + cursor.children.capacity() as u64 * 144)
                    .sum::<u64>();
            let Some(cursor) = directories.last_mut() else {
                break;
            };
            if let Some((inode, record)) = cursor.children.pop_front() {
                next = Some((inode, Some(record), 1));
                continue;
            }
            if cursor.finished {
                directories.pop();
                continue;
            }
            let limit = budget.saturating_sub(held.saturating_add(1024)) / 1024;
            if limit == 0 {
                return Err(StorageError::InvalidInput("workspace final-delta limit"));
            }
            let limit = limit.min(128) as usize;
            let started = Instant::now();
            let page = directory_page_after(
                objects,
                cursor.root,
                cursor.after.as_ref(),
                limit,
                limit * 512,
                &mut NamespaceCounters::default(),
            )?;
            note_commit_phase(WorkspaceCommitPhase::DeletionCursor, started);
            layerfs_layerstack_store::note_workspace_namespace_visits(
                page.entries.len() as u64,
                0,
                0,
                0,
                0,
            );
            cursor.finished = page.continuation.is_none();
            cursor.after = page.continuation;
            let started = Instant::now();
            let inodes = page
                .entries
                .iter()
                .map(|(_, inode)| *inode)
                .collect::<Vec<_>>();
            // Removed directories originate in the immutable base. Batch their
            // records there, then preserve sequential changes at consumption above.
            // A lookup batch also holds decoded and raw inode-table pages.
            // Small policies keep the existing single-key canonical read width.
            let lookup_limit = (budget.saturating_sub(held + limit as u64 * 1024) / (32 * 1024))
                .clamp(1, 128) as usize;
            let records = Self::base_records(
                base,
                InodeTableRoot(namespace.inode_table_root),
                &inodes,
                lookup_limit,
            )?;
            cursor.children = inodes.into_iter().zip(records).collect();
            note_commit_phase(WorkspaceCommitPhase::DeletionRecords, started);
        }
        Ok(())
    }
}

fn note_commit_phase(phase: WorkspaceCommitPhase, started: Instant) {
    layerfs_layerstack_store::note_workspace_commit_phase(
        phase,
        started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
    );
}

fn frame(output: &mut impl Write, value: &[u8]) -> Result<()> {
    output.write_all(&(value.len() as u64).to_be_bytes())?;
    output.write_all(value)?;
    Ok(())
}

fn path_intersects(left: &str, right: &str) -> bool {
    left == right || path_is_ancestor(left, right) || path_is_ancestor(right, left)
}

fn path_is_ancestor(parent: &str, child: &str) -> bool {
    parent.is_empty()
        || (child.starts_with(parent) && child.as_bytes().get(parent.len()) == Some(&b'/'))
}

fn manifest_charge<T>(manifest: &BTreeMap<String, T>) -> u64 {
    manifest.keys().map(|path| path_charge(path)).sum()
}

fn path_charge(path: &str) -> u64 {
    (path.len() as u64).saturating_mul(4).saturating_add(512)
}

struct WorkspaceFileReader {
    source: WorkspaceFileSource,
    offset: u64,
    len: u64,
}

enum WorkspaceFileSource {
    Direct(layerfs_workspace_core::backing::BackingRef, u64),
    Mixed(FrozenFile),
}

struct WorkspaceRangeReader<'a> {
    input: &'a FrozenFile,
    offset: u64,
    end: u64,
}

impl<'a> WorkspaceRangeReader<'a> {
    fn new(input: &'a FrozenFile, offset: u64, len: u64) -> Result<Self> {
        let end = offset
            .checked_add(len)
            .ok_or(StorageError::InvalidInput("file range"))?;
        if end > input.len {
            return Err(StorageError::InvalidInput("file range"));
        }
        Ok(Self { input, offset, end })
    }
}

impl Read for WorkspaceRangeReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.offset == self.end || output.is_empty() {
            return Ok(0);
        }
        let bytes = self
            .input
            .read(
                self.offset,
                output.len().min((self.end - self.offset) as usize),
            )
            .map_err(std::io::Error::other)?;
        output[..bytes.len()].copy_from_slice(&bytes);
        self.offset += bytes.len() as u64;
        Ok(bytes.len())
    }
}

impl WorkspaceFileReader {
    fn new(workspace: &Workspace, node: NodeId) -> Result<Self> {
        Ok(FrozenFile::from_node(
            &workspace.reader,
            workspace
                .live
                .nodes
                .get(&node)
                .ok_or(StorageError::NotFound("node"))?,
        )?
        .reader())
    }
}

impl Read for WorkspaceFileReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.offset == self.len || output.is_empty() {
            return Ok(0);
        }
        let count = output.len().min((self.len - self.offset) as usize);
        match &self.source {
            WorkspaceFileSource::Direct(file, start) => {
                let mut read = 0;
                while read < count {
                    let next = crate::file_io::spool_segment(file)
                        .map_err(std::io::Error::other)?
                        .file
                        .read_at(&mut output[read..count], start + self.offset + read as u64)?;
                    if next == 0 {
                        return Err(std::io::ErrorKind::UnexpectedEof.into());
                    }
                    read += next;
                }
                self.offset += read as u64;
                Ok(read)
            }
            WorkspaceFileSource::Mixed(input) => {
                let bytes = input
                    .read(self.offset, count)
                    .map_err(std::io::Error::other)?;
                output[..bytes.len()].copy_from_slice(&bytes);
                self.offset += bytes.len() as u64;
                Ok(bytes.len())
            }
        }
    }
}

fn kind(kind: InodeKind) -> Kind {
    match kind {
        InodeKind::RegularFile => Kind::File,
        InodeKind::Directory => Kind::Directory,
        InodeKind::Symlink => Kind::Symlink,
    }
}

fn join(parent: &CanonicalPath, name: &str) -> Result<CanonicalPath> {
    let value = if parent.is_root() {
        name.to_owned()
    } else {
        format!("{}/{name}", parent.as_str())
    };
    CanonicalPath::new(&value).map_err(Into::into)
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = self.clear_spool();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_layerstack_store::{
        CommitOutcome, EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
    };
    use std::collections::BTreeSet;

    fn empty_workspace(label: &str) -> (std::path::PathBuf, Workspace) {
        let root = std::env::temp_dir().join(format!(
            "layerfs-direct-spool-{label}-{}-{}",
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
                EntityName::new(label).unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let workspace = Workspace::open(store, branch, root.join("spool")).unwrap();
        (root, workspace)
    }

    #[test]
    fn removed_small_candidates_preserve_identity_history_and_bounds() {
        for remove_parent in [false, true] {
            let (root, mut workspace) = empty_workspace("removed-small");
            let old_dir = workspace.mkdir(ROOT, b"old", 0o750).unwrap().node;
            let old_file = workspace
                .create_file(old_dir, b"payload", 0o640)
                .unwrap()
                .node;
            let data: Vec<_> = (0..8192_u32).flat_map(u32::to_le_bytes).collect();
            workspace.write(old_file, 0, &data).unwrap();
            let left = workspace.mkdir(ROOT, b"left", 0o700).unwrap().node;
            let right = workspace.mkdir(ROOT, b"right", 0o700).unwrap().node;
            for (dir, bytes) in [(left, b"left".as_slice()), (right, b"right".as_slice())] {
                let file = workspace
                    .create_file(dir, b"ambiguous", 0o600)
                    .unwrap()
                    .node;
                workspace.write(file, 0, bytes).unwrap();
            }
            workspace.commit().unwrap();
            let old = workspace.reader.clone();
            let old_root = workspace.base_root;
            let path = CanonicalPath::new("old/payload").unwrap();
            let content_root = filesystem::resolve(
                &CoreReader(&old),
                old_root,
                &path,
                &mut LogicalCounters::default(),
            )
            .unwrap()
            .record
            .content_root;
            workspace.unlink(old_dir, b"payload", false).unwrap();
            if remove_parent {
                workspace.unlink(ROOT, b"old", true).unwrap();
            }
            workspace.unlink(left, b"ambiguous", false).unwrap();
            workspace.unlink(right, b"ambiguous", false).unwrap();
            let new_dir = workspace.mkdir(ROOT, b"new", 0o750).unwrap().node;
            let new_file = workspace
                .create_file(new_dir, b"payload", 0o600)
                .unwrap()
                .node;
            let mut changed = data.clone();
            changed[100..104].copy_from_slice(b"edit");
            workspace.write(new_file, 0, &changed).unwrap();
            {
                let mut inputs = StableFileInputs {
                    nodes: &workspace.live.nodes,
                    dirty: &workspace.live.dirty,
                    reader: workspace.reader.clone(),
                    base_inodes: workspace.base_inodes,
                    generation: workspace.live.mutation_generation,
                    spool: &workspace.spool,
                    io_bytes: 32768,
                    planning_bytes: 8 * 1024 * 1024,
                    captured: std::sync::Mutex::new(None),
                    base_root: workspace.base_root,
                    correspondence_reserved: std::sync::Arc::new(
                        std::sync::atomic::AtomicU64::new(0),
                    ),
                };
                let candidates = RemovedSmallCandidates::discover(&inputs, 2).unwrap();
                assert_eq!(candidates.find(b"payload"), Some(content_root));
                assert_eq!(candidates.find(b"ambiguous"), None);
                assert_eq!(candidates.find(b"absent"), None);
                let plan = inputs.prepare().unwrap();
                assert!(plan.has_predecessor);
                assert_eq!(plan.count, 1);
                let mut slot = [0; FILE_TASK_BYTES as usize];
                plan.file.read_exact_at(&mut slot, 0).unwrap();
                assert_eq!(&slot[32..64], content_root.as_bytes());
                assert_eq!(
                    &slot[64..68],
                    &[0; 4],
                    "compression hint must not become before inode"
                );
                inputs.planning_bytes = 1024;
                assert!(RemovedSmallCandidates::discover(&inputs, 2)
                    .unwrap()
                    .entries
                    .is_empty());
                inputs.planning_bytes = 8 * 1024 * 1024;
                let too_many: BTreeSet<_> = (0..16385).map(NodeId).collect();
                inputs.dirty = &too_many;
                assert!(RemovedSmallCandidates::discover(&inputs, 2)
                    .unwrap()
                    .entries
                    .is_empty());
            }
            workspace.commit().unwrap();
            assert_eq!(workspace.read(new_file, 0, changed.len()).unwrap(), changed);
            assert_eq!(workspace.live.nodes[&new_file].mode, 0o600);
            let mut retained = Vec::new();
            filesystem::stream(&CoreReader(&old), old_root, &path, &mut retained).unwrap();
            assert_eq!(retained, data);
            drop(old);
            drop(workspace);
            std::fs::remove_dir_all(root).unwrap();
        }
        let mut discovery = RemovedSmallDiscovery {
            candidates: Default::default(),
            directories: Vec::new(),
            name_bytes: 0,
            entries: 4096,
            calls: 8191,
        };
        assert!(discovery.charge(0, 1));
        assert!(!discovery.charge(1, 0));
    }

    #[test]
    fn detached_changed_inputs_preserve_cut_without_a_live_workspace() {
        let (root, mut workspace) = empty_workspace("detached-changes");
        let file = workspace.create_file(ROOT, b"file", 0o640).unwrap().node;
        workspace.write(file, 0, b"before").unwrap();
        let untouched = workspace
            .create_file(ROOT, b"untouched", 0o600)
            .unwrap()
            .node;
        workspace.write(untouched, 0, b"unchanged").unwrap();
        workspace.commit().unwrap();
        // Directory deltas need their referenced identities; unrelated cached nodes
        // are not construction inputs.
        workspace.link(file, ROOT, b"alias").unwrap();
        workspace.write(file, 0, b"at cut").unwrap();
        let expected = workspace
            .build_frontier_candidate(CandidatePurpose::Preview)
            .unwrap()
            .built
            .root_id;
        let dirty = workspace.live.dirty.clone();
        let mut selected = dirty.clone();
        for node in &dirty {
            if let Data::Directory(directory) = &workspace.live.nodes[node].data {
                selected.extend(directory.changes.values().flatten());
            }
        }
        let nodes = selected
            .into_iter()
            .map(|id| (id, workspace.live.nodes[&id].clone()))
            .collect::<std::collections::HashMap<_, _>>();
        assert!(!nodes.contains_key(&untouched));
        let canonical = nodes
            .iter()
            .filter_map(|(&id, node)| node.canonical.map(|inode| (inode, id)))
            .collect();
        let generation = workspace.live.mutation_generation;
        let base_root = workspace.base_root;
        let base_inodes = workspace.base_inodes;
        let policy = workspace.live.policy;
        let reader = workspace.reader.clone();
        let store = workspace.store.clone();
        let workspace_id = workspace.workspace_id;
        let spool = workspace.spool.clone();
        // Keep the old ranges in the detached records through a newer write/unlink.
        workspace.write(file, 0, b"later!").unwrap();
        workspace.unlink(ROOT, b"alias", false).unwrap();
        let frozen = CandidateInputs {
            scope: workspace.inode_scope,
            serials: workspace.inode_serials.clone(),
            live: layerfs_workspace_core::FrozenWorkspaceChanges {
                nodes: &nodes,
                dirty: &dirty,
                canonical_nodes: &canonical,
                base_root,
                mutation_generation: generation,
                policy,
            },
            store: &store,
            workspace_id,
            reader,
            base_inodes,
            spool: &spool,
        };
        let candidate = frozen.build(CandidatePurpose::Preview, 1, None).unwrap();
        assert_eq!(candidate.built.root_id, expected);
        assert_eq!(candidate.checkpoint.generation, generation);
        assert_ne!(workspace.live.mutation_generation, generation);
        assert_eq!(workspace.read(file, 0, 6).unwrap(), b"later!");
        assert!(workspace.lookup(ROOT, b"alias").is_err());
        drop(candidate);
        drop(frozen);
        drop(nodes);
        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parallel_predecessor_plan_keeps_correspondence_and_native_prefixes() {
        let (root, mut workspace) = empty_workspace("predecessor-worker-budget");
        let mut files = Vec::new();
        for index in 0..2 {
            let mut seed = 91_u64 + index;
            let bytes = (0..layerfs_content::file::content::SMALL_LIMIT)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    seed as u8
                })
                .collect::<Vec<_>>();
            let name = format!("file-{index}");
            let node = workspace
                .create_file(ROOT, name.as_bytes(), 0o640)
                .unwrap()
                .node;
            workspace.write(node, 0, &bytes).unwrap();
            files.push((name, node, bytes));
        }
        workspace.invalidate_capture();
        workspace.commit().unwrap();
        let base_root = workspace.base_root;
        let old = workspace.reader.clone();
        let original_bytes = files
            .iter()
            .map(|(_, _, bytes)| bytes.clone())
            .collect::<Vec<_>>();
        for (_, node, bytes) in &mut files {
            bytes[17] ^= 1;
            // Stay at the chunked-file threshold: this test qualifies native
            // predecessor cursors, not the separate SmallContent encoder.
            workspace.write(*node, 0, bytes).unwrap();
        }
        workspace.invalidate_capture();
        let serial = workspace
            .build_frontier_candidate_with_workers(CandidatePurpose::Preview, 1)
            .unwrap();
        let expected = serial.built.root_id;
        drop(serial);
        let before = workspace.store.physical_storage_receipt();
        let candidate = workspace
            .build_frontier_candidate_with_workers(CandidatePurpose::Commit, 4)
            .unwrap();
        assert_eq!(candidate.built.root_id, expected);
        let branch = workspace
            .store
            .branch(workspace.branch_id)
            .unwrap()
            .unwrap();
        let outcome = workspace
            .store
            .commit_workspace_candidate(
                workspace.workspace_id,
                &branch,
                base_root,
                workspace.expected_base,
                candidate.built,
                candidate.admission.unwrap(),
            )
            .unwrap();
        assert!(matches!(outcome, CommitOutcome::Committed { root_id, .. } if root_id == expected));
        let physical = workspace.store.physical_storage_receipt().since(before);
        assert!(
            physical.diag_cursor_grants > 0,
            "predecessor metadata must fit the selected producer budget"
        );
        assert_eq!(physical.diag_cursor_memory_limit, 0);
        assert!(
            physical.native_admitted_prefix_count > 0,
            "similar replacement chunks must retain native PREFIX selection"
        );
        let reader = workspace.store.snapshot_reader(expected);
        for ((name, _, bytes), retained) in files.iter().zip(&original_bytes) {
            let path = CanonicalPath::new(name).unwrap();
            let mut actual = Vec::new();
            filesystem::stream(&CoreReader(&reader), expected, &path, &mut actual).unwrap();
            assert_eq!(&actual, bytes);
            actual.clear();
            filesystem::stream(&CoreReader(&old), base_root, &path, &mut actual).unwrap();
            assert_eq!(&actual, retained);
        }
        drop(reader);
        drop(old);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn producer_workers_preserve_roots_alias_capture_and_empty_inputs() {
        let (root, mut workspace) = empty_workspace("producer-workers");
        workspace.mkdir(ROOT, b"directory", 0o700).unwrap();
        let empty = workspace
            .build_frontier_candidate_with_workers(CandidatePurpose::Preview, 4)
            .unwrap();
        assert_eq!(empty.built.counters.cdc_bytes_scanned, 0);
        drop(empty);
        let mut files = Vec::new();
        for index in 0..4 {
            let data = vec![index as u8 + 1; 100_000 + index * 123];
            let node = workspace
                .create_file(ROOT, format!("file-{index}").as_bytes(), 0o640)
                .unwrap()
                .node;
            workspace.write(node, 0, &data).unwrap();
            files.push((node, data));
        }
        workspace.link(files[0].0, ROOT, b"alias").unwrap();
        let arm_capture = |workspace: &mut Workspace| {
            let mut objects = ObjectBuffer::empty().unwrap();
            let (root, counters) = rope::build(&mut objects, files[0].1.as_slice()).unwrap();
            workspace.capture =
                crate::capture::CaptureState::Ready(Box::new(crate::capture::CapturedFile {
                    node: files[0].0,
                    len: files[0].1.len() as u64,
                    root: root.into(),
                    counters,
                    objects: objects.into_resumable().unwrap(),
                }));
        };
        let before = workspace.store.store_counts().unwrap();
        arm_capture(&mut workspace);
        let serial = workspace
            .build_frontier_candidate_with_workers(CandidatePurpose::Preview, 1)
            .unwrap();
        let expected = serial.built.root_id;
        let expected_ids = serial
            .built
            .objects
            .ids_in_order(usize::MAX)
            .unwrap()
            .unwrap()
            .into_iter()
            .collect::<BTreeSet<_>>();
        // The injected capture explicitly uses the legacy rope builder. The
        // remaining files are SmallContent and must not claim CDC work.
        assert!(files
            .iter()
            .all(|(_, bytes)| bytes.len() < layerfs_content::file::content::SMALL_LIMIT));
        let scanned = files[0].1.len() as u64;
        let input_bytes = files
            .iter()
            .map(|(_, bytes)| bytes.len() as u64)
            .sum::<u64>();
        assert_eq!(serial.built.counters.cdc_bytes_scanned, scanned);
        drop(serial);
        // Corrupt only the captured inode's backing to prove that its completed
        // output is reused, rather than silently rebuilding it for either alias.
        let Data::File(FileData::Edited { pieces, .. }) = &workspace.live.nodes[&files[0].0].data
        else {
            unreachable!()
        };
        let captured_slice = pieces.compact_spool().unwrap();
        let captured_segment = captured_slice.segment.clone();
        let captured_offset = captured_slice.offset;
        crate::file_io::spool_segment(&captured_segment)
            .unwrap()
            .file
            .write_all_at(&vec![0; files[0].1.len()], captured_offset)
            .unwrap();
        arm_capture(&mut workspace);
        let parallel = workspace
            .build_frontier_candidate_with_workers(CandidatePurpose::Preview, 4)
            .unwrap();
        assert_eq!(parallel.built.root_id, expected);
        assert_eq!(parallel.built.counters.cdc_bytes_scanned, scanned);
        assert_eq!(
            parallel
                .built
                .objects
                .ids_in_order(usize::MAX)
                .unwrap()
                .unwrap()
                .into_iter()
                .collect::<BTreeSet<_>>(),
            expected_ids
        );
        assert_eq!(workspace.store.store_counts().unwrap(), before);
        assert!(workspace.take_capture().is_none());
        drop(parallel);
        crate::file_io::spool_segment(&captured_segment)
            .unwrap()
            .file
            .write_all_at(&files[0].1, captured_offset)
            .unwrap();
        drop(captured_segment);
        arm_capture(&mut workspace);
        let admitted = workspace
            .build_frontier_candidate_with_workers(CandidatePurpose::Commit, 4)
            .unwrap();
        assert_eq!(admitted.built.root_id, expected);
        assert_eq!(admitted.built.counters.cdc_bytes_scanned, scanned);
        assert!(admitted.admission.is_some());
        drop(admitted);
        // The discarded candidate consumed the injected legacy capture. Compare
        // publication under the same capture policy, not a fresh SmallContent rebuild.
        arm_capture(&mut workspace);
        workspace.commit().unwrap();
        assert_eq!(workspace.base_root, expected);
        let old = workspace.reader.clone();
        workspace.write(files[1].0, 7, b"edit").unwrap();
        workspace.truncate(files[2].0, 150_000).unwrap();
        let serial = workspace
            .build_frontier_candidate_with_workers(CandidatePurpose::Preview, 1)
            .unwrap();
        let parallel = workspace
            .build_frontier_candidate_with_workers(CandidatePurpose::Preview, 4)
            .unwrap();
        assert_eq!(serial.built.root_id, parallel.built.root_id);
        assert_eq!(
            serial.built.counters.cdc_bytes_scanned,
            parallel.built.counters.cdc_bytes_scanned
        );
        assert!(parallel.built.counters.cdc_bytes_scanned < input_bytes);
        assert!(layerfs_layerstack_store::ObjectSource::read_object(&old, expected).is_ok());
        drop(serial);
        drop(parallel);
        drop(old);
        workspace.live.policy.max_final_delta_memory_bytes = 1024;
        let result = workspace.build_frontier_candidate_with_workers(CandidatePurpose::Preview, 4);
        // Existing tiny-budget fallback is allowed to reject, never to exceed its policy.
        if let Err(error) = result {
            assert!(error.to_string().contains("limit"));
        }
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_result_journals_restore_reverse_completion_and_reject_missing_slots() {
        let (root, mut workspace) = empty_workspace("reverse-results");
        for index in 0..4 {
            let node = workspace
                .create_file(ROOT, format!("file-{index}").as_bytes(), 0o600)
                .unwrap()
                .node;
            workspace.write(node, 0, &[index as u8; 17]).unwrap();
        }
        let inputs = StableFileInputs {
            nodes: &workspace.live.nodes,
            dirty: &workspace.live.dirty,
            reader: workspace.reader.clone(),
            base_inodes: workspace.base_inodes,
            base_root: workspace.base_root,
            correspondence_reserved: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            generation: workspace.live.mutation_generation,
            spool: &workspace.spool,
            io_bytes: 1024,
            planning_bytes: workspace.live.policy.max_final_delta_memory_bytes,
            captured: std::sync::Mutex::new(None),
        };
        let plan = inputs.prepare().unwrap();
        assert_eq!(plan.count, 4);
        let tasks = plan
            .tasks(256)
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        let mut objects = ObjectBuffer::bounded_output(None).unwrap();
        let workers = objects
            .construct_files(
                1,
                1,
                std::iter::once(()),
                |_| {
                    (0..4)
                        .map(|index| inputs.worker(index, 4))
                        .collect::<Result<Vec<_>>>()
                },
                |workers, _, (), writer| {
                    for ordinal in (0..4).rev() {
                        inputs.produce_file(
                            &mut workers[ordinal],
                            &plan.file,
                            ordinal,
                            tasks[ordinal],
                            writer,
                        )?;
                    }
                    Ok(())
                },
                |workers| {
                    workers
                        .into_iter()
                        .map(|worker| worker.finish())
                        .collect::<Result<Vec<_>>>()
                },
            )
            .unwrap()
            .pop()
            .unwrap();
        let mut output = FileResults::new(plan, workers, 256).unwrap();
        for node in &tasks {
            output.next(*node, inputs.generation, 17).unwrap();
        }
        output.finish_read().unwrap();
        let missing = inputs.prepare().unwrap();
        assert!(FileResults::new(missing, Vec::new(), 256).is_err());
        drop(output);
        drop(inputs);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preview_stays_private_and_streamed_commit_has_the_same_root() {
        let (root, mut workspace) = empty_workspace("selected-output");
        for name in [b"first".as_slice(), b"second"] {
            let node = workspace.create_file(ROOT, name, 0o640).unwrap().node;
            workspace.write(node, 0, b"same selected payload").unwrap();
        }
        let counts = workspace.store.store_counts().unwrap();
        let preview = workspace
            .build_candidate(CandidatePurpose::Preview)
            .unwrap();
        assert_eq!(workspace.store.store_counts().unwrap(), counts);
        assert!(preview.admission.is_none());
        let expected = preview.built.root_id;
        drop(preview);
        workspace.commit().unwrap();
        assert_eq!(workspace.base_root, expected);
        assert_eq!(
            workspace.store.store_counts().unwrap().commits,
            counts.commits + 1
        );
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn new_complete_file_streams_without_private_spill_and_preserves_history() {
        let (root, mut workspace) = empty_workspace("complete-file-stream");
        let mut seed = 91_u64;
        let data = (0..2 * 1024 * 1024)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed as u8
            })
            .collect::<Vec<_>>();
        let file = workspace.create_file(ROOT, b"payload", 0o640).unwrap().node;
        workspace.write(file, 0, &data).unwrap();
        workspace.invalidate_capture();
        workspace.create_file(ROOT, b"empty", 0o600).unwrap();
        let control = FrozenFile::from_node(&workspace.reader, &workspace.live.nodes[&file])
            .unwrap()
            .build(
                None,
                None,
                std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
                None,
                1,
            )
            .unwrap();
        let expected_file = control.root_id;
        assert!(control.counters.spill_count > 0);
        drop(control);
        let counts = workspace.store.store_counts().unwrap();
        let preview = workspace
            .build_candidate(CandidatePurpose::Preview)
            .unwrap();
        assert_eq!(workspace.store.store_counts().unwrap(), counts);
        let expected_root = preview.built.root_id;
        drop(preview);
        let candidate = workspace.build_candidate(CandidatePurpose::Commit).unwrap();
        assert_eq!(candidate.built.root_id, expected_root);
        assert_eq!(
            candidate.built.counters.cdc_bytes_scanned,
            data.len() as u64
        );
        assert_eq!(candidate.built.counters.spill_count, 0);
        assert!(candidate.built.counters.first_store_write_bytes < data.len() as u64);
        drop(candidate);
        workspace.commit().unwrap();
        assert_eq!(workspace.base_root, expected_root);
        let old = workspace.reader.clone();
        let path = CanonicalPath::new("payload").unwrap();
        let record = filesystem::resolve(
            &CoreReader(&old),
            expected_root,
            &path,
            &mut LogicalCounters::default(),
        )
        .unwrap()
        .record;
        assert_eq!(record.content_root, expected_file);
        workspace.write(file, 19, b"changed").unwrap();
        workspace.commit().unwrap();
        let mut retained = Vec::new();
        filesystem::stream(&CoreReader(&old), expected_root, &path, &mut retained).unwrap();
        assert_eq!(retained, data);
        assert_eq!(workspace.read(file, 19, 7).unwrap(), b"changed");
        let empty = workspace.lookup(ROOT, b"empty").unwrap().node;
        assert!(workspace.read(empty, 0, 1).unwrap().is_empty());
        drop(old);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn paged_release_keeps_prior_moves_and_repeated_unseen_aliases() {
        let (root, mut workspace) = empty_workspace("paged-release");
        let directory = workspace.mkdir(ROOT, b"remove", 0o700).unwrap().node;
        for index in 0..80 {
            let name = format!("f{index:03}");
            let node = workspace
                .create_file(directory, name.as_bytes(), 0o600)
                .unwrap()
                .node;
            workspace.write(node, 0, name.as_bytes()).unwrap();
            if index == 0 {
                workspace.link(node, ROOT, b"outside").unwrap();
                workspace.link(node, ROOT, b"hidden").unwrap();
                workspace.link(node, directory, b"another").unwrap();
            }
        }
        workspace.commit().unwrap();
        let branch = workspace.branch_id;
        let store = workspace.store.clone();
        drop(workspace);
        let mut workspace = Workspace::open_with_policy(
            store,
            branch,
            root.join("again"),
            crate::ResourcePolicy {
                max_final_delta_memory_bytes: 16 * 1024,
                ..crate::ResourcePolicy::default()
            },
        )
        .unwrap();
        let directory = workspace.lookup(ROOT, b"remove").unwrap().node;
        workspace
            .rename(directory, b"f001", ROOT, b"moved", false)
            .unwrap();
        for index in 0..80 {
            if index != 1 {
                workspace
                    .unlink(directory, format!("f{index:03}").as_bytes(), false)
                    .unwrap();
            }
        }
        workspace.unlink(directory, b"another", false).unwrap();
        workspace.unlink(ROOT, b"remove", true).unwrap();
        workspace.commit().unwrap();
        assert!(workspace.lookup(ROOT, b"remove").is_err());
        let outside = workspace.lookup(ROOT, b"outside").unwrap();
        assert_eq!(outside.links, 2);
        assert_eq!(
            workspace.lookup(ROOT, b"hidden").unwrap().node,
            outside.node
        );
        let moved = workspace.lookup(ROOT, b"moved").unwrap();
        assert_eq!(moved.links, 1);
        assert_eq!(workspace.read(moved.node, 0, 4).unwrap(), b"f001");
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reference_journal_rewind_keeps_prior_edges_and_latest_alias_counts() {
        let (root, mut workspace) = empty_workspace("reference-facts");
        let mut nodes = Vec::new();
        for name in [b"first", b"other", b"alias"] {
            let node = workspace.create_file(ROOT, name, 0o600).unwrap().node;
            workspace.write(node, 0, name).unwrap();
            nodes.push(node);
        }
        workspace.commit().unwrap();
        let ids = nodes
            .iter()
            .map(|node| workspace.live.nodes[node].canonical.unwrap())
            .collect::<Vec<_>>();
        let mut journal = ReferenceJournal::new(&workspace.spool, 128);
        journal.push(Some(ids[0]), None).unwrap();
        let before_trial = journal.count;
        journal.push(Some(ids[1]), None).unwrap();
        journal.rewind(before_trial).unwrap();
        journal.push(None, Some(ids[2])).unwrap();
        journal.push(None, Some(ids[2])).unwrap();
        journal.push(Some(ids[2]), None).unwrap();
        let objects = ObjectBuffer::new(&workspace.reader).unwrap();
        let mut inodes = FrontierInodes::new(workspace.base_root, 1, 0, &workspace.spool);
        inodes
            .apply_references(
                &objects,
                &CoreReader(&workspace.reader),
                journal.finish().unwrap(),
                8 * 1024 * 1024,
                128,
            )
            .unwrap();
        assert!(inodes.record(&objects, ids[0]).is_err());
        assert_eq!(
            inodes.record(&objects, ids[1]).unwrap().namespace_ref_count,
            1
        );
        assert_eq!(
            inodes.record(&objects, ids[2]).unwrap().namespace_ref_count,
            2
        );
        drop(objects);
        // A single-file deletion remains supported at the existing minimum budget.
        workspace.live.policy.max_final_delta_memory_bytes = 1024;
        workspace.unlink(ROOT, b"other", false).unwrap();
        workspace.commit().unwrap();
        assert!(workspace.lookup(ROOT, b"other").is_err());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reduced_budget_real_workspace_uses_one_tree_fallback_and_reopens_exactly() {
        let (root, mut workspace) = empty_workspace("tree-pressure");
        let mut nodes = Vec::new();
        for index in 0..16 {
            let node = workspace
                .create_file(ROOT, format!("f{index}").as_bytes(), 0o600)
                .unwrap()
                .node;
            workspace.write(node, 0, b"original").unwrap();
            nodes.push(node);
        }
        workspace.commit().unwrap();
        let original = workspace.base_root;
        workspace.live.policy.max_final_delta_memory_bytes = 1024;
        workspace.write(nodes[7], 0, b"modified").unwrap();
        workspace.commit().unwrap();
        let updated = workspace.base_root;
        let stats = workspace.frontier_stats();
        assert_eq!(
            (
                stats.tree_attempts,
                stats.tree_fallbacks,
                stats.fallback_inodes
            ),
            (1, 1, 1)
        );
        assert_ne!(original, updated);
        drop(workspace);
        let store = LayerStackStore::connect(root.join("store.sqlite")).unwrap();
        for (snapshot, changed) in [(original, false), (updated, true)] {
            let reader = store.snapshot_reader(snapshot);
            for index in 0..16 {
                let mut bytes = Vec::new();
                filesystem::stream(
                    &CoreReader(&reader),
                    snapshot,
                    &CanonicalPath::new(&format!("f{index}")).unwrap(),
                    &mut bytes,
                )
                .unwrap();
                assert_eq!(
                    bytes,
                    if changed && index == 7 {
                        b"modified"
                    } else {
                        b"original"
                    }
                );
            }
        }
        println!(
            "workspace-tree-fallback attempts={} fallbacks={} inodes={} history_files_checked=32",
            stats.tree_attempts, stats.tree_fallbacks, stats.fallback_inodes
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn final_records_match_in_memory_spill_and_small_budget_builders() {
        assert!(std::mem::size_of::<(InodeId, FrontierValue)>() + 64 <= 256);
        let (root, mut workspace) = empty_workspace("final-records");
        let mut nodes = Vec::new();
        for index in 0..12 {
            let node = workspace
                .create_file(ROOT, format!("f{index}").as_bytes(), 0o600)
                .unwrap()
                .node;
            workspace.write(node, 0, &[index]).unwrap();
            nodes.push(node);
        }
        workspace.commit().unwrap();
        let mut roots = Vec::new();
        for (capacity, scratch) in [(32, 16384), (2, 16384), (2, 0)] {
            let mut objects = ObjectBuffer::new(&workspace.reader).unwrap();
            let mut inodes =
                FrontierInodes::new(workspace.base_root, capacity, scratch, &workspace.spool);
            let first = inodes
                .record(&objects, workspace.live.nodes[&nodes[0]].canonical.unwrap())
                .unwrap();
            let first_inode = workspace.live.nodes[&nodes[0]].canonical.unwrap();
            inodes
                .set_checkpoint(first_inode, first, nodes[0], first.content_root)
                .unwrap();
            let discarded = workspace.live.nodes[nodes.last().unwrap()]
                .canonical
                .unwrap();
            inodes.set(discarded, Some(first)).unwrap();
            for node in &nodes {
                let inode = workspace.live.nodes[node].canonical.unwrap();
                let mut record = inodes.record(&objects, inode).unwrap();
                record.content_root = first.content_root;
                inodes.set(inode, Some(record)).unwrap();
            }
            inodes.set(discarded, None).unwrap();
            let mut checked = 0;
            inodes
                .finish(&mut objects, |_, inode, node, content, record| {
                    assert_eq!(
                        (inode, node, content),
                        (first_inode, nodes[0], first.content_root)
                    );
                    assert_eq!(record.content_root, content);
                    checked += 1;
                    Ok(())
                })
                .unwrap();
            assert_eq!(checked, 1);
            assert_eq!(inodes.spill.is_none(), capacity == 32);
            roots.push(inodes.root);
        }
        assert!(roots.windows(2).all(|pair| pair[0] == pair[1]));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn coalesced_spill_keeps_tombstones_and_reference_updates_across_merge_failure() {
        let (root, mut workspace) = empty_workspace("inode-merge");
        let file = workspace.create_file(ROOT, b"file", 0o600).unwrap().node;
        workspace.write(file, 0, b"data").unwrap();
        workspace.commit().unwrap();
        let objects = ObjectBuffer::new(&workspace.reader).unwrap();
        let inode = workspace.live.nodes[&file].canonical.unwrap();
        let mut inodes = FrontierInodes::new(workspace.base_root, 1, 0, &workspace.spool);
        let mut record = inodes.record(&objects, inode).unwrap();
        record.namespace_ref_count = 3;
        inodes.set(inode, Some(record)).unwrap();
        let extra = InodeId::allocate([9; 32], 1);
        inodes.set(extra, Some(record)).unwrap(); // spills the first key
        record.namespace_ref_count = 2;
        inodes.set(inode, Some(record)).unwrap(); // updates an existing spilled key
        inodes.set(extra, None).unwrap();
        INJECT_INODE_MERGE_FAILURE.with(|inject| inject.set(true));
        assert!(inodes.merge_pending().is_err());
        assert_eq!(
            inodes.record(&objects, inode).unwrap().namespace_ref_count,
            2
        );
        assert!(inodes.record(&objects, extra).is_err());
        inodes.merge_pending().unwrap();
        assert_eq!(inodes.run_record_count(), 2);
        assert_eq!(
            inodes.record(&objects, inode).unwrap().namespace_ref_count,
            2
        );
        assert!(inodes.record(&objects, extra).is_err());
        inodes.finalize().unwrap();
        let spill = inodes.spill.as_ref().unwrap();
        let first = FrontierInodes::read(spill, 0).unwrap();
        let second = FrontierInodes::read(spill, 1).unwrap();
        assert!(first.0 < second.0);
        drop(objects);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dirty_checkpoint_keeps_rejected_edit_accounting_and_unseen_pinned_alias() {
        for pinned in [false, true] {
            let (root, mut workspace) = empty_workspace("dirty-checkpoint");
            let a = workspace.create_file(ROOT, b"a", 0o600).unwrap().node;
            workspace.create_file(ROOT, b"b", 0o600).unwrap();
            workspace.write(a, 0, b"abc").unwrap();
            workspace.link(a, ROOT, b"hidden").unwrap();
            workspace.commit().unwrap();
            let branch = workspace.branch_id;
            let store = workspace.store.clone();
            drop(workspace);
            let mut workspace = Workspace::open(store, branch, root.join("again")).unwrap();
            let a = workspace.lookup(ROOT, b"a").unwrap().node;
            let b = workspace.lookup(ROOT, b"b").unwrap().node;
            workspace.live.policy.max_spool_bytes = 0;
            assert!(workspace.write(a, 0, b"x").is_err());
            workspace.chmod(b, 0o640).unwrap();
            workspace.commit().unwrap();
            workspace.live.policy.max_spool_bytes = 1024;
            workspace.write(a, 0, b"x").unwrap();
            if pinned {
                workspace.pin(a, false).unwrap();
            }
            workspace.unlink(ROOT, b"a", false).unwrap();
            workspace.commit().unwrap();
            let mut published = Vec::new();
            filesystem::stream(
                &CoreReader(&workspace.reader),
                workspace.base_root,
                &CanonicalPath::new("hidden").unwrap(),
                &mut published,
            )
            .unwrap();
            assert_eq!(published, b"xbc");
            assert_eq!(workspace.lookup(ROOT, b"hidden").unwrap().node, a);
            assert_eq!(workspace.read(a, 0, 3).unwrap(), b"xbc");
            if pinned {
                workspace.unpin(a).unwrap();
            }
            drop(workspace);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn checkpoint_checks_final_references_and_preserves_payload_limit() {
        let (root, mut workspace) = empty_workspace("checkpoint-checks");
        workspace.live.policy.max_spool_bytes = 3;
        let file = workspace.create_file(ROOT, b"file", 0o640).unwrap().node;
        workspace.write(file, 0, b"abc").unwrap();
        workspace.commit().unwrap();
        workspace.live.policy.max_spool_bytes = 0;
        workspace.set_mtime(file, 17, 3).unwrap();
        workspace.commit().unwrap();
        let mut journal = CheckpointJournal::new(&workspace.candidate_inputs()).unwrap();
        let attr = workspace.attr(file).unwrap();
        let inode = workspace.live.nodes[&file].canonical.unwrap();
        let objects = ObjectBuffer::new(&workspace.reader).unwrap();
        let mut record = FrontierInodes::new(workspace.base_root, 1, 0, &workspace.spool)
            .record(&objects, inode)
            .unwrap();
        journal
            .push(file, inode, record.content_root, attr, Some(attr.size))
            .unwrap();
        journal
            .validate(&objects, &PortableMetadataCache::default(), |_| Ok(record))
            .unwrap();
        record.namespace_ref_count += 1;
        assert!(matches!(
            journal.validate(&objects, &PortableMetadataCache::default(), |_| Ok(record)),
            Err(StorageError::Integrity("candidate checkpoint record"))
        ));
        drop(objects);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn structural_frontier_keeps_unrelated_namespace_out_of_budget() {
        let (root, empty) = empty_workspace("frontier");
        let store = empty.store.clone();
        drop(empty);
        let source = root.join("source");
        std::fs::create_dir_all(source.join("background")).unwrap();
        for index in 0..200 {
            std::fs::write(
                source.join(format!("background/file-{index:03}")),
                b"unchanged",
            )
            .unwrap();
        }
        for name in ["tree", "keep"] {
            std::fs::create_dir(source.join(name)).unwrap();
            std::fs::write(source.join(name).join("child"), name.as_bytes()).unwrap();
        }
        std::fs::hard_link(source.join("tree/child"), source.join("outside")).unwrap();
        std::fs::write(source.join("shared"), b"original alias").unwrap();
        std::fs::hard_link(source.join("shared"), source.join("hidden")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("fixture").unwrap(),
                LayerStackInitialization::Directory(source),
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("changed").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let mut workspace = Workspace::open_with_policy(
            store.clone(),
            branch,
            root.join("changed-spool"),
            crate::ResourcePolicy {
                max_final_delta_memory_bytes: 16 * 1024,
                ..crate::ResourcePolicy::default()
            },
        )
        .unwrap();
        // The former structural fallback cannot even hold this small fixture.
        assert!(matches!(
            workspace.base_manifest(),
            Err(StorageError::InvalidInput("workspace final-delta limit"))
        ));
        let before_root = workspace.base_root;
        let reader = store.snapshot_reader(before_root);
        let before = |path: &str| {
            filesystem::resolve(
                &CoreReader(&reader),
                before_root,
                &CanonicalPath::new(path).unwrap(),
                &mut LogicalCounters::default(),
            )
            .unwrap()
        };
        let background = before("background");
        let keep = before("keep");
        let old_shared = before("shared");
        let old_tree = before("tree");
        let old_child = before("tree/child");

        // Neither hidden alias is looked up: releasing only the known binding
        // must still retain its inode and content under the unseen name.
        workspace.unlink(ROOT, b"shared", false).unwrap();
        let replacement = workspace.create_file(ROOT, b"shared", 0o600).unwrap();
        workspace
            .write(replacement.node, 0, b"replacement")
            .unwrap();
        workspace
            .rename(ROOT, b"tree", ROOT, b"moved", false)
            .unwrap();
        let moved = workspace.lookup(ROOT, b"moved").unwrap();
        workspace.unlink(moved.node, b"child", false).unwrap();
        workspace.unlink(ROOT, b"moved", true).unwrap();
        workspace
            .rename(ROOT, b"keep", ROOT, b"kept", false)
            .unwrap();
        let added = workspace.create_file(ROOT, b"added", 0o640).unwrap();
        workspace.write(added.node, 0, b"linked payload").unwrap();
        workspace.link(added.node, ROOT, b"added-alias").unwrap();
        workspace.link(added.node, ROOT, b"second-alias").unwrap();
        workspace.unlink(ROOT, b"added", false).unwrap();
        workspace.mkdir(ROOT, b"new-directory", 0o750).unwrap();
        for index in 0..120 {
            workspace
                .create_file(ROOT, format!("new-{index:03}").as_bytes(), 0o600)
                .unwrap();
        }
        workspace.commit().unwrap();
        let final_root = workspace.base_root;
        let reader = store.snapshot_reader(final_root);
        let core = CoreReader(&reader);
        let resolve = |path: &str| {
            filesystem::resolve(
                &core,
                final_root,
                &CanonicalPath::new(path).unwrap(),
                &mut LogicalCounters::default(),
            )
            .unwrap()
        };
        assert_eq!(resolve("background").record, background.record);
        assert_eq!(resolve("kept").inode, keep.inode);
        assert_eq!(resolve("kept").record, keep.record);
        assert_eq!(resolve("hidden").inode, old_shared.inode);
        assert_eq!(resolve("hidden").record.namespace_ref_count, 1);
        assert_ne!(resolve("shared").inode, old_shared.inode);
        assert_eq!(resolve("outside").inode, old_child.inode);
        assert_eq!(resolve("outside").record.namespace_ref_count, 1);
        assert_eq!(resolve("added-alias").record.namespace_ref_count, 2);
        assert_eq!(resolve("added-alias").inode, resolve("second-alias").inode);
        assert_eq!(resolve("new-directory").record.namespace_ref_count, 1);
        for (path, expected) in [
            ("hidden", b"original alias".as_slice()),
            ("shared", b"replacement"),
            ("outside", b"tree"),
            ("kept/child", b"keep"),
            ("added-alias", b"linked payload"),
        ] {
            let mut bytes = Vec::new();
            filesystem::stream(
                &core,
                final_root,
                &CanonicalPath::new(path).unwrap(),
                &mut bytes,
            )
            .unwrap();
            assert_eq!(bytes, expected, "{path}");
        }
        for index in 0..120 {
            assert_eq!(
                resolve(&format!("new-{index:03}"))
                    .record
                    .namespace_ref_count,
                1
            );
        }
        let namespace = filesystem::namespace(&core, final_root).unwrap();
        let table = layerfs_content::tree::inode::InodeTableRoot(namespace.inode_table_root);
        assert!(layerfs_content::tree::inode::inode_record_lookup(
            &core,
            table,
            old_tree.inode,
            &mut InodeTableCounters::default()
        )
        .unwrap()
        .is_none());
        let mut entry_count = 0;
        layerfs_content::tree::inode::visit_inode_records(
            &core,
            table,
            &mut InodeTableCounters::default(),
            |_, _| {
                entry_count += 1;
                Ok(())
            },
        )
        .unwrap();
        // root + background directory/files + kept directory/file + outside +
        // hidden + replacement + aliased new file + new directory + 120 files.
        assert_eq!(entry_count, 329);
        // A valid long path still must fit the transient planner allocation,
        // together with its pending inode batch, under a custom small policy.
        workspace.live.policy.max_final_delta_memory_bytes = 4096;
        let name = "x".repeat(250);
        let mut parent = ROOT;
        for _ in 0..4 {
            parent = workspace
                .mkdir(parent, name.as_bytes(), 0o700)
                .unwrap()
                .node;
        }
        let path = workspace.live.nodes[&parent].paths.first().unwrap();
        assert!(CanonicalPath::new(path).is_ok());
        assert!(matches!(
            workspace.candidate_inputs().frontier_inode(parent),
            Err(StorageError::InvalidInput("workspace final-delta limit"))
        ));
        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dense_existing_file_delta_uses_bounded_frontier_and_preserves_aliases() {
        let (root, mut workspace) = empty_workspace("dense-frontier");
        let mut files = Vec::new();
        for index in 0..16 {
            let name = format!("file-{index:02}");
            let node = workspace
                .create_file(ROOT, name.as_bytes(), 0o640)
                .unwrap()
                .node;
            workspace.write(node, 0, b"initial").unwrap();
            files.push((name, node));
        }
        workspace.link(files[0].1, ROOT, b"alias").unwrap();
        let sentinel = workspace
            .create_file(ROOT, b"sentinel", 0o600)
            .unwrap()
            .node;
        workspace.write(sentinel, 0, b"unchanged").unwrap();
        workspace.commit().unwrap();
        let original_inodes = files
            .iter()
            .map(|(_, node)| workspace.live.nodes[node].canonical.unwrap())
            .collect::<Vec<_>>();
        workspace.live.policy.max_final_delta_memory_bytes = 4096;
        for (index, (_, node)) in files.iter().enumerate() {
            workspace
                .write(*node, 0, format!("changed-{index:02}").as_bytes())
                .unwrap();
            workspace.set_mtime(*node, 1700000001, 123).unwrap();
        }
        assert!(workspace
            .live
            .nodes
            .values()
            .all(|node| !matches!(&node.data,
            Data::Directory(directory) if !directory.changes.is_empty())));
        assert!(
            workspace
                .live
                .mutation_paths
                .keys()
                .map(|path| path_charge(path))
                .sum::<u64>()
                > workspace.live.policy.max_final_delta_memory_bytes
        );
        workspace.commit().unwrap();
        let reader = workspace.store.snapshot_reader(workspace.base_root);
        let core = CoreReader(&reader);
        for (index, (name, _)) in files.iter().enumerate() {
            let path = CanonicalPath::new(name).unwrap();
            let resolved = filesystem::resolve(
                &core,
                workspace.base_root,
                &path,
                &mut LogicalCounters::default(),
            )
            .unwrap();
            assert_eq!(resolved.inode, original_inodes[index]);
            let metadata =
                portable_metadata(&core, resolved.record.metadata_root, resolved.record.kind)
                    .unwrap();
            assert_eq!(
                (
                    metadata.permission_mode,
                    metadata.mtime_seconds,
                    metadata.mtime_nanoseconds
                ),
                (0o640, 1700000001, 123)
            );
            let mut bytes = Vec::new();
            filesystem::stream(&core, workspace.base_root, &path, &mut bytes).unwrap();
            assert_eq!(bytes, format!("changed-{index:02}").as_bytes());
        }
        for (path, expected) in [
            ("alias", b"changed-00".as_slice()),
            ("sentinel", b"unchanged"),
        ] {
            let mut bytes = Vec::new();
            filesystem::stream(
                &core,
                workspace.base_root,
                &CanonicalPath::new(path).unwrap(),
                &mut bytes,
            )
            .unwrap();
            assert_eq!(bytes, expected);
        }
        let alias = filesystem::resolve(
            &core,
            workspace.base_root,
            &CanonicalPath::new("alias").unwrap(),
            &mut LogicalCounters::default(),
        )
        .unwrap();
        assert_eq!(alias.inode, original_inodes[0]);
        assert_eq!(alias.record.namespace_ref_count, 2);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clean_workspace_commit_is_immediately_up_to_date() {
        let (root, mut workspace) = empty_workspace("clean-commit");
        let base_root = workspace.base_root;
        let (outcome, transition) = workspace.commit().unwrap();
        assert!(matches!(
            outcome,
            CommitOutcome::UpToDate { root_id } if root_id == base_root
        ));
        assert_eq!(transition, crate::lifecycle::CommitTransition::Checkpointed);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fully_charged_new_file_reads_directly_from_retained_spool() {
        let (root, mut workspace) = empty_workspace("direct-reader");
        let data = (0..128 * 1024)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let file = workspace.create_file(ROOT, b"full", 0o600).unwrap();
        workspace.write(file.node, 0, &data).unwrap();
        let mut reader = WorkspaceFileReader::new(&workspace, file.node).unwrap();
        assert!(matches!(reader.source, WorkspaceFileSource::Direct(..)));
        let mut actual = Vec::new();
        reader.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, data);

        let sparse = workspace.create_file(ROOT, b"sparse", 0o600).unwrap();
        workspace.write(sparse.node, 4096, b"x").unwrap();
        assert!(matches!(
            WorkspaceFileReader::new(&workspace, sparse.node)
                .unwrap()
                .source,
            WorkspaceFileSource::Mixed(..)
        ));
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sequential_new_file_capture_is_canonical_and_commit_ready() {
        let (root, mut workspace) = empty_workspace("streaming-capture");
        let data = (0..2 * 1024 * 1024)
            .map(|index| ((index * 29 + index / 11) % 251) as u8)
            .collect::<Vec<_>>();
        let file = workspace.create_file(ROOT, b"payload", 0o600).unwrap();
        for (index, chunk) in data.chunks(64 * 1024).enumerate() {
            workspace
                .write(file.node, (index * 64 * 1024) as u64, chunk)
                .unwrap();
        }
        workspace.fsync(Some(file.node)).unwrap();
        if let Data::File(FileData::Edited { pieces, .. }) =
            &mut workspace.live.nodes.get_mut(&file.node).unwrap().data
        {
            let mut original = pieces.pieces().into_iter();
            let crate::file_edit::Piece::Spool {
                segment,
                offset,
                len,
            } = original.next().unwrap()
            else {
                panic!("spool piece")
            };
            let count = pieces.count();
            *pieces = crate::file_edit::PieceTree::empty()
                .replace(
                    0,
                    0,
                    [
                        crate::file_edit::Piece::Spool {
                            segment: segment.clone(),
                            offset,
                            len: 1,
                        },
                        crate::file_edit::Piece::Spool {
                            segment,
                            offset: offset + 1,
                            len: len - 1,
                        },
                    ]
                    .into_iter()
                    .chain(original),
                )
                .unwrap();
            assert_eq!(pieces.count(), count + 1);
        }
        let captured = workspace
            .take_capture()
            .expect("sequential capture is reusable");
        let captured_root = captured.root;
        workspace.capture = crate::capture::CaptureState::Ready(Box::new(captured));

        let store = workspace.store.clone();
        let branch = store.branch(workspace.branch_id).unwrap().unwrap();
        let built = workspace
            .build_candidate(CandidatePurpose::Preview)
            .unwrap();
        let outcome = store
            .commit_candidate(
                &branch,
                workspace.base_root,
                workspace.expected_base,
                built.built,
            )
            .unwrap();
        let CommitOutcome::Committed { root_id, .. } = outcome else {
            panic!("capture Commit was not created")
        };
        let reader = store.snapshot_reader(root_id);
        let resolved = filesystem::resolve(
            &CoreReader(&reader),
            root_id,
            &CanonicalPath::new("payload").unwrap(),
            &mut LogicalCounters::default(),
        )
        .unwrap();
        assert_eq!(resolved.record.content_root, captured_root.0);
        let mut actual = Vec::new();
        filesystem::stream(
            &CoreReader(&reader),
            root_id,
            &CanonicalPath::new("payload").unwrap(),
            &mut actual,
        )
        .unwrap();
        assert_eq!(actual, data);

        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn backward_write_invalidates_capture_and_uses_exact_fallback() {
        let (root, mut workspace) = empty_workspace("capture-fallback");
        let mut expected = vec![17; 128 * 1024];
        let file = workspace.create_file(ROOT, b"payload", 0o600).unwrap();
        workspace.write(file.node, 0, &expected).unwrap();
        workspace.write(file.node, 4096, b"backward10").unwrap();
        expected[4096..4106].copy_from_slice(b"backward10");
        assert!(matches!(
            workspace.capture,
            crate::capture::CaptureState::Invalid
        ));

        let store = workspace.store.clone();
        let branch = store.branch(workspace.branch_id).unwrap().unwrap();
        let built = workspace
            .build_candidate(CandidatePurpose::Preview)
            .unwrap();
        let outcome = store
            .commit_candidate(
                &branch,
                workspace.base_root,
                workspace.expected_base,
                built.built,
            )
            .unwrap();
        let CommitOutcome::Committed { root_id, .. } = outcome else {
            panic!("fallback Commit was not created")
        };
        let reader = store.snapshot_reader(root_id);
        let mut actual = Vec::new();
        filesystem::stream(
            &CoreReader(&reader),
            root_id,
            &CanonicalPath::new("payload").unwrap(),
            &mut actual,
        )
        .unwrap();
        assert_eq!(actual, expected);

        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_file_mutations_are_exact_and_bounded() {
        let base = (0..1024 * 1024)
            .map(|index| ((index * 31 + index / 7) % 251) as u8)
            .collect::<Vec<_>>();
        for case in ["overwrite", "noop", "append", "shrink", "grow", "rename"] {
            let root = std::env::temp_dir().join(format!(
                "layerfs-incremental-{case}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let source = root.join("source");
            std::fs::create_dir_all(&source).unwrap();
            std::fs::write(source.join("payload"), &base).unwrap();
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
                    EntityName::new(case).unwrap(),
                    LocalForkSource::Layer { layer_id: layer },
                )
                .unwrap();
            let policy = if case == "overwrite" {
                crate::ResourcePolicy {
                    max_final_delta_memory_bytes: 1024,
                    ..crate::ResourcePolicy::default()
                }
            } else {
                crate::ResourcePolicy::default()
            };
            let mut workspace =
                Workspace::open_with_policy(store.clone(), branch, root.join("spool"), policy)
                    .unwrap();
            let file = workspace.lookup(ROOT, b"payload").unwrap().node;
            let original_inode = workspace.live.nodes[&file].canonical.unwrap();
            let mut expected = base.clone();
            let (expected_cdc, expected_path) = match case {
                "overwrite" => {
                    let offset = 543_219;
                    workspace.write(file, offset, b"changed-10").unwrap();
                    expected[offset as usize..offset as usize + 10].copy_from_slice(b"changed-10");
                    (10, "payload")
                }
                "noop" => {
                    let offset = 123_457;
                    workspace
                        .write(file, offset, &base[offset as usize..offset as usize + 10])
                        .unwrap();
                    (0, "payload")
                }
                "append" => {
                    workspace
                        .write(file, base.len() as u64, b"append-010")
                        .unwrap();
                    expected.extend_from_slice(b"append-010");
                    (10, "payload")
                }
                "shrink" => {
                    workspace.truncate(file, base.len() as u64 - 4096).unwrap();
                    expected.truncate(base.len() - 4096);
                    (0, "payload")
                }
                "grow" => {
                    workspace.truncate(file, base.len() as u64 + 4096).unwrap();
                    expected.resize(base.len() + 4096, 0);
                    (4096, "payload")
                }
                "rename" => {
                    workspace
                        .rename(ROOT, b"payload", ROOT, b"renamed", false)
                        .unwrap();
                    (0, "renamed")
                }
                _ => unreachable!(),
            };
            let built = workspace
                .build_candidate(CandidatePurpose::Preview)
                .unwrap();
            assert_eq!(
                built.built.counters.cdc_bytes_scanned, expected_cdc,
                "{case}"
            );
            assert!(built.built.objects.encoded_bytes() < 256 * 1024, "{case}");
            if case == "noop" {
                assert_eq!(built.built.root_id, workspace.base_root);
                assert!(built.built.objects.is_empty());
            }
            let candidate_root = built.built.root_id;
            let record = store.branch(branch).unwrap().unwrap();
            let outcome = store
                .commit_candidate(
                    &record,
                    workspace.base_root,
                    workspace.expected_base,
                    built.built,
                )
                .unwrap();
            assert_eq!(
                matches!(outcome, CommitOutcome::UpToDate { .. }),
                case == "noop"
            );
            let reader = store.snapshot_reader(candidate_root);
            let mut actual = Vec::new();
            let resolved = filesystem::resolve(
                &CoreReader(&reader),
                candidate_root,
                &CanonicalPath::new(expected_path).unwrap(),
                &mut LogicalCounters::default(),
            )
            .unwrap();
            if case == "rename" {
                assert_eq!(resolved.inode, original_inode);
                assert!(filesystem::resolve(
                    &CoreReader(&reader),
                    candidate_root,
                    &CanonicalPath::new("payload").unwrap(),
                    &mut LogicalCounters::default(),
                )
                .is_err());
            }
            filesystem::stream(
                &CoreReader(&reader),
                candidate_root,
                &CanonicalPath::new(expected_path).unwrap(),
                &mut actual,
            )
            .unwrap();
            assert_eq!(actual, expected, "{case}");

            drop(workspace);
            drop(store);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn owner_prepend_scans_only_replacement_and_retains_every_base_payload() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-owner-prepend-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("payload"), vec![0x5a; 1024 * 1024]).unwrap();
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
        let mut workspace = Workspace::open(store.clone(), branch, root.join("spool")).unwrap();
        let file = workspace.lookup(ROOT, b"payload").unwrap().node;
        let base_root = match workspace.live.nodes[&file].data {
            Data::File(FileData::Base { root, .. }) => root,
            _ => panic!("base file"),
        };
        let mut base_payloads = BTreeSet::new();
        rope::visit_extents(
            &CoreReader(&workspace.reader),
            rope::FileStateRoot(base_root.0),
            |extents| {
                base_payloads.extend(extents.iter().map(|extent| extent.payload_object_id));
                Ok(())
            },
        )
        .unwrap();
        workspace
            .edit_many(
                file,
                vec![(
                    0,
                    0,
                    crate::WorkspaceFileReplacement::Inline(b"PREPEND010".to_vec()),
                )],
            )
            .unwrap();
        let reads_before = workspace.reader.read_metrics_snapshot().unwrap();
        let built = workspace
            .build_candidate(CandidatePurpose::Preview)
            .unwrap();
        let reads_after = workspace.reader.read_metrics_snapshot().unwrap();
        assert_eq!(built.built.counters.cdc_bytes_scanned, 10);
        assert_eq!(
            reads_after.payload_bytes_read - reads_before.payload_bytes_read,
            0
        );
        let record = store.branch(branch).unwrap().unwrap();
        let outcome = store
            .commit_candidate(
                &record,
                workspace.base_root,
                workspace.expected_base,
                built.built,
            )
            .unwrap();
        let CommitOutcome::Committed { root_id, .. } = outcome else {
            panic!("prepend commit")
        };
        let reader = store.snapshot_reader(root_id);
        let resolved = filesystem::resolve(
            &CoreReader(&reader),
            root_id,
            &CanonicalPath::new("payload").unwrap(),
            &mut LogicalCounters::default(),
        )
        .unwrap();
        let mut final_payloads = BTreeSet::new();
        rope::visit_extents(
            &CoreReader(&reader),
            rope::FileStateRoot(resolved.record.content_root),
            |extents| {
                final_payloads.extend(extents.iter().map(|extent| extent.payload_object_id));
                Ok(())
            },
        )
        .unwrap();
        assert!(base_payloads.is_subset(&final_payloads));
        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    // ---- Stage 3: bounded tiered frontier spill runs (#111) ----------------

    // `frontier_budget` and therefore the pending capacity are not a monotone
    // function of the policy byte count at small values (the journal allowance
    // moves in 64-byte steps), so search a bounded window around the estimate
    // instead of assuming one direction. The constructed instance is the
    // authority: every test asserts `batch_size == B`.
    fn batch_for(memory_bytes: u64) -> u64 {
        let io_bytes = journal_io_bytes(memory_bytes);
        let frontier_budget = memory_bytes.saturating_sub(4 * io_bytes.saturating_sub(256) as u64);
        1 + (frontier_budget.saturating_sub(1024) / 512)
    }

    fn policy_for_batch(batch_size: u64) -> crate::ResourcePolicy {
        assert!(batch_size >= 1);
        let base = 1024 + batch_size * 512;
        let low = base.saturating_sub(4 * 512);
        for memory_bytes in low..base + 4 * 512 {
            if batch_for(memory_bytes) == batch_size {
                return crate::ResourcePolicy {
                    max_final_delta_memory_bytes: memory_bytes,
                    ..crate::ResourcePolicy::default()
                };
            }
        }
        panic!("no policy produces pending capacity {batch_size}");
    }

    fn frontier_for(workspace: &Workspace, policy: &crate::ResourcePolicy) -> FrontierInodes {
        assert!(policy.check_final_delta(1024).is_ok());
        let io_bytes = journal_io_bytes(policy.max_final_delta_memory_bytes);
        let frontier_budget = policy
            .max_final_delta_memory_bytes
            .saturating_sub(4 * io_bytes.saturating_sub(256) as u64);
        let tree_scratch = usize::try_from(frontier_budget.saturating_sub(1024) / 2)
            .unwrap_or(usize::MAX)
            .min(SORTED_TREE_UPDATE_SCRATCH_BYTES);
        FrontierInodes::new(
            workspace.base_root,
            1 + (frontier_budget.saturating_sub(1024) / 512) as usize,
            tree_scratch,
            &workspace.spool,
        )
    }

    fn serial_keys(count: usize) -> Vec<InodeId> {
        (0..count)
            .map(|serial| InodeId::allocate([0x5a; 32], serial as u64))
            .collect()
    }

    fn staged_record(key: InodeId, refs: u64) -> InodeRecordV1 {
        InodeRecordV1 {
            kind: InodeKind::RegularFile,
            namespace_ref_count: refs,
            content_root: ObjectId::from_bytes(&key.0).unwrap(),
            metadata_root: ObjectId::from_bytes(&[0x11; 32]).unwrap(),
        }
    }

    // Read the whole resolved frontier back through the same run representation
    // the commit path consumes.
    fn resolved_rows(inodes: &mut FrontierInodes) -> Vec<(InodeId, Option<InodeRecordV1>)> {
        inodes.finalize().unwrap();
        let file = inodes.spill.as_ref().expect("finalized spill");
        let mut rows = Vec::with_capacity(inodes.spilled_count as usize);
        for index in 0..inodes.spilled_count {
            let mut bytes = [0; 192];
            file.read_exact_at(&mut bytes, index * 192).unwrap();
            rows.push(FrontierInodes::decode(&bytes).unwrap());
        }
        rows.into_iter()
            .map(|(inode, value)| (inode, value.record))
            .collect()
    }

    fn assert_sorted_unique(rows: &[(InodeId, Option<InodeRecordV1>)]) {
        assert!(
            rows.windows(2).all(|pair| pair[0].0 < pair[1].0),
            "spill rows must stay sorted and key-unique"
        );
    }

    // Reduced-budget integration proof: the real Workspace Commit path on a real
    // Store, with a frontier small enough that a reachable changed set crosses many
    // flushes. This is a separately declared case, not default-policy performance:
    // `max_final_delta_memory_bytes` is a supported policy parameter of this entry
    // point and the default policy is untouched.
    //
    // Select explicitly: cargo test -p layerfs-workspace --lib --ignored \
    //   changes::tests::reduced_budget_workspace_commit_spills_exactly
    #[test]
    #[ignore = "reduced-budget integration proof; run explicitly when collecting Stage 3 evidence"]
    fn reduced_budget_workspace_commit_spills_exactly() {
        const FILES: usize = 600;
        let root = std::env::temp_dir().join(format!(
            "layerfs-stage3-reduced-budget-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("stage3-reduced-budget").unwrap(),
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
        // The same policy derivation `CandidateInputs::build` uses, at a declared
        // reduced budget: this must yield a small pending capacity.
        let policy = policy_for_batch(16);
        let mut workspace =
            Workspace::open_with_policy(store.clone(), branch, root.join("spool"), policy).unwrap();
        let io_bytes = journal_io_bytes(policy.max_final_delta_memory_bytes);
        let frontier_budget = policy
            .max_final_delta_memory_bytes
            .saturating_sub(4 * io_bytes.saturating_sub(256) as u64);
        let batch = 1 + (frontier_budget.saturating_sub(1024) / 512);
        assert_eq!(batch, 16, "declared reduced-budget pending capacity");
        let mut expected = Vec::with_capacity(FILES);
        for index in 0..FILES {
            let name = format!("f{index:04}");
            let payload = format!("payload-{index:04}");
            let file = workspace
                .create_file(ROOT, name.as_bytes(), 0o640)
                .unwrap()
                .node;
            workspace.write(file, 0, payload.as_bytes()).unwrap();
            expected.push((name, payload));
        }
        let built = workspace
            .build_candidate(CandidatePurpose::Preview)
            .unwrap();
        let candidate_root = built.built.root_id;
        let outcome = store
            .commit_candidate(
                &store.branch(branch).unwrap().unwrap(),
                workspace.base_root,
                workspace.expected_base,
                built.built,
            )
            .unwrap();
        let CommitOutcome::Committed { root_id, .. } = outcome else {
            panic!("reduced-budget Commit was not created")
        };
        assert_eq!(root_id, candidate_root);
        let reader = store.snapshot_reader(root_id);
        let core = CoreReader(&reader);
        for (name, payload) in &expected {
            let mut bytes = Vec::new();
            filesystem::stream(
                &core,
                root_id,
                &CanonicalPath::new(name).unwrap(),
                &mut bytes,
            )
            .unwrap();
            assert_eq!(&bytes, payload.as_bytes(), "{name}");
        }
        // The declared case must actually spill, and the whole run must stay inside
        // the derived bound for this capacity.
        let stats = workspace.frontier_stats();
        let flushes = stats.batch_flushes;
        assert!(flushes >= 8, "expected many flushes, saw {flushes}");
        // Every created file plus the root directory it was created in.
        assert_eq!(stats.spill_keys, FILES as u64 + 1);
        let f = flushes.max(1);
        let levels = (usize::BITS - f.leading_zeros()) as u64;
        let bound = batch as u64 * f * (1 + levels + 2) + 2 * stats.spill_keys;
        assert!(
            stats.record_writes <= bound,
            "writes={} bound={bound}",
            stats.record_writes
        );
        assert!(stats.peak_live_runs <= levels + 1);
        println!(
            "reduced-budget batch={batch} files={FILES} flushes={flushes} \
             writes={} reads={} bytes_written={} bytes_read={} merges={} deepest_level={} \
             peak_runs={} peak_open_files={} scratch={} peak_live_bytes={} \
             control_writes={} control_reads={}",
            stats.record_writes,
            stats.record_reads,
            stats.bytes_written,
            stats.bytes_read,
            stats.merges,
            stats.merge_levels,
            stats.peak_live_runs,
            stats.peak_open_files,
            stats.peak_scratch_bytes,
            stats.peak_live_bytes,
            batch as u64 * f * (f + 1) / 2 + batch as u64,
            batch as u64 * f * (f - 1) / 2,
        );
        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    // Exact current resolution of one staged key, independent of which structure
    // holds it. Runs and the map are one frontier, not two.
    fn assert_resolves(inodes: &mut FrontierInodes, key: InodeId, expected: Option<InodeRecordV1>) {
        let value = match inodes.pending.get(&key) {
            Some(value) => Some(*value),
            None => inodes.spilled(key).unwrap().map(|(_, _, value)| value),
        }
        .expect("staged key is missing from every structure");
        assert_eq!(value.record, expected, "resolution drifted");
    }

    // Deterministic counters at B-1, B, B+1, 2B, 4B and 8B. The control's cost is
    // arithmetic on its own accounting (flush f reads (f-1)*B and writes f*B),
    // which the campaign's legacy model reproduces exactly.
    #[test]
    fn tiered_spill_traffic_is_logarithmic_and_below_the_quadratic_control() {
        let (root, workspace) = empty_workspace("spill-scaling");
        for batch in [8_usize, 16, 32] {
            let policy = policy_for_batch(batch as u64);
            for factor in [0_usize, 1, 2, 4, 8] {
                let keys = serial_keys(batch * factor.max(1));
                let count = if factor == 0 {
                    batch - 1
                } else {
                    batch * factor
                };
                let keys = &keys[..count];
                let mut inodes = frontier_for(&workspace, &policy);
                assert_eq!(inodes.batch_size, batch);
                for (index, key) in keys.iter().enumerate() {
                    inodes
                        .set(*key, Some(staged_record(*key, 1 + index as u64 % 3)))
                        .unwrap();
                }
                let flushes = inodes.stats.batch_flushes.get();
                // The first flush happens on the insert that finds the map already
                // holding `batch` entries, so `count` keys produce `count - batch`
                // (saturating) later flushes plus that first one.
                let expected_flushes = if count <= batch {
                    0
                } else {
                    1 + ((count - batch - 1) / batch) as u64
                };
                assert_eq!(flushes, expected_flushes, "B={batch} count={count}");
                let rows = resolved_rows(&mut inodes);
                let stats = inodes.stats.snapshot(inodes.batch_size);
                assert_eq!(rows.len(), count, "B={batch} count={count}");
                assert_sorted_unique(&rows);
                assert_eq!(inodes.stats.spill_keys.get(), count as u64);
                // Level scaling: each merge more than doubles its older input, so
                // the record rewrite count is bounded by the derived O(K log K/B)
                // expression and never by the quadratic control expression.
                let f = stats.batch_flushes.max(1);
                let levels = (usize::BITS - f.leading_zeros()) as u64;
                let bound = batch as u64 * f * (1 + levels + 2) + 2 * count as u64;
                assert!(
                    stats.record_writes <= bound,
                    "B={batch} count={count} writes={} bound={bound}",
                    stats.record_writes
                );
                assert!(
                    stats.merge_levels <= levels + 1,
                    "B={batch} count={count} deepest_level={}",
                    stats.merge_levels
                );
                let control_writes = batch as u64 * f * (f + 1) / 2;
                let control_reads = batch as u64 * f * (f - 1) / 2;
                println!(
                    "spill-scaling B={batch} flush={} count={count} new_writes={} new_reads={} \
                     control_writes={control_writes} control_reads={control_reads} \
                     merges={} levels={} live_runs={} scratch={} live_bytes={} merge_read={}",
                    stats.batch_flushes,
                    stats.record_writes,
                    stats.record_reads,
                    stats.merges,
                    stats.merge_levels,
                    stats.peak_live_runs,
                    stats.peak_scratch_bytes,
                    stats.peak_live_bytes,
                    stats.peak_merge_read_bytes,
                );
            }
        }
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    // Production budget derived from the shipped default policy, exercised at
    // increasing multiples of the pending capacity. `8B` does not fit the original
    // 100 000-file namespace, so it is proved here rather than in a public cell.
    fn production_budget_traffic(factors: &[u64]) {
        let (root, workspace) = empty_workspace("spill-production-budget");
        let policy = crate::ResourcePolicy::default();
        let io_bytes = journal_io_bytes(policy.max_final_delta_memory_bytes);
        let frontier_budget = policy
            .max_final_delta_memory_bytes
            .saturating_sub(4 * io_bytes.saturating_sub(256) as u64);
        let batch = 1 + (frontier_budget.saturating_sub(1024) / 512);
        let tree_scratch = usize::try_from(frontier_budget.saturating_sub(1024) / 2)
            .unwrap_or(usize::MAX)
            .min(SORTED_TREE_UPDATE_SCRATCH_BYTES);
        let mut previous: Option<u64> = None;
        let mut worst_ratio = 0.0_f64;
        let mut final_ratio = 0.0_f64;
        let mut control_total = 0_u64;
        for &factor in factors {
            let count = batch * factor;
            let keys = serial_keys(count as usize);
            let mut inodes = FrontierInodes::new(
                workspace.base_root,
                batch as usize,
                tree_scratch,
                &workspace.spool,
            );
            assert_eq!(inodes.batch_size as u64, batch);
            for (index, key) in keys.iter().enumerate() {
                inodes
                    .set(*key, Some(staged_record(*key, 1 + index as u64 % 3)))
                    .unwrap();
            }
            let rows = resolved_rows(&mut inodes);
            assert_eq!(rows.len() as u64, count);
            assert_sorted_unique(&rows);
            let stats = inodes.stats.snapshot(inodes.batch_size);
            let flush = stats.batch_flushes.max(1);
            // Original single-spill model: flush f reads (f-1) batches and
            // writes f batches, including the final pending flush.
            let control_writes = batch * flush * (flush + 1) / 2;
            let control_reads = batch * flush * (flush - 1) / 2;
            control_total = control_writes + control_reads;
            let levels = (usize::BITS - flush.leading_zeros()) as u64;
            let bound = batch * flush * (1 + levels + 2) + 2 * count;
            assert!(
                stats.record_writes <= bound,
                "factor={factor} writes={} bound={bound}",
                stats.record_writes
            );
            assert!(
                stats.merge_levels <= levels + 1,
                "factor={factor} deepest_level={}",
                stats.merge_levels
            );
            let candidate = stats.record_writes + stats.record_reads;
            // Traffic must grow like K log(K/B), not like K^2/B: doubling the size
            // stays well under 3.5x, while the control's model grows by factor+1.
            // Below the crossover the control has little accumulated spill to
            // rewrite, so the candidate's one consolidation pass may be larger; the
            // quadratic term must have taken over above 8B.
            assert_eq!(stats.record_writes, count * (1 + factor.ilog2() as u64));
            assert_eq!(stats.record_reads, count * factor.ilog2() as u64);
            if let Some(previous) = previous {
                // 1B has no merge or final copy: the first doubling is exactly
                // 6x total traffic. Thereafter the ratio is <=10/3 and declines.
                assert!(
                    factor == 2 || candidate < previous * 7 / 2,
                    "factor={factor} candidate={candidate} previous={previous}"
                );
                println!(
                    "spill-production growth factor={factor} ratio={:.3} (control model \
                     grows by factor+1 here)",
                    candidate as f64 / previous as f64
                );
            }
            if factor > 8 {
                assert!(
                    candidate < control_total,
                    "factor={factor} candidate={candidate} control={control_total}"
                );
            } else if factor > 2 {
                assert!(
                    candidate < control_total * 2,
                    "factor={factor} candidate={candidate} control={control_total}"
                );
            }
            worst_ratio = worst_ratio.max(candidate as f64 / control_total as f64);
            final_ratio = candidate as f64 / control_total as f64;
            previous = Some(candidate);
            println!(
                "spill-production batch={} count={count} flushes={} new_writes={} \
                 new_reads={} control_writes={control_writes} control_reads={control_reads} \
                 merges={} deepest_level={} peak_runs={} peak_open_files={} scratch={} \
                 peak_live_bytes={}",
                stats.batch_size,
                stats.batch_flushes,
                stats.record_writes,
                stats.record_reads,
                stats.merges,
                stats.merge_levels,
                stats.peak_live_runs,
                stats.peak_open_files,
                stats.peak_scratch_bytes,
                stats.peak_live_bytes,
            );
        }
        assert!(control_total > 0);
        println!(
            "spill-production final_ratio={final_ratio:.4} worst_ratio={worst_ratio:.4} \
             (worst_ratio includes the startup crossover)"
        );
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "production-budget spill proof; select explicitly"]
    fn production_budget_spill_boundaries_stay_bounded() {
        production_budget_traffic(&[1, 2, 4, 8]);
    }

    // The quadratic control only becomes dominant well above 8B, where the original
    // 100 000-file namespace cannot reach; this is the structural-complexity proof.
    #[test]
    #[ignore = "structural sweep to 64B; run explicitly when collecting Stage 3 evidence"]
    fn production_budget_spill_quadratic_crossover() {
        production_budget_traffic(&[1, 2, 4, 8, 16, 32, 64]);
    }

    // Exact results across key orders, clustered/spread keys, repeated updates,
    // deletion, reversion and post-flush revisions, against an independent model.
    #[test]
    fn tiered_spill_resolution_matches_the_model_for_every_key_order() {
        use std::collections::BTreeMap;
        let (root, workspace) = empty_workspace("spill-orders");
        let batch = 8_usize;
        let policy = policy_for_batch(batch as u64);
        let count = batch * 9;
        let keys = serial_keys(count);
        let ascending: Vec<InodeId> = keys.clone();
        let mut descending = keys.clone();
        descending.reverse();
        let mut interleaved = Vec::with_capacity(count);
        for index in 0..count {
            let mut value = index as u64;
            let mut reversed = 0_u64;
            for _ in 0..8 {
                reversed = (reversed << 1) | (value & 1);
                value >>= 1;
            }
            interleaved.push((reversed % count as u64, index));
        }
        interleaved.sort();
        let interleaved: Vec<InodeId> = interleaved
            .into_iter()
            .map(|(_, index)| keys[index])
            .collect();
        let mut clustered = Vec::with_capacity(count);
        for block in 0..3 {
            let start = block * (count / 3);
            for offset in 0..(count / 3) {
                clustered.push(keys[start + (count / 3 - 1 - offset)]);
            }
        }
        for (label, order) in [
            ("ascending", ascending),
            ("descending", descending),
            ("interleaved", interleaved),
            ("clustered", clustered),
        ] {
            let mut inodes = frontier_for(&workspace, &policy);
            let mut model = BTreeMap::new();
            for (index, key) in order.iter().enumerate() {
                let record = staged_record(*key, 1 + index as u64 % 3);
                inodes.set(*key, Some(record)).unwrap();
                model.insert(*key, Some(record));
                // Revise a key that has already been flushed out of the map.
                if index == count / 2 {
                    let revised = staged_record(order[0], 7);
                    inodes.set(order[0], Some(revised)).unwrap();
                    model.insert(order[0], Some(revised));
                }
            }
            // Repeated updates on live, spilled and tombstoned keys.
            for (offset, key) in order.iter().take(3).enumerate() {
                let revised = staged_record(*key, 11 + offset as u64);
                inodes.set(*key, Some(revised)).unwrap();
                model.insert(*key, Some(revised));
            }
            let removed = order[count - 1];
            inodes.set(removed, None).unwrap();
            model.insert(removed, None);
            for (offset, key) in order.iter().skip(1).take(2).enumerate() {
                let reverted = staged_record(*key, 21 + offset as u64);
                inodes.set(*key, Some(reverted)).unwrap();
                model.insert(*key, Some(reverted));
            }
            let rows = resolved_rows(&mut inodes);
            assert_sorted_unique(&rows);
            let expected = model
                .iter()
                .map(|(key, record)| (*key, *record))
                .collect::<Vec<_>>();
            assert_eq!(rows, expected, "{label}");
            assert!(inodes.stats.merges.get() > 0, "{label} must spill");
        }
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    // A post-flush revision of a key that sits outside the newest tier must still
    // resolve to the newest value, and the merged row set must stay key-unique.
    #[test]
    fn tiered_spill_revision_outside_the_newest_tier_keeps_the_latest_value() {
        use std::collections::BTreeMap;
        let (root, workspace) = empty_workspace("spill-late-revision");
        let batch = 4_usize;
        let policy = policy_for_batch(batch as u64);
        let count = batch * 6;
        let keys = serial_keys(count);
        let mut inodes = frontier_for(&workspace, &policy);
        let mut model = BTreeMap::new();
        for key in keys.iter() {
            let record = staged_record(*key, 1);
            inodes.set(*key, Some(record)).unwrap();
            model.insert(*key, Some(record));
        }
        // Every staged key is spilled now.
        assert!(inodes.run_record_count() >= batch as u64 * 3);
        for offset in 0..batch * 2 {
            let record = staged_record(keys[offset], 40 + offset as u64);
            inodes.set(keys[offset], Some(record)).unwrap();
            model.insert(keys[offset], Some(record));
        }
        // A tombstone outside the newest tier re-enters the map as well.
        inodes.set(keys[batch * 2], None).unwrap();
        model.insert(keys[batch * 2], None);
        // A reference adjustment on a spilled key takes the same resolution path.
        let mut adjusted = model[&keys[batch * 2 + 1]].unwrap();
        adjusted.namespace_ref_count = 5;
        inodes.set(keys[batch * 2 + 1], Some(adjusted)).unwrap();
        model.insert(keys[batch * 2 + 1], Some(adjusted));
        for (key, expected) in model.iter() {
            assert_resolves(&mut inodes, *key, *expected);
        }
        let rows = resolved_rows(&mut inodes);
        assert_sorted_unique(&rows);
        assert_eq!(
            rows,
            model
                .iter()
                .map(|(key, record)| (*key, *record))
                .collect::<Vec<_>>()
        );
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tiered_spill_crosses_the_old_highest_tier_without_prefix_rewrites() {
        let (root, workspace) = empty_workspace("spill-highest-tier");
        let batch = 8;
        for flushes in [64_usize, 128, 256] {
            let mut inodes = frontier_for(&workspace, &policy_for_batch(batch as u64));
            let mut model = BTreeMap::new();
            for key in serial_keys(batch * flushes) {
                let record = staged_record(key, 1);
                inodes.set(key, Some(record)).unwrap();
                model.insert(key, Some(record));
            }
            inodes.merge_pending().unwrap();
            let levels = flushes.ilog2() as u64;
            let count = (batch * flushes) as u64;
            println!(
                "spill-highest-tier before_finalize rows={count} flushes={flushes} stats={:?}",
                inodes.stats.snapshot(batch)
            );
            assert_eq!(
                inodes.stats.record_writes.get(),
                count * (1 + levels),
                "growing-prefix rewrite before finalization"
            );
            let rows = resolved_rows(&mut inodes);
            assert_eq!(rows, model.into_iter().collect::<Vec<_>>());
            let stats = inodes.stats.snapshot(batch);
            assert_eq!(stats.batch_flushes, flushes as u64);
            assert_eq!(stats.record_writes, count * (1 + levels));
            assert_eq!(stats.record_reads, count * levels);
            assert_eq!(stats.merge_levels, levels);
            assert!(stats.peak_open_files <= levels + 2);
            assert!(stats.peak_scratch_bytes <= (batch * 256 + 512) as u64);
            assert!(stats.peak_live_bytes <= 3 * count * RUN_ROW_BYTES as u64);
            assert!(
                stats.peak_allocated_bytes <= stats.peak_live_bytes + stats.peak_open_files * 4096,
                "allocated disk exceeded the logical bound plus one 4KiB tail per file"
            );
            assert_eq!(
                inodes.spill.as_ref().unwrap().metadata().unwrap().len(),
                count * RUN_ROW_BYTES as u64
            );
            println!("spill-highest-tier rows={count} flushes={flushes} stats={stats:?} run_size={} filter_capacity={} run_capacity={}",
                std::mem::size_of::<Option<SpillRun>>(), inodes.filter.capacity(), inodes.runs.capacity());
        }
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tiered_spill_partial_writes_and_final_merge_are_retryable_and_clean() {
        let (root, workspace) = empty_workspace("spill-write-failure");
        let batch = 4;
        let fd_directory = if std::path::Path::new("/dev/fd").exists() {
            "/dev/fd"
        } else {
            "/proc/self/fd"
        };
        let fd_count = || std::fs::read_dir(fd_directory).unwrap().count();
        let spool_count = || std::fs::read_dir(&workspace.spool).unwrap().count();
        let outside_fds = fd_count();
        let outside_spool = spool_count();
        // Seven runs' worth of data force three carries on the next flush.
        for fail_after in [
            0,
            1,
            batch + 1,
            batch + 2 * batch + 1,
            batch + 2 * batch + 4 * batch + 1,
        ] {
            let mut inodes = frontier_for(&workspace, &policy_for_batch(batch as u64));
            let mut model = BTreeMap::new();
            for key in serial_keys(batch * 8) {
                let record = staged_record(key, 2);
                inodes.set(key, Some(record)).unwrap();
                model.insert(key, Some(record));
            }
            let original_bytes = inodes.live_run_bytes();
            let original_fds = fd_count();
            INJECT_SPILL_WRITE_FAILURE.with(|inject| inject.set(Some(fail_after as u64)));
            assert!(inodes.merge_pending().is_err());
            assert_eq!(inodes.pending.len(), batch);
            assert_eq!(inodes.live_run_bytes(), original_bytes);
            assert_eq!(fd_count(), original_fds, "partial output descriptor leaked");
            assert_eq!(
                spool_count(),
                outside_spool,
                "named temporary output leaked"
            );
            for (&key, &record) in &model {
                assert_resolves(&mut inodes, key, record);
            }
            let rows = resolved_rows(&mut inodes);
            assert_eq!(rows, model.into_iter().collect::<Vec<_>>());
            drop(inodes);
            assert_eq!(fd_count(), outside_fds);
            println!("spill-write-failure after={fail_after} inputs_restored=true anonymous_output_closed=true");
        }
        let mut inodes = frontier_for(&workspace, &policy_for_batch(batch as u64));
        let keys = serial_keys(batch * 4);
        for &key in &keys {
            inodes.set(key, Some(staged_record(key, 7))).unwrap();
        }
        let original_directory = inodes.directory.clone();
        inodes.directory = root.join("missing-spill-directory");
        assert!(inodes.merge_pending().is_err());
        assert_eq!(inodes.pending.len(), batch);
        inodes.directory = original_directory;
        let damaged_level = inodes.runs.iter().rposition(Option::is_some).unwrap();
        let damaged = &inodes.runs[damaged_level].as_ref().unwrap().file;
        let mut original = vec![0; damaged.metadata().unwrap().len() as usize];
        damaged.read_exact_at(&mut original, 0).unwrap();
        damaged.set_len((RUN_ROW_BYTES + 1) as u64).unwrap();
        let original_fds = fd_count();
        assert!(
            inodes.merge_pending().is_err(),
            "short final row must fail without panicking"
        );
        assert_eq!(fd_count(), original_fds);
        inodes.runs[damaged_level]
            .as_ref()
            .unwrap()
            .file
            .write_all_at(&original, 0)
            .unwrap();
        for &key in &keys {
            assert_resolves(&mut inodes, key, Some(staged_record(key, 7)));
        }
        assert_eq!(resolved_rows(&mut inodes).len(), keys.len());
        drop(inodes);
        assert_eq!(fd_count(), outside_fds);
        assert_eq!(spool_count(), outside_spool);

        let mut inodes = frontier_for(&workspace, &policy_for_batch(batch as u64));
        let mut model = BTreeMap::new();
        for key in serial_keys(batch * 7) {
            let record = staged_record(key, 3);
            inodes.set(key, Some(record)).unwrap();
            model.insert(key, Some(record));
        }
        inodes.merge_pending().unwrap();
        let original_bytes = inodes.live_run_bytes();
        let original_fds = fd_count();
        INJECT_SPILL_WRITE_FAILURE.with(|inject| inject.set(Some((3 * batch + 1) as u64)));
        assert!(
            inodes.finalize().is_err(),
            "failure must hit the second final merge"
        );
        assert!(inodes.spill.is_none());
        assert_eq!(inodes.live_run_bytes(), original_bytes);
        assert_eq!(fd_count(), original_fds);
        assert_eq!(spool_count(), outside_spool);
        for (&key, &record) in &model {
            assert_resolves(&mut inodes, key, record);
        }
        assert_eq!(
            resolved_rows(&mut inodes),
            model.into_iter().collect::<Vec<_>>()
        );
        drop(inodes);
        assert_eq!(fd_count(), outside_fds);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    // Failure and retry: a failed flush must leave both the runs and the map
    // usable, and the retry must produce the same resolution.
    #[test]
    fn tiered_spill_flush_failure_keeps_runs_and_map_intact() {
        use std::collections::BTreeMap;
        let (root, workspace) = empty_workspace("spill-retry");
        let batch = 4_usize;
        let policy = policy_for_batch(batch as u64);
        let keys = serial_keys(batch * 5);
        let mut inodes = frontier_for(&workspace, &policy);
        let mut model = BTreeMap::new();
        for key in keys.iter().take(batch * 3) {
            let record = staged_record(*key, 2);
            inodes.set(*key, Some(record)).unwrap();
            model.insert(*key, Some(record));
        }
        let flushes = inodes.stats.batch_flushes.get();
        let live = inodes.run_record_count();
        assert_eq!(inodes.pending.len(), batch);
        // The next insertion fills the map and fails inside the flush itself.
        INJECT_INODE_MERGE_FAILURE.with(|inject| inject.set(true));
        let failed = keys[batch * 3];
        let record = staged_record(failed, 3);
        assert!(inodes.set(failed, Some(record)).is_err());
        // The failed attempt wrote its candidate run but retained every input row
        // and left the map exactly as it was for a clean retry.
        assert_eq!(inodes.stats.batch_flushes.get(), flushes + 1);
        assert_eq!(inodes.run_record_count(), live);
        assert_eq!(inodes.pending.len(), batch);
        for (key, expected) in model.iter() {
            assert_resolves(&mut inodes, *key, *expected);
        }
        // Retry succeeds and produces the model's resolution exactly.
        inodes.merge_pending().unwrap();
        inodes.set(failed, Some(record)).unwrap();
        model.insert(failed, Some(record));
        let extra = keys[batch * 3 + 1];
        let record = staged_record(extra, 4);
        inodes.set(extra, Some(record)).unwrap();
        model.insert(extra, Some(record));
        let rows = resolved_rows(&mut inodes);
        assert_sorted_unique(&rows);
        assert_eq!(
            rows,
            model
                .iter()
                .map(|(key, record)| (*key, *record))
                .collect::<Vec<_>>()
        );
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }
}
