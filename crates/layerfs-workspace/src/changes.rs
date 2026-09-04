use crate::cow_tree::{portable_metadata, Attr, Data, FileData, Kind, NodeId, Workspace, ROOT};
use layerfs_content::file::rope::{self, FileMutationBatch, FileStateRoot, RopeCounters};
use layerfs_content::filesystem::{self, LogicalCounters};
use layerfs_content::object::access::{ObjectRead, ObjectStore};
use layerfs_content::object::{ContentDigestWriter, ObjectId};
use layerfs_content::tree::directory::{
    directory_lookup, directory_page_after, empty_directory, DirectoryStateRoot, NamespaceCounters,
};
use layerfs_content::tree::inode::codec::decode_inode_record;
use layerfs_content::tree::inode::{inode_table_lookup, InodeTableCounters};
use layerfs_content::tree::inode::{InodeId, InodeKind, InodeRecordV1};
use layerfs_content::{CanonicalName, CanonicalPath};
use layerfs_layerstack_store::{
    BuiltRoot, CoreReader, ObjectBuffer, Result, StoreError as StorageError, WorkspaceCommitPhase,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::FileExt;
use std::time::Instant;

#[derive(Clone, Copy)]
#[cfg(test)]
struct BaseEntry {
    record: InodeRecordV1,
    mode: u32,
    mtime_seconds: i64,
    mtime_nanoseconds: u32,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum FingerprintNode {
    Workspace(NodeId),
    Base(InodeId),
}

#[derive(Clone, Copy)]
struct ContentHeader {
    generation: u64,
    base_root: ObjectId,
    node: NodeId,
    inode: InodeId,
    attr: Attr,
    before: Option<InodeRecordV1>,
    count: u64,
    metadata_root: ObjectId,
}
struct ContentTask {
    header: ContentHeader,
    file: crate::commit_file::FrozenFile,
    private_budget: usize,
}
struct ContentResult {
    header: ContentHeader,
    file: crate::commit_file::CompiledFile,
}
struct ContentPool {
    pool: layerfs_layerstack_store::PrivateContentPool<ContentTask, ContentResult>,
    reserved: u64,
    file_share: u64,
}
fn compile_content_task(
    task: ContentTask,
    store: &mut layerfs_layerstack_store::ConstructionWorkerStore,
) -> Result<ContentResult> {
    Ok(ContentResult {
        header: task.header,
        file: task.file.compile(store, task.private_budget)?,
    })
}
const CONTENT_COORDINATOR_SCRATCH: u64 = 128 * 1024;
const FILE_CONSTRUCTION_SHARE: u64 = rope::FILE_MUTATION_BATCH_MAX_DEFERRED_BYTES as u64;
const METADATA_CACHE_RESERVE: u64 = 8 * 64;

impl Workspace {
    pub(crate) fn build_candidate(&mut self) -> Result<BuiltRoot> {
        #[cfg(any(debug_assertions, feature = "test-instrumentation"))]
        if INJECT_CANDIDATE_FAILURE.with(|inject| inject.replace(false)) {
            return Err(StorageError::Integrity(
                "injected Workspace candidate failure",
            ));
        }
        self.commit_handoff = None;
        crate::commit_spool::reset_metrics();
        let result = self.build_frontier_candidate();
        if let Some(stats) = crate::commit_spool::take_metrics() {
            layerfs_layerstack_store::note_workspace_commit_sort(
                stats.sequential_read_calls,
                stats.sequential_read_bytes,
                stats.positional_read_calls,
                stats.positional_read_bytes,
                stats.write_calls,
                stats.write_bytes,
                stats.merge_passes,
            );
        }
        result
    }

    // Directory overlays already are the final binding delta. Applying their inode
    // edges directly preserves untouched subtrees, including a renamed directory,
    // without building either complete namespace manifest.
    fn build_frontier_candidate(&mut self) -> Result<BuiltRoot> {
        let started = CommitPhaseStart::new();
        let mut handoff = self.begin_handoff()?;
        let memory = self
            .policy
            .max_final_delta_memory_bytes
            .checked_sub(handoff.buffer_capacity)
            .ok_or(StorageError::InvalidInput("workspace final-delta limit"))?;

        let disk = self
            .policy
            .max_spool_bytes
            .checked_sub(self.spool_bytes)
            .and_then(|n| n.checked_sub(self.references.bytes()))
            .and_then(|n| n.checked_sub(handoff.reserved_spool_bytes))
            .ok_or(StorageError::InvalidInput("workspace spool limit"))?;
        let mut references = self.references.sorted(&self.spool, disk / 4, memory / 8)?;
        let mut nodes = crate::commit_spool::Sorter::<41>::new(&self.spool, disk / 4, memory / 8)?;
        for id in self.commit_nodes() {
            let id = id?;
            let value = &self.nodes[&id];
            let mut record = [0; 41];
            record[..33].copy_from_slice(&crate::references::key(value.canonical, id));
            record[33..].copy_from_slice(&id.0.to_be_bytes());
            nodes.push(record)?;
        }
        let mut nodes = nodes.finish()?;
        let mut next_reference = references.next()?;
        let mut pairs = crate::commit_spool::Sorter::<65>::new(&self.spool, disk / 4, memory / 8)?;
        let captured = self.take_capture();
        if let Some(captured) = &captured {
            layerfs_layerstack_store::note_workspace_capture(1, captured.len);
        }
        let (mut objects, captured) = match captured {
            Some(crate::capture::CapturedFile {
                node,
                len,
                root,
                counters,
                objects,
            }) => (
                ObjectBuffer::resume_prevalidated(&self.reader, objects),
                Some((node, len, root, counters)),
            ),
            None => (ObjectBuffer::new(&self.reader)?, None),
        };
        let mut metadata_cache = Vec::new();
        let mut tree_counts = layerfs_content::tree::batch::TreeBatchCounters::default();
        let mut cdc_bytes_scanned = 0_u64;
        let mut content_pool: Option<ContentPool> = None;
        note_commit_phase(WorkspaceCommitPhase::CandidatePlan, started);
        let started = CommitPhaseStart::new();
        while let Some(identity) = nodes.next()? {
            let node = NodeId(u64::from_be_bytes(identity[33..].try_into().unwrap()));
            let value = &self.nodes[&node];
            layerfs_layerstack_store::note_workspace_namespace_visits(0, 0, 0, 0, 1);
            let key = &identity[..33];
            let mut change = 0i64;
            let mut reference_base = None;
            while let Some(reference) = next_reference.filter(|r| r[..33] <= *key) {
                if reference[..33] == *key {
                    let hint = crate::references::base_hint(&reference);
                    if reference_base
                        .replace(hint)
                        .is_some_and(|previous| previous != hint)
                    {
                        return Err(StorageError::Integrity("namespace reference base hint"));
                    }
                    change = change
                        .checked_add(crate::references::delta(&reference))
                        .ok_or(StorageError::Integrity("namespace reference overflow"))?;
                }
                next_reference = references.next()?;
            }
            let before = value
                .canonical
                .map(|inode| {
                    let reserved = content_pool.as_ref().map_or(0, |pool| pool.reserved)
                        + self.references.capacity()
                        + (memory / 8).min(crate::commit_spool::SORT_BYTES)
                        + (metadata_cache.capacity() * 64) as u64;
                    let available = memory
                        .checked_sub(reserved)
                        .ok_or(StorageError::InvalidInput("workspace final-delta limit"))?;
                    self.base_record_with_budget(inode, available as usize)
                })
                .transpose()?;
            let base_count = before.map_or(0, |record| record.namespace_ref_count);
            if reference_base.is_some_and(|hint| hint != base_count) {
                return Err(StorageError::Integrity("namespace reference base hint"));
            }
            let count = final_count(base_count, change)?;
            if count == 0 && node != ROOT {
                continue;
            }
            let inode = self.frontier_inode(node)?;
            let attr = self.attr(node)?;
            let inode_kind = match attr.kind {
                Kind::File => InodeKind::RegularFile,
                Kind::Directory => InodeKind::Directory,
                Kind::Symlink => InodeKind::Symlink,
            };
            let old_metadata = before
                .map(|record| {
                    crate::commit_file::portable_metadata_bounded(
                        &self.reader,
                        record.metadata_root,
                        record.kind,
                    )
                })
                .transpose()?;
            let metadata_root = if old_metadata.is_some_and(|metadata| {
                metadata.permission_mode == attr.mode
                    && metadata.mtime_seconds == attr.mtime_seconds
                    && metadata.mtime_nanoseconds == attr.mtime_nanoseconds
            }) {
                before.unwrap().metadata_root
            } else {
                let key = (
                    inode_kind,
                    attr.mode,
                    attr.mtime_seconds,
                    attr.mtime_nanoseconds,
                );
                if let Some((_, root)) = metadata_cache.iter().find(|(stored, _)| *stored == key) {
                    *root
                } else {
                    let root = filesystem::build_portable_metadata(
                        &mut objects,
                        inode_kind,
                        attr.mode,
                        attr.mtime_seconds,
                        attr.mtime_nanoseconds,
                    )?;
                    if metadata_cache.len() == 8 {
                        metadata_cache.remove(0);
                    }
                    if metadata_cache.len() == metadata_cache.capacity() {
                        self.policy.check_final_delta(
                            handoff.buffer_capacity
                                + content_pool.as_ref().map_or(0, |pool| pool.reserved)
                                + self.references.capacity()
                                + pairs.capacity()
                                + ((metadata_cache.capacity() + metadata_cache.len() + 1) * 64)
                                    as u64,
                        )?;
                        metadata_cache.reserve_exact(1);
                    }
                    metadata_cache.push((key, root));
                    root
                }
            };
            let header = ContentHeader {
                generation: self.mutation_generation,
                base_root: self.base_root,
                node,
                inode,
                attr,
                before,
                count,
                metadata_root,
            };
            let content_root = match &value.data {
                Data::Directory(directory) => {
                    let content = match directory.base {
                        Some(base) => base,
                        None => empty_directory(&mut objects)?,
                    };
                    loop {
                        let reserve = content_pool.as_ref().map_or(0, |pool| pool.reserved)
                            + self.references.capacity()
                            + pairs.capacity()
                            + (metadata_cache.capacity() * 64) as u64;
                        let budget = memory
                            .checked_sub(reserve)
                            .ok_or(StorageError::InvalidInput("workspace final-delta limit"))?;
                        let changes = directory.changes.iter().map(|(name, desired)| {
                            Ok((
                                CanonicalName::from_bytes(name)?,
                                desired
                                    .map(|child| self.frontier_inode(child))
                                    .transpose()
                                    .map_err(|_| {
                                        layerfs_content::CoreError::InvalidRecord(
                                            "Workspace binding identity",
                                        )
                                    })?,
                            ))
                        });
                        let attempt = layerfs_content::tree::directory::directory_apply_sorted_with_spill_attempt(
                            &mut objects, content, changes, budget as usize, &self.spool, disk / 4);
                        // Include partial work before a pressure retry. Failed
                        // local buffers/spills have already dropped on return.
                        let stats = attempt.counters;
                        tree_counts.spill_write_bytes += stats.spill_write_bytes;
                        tree_counts.spill_read_bytes += stats.spill_read_bytes;
                        tree_counts.peak_spill_bytes =
                            tree_counts.peak_spill_bytes.max(stats.peak_spill_bytes);
                        tree_counts.nodes_read += stats.nodes_read;
                        tree_counts.nodes_created += stats.nodes_created;
                        tree_counts.nodes_reused += stats.nodes_reused;
                        tree_counts.peak_scratch_bytes = tree_counts
                            .peak_scratch_bytes
                            .max(stats.peak_scratch_bytes + reserve as usize);
                        match attempt.result {
                            Ok(next) => break next.0,
                            Err(_) if attempt.budget_denied && content_pool.is_some() => {
                                // Retry exactly this writer/root/input once,
                                // after releasing actual producer reservations.
                                self.drain_content_pool(
                                    &mut content_pool,
                                    &mut objects,
                                    &mut pairs,
                                    &mut handoff,
                                    &mut cdc_bytes_scanned,
                                )?;
                            }
                            Err(error) => return Err(error.into()),
                        }
                    }
                }
                Data::Symlink(target) => match before {
                    Some(record) => record.content_root,
                    None => filesystem::symlink_content(&mut objects, target.clone())?,
                },
                Data::File(_) => {
                    if let Some((_, len, root, counters)) =
                        captured.filter(|(id, _, _, _)| *id == node)
                    {
                        self.finish_content_result(
                            ContentResult {
                                header,
                                file: crate::commit_file::CompiledFile {
                                    root,
                                    len,
                                    counters,
                                    preparation: Default::default(),
                                },
                            },
                            &mut objects,
                            &mut pairs,
                            &mut handoff,
                            &mut cdc_bytes_scanned,
                        )?;
                        continue;
                    }
                    // The one coordinator-held unsent task is separate from
                    // the pool's bounded in-flight ownership window.
                    self.policy.check_final_delta(
                        handoff.buffer_capacity
                            + self.references.capacity()
                            + pairs.capacity()
                            + (metadata_cache.capacity() * 64) as u64
                            + content_pool
                                .as_ref()
                                .map_or(0, |pool| pool.reserved + CONTENT_COORDINATOR_SCRATCH)
                            + self.reader.clone_owned_bytes() as u64
                            + std::mem::size_of::<ContentTask>() as u64,
                    )?;
                    let frozen = crate::commit_file::FrozenFile::freeze(
                        self,
                        node,
                        before.map(|record| FileStateRoot(record.content_root)),
                    )?;
                    let scratch = frozen.scratch_bytes()?;
                    if content_pool.as_ref().is_some_and(|pool| {
                        pool.file_share
                            .checked_sub(scratch)
                            .is_none_or(|private| private < 1024)
                    }) {
                        self.drain_content_pool(
                            &mut content_pool,
                            &mut objects,
                            &mut pairs,
                            &mut handoff,
                            &mut cdc_bytes_scanned,
                        )?;
                    }
                    if content_pool.is_none() {
                        let retained = self.references.capacity()
                            + (memory / 8).min(crate::commit_spool::SORT_BYTES)
                            + METADATA_CACHE_RESERVE;
                        content_pool = self.start_content_pool(memory, retained, scratch)?;
                    }
                    if let Some(pool) = &mut content_pool {
                        tree_counts.peak_scratch_bytes = tree_counts.peak_scratch_bytes.max(
                            (pool.reserved
                                + self.references.capacity()
                                + (memory / 8).min(crate::commit_spool::SORT_BYTES)
                                + METADATA_CACHE_RESERVE
                                + CONTENT_COORDINATOR_SCRATCH) as usize,
                        );
                        let mut task = ContentTask {
                            header,
                            file: frozen,
                            private_budget: pool
                                .file_share
                                .checked_sub(scratch)
                                .filter(|private| *private >= 1024)
                                .ok_or(StorageError::Integrity("content task reservation"))?
                                as usize,
                        };
                        loop {
                            match pool.pool.try_submit(task)? {
                                None => break,
                                Some(unsent) => task = unsent,
                            }
                            if !pool.pool.pump(&mut objects, |result, objects| {
                                self.finish_content_result(
                                    result,
                                    objects,
                                    &mut pairs,
                                    &mut handoff,
                                    &mut cdc_bytes_scanned,
                                )
                            })? {
                                return Err(StorageError::Integrity(
                                    "content pool ended before submission",
                                ));
                            }
                        }
                    } else {
                        let retained = self.references.capacity()
                            + pairs.capacity()
                            + (metadata_cache.capacity() * 64) as u64;
                        // The prior file construction category is shared by
                        // all content workers, distinct from namespace deltas.
                        let private_budget = FILE_CONSTRUCTION_SHARE.checked_sub(scratch).ok_or(
                            StorageError::InvalidInput("workspace file construction limit"),
                        )?;
                        self.policy
                            .check_final_delta(handoff.buffer_capacity + retained)?;
                        tree_counts.peak_scratch_bytes =
                            tree_counts.peak_scratch_bytes.max(retained as usize);
                        layerfs_layerstack_store::note_workspace_content_preparation(
                            0,
                            0,
                            0,
                            0,
                            0,
                            FILE_CONSTRUCTION_SHARE,
                        );
                        let file = frozen.compile(&mut objects, private_budget as usize)?;
                        self.finish_content_result(
                            ContentResult { header, file },
                            &mut objects,
                            &mut pairs,
                            &mut handoff,
                            &mut cdc_bytes_scanned,
                        )?;
                    }
                    continue;
                }
            };
            self.finish_content_inode(
                header,
                content_root,
                &mut objects,
                &mut pairs,
                &mut handoff,
            )?;
        }
        self.drain_content_pool(
            &mut content_pool,
            &mut objects,
            &mut pairs,
            &mut handoff,
            &mut cdc_bytes_scanned,
        )?;
        nodes.remove()?;
        note_commit_phase(WorkspaceCommitPhase::Content, started);
        let started = CommitPhaseStart::new();
        // Reclaimed and unmaterialized existing inodes use authenticated base
        // counts plus the same net binding input. No deleted-subtree walk.
        references.rewind()?;
        drop(metadata_cache);
        // The pair sorter may allocate lazily during this loop; reserve its
        // configured share rather than only its current capacity.
        let reference_reserve =
            self.references.capacity() + (memory / 8).min(crate::commit_spool::SORT_BYTES);
        let reference_available = memory
            .checked_sub(reference_reserve)
            .ok_or(StorageError::InvalidInput("workspace final-delta limit"))?;
        const LOOKUP_PER_KEY: u64 = 32 * 1024;
        let row_bytes = std::mem::size_of::<(InodeId, i64, u64)>() as u64;
        let batch_limit = (reference_available / (LOOKUP_PER_KEY + row_bytes))
            .min(128)
            .max(1) as usize;
        let lookup_budget = reference_available
            .checked_sub(batch_limit as u64 * row_bytes)
            .ok_or(StorageError::InvalidInput("workspace final-delta limit"))?;
        let mut pending_references = Vec::new();
        let mut current = references.next()?;
        while let Some(record) = current.take() {
            let key = <[u8; 33]>::try_from(&record[..33]).unwrap();
            let mut change = crate::references::delta(&record);
            let base_hint = crate::references::base_hint(&record);
            loop {
                current = references.next()?;
                if let Some(next) = current.filter(|next| next[..33] == key) {
                    if crate::references::base_hint(&next) != base_hint {
                        return Err(StorageError::Integrity("namespace reference base hint"));
                    }
                    change = change
                        .checked_add(crate::references::delta(&next))
                        .ok_or(StorageError::Integrity("namespace reference overflow"))?;
                } else {
                    break;
                }
            }
            if key[0] != 0 {
                if base_hint != 0 {
                    return Err(StorageError::Integrity("namespace reference base hint"));
                }
                continue;
            }
            if change == 0 {
                continue;
            }
            let inode = InodeId::from_slice(&key[1..])?;
            if self
                .canonical_nodes
                .get(&inode)
                .is_some_and(|id| self.nodes[id].links != 0)
            {
                continue;
            }
            // Reclamation retained an authenticated count before dropping the
            // Node. Zero final reachability needs no old record content or
            // metadata. Surviving identities still load/check their full record.
            if !self.canonical_nodes.contains_key(&inode) && final_count(base_hint, change)? == 0 {
                push_inode(&mut pairs, &mut objects, inode, None)?;
                continue;
            }
            // Reserve only when the semantic input actually needs a base
            // reference lookup; an empty reference stream owns no lookup batch.
            if pending_references.capacity() == 0 {
                if lookup_budget < 8192 + 512 {
                    return Err(StorageError::InvalidInput("workspace final-delta limit"));
                }
                pending_references
                    .try_reserve_exact(batch_limit)
                    .map_err(|_| StorageError::InvalidInput("workspace reference allocation"))?;
                tree_counts.peak_scratch_bytes = tree_counts.peak_scratch_bytes.max(
                    (reference_reserve
                        + pending_references.capacity() as u64 * row_bytes
                        + if batch_limit == 1 {
                            8192 + 512
                        } else {
                            batch_limit as u64 * LOOKUP_PER_KEY
                        }) as usize,
                );
            }
            pending_references.push((inode, change, base_hint));
            if pending_references.len() == batch_limit {
                self.apply_reference_batch(
                    &mut objects,
                    &mut pairs,
                    &mut pending_references,
                    lookup_budget as usize,
                )?;
            }
        }
        self.apply_reference_batch(
            &mut objects,
            &mut pairs,
            &mut pending_references,
            lookup_budget as usize,
        )?;
        drop(pending_references);
        references.remove()?;
        let mut pairs = pairs.finish()?;
        let input = std::iter::from_fn(|| match pairs.next() {
            Ok(Some(record)) => Some(Ok((
                InodeId::from_slice(&record[..32]).unwrap(),
                (record[32] != 0).then(|| ObjectId::from_bytes(&record[33..]).unwrap()),
            ))),
            Ok(None) => None,
            Err(_) => Some(Err(layerfs_content::CoreError::InvalidRecord(
                "Workspace inode spool",
            ))),
        });
        let namespace = filesystem::namespace(&objects, self.base_root)?;
        let budget = memory
            .checked_sub(self.references.capacity())
            .ok_or(StorageError::InvalidInput("workspace final-delta limit"))?;
        let (table, stats) = layerfs_content::tree::inode::inode_table_apply_sorted_with_spill(
            &mut objects,
            self.base_inodes,
            input,
            budget as usize,
            &self.spool,
            disk / 4,
        )?;
        tree_counts.spill_write_bytes += stats.spill_write_bytes;
        tree_counts.spill_read_bytes += stats.spill_read_bytes;
        tree_counts.peak_spill_bytes = tree_counts.peak_spill_bytes.max(stats.peak_spill_bytes);
        tree_counts.nodes_read += stats.nodes_read;
        tree_counts.nodes_created += stats.nodes_created;
        tree_counts.nodes_reused += stats.nodes_reused;
        tree_counts.peak_scratch_bytes = tree_counts
            .peak_scratch_bytes
            .max(stats.peak_scratch_bytes + self.references.capacity() as usize);
        let root = if table.0 == namespace.inode_table_root {
            self.base_root
        } else {
            objects.put_owned(
                layerfs_content::tree::directory::codec::encode_namespace_root(
                    layerfs_content::tree::NamespaceRootV1 {
                        inode_table_root: table.0,
                        ..namespace
                    },
                )?,
            )?
        };
        pairs.remove()?;
        note_commit_phase(WorkspaceCommitPhase::Namespace, started);
        let started = CommitPhaseStart::new();
        layerfs_layerstack_store::note_workspace_generic_commit(
            self.references.events,
            self.references.bytes(),
            self.references.buffer_peak,
            self.references.bookkeeping_ns,
            tree_counts.nodes_read,
            tree_counts.nodes_created,
            tree_counts.nodes_reused,
            tree_counts.peak_scratch_bytes as u64,
        );
        layerfs_layerstack_store::note_workspace_tree_spill(
            tree_counts.spill_write_bytes,
            tree_counts.spill_read_bytes,
            tree_counts.peak_spill_bytes,
        );
        let built = objects.finish(root, cdc_bytes_scanned)?;
        self.commit_handoff = Some(self.finish_handoff(handoff, root, table)?);
        note_commit_phase(WorkspaceCommitPhase::CandidateFinish, started);
        Ok(built)
    }

    fn start_content_pool(
        &self,
        memory: u64,
        retained: u64,
        scratch: u64,
    ) -> Result<Option<ContentPool>> {
        use layerfs_layerstack_store::PrivateContentPool;
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(8);
        for workers in (2..=cpus).rev() {
            let base = PrivateContentPool::<ContentTask, ContentResult>::reservation_bytes(
                workers,
                workers,
                crate::commit_file::CANONICAL_BYTES,
                self.reader.clone_owned_bytes(),
                0,
            )?;
            if memory
                .checked_sub(retained)
                .and_then(|n| n.checked_sub(CONTENT_COORDINATOR_SCRATCH))
                .is_none_or(|available| base > available)
            {
                continue;
            }
            let Some(file_per_worker) = FILE_CONSTRUCTION_SHARE.checked_div(workers as u64) else {
                continue;
            };
            let Some(private) = file_per_worker.checked_sub(scratch) else {
                continue;
            };
            let private_budget =
                private.min(rope::FILE_MUTATION_BATCH_MAX_DEFERRED_BYTES as u64) as usize;
            if private_budget < 1024 {
                continue;
            }
            let reserved = base;
            self.policy.check_final_delta(
                self.policy.max_final_delta_memory_bytes - memory
                    + retained
                    + CONTENT_COORDINATOR_SCRATCH
                    + reserved,
            )?;
            let mut pool = PrivateContentPool::new(
                workers,
                workers,
                crate::commit_file::CANONICAL_BYTES,
                compile_content_task,
            )?;
            pool.set_reserved_bytes(reserved)?;
            layerfs_layerstack_store::note_workspace_content_preparation(
                0,
                0,
                0,
                0,
                0,
                reserved + FILE_CONSTRUCTION_SHARE,
            );
            return Ok(Some(ContentPool {
                pool,
                reserved,
                file_share: file_per_worker,
            }));
        }
        Ok(None)
    }

    fn drain_content_pool(
        &self,
        pool: &mut Option<ContentPool>,
        objects: &mut ObjectBuffer<'_>,
        pairs: &mut crate::commit_spool::Sorter<65>,
        handoff: &mut crate::lifecycle::HandoffWriter,
        cdc: &mut u64,
    ) -> Result<()> {
        if let Some(mut pool) = pool.take() {
            pool.pool.finish(objects, |result, objects| {
                self.finish_content_result(result, objects, pairs, handoff, cdc)
            })?;
        }
        Ok(())
    }

    fn finish_content_result(
        &self,
        result: ContentResult,
        objects: &mut ObjectBuffer<'_>,
        pairs: &mut crate::commit_spool::Sorter<65>,
        handoff: &mut crate::lifecycle::HandoffWriter,
        cdc: &mut u64,
    ) -> Result<()> {
        if result.file.len != result.header.attr.size {
            return Err(StorageError::Integrity("frozen content result length"));
        }
        *cdc = cdc
            .checked_add(result.file.counters.cdc_bytes_scanned)
            .ok_or(StorageError::Integrity("CDC counter"))?;
        let stats = result.file.preparation;
        layerfs_layerstack_store::note_workspace_content_preparation(
            stats.flushes,
            stats.flush_objects + stats.inner_flush_objects,
            stats.flush_bytes + stats.inner_flush_bytes,
            stats.private_puts,
            stats.deferred_peak_bytes + stats.inner_deferred_peak_bytes,
            0,
        );
        self.finish_content_inode(result.header, result.file.root.0, objects, pairs, handoff)
    }

    fn finish_content_inode(
        &self,
        header: ContentHeader,
        content_root: ObjectId,
        objects: &mut ObjectBuffer<'_>,
        pairs: &mut crate::commit_spool::Sorter<65>,
        handoff: &mut crate::lifecycle::HandoffWriter,
    ) -> Result<()> {
        if header.generation != self.mutation_generation || header.base_root != self.base_root {
            return Err(StorageError::Integrity("frozen content generation"));
        }
        let record = InodeRecordV1 {
            kind: match header.attr.kind {
                Kind::File => InodeKind::RegularFile,
                Kind::Directory => InodeKind::Directory,
                Kind::Symlink => InodeKind::Symlink,
            },
            content_root,
            metadata_root: header.metadata_root,
            namespace_ref_count: header.count,
        };
        layerfs_layerstack_store::note_workspace_namespace_visits(
            0,
            0,
            u64::from(header.before != Some(record)),
            u64::from(header.before == Some(record)),
            0,
        );
        if header.before != Some(record) {
            push_inode(pairs, objects, header.inode, Some(record))?;
        }
        self.push_handoff(handoff, header.node, header.inode, record, header.attr)
    }

    pub(crate) fn resolution_fingerprint(
        &mut self,
        affected_paths: &[CanonicalPath],
    ) -> Result<[u8; 32]> {
        let affected_bytes = (affected_paths.len() as u64)
            .checked_mul(std::mem::size_of::<&CanonicalPath>() as u64)
            .ok_or(StorageError::Integrity("resolution affected paths"))?;
        let retained = affected_bytes
            .checked_add(self.references.capacity())
            .ok_or(StorageError::Integrity("resolution retained allocation"))?;
        self.policy.check_final_delta(retained)?;
        let memory = self
            .policy
            .max_final_delta_memory_bytes
            .checked_sub(retained)
            .ok_or(StorageError::InvalidInput("workspace final-delta limit"))?
            / 8;
        // First sort groups only affected bindings by temporary identity; the
        // second sorts group digests. Neither temporary identity nor discovery
        // order becomes part of the logical fingerprint.
        let disk = self
            .policy
            .max_spool_bytes
            .checked_sub(self.spool_bytes)
            .and_then(|n| n.checked_sub(self.references.bytes()))
            .ok_or(StorageError::InvalidInput("workspace spool limit"))?;
        // Sorter owns one scratch-directory path; the later group sorter
        // coexists with the first run's handle. Reserve both path capacities.
        let walk_reservation = affected_bytes
            .checked_add(memory)
            .and_then(|n| n.checked_add(2 * self.spool.capacity() as u64 + 4096))
            .ok_or(StorageError::Integrity("resolution traversal reservation"))?;
        self.fingerprint_check(walk_reservation, 0)?;
        let mut aliases = crate::commit_spool::Sorter::<65>::new(&self.spool, disk / 2, memory)?;
        let mut affected = Vec::new();
        affected
            .try_reserve_exact(affected_paths.len())
            .map_err(|_| StorageError::InvalidInput("resolution affected allocation"))?;
        affected.extend(affected_paths);
        affected.sort_unstable();
        let mut digest = ContentDigestWriter::new();
        digest.write_all(b"layerfs/workspace-resolution/v5\0")?;
        for path in affected {
            digest.write_all(b"A")?;
            frame(&mut digest, path.as_bytes())?;
            let mut current = FingerprintNode::Workspace(ROOT);
            self.fingerprint_check(walk_reservation, path.as_bytes().len() as u64)?;
            let mut prefix = String::with_capacity(path.as_bytes().len());
            let path_reservation = walk_reservation + prefix.capacity() as u64;
            let mut components = path.components().peekable();
            if components.peek().is_none() {
                self.fingerprint_children(
                    current,
                    &prefix,
                    &mut digest,
                    &mut aliases,
                    path_reservation,
                )?;
            } else {
                while let Some(component) = components.next() {
                    let (base, overlay) = self.fingerprint_directory(current, path_reservation)?;
                    let child = if let Some(desired) =
                        overlay.and_then(|id| match &self.nodes[&id].data {
                            Data::Directory(dir) => dir.changes.get(component),
                            _ => None,
                        }) {
                        desired.map(FingerprintNode::Workspace)
                    } else if let Some(base) = base {
                        self.fingerprint_directory_preflight(base, path_reservation)?;
                        directory_lookup(
                            &CoreReader(&self.reader),
                            base,
                            &CanonicalName::from_bytes(component)?,
                            &mut NamespaceCounters::default(),
                        )?
                        .map(|inode| self.fingerprint_identity(inode))
                    } else {
                        None
                    };
                    let Some(child) = child else {
                        break;
                    };
                    if !prefix.is_empty() {
                        prefix.push('/');
                    }
                    prefix.push_str(
                        std::str::from_utf8(component)
                            .map_err(|_| StorageError::Integrity("resolution path"))?,
                    );
                    let directory = self.fingerprint_entry(
                        child,
                        &prefix,
                        &mut digest,
                        &mut aliases,
                        path_reservation,
                    )?;
                    if components.peek().is_none() && directory {
                        self.fingerprint_children(
                            child,
                            &prefix,
                            &mut digest,
                            &mut aliases,
                            path_reservation,
                        )?;
                    } else if !directory {
                        break;
                    }
                    current = child;
                }
            }
            digest.write_all(b"Z")?;
        }
        let mut aliases = aliases.finish()?;
        let mut groups = crate::commit_spool::Sorter::<32>::new(&self.spool, disk / 2, memory)?;
        let mut identity = None;
        let mut previous_path = None;
        let mut group = ContentDigestWriter::new();
        while let Some(record) = aliases.next()? {
            let key: [u8; 33] = record[..33].try_into().unwrap();
            let path: [u8; 32] = record[33..].try_into().unwrap();
            if identity.is_some_and(|previous| previous != key) {
                groups.push(group.finish())?;
                group = ContentDigestWriter::new();
                previous_path = None;
            }
            identity = Some(key);
            if previous_path != Some(path) {
                group.write_all(&path)?;
            }
            previous_path = Some(path);
        }
        if identity.is_some() {
            groups.push(group.finish())?;
        }
        aliases.remove()?;
        let mut groups = groups.finish()?;
        digest.write_all(b"alias-groups\0")?;
        while let Some(group) = groups.next()? {
            digest.write_all(&group)?;
        }
        groups.remove()?;
        Ok(digest.finish())
    }

    fn fingerprint_identity(&self, inode: InodeId) -> FingerprintNode {
        self.canonical_nodes
            .get(&inode)
            .copied()
            .map_or(FingerprintNode::Base(inode), FingerprintNode::Workspace)
    }

    fn fingerprint_directory(
        &self,
        node: FingerprintNode,
        owned: u64,
    ) -> Result<(Option<DirectoryStateRoot>, Option<NodeId>)> {
        match node {
            FingerprintNode::Workspace(id) => match &self.nodes[&id].data {
                Data::Directory(dir) => Ok((dir.base, Some(id))),
                _ => Err(StorageError::InvalidInput("resolution directory")),
            },
            FingerprintNode::Base(inode) => {
                let record =
                    self.base_record_with_budget(inode, self.fingerprint_remaining(owned)?)?;
                if record.kind != InodeKind::Directory {
                    return Err(StorageError::InvalidInput("resolution directory"));
                }
                Ok((Some(DirectoryStateRoot(record.content_root)), None))
            }
        }
    }

    fn fingerprint_children(
        &self,
        node: FingerprintNode,
        prefix: &str,
        digest: &mut ContentDigestWriter,
        aliases: &mut crate::commit_spool::Sorter<65>,
        stack_bytes: u64,
    ) -> Result<()> {
        self.fingerprint_walk(node, prefix, stack_bytes, &mut |node, path, owned| {
            self.fingerprint_entry(node, path, digest, aliases, owned)
        })
    }

    fn fingerprint_walk(
        &self,
        node: FingerprintNode,
        prefix: &str,
        stack_bytes: u64,
        visit: &mut impl FnMut(FingerprintNode, &str, u64) -> Result<bool>,
    ) -> Result<()> {
        let charge = stack_bytes
            .checked_add(2048)
            .ok_or(StorageError::Integrity("resolution traversal"))?;
        self.fingerprint_check(charge, 0)?;
        let (base, overlay) = self.fingerprint_directory(node, charge)?;
        let mut after: Option<CanonicalName> = None;
        loop {
            let changes = overlay.and_then(|id| match &self.nodes[&id].data {
                Data::Directory(dir) => Some(&dir.changes),
                _ => None,
            });
            let next_overlay = changes.and_then(|changes| {
                let bound = after.as_ref().map_or(std::ops::Bound::Unbounded, |name| {
                    std::ops::Bound::Excluded(name.as_bytes().to_vec())
                });
                changes
                    .range((bound, std::ops::Bound::Unbounded))
                    .find_map(|(name, child)| child.map(|child| (name, child)))
            });
            let mut base_after = after.clone();
            let next_base = if let Some(base) = base {
                loop {
                    self.fingerprint_directory_preflight(base, charge)?;
                    let page = directory_page_after(
                        &CoreReader(&self.reader),
                        base,
                        base_after.as_ref(),
                        1,
                        4096,
                        &mut NamespaceCounters::default(),
                    )?;
                    let Some((name, inode)) = page.entries.into_iter().next() else {
                        break None;
                    };
                    if changes.is_some_and(|changes| changes.contains_key(name.as_bytes())) {
                        base_after = Some(name);
                        continue;
                    }
                    break Some((name, inode));
                }
            } else {
                None
            };
            let next = match (next_overlay, next_base) {
                (Some((name, id)), Some((base_name, _)))
                    if name.as_slice() < base_name.as_bytes() =>
                {
                    Some((
                        CanonicalName::from_bytes(name)?,
                        FingerprintNode::Workspace(id),
                    ))
                }
                (_, Some((name, inode))) => Some((name, self.fingerprint_identity(inode))),
                (Some((name, id)), None) => Some((
                    CanonicalName::from_bytes(name)?,
                    FingerprintNode::Workspace(id),
                )),
                (None, None) => None,
            };
            let Some((name, child)) = next else {
                break;
            };
            let path_len = prefix
                .len()
                .checked_add(name.as_bytes().len())
                .and_then(|n| n.checked_add(usize::from(!prefix.is_empty())))
                .ok_or(StorageError::Integrity("resolution path length"))?;
            self.fingerprint_check(charge, path_len as u64)?;
            let mut path = String::with_capacity(path_len);
            path.push_str(prefix);
            if !prefix.is_empty() {
                path.push('/');
            }
            path.push_str(name.as_str());
            let retained_path = charge + path.capacity() as u64;
            if visit(child, &path, retained_path)? {
                self.fingerprint_walk(child, &path, retained_path, visit)?;
            }
            after = Some(name);
        }
        Ok(())
    }

    fn fingerprint_entry(
        &self,
        node: FingerprintNode,
        path: &str,
        digest: &mut ContentDigestWriter,
        aliases: &mut crate::commit_spool::Sorter<65>,
        owned: u64,
    ) -> Result<bool> {
        let (attr, record) = match node {
            FingerprintNode::Workspace(id) => (self.attr(id)?, None),
            FingerprintNode::Base(inode) => {
                let record =
                    self.base_record_with_budget(inode, self.fingerprint_remaining(owned)?)?;
                // Metadata lookup decodes one 8192-byte page at a time; its
                // tiny portable values use the shared rope reader. This bound
                // covers two metadata decodes, key clones, selected extents,
                // ancestor vectors and every possible decoded rope level.
                self.fingerprint_check(owned, 1024 * 1024)?;
                let portable = portable_metadata(
                    &CoreReader(&self.reader),
                    record.metadata_root,
                    record.kind,
                )?;
                let size = match record.kind {
                    InodeKind::RegularFile => {
                        self.fingerprint_check(owned, 1024)?;
                        rope::state(
                            &CoreReader(&self.reader),
                            FileStateRoot(record.content_root),
                            &mut RopeCounters::default(),
                        )?
                        .logical_len
                    }
                    InodeKind::Directory => 0,
                    InodeKind::Symlink => {
                        self.fingerprint_check(owned, 4096)?;
                        self.base_symlink(record.content_root)?.len() as u64
                    }
                };
                (
                    Attr {
                        node: NodeId(0),
                        kind: match record.kind {
                            InodeKind::RegularFile => Kind::File,
                            InodeKind::Directory => Kind::Directory,
                            InodeKind::Symlink => Kind::Symlink,
                        },
                        size,
                        mode: portable.permission_mode,
                        links: if record.kind == InodeKind::Directory {
                            2
                        } else {
                            u32::try_from(record.namespace_ref_count)
                                .map_err(|_| StorageError::Integrity("resolution links"))?
                        },
                        mtime_seconds: portable.mtime_seconds,
                        mtime_nanoseconds: portable.mtime_nanoseconds,
                    },
                    Some((inode, record)),
                )
            }
        };
        layerfs_layerstack_store::note_workspace_namespace_visits(
            u64::from(record.is_some()),
            1,
            0,
            0,
            0,
        );
        digest.write_all(b"E")?;
        frame(digest, path.as_bytes())?;
        digest.write_all(&[match attr.kind {
            Kind::File => 1,
            Kind::Directory => 2,
            Kind::Symlink => 3,
        }])?;
        digest.write_all(&attr.size.to_be_bytes())?;
        digest.write_all(&attr.mode.to_be_bytes())?;
        digest.write_all(&attr.links.to_be_bytes())?;
        digest.write_all(&attr.mtime_seconds.to_be_bytes())?;
        digest.write_all(&attr.mtime_nanoseconds.to_be_bytes())?;
        if attr.kind != Kind::Directory {
            let key = match node {
                FingerprintNode::Workspace(id) => {
                    crate::references::key(self.nodes[&id].canonical, id)
                }
                FingerprintNode::Base(inode) => crate::references::key(Some(inode), NodeId(0)),
            };
            let mut path_digest = ContentDigestWriter::new();
            frame(&mut path_digest, path.as_bytes())?;
            let mut record = [0; 65];
            record[..33].copy_from_slice(&key);
            record[33..].copy_from_slice(&path_digest.finish());
            aliases.push(record)?;
        }
        if let FingerprintNode::Workspace(id) = node {
            match attr.kind {
                Kind::File => {
                    self.fingerprint_check(owned, 8192)?;
                    self.fingerprint_workspace_file(id, digest, owned)?;
                }
                Kind::Symlink => {
                    let Data::Symlink(target) = &self.nodes[&id].data else {
                        unreachable!()
                    };
                    frame(digest, target)?;
                }
                Kind::Directory => {}
            }
        } else if let Some((_, record)) = record {
            match attr.kind {
                Kind::File => {
                    self.fingerprint_base_file(
                        FileStateRoot(record.content_root),
                        0,
                        attr.size,
                        digest,
                        owned,
                    )?;
                }
                Kind::Symlink => {
                    self.fingerprint_check(owned, 4096)?;
                    frame(digest, &self.base_symlink(record.content_root)?)?;
                }
                Kind::Directory => {}
            }
        }
        Ok(attr.kind == Kind::Directory)
    }

    fn fingerprint_remaining(&self, owned: u64) -> Result<usize> {
        self.policy
            .max_final_delta_memory_bytes
            .checked_sub(self.references.capacity())
            .and_then(|n| n.checked_sub(owned))
            .and_then(|n| usize::try_from(n).ok())
            .ok_or(StorageError::InvalidInput("workspace final-delta limit"))
    }

    fn fingerprint_check(&self, owned: u64, temporary: u64) -> Result<()> {
        let scratch = owned
            .checked_add(temporary)
            .ok_or(StorageError::Integrity("resolution scratch capacity"))?;
        self.policy.check_final_delta(
            scratch
                .checked_add(self.references.capacity())
                .ok_or(StorageError::Integrity("resolution total capacity"))?,
        )?;
        layerfs_layerstack_store::note_workspace_fingerprint_memory(
            scratch,
            self.references.capacity(),
        );
        Ok(())
    }

    fn fingerprint_directory_preflight(&self, root: DirectoryStateRoot, owned: u64) -> Result<()> {
        self.fingerprint_check(owned, 1024)?;
        let state = CoreReader(&self.reader).with_authenticated_canonical(
            root.0,
            layerfs_content::tree::directory::codec::decode_directory_state,
        )?;
        // Canonical directory pages are <=8192 bytes, each entry encodes at
        // least 35 bytes. Cursor pending siblings retain names at every level;
        // 3x frame slots covers Vec growth's old+new allocation. Four decoded
        // pages cover parent/child handoff and the leaf IntoIter collection.
        let entries = 8192_u64 / 35;
        let page = 8192 + entries * std::mem::size_of::<(CanonicalName, InodeId)>() as u64;
        let frames = (u64::from(state.tree_level) + 1) * (3 * entries * 128 + 8192);
        self.fingerprint_check(owned, frames + 4 * page + 4096)
    }

    fn fingerprint_base_file(
        &self,
        root: FileStateRoot,
        start: u64,
        len: u64,
        digest: &mut ContentDigestWriter,
        owned: u64,
    ) -> Result<()> {
        self.fingerprint_check(owned, 1024)?;
        let state = rope::state(
            &CoreReader(&self.reader),
            root,
            &mut RopeCounters::default(),
        )?;
        let end = start
            .checked_add(len)
            .filter(|end| *end <= state.logical_len)
            .ok_or(StorageError::Integrity("resolution base range"))?;
        // Valid 8192-byte mapping nodes have at most 128 descriptors. Ancestors
        // retain decoded nodes and summary vectors during descent: 16KiB per
        // level covers those vectors. Selected extents/IDs and geometric
        // ancestor growth fit the fixed 32KiB term.
        // Valid payload batches are <=4MiB (127 canonical CDC chunks), owned
        // by the existing Store ObjectSource; this is not zero-cost storage.
        let decoded = (u64::from(state.tree_level) + 1) * 16384 + 32768;
        self.fingerprint_check(owned, decoded)?;
        rope::read_range(&CoreReader(&self.reader), root, start..end, digest)?;
        Ok(())
    }

    fn fingerprint_workspace_file(
        &self,
        id: NodeId,
        digest: &mut ContentDigestWriter,
        owned: u64,
    ) -> Result<()> {
        use crate::file_edit::Piece;
        match &self.nodes[&id].data {
            Data::File(FileData::Base { root, len }) => {
                self.fingerprint_base_file(*root, 0, *len, digest, owned)
            }
            Data::File(FileData::Edited { pieces, spool, .. }) => {
                let descriptors = (pieces.count() as u64)
                    .checked_mul(std::mem::size_of::<Piece>() as u64)
                    .ok_or(StorageError::Integrity("resolution piece capacity"))?;
                self.fingerprint_check(owned, descriptors + 8192)?;
                // Piece payload Arcs and the original tree remain retained
                // Workspace state. This snapshot allocates descriptors only.
                let pieces = pieces.pieces();
                let owned =
                    owned + (pieces.capacity() * std::mem::size_of::<Piece>()) as u64 + 8192;
                let mut buffer = [0_u8; 8192];
                for piece in pieces.iter() {
                    match piece {
                        Piece::Base { root, offset, len } => {
                            self.fingerprint_base_file(*root, *offset, *len, digest, owned)?
                        }
                        Piece::Inline { bytes, offset, len } => {
                            let start = usize::try_from(*offset)
                                .map_err(|_| StorageError::Integrity("resolution inline range"))?;
                            let end = usize::try_from(
                                offset
                                    .checked_add(*len)
                                    .ok_or(StorageError::Integrity("resolution inline range"))?,
                            )
                            .map_err(|_| StorageError::Integrity("resolution inline range"))?;
                            digest.write_all(
                                bytes
                                    .get(start..end)
                                    .ok_or(StorageError::Integrity("resolution inline range"))?,
                            )?;
                        }
                        Piece::Zero { len } => {
                            buffer.fill(0);
                            let mut left = *len;
                            while left != 0 {
                                let count = left.min(buffer.len() as u64) as usize;
                                digest.write_all(&buffer[..count])?;
                                left -= count as u64;
                            }
                        }
                        Piece::Spool { offset, len } => {
                            let file = self.spool_file(id, spool)?;
                            let mut position = *offset;
                            let end = offset
                                .checked_add(*len)
                                .ok_or(StorageError::Integrity("resolution spool range"))?;
                            while position < end {
                                let count = (end - position).min(buffer.len() as u64) as usize;
                                file.read_exact_at(&mut buffer[..count], position)?;
                                digest.write_all(&buffer[..count])?;
                                position += count as u64;
                            }
                        }
                    }
                }
                Ok(())
            }
            _ => Err(StorageError::InvalidInput("resolution file")),
        }
    }

    fn base_symlink(&self, root: ObjectId) -> Result<Vec<u8>> {
        Ok(CoreReader(&self.reader)
            .with_authenticated_canonical(
                root,
                layerfs_content::tree::directory::codec::decode_symlink,
            )?
            .target)
    }

    #[cfg(test)]
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
                    let metadata = portable_metadata(
                        &reader,
                        resolved.record.metadata_root,
                        resolved.record.kind,
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
                            mode: metadata.permission_mode,
                            mtime_seconds: metadata.mtime_seconds,
                            mtime_nanoseconds: metadata.mtime_nanoseconds,
                        },
                    );
                    self.policy.check_final_delta(charge)?;
                }
                let Some(next) = page.continuation else { break };
                after = Some(next);
            }
        }
        Ok(output)
    }

    #[cfg(test)]
    fn file_matches(&self, node: NodeId, base: ObjectId) -> Result<bool> {
        match &self
            .nodes
            .get(&node)
            .ok_or(StorageError::NotFound("node"))?
            .data
        {
            Data::File(FileData::Base { root, .. }) if root.0 == base => return Ok(true),
            Data::File(FileData::Edited {
                base: Some((root, base_len)),
                pieces,
                ..
            }) if root.0 == base
                && pieces.len() == *base_len
                && matches!(pieces.pieces().as_slice(), [crate::file_edit::Piece::Base { root: piece_root, offset: 0, len }] if *piece_root == *root && *len == *base_len) =>
            {
                return Ok(true);
            }
            Data::File(_) => {}
            _ => return Err(StorageError::InvalidInput("file")),
        }
        let mut final_digest = ContentDigestWriter::new();
        let mut input = WorkspaceFileReader::new(self, node)?;
        std::io::copy(&mut input, &mut final_digest)?;
        let mut base_digest = ContentDigestWriter::new();
        rope::read_all(
            &CoreReader(&self.reader),
            FileStateRoot(base),
            &mut base_digest,
        )?;
        Ok(final_digest.finish() == base_digest.finish())
    }

    fn apply_reference_batch(
        &self,
        objects: &mut ObjectBuffer<'_>,
        pairs: &mut crate::commit_spool::Sorter<65>,
        changes: &mut Vec<(InodeId, i64, u64)>,
        maximum: usize,
    ) -> Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        let reader = CoreReader(&self.reader);
        let mut counters = InodeTableCounters::default();
        if changes.len() == 1 {
            let (inode, change, base_hint) = changes[0];
            let (record, _) = layerfs_content::tree::inode::inode_table_lookup_with_budget(
                &reader,
                self.base_inodes,
                inode,
                maximum,
                &mut counters,
            )?;
            layerfs_layerstack_store::note_workspace_base_inode_reads(counters.nodes_read, 1);
            let record = record.ok_or(StorageError::Integrity("Workspace reference base inode"))?;
            let before = reader.with_authenticated_canonical(record, decode_inode_record)?;
            if before.namespace_ref_count != base_hint {
                return Err(StorageError::Integrity("namespace reference base hint"));
            }
            let count = final_count(base_hint, change)?;
            push_inode(
                pairs,
                objects,
                inode,
                (count != 0).then_some(InodeRecordV1 {
                    namespace_ref_count: count,
                    ..before
                }),
            )?;
            changes.clear();
            return Ok(());
        }
        // Includes canonical and decoded pages, lookup pending/next arrays,
        // keys/results, ID collections and one transient codec buffer.
        if changes.len().saturating_mul(32 * 1024) > maximum {
            return Err(StorageError::InvalidInput("workspace final-delta limit"));
        }
        let keys = changes
            .iter()
            .map(|(inode, _, _)| *inode)
            .collect::<Vec<_>>();
        let records = layerfs_content::tree::inode::inode_table_lookup_many(
            &reader,
            self.base_inodes,
            &keys,
            &mut counters,
        )?;
        layerfs_layerstack_store::note_workspace_base_inode_reads(
            counters.nodes_read,
            records.len() as u64,
        );
        for ((inode, change, base_hint), record) in changes.drain(..).zip(records) {
            let record = record.ok_or(StorageError::Integrity("Workspace reference base inode"))?;
            let before = reader.with_authenticated_canonical(record, decode_inode_record)?;
            if before.namespace_ref_count != base_hint {
                return Err(StorageError::Integrity("namespace reference base hint"));
            }
            let count = final_count(base_hint, change)?;
            push_inode(
                pairs,
                objects,
                inode,
                (count != 0).then_some(InodeRecordV1 {
                    namespace_ref_count: count,
                    ..before
                }),
            )?;
        }
        Ok(())
    }

    fn base_record(&self, inode: InodeId) -> Result<InodeRecordV1> {
        let maximum = self
            .policy
            .max_final_delta_memory_bytes
            .checked_sub(self.references.capacity())
            .ok_or(StorageError::InvalidInput("workspace final-delta limit"))?;
        self.base_record_with_budget(inode, maximum as usize)
    }

    fn base_record_with_budget(&self, inode: InodeId, maximum: usize) -> Result<InodeRecordV1> {
        let core = CoreReader(&self.reader);
        let mut counters = InodeTableCounters::default();
        let (id, _) = self.reader.lookup_inode_with_budget(
            self.base_inodes,
            inode,
            maximum,
            &mut counters,
        )?;
        let id = id.ok_or(StorageError::Integrity("Workspace base inode"))?;
        layerfs_layerstack_store::note_workspace_base_inode_reads(counters.nodes_read, 1);
        Ok(core.with_authenticated_canonical(id, decode_inode_record)?)
    }

    pub(crate) fn frontier_inode(&self, node: NodeId) -> Result<InodeId> {
        let value = self
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
        let batch_allowance = (self.policy.max_final_delta_memory_bytes / 4096).min(128) * 1024;
        self.policy
            .check_final_delta(batch_allowance.saturating_add(path_charge(path)))?;
        layerfs_layerstack_store::note_workspace_namespace_visits(0, 1, 0, 0, 0);
        // Bind new identity to this base snapshot: replacing one alias must not
        // accidentally reuse the still-live inode originally allocated at its path.
        Ok(filesystem::allocated_inode(
            self.base_root.to_bytes(),
            &CanonicalPath::new(path)?,
        ))
    }

    #[cfg(test)]
    fn file_may_differ(&self, node: NodeId, base: ObjectId) -> Result<bool> {
        match &self
            .nodes
            .get(&node)
            .ok_or(StorageError::NotFound("node"))?
            .data
        {
            Data::File(FileData::Base { root, .. }) if root.0 == base => Ok(false),
            Data::File(FileData::Edited {
                base: Some((root, base_len)),
                pieces,
                ..
            }) if root.0 == base => Ok(pieces.len() != *base_len
                || !matches!(pieces.pieces().as_slice(), [crate::file_edit::Piece::Base { root: piece_root, offset: 0, len }] if *piece_root == *root && *len == *base_len)),
            Data::File(_) => Ok(!self.file_matches(node, base)?),
            _ => Err(StorageError::InvalidInput("file")),
        }
    }

    #[cfg(test)]
    fn incremental_file_supported(&self, node: NodeId, base: ObjectId) -> bool {
        matches!(
            self.nodes.get(&node).map(|node| &node.data),
            Some(Data::File(FileData::Edited {
                base: Some((root, _)),
                ..
            })) if root.0 == base
        )
    }

    #[cfg(test)]
    fn mutate_existing_file(
        &self,
        objects: &mut ObjectBuffer<'_>,
        node: NodeId,
        base: BaseEntry,
    ) -> Result<Option<(FileStateRoot, RopeCounters)>> {
        let Data::File(FileData::Edited {
            base: Some((file_root, _)),
            pieces,
            ..
        }) = &self
            .nodes
            .get(&node)
            .ok_or(StorageError::NotFound("node"))?
            .data
        else {
            return Ok(None);
        };
        if file_root.0 != base.record.content_root {
            return Ok(None);
        }
        let (file_root, pieces) = (*file_root, pieces.pieces());
        let mut batch = FileMutationBatch::new(objects, Some(file_root))?;
        let mut changed = false;
        let original_len = layerfs_content::file::rope::state(
            &CoreReader(&self.reader),
            file_root,
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
                                node,
                                file_root,
                                final_cursor,
                                final_cursor + replacement_len,
                            )?)
                    {
                        batch.replace(
                            final_cursor,
                            delete_len,
                            WorkspaceRangeReader::new(self, node, final_cursor, replacement_len)?,
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
                    node,
                    file_root,
                    final_cursor,
                    final_cursor + replacement_len,
                )?)
        {
            batch.replace(
                final_cursor,
                delete_len,
                WorkspaceRangeReader::new(self, node, final_cursor, replacement_len)?,
            )?;
            changed = true;
        }
        if batch.logical_len()? != self.attr(node)?.size {
            return Err(StorageError::Integrity("Workspace file mutation length"));
        }
        if !changed {
            return Ok(None);
        }
        Ok(Some(batch.finish()?))
    }

    #[cfg(test)]
    fn workspace_range_matches_base(
        &self,
        node: NodeId,
        base: FileStateRoot,
        start: u64,
        end: u64,
    ) -> Result<bool> {
        let mut offset = start;
        while offset < end {
            let count = (end - offset).min(64 * 1024) as usize;
            let final_bytes = self.read(node, offset, count)?;
            let mut base_bytes = Vec::with_capacity(count);
            rope::read_range(
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
}

#[cfg(any(debug_assertions, feature = "test-instrumentation"))]
thread_local! {
    static INJECT_CANDIDATE_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(any(debug_assertions, feature = "test-instrumentation"))]
pub(crate) fn inject_candidate_failure_once() {
    INJECT_CANDIDATE_FAILURE.with(|inject| inject.set(true));
}

fn final_count(base: u64, change: i64) -> Result<u64> {
    base.checked_add_signed(change)
        .ok_or(StorageError::Integrity("namespace reference overflow"))
}

fn push_inode(
    pairs: &mut crate::commit_spool::Sorter<65>,
    objects: &mut ObjectBuffer<'_>,
    inode: InodeId,
    record: Option<InodeRecordV1>,
) -> Result<()> {
    let mut pair = [0; 65];
    pair[..32].copy_from_slice(inode.as_bytes());
    if let Some(record) = record {
        pair[32] = 1;
        let id = objects.put_owned(layerfs_content::tree::inode::codec::encode_inode_record(
            record,
        )?)?;
        pair[33..].copy_from_slice(id.as_bytes());
    }
    pairs.push(pair)
}

struct CommitPhaseStart {
    wall: Instant,
    cpu: Option<u64>,
}
impl CommitPhaseStart {
    fn new() -> Self {
        Self {
            wall: Instant::now(),
            cpu: layerfs_layerstack_store::workspace_process_cpu_ns(),
        }
    }
}
fn note_commit_phase(phase: WorkspaceCommitPhase, started: CommitPhaseStart) {
    let cpu = started
        .cpu
        .zip(layerfs_layerstack_store::workspace_process_cpu_ns())
        .and_then(|(start, end)| end.checked_sub(start));
    layerfs_layerstack_store::note_workspace_commit_phase(
        phase,
        started.wall.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
    );
    layerfs_layerstack_store::note_workspace_commit_phase_cpu(phase, cpu);
}

fn frame(output: &mut impl Write, value: &[u8]) -> Result<()> {
    output.write_all(&(value.len() as u64).to_be_bytes())?;
    output.write_all(value)?;
    Ok(())
}

fn path_charge(path: &str) -> u64 {
    (path.len() as u64).saturating_mul(4).saturating_add(512)
}

#[cfg(test)]
struct WorkspaceFileReader<'a> {
    source: WorkspaceFileSource<'a>,
    offset: u64,
    len: u64,
}

#[derive(Clone, Copy)]
#[cfg(test)]
enum WorkspaceFileSource<'a> {
    Direct(&'a File),
    Mixed {
        workspace: &'a Workspace,
        node: NodeId,
    },
}

#[cfg(test)]
struct WorkspaceRangeReader<'a> {
    workspace: &'a Workspace,
    node: NodeId,
    offset: u64,
    end: u64,
}

