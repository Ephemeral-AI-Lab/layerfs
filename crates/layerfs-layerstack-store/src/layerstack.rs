use crate::ids::TypedId;
use crate::objects::{
    admit_initialization_objects, empty_root, insert_initialization_object_batch,
    insert_initialization_segment_batch, BuiltRoot, DeferredObjectStore,
    InitializationDirectAdmissionWriter, InitializationObjectSlab, InitializationSegmentAdmission,
    InitializationSlabQueueMetrics, InitializationSlabWriter, InitializationSlabWriterMetrics,
    InitializationSqlPhase, InitializationTaskObjectBuffer, ObjectBuffer,
    INITIALIZATION_SLAB_QUEUE_SLOTS,
};
#[cfg(test)]
use crate::objects::{
    AppendOnlyInitializationSegment, AppendOnlyInitializationWriter, InitializationTaskBlock,
};
use crate::records::decode_object_id;
use crate::{
    AddLayerResult, BranchId, CommitId, EntityName, InitializeLayerStackResult, LayerId,
    LayerRecord, LayerStackId, LayerStackInitialization, LayerStackRecord, LayerStackStore, Result,
    StoreError,
};
use layerfs_content::object::access::ObjectStore;
use rusqlite::TransactionBehavior;

impl LayerStackStore {
    pub fn initialize_layerstack(
        &self,
        name: EntityName,
        source: LayerStackInitialization,
    ) -> Result<InitializeLayerStackResult> {
        let _operation = self.db.enter_operation()?;
        let mut initialization_diagnostic = InitializationDiagnostic::from_env();
        let mut cleanup_failed_empty_initialization = false;
        let layer_stack_id = LayerStackId::new();
        let seed = initialization_seed(&layer_stack_id, initialization_diagnostic.is_some())?;
        let (
            root_id,
            scanned_files,
            scanned_bytes,
            final_batch,
            mut candidate_receipt,
            mut statement_number,
            direct_segments,
            mut fast_diagnostics,
        ) = match source {
            LayerStackInitialization::Empty => {
                let built = empty_root(seed)?;
                let (final_batch, receipt, statement_number) =
                    plan_single_initialization(&self.db, &built.objects)?;
                (
                    built.root_id,
                    0,
                    0,
                    final_batch,
                    receipt,
                    statement_number,
                    false,
                    None,
                )
            }
            LayerStackInitialization::Directory(path) => {
                if !path.is_dir() {
                    return Err(StoreError::InvalidInput("Layer initialization directory"));
                }
                let direct_segments = self.db.initialization_store_is_empty()?;
                cleanup_failed_empty_initialization = direct_segments;
                if direct_segments {
                    let prepare_started = initialization_diagnostic
                        .as_ref()
                        .map(|_| std::time::Instant::now());
                    let prepared = direct_initialize_root_directories(&self.db, &path, seed)?;
                    if let (Some(diagnostic), Some(started)) =
                        (initialization_diagnostic.as_mut(), prepare_started)
                    {
                        diagnostic.prepare_import_wall_ns =
                            started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                    }
                    match prepared {
                        Some(finished) => (
                            finished.root_id,
                            finished.scanned_files,
                            finished.scanned_bytes,
                            finished.final_batch,
                            finished.receipt,
                            finished.statement_number,
                            true,
                            Some(finished.diagnostics),
                        ),
                        None => {
                            let (built, scanned_files, scanned_bytes) =
                                directory_root(&path, seed)?;
                            let (final_batch, receipt, statement_number) =
                                plan_single_initialization(&self.db, &built.objects)?;
                            (
                                built.root_id,
                                scanned_files,
                                scanned_bytes,
                                final_batch,
                                receipt,
                                statement_number,
                                false,
                                None,
                            )
                        }
                    }
                } else {
                    let (built, scanned_files, scanned_bytes) = directory_root(&path, seed)?;
                    let (final_batch, receipt, statement_number) =
                        plan_single_initialization(&self.db, &built.objects)?;
                    (
                        built.root_id,
                        scanned_files,
                        scanned_bytes,
                        final_batch,
                        receipt,
                        statement_number,
                        false,
                        None,
                    )
                }
            }
        };
        let layer = LayerRecord {
            id: LayerId::derive(layer_stack_id, None, root_id),
            layer_stack_id,
            parent_layer_id: None,
            root_id,
            source_branch_id: None,
            source_commit_id: None,
        };
        let stack = LayerStackRecord {
            id: layer_stack_id,
            name: name.clone(),
            head_layer_id: layer.id,
        };
        let final_begin_started = fast_diagnostics.as_ref().map(|_| std::time::Instant::now());
        let publication = (|| -> Result<_> {
            let mut connection = self.db.writer()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let final_begin_ns = final_begin_started
                .map(|started| started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64)
                .unwrap_or(0);
            let final_metrics = if direct_segments {
                insert_initialization_segment_batch(
                    &transaction,
                    &final_batch,
                    &mut statement_number,
                )?
            } else {
                insert_initialization_object_batch(
                    &transaction,
                    &final_batch,
                    &mut statement_number,
                )?
            };
            #[cfg(debug_assertions)]
            crate::schema::fail_transaction_statement(u64::MAX)?;
            statement_number += 1;
            crate::schema::fail_transaction_statement(statement_number)?;
            transaction.execute(
                crate::statements::layerstack::INSERT_LAYER,
                rusqlite::params![
                    layer.id.as_slice(),
                    layer.layer_stack_id.as_slice(),
                    Option::<&[u8]>::None,
                    layer.root_id.as_bytes().as_slice(),
                    Option::<&[u8]>::None,
                    Option::<&[u8]>::None,
                ],
            )?;
            statement_number += 1;
            crate::schema::fail_transaction_statement(statement_number)?;
            if let Err(error) = transaction.execute(
                crate::statements::layerstack::INSERT,
                rusqlite::params![
                    stack.id.as_slice(),
                    stack.name.as_str(),
                    stack.head_layer_id.as_slice()
                ],
            ) {
                drop(transaction);
                drop(connection);
                if let Some(existing) = self.layer_stack_by_name(&name)? {
                    return Err(StoreError::LayerStackNameConflict {
                        name,
                        existing_id: existing.id,
                        incoming_id: layer_stack_id,
                    });
                }
                return Err(error.into());
            }
            let final_commit_started = fast_diagnostics.as_ref().map(|_| std::time::Instant::now());
            transaction.commit()?;
            let final_commit_ns = final_commit_started
                .map(|started| started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64)
                .unwrap_or(0);
            Ok((final_metrics, final_begin_ns, final_commit_ns))
        })();
        let (final_metrics, final_begin_ns, final_commit_ns) = match publication {
            Ok(publication) => publication,
            Err(error) => {
                if cleanup_failed_empty_initialization {
                    self.db.clear_failed_direct_initialization()?;
                }
                return Err(error);
            }
        };
        if let Some(diagnostics) = fast_diagnostics.as_mut() {
            diagnostics.admission.record_sql_batch(
                final_metrics,
                final_begin_ns,
                final_commit_ns,
                InitializationSqlPhase::Publication,
            );
        }
        if direct_segments {
            candidate_receipt.candidate_objects += final_metrics.objects;
            candidate_receipt.candidate_bytes = candidate_receipt
                .candidate_bytes
                .saturating_add(final_metrics.bytes);
            candidate_receipt.inserted_objects += final_metrics.objects;
            candidate_receipt.inserted_bytes = candidate_receipt
                .inserted_bytes
                .saturating_add(final_metrics.bytes);
            candidate_receipt.max_transaction_objects = candidate_receipt
                .max_transaction_objects
                .max(final_metrics.objects);
            candidate_receipt.max_transaction_bytes = candidate_receipt
                .max_transaction_bytes
                .max(final_metrics.bytes);
        }
        candidate_receipt.final_inserted_objects = final_metrics.objects;
        candidate_receipt.final_inserted_bytes = final_metrics.bytes;
        crate::telemetry::record_initialization_candidate(candidate_receipt)?;
        crate::telemetry::record_layerstack_initialization(
            crate::LayerStackInitializationReceipt {
                layer_stack_id,
                scanned_files,
                scanned_bytes,
            },
        );
        if let Some(mut diagnostic) = initialization_diagnostic {
            diagnostic.fast = fast_diagnostics;
            diagnostic.emit();
        }
        Ok(InitializeLayerStackResult {
            layer_stack_id,
            genesis_layer_id: layer.id,
        })
    }

    #[doc(hidden)]
    pub fn take_layerstack_initialization_receipts(
        &self,
    ) -> Vec<crate::LayerStackInitializationReceipt> {
        crate::telemetry::take_layerstack_initialization_receipts()
    }

    pub fn add_layer(&self, branch_id: BranchId) -> Result<AddLayerResult> {
        let _operation = self.db.enter_operation()?;
        let snapshot = self.load_add_snapshot(branch_id)?;
        if let Some(layer_id) = snapshot.existing_layer_id {
            return Ok(AddLayerResult::UpToDate { layer_id });
        }
        if snapshot.commit_base_layer_id != snapshot.branch_base_layer_id
            || snapshot.layer_stack_head_id != snapshot.branch_base_layer_id
        {
            return Ok(AddLayerResult::HeadMoved {
                expected: snapshot.branch_base_layer_id,
                actual: snapshot.layer_stack_head_id,
            });
        }
        if snapshot.commit_root_id == snapshot.base_root_id {
            return Ok(AddLayerResult::NoChanges {
                head_layer_id: snapshot.branch_base_layer_id,
            });
        }
        let layer = LayerRecord {
            id: LayerId::derive(
                snapshot.layer_stack_id,
                Some(snapshot.branch_base_layer_id),
                snapshot.commit_root_id,
            ),
            layer_stack_id: snapshot.layer_stack_id,
            parent_layer_id: Some(snapshot.branch_base_layer_id),
            root_id: snapshot.commit_root_id,
            source_branch_id: Some(branch_id),
            source_commit_id: Some(snapshot.head_commit_id),
        };
        let mut connection = self.db.writer()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::fail_transaction_statement(1)?;
        transaction.execute(
            crate::statements::layerstack::INSERT_LAYER,
            rusqlite::params![
                layer.id.as_slice(),
                layer.layer_stack_id.as_slice(),
                layer.parent_layer_id.map(|id| id.to_bytes().to_vec()),
                layer.root_id.as_bytes().as_slice(),
                layer.source_branch_id.map(|id| id.to_bytes().to_vec()),
                layer.source_commit_id.map(|id| id.to_bytes().to_vec()),
            ],
        )?;
        crate::schema::fail_transaction_statement(2)?;
        if transaction.execute(
            crate::statements::layerstack::ADVANCE_HEAD,
            rusqlite::params![
                snapshot.layer_stack_id.as_slice(),
                layer.id.as_slice(),
                snapshot.branch_base_layer_id.as_slice(),
            ],
        )? == 0
        {
            let actual = transaction.query_row(
                crate::statements::layerstack::CURRENT_HEAD,
                [snapshot.layer_stack_id.as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )?;
            let actual = LayerId::from_slice(&actual)?;
            drop(transaction);
            return Ok(AddLayerResult::HeadMoved {
                expected: snapshot.branch_base_layer_id,
                actual,
            });
        }
        transaction.commit()?;
        Ok(AddLayerResult::Added { layer_id: layer.id })
    }

    fn load_add_snapshot(&self, branch_id: BranchId) -> Result<AddSnapshot> {
        self.db
            .reader()?
            .query_row(
                crate::statements::layerstack::LOAD_ADD_SNAPSHOT,
                [branch_id.as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, Vec<u8>>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                        row.get::<_, Vec<u8>>(6)?,
                        row.get::<_, Vec<u8>>(7)?,
                        row.get::<_, Vec<u8>>(8)?,
                        row.get::<_, Option<Vec<u8>>>(9)?,
                    ))
                },
            )
            .map_err(StoreError::from)
            .and_then(
                |(stack, base, head, root, commit_base, base_root, stack_head, existing)| {
                    Ok(AddSnapshot {
                        layer_stack_id: LayerStackId::from_slice(&stack)?,
                        branch_base_layer_id: LayerId::from_slice(&base)?,
                        head_commit_id: CommitId::from_slice(&head)?,
                        commit_root_id: decode_object_id(root)?,
                        commit_base_layer_id: LayerId::from_slice(&commit_base)?,
                        base_root_id: decode_object_id(base_root)?,
                        layer_stack_head_id: LayerId::from_slice(&stack_head)?,
                        existing_layer_id: existing
                            .map(|bytes| LayerId::from_slice(&bytes))
                            .transpose()?,
                    })
                },
            )
            .map_err(|error| match error {
                StoreError::Database(_) => StoreError::NotFound("Branch Commit"),
                error => error,
            })
    }
}

