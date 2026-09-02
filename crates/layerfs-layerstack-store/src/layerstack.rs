use crate::ids::TypedId;
use crate::objects::{
    admit_initialization_objects, empty_root, insert_initialization_object_batch, BuiltRoot,
    DeferredObjectStore, ObjectBuffer,
};
use crate::records::decode_object_id;
use crate::{
    AddLayerResult, BranchId, CommitId, EntityName, InitializeLayerStackResult, LayerId,
    LayerRecord, LayerStackId, LayerStackInitialization, LayerStackRecord, LayerStackStore, Result,
    StoreError,
};
use rusqlite::TransactionBehavior;

impl LayerStackStore {
    pub fn initialize_layerstack(
        &self,
        name: EntityName,
        source: LayerStackInitialization,
    ) -> Result<InitializeLayerStackResult> {
        let _operation = self.db.enter_operation()?;
        let layer_stack_id = LayerStackId::new();
        let seed = *blake3::hash(layer_stack_id.as_slice()).as_bytes();
        let (built, scanned_files, scanned_bytes) = match source {
            LayerStackInitialization::Empty => (empty_root(seed)?, 0, 0),
            LayerStackInitialization::Directory(path) => {
                if !path.is_dir() {
                    return Err(StoreError::InvalidInput("Layer initialization directory"));
                }
                directory_root(&path, seed)?
            }
        };
        let layer = LayerRecord {
            id: LayerId::derive(layer_stack_id, None, built.root_id),
            layer_stack_id,
            parent_layer_id: None,
            root_id: built.root_id,
            source_branch_id: None,
            source_commit_id: None,
        };
        let stack = LayerStackRecord {
            id: layer_stack_id,
            name: name.clone(),
            head_layer_id: layer.id,
        };
        let plan = self.db.plan_initialization_candidate(&built.objects)?;
        let mut statement_number = 0;
        let admission =
            admit_initialization_objects(&self.db, &built.objects, &plan, &mut statement_number)?;
        let mut connection = self.db.writer()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let final_metrics = insert_initialization_object_batch(
            &transaction,
            &admission.final_batch,
            &mut statement_number,
        )?;
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
        transaction.commit()?;
        crate::telemetry::record_initialization_candidate(crate::CandidateReceipt {
            candidate_objects: plan.candidate_objects,
            candidate_bytes: plan.candidate_bytes,
            inserted_objects: plan.inserted_objects,
            inserted_bytes: plan.inserted_bytes,
            reused_objects: plan.reused_objects,
            reused_bytes: plan.reused_bytes,
            batch_inserted_objects: admission.batch_inserted_objects,
            batch_inserted_bytes: admission.batch_inserted_bytes,
            final_inserted_objects: final_metrics.objects,
            final_inserted_bytes: final_metrics.bytes,
            preexisting_reused_objects: plan.reused_objects,
            preexisting_reused_bytes: plan.reused_bytes,
            admission_transactions: admission.transactions,
            max_transaction_objects: admission.max_transaction_objects,
            max_transaction_bytes: admission.max_transaction_bytes,
        })?;
        crate::telemetry::record_layerstack_initialization(
            crate::LayerStackInitializationReceipt {
                layer_stack_id,
                scanned_files,
                scanned_bytes,
            },
        );
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

fn directory_root(path: &std::path::Path, seed: [u8; 32]) -> Result<(BuiltRoot, u64, u64)> {
    let mut objects = ObjectBuffer::empty_all_reachable()?;
    let imported = match parallel_root_directories(path, seed, &mut objects)? {
        Some(imported) => imported,
        None => {
            let mut import = NativeImport::new(seed, &mut objects);
            import.directory(path, &layerfs_content::CanonicalPath::root(), true)?;
            import.finish()?
        }
    };
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
}

struct PreparedDirectory {
    name: layerfs_content::CanonicalName,
    inode: layerfs_content::tree::inode::InodeId,
    imported: ImportedTree,
    objects: DeferredObjectStore,
}

struct RootDirectoryTask {
    name: layerfs_content::CanonicalName,
    logical: layerfs_content::CanonicalPath,
    native: std::path::PathBuf,
}

fn parallel_root_directories(
    native: &std::path::Path,
    seed: [u8; 32],
    objects: &mut ObjectBuffer<'_>,
) -> Result<Option<ImportedTree>> {
    use layerfs_content::filesystem;
    use layerfs_content::tree::inode::{InodeId, InodeKind, InodeRecordV1};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

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
    let chunk_size = tasks.len().div_ceil(workers);
    let prepared = std::thread::scope(|scope| -> Result<Vec<PreparedDirectory>> {
        let handles = tasks
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    let mut output = Vec::with_capacity(chunk.len());
                    for task in chunk {
                        let mut local = ObjectBuffer::empty_all_reachable()?;
                        let mut import = NativeImport::new(seed, &mut local);
                        let inode = import.directory(&task.native, &task.logical, false)?;
                        output.push(PreparedDirectory {
                            name: task.name.clone(),
                            inode,
                            imported: import.finish()?,
                            objects: local.into_prevalidated()?,
                        });
                    }
                    Ok::<_, StoreError>(output)
                })
            })
            .collect::<Vec<_>>();
        let mut output = Vec::with_capacity(tasks.len());
        for handle in handles {
            output.extend(
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
        .flat_map(|directory| directory.imported.hard_links.iter())
        .any(|identity| !identities.insert(*identity))
    {
        return Ok(None);
    }

    let mut children = Vec::with_capacity(prepared.len());
    let mut mutations = Vec::new();
    let mut hard_links = Vec::new();
    let mut scanned_files = 0_u64;
    let mut scanned_bytes = 0_u64;
    for directory in prepared {
        objects.merge_prevalidated(directory.objects)?;
        children.push((directory.name, directory.inode));
        mutations.extend(directory.imported.mutations);
        hard_links.extend(directory.imported.hard_links);
        scanned_files = scanned_files
            .checked_add(directory.imported.scanned_files)
            .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
        scanned_bytes = scanned_bytes
            .checked_add(directory.imported.scanned_bytes)
            .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
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
    Ok(Some(ImportedTree {
        mutations,
        hard_links,
        scanned_files,
        scanned_bytes,
    }))
}

struct NativeImport<'objects, 'source> {
    seed: [u8; 32],
    objects: &'objects mut ObjectBuffer<'source>,
    hard_links:
        std::collections::HashMap<(u64, u64), (layerfs_content::tree::inode::InodeId, usize)>,
    records: Vec<Option<ImportedRecord>>,
    scanned_files: u64,
    scanned_bytes: u64,
}

impl<'objects, 'source> NativeImport<'objects, 'source> {
    fn new(seed: [u8; 32], objects: &'objects mut ObjectBuffer<'source>) -> Self {
        Self {
            seed,
            objects,
            hard_links: std::collections::HashMap::new(),
            records: Vec::new(),
            scanned_files: 0,
            scanned_bytes: 0,
        }
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

        let metadata = std::fs::symlink_metadata(native)?;
        let inode = if root {
            InodeId::allocate(self.seed, 0)
        } else {
            filesystem::allocated_inode(self.seed, logical)
        };
        let slot = self.reserve();
        let mut children = Vec::new();
        let mut entries = std::fs::read_dir(native)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by(|left, right| {
            left.file_name()
                .as_bytes()
                .cmp(right.file_name().as_bytes())
        });
        for entry in entries {
            let name = layerfs_content::CanonicalName::from_bytes(entry.file_name().as_bytes())?;
            let logical_path = child(logical, &name)?;
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
                let (content, counters) = layerfs_content::file::rope::build(
                    self.objects,
                    std::fs::File::open(entry.path())?,
                )?;
                self.scanned_files = self
                    .scanned_files
                    .checked_add(1)
                    .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
                self.scanned_bytes = self
                    .scanned_bytes
                    .checked_add(counters.cdc_bytes_scanned)
                    .ok_or(StoreError::Integrity("Layer initialization scan counter"))?;
                let metadata_root = filesystem::build_portable_metadata(
                    self.objects,
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
                let metadata_root = filesystem::build_portable_metadata(
                    self.objects,
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

        let content = filesystem::build_initial_directory(self.objects, children)?;
        let metadata_root = filesystem::build_portable_metadata(
            self.objects,
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
