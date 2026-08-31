use crate::ids::TypedId;
use crate::objects::{
    admit_planned_objects, empty_root, insert_object_batch, BuiltRoot, ObjectBuffer,
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
        let built = match source {
            LayerStackInitialization::Empty => empty_root(seed)?,
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
        let plan = self.db.plan_candidate(&built.objects)?;
        let mut statement_number = 0;
        let admission =
            admit_planned_objects(&self.db, &built.objects, &plan, &mut statement_number)?;
        let mut connection = self.db.writer()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let final_metrics =
            insert_object_batch(&transaction, &admission.final_batch, &mut statement_number)?;
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
        crate::telemetry::record_candidate(crate::CandidateReceipt {
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
        Ok(InitializeLayerStackResult {
            layer_stack_id,
            genesis_layer_id: layer.id,
        })
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

fn directory_root(path: &std::path::Path, seed: [u8; 32]) -> Result<BuiltRoot> {
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
    let mut scanned = 0;
    import_directory(
        path,
        &CanonicalPath::root(),
        seed,
        &mut objects,
        &mut root,
        &mut hard_links,
        &mut scanned,
    )?;
    objects.finish(root, scanned)
}

#[allow(clippy::too_many_arguments)]
fn import_directory(
    native: &std::path::Path,
    logical: &layerfs_content::CanonicalPath,
    seed: [u8; 32],
    objects: &mut ObjectBuffer<'_>,
    root: &mut layerfs_content::ObjectId,
    hard_links: &mut std::collections::HashMap<(u64, u64), layerfs_content::CanonicalPath>,
    scanned: &mut u64,
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
            import_directory(
                &entry.path(),
                &path,
                seed,
                objects,
                root,
                hard_links,
                scanned,
            )?;
        } else if metadata.file_type().is_file() {
            *root = filesystem::write_file(
                objects,
                *root,
                &path,
                std::fs::File::open(entry.path())?,
                metadata.permissions().mode(),
                seed,
            )?
            .root();
            *scanned = scanned.saturating_add(metadata.len());
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