struct AddSnapshot {
    layer_stack_id: LayerStackId,
    branch_base_layer_id: LayerId,
    head_commit_id: CommitId,
    commit_root_id: layerfs_content::ObjectId,
    commit_base_layer_id: LayerId,
    base_root_id: layerfs_content::ObjectId,
    layer_stack_head_id: LayerId,
    existing_layer_id: Option<LayerId>,
}

fn plan_single_initialization(
    db: &crate::schema::StoreDb,
    objects: &DeferredObjectStore,
) -> Result<(Vec<crate::CanonicalObject>, crate::CandidateReceipt, u64)> {
    let plan = db.plan_initialization_candidate(objects)?;
    let mut statement_number = 0;
    let admission = admit_initialization_objects(db, objects, &plan, &mut statement_number)?;
    Ok((
        admission.final_batch,
        crate::CandidateReceipt {
            candidate_objects: plan.candidate_objects,
            candidate_bytes: plan.candidate_bytes,
            inserted_objects: plan.inserted_objects,
            inserted_bytes: plan.inserted_bytes,
            reused_objects: plan.reused_objects,
            reused_bytes: plan.reused_bytes,
            batch_inserted_objects: admission.batch_inserted_objects,
            batch_inserted_bytes: admission.batch_inserted_bytes,
            final_inserted_objects: 0,
            final_inserted_bytes: 0,
            preexisting_reused_objects: plan.reused_objects,
            preexisting_reused_bytes: plan.reused_bytes,
            admission_transactions: admission.transactions,
            max_transaction_objects: admission.max_transaction_objects,
            max_transaction_bytes: admission.max_transaction_bytes,
        },
        statement_number,
    ))
}

fn directory_root(path: &std::path::Path, seed: [u8; 32]) -> Result<(BuiltRoot, u64, u64)> {
    match prepare_parallel_root_directories(path, seed)? {
        Some(prepared) => finish_parallel_candidate(prepared, seed),
        None => serial_directory_root(path, seed),
    }
}

fn finish_parallel_candidate(
    mut prepared: PreparedParallelRoot,
    seed: [u8; 32],
) -> Result<(BuiltRoot, u64, u64)> {
    let mut objects = ObjectBuffer::empty_all_reachable()?;
    for segment in prepared.segments.drain(..) {
        objects.merge_prevalidated(segment)?;
    }
    let imported = finish_parallel_root(prepared, seed, &mut objects)?;
    let root = layerfs_content::filesystem::build_initial_namespace(
        &mut objects,
        seed,
        imported.mutations,
    )?;
    Ok((
        objects.finish_all_reachable(root, imported.scanned_bytes)?,
        imported.scanned_files,
        imported.scanned_bytes,
    ))
}

fn serial_directory_root(path: &std::path::Path, seed: [u8; 32]) -> Result<(BuiltRoot, u64, u64)> {
    let mut objects = ObjectBuffer::empty_all_reachable()?;
    let mut import = NativeImport::new(seed, &mut objects);
    import.directory(path, &layerfs_content::CanonicalPath::root(), true)?;
    let imported = import.finish()?;
    let root = layerfs_content::filesystem::build_initial_namespace(
        &mut objects,
        seed,
        imported.mutations,
    )?;
    Ok((
        objects.finish_all_reachable(root, imported.scanned_bytes)?,
        imported.scanned_files,
        imported.scanned_bytes,
    ))
}

type ImportedRecord = (
    layerfs_content::tree::inode::InodeId,
    layerfs_content::tree::inode::InodeRecordV1,
);

struct ImportedTree {
    mutations: Vec<layerfs_content::filesystem::InodeMutation>,
    hard_links: Vec<(u64, u64)>,
    scanned_files: u64,
    scanned_bytes: u64,
    source: SourceImportMetrics,
}

struct CompactImportedTree {
    hard_links: Vec<(u64, u64)>,
    scanned_files: u64,
    scanned_bytes: u64,
    source: SourceImportMetrics,
    #[cfg(test)]
    record_len: usize,
    #[cfg(test)]
    record_capacity: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SourceImportMetrics {
    file_open_calls: u64,
    file_read_calls: u64,
    file_read_bytes: u64,
    symlink_metadata_calls: u64,
    read_dir_calls: u64,
    single_chunk_files: u64,
    streaming_files: u64,
    cdc_scratch_peak_bytes: u64,
    metadata_cache_hits: u64,
    metadata_cache_misses: u64,
    metadata_cache_peak_entries: u64,
}

impl SourceImportMetrics {
    fn merge(&mut self, other: Self) {
        self.file_open_calls = self.file_open_calls.saturating_add(other.file_open_calls);
        self.file_read_calls = self.file_read_calls.saturating_add(other.file_read_calls);
        self.file_read_bytes = self.file_read_bytes.saturating_add(other.file_read_bytes);
        self.symlink_metadata_calls = self
            .symlink_metadata_calls
            .saturating_add(other.symlink_metadata_calls);
        self.read_dir_calls = self.read_dir_calls.saturating_add(other.read_dir_calls);
        self.single_chunk_files = self
            .single_chunk_files
            .saturating_add(other.single_chunk_files);
        self.streaming_files = self.streaming_files.saturating_add(other.streaming_files);
        self.cdc_scratch_peak_bytes = self
            .cdc_scratch_peak_bytes
            .max(other.cdc_scratch_peak_bytes);
        self.metadata_cache_hits = self
            .metadata_cache_hits
            .saturating_add(other.metadata_cache_hits);
        self.metadata_cache_misses = self
            .metadata_cache_misses
            .saturating_add(other.metadata_cache_misses);
        self.metadata_cache_peak_entries = self
            .metadata_cache_peak_entries
            .max(other.metadata_cache_peak_entries);
    }
}

struct CountedSourceReader {
    file: std::fs::File,
    calls: u64,
    bytes: u64,
}

#[derive(Default)]
struct FastInitializationDiagnostics {
    worker_count: u64,
    source: SourceImportMetrics,
    object_io: crate::objects::InitializationSegmentIoMetrics,
    pair_io: crate::objects::InitializationSegmentIoMetrics,
    admission: crate::objects::InitializationAdmissionDiagnostics,
    final_root_inode_table_wall_ns: u64,
    insert_node_peak_len: u64,
    insert_node_peak_capacity: u64,
    slab: InitializationSlabWriterMetrics,
    queue_peak: u64,
    queue_peak_bytes: u64,
    consumer_idle_ns: u64,
    last_slab_receive_offset_ns: u64,
    pipeline_wall_ns: u64,
    active_thread_peak: u64,
    active_producers_after: u64,
    task_state_bytes: u64,
    completed_result_peak_bytes: u64,
    parent_final_state_peak_bytes: u64,
    producers: Vec<ProducerDiagnostic>,
}

struct ProducerDiagnostic {
    index: usize,
    metrics: InitializationSlabWriterMetrics,
}

struct FinishedAppendOnlyInitialization {
    root_id: layerfs_content::ObjectId,
    scanned_files: u64,
    scanned_bytes: u64,
    final_batch: Vec<crate::CanonicalObject>,
    receipt: crate::CandidateReceipt,
    statement_number: u64,
    diagnostics: FastInitializationDiagnostics,
}

struct InitializationDiagnostic {
    nonce: String,
    prepare_import_wall_ns: u64,
    fast: Option<FastInitializationDiagnostics>,
}

fn decode_initialization_seed(value: &str) -> Result<[u8; 32]> {
    fn nibble(value: u8) -> Option<u8> {
        match value {
            b'0'..=b'9' => Some(value - b'0'),
            b'a'..=b'f' => Some(value - b'a' + 10),
            _ => None,
        }
    }
    if value.len() != 64 {
        return Err(StoreError::InvalidInput("benchmark initialization seed"));
    }
    let mut seed = [0_u8; 32];
    for (output, pair) in seed.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        *output = nibble(pair[0])
            .zip(nibble(pair[1]))
            .map(|(left, right)| left << 4 | right)
            .ok_or(StoreError::InvalidInput("benchmark initialization seed"))?;
    }
    Ok(seed)
}

fn initialization_seed(layer_stack_id: &LayerStackId, diagnostic: bool) -> Result<[u8; 32]> {
    let Some(value) = std::env::var_os("LAYERFS_BENCH_INITIALIZATION_SEED_HEX") else {
        return Ok(*blake3::hash(layer_stack_id.as_slice()).as_bytes());
    };
    decode_initialization_seed(
        value
            .to_str()
            .filter(|_| diagnostic)
            .ok_or(StoreError::InvalidInput("benchmark initialization seed"))?,
    )
}

impl std::io::Read for CountedSourceReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.calls = self.calls.saturating_add(1);
        let bytes = std::io::Read::read(&mut self.file, buffer)?;
        self.bytes = self.bytes.saturating_add(bytes as u64);
        Ok(bytes)
    }
}