#[cfg(test)]
impl<'a> WorkspaceRangeReader<'a> {
    fn new(workspace: &'a Workspace, node: NodeId, offset: u64, len: u64) -> Result<Self> {
        let end = offset
            .checked_add(len)
            .ok_or(StorageError::InvalidInput("file range"))?;
        if end > workspace.attr(node)?.size {
            return Err(StorageError::InvalidInput("file range"));
        }
        Ok(Self {
            workspace,
            node,
            offset,
            end,
        })
    }
}

#[cfg(test)]
impl Read for WorkspaceRangeReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.offset == self.end || output.is_empty() {
            return Ok(0);
        }
        let bytes = self
            .workspace
            .read(
                self.node,
                self.offset,
                output.len().min((self.end - self.offset) as usize),
            )
            .map_err(std::io::Error::other)?;
        output[..bytes.len()].copy_from_slice(&bytes);
        self.offset += bytes.len() as u64;
        Ok(bytes.len())
    }
}

#[cfg(test)]
impl<'a> WorkspaceFileReader<'a> {
    fn new(workspace: &'a Workspace, node: NodeId) -> Result<Self> {
        let len = workspace.attr(node)?.size;
        let source = match &workspace
            .nodes
            .get(&node)
            .ok_or(StorageError::NotFound("node"))?
            .data
        {
            Data::File(FileData::Edited {
                base: None,
                spool,
                spool_high_water,
                pieces,
                ..
            }) if *spool_high_water == len
                && pieces
                    .pieces()
                    .iter()
                    .try_fold(0_u64, |offset, piece| match piece {
                        crate::file_edit::Piece::Spool {
                            offset: source,
                            len,
                        } if *source == offset => offset.checked_add(*len),
                        _ => None,
                    })
                    == Some(len) =>
            {
                WorkspaceFileSource::Direct(workspace.spool_file(node, spool)?)
            }
            Data::File(_) => WorkspaceFileSource::Mixed { workspace, node },
            _ => return Err(StorageError::InvalidInput("file")),
        };
        Ok(Self {
            source,
            offset: 0,
            len,
        })
    }
}