impl InitializationDiagnostic {
    fn from_env() -> Option<Self> {
        let nonce = std::env::var("LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE").ok()?;
        if nonce.is_empty() || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self {
            nonce,
            prepare_import_wall_ns: 0,
            fast: None,
        })
    }

    fn emit(self) {
        let fast_path = u8::from(self.fast.is_some());
        let parent_merge_bytes = if fast_path == 1 { "0" } else { "na" };
        let fast = self.fast.unwrap_or_default();
        let admission = fast.admission;
        let cdc_peak = fast
            .source
            .cdc_scratch_peak_bytes
            .saturating_mul(fast.worker_count);
        let explicit_buffer_peak_bytes = if fast.slab.handoffs == 0 {
            let prepare_peak = (INITIALIZATION_APPEND_PENDING_BYTES
                + INITIALIZATION_PAIR_PENDING_BYTES) as u64
                + cdc_peak;
            let admission_peak = admission
                .batch_peak_payload_bytes
                .saturating_add(
                    admission
                        .batch_peak_vec_capacity
                        .saturating_mul(std::mem::size_of::<crate::CanonicalObject>() as u64),
                )
                .saturating_add(admission.pending_index_peak_bytes)
                .saturating_add(INITIALIZATION_APPEND_PENDING_BYTES as u64);
            let final_peak = fast
                .insert_node_peak_capacity
                .saturating_add(INITIALIZATION_FINAL_PENDING_BYTES as u64)
                .saturating_add(INITIALIZATION_PAIR_PENDING_BYTES as u64);
            prepare_peak.max(admission_peak).max(final_peak)
        } else {
            let slab_headers = fast
                .worker_count
                .saturating_add(fast.queue_peak)
                .saturating_add(1)
                .saturating_mul(crate::objects::INITIALIZATION_SLAB_OBJECTS as u64)
                .saturating_mul(std::mem::size_of::<crate::CanonicalObject>() as u64);
            let pipeline_peak = fast
                .worker_count
                .saturating_mul(crate::objects::INITIALIZATION_SLAB_BYTES as u64)
                .saturating_add(fast.queue_peak_bytes)
                .saturating_add(crate::objects::INITIALIZATION_SLAB_BYTES as u64)
                .saturating_add(INITIALIZATION_PAIR_PENDING_BYTES as u64)
                .saturating_add(cdc_peak)
                .saturating_add(fast.slab.structural_peak_bytes)
                .saturating_add(fast.task_state_bytes)
                .saturating_add(slab_headers)
                .saturating_add(admission.batch_peak_payload_bytes)
                .saturating_add(
                    admission
                        .batch_peak_vec_capacity
                        .saturating_mul(std::mem::size_of::<crate::CanonicalObject>() as u64),
                )
                .saturating_add(admission.pending_index_peak_bytes);
            let completed_peak = admission
                .batch_peak_payload_bytes
                .saturating_add(
                    admission
                        .batch_peak_vec_capacity
                        .saturating_mul(std::mem::size_of::<crate::CanonicalObject>() as u64),
                )
                .saturating_add(admission.pending_index_peak_bytes)
                .saturating_add(fast.task_state_bytes)
                .saturating_add(fast.completed_result_peak_bytes)
                .saturating_add(INITIALIZATION_PAIR_PENDING_BYTES as u64);
            let final_peak = admission
                .final_simultaneous_owned_peak_bytes
                .saturating_add(INITIALIZATION_PAIR_PENDING_BYTES as u64);
            pipeline_peak.max(completed_peak).max(final_peak)
        };
        eprintln!(
            "layerfs-initialization-diagnostic-v3 nonce={} fast_path={} worker_count={} prepare_import_wall_ns={} source_file_open_calls={} source_file_read_calls={} source_file_read_bytes={} source_symlink_metadata_calls={} source_read_dir_calls={} single_chunk_files={} streaming_files={} cdc_scratch_peak_bytes={} metadata_cache_hits={} metadata_cache_misses={} metadata_cache_peak_entries={} explicit_buffer_peak_bytes={} explicit_slab_payload_limit_bytes={} explicit_slab_object_limit={} explicit_canonical_object_header_bytes={} explicit_pair_pending_limit_bytes={} canonical_frame_count={} canonical_payload_bytes={} canonical_payload_capacity_bytes={} canonical_payload_capacity_slack_bytes={} canonical_encode_calls={} canonical_hash_calls={} canonical_framing_bytes={} object_segment_write_calls={} object_segment_write_bytes={} object_segment_raw_read_calls={} object_segment_raw_read_bytes={} object_segment_passes={} slab_handoffs={} slab_sent_objects={} slab_sent_bytes={} slab_send_blocked_ns={} slab_partial_peak_objects={} slab_partial_peak_payload_bytes={} slab_queue_peak={} slab_queue_peak_bytes={} slab_consumer_idle_ns={} last_slab_receive_offset_ns={} direct_pipeline_wall_ns={} import_pipeline_thread_peak={} active_producers_after={} task_state_bytes={} completed_result_peak_bytes={} parent_final_state_peak_bytes={} candidate_copy_bytes={} structural_peak_bytes={} parent_payload_copy_bytes={} pair_segment_write_calls={} pair_segment_write_bytes={} pair_segment_raw_read_calls={} pair_segment_raw_read_bytes={} pair_segment_passes={} parent_merge_bytes={} pending_duplicate_objects={} pending_duplicate_bytes={} cross_batch_skipped_objects={} cross_batch_skipped_bytes={} collision_checks={} admission_batch_peak_objects={} admission_batch_peak_payload_bytes={} admission_batch_peak_vec_capacity={} pending_index_peak_entries={} pending_index_peak_bytes={} final_batch_peak_payload_bytes={} final_batch_peak_vec_capacity={} final_pending_index_peak_bytes={} final_simultaneous_owned_peak_bytes={} sql_batch_count={} sql_row_count_shape_count={} sql_submitted_rows={} sql_returned_ids={} sql_skipped_ids={} sql_string_build_ns={} sql_prepare_ns={} sql_bind_step_returning_ns={} conflict_read_calls={} conflict_read_rows={} conflict_read_bytes={} conflict_read_ns={} sql_begin_ns={} sql_commit_ns={} final_root_inode_table_wall_ns={} insert_node_peak_len={} insert_node_peak_capacity={}",
            self.nonce,
            fast_path,
            fast.worker_count,
            self.prepare_import_wall_ns,
            fast.source.file_open_calls,
            fast.source.file_read_calls,
            fast.source.file_read_bytes,
            fast.source.symlink_metadata_calls,
            fast.source.read_dir_calls,
            fast.source.single_chunk_files,
            fast.source.streaming_files,
            fast.source
                .cdc_scratch_peak_bytes
                .saturating_mul(fast.worker_count),
            fast.source.metadata_cache_hits,
            fast.source.metadata_cache_misses,
            fast.source.metadata_cache_peak_entries,
            explicit_buffer_peak_bytes,
            crate::objects::INITIALIZATION_SLAB_BYTES,
            crate::objects::INITIALIZATION_SLAB_OBJECTS,
            std::mem::size_of::<crate::CanonicalObject>(),
            INITIALIZATION_PAIR_PENDING_BYTES,
            fast.object_io.frames,
            fast.object_io.payload_bytes,
            fast.slab.payload_capacity_bytes,
            fast.slab
                .payload_capacity_bytes
                .saturating_sub(fast.slab.payload_bytes),
            fast.slab.objects,
            fast.slab.canonical_hash_calls,
            fast.object_io.framing_bytes,
            fast.object_io.write_calls,
            fast.object_io.write_bytes,
            fast.object_io.raw_read_calls,
            fast.object_io.raw_read_bytes,
            fast.object_io.passes,
            fast.slab.handoffs,
            fast.slab.objects,
            fast.slab.payload_bytes,
            fast.slab.blocked_ns,
            fast.slab.partial_peak_objects,
            fast.slab.partial_peak_payload_bytes,
            fast.queue_peak,
            fast.queue_peak_bytes,
            fast.consumer_idle_ns,
            fast.last_slab_receive_offset_ns,
            fast.pipeline_wall_ns,
            fast.active_thread_peak,
            fast.active_producers_after,
            fast.task_state_bytes,
            fast.completed_result_peak_bytes,
            fast.parent_final_state_peak_bytes,
            fast.slab.candidate_copy_bytes,
            fast.slab.structural_peak_bytes,
            fast.slab.parent_payload_copy_bytes,
            fast.pair_io.write_calls,
            fast.pair_io.write_bytes,
            fast.pair_io.raw_read_calls,
            fast.pair_io.raw_read_bytes,
            fast.pair_io.passes,
            parent_merge_bytes,
            admission.pending_duplicate_objects,
            admission.pending_duplicate_bytes,
            admission.cross_batch_skipped_objects,
            admission.cross_batch_skipped_bytes,
            admission.collision_checks,
            admission.batch_peak_objects,
            admission.batch_peak_payload_bytes,
            admission.batch_peak_vec_capacity,
            admission.pending_index_peak_entries,
            admission.pending_index_peak_bytes,
            admission.final_batch_peak_payload_bytes,
            admission.final_batch_peak_vec_capacity,
            admission.final_pending_index_peak_bytes,
            admission.final_simultaneous_owned_peak_bytes,
            admission.sql_batch_count,
            admission.sql_row_shapes.len(),
            admission.sql_submitted_rows,
            admission.sql_returned_ids,
            admission.sql_skipped_ids,
            admission.sql_string_build_ns,
            admission.sql_prepare_ns,
            admission.sql_bind_step_returning_ns,
            admission.conflict_read_calls,
            admission.conflict_read_rows,
            admission.conflict_read_bytes,
            admission.conflict_read_ns,
            admission.sql_begin_ns,
            admission.sql_commit_ns,
            fast.final_root_inode_table_wall_ns,
            fast.insert_node_peak_len,
            fast.insert_node_peak_capacity,
        );
        eprintln!(
            "layerfs-initialization-commits-v1 nonce={} pipeline_count={} pipeline_ns={} pipeline_max_ns={} pipeline_max_ordinal={} final_build_count={} final_build_ns={} final_build_max_ns={} final_build_max_ordinal={} publication_ns={} publication_ordinal={} total_count={} total_ns={}",
            self.nonce,
            admission.pipeline_commit_count,
            admission.pipeline_commit_ns,
            admission.pipeline_commit_max_ns,
            admission.pipeline_commit_max_ordinal,
            admission.final_build_commit_count,
            admission.final_build_commit_ns,
            admission.final_build_commit_max_ns,
            admission.final_build_commit_max_ordinal,
            admission.publication_commit_ns,
            admission.sql_batch_count,
            admission.sql_batch_count,
            admission.sql_commit_ns,
        );
        for producer in fast.producers {
            eprintln!(
                "layerfs-initialization-producer-v1 nonce={} producer={} wall_ns={} blocked_ns={} tasks={} files={} bytes={} completion_offset_ns={}",
                self.nonce,
                producer.index,
                producer.metrics.producer_wall_ns,
                producer.metrics.blocked_ns,
                producer.metrics.producer_tasks,
                producer.metrics.producer_files,
                producer.metrics.producer_bytes,
                producer.metrics.producer_completion_offset_ns,
            );
        }
    }
}

struct PreparedDirectory {
    index: usize,
    name: layerfs_content::CanonicalName,
    inode: layerfs_content::tree::inode::InodeId,
    imported: ImportedTree,
}

struct PreparedCompactDirectory {
    index: usize,
    name: layerfs_content::CanonicalName,
    inode: layerfs_content::tree::inode::InodeId,
    imported: CompactImportedTree,
}

struct PreparedWorker {
    directories: Vec<PreparedDirectory>,
    objects: DeferredObjectStore,
}

struct PreparedParallelRoot {
    metadata: std::fs::Metadata,
    directories: Vec<PreparedDirectory>,
    segments: Vec<DeferredObjectStore>,
}

#[cfg(test)]
struct PreparedAppendOnlyWorker {
    directories: Vec<PreparedCompactDirectory>,
    blocks: Vec<InitializationTaskBlock>,
    segment: AppendOnlyInitializationSegment,
    pair_blocks: Vec<crate::objects::CompactInodePairBlock>,
    pairs: crate::objects::CompactInodePairSegment,
}

struct PreparedDirectWorker {
    index: usize,
    directories: Vec<PreparedCompactDirectory>,
    pair_blocks: Vec<crate::objects::CompactInodePairBlock>,
    pairs: crate::objects::CompactInodePairSegment,
    slab: InitializationSlabWriterMetrics,
}

#[cfg(test)]
struct PreparedAppendOnlyRoot {
    directories: Vec<PreparedCompactDirectory>,
    segments: Vec<AppendOnlyInitializationSegment>,
    pair_blocks: Vec<crate::objects::CompactInodePairBlock>,
    pairs: Vec<crate::objects::CompactInodePairSegment>,
}

const INITIALIZATION_APPEND_PENDING_BYTES: usize = 1024 * 1024;
const INITIALIZATION_PAIR_PENDING_BYTES: usize = 256 * 1024;
const INITIALIZATION_FINAL_PENDING_BYTES: usize = 64 * 1024;
const INITIALIZATION_TASK_BLOCK_LIMIT: usize = 1_000;

struct RootDirectoryTask {
    name: layerfs_content::CanonicalName,
    logical: layerfs_content::CanonicalPath,
    native: std::path::PathBuf,
}

fn direct_initialize_root_directories(
    db: &crate::schema::StoreDb,
    native: &std::path::Path,
    seed: [u8; 32],
) -> Result<Option<FinishedAppendOnlyInitialization>> {
    let result = direct_initialize_root_directories_inner(db, native, seed);
    if result.is_err() {
        db.clear_failed_direct_initialization()?;
    }
    result
}

fn direct_initialize_root_directories_inner(
    db: &crate::schema::StoreDb,
    native: &std::path::Path,
    seed: [u8; 32],
) -> Result<Option<FinishedAppendOnlyInitialization>> {
    use std::os::unix::ffi::OsStrExt;

    let parent_payload_copies = crate::objects::ParentPayloadCopyCounter::start();
    let metadata = std::fs::symlink_metadata(native)?;
    let mut entries = std::fs::read_dir(native)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by(|left, right| {
        left.file_name()
            .as_bytes()
            .cmp(right.file_name().as_bytes())
    });
    if entries.is_empty() {
        return Ok(None);
    }
    let mut tasks = Vec::with_capacity(entries.len());
    for entry in entries {
        if !entry.file_type()?.is_dir() {
            return Ok(None);
        }
        let name = layerfs_content::CanonicalName::from_bytes(entry.file_name().as_bytes())?;
        tasks.push(RootDirectoryTask {
            logical: child(&layerfs_content::CanonicalPath::root(), &name)?,
            name,
            native: entry.path(),
        });
    }
    if tasks.len() > INITIALIZATION_TASK_BLOCK_LIMIT {
        return Ok(None);
    }
    let task_state_bytes = (tasks.capacity() * std::mem::size_of::<RootDirectoryTask>()) as u64
        + tasks
            .iter()
            .map(|task| {
                task.name.owned_capacity_bytes() as u64
                    + task.logical.owned_capacity_bytes() as u64
                    + task.native.as_os_str().as_bytes().len() as u64
            })
            .sum::<u64>();
    let workers = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
        .min(8)
        .min(tasks.len());
    let queue = std::sync::Arc::new(InitializationSlabQueueMetrics::default());
    let (sender, receiver) =
        std::sync::mpsc::sync_channel::<InitializationObjectSlab>(INITIALIZATION_SLAB_QUEUE_SLOTS);
    let pair_pending_bytes = INITIALIZATION_PAIR_PENDING_BYTES.div_ceil(workers).max(64);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let fallback = std::sync::atomic::AtomicBool::new(false);
    let active_producers = std::sync::atomic::AtomicU64::new(0);
    let active_producer_peak = std::sync::atomic::AtomicU64::new(0);
    let mut admission = InitializationSegmentAdmission::new(db)?;
    let pipeline_started = std::time::Instant::now();
    let (prepared, consumer_idle_ns, last_slab_receive_offset_ns) =
        std::thread::scope(|scope| -> Result<(Vec<PreparedDirectWorker>, u64, u64)> {
            let handles = (0..workers)
                .map(|worker_index| {
                    let next = &next;
                    let fallback = &fallback;
                    let active_producers = &active_producers;
                    let active_producer_peak = &active_producer_peak;
                    let tasks = &tasks;
                    let sender = sender.clone();
                    let queue = queue.clone();
                    scope.spawn(move || {
                        let active =
                            active_producers.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
                        active_producer_peak
                            .fetch_max(active, std::sync::atomic::Ordering::Relaxed);
                        let producer_started = std::time::Instant::now();
                        let result = (|| {
                            let mut directories = Vec::new();
                            let mut pair_blocks = Vec::new();
                            let mut objects = InitializationSlabWriter::new(sender, queue);
                            let mut pairs =
                                crate::objects::CompactInodePairWriter::new(pair_pending_bytes)?;
                            let mut structural_peak_bytes = 0_u64;
                            loop {
                                if fallback.load(std::sync::atomic::Ordering::Acquire) {
                                    break;
                                }
                                let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                let Some(task) = tasks.get(index) else {
                                    break;
                                };
                                let pair_checkpoint = pairs.checkpoint();
                                let mut structure = InitializationTaskObjectBuffer::new();
                                let mut import =
                                    NativeImport::new_split(seed, &mut objects, &mut structure);
                                let inode =
                                    match import.directory(&task.native, &task.logical, false) {
                                        Ok(inode) => inode,
                                        Err(StoreError::Core(
                                            layerfs_content::CoreError::ObjectLimitExceeded,
                                        )) => {
                                            fallback
                                                .store(true, std::sync::atomic::Ordering::Release);
                                            break;
                                        }
                                        Err(error) => return Err(error),
                                    };
                                let imported = match import.finish_compact(&mut pairs) {
                                    Ok(imported) => imported,
                                    Err(StoreError::Core(
                                        layerfs_content::CoreError::ObjectLimitExceeded,
                                    )) => {
                                        fallback.store(true, std::sync::atomic::Ordering::Release);
                                        break;
                                    }
                                    Err(error) => return Err(error),
                                };
                                if imported.hard_links.is_empty() {
                                    structural_peak_bytes =
                                        structural_peak_bytes.max(structure.explicit_owned_bytes());
                                    objects.note_hash_invocations(structure.hash_invocations());
                                    structure.move_into(&mut objects)?;
                                } else {
                                    fallback.store(true, std::sync::atomic::Ordering::Release);
                                }
                                pair_blocks.push(pairs.block_since(
                                    index,
                                    worker_index,
                                    pair_checkpoint,
                                )?);
                                directories.push(PreparedCompactDirectory {
                                    index,
                                    name: task.name.clone(),
                                    inode,
                                    imported,
                                });
                            }
                            let mut slab = objects.finish()?;
                            slab.structural_peak_bytes = structural_peak_bytes;
                            slab.producer_wall_ns = producer_started
                                .elapsed()
                                .as_nanos()
                                .min(u128::from(u64::MAX))
                                as u64;
                            slab.producer_completion_offset_ns = pipeline_started
                                .elapsed()
                                .as_nanos()
                                .min(u128::from(u64::MAX))
                                as u64;
                            slab.producer_tasks = directories.len() as u64;
                            slab.producer_files = directories
                                .iter()
                                .map(|directory| directory.imported.scanned_files)
                                .sum();
                            slab.producer_bytes = directories
                                .iter()
                                .map(|directory| directory.imported.scanned_bytes)
                                .sum();
                            Ok::<_, StoreError>((
                                worker_index,
                                directories,
                                pair_blocks,
                                pairs,
                                slab,
                            ))
                        })();
                        active_producers.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
                        result
                    })
                })
                .collect::<Vec<_>>();
            drop(sender);

            let mut consumer_idle_ns = 0_u64;
            let mut last_slab_receive_offset_ns = 0_u64;
            let mut admission_error = None;
            while let Ok(slab) = {
                let started = std::time::Instant::now();
                let received = receiver.recv();
                consumer_idle_ns = consumer_idle_ns
                    .saturating_add(started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64);
                received
            } {
                last_slab_receive_offset_ns = pipeline_started
                    .elapsed()
                    .as_nanos()
                    .min(u128::from(u64::MAX)) as u64;
                queue.received(slab.payload_bytes);
                if admission_error.is_none() {
                    if let Err(error) = admission.admit_page(slab.objects) {
                        admission_error = Some(error);
                    }
                }
            }

            let mut output = Vec::with_capacity(workers);
            let mut worker_error = None;
            for handle in handles {
                match handle.join() {
                    Ok(Ok((index, directories, pair_blocks, pairs, slab))) => {
                        output.push(PreparedDirectWorker {
                            index,
                            directories,
                            pair_blocks,
                            pairs: pairs.seal()?,
                            slab,
                        });
                    }
                    Ok(Err(error)) => {
                        worker_error.get_or_insert(error);
                    }
                    Err(_) => {
                        worker_error
                            .get_or_insert(StoreError::Integrity("Layer initialization worker"));
                    }
                };
            }
            if let Some(error) = admission_error.or(worker_error) {
                return Err(error);
            }
            Ok((output, consumer_idle_ns, last_slab_receive_offset_ns))
        })?;
    let pipeline_wall_ns = pipeline_started
        .elapsed()
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64;

    let completed_result_peak_bytes = (prepared.capacity()
        * std::mem::size_of::<PreparedDirectWorker>()) as u64
        + prepared
            .iter()
            .map(|worker| {
                (worker.directories.capacity() * std::mem::size_of::<PreparedCompactDirectory>())
                    as u64
                    + worker
                        .directories
                        .iter()
                        .map(|directory| {
                            directory.name.owned_capacity_bytes() as u64
                                + (directory.imported.hard_links.capacity()
                                    * std::mem::size_of::<(u64, u64)>())
                                    as u64
                        })
                        .sum::<u64>()
                    + (worker.pair_blocks.capacity()
                        * std::mem::size_of::<crate::objects::CompactInodePairBlock>())
                        as u64
            })
            .sum::<u64>()
        + (tasks.len() * std::mem::size_of::<PreparedCompactDirectory>()) as u64
        + (tasks.len() * std::mem::size_of::<crate::objects::CompactInodePairBlock>()) as u64
        + (prepared.len() * std::mem::size_of::<crate::objects::CompactInodePairSegment>()) as u64;

    if fallback.load(std::sync::atomic::Ordering::Acquire) {
        drop(admission);
        db.clear_failed_direct_initialization()?;
        return Ok(None);
    }

    let mut identities = std::collections::HashSet::new();
    if prepared
        .iter()
        .flat_map(|worker| worker.directories.iter())
        .flat_map(|directory| directory.imported.hard_links.iter())
        .any(|identity| !identities.insert(*identity))
    {
        drop(admission);
        db.clear_failed_direct_initialization()?;
        return Ok(None);
    }

    let mut directories = Vec::with_capacity(tasks.len());
    let mut pair_blocks = Vec::with_capacity(tasks.len());
    let mut pairs = Vec::with_capacity(prepared.len());
    let mut producers = Vec::with_capacity(prepared.len());
    let mut slab = InitializationSlabWriterMetrics::default();
    for worker in prepared {
        producers.push(ProducerDiagnostic {
            index: worker.index,
            metrics: worker.slab,
        });
        directories.extend(worker.directories);
        pair_blocks.extend(worker.pair_blocks);
        pairs.push(worker.pairs);
        slab.handoffs = slab.handoffs.saturating_add(worker.slab.handoffs);
        slab.objects = slab.objects.saturating_add(worker.slab.objects);
        slab.payload_bytes = slab.payload_bytes.saturating_add(worker.slab.payload_bytes);
        slab.payload_capacity_bytes = slab
            .payload_capacity_bytes
            .saturating_add(worker.slab.payload_capacity_bytes);
        slab.canonical_hash_calls = slab
            .canonical_hash_calls
            .saturating_add(worker.slab.canonical_hash_calls);
        slab.blocked_ns = slab.blocked_ns.saturating_add(worker.slab.blocked_ns);
        slab.partial_peak_objects = slab
            .partial_peak_objects
            .max(worker.slab.partial_peak_objects);
        slab.partial_peak_payload_bytes = slab
            .partial_peak_payload_bytes
            .max(worker.slab.partial_peak_payload_bytes);
        slab.candidate_copy_bytes = slab
            .candidate_copy_bytes
            .saturating_add(worker.slab.candidate_copy_bytes);
        slab.structural_peak_bytes = slab
            .structural_peak_bytes
            .saturating_add(worker.slab.structural_peak_bytes);
    }
    producers.sort_by_key(|producer| producer.index);
    directories.sort_by_key(|directory| directory.index);
    pair_blocks.sort_by_key(|block| block.task_ordinal);
    if directories.len() != tasks.len()
        || pair_blocks.len() != tasks.len()
        || directories
            .iter()
            .zip(&pair_blocks)
            .any(|(directory, block)| directory.index != block.task_ordinal)
    {
        return Err(StoreError::Integrity("direct initialization task order"));
    }

    admission.prepare_final_phase()?;
    let final_started = std::time::Instant::now();
    let mut children = Vec::with_capacity(directories.len());
    let mut scanned_files = 0_u64;
    let mut scanned_bytes = 0_u64;
    let mut source = SourceImportMetrics {
        symlink_metadata_calls: 1,
        read_dir_calls: 1,
        ..SourceImportMetrics::default()
    };
    for directory in directories {
        children.push((directory.name, directory.inode));
        scanned_files = scanned_files
            .checked_add(directory.imported.scanned_files)
            .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
        scanned_bytes = scanned_bytes
            .checked_add(directory.imported.scanned_bytes)
            .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
        source.merge(directory.imported.source);
    }
    let parent_final_state_peak_bytes = (children.capacity()
        * std::mem::size_of::<(
            layerfs_content::CanonicalName,
            layerfs_content::tree::inode::InodeId,
        )>()) as u64
        + children
            .iter()
            .map(|(name, _)| name.owned_capacity_bytes() as u64)
            .sum::<u64>();
    let mut final_objects = InitializationDirectAdmissionWriter::new(&mut admission);
    final_objects.note_transient_owned_bytes(parent_final_state_peak_bytes)?;
    let content =
        match layerfs_content::filesystem::build_initial_directory(&mut final_objects, children) {
            Ok(content) => content,
            Err(error) => return Err(final_objects.error(error)),
        };
    final_objects.note_transient_owned_bytes(0)?;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata_root = match layerfs_content::filesystem::build_portable_metadata(
        &mut final_objects,
        layerfs_content::tree::inode::InodeKind::Directory,
        metadata.permissions().mode(),
        metadata.mtime(),
        metadata.mtime_nsec() as u32,
    ) {
        Ok(root) => root,
        Err(error) => return Err(final_objects.error(error)),
    };
    let root_inode = layerfs_content::tree::inode::InodeId::allocate(seed, 0);
    let root_record =
        match final_objects.put_owned(layerfs_content::tree::inode::codec::encode_inode_record(
            layerfs_content::tree::inode::InodeRecordV1 {
                kind: layerfs_content::tree::inode::InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: content.0,
                metadata_root,
            },
        )?) {
            Ok(root) => root,
            Err(error) => return Err(final_objects.error(error)),
        };
    let mut pair_stream = crate::objects::CompactInodePairStream::new(pairs, pair_blocks)?;
    let (inode_table, insert_node_peak_len, insert_node_peak_capacity) =
        match layerfs_content::tree::inode::build_initial_inode_table_from_pairs(
            &mut final_objects,
            root_inode,
            std::iter::once(Ok((root_inode, root_record))).chain(&mut pair_stream),
        ) {
            Ok(table) => table,
            Err(error) => return Err(final_objects.error(error)),
        };
    let pair_io = pair_stream.finish()?;
    let root_id = match final_objects.put_owned(
        layerfs_content::tree::directory::codec::encode_namespace_root(
            layerfs_content::tree::NamespaceRootV1 {
                profile_id: layerfs_content::tree::directory::codec::profile_id(),
                root_directory_inode: root_inode,
                inode_table_root: inode_table.0,
            },
        )?,
    ) {
        Ok(root) => root,
        Err(error) => return Err(final_objects.error(error)),
    };
    slab.objects = slab.objects.saturating_add(final_objects.metrics.objects);
    slab.payload_bytes = slab
        .payload_bytes
        .saturating_add(final_objects.metrics.payload_bytes);
    slab.payload_capacity_bytes = slab
        .payload_capacity_bytes
        .saturating_add(final_objects.metrics.payload_capacity_bytes);
    slab.canonical_hash_calls = slab
        .canonical_hash_calls
        .saturating_add(final_objects.metrics.canonical_hash_calls);
    slab.candidate_copy_bytes = slab
        .candidate_copy_bytes
        .saturating_add(final_objects.metrics.candidate_copy_bytes);
    slab.parent_payload_copy_bytes = parent_payload_copies.bytes();
    drop(final_objects);
    let final_root_inode_table_wall_ns =
        final_started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
    let queue_peak = queue.peak();
    let queue_peak_bytes = queue.peak_bytes();
    let admission = admission.finish()?;
    Ok(Some(FinishedAppendOnlyInitialization {
        root_id,
        scanned_files,
        scanned_bytes,
        final_batch: admission.final_batch,
        receipt: admission.receipt,
        statement_number: admission.statement_number,
        diagnostics: FastInitializationDiagnostics {
            worker_count: workers as u64,
            source,
            object_io: crate::objects::InitializationSegmentIoMetrics {
                frames: slab.objects,
                payload_bytes: slab.payload_bytes,
                ..crate::objects::InitializationSegmentIoMetrics::default()
            },
            pair_io,
            admission: admission.diagnostics,
            final_root_inode_table_wall_ns,
            insert_node_peak_len,
            insert_node_peak_capacity,
            slab,
            queue_peak,
            queue_peak_bytes,
            consumer_idle_ns,
            last_slab_receive_offset_ns,
            pipeline_wall_ns,
            active_thread_peak: active_producer_peak
                .load(std::sync::atomic::Ordering::Relaxed)
                .saturating_add(1),
            active_producers_after: active_producers.load(std::sync::atomic::Ordering::Acquire),
            task_state_bytes,
            completed_result_peak_bytes,
            parent_final_state_peak_bytes,
            producers,
        },
    }))
}