#[cfg(test)]
impl Read for WorkspaceFileReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.offset == self.len || output.is_empty() {
            return Ok(0);
        }
        let count = output.len().min((self.len - self.offset) as usize);
        match self.source {
            WorkspaceFileSource::Direct(file) => {
                let mut read = 0;
                while read < count {
                    let next = file.read_at(&mut output[read..count], self.offset + read as u64)?;
                    if next == 0 {
                        return Err(std::io::ErrorKind::UnexpectedEof.into());
                    }
                    read += next;
                }
                self.offset += read as u64;
                Ok(read)
            }
            WorkspaceFileSource::Mixed { workspace, node } => {
                let bytes = workspace
                    .read(node, self.offset, count)
                    .map_err(std::io::Error::other)?;
                output[..bytes.len()].copy_from_slice(&bytes);
                self.offset += bytes.len() as u64;
                Ok(bytes.len())
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_layerstack_store::{
        CommitOutcome, EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
    };

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
    fn content_pool_survives_directory_and_larger_scratch_tasks() {
        let (root, mut workspace) = empty_workspace("pool-survives-tree");
        let first = workspace.create_file(ROOT, b"first", 0o640).unwrap().node;
        workspace.write(first, 0, b"a").unwrap();
        workspace.write(first, 0, b"b").unwrap(); // invalidate the unrelated live-capture shortcut
        let directory = workspace.mkdir(ROOT, b"middle", 0o755).unwrap().node;
        workspace.create_file(directory, b"empty", 0o640).unwrap(); // streaming scratch exceeds the first small task
        let last = workspace
            .create_file(directory, b"last", 0o640)
            .unwrap()
            .node;
        workspace.write(last, 0, b"last").unwrap();
        layerfs_layerstack_store::take_storage_receipts();
        let timer = layerfs_layerstack_store::begin_workspace_commit(
            layerfs_layerstack_store::CaptureMode::Live,
        )
        .unwrap();
        workspace.commit().unwrap();
        drop(timer);
        let receipt = layerfs_layerstack_store::take_storage_receipts()
            .into_iter()
            .find_map(|receipt| match receipt {
                layerfs_layerstack_store::StorageReceipt::WorkspaceCommit(value) => Some(value),
                _ => None,
            })
            .unwrap();
        let pool = receipt.content_pool;
        if std::thread::available_parallelism().is_ok_and(|cpus| cpus.get() >= 2) {
            assert_eq!(
                pool.pools, 1,
                "ordinary directory stages and affordable scratch changes must retain the pool"
            );
            assert_eq!(pool.submitted, 3);
            assert_eq!(pool.results_received, 3);
            assert_eq!(pool.workers_started, pool.workers_joined);
        }
        assert_eq!(workspace.read(first, 0, 8).unwrap(), b"b");
        assert_eq!(workspace.read(last, 0, 8).unwrap(), b"last");
        workspace.end_clean().unwrap();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reclaimed_zero_reference_hints_avoid_record_discovery() {
        let (root, mut workspace) = empty_workspace("zero-reference-hints");
        let directory = workspace.mkdir(ROOT, b"dir", 0o755).unwrap().node;
        for index in 0..128 {
            workspace
                .create_file(directory, format!("f{index:03}").as_bytes(), 0o640)
                .unwrap();
        }
        workspace.commit().unwrap();
        for index in 0..128 {
            workspace
                .unlink(directory, format!("f{index:03}").as_bytes(), false)
                .unwrap();
        }
        assert_eq!(workspace.nodes.len(), 2);
        layerfs_layerstack_store::take_storage_receipts();
        let timer = layerfs_layerstack_store::begin_workspace_commit(
            layerfs_layerstack_store::CaptureMode::Live,
        )
        .unwrap();
        let candidate = workspace.build_candidate().unwrap();
        drop(timer);
        let receipt = layerfs_layerstack_store::take_storage_receipts()
            .into_iter()
            .find_map(|receipt| match receipt {
                layerfs_layerstack_store::StorageReceipt::WorkspaceCommit(value) => Some(value),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            receipt.base_inode_records_read, 1,
            "only the surviving changed directory needs a base record"
        );
        let source = ObjectBuffer::resume_prevalidated(&workspace.reader, candidate.objects);
        let (page, _) = filesystem::list(
            &source,
            candidate.root_id,
            &CanonicalPath::new("dir").unwrap(),
            None,
            8,
            4096,
        )
        .unwrap();
        assert!(page.entries.is_empty());
        drop(source);
        workspace.end_clean().unwrap();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cross_run_reference_hint_mismatch_is_not_hidden_by_zero_net_delta() {
        let (root, mut workspace) = empty_workspace("reference-hint-conflict");
        let node = workspace.create_file(ROOT, b"file", 0o640).unwrap().node;
        workspace.commit().unwrap();
        let inode = workspace.nodes[&node].canonical.unwrap();
        let head = workspace
            .store
            .branch(workspace.branch_id)
            .unwrap()
            .unwrap()
            .head_commit_id;
        workspace.unlink(ROOT, b"file", false).unwrap();
        workspace
            .references
            .sorted(&workspace.spool, 1024 * 1024, 16 * 1024)
            .unwrap()
            .remove()
            .unwrap();
        workspace
            .references
            .reserve(&workspace.spool, 1024 * 1024, 16 * 1024)
            .unwrap();
        workspace.references.push(Some(inode), node, 1, 2);
        assert!(matches!(
            workspace.build_candidate(),
            Err(StorageError::Integrity("namespace reference base hint"))
        ));
        assert_eq!(
            workspace
                .store
                .branch(workspace.branch_id)
                .unwrap()
                .unwrap()
                .head_commit_id,
            head
        );
        assert!(
            workspace.references.bytes() != 0,
            "failed Commit retains its mutation facts"
        );
        workspace.end_clean().unwrap();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolution_fingerprint_is_local_and_materialization_independent() {
        let (root, mut workspace) = empty_workspace("fingerprint-local");
        let focus = workspace.mkdir(ROOT, b"focus", 0o755).unwrap().node;
        let target = workspace.create_file(focus, b"file", 0o644).unwrap().node;
        workspace.write(target, 0, b"target").unwrap();
        let other = workspace.create_file(ROOT, b"other", 0o644).unwrap().node;
        workspace.write(other, 0, b"other").unwrap();
        workspace.commit().unwrap();
        let mut fresh = Workspace::open(
            workspace.store.clone(),
            workspace.branch_id,
            root.join("fresh-spool"),
        )
        .unwrap();
        let paths = [CanonicalPath::new("focus").unwrap()];
        let before = fresh.resolution_fingerprint(&paths).unwrap();
        assert_eq!(
            fresh.nodes.len(),
            1,
            "fingerprint must not materialize a namespace manifest"
        );
        let focus = fresh.lookup_node(ROOT, b"focus").unwrap();
        let target = fresh.lookup_node(focus, b"file").unwrap();
        assert_eq!(fresh.resolution_fingerprint(&paths).unwrap(), before);
        let other = fresh.lookup_node(ROOT, b"other").unwrap();
        fresh.write(other, 0, b"unrelated").unwrap();
        assert_eq!(fresh.resolution_fingerprint(&paths).unwrap(), before);
        fresh.write(target, 0, b"changed").unwrap();
        assert_ne!(fresh.resolution_fingerprint(&paths).unwrap(), before);
        fresh.end_clean().unwrap();
        workspace.end_clean().unwrap();
        drop(fresh);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    // Test-only reuse of reference 2c30e7ef; no experiment selector or product
    // deletion path is promoted. The selected engine is the ordinary Commit.
    #[test]
    fn generic_delete_preserves_every_preexisting_sqlite_object_after_reopen() {
        let (root, mut workspace) = empty_workspace("immutable-delete");
        let gone = workspace.mkdir(ROOT, b"gone", 0o750).unwrap().node;
        for index in 0..600 {
            workspace
                .create_file(gone, format!("f{index:04}").as_bytes(), 0o640)
                .unwrap();
        }
        let keep = workspace.create_file(ROOT, b"keep", 0o640).unwrap().node;
        workspace.write(keep, 0, b"sentinel").unwrap();
        workspace.link(keep, ROOT, b"keep2").unwrap();
        workspace.link(keep, gone, b"alias").unwrap();
        let open = workspace.create_file(ROOT, b"open", 0o640).unwrap().node;
        workspace.write(open, 0, b"open").unwrap();
        workspace.commit().unwrap();
        assert_eq!(workspace.lookup(ROOT, b"keep").unwrap().links, 3);
        let old_root = workspace.base_root;
        let branch = workspace.branch_id;
        drop(workspace);
        let database = rusqlite::Connection::open_with_flags(
            root.join("store.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let before_objects: Vec<(Vec<u8>, Vec<u8>)> = {
            let mut query = database
                .prepare("SELECT object_id, bytes FROM objects")
                .unwrap();
            let rows = query
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap();
            rows.collect::<std::result::Result<_, _>>().unwrap()
        };
        drop(database);
        let mut workspace = Workspace::open(
            LayerStackStore::connect(root.join("store.sqlite")).unwrap(),
            branch,
            root.join("reopened-spool"),
        )
        .unwrap();
        let gone = workspace.lookup(ROOT, b"gone").unwrap().node;
        let open = workspace.lookup(ROOT, b"open").unwrap().node;
        workspace.pin(open, false).unwrap();
        workspace.write(open, 0, b"unlinked-edited").unwrap();
        workspace.unlink(ROOT, b"open", false).unwrap();
        for index in 0..600 {
            workspace
                .unlink(gone, format!("f{index:04}").as_bytes(), false)
                .unwrap();
        }
        workspace.unlink(gone, b"alias", false).unwrap();
        workspace.unlink(ROOT, b"gone", true).unwrap();
        workspace.commit().unwrap();
        let a = workspace.lookup(ROOT, b"keep").unwrap();
        let b = workspace.lookup(ROOT, b"keep2").unwrap();
        assert_eq!((a.node, a.links), (b.node, 2));
        let retained = filesystem::resolve(
            &CoreReader(&workspace.reader),
            workspace.base_root,
            &CanonicalPath::new("keep").unwrap(),
            &mut LogicalCounters::default(),
        )
        .unwrap();
        assert_eq!(
            retained.record.namespace_ref_count, 2,
            "outside aliases retain exact published count"
        );
        assert_eq!(
            workspace.nodes[&a.node].base_ref_count, 2,
            "installed authenticated hint is ready for the next mutation"
        );
        assert_eq!(workspace.read(a.node, 0, 8).unwrap(), b"sentinel");
        assert_eq!(workspace.read(open, 0, 32).unwrap(), b"unlinked-edited");
        assert!(
            workspace.spool_bytes > 0,
            "open-unlinked backing stays charged"
        );
        workspace.unpin(open).unwrap();
        assert!(workspace.attr(open).is_err());
        assert!(workspace.lookup(ROOT, b"gone").is_err());
        assert!(workspace.open_spools.is_empty());
        workspace.end_clean().unwrap();
        drop(workspace);
        let reopened = LayerStackStore::connect(root.join("store.sqlite")).unwrap();
        let reader = reopened.snapshot_reader(old_root);
        assert!(filesystem::resolve(
            &CoreReader(&reader),
            old_root,
            &CanonicalPath::new("gone/f0000").unwrap(),
            &mut LogicalCounters::default()
        )
        .is_ok());
        let mut bytes = Vec::new();
        filesystem::stream(
            &CoreReader(&reader),
            old_root,
            &CanonicalPath::new("open").unwrap(),
            &mut bytes,
        )
        .unwrap();
        assert_eq!(bytes, b"open");
        drop(reader);
        drop(reopened);
        let database = rusqlite::Connection::open_with_flags(
            root.join("store.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        for (id, before) in &before_objects {
            let after: Vec<u8> = database
                .query_row(
                    "SELECT bytes FROM objects WHERE object_id = ?1",
                    [id.as_slice()],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(&after, before, "delete changed an immutable CAS object");
        }
        eprintln!(
            "immutable_cas retained_unchanged_objects={}",
            before_objects.len()
        );
        drop(database);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolution_fingerprint_tracks_logical_aliases_across_capture_and_lookup() {
        let (root, mut workspace) = empty_workspace("fingerprint-capture-aliases");
        let file = workspace.create_file(ROOT, b"file", 0o640).unwrap().node;
        workspace.write(file, 0, b"same contents").unwrap();
        workspace.link(file, ROOT, b"alias").unwrap();
        let paths = [
            CanonicalPath::new("file").unwrap(),
            CanonicalPath::new("alias").unwrap(),
        ];
        let expected = workspace.resolution_fingerprint(&paths).unwrap();
        workspace.commit().unwrap();
        let mut fresh = Workspace::open(
            workspace.store.clone(),
            workspace.branch_id,
            root.join("fresh-spool"),
        )
        .unwrap();
        assert_eq!(fresh.resolution_fingerprint(&paths).unwrap(), expected);
        assert_eq!(
            fresh.nodes.len(),
            1,
            "alias discovery must not materialize the namespace"
        );
        let old = fresh.lookup_node(ROOT, b"file").unwrap();
        assert_eq!(fresh.resolution_fingerprint(&paths).unwrap(), expected);
        assert_eq!(fresh.lookup_node(ROOT, b"alias").unwrap(), old);
        assert_eq!(fresh.resolution_fingerprint(&paths).unwrap(), expected);
        // Whole-root materialization capture uses these same clear/create/link
        // mutators and is allowed to regenerate canonical inode identities.
        fresh.unlink(ROOT, b"file", false).unwrap();
        fresh.unlink(ROOT, b"alias", false).unwrap();
        let recreated = fresh.create_file(ROOT, b"file", 0o640).unwrap().node;
        assert_ne!(recreated, old);
        fresh.write(recreated, 0, b"same contents").unwrap();
        fresh.link(recreated, ROOT, b"alias").unwrap();
        fresh.create_file(ROOT, b"unrelated", 0o600).unwrap();
        assert_eq!(fresh.resolution_fingerprint(&paths).unwrap(), expected);
        fresh
            .rename(ROOT, b"alias", ROOT, b"renamed-alias", false)
            .unwrap();
        assert_ne!(
            fresh.resolution_fingerprint(&paths).unwrap(),
            expected,
            "an outside alias rename remains a relevant logical mutation"
        );
        fresh.end_clean().unwrap();
        workspace.end_clean().unwrap();
        drop(fresh);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn affected_alias_fingerprint_detects_group_changes_without_global_discovery() {
        let (root, mut workspace) = empty_workspace("fingerprint-affected-groups");
        let first = workspace.create_file(ROOT, b"a", 0o640).unwrap().node;
        let second = workspace.create_file(ROOT, b"c", 0o640).unwrap().node;
        workspace.write(first, 0, b"same").unwrap();
        workspace.write(second, 0, b"same").unwrap();
        workspace.link(first, ROOT, b"b").unwrap();
        workspace.link(second, ROOT, b"d").unwrap();
        for index in 0..1024 {
            workspace
                .create_file(ROOT, format!("unrelated-{index:04}").as_bytes(), 0o600)
                .unwrap();
        }
        let paths = ["a", "b", "c", "d"].map(|path| CanonicalPath::new(path).unwrap());
        let expected = workspace.resolution_fingerprint(&paths).unwrap();
        workspace.commit().unwrap();
        let mut fresh = Workspace::open(
            workspace.store.clone(),
            workspace.branch_id,
            root.join("fresh-spool"),
        )
        .unwrap();
        let before = fresh.reader.read_metrics_snapshot().unwrap();
        assert_eq!(fresh.resolution_fingerprint(&paths).unwrap(), expected);
        let after = fresh.reader.read_metrics_snapshot().unwrap();
        assert_eq!(fresh.nodes.len(), 1);
        assert!(
            after.snapshot_database_calls - before.snapshot_database_calls < 128,
            "affected aliases must not discover the unrelated namespace"
        );
        let first = fresh.lookup_node(ROOT, b"a").unwrap();
        let second = fresh.lookup_node(ROOT, b"c").unwrap();
        fresh.unlink(ROOT, b"b", false).unwrap();
        fresh.unlink(ROOT, b"d", false).unwrap();
        fresh.link(first, ROOT, b"d").unwrap();
        fresh.link(second, ROOT, b"b").unwrap();
        // Every path retains its bytes, metadata and global link count. Only
        // the equivalence relation changes from {a,b}/{c,d} to {a,d}/{b,c}.
        assert_ne!(fresh.resolution_fingerprint(&paths).unwrap(), expected);
        fresh.end_clean().unwrap();
        workspace.end_clean().unwrap();
        drop(fresh);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fingerprint_scratch_preflight_failure_preserves_mutations_and_retries() {
        let (root, mut workspace) = empty_workspace("fingerprint-scratch-bound");
        let file = workspace.create_file(ROOT, b"file", 0o640).unwrap().node;
        workspace
            .write(file, 0, b"content requiring the spool reader")
            .unwrap();
        let paths = [CanonicalPath::new("file").unwrap()];
        let expected = workspace.resolution_fingerprint(&paths).unwrap();
        let generation = workspace.mutation_generation;
        let nodes = workspace.nodes.len();
        let limit = workspace.policy.max_final_delta_memory_bytes;
        workspace.policy.max_final_delta_memory_bytes = 8192;
        assert!(matches!(
            workspace.resolution_fingerprint(&paths),
            Err(StorageError::InvalidInput("workspace final-delta limit"))
        ));
        assert_eq!(workspace.mutation_generation, generation);
        assert_eq!(workspace.nodes.len(), nodes);
        workspace.policy.max_final_delta_memory_bytes = limit;
        assert_eq!(workspace.resolution_fingerprint(&paths).unwrap(), expected);
        let remaining = std::fs::read_dir(&workspace.spool)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            remaining,
            [std::ffi::OsString::from(file.0.to_string())],
            "fingerprint leaves no private named sorter files"
        );
        workspace.end_clean().unwrap();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sparse_metadata_commit_reuses_fully_materialized_large_tree() {
        let mut previous: Option<(u64, u64)> = None;
        for count in [128, 4096] {
            let (root, mut workspace) = empty_workspace(&format!("materialized-sparse-{count}"));
            for index in 0..count {
                let node = workspace
                    .create_file(ROOT, format!("f{index:05}").as_bytes(), 0o640)
                    .unwrap()
                    .node;
                workspace.write(node, 0, b"unchanged payload").unwrap();
            }
            workspace.commit().unwrap();
            assert_eq!(workspace.nodes.len(), count + 1);
            let inode_table_before = workspace.base_inodes;
            let directory_before = match &workspace.nodes[&ROOT].data {
                Data::Directory(dir) => dir.base,
                _ => unreachable!(),
            };
            let edited = workspace.lookup_node(ROOT, b"f00000").unwrap();
            workspace.chmod(edited, 0o600).unwrap();
            let reads_before = workspace.reader.read_metrics_snapshot().unwrap();
            layerfs_layerstack_store::take_storage_receipts();
            let diagnostics =
                layerfs_layerstack_store::capture_workspace_commit_diagnostics().unwrap();
            {
                let _timer = layerfs_layerstack_store::begin_workspace_commit(
                    layerfs_layerstack_store::CaptureMode::Live,
                )
                .unwrap();
                assert!(matches!(
                    workspace.commit().unwrap().0,
                    CommitOutcome::Committed { .. }
                ));
            }
            let reads_after = workspace.reader.read_metrics_snapshot().unwrap();
            let samples = layerfs_layerstack_store::take_workspace_commit_diagnostics();
            assert_eq!(samples.len(), 1);
            let sample = samples[0];
            assert_eq!(sample.handoff_records, 1);
            assert_eq!(sample.handoff_inode_pages_read, 0);
            assert_eq!(sample.namespace_candidate_probe_nodes, 1);
            assert_eq!(
                sample.commit_node_visits, 3,
                "count, sorted input, and installation each visit only the changed inode"
            );
            assert_eq!(
                sample.commit_tracking_live_field_bytes,
                16 * (count as u64 + 1)
            );
            assert_eq!(sample.cdc_bytes_scanned, 0);
            assert_eq!(
                reads_after.payload_bytes_read,
                reads_before.payload_bytes_read
            );
            let receipt = layerfs_layerstack_store::take_storage_receipts()
                .into_iter()
                .find_map(|receipt| match receipt {
                    layerfs_layerstack_store::StorageReceipt::WorkspaceCommit(receipt) => {
                        Some(receipt)
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(receipt.reference_events, 0);
            assert!(receipt.generic_tree_reads > 0 && receipt.generic_tree_reads < 32);
            assert!(receipt.generic_tree_emissions > 0 && receipt.generic_tree_emissions < 8);
            // Forwarded immutable children need not visit the writer's persist
            // method, so prove reuse with IDs instead of assuming that counter.
            let core = CoreReader(&workspace.reader);
            let before = core
                .with_authenticated_canonical(
                    inode_table_before.0,
                    layerfs_content::tree::inode::codec::decode_inode_table_node,
                )
                .unwrap();
            let after = core
                .with_authenticated_canonical(
                    workspace.base_inodes.0,
                    layerfs_content::tree::inode::codec::decode_inode_table_node,
                )
                .unwrap();
            if let layerfs_content::tree::inode::codec::InodeTableNodeV1::Branch {
                children: before,
                ..
            } = before
            {
                let layerfs_content::tree::inode::codec::InodeTableNodeV1::Branch {
                    children: after,
                    ..
                } = after
                else {
                    panic!("metadata edit cannot collapse an unchanged key set");
                };
                assert!(
                    before
                        .iter()
                        .any(|(_, id)| after.iter().any(|(_, after)| after == id)),
                    "untouched inode children must keep their object IDs"
                );
            }
            if let Some((reads, emissions)) = previous {
                assert!(
                    receipt.generic_tree_reads <= reads + 4,
                    "structural reads grow with tree height, not materialized file count"
                );
                assert!(receipt.generic_tree_emissions <= emissions + 4);
            }
            previous = Some((receipt.generic_tree_reads, receipt.generic_tree_emissions));
            let directory_after = match &workspace.nodes[&ROOT].data {
                Data::Directory(dir) => dir.base,
                _ => unreachable!(),
            };
            assert_eq!(directory_after, directory_before);
            assert_eq!(workspace.attr(edited).unwrap().mode, 0o600);
            eprintln!(
                "sparse_materialized files={count} tree_reads={} emissions={} reused={} handoff={}",
                receipt.generic_tree_reads,
                receipt.generic_tree_emissions,
                receipt.generic_tree_reused_children,
                sample.handoff_records
            );
            drop(diagnostics);
            workspace.end_clean().unwrap();
            drop(workspace);
            std::fs::remove_dir_all(root).unwrap();
        }
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
        workspace.unlink(ROOT, b"added", false).unwrap();
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
        assert!(inode_table_lookup(
            &core,
            table,
            old_tree.inode,
            &mut InodeTableCounters::default()
        )
        .unwrap()
        .is_none());
        let entries = layerfs_content::tree::inode::inode_table_entries(
            &core,
            table,
            &mut InodeTableCounters::default(),
        )
        .unwrap();
        // root + background directory/files + kept directory/file + outside +
        // hidden + replacement + added-alias + 120 new files; no orphan inodes.
        assert_eq!(entries.len(), 328);
        // A valid long path still must fit the transient planner allocation,
        // together with its pending inode batch, under a custom small policy.
        workspace.policy.max_final_delta_memory_bytes = 4096;
        let name = "x".repeat(250);
        let mut parent = ROOT;
        for _ in 0..4 {
            parent = workspace
                .mkdir(parent, name.as_bytes(), 0o700)
                .unwrap()
                .node;
        }
        let path = workspace.nodes[&parent].paths.first().unwrap();
        assert!(CanonicalPath::new(path).is_ok());
        assert!(matches!(
            workspace.frontier_inode(parent),
            Err(StorageError::InvalidInput("workspace final-delta limit"))
        ));
        drop(workspace);
        drop(store);
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
        assert_eq!(transition, crate::lifecycle::CommitTransition::Rebased);
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
        assert!(matches!(reader.source, WorkspaceFileSource::Direct(_)));
        let mut actual = Vec::new();
        reader.read_to_end(&mut actual).unwrap();
        assert_eq!(actual, data);

        let sparse = workspace.create_file(ROOT, b"sparse", 0o600).unwrap();
        workspace.write(sparse.node, 4096, b"x").unwrap();
        assert!(matches!(
            WorkspaceFileReader::new(&workspace, sparse.node)
                .unwrap()
                .source,
            WorkspaceFileSource::Mixed { .. }
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
        let captured_root = match &workspace.capture {
            crate::capture::CaptureState::Ready(captured) => captured.root,
            _ => panic!("sequential capture did not finish"),
        };

        let store = workspace.store.clone();
        let branch = store.branch(workspace.branch_id).unwrap().unwrap();
        let built = workspace.build_candidate().unwrap();
        let outcome = store
            .commit_candidate(&branch, workspace.base_root, workspace.expected_base, built)
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
        let built = workspace.build_candidate().unwrap();
        let outcome = store
            .commit_candidate(&branch, workspace.base_root, workspace.expected_base, built)
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
            let original_inode = workspace.nodes[&file].canonical.unwrap();
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
            let built = workspace.build_candidate().unwrap();
            assert_eq!(built.counters.cdc_bytes_scanned, expected_cdc, "{case}");
            assert!(built.objects.encoded_bytes() < 256 * 1024, "{case}");
            if case == "noop" {
                assert_eq!(built.root_id, workspace.base_root);
                assert!(built.objects.is_empty());
            }
            let candidate_root = built.root_id;
            let record = store.branch(branch).unwrap().unwrap();
            let outcome = store
                .commit_candidate(&record, workspace.base_root, workspace.expected_base, built)
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
        let base_root = match workspace.nodes[&file].data {
            Data::File(FileData::Base { root, .. }) => root,
            _ => panic!("base file"),
        };
        let mut base_payloads = BTreeSet::new();
        rope::visit_extents(&CoreReader(&workspace.reader), base_root, |extents| {
            base_payloads.extend(extents.iter().map(|extent| extent.payload_object_id));
            Ok(())
        })
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
        let built = workspace.build_candidate().unwrap();
        let reads_after = workspace.reader.read_metrics_snapshot().unwrap();
        assert_eq!(built.counters.cdc_bytes_scanned, 10);
        assert_eq!(
            reads_after.payload_bytes_read - reads_before.payload_bytes_read,
            0
        );
        let record = store.branch(branch).unwrap().unwrap();
        let outcome = store
            .commit_candidate(&record, workspace.base_root, workspace.expected_base, built)
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
            FileStateRoot(resolved.record.content_root),
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
}