#[cfg(test)]
fn prepare_append_only_root_directories(
    native: &std::path::Path,
    seed: [u8; 32],
) -> Result<Option<PreparedAppendOnlyRoot>> {
    use std::os::unix::ffi::OsStrExt;

    let mut entries = std::fs::read_dir(native)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by(|left, right| {
        left.file_name()
            .as_bytes()
            .cmp(right.file_name().as_bytes())
    });
    if entries.len() < 2 {
        return Ok(None);
    }
    let mut tasks = Vec::with_capacity(entries.len());
    for entry in entries {
        if !entry.file_type()?.is_dir() {
            return Ok(None);
        }
        let name = layerfs_content::CanonicalName::from_bytes(entry.file_name().as_bytes())?;
        tasks.push(RootDirectoryTask {
            logical: child(&layerfs_content::CanonicalPath::root(), &name)?,
            name,
            native: entry.path(),
        });
    }
    let workers = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
        .min(10)
        .min(tasks.len());
    if workers < 2 {
        return Ok(None);
    }
    let pending_bytes = INITIALIZATION_APPEND_PENDING_BYTES.div_ceil(workers);
    let pair_pending_bytes = INITIALIZATION_PAIR_PENDING_BYTES.div_ceil(workers).max(64);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let prepared = std::thread::scope(|scope| -> Result<Vec<PreparedAppendOnlyWorker>> {
        let handles = (0..workers)
            .map(|worker_index| {
                let next = &next;
                let tasks = &tasks;
                scope.spawn(move || {
                    let mut directories = Vec::new();
                    let mut blocks = Vec::new();
                    let mut pair_blocks = Vec::new();
                    let mut segment = AppendOnlyInitializationWriter::new(pending_bytes)?;
                    let mut pairs =
                        crate::objects::CompactInodePairWriter::new(pair_pending_bytes)?;
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(task) = tasks.get(index) else {
                            break;
                        };
                        let checkpoint = segment.checkpoint();
                        let pair_checkpoint = pairs.checkpoint();
                        let mut import = NativeImport::new(seed, &mut segment);
                        let inode = import.directory(&task.native, &task.logical, false)?;
                        let imported = import.finish_compact(&mut pairs)?;
                        blocks.push(segment.block_since(index, worker_index, checkpoint)?);
                        pair_blocks.push(pairs.block_since(
                            index,
                            worker_index,
                            pair_checkpoint,
                        )?);
                        directories.push(PreparedCompactDirectory {
                            index,
                            name: task.name.clone(),
                            inode,
                            imported,
                        });
                    }
                    if segment.get_calls() != 0 {
                        return Err(StoreError::Integrity("append-only initialization get"));
                    }
                    Ok::<_, StoreError>(PreparedAppendOnlyWorker {
                        directories,
                        blocks,
                        segment: segment.seal()?,
                        pair_blocks,
                        pairs: pairs.seal()?,
                    })
                })
            })
            .collect::<Vec<_>>();
        let mut output = Vec::with_capacity(workers);
        for handle in handles {
            output.push(
                handle
                    .join()
                    .map_err(|_| StoreError::Integrity("Layer initialization worker"))??,
            );
        }
        Ok(output)
    })?;

    let mut identities = std::collections::HashSet::new();
    if prepared
        .iter()
        .flat_map(|worker| worker.directories.iter())
        .flat_map(|directory| directory.imported.hard_links.iter())
        .any(|identity| !identities.insert(*identity))
    {
        return Ok(None);
    }

    let mut directories = Vec::with_capacity(tasks.len());
    let mut blocks = Vec::with_capacity(tasks.len());
    let mut segments = Vec::with_capacity(prepared.len());
    let mut pair_blocks = Vec::with_capacity(tasks.len());
    let mut pairs = Vec::with_capacity(prepared.len());
    for worker in prepared {
        directories.extend(worker.directories);
        blocks.extend(worker.blocks);
        segments.push(worker.segment);
        pair_blocks.extend(worker.pair_blocks);
        pairs.push(worker.pairs);
    }
    directories.sort_by_key(|directory| directory.index);
    blocks.sort_by_key(|block| block.task_ordinal);
    pair_blocks.sort_by_key(|block| block.task_ordinal);
    if directories.len() != tasks.len()
        || blocks.len() != tasks.len()
        || pair_blocks.len() != tasks.len()
        || blocks.len() > INITIALIZATION_TASK_BLOCK_LIMIT
        || directories
            .iter()
            .zip(&blocks)
            .any(|(directory, block)| directory.index != block.task_ordinal)
        || blocks.iter().zip(&pair_blocks).any(|(objects, pairs)| {
            objects.task_ordinal != pairs.task_ordinal || objects.worker_index != pairs.worker_index
        })
    {
        return Err(StoreError::Integrity(
            "append-only initialization task order",
        ));
    }
    Ok(Some(PreparedAppendOnlyRoot {
        directories,
        segments,
        pair_blocks,
        pairs,
    }))
}

fn prepare_parallel_root_directories(
    native: &std::path::Path,
    seed: [u8; 32],
) -> Result<Option<PreparedParallelRoot>> {
    use std::os::unix::ffi::OsStrExt;

    let metadata = std::fs::symlink_metadata(native)?;
    let mut entries = std::fs::read_dir(native)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by(|left, right| {
        left.file_name()
            .as_bytes()
            .cmp(right.file_name().as_bytes())
    });
    if entries.len() < 2 {
        return Ok(None);
    }
    let mut tasks = Vec::with_capacity(entries.len());
    for entry in entries {
        if !entry.file_type()?.is_dir() {
            return Ok(None);
        }
        let name = layerfs_content::CanonicalName::from_bytes(entry.file_name().as_bytes())?;
        tasks.push(RootDirectoryTask {
            logical: child(&layerfs_content::CanonicalPath::root(), &name)?,
            name,
            native: entry.path(),
        });
    }
    let workers = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
        .min(16)
        .min(tasks.len());
    if workers < 2 {
        return Ok(None);
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let prepared = std::thread::scope(|scope| -> Result<Vec<PreparedWorker>> {
        let handles = (0..workers)
            .map(|_| {
                let next = &next;
                let tasks = &tasks;
                scope.spawn(move || {
                    let mut directories = Vec::new();
                    let mut local = ObjectBuffer::empty_all_reachable()?;
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(task) = tasks.get(index) else {
                            break;
                        };
                        let mut import = NativeImport::new(seed, &mut local);
                        let inode = import.directory(&task.native, &task.logical, false)?;
                        directories.push(PreparedDirectory {
                            index,
                            name: task.name.clone(),
                            inode,
                            imported: import.finish()?,
                        });
                    }
                    Ok::<_, StoreError>(PreparedWorker {
                        directories,
                        objects: local.into_prevalidated()?,
                    })
                })
            })
            .collect::<Vec<_>>();
        let mut output = Vec::with_capacity(workers);
        for handle in handles {
            output.push(
                handle
                    .join()
                    .map_err(|_| StoreError::Integrity("Layer initialization worker"))??,
            );
        }
        Ok(output)
    })?;

    let mut identities = std::collections::HashSet::new();
    if prepared
        .iter()
        .flat_map(|worker| worker.directories.iter())
        .flat_map(|directory| directory.imported.hard_links.iter())
        .any(|identity| !identities.insert(*identity))
    {
        return Ok(None);
    }

    let mut directories = Vec::with_capacity(tasks.len());
    let mut segments = Vec::with_capacity(prepared.len());
    for worker in prepared {
        segments.push(worker.objects);
        directories.extend(worker.directories);
    }
    directories.sort_by_key(|directory| directory.index);
    Ok(Some(PreparedParallelRoot {
        metadata,
        directories,
        segments,
    }))
}

fn finish_parallel_root(
    prepared: PreparedParallelRoot,
    seed: [u8; 32],
    objects: &mut impl ObjectStore,
) -> Result<ImportedTree> {
    use layerfs_content::filesystem;
    use layerfs_content::tree::inode::{InodeId, InodeKind, InodeRecordV1};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let PreparedParallelRoot {
        metadata,
        directories,
        segments: _,
    } = prepared;
    let mut children = Vec::with_capacity(directories.len());
    let mut mutations = Vec::new();
    let mut hard_links = Vec::new();
    let mut scanned_files = 0_u64;
    let mut scanned_bytes = 0_u64;
    let mut source = SourceImportMetrics::default();
    for directory in directories {
        children.push((directory.name, directory.inode));
        mutations.extend(directory.imported.mutations);
        hard_links.extend(directory.imported.hard_links);
        scanned_files = scanned_files
            .checked_add(directory.imported.scanned_files)
            .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
        scanned_bytes = scanned_bytes
            .checked_add(directory.imported.scanned_bytes)
            .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
        source.merge(directory.imported.source);
    }
    let content = filesystem::build_initial_directory(objects, children)?;
    let metadata_root = filesystem::build_portable_metadata(
        objects,
        InodeKind::Directory,
        metadata.permissions().mode(),
        metadata.mtime(),
        metadata.mtime_nsec() as u32,
    )?;
    mutations.insert(
        0,
        filesystem::InodeMutation::Upsert {
            inode: InodeId::allocate(seed, 0),
            record: InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: content.0,
                metadata_root,
            },
        },
    );
    Ok(ImportedTree {
        mutations,
        hard_links,
        scanned_files,
        scanned_bytes,
        source,
    })
}

struct NativeImport<'objects, 'structure, S: ObjectStore, T: ObjectStore> {
    seed: [u8; 32],
    objects: &'objects mut S,
    structure: Option<&'structure mut T>,
    hard_links:
        std::collections::HashMap<(u64, u64), (layerfs_content::tree::inode::InodeId, usize)>,
    records: Vec<Option<ImportedRecord>>,
    scanned_files: u64,
    scanned_bytes: u64,
    source: SourceImportMetrics,
    metadata_cache: Vec<(
        layerfs_content::tree::inode::InodeKind,
        u32,
        i64,
        u32,
        layerfs_content::ObjectId,
    )>,
    metadata_cache_next: usize,
}

const NATIVE_METADATA_CACHE_ENTRIES: usize = 8;

impl<'objects, S: ObjectStore> NativeImport<'objects, 'objects, S, S> {
    fn new(seed: [u8; 32], objects: &'objects mut S) -> Self {
        Self {
            seed,
            objects,
            structure: None,
            hard_links: std::collections::HashMap::new(),
            records: Vec::new(),
            scanned_files: 0,
            scanned_bytes: 0,
            source: SourceImportMetrics::default(),
            metadata_cache: Vec::with_capacity(NATIVE_METADATA_CACHE_ENTRIES),
            metadata_cache_next: 0,
        }
    }
}

impl<'objects, 'structure, S: ObjectStore, T: ObjectStore>
    NativeImport<'objects, 'structure, S, T>
{
    fn new_split(seed: [u8; 32], objects: &'objects mut S, structure: &'structure mut T) -> Self {
        Self {
            seed,
            objects,
            structure: Some(structure),
            hard_links: std::collections::HashMap::new(),
            records: Vec::new(),
            scanned_files: 0,
            scanned_bytes: 0,
            source: SourceImportMetrics::default(),
            metadata_cache: Vec::with_capacity(NATIVE_METADATA_CACHE_ENTRIES),
            metadata_cache_next: 0,
        }
    }

    fn portable_metadata(
        &mut self,
        kind: layerfs_content::tree::inode::InodeKind,
        mode: u32,
        mtime_seconds: i64,
        mtime_nanoseconds: u32,
    ) -> Result<layerfs_content::ObjectId> {
        let normalized_mode = mode
            & if kind == layerfs_content::tree::inode::InodeKind::Directory {
                0o1777
            } else {
                0o777
            };
        if let Some((_, _, _, _, root)) = self.metadata_cache.iter().find(|entry| {
            (entry.0, entry.1, entry.2, entry.3)
                == (kind, normalized_mode, mtime_seconds, mtime_nanoseconds)
        }) {
            self.source.metadata_cache_hits += 1;
            return Ok(*root);
        }
        self.source.metadata_cache_misses += 1;
        let root = layerfs_content::filesystem::build_portable_metadata(
            self.objects,
            kind,
            normalized_mode,
            mtime_seconds,
            mtime_nanoseconds,
        )?;
        let entry = (
            kind,
            normalized_mode,
            mtime_seconds,
            mtime_nanoseconds,
            root,
        );
        if self.metadata_cache.len() < NATIVE_METADATA_CACHE_ENTRIES {
            self.metadata_cache.push(entry);
        } else {
            self.metadata_cache[self.metadata_cache_next] = entry;
            self.metadata_cache_next =
                (self.metadata_cache_next + 1) % NATIVE_METADATA_CACHE_ENTRIES;
        }
        self.source.metadata_cache_peak_entries = self
            .source
            .metadata_cache_peak_entries
            .max(self.metadata_cache.len() as u64);
        Ok(root)
    }

    fn reserve(&mut self) -> usize {
        let index = self.records.len();
        self.records.push(None);
        index
    }

    fn set_record(&mut self, index: usize, record: ImportedRecord) -> Result<()> {
        let slot = self
            .records
            .get_mut(index)
            .ok_or(StoreError::Integrity("Layer initialization inode slot"))?;
        if slot.replace(record).is_some() {
            return Err(StoreError::Integrity(
                "Layer initialization duplicate inode",
            ));
        }
        Ok(())
    }

    fn finish(self) -> Result<ImportedTree> {
        let mutations = self
            .records
            .into_iter()
            .map(|record| {
                let (inode, record) =
                    record.ok_or(StoreError::Integrity("Layer initialization inode record"))?;
                Ok(layerfs_content::filesystem::InodeMutation::Upsert { inode, record })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(ImportedTree {
            mutations,
            hard_links: self.hard_links.into_keys().collect(),
            scanned_files: self.scanned_files,
            scanned_bytes: self.scanned_bytes,
            source: self.source,
        })
    }

    fn finish_compact(
        mut self,
        pairs: &mut crate::objects::CompactInodePairWriter,
    ) -> Result<CompactImportedTree> {
        #[cfg(test)]
        let record_len = self.records.len();
        #[cfg(test)]
        let record_capacity = self.records.capacity();
        for record in self.records {
            let (inode, record) =
                record.ok_or(StoreError::Integrity("Layer initialization inode record"))?;
            let canonical = layerfs_content::tree::inode::codec::encode_inode_record(record)?;
            let record = match self.structure.as_deref_mut() {
                Some(structure) => structure.put_owned(canonical)?,
                None => self.objects.put_owned(canonical)?,
            };
            pairs.push(inode, record)?;
        }
        Ok(CompactImportedTree {
            hard_links: self.hard_links.into_keys().collect(),
            scanned_files: self.scanned_files,
            scanned_bytes: self.scanned_bytes,
            source: self.source,
            #[cfg(test)]
            record_len,
            #[cfg(test)]
            record_capacity,
        })
    }

    fn directory(
        &mut self,
        native: &std::path::Path,
        logical: &layerfs_content::CanonicalPath,
        root: bool,
    ) -> Result<layerfs_content::tree::inode::InodeId> {
        use layerfs_content::filesystem;
        use layerfs_content::tree::inode::{InodeId, InodeKind, InodeRecordV1};
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        self.source.symlink_metadata_calls += 1;
        let metadata = std::fs::symlink_metadata(native)?;
        let inode = if root {
            InodeId::allocate(self.seed, 0)
        } else {
            filesystem::allocated_inode(self.seed, logical)
        };
        let slot = self.reserve();
        let mut children = Vec::new();
        self.source.read_dir_calls += 1;
        let mut entries = std::fs::read_dir(native)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by(|left, right| {
            left.file_name()
                .as_bytes()
                .cmp(right.file_name().as_bytes())
        });
        for entry in entries {
            let name = layerfs_content::CanonicalName::from_bytes(entry.file_name().as_bytes())?;
            let logical_path = child(logical, &name)?;
            self.source.symlink_metadata_calls += 1;
            let entry_metadata = std::fs::symlink_metadata(entry.path())?;
            let native_key = (entry_metadata.dev(), entry_metadata.ino());
            if entry_metadata.nlink() > 1 {
                if let Some((linked_inode, record_index)) =
                    self.hard_links.get(&native_key).copied()
                {
                    let (_, record) = self
                        .records
                        .get_mut(record_index)
                        .and_then(Option::as_mut)
                        .ok_or(StoreError::Integrity("Layer initialization hard link"))?;
                    record.namespace_ref_count =
                        record
                            .namespace_ref_count
                            .checked_add(1)
                            .ok_or(StoreError::Integrity(
                                "Layer initialization hard link count",
                            ))?;
                    children.push((name, linked_inode));
                    continue;
                }
            }

            let (child_inode, record_index) = if entry_metadata.file_type().is_dir() {
                (self.directory(&entry.path(), &logical_path, false)?, None)
            } else if entry_metadata.file_type().is_file() {
                let child_inode = filesystem::allocated_inode(self.seed, &logical_path);
                let record_index = self.reserve();
                self.source.file_open_calls += 1;
                let mut source = CountedSourceReader {
                    file: std::fs::File::open(entry.path())?,
                    calls: 0,
                    bytes: 0,
                };
                let (content, counters) =
                    layerfs_content::file::rope::build(self.objects, &mut source)?;
                self.source.file_read_calls =
                    self.source.file_read_calls.saturating_add(source.calls);
                self.source.file_read_bytes =
                    self.source.file_read_bytes.saturating_add(source.bytes);
                self.source.streaming_files += 1;
                self.source.cdc_scratch_peak_bytes = self
                    .source
                    .cdc_scratch_peak_bytes
                    .max((layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES * 2) as u64);
                self.scanned_files = self
                    .scanned_files
                    .checked_add(1)
                    .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
                self.scanned_bytes = self
                    .scanned_bytes
                    .checked_add(counters.cdc_bytes_scanned)
                    .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
                let metadata_root = self.portable_metadata(
                    InodeKind::RegularFile,
                    entry_metadata.permissions().mode(),
                    entry_metadata.mtime(),
                    entry_metadata.mtime_nsec() as u32,
                )?;
                self.set_record(
                    record_index,
                    (
                        child_inode,
                        InodeRecordV1 {
                            kind: InodeKind::RegularFile,
                            namespace_ref_count: 1,
                            content_root: content.0,
                            metadata_root,
                        },
                    ),
                )?;
                (child_inode, Some(record_index))
            } else if entry_metadata.file_type().is_symlink() {
                let child_inode = filesystem::allocated_inode(self.seed, &logical_path);
                let record_index = self.reserve();
                let content_root = filesystem::symlink_content(
                    self.objects,
                    std::fs::read_link(entry.path())?
                        .as_os_str()
                        .as_bytes()
                        .to_vec(),
                )?;
                let metadata_root = self.portable_metadata(
                    InodeKind::Symlink,
                    0o777,
                    entry_metadata.mtime(),
                    entry_metadata.mtime_nsec() as u32,
                )?;
                self.set_record(
                    record_index,
                    (
                        child_inode,
                        InodeRecordV1 {
                            kind: InodeKind::Symlink,
                            namespace_ref_count: 1,
                            content_root,
                            metadata_root,
                        },
                    ),
                )?;
                (child_inode, Some(record_index))
            } else {
                return Err(StoreError::InvalidInput("unsupported Layer entry"));
            };
            if entry_metadata.nlink() > 1 {
                if let Some(record_index) = record_index {
                    self.hard_links
                        .insert(native_key, (child_inode, record_index));
                }
            }
            children.push((name, child_inode));
        }

        let content = match self.structure.as_deref_mut() {
            Some(structure) => filesystem::build_initial_directory(structure, children)?,
            None => filesystem::build_initial_directory(self.objects, children)?,
        };
        let metadata_root = self.portable_metadata(
            InodeKind::Directory,
            metadata.permissions().mode(),
            metadata.mtime(),
            metadata.mtime_nsec() as u32,
        )?;
        self.set_record(
            slot,
            (
                inode,
                InodeRecordV1 {
                    kind: InodeKind::Directory,
                    namespace_ref_count: u64::from(!root),
                    content_root: content.0,
                    metadata_root,
                },
            ),
        )?;
        Ok(inode)
    }
}

#[cfg(test)]
fn legacy_directory_root(path: &std::path::Path, seed: [u8; 32]) -> Result<(BuiltRoot, u64, u64)> {
    use layerfs_content::filesystem;
    use layerfs_content::CanonicalPath;
    use std::collections::HashMap;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let mut objects = ObjectBuffer::empty()?;
    let mut root = filesystem::empty_root(&mut objects, seed)?;
    let metadata = std::fs::symlink_metadata(path)?;
    root = filesystem::set_mode(
        &mut objects,
        root,
        &CanonicalPath::root(),
        metadata.permissions().mode(),
    )?
    .root();
    root = filesystem::set_mtime(
        &mut objects,
        root,
        &CanonicalPath::root(),
        metadata.mtime(),
        metadata.mtime_nsec() as u32,
    )?
    .root();
    let mut hard_links = HashMap::new();
    let mut scanned_files = 0_u64;
    let mut scanned_bytes = 0_u64;
    legacy_import_directory(
        path,
        &CanonicalPath::root(),
        seed,
        &mut objects,
        &mut root,
        &mut hard_links,
        &mut scanned_files,
        &mut scanned_bytes,
    )?;
    Ok((
        objects.finish(root, scanned_bytes)?,
        scanned_files,
        scanned_bytes,
    ))
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn legacy_import_directory(
    native: &std::path::Path,
    logical: &layerfs_content::CanonicalPath,
    seed: [u8; 32],
    objects: &mut ObjectBuffer<'_>,
    root: &mut layerfs_content::ObjectId,
    hard_links: &mut std::collections::HashMap<(u64, u64), layerfs_content::CanonicalPath>,
    scanned_files: &mut u64,
    scanned_bytes: &mut u64,
) -> Result<()> {
    use layerfs_content::filesystem;
    use layerfs_content::CanonicalName;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let mut entries = std::fs::read_dir(native)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by(|left, right| {
        left.file_name()
            .as_bytes()
            .cmp(right.file_name().as_bytes())
    });
    for entry in entries {
        let name = CanonicalName::from_bytes(entry.file_name().as_bytes())?;
        let path = child(logical, &name)?;
        let path_text = path.as_str().to_owned();
        let metadata = std::fs::symlink_metadata(entry.path())?;
        let hard_link = (metadata.nlink() > 1)
            .then(|| hard_links.get(&(metadata.dev(), metadata.ino())).cloned())
            .flatten();
        if let Some(source) = hard_link {
            *root = filesystem::apply_changes(
                objects,
                *root,
                &[filesystem::ContentChange::HardLink {
                    source: source.as_str().to_owned(),
                    target: path_text,
                }],
                seed,
            )?
            .root_id;
            continue;
        }
        if metadata.file_type().is_dir() {
            *root = filesystem::apply_changes(
                objects,
                *root,
                &[filesystem::ContentChange::Mkdir {
                    path: path_text,
                    mode: metadata.permissions().mode(),
                }],
                seed,
            )?
            .root_id;
            legacy_import_directory(
                &entry.path(),
                &path,
                seed,
                objects,
                root,
                hard_links,
                scanned_files,
                scanned_bytes,
            )?;
        } else if metadata.file_type().is_file() {
            let candidate = filesystem::write_file(
                objects,
                *root,
                &path,
                std::fs::File::open(entry.path())?,
                metadata.permissions().mode(),
                seed,
            )?;
            *scanned_files = scanned_files
                .checked_add(1)
                .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
            *scanned_bytes = scanned_bytes
                .checked_add(candidate.counters().rope.cdc_bytes_scanned)
                .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
            *root = candidate.root();
        } else if metadata.file_type().is_symlink() {
            *root = filesystem::apply_changes(
                objects,
                *root,
                &[filesystem::ContentChange::Symlink {
                    path: path_text,
                    target: std::fs::read_link(entry.path())?
                        .as_os_str()
                        .as_bytes()
                        .to_vec(),
                }],
                seed,
            )?
            .root_id;
        } else {
            return Err(StoreError::InvalidInput("unsupported Layer entry"));
        }
        *root = filesystem::set_mtime(
            objects,
            *root,
            &path,
            metadata.mtime(),
            metadata.mtime_nsec() as u32,
        )?
        .root();
        if metadata.nlink() > 1 {
            hard_links.insert((metadata.dev(), metadata.ino()), path);
        }
    }
    Ok(())
}

fn child(
    parent: &layerfs_content::CanonicalPath,
    name: &layerfs_content::CanonicalName,
) -> Result<layerfs_content::CanonicalPath> {
    let mut bytes = parent.as_bytes().to_vec();
    if !bytes.is_empty() {
        bytes.push(b'/');
    }
    bytes.extend_from_slice(name.as_bytes());
    Ok(layerfs_content::CanonicalPath::from_bytes(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn benchmark_initialization_seed_is_exact_lower_hex() {
        assert_eq!(
            decode_initialization_seed(&"01".repeat(32)).unwrap(),
            [1; 32]
        );
        assert!(decode_initialization_seed(&"01".repeat(31)).is_err());
        assert!(decode_initialization_seed(&"AA".repeat(32)).is_err());
    }

    fn cached_directory_root(
        path: &std::path::Path,
        seed: [u8; 32],
    ) -> (BuiltRoot, SourceImportMetrics) {
        let mut objects = ObjectBuffer::empty_all_reachable().unwrap();
        let mut import = NativeImport::new(seed, &mut objects);
        import
            .directory(path, &layerfs_content::CanonicalPath::root(), true)
            .unwrap();
        let imported = import.finish().unwrap();
        let source = imported.source;
        let root = layerfs_content::filesystem::build_initial_namespace(
            &mut objects,
            seed,
            imported.mutations,
        )
        .unwrap();
        (
            objects
                .finish_all_reachable(root, imported.scanned_bytes)
                .unwrap(),
            source,
        )
    }

    #[test]
    fn native_metadata_cache_is_exact_canonical_and_bounded() {
        let root = temporary("metadata-cache");
        for index in 0..10 {
            let path = root.join(format!("file-{index}"));
            std::fs::write(&path, index.to_string()).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
            std::fs::File::open(path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(
                    std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + index),
                ))
                .unwrap();
        }
        let seed = [0x61; 32];
        let (unique, metrics) = cached_directory_root(&root, seed);
        let (legacy, _, _) = legacy_directory_root(&root, seed).unwrap();
        assert_eq!(unique.root_id, legacy.root_id);
        assert_eq!(unique.objects.len(), legacy.objects.len());
        assert_eq!(metrics.metadata_cache_hits, 0);
        assert_eq!(metrics.metadata_cache_misses, 11);
        assert_eq!(metrics.metadata_cache_peak_entries, 8);
        drop(unique);
        drop(legacy);
        std::fs::remove_dir_all(&root).unwrap();

        let root = temporary("metadata-cache-reuse");
        for name in ["first", "second"] {
            let path = root.join(name);
            std::fs::write(&path, name).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
            std::fs::File::open(path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(
                    std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_123),
                ))
                .unwrap();
        }
        let (cached, metrics) = cached_directory_root(&root, seed);
        let (legacy, _, _) = legacy_directory_root(&root, seed).unwrap();
        assert_eq!(cached.root_id, legacy.root_id);
        assert_eq!(cached.objects.len(), legacy.objects.len());
        assert_eq!(metrics.metadata_cache_hits, 1);
        assert_eq!(metrics.metadata_cache_misses, 2);
        assert_eq!(metrics.metadata_cache_peak_entries, 2);
        drop(cached);
        drop(legacy);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batched_directory_import_matches_legacy_canonical_root() {
        let root = temporary("canonical");
        let nested = root.join("a");
        std::fs::create_dir(&nested).unwrap();
        std::fs::create_dir(root.join("z-empty")).unwrap();
        std::fs::write(nested.join("a-10"), b"first-content").unwrap();
        std::fs::write(nested.join("a-2"), b"second-content").unwrap();
        std::fs::hard_link(nested.join("a-10"), root.join("link")).unwrap();
        symlink("a/a-2", root.join("symlink")).unwrap();
        std::fs::set_permissions(nested.join("a-10"), std::fs::Permissions::from_mode(0o600))
            .unwrap();
        std::fs::set_permissions(nested.join("a-2"), std::fs::Permissions::from_mode(0o640))
            .unwrap();
        std::fs::set_permissions(&nested, std::fs::Permissions::from_mode(0o750)).unwrap();
        let times = std::fs::FileTimes::new()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_123));
        for path in [
            nested.join("a-10"),
            nested.join("a-2"),
            nested,
            root.join("z-empty"),
            root.clone(),
        ] {
            std::fs::File::open(path).unwrap().set_times(times).unwrap();
        }

        let seed = [37; 32];
        let (batched, batched_files, batched_bytes) = directory_root(&root, seed).unwrap();
        let (legacy, legacy_files, legacy_bytes) = legacy_directory_root(&root, seed).unwrap();
        assert_eq!(batched.root_id, legacy.root_id);
        assert_eq!(batched.objects.len(), legacy.objects.len());
        assert_eq!(
            batched.objects.encoded_bytes(),
            legacy.objects.encoded_bytes()
        );
        assert_eq!((batched_files, batched_bytes), (legacy_files, legacy_bytes));

        drop(batched);
        drop(legacy);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parallel_root_import_and_cross_directory_hard_link_match_legacy() {
        for cross_link in [false, true] {
            let root = temporary(if cross_link {
                "parallel-link"
            } else {
                "parallel"
            });
            let left = root.join("left");
            let right = root.join("right");
            std::fs::create_dir(&left).unwrap();
            std::fs::create_dir(&right).unwrap();
            std::fs::write(left.join("file"), b"left-content").unwrap();
            if cross_link {
                std::fs::hard_link(left.join("file"), right.join("file")).unwrap();
            } else {
                std::fs::write(right.join("file"), b"right-content").unwrap();
            }
            let seed = [83; 32];
            let (parallel, parallel_files, parallel_bytes) = directory_root(&root, seed).unwrap();
            let (legacy, legacy_files, legacy_bytes) = legacy_directory_root(&root, seed).unwrap();
            assert_eq!(parallel.root_id, legacy.root_id);
            assert_eq!(parallel.objects.len(), legacy.objects.len());
            assert_eq!(
                (parallel_files, parallel_bytes),
                (legacy_files, legacy_bytes)
            );
            drop(parallel);
            drop(legacy);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn composite_initialization_builds_the_final_root_without_worker_payload_reads() {
        let root = temporary("composite-final-root");
        let source = root.join("source");
        let left = source.join("left");
        let right = source.join("right");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir(&right).unwrap();
        std::fs::write(left.join("one"), b"one").unwrap();
        std::fs::write(right.join("two"), b"two").unwrap();

        let store_path = root.join("store.sqlite");
        let store = LayerStackStore::create(&store_path).unwrap();
        let initialized = store
            .initialize_layerstack(
                EntityName::new("composite").unwrap(),
                LayerStackInitialization::Directory(source.clone()),
            )
            .unwrap();
        let seed = *blake3::hash(initialized.layer_stack_id.as_slice()).as_bytes();
        drop(store);

        let reopened = LayerStackStore::connect(&store_path).unwrap();
        let stack = reopened
            .layer_stack(initialized.layer_stack_id)
            .unwrap()
            .unwrap();
        let layer = reopened.layer(stack.head_layer_id).unwrap().unwrap();
        let (legacy, legacy_files, legacy_bytes) = legacy_directory_root(&source, seed).unwrap();
        assert_eq!((legacy_files, legacy_bytes), (2, 6));
        assert_eq!(layer.root_id, legacy.root_id);
        let mut ids = legacy.objects.ids_in_order(usize::MAX).unwrap().unwrap();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(reopened.store_counts().unwrap().objects, ids.len() as u64);
        for id in ids {
            assert_eq!(
                crate::ObjectSource::read_object(&reopened, id).unwrap(),
                crate::ObjectSource::read_object(&legacy.objects, id).unwrap()
            );
        }

        drop(legacy);
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn append_only_zero_get_path_matches_every_canonical_object_class() {
        let root = temporary("append-only-canonical");
        let source = root.join("source");
        let left = source.join("left");
        let nested = left.join("nested");
        let right = source.join("right");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir(&right).unwrap();
        std::fs::write(left.join("empty"), []).unwrap();
        std::fs::write(left.join("tiny"), b"tiny").unwrap();
        std::fs::write(left.join("medium"), vec![23_u8; 9_000]).unwrap();
        std::fs::write(left.join("large"), vec![41_u8; 1024 * 1024]).unwrap();
        std::fs::write(nested.join("file"), b"nested").unwrap();
        std::fs::write(left.join("hard-source"), b"hard-linked").unwrap();
        std::fs::hard_link(left.join("hard-source"), left.join("hard-target")).unwrap();
        symlink("nested/file", left.join("symlink")).unwrap();
        std::fs::write(right.join("file"), b"right").unwrap();

        let store_path = root.join("store.sqlite");
        let store = LayerStackStore::create(&store_path).unwrap();
        let initialized = store
            .initialize_layerstack(
                EntityName::new("append-only").unwrap(),
                LayerStackInitialization::Directory(source.clone()),
            )
            .unwrap();
        let seed = *blake3::hash(initialized.layer_stack_id.as_slice()).as_bytes();
        drop(store);

        let reopened = LayerStackStore::connect(&store_path).unwrap();
        let stack = reopened
            .layer_stack(initialized.layer_stack_id)
            .unwrap()
            .unwrap();
        let layer = reopened.layer(stack.head_layer_id).unwrap().unwrap();
        let (legacy, _, _) = legacy_directory_root(&source, seed).unwrap();
        assert_eq!(layer.root_id, legacy.root_id);
        let mut ids = legacy.objects.ids_in_order(usize::MAX).unwrap().unwrap();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(reopened.store_counts().unwrap().objects, ids.len() as u64);
        for id in ids {
            assert_eq!(
                crate::ObjectSource::read_object(&reopened, id).unwrap(),
                crate::ObjectSource::read_object(&legacy.objects, id).unwrap()
            );
        }

        drop(legacy);
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compact_inode_path_matches_canonical_roots_at_every_directory_tier() {
        for count in [0_usize, 1, 100, 1_000] {
            let root = temporary(&format!("compact-root-{count}"));
            let source = root.join("source");
            std::fs::create_dir(&source).unwrap();
            for index in 0..count {
                std::fs::create_dir(source.join(format!("d{index:04}"))).unwrap();
            }
            let store_path = root.join("store.sqlite");
            let store = LayerStackStore::create(&store_path).unwrap();
            let initialized = store
                .initialize_layerstack(
                    EntityName::new(format!("root-{count}")).unwrap(),
                    LayerStackInitialization::Directory(source.clone()),
                )
                .unwrap();
            let seed = *blake3::hash(initialized.layer_stack_id.as_slice()).as_bytes();
            drop(store);

            let reopened = LayerStackStore::connect(&store_path).unwrap();
            let stack = reopened
                .layer_stack(initialized.layer_stack_id)
                .unwrap()
                .unwrap();
            let layer = reopened.layer(stack.head_layer_id).unwrap().unwrap();
            let (expected, _, _) = directory_root(&source, seed).unwrap();
            assert_eq!(layer.root_id, expected.root_id);
            let mut ids = expected.objects.ids_in_order(usize::MAX).unwrap().unwrap();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(reopened.store_counts().unwrap().objects, ids.len() as u64);
            for id in ids {
                assert_eq!(
                    crate::ObjectSource::read_object(&reopened, id).unwrap(),
                    crate::ObjectSource::read_object(&expected.objects, id).unwrap()
                );
            }

            drop(expected);
            drop(reopened);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn over_task_block_limit_falls_back_before_admission_and_reuses_store() {
        let root = temporary("compact-root-over-task-limit");
        let source = root.join("source");
        std::fs::create_dir(&source).unwrap();
        for index in 0..1_001 {
            std::fs::create_dir(source.join(format!("d{index:04}"))).unwrap();
        }
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();

        assert!(
            direct_initialize_root_directories(&store.db, &source, [83; 32])
                .unwrap()
                .is_none()
        );
        assert_eq!(store.store_counts().unwrap().objects, 0);

        let initialized = store
            .initialize_layerstack(
                EntityName::new("over-task-limit").unwrap(),
                LayerStackInitialization::Directory(source.clone()),
            )
            .unwrap();
        let seed = *blake3::hash(initialized.layer_stack_id.as_slice()).as_bytes();
        let stack = store
            .layer_stack(initialized.layer_stack_id)
            .unwrap()
            .unwrap();
        let layer = store.layer(stack.head_layer_id).unwrap().unwrap();
        let (expected, files, _) = directory_root(&source, seed).unwrap();
        assert_eq!(files, 0);
        assert_eq!(layer.root_id, expected.root_id);

        drop(expected);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compact_inode_cross_task_hard_link_falls_back_before_admission() {
        let root = temporary("compact-cross-task-hard-link");
        let source = root.join("source");
        let left = source.join("left");
        let right = source.join("right");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir(&right).unwrap();
        std::fs::write(left.join("file"), b"cross-task").unwrap();
        std::fs::hard_link(left.join("file"), right.join("file")).unwrap();
        let store_path = root.join("store.sqlite");
        let store = LayerStackStore::create(&store_path).unwrap();
        let initialized = store
            .initialize_layerstack(
                EntityName::new("cross-task").unwrap(),
                LayerStackInitialization::Directory(source.clone()),
            )
            .unwrap();
        let seed = *blake3::hash(initialized.layer_stack_id.as_slice()).as_bytes();
        let stack = store
            .layer_stack(initialized.layer_stack_id)
            .unwrap()
            .unwrap();
        let layer = store.layer(stack.head_layer_id).unwrap().unwrap();
        let (expected, _, _) = directory_root(&source, seed).unwrap();
        assert_eq!(layer.root_id, expected.root_id);
        assert_eq!(
            store.store_counts().unwrap().objects,
            expected.objects.len()
        );
        let mut ids = expected.objects.ids_in_order(usize::MAX).unwrap().unwrap();
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            assert_eq!(
                crate::ObjectSource::read_object(&store, id).unwrap(),
                crate::ObjectSource::read_object(&expected.objects, id).unwrap()
            );
        }

        drop(expected);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compact_inode_mutable_records_are_bounded_by_one_task() {
        if std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
            < 2
        {
            return;
        }
        let root = temporary("compact-task-bound");
        for directory in ["left", "right"] {
            let path = root.join(directory);
            std::fs::create_dir(&path).unwrap();
            for file in 0..100 {
                std::fs::write(path.join(format!("f{file:03}")), file.to_string()).unwrap();
            }
        }
        let prepared = prepare_append_only_root_directories(&root, [71; 32])
            .unwrap()
            .unwrap();
        let record_lengths = prepared
            .directories
            .iter()
            .map(|directory| directory.imported.record_len)
            .collect::<Vec<_>>();
        let record_capacities = prepared
            .directories
            .iter()
            .map(|directory| directory.imported.record_capacity)
            .collect::<Vec<_>>();
        assert_eq!(record_lengths, vec![101, 101]);
        assert!(record_capacities
            .iter()
            .zip(&record_lengths)
            .all(|(capacity, length)| capacity >= length));
        assert!(record_capacities.iter().all(|capacity| *capacity < 202));
        assert!(prepared
            .pair_blocks
            .iter()
            .zip(&record_lengths)
            .all(|(block, length)| block.pair_count == *length as u64));
        let object_reader_capacity = prepared
            .segments
            .iter()
            .map(AppendOnlyInitializationSegment::reader_capacity)
            .sum::<usize>();
        let pair_reader_capacity = prepared
            .pairs
            .iter()
            .map(crate::objects::CompactInodePairSegment::reader_capacity)
            .sum::<usize>();
        assert!(object_reader_capacity <= INITIALIZATION_APPEND_PENDING_BYTES);
        assert!(pair_reader_capacity <= INITIALIZATION_PAIR_PENDING_BYTES);

        drop(prepared);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(feature = "test-instrumentation")]
    #[test]
    fn empty_composite_admission_uses_no_object_point_selects() {
        if std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
            < 2
        {
            return;
        }
        let root = temporary("composite-sql-shape");
        let source = root.join("source");
        std::fs::create_dir_all(source.join("left")).unwrap();
        std::fs::create_dir(source.join("right")).unwrap();
        std::fs::write(source.join("left/file"), b"left").unwrap();
        std::fs::write(source.join("right/file"), b"right").unwrap();
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();

        crate::schema::reset_sql_trace();
        store
            .initialize_layerstack(
                EntityName::new("sql-shape").unwrap(),
                LayerStackInitialization::Directory(source),
            )
            .unwrap();
        let trace = crate::schema::sql_trace();
        let object_inserts = trace
            .iter()
            .filter(|sql| sql.contains("INSERT INTO objects(object_id, bytes)"))
            .collect::<Vec<_>>();
        assert!(!object_inserts.is_empty());
        assert!(object_inserts
            .iter()
            .all(|sql| !sql.contains("RETURNING object_id")));
        assert!(trace
            .iter()
            .all(|sql| !sql.contains("SELECT bytes FROM objects WHERE object_id =")));
        assert!(trace
            .iter()
            .filter(|sql| sql.contains("FROM objects"))
            .all(|sql| sql.contains("SELECT NOT EXISTS") || sql.contains("WHERE object_id IN")));

        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn late_direct_initialization_failure_publishes_nothing() {
        let root = temporary("composite-late-failure");
        let source = root.join("source");
        std::fs::create_dir_all(source.join("left")).unwrap();
        std::fs::create_dir(source.join("right")).unwrap();
        std::fs::write(source.join("left/file"), b"left").unwrap();
        std::fs::write(source.join("right/file"), b"right").unwrap();
        let (candidate, _, _) = directory_root(&source, [91; 32]).unwrap();
        assert!(candidate.objects.encoded_bytes() < crate::objects::ADMISSION_BATCH_BYTES as u64);
        let layer_statement = candidate.objects.len() + 1;
        drop(candidate);

        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        crate::schema::set_transaction_failure_at(Some(layer_statement));
        let result = store.initialize_layerstack(
            EntityName::new("late-failure").unwrap(),
            LayerStackInitialization::Directory(source.clone()),
        );
        assert!(
            matches!(
                result,
                Err(StoreError::Integrity("injected transaction failure"))
            ),
            "{result:?}"
        );
        crate::schema::set_transaction_failure_at(None);
        let counts = store.store_counts().unwrap();
        assert_eq!(counts.objects, 0);
        assert_eq!(counts.layer_stacks, 0);
        assert_eq!(counts.layers, 0);

        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn multi_batch_publication_failure_removes_admitted_objects() {
        let root = temporary("multi-batch-publication-failure");
        let source = root.join("source");
        for directory in ["left", "right"] {
            let path = source.join(directory);
            std::fs::create_dir_all(&path).unwrap();
            for file in 0..1_500_u64 {
                std::fs::write(path.join(format!("f{file:04}")), file.to_be_bytes()).unwrap();
            }
        }
        let store_path = root.join("store.sqlite");
        let store = LayerStackStore::create(&store_path).unwrap();
        let baseline_bytes = std::fs::metadata(&store_path).unwrap().len();
        let baseline_pages: i64 = store
            .db
            .reader()
            .unwrap()
            .pragma_query_value(None, "page_count", |row| row.get(0))
            .unwrap();
        let baseline_freelist: i64 = store
            .db
            .reader()
            .unwrap()
            .pragma_query_value(None, "freelist_count", |row| row.get(0))
            .unwrap();
        assert_eq!(baseline_freelist, 0);
        crate::schema::set_transaction_failure_at(Some(u64::MAX));
        let result = store.initialize_layerstack(
            EntityName::new("failed").unwrap(),
            LayerStackInitialization::Directory(source.clone()),
        );
        crate::schema::set_transaction_failure_at(None);
        assert!(matches!(
            result,
            Err(StoreError::Integrity("injected transaction failure"))
        ));
        assert_eq!(store.store_counts().unwrap().objects, 0);
        assert_eq!(store.store_counts().unwrap().layers, 0);
        assert_eq!(store.store_counts().unwrap().layer_stacks, 0);
        let failed_bytes = std::fs::metadata(&store_path).unwrap().len();
        let failed_pages: i64 = store
            .db
            .reader()
            .unwrap()
            .pragma_query_value(None, "page_count", |row| row.get(0))
            .unwrap();
        let failed_freelist: i64 = store
            .db
            .reader()
            .unwrap()
            .pragma_query_value(None, "freelist_count", |row| row.get(0))
            .unwrap();
        assert_eq!(
            baseline_bytes,
            baseline_pages as u64 * crate::schema::SQLITE_PAGE_SIZE_BYTES as u64
        );
        assert_eq!(
            failed_bytes,
            failed_pages as u64 * crate::schema::SQLITE_PAGE_SIZE_BYTES as u64
        );
        assert!(failed_pages >= baseline_pages);
        assert!(
            failed_pages == baseline_pages || failed_freelist > baseline_freelist,
            "cleanup must truncate to baseline or expose reclaimed pages on the freelist"
        );
        drop(store);

        let reopened = LayerStackStore::connect(&store_path).unwrap();
        reopened
            .initialize_layerstack(
                EntityName::new("recovered").unwrap(),
                LayerStackInitialization::Directory(source),
            )
            .unwrap();
        assert_eq!(reopened.store_counts().unwrap().layer_stacks, 1);
        assert_eq!(reopened.store_counts().unwrap().layers, 1);
        assert!(reopened.store_counts().unwrap().objects > 0);
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "large spill correctness gate"]
    fn parallel_large_spill_matches_legacy_after_fresh_store_reopen() {
        let root = temporary("parallel-large-spill");
        let source = root.join("source");
        let left = source.join("left");
        let right = source.join("right");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir(&right).unwrap();

        let mut anchor = std::fs::File::create(left.join("anchor")).unwrap();
        let mut buffer = vec![0_u8; 1024 * 1024];
        let mut remaining = 100_000_000_usize;
        let mut block = 0_u64;
        while remaining != 0 {
            buffer[..8].copy_from_slice(&block.to_le_bytes());
            let length = remaining.min(buffer.len());
            std::io::Write::write_all(&mut anchor, &buffer[..length]).unwrap();
            remaining -= length;
            block += 1;
        }
        drop(anchor);
        std::fs::write(left.join("tiny"), b"tiny").unwrap();
        std::fs::write(right.join("empty"), []).unwrap();
        std::fs::write(right.join("small"), vec![37_u8; 4 * 1024]).unwrap();

        let store_path = root.join("store.sqlite");
        let store = LayerStackStore::create(&store_path).unwrap();
        let initialized = store
            .initialize_layerstack(
                EntityName::new("large-spill").unwrap(),
                LayerStackInitialization::Directory(source.clone()),
            )
            .unwrap();
        let seed = *blake3::hash(initialized.layer_stack_id.as_slice()).as_bytes();
        drop(store);

        let reopened = LayerStackStore::connect(&store_path).unwrap();
        let stack = reopened
            .layer_stack(initialized.layer_stack_id)
            .unwrap()
            .unwrap();
        let layer = reopened.layer(stack.head_layer_id).unwrap().unwrap();
        let (legacy, legacy_files, legacy_bytes) = legacy_directory_root(&source, seed).unwrap();
        assert_eq!(legacy_files, 4);
        assert_eq!(legacy_bytes, 100_004_100);
        assert_eq!(layer.root_id, legacy.root_id);
        assert_eq!(
            reopened.store_counts().unwrap().objects,
            legacy.objects.len()
        );

        let mut ids = legacy.objects.ids_in_order(usize::MAX).unwrap().unwrap();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len() as u64, legacy.objects.len());
        for id in ids {
            assert_eq!(
                crate::ObjectSource::read_object(&reopened, id).unwrap(),
                crate::ObjectSource::read_object(&legacy.objects, id).unwrap()
            );
        }

        drop(legacy);
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batched_directory_import_retains_only_final_structure() {
        let root = temporary("thousand");
        for directory in 0..10 {
            let data = root.join(format!("d{directory:04}"));
            std::fs::create_dir(&data).unwrap();
            for file in 0..100 {
                std::fs::write(
                    data.join(format!("f{file:06}")),
                    format!("{directory:04}/{file:06}"),
                )
                .unwrap();
            }
        }
        let (built, files, _) = directory_root(&root, [19; 32]).unwrap();
        assert_eq!(files, 1_000);
        assert_eq!(built.counters.encode_hash_invocations, built.objects.len());
        assert!(!built.objects.has_reference_index());

        drop(built);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_single_task_clears_direct_admission_and_falls_back() {
        let root = temporary("oversized-direct-task");
        let source = root.join("source");
        let subtree = source.join("subtree");
        std::fs::create_dir_all(&subtree).unwrap();
        for file in 0..4_000_u64 {
            std::fs::write(subtree.join(format!("f{file:04}")), file.to_be_bytes()).unwrap();
        }
        let store_path = root.join("store.sqlite");
        let store = LayerStackStore::create(&store_path).unwrap();
        assert!(
            direct_initialize_root_directories(&store.db, &source, [29; 32])
                .unwrap()
                .is_none()
        );
        assert_eq!(store.store_counts().unwrap().objects, 0);

        let initialized = store
            .initialize_layerstack(
                EntityName::new("oversized").unwrap(),
                LayerStackInitialization::Directory(source.clone()),
            )
            .unwrap();
        let seed = *blake3::hash(initialized.layer_stack_id.as_slice()).as_bytes();
        let stack = store
            .layer_stack(initialized.layer_stack_id)
            .unwrap()
            .unwrap();
        let layer = store.layer(stack.head_layer_id).unwrap().unwrap();
        let (expected, files, _) = directory_root(&source, seed).unwrap();
        assert_eq!(files, 4_000);
        assert_eq!(layer.root_id, expected.root_id);

        drop(expected);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn temporary(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "layerfs-import-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }
}
