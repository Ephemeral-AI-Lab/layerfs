use crate::{
    AdmissionSetReceipt, BranchFact, BranchId, BranchRecord, BranchRecordPage, BranchScope,
    BranchScopePage, BranchScopeRecord, CanonicalObject, CommitHistoryPage, CommitId, CommitRecord,
    EntityName, Fact, FactKind, LayerId, LayerPrefixPage, LayerRecord, LayerStackFact,
    LayerStackId, LayerStackRecord, LayerStackRecordPage, LayerStackScopePage,
    LayerStackScopeRecord, MissingBitmap, ObjectSource, PinnedBranchRecord, RemotePlacement,
    Result, StorageError, StorageId, StoreDb, StoreRole, FACT_BATCH_BYTES, FACT_BATCH_COUNT,
    ID_BATCH_COUNT, OBJECT_BATCH_BYTES, OBJECT_BATCH_COUNT,
};
use layerfs_content::ObjectId;
use rusqlite::{
    params, params_from_iter, types::Value, OptionalExtension, Transaction, TransactionBehavior,
};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

impl StoreDb {
    pub fn read_object_row(&self, id: ObjectId) -> Result<Vec<u8>> {
        let bytes = self
            .connection()?
            .query_row(
                "SELECT bytes FROM objects WHERE object_id=?1",
                [id.as_bytes().as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .ok_or(StorageError::MissingObject(id))?;
        layerfs_content::authenticate_identity(&bytes, id)?;
        Ok(bytes)
    }

    pub fn has_object(&self, id: ObjectId) -> Result<bool> {
        Ok(self
            .connection()?
            .query_row(
                "SELECT 1 FROM objects WHERE object_id=?1",
                [id.as_bytes().as_slice()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn read_object_rows(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        if ids.len() > ID_BATCH_COUNT {
            return Err(StorageError::InvalidInput("object read page"));
        }
        let rows = self.existing_object_rows(ids)?;
        ids.iter()
            .map(|id| {
                let bytes = rows
                    .get(id)
                    .cloned()
                    .ok_or(StorageError::MissingObject(*id))?;
                layerfs_content::authenticate_identity(&bytes, *id)?;
                Ok(CanonicalObject { id: *id, bytes })
            })
            .collect()
    }

    pub fn existing_object_rows(&self, ids: &[ObjectId]) -> Result<BTreeMap<ObjectId, Vec<u8>>> {
        if ids.len() > ID_BATCH_COUNT {
            return Err(StorageError::InvalidInput("object read page"));
        }
        if ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        let connection = self.connection()?;
        existing_object_rows_on(&connection, ids)
    }

    pub fn missing_objects(&self, ids: &[ObjectId]) -> Result<MissingBitmap> {
        Ok(self.object_membership(ids)?.0)
    }

    pub fn object_membership(&self, ids: &[ObjectId]) -> Result<(MissingBitmap, Vec<Option<u64>>)> {
        if ids.len() > ID_BATCH_COUNT || ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(StorageError::InvalidInput("object membership page"));
        }
        if ids.is_empty() {
            return Ok((MissingBitmap::empty(), Vec::new()));
        }
        let sql = fixed_membership_sql("objects", "object_id", "object_id,length(bytes)");
        let mut values = ids
            .iter()
            .map(|id| Value::Blob(id.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        values.resize(ID_BATCH_COUNT, Value::Null);
        let connection = self.connection()?;
        let mut statement = connection.prepare_cached(&sql)?;
        let known = statement
            .query_map(params_from_iter(values), |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
            })?
            .map(|row| {
                let (id, bytes) = row?;
                let bytes =
                    u64::try_from(bytes).map_err(|_| StorageError::Integrity("object length"))?;
                Ok((ObjectId::from_bytes(&id)?, bytes))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let lengths = ids
            .iter()
            .map(|id| known.get(id).copied())
            .collect::<Vec<_>>();
        let missing = MissingBitmap::from_missing(
            lengths
                .iter()
                .enumerate()
                .filter_map(|(index, length)| length.is_none().then_some(index)),
        )?;
        Ok((missing, lengths))
    }

    pub fn admit_objects(&self, objects: &[CanonicalObject]) -> Result<AdmissionSetReceipt> {
        validate_object_batch(objects)?;
        for object in objects {
            layerfs_content::authenticate_identity(&object.bytes, object.id)?;
            crate::note_receiver_authentication();
        }
        let bytes = objects.iter().map(|object| object.bytes.len() as u64).sum();
        let total_started = Instant::now();
        let started = Instant::now();
        let mut connection = self.connection()?;
        let connection_wait_ns = elapsed_ns(started);
        let started = Instant::now();
        let known = existing_object_rows_on(
            &connection,
            &objects.iter().map(|object| object.id).collect::<Vec<_>>(),
        )?;
        for object in objects {
            if let Some(bytes) = known.get(&object.id) {
                if bytes != &object.bytes {
                    return Err(StorageError::Integrity("object collision"));
                }
            }
        }
        let mut statement_ns = elapsed_ns(started);
        let mut statement_count = 1_u64;
        let started = Instant::now();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let writer_acquire_ns = elapsed_ns(started);
        let mut receipt = AdmissionSetReceipt::default();
        let started = Instant::now();
        for object in objects {
            if known.contains_key(&object.id) {
                receipt.raced_existing_ids += 1;
                receipt.raced_existing_bytes += object.bytes.len() as u64;
                continue;
            }
            let inserted = transaction.execute(
                "INSERT INTO objects(object_id,bytes) VALUES(?1,?2) ON CONFLICT DO NOTHING",
                params![object.id.as_bytes().as_slice(), &object.bytes],
            )?;
            statement_count += 1;
            if inserted == 1 {
                receipt.inserted_ids += 1;
                receipt.inserted_bytes += object.bytes.len() as u64;
            } else {
                return Err(StorageError::Integrity("object admission race"));
            }
        }
        statement_ns = statement_ns.saturating_add(elapsed_ns(started));
        let started = Instant::now();
        transaction.commit()?;
        let commit_sync_ns = elapsed_ns(started);
        let total_ns = elapsed_ns(total_started);
        crate::record_database(database_receipt(
            self,
            crate::DatabaseOperation::ObjectAdmission,
            total_ns,
            connection_wait_ns,
            writer_acquire_ns,
            statement_ns,
            0,
            commit_sync_ns,
            statement_count,
            objects.len() as u64,
            bytes,
        ))?;
        crate::note_push_phase(crate::PushPhase::ObjectAdmission, total_ns);
        Ok(receipt)
    }

    pub fn missing_facts(&self, facts: &[Fact]) -> Result<MissingBitmap> {
        if facts.len() > ID_BATCH_COUNT
            || facts
                .windows(2)
                .any(|pair| pair[0].kind() != pair[1].kind() || pair[0].id() >= pair[1].id())
        {
            return Err(StorageError::InvalidInput("fact membership page"));
        }
        let Some(kind) = facts.first().map(Fact::kind) else {
            return Ok(MissingBitmap::empty());
        };
        let (table, key) = fact_table(self.role(), kind)?;
        let mut values = facts
            .iter()
            .map(Fact::id)
            .map(Value::Blob)
            .collect::<Vec<_>>();
        values.resize(ID_BATCH_COUNT, Value::Null);
        let connection = self.connection()?;
        let existing = existing_facts(&connection, table, key, kind, values)?;
        for fact in facts {
            if let Some(known) = existing.get(&fact.id()) {
                if known.signing_bytes() != fact.signing_bytes() {
                    return Err(StorageError::Integrity("fact collision"));
                }
            }
        }
        MissingBitmap::from_missing(
            facts
                .iter()
                .enumerate()
                .filter_map(|(index, fact)| (!existing.contains_key(&fact.id())).then_some(index)),
        )
    }

    pub fn admit_facts(&self, facts: &[Fact]) -> Result<AdmissionSetReceipt> {
        validate_fact_batch(self.role(), facts)?;
        for fact in facts {
            validate_fact(fact)?;
        }
        let encoded_sizes = facts
            .iter()
            .map(|fact| fact.encoded_size() as u64)
            .collect::<Vec<_>>();
        let bytes = encoded_sizes.iter().sum();
        let total_started = Instant::now();
        let started = Instant::now();
        let mut connection = self.connection()?;
        let connection_wait_ns = elapsed_ns(started);
        let started = Instant::now();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let writer_acquire_ns = elapsed_ns(started);
        let mut receipt = AdmissionSetReceipt::default();
        let started = Instant::now();
        for (fact, bytes) in facts.iter().zip(encoded_sizes) {
            let inserted = insert_fact(&transaction, self.role(), fact)?;
            if inserted {
                receipt.inserted_ids += 1;
                receipt.inserted_bytes += bytes;
            } else {
                receipt.raced_existing_ids += 1;
                receipt.raced_existing_bytes += bytes;
            }
        }
        let statement_ns = elapsed_ns(started);
        let started = Instant::now();
        transaction.commit()?;
        let commit_sync_ns = elapsed_ns(started);
        let total_ns = elapsed_ns(total_started);
        crate::record_database(database_receipt(
            self,
            crate::DatabaseOperation::FactAdmission,
            total_ns,
            connection_wait_ns,
            writer_acquire_ns,
            statement_ns,
            0,
            commit_sync_ns,
            facts.len() as u64,
            facts.len() as u64,
            bytes,
        ))?;
        crate::note_push_phase(crate::PushPhase::FactAdmission, total_ns);
        Ok(receipt)
    }

    pub fn layer_stack_fact(&self, id: LayerStackId) -> Result<Option<LayerStackFact>> {
        self.connection()?
            .query_row(
                "SELECT name FROM layer_stacks WHERE layer_stack_id=?1",
                [id.as_slice()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|name| {
                Ok(LayerStackFact {
                    id,
                    name: EntityName::new(name)?,
                })
            })
            .transpose()
    }

    pub fn layer_stack(&self, id: LayerStackId) -> Result<Option<LayerStackRecord>> {
        let sql = match self.role() {
            StoreRole::LayerStack => {
                "SELECT s.name,s.head_layer_id FROM layer_stacks s WHERE s.layer_stack_id=?1"
            }
            StoreRole::Branch => {
                "SELECT s.name,p.through_layer_id FROM layer_stacks s
                 JOIN layer_stack_scopes p USING(layer_stack_id) WHERE s.layer_stack_id=?1"
            }
        };
        self.connection()?
            .query_row(sql, [id.as_slice()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .optional()?
            .map(|(name, head)| {
                Ok(LayerStackRecord {
                    id,
                    name: EntityName::new(name)?,
                    head_layer_id: LayerId::from_slice(&head)?,
                })
            })
            .transpose()
    }

    pub fn layer(&self, id: LayerId) -> Result<Option<LayerRecord>> {
        self.connection()?
            .query_row(
                "SELECT layer_stack_id,parent_layer_id,root_id,source_branch_id,source_commit_id
                 FROM layers WHERE layer_id=?1",
                [id.as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Option<Vec<u8>>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Option<Vec<u8>>>(3)?,
                        row.get::<_, Option<Vec<u8>>>(4)?,
                    ))
                },
            )
            .optional()?
            .map(|(stack, parent, root, source_branch, source_commit)| {
                Ok(LayerRecord {
                    id,
                    layer_stack_id: LayerStackId::from_slice(&stack)?,
                    parent_layer_id: parent.as_deref().map(LayerId::from_slice).transpose()?,
                    root_id: ObjectId::from_bytes(&root)?,
                    source_branch_id: source_branch
                        .as_deref()
                        .map(BranchId::from_slice)
                        .transpose()?,
                    source_commit_id: source_commit
                        .as_deref()
                        .map(CommitId::from_slice)
                        .transpose()?,
                })
            })
            .transpose()
    }

    pub fn commit(&self, id: CommitId) -> Result<Option<CommitRecord>> {
        self.connection()?
            .query_row(
                "SELECT root_id,parent_commit_id,base_layer_id FROM commits WHERE commit_id=?1",
                [id.as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Option<Vec<u8>>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()?
            .map(|(root, parent, base)| {
                Ok(CommitRecord {
                    id,
                    root_id: ObjectId::from_bytes(&root)?,
                    parent_commit_id: parent.as_deref().map(CommitId::from_slice).transpose()?,
                    base_layer_id: LayerId::from_slice(&base)?,
                })
            })
            .transpose()
    }

    pub fn branch_fact(&self, id: BranchId) -> Result<Option<BranchFact>> {
        let connection = self.connection()?;
        branch_fact_on(&connection, id)
    }

    pub fn branch(&self, id: BranchId) -> Result<Option<BranchRecord>> {
        let connection = self.connection()?;
        if self.role() == StoreRole::Branch
            && connection
                .query_row(
                    "SELECT 1 FROM branch_scopes WHERE branch_id=?1",
                    [id.as_slice()],
                    |_| Ok(()),
                )
                .optional()?
                .is_none()
        {
            return Ok(None);
        }
        branch_row_on(&connection, id)
    }

    pub fn layer_stack_record_page(
        &self,
        after: Option<LayerStackId>,
        limit: u16,
    ) -> Result<LayerStackRecordPage> {
        validate_record_limit(limit)?;
        let sql = match self.role() {
            StoreRole::LayerStack => {
                "SELECT s.layer_stack_id,s.name,s.head_layer_id FROM layer_stacks s
                 WHERE s.layer_stack_id>?1
                 ORDER BY s.layer_stack_id LIMIT ?2"
            }
            StoreRole::Branch => {
                "SELECT s.layer_stack_id,s.name,p.through_layer_id FROM layer_stacks s
                 JOIN layer_stack_scopes p USING(layer_stack_id)
                 WHERE s.layer_stack_id>?1
                 ORDER BY s.layer_stack_id LIMIT ?2"
            }
        };
        let connection = self.connection()?;
        let mut statement = connection.prepare_cached(sql)?;
        let rows = statement.query_map(
            params![
                after.map(|id| id.to_bytes().to_vec()).unwrap_or_default(),
                i64::from(limit) + 1
            ],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )?;
        let mut records = rows
            .map(|row| {
                let (id, name, head) = row?;
                Ok(LayerStackRecord {
                    id: LayerStackId::from_slice(&id)?,
                    name: EntityName::new(name)?,
                    head_layer_id: LayerId::from_slice(&head)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let has_more = records.len() > usize::from(limit);
        records.truncate(usize::from(limit));
        let continuation = has_more
            .then(|| records.last().map(|record| record.id))
            .flatten();
        Ok(LayerStackRecordPage {
            records,
            continuation,
        })
    }

    pub fn branch_record_page(
        &self,
        layer_stack_id: Option<LayerStackId>,
        after: Option<BranchId>,
        limit: u16,
    ) -> Result<BranchRecordPage> {
        validate_record_limit(limit)?;
        let sql = match (self.role(), layer_stack_id.is_some()) {
            (StoreRole::LayerStack, true) => {
                "SELECT b.branch_id,b.layer_stack_id,b.name,b.base_layer_id,b.head_commit_id,
                        b.forked_from_layer_id,b.forked_from_branch_id,b.forked_from_commit_id
                 FROM branches b
                 WHERE b.layer_stack_id=?1 AND b.branch_id>?2
                 ORDER BY b.branch_id LIMIT ?3"
            }
            (StoreRole::LayerStack, false) => {
                "SELECT b.branch_id,b.layer_stack_id,b.name,b.base_layer_id,b.head_commit_id,
                        b.forked_from_layer_id,b.forked_from_branch_id,b.forked_from_commit_id
                 FROM branches b WHERE b.branch_id>?2 ORDER BY b.branch_id LIMIT ?3"
            }
            (StoreRole::Branch, true) => {
                "SELECT b.branch_id,b.layer_stack_id,b.name,b.base_layer_id,b.head_commit_id,
                        b.forked_from_layer_id,b.forked_from_branch_id,b.forked_from_commit_id
                 FROM branches b JOIN branch_scopes p USING(branch_id)
                 WHERE b.layer_stack_id=?1 AND b.branch_id>?2
                 ORDER BY b.branch_id LIMIT ?3"
            }
            (StoreRole::Branch, false) => {
                "SELECT b.branch_id,b.layer_stack_id,b.name,b.base_layer_id,b.head_commit_id,
                        b.forked_from_layer_id,b.forked_from_branch_id,b.forked_from_commit_id
                 FROM branches b JOIN branch_scopes p USING(branch_id)
                 WHERE b.branch_id>?2 ORDER BY b.branch_id LIMIT ?3"
            }
        };
        let connection = self.connection()?;
        let mut statement = connection.prepare_cached(sql)?;
        let rows = statement.query_map(
            params![
                layer_stack_id.map(|id| id.to_bytes().to_vec()),
                after.map(|id| id.to_bytes().to_vec()).unwrap_or_default(),
                i64::from(limit) + 1
            ],
            branch_record_raw,
        )?;
        let mut records = rows
            .map(|row| decode_branch_record(row?))
            .collect::<Result<Vec<_>>>()?;
        let has_more = records.len() > usize::from(limit);
        records.truncate(usize::from(limit));
        let continuation = has_more
            .then(|| records.last().map(|record| record.id))
            .flatten();
        Ok(BranchRecordPage {
            records,
            continuation,
        })
    }

    pub fn layer_stack_scope_page(
        &self,
        after: Option<LayerStackId>,
        limit: u16,
    ) -> Result<LayerStackScopePage> {
        if self.role() != StoreRole::Branch {
            return Err(StorageError::WrongStoreRole);
        }
        validate_record_limit(limit)?;
        let connection = self.connection()?;
        let mut statement = connection.prepare_cached(
            "SELECT s.layer_stack_id,s.name,p.through_layer_id,p.serving_mode
             FROM layer_stacks s JOIN layer_stack_scopes p USING(layer_stack_id)
             WHERE s.layer_stack_id>?1
             ORDER BY s.layer_stack_id LIMIT ?2",
        )?;
        let rows = statement.query_map(
            params![
                after.map(|id| id.to_bytes().to_vec()).unwrap_or_default(),
                i64::from(limit) + 1
            ],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )?;
        let mut records = rows
            .map(|row| {
                let (id, name, through, mode) = row?;
                let id = LayerStackId::from_slice(&id)?;
                Ok((
                    LayerStackFact {
                        id,
                        name: EntityName::new(name)?,
                    },
                    LayerStackScopeRecord {
                        layer_stack_id: id,
                        through_layer_id: LayerId::from_slice(&through)?,
                        serving_mode: RemotePlacement::parse(&mode)?,
                    },
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let has_more = records.len() > usize::from(limit);
        records.truncate(usize::from(limit));
        let continuation = has_more
            .then(|| records.last().map(|(fact, _)| fact.id))
            .flatten();
        Ok(LayerStackScopePage {
            records,
            continuation,
        })
    }

    pub fn branch_scope_page(
        &self,
        layer_stack_id: Option<LayerStackId>,
        after: Option<BranchId>,
        limit: u16,
    ) -> Result<BranchScopePage> {
        if self.role() != StoreRole::Branch {
            return Err(StorageError::WrongStoreRole);
        }
        validate_record_limit(limit)?;
        let sql = if layer_stack_id.is_some() {
            "SELECT b.branch_id,b.layer_stack_id,b.name,b.base_layer_id,b.head_commit_id,
                    b.forked_from_layer_id,b.forked_from_branch_id,b.forked_from_commit_id,
                    p.scope_kind,p.through_commit_id,p.serving_mode
             FROM branches b JOIN branch_scopes p USING(branch_id)
             WHERE b.layer_stack_id=?1 AND b.branch_id>?2 ORDER BY b.branch_id LIMIT ?3"
        } else {
            "SELECT b.branch_id,b.layer_stack_id,b.name,b.base_layer_id,b.head_commit_id,
                    b.forked_from_layer_id,b.forked_from_branch_id,b.forked_from_commit_id,
                    p.scope_kind,p.through_commit_id,p.serving_mode
             FROM branches b JOIN branch_scopes p USING(branch_id)
             WHERE b.branch_id>?2 ORDER BY b.branch_id LIMIT ?3"
        };
        let connection = self.connection()?;
        let mut statement = connection.prepare_cached(sql)?;
        let rows = statement.query_map(
            params![
                layer_stack_id.map(|id| id.to_bytes().to_vec()),
                after.map(|id| id.to_bytes().to_vec()).unwrap_or_default(),
                i64::from(limit) + 1
            ],
            |row| {
                let branch = branch_record_raw(row)?;
                Ok((
                    branch,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<Vec<u8>>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            },
        )?;
        let mut records = rows
            .map(|row| {
                let (branch, kind, through, mode) = row?;
                let branch = decode_branch_record(branch)?;
                let scope = decode_branch_scope(branch.id, &kind, through, mode)?;
                Ok((branch, scope))
            })
            .collect::<Result<Vec<_>>>()?;
        let has_more = records.len() > usize::from(limit);
        records.truncate(usize::from(limit));
        let continuation = has_more
            .then(|| records.last().map(|(record, _)| record.id))
            .flatten();
        Ok(BranchScopePage {
            records,
            continuation,
        })
    }

    pub fn insert_layerstack_genesis(
        &self,
        stack: &LayerStackRecord,
        layer: &LayerRecord,
    ) -> Result<()> {
        if self.role() != StoreRole::LayerStack
            || stack.head_layer_id != layer.id
            || layer.layer_stack_id != stack.id
            || layer.parent_layer_id.is_some()
            || layer.source_branch_id.is_some()
            || layer.source_commit_id.is_some()
        {
            return Err(StorageError::InvalidInput("LayerStack genesis"));
        }
        EntityName::validate(stack.name.as_str())?;
        validate_layer_identity(layer)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        if let Some(known) = layer_stack_fact_on(&transaction, stack.id)? {
            if known != stack.fact() {
                return Err(StorageError::Integrity("LayerStack collision"));
            }
            let head: Vec<u8> = transaction.query_row(
                "SELECT head_layer_id FROM layer_stacks WHERE layer_stack_id=?1",
                [stack.id.as_slice()],
                |row| row.get(0),
            )?;
            if LayerId::from_slice(&head)? != stack.head_layer_id {
                return Err(StorageError::Integrity("LayerStack pointer collision"));
            }
            insert_layer(&transaction, *layer)?;
            transaction.commit()?;
            return Ok(());
        }
        reject_layer_stack_name_conflict(&transaction, &stack.fact())?;
        transaction.execute(
            "INSERT INTO layer_stacks(layer_stack_id,name,head_layer_id) VALUES(?1,?2,?3)",
            params![
                stack.id.as_slice(),
                stack.name.as_str(),
                stack.head_layer_id.as_slice()
            ],
        )?;
        insert_layer(&transaction, *layer)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn insert_layer_fact(&self, layer: LayerRecord) -> Result<bool> {
        validate_layer_identity(&layer)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let inserted = insert_layer(&transaction, layer)?;
        transaction.commit()?;
        Ok(inserted)
    }

    pub fn insert_commit_fact(&self, commit: CommitRecord) -> Result<bool> {
        validate_commit_identity(&commit)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let inserted = insert_commit(&transaction, commit)?;
        transaction.commit()?;
        Ok(inserted)
    }

    pub fn insert_branch_fact(&self, branch: &BranchFact) -> Result<bool> {
        EntityName::validate(branch.name.as_str())?;
        branch.validate_origin()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let inserted = insert_branch_fact(&transaction, self.role(), branch)?;
        transaction.commit()?;
        Ok(inserted)
    }

    pub fn publish_layer_stack_scope(&self, scope: LayerStackScopeRecord) -> Result<()> {
        if self.role() != StoreRole::Branch {
            return Err(StorageError::WrongStoreRole);
        }
        let mut connection = self.connection()?;
        let layer = layer_row_on(&connection, scope.through_layer_id)?
            .ok_or(StorageError::NotFound("LayerStack scope Layer"))?;
        if layer.layer_stack_id != scope.layer_stack_id {
            return Err(StorageError::Integrity("LayerStack scope ownership"));
        }
        if let Some(existing) = layer_stack_scope_on(&connection, scope.layer_stack_id)? {
            if existing.through_layer_id != scope.through_layer_id
                && !layer_is_ancestor_on(
                    &connection,
                    existing.through_layer_id,
                    scope.through_layer_id,
                )?
            {
                return Err(StorageError::LayerHeadMoved {
                    expected: existing.through_layer_id,
                    actual: scope.through_layer_id,
                });
            }
        }
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO layer_stack_scopes(layer_stack_id,through_layer_id,serving_mode)
             VALUES(?1,?2,?3)
             ON CONFLICT(layer_stack_id) DO UPDATE SET
                 through_layer_id=excluded.through_layer_id,
                 serving_mode=excluded.serving_mode",
            params![
                scope.layer_stack_id.as_slice(),
                scope.through_layer_id.as_slice(),
                scope.serving_mode.as_str()
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn layer_stack_scope(
        &self,
        layer_stack_id: LayerStackId,
    ) -> Result<Option<LayerStackScopeRecord>> {
        if self.role() != StoreRole::Branch {
            return Err(StorageError::WrongStoreRole);
        }
        let connection = self.connection()?;
        layer_stack_scope_on(&connection, layer_stack_id)
    }

    pub fn publish_local_branch(&self, branch: &BranchRecord) -> Result<()> {
        self.publish_branch_scope(branch, BranchScope::Local)
    }

    pub fn publish_remote_branch_scope(
        &self,
        branch: &BranchRecord,
        scope: BranchScopeRecord,
    ) -> Result<()> {
        if scope.branch_id != branch.id {
            return Err(StorageError::InvalidInput("Branch scope identity"));
        }
        self.publish_branch_scope(branch, scope.scope)
    }

    fn publish_branch_scope(&self, branch: &BranchRecord, scope: BranchScope) -> Result<()> {
        if self.role() != StoreRole::Branch {
            return Err(StorageError::WrongStoreRole);
        }
        EntityName::validate(branch.name.as_str())?;
        branch.fact().validate_origin()?;
        match scope {
            BranchScope::Local => {}
            BranchScope::Remote {
                through_commit_id, ..
            } if branch.head_commit_id == Some(through_commit_id) => {}
            BranchScope::Remote { .. } => {
                return Err(StorageError::InvalidInput("Branch scope boundary"));
            }
        }
        let mut connection = self.connection()?;
        if let Some(head) = branch.head_commit_id {
            let commit = commit_row_on(&connection, head)?
                .ok_or(StorageError::NotFound("Branch head Commit"))?;
            if commit.base_layer_id != branch.base_layer_id {
                return Err(StorageError::Integrity("Branch head base"));
            }
        }
        if let Some(existing) = branch_scope_on(&connection, branch.id)? {
            match (existing.scope, scope) {
                (BranchScope::Local, BranchScope::Local) => {
                    if branch_row_on(&connection, branch.id)?.as_ref() != Some(branch) {
                        return Err(StorageError::Integrity("local Branch publication"));
                    }
                    return Ok(());
                }
                (BranchScope::Local, BranchScope::Remote { .. }) => {
                    return Err(StorageError::Integrity("local Branch ownership"));
                }
                (BranchScope::Remote { .. }, BranchScope::Local) => {
                    return Err(StorageError::ReadOnlyBranch(branch.id));
                }
                (
                    BranchScope::Remote {
                        through_commit_id: current,
                        ..
                    },
                    BranchScope::Remote {
                        through_commit_id: incoming,
                        ..
                    },
                ) if current != incoming
                    && !commit_is_ancestor_on(&connection, current, incoming)? =>
                {
                    return Err(StorageError::CommitHeadMoved {
                        expected: Some(current),
                        actual: Some(incoming),
                    });
                }
                _ => {}
            }
        }
        let transaction = connection.transaction()?;
        publish_branch_record(&transaction, branch)?;
        let (kind, through, mode) = match scope {
            BranchScope::Local => ("local", None, None),
            BranchScope::Remote {
                through_commit_id,
                serving_mode,
            } => (
                "remote",
                Some(through_commit_id.to_bytes().to_vec()),
                Some(serving_mode.as_str()),
            ),
        };
        transaction.execute(
            "INSERT INTO branch_scopes(branch_id,scope_kind,through_commit_id,serving_mode)
             VALUES(?1,?2,?3,?4)
             ON CONFLICT(branch_id) DO UPDATE SET scope_kind=excluded.scope_kind,
                 through_commit_id=excluded.through_commit_id,
                 serving_mode=excluded.serving_mode",
            params![branch.id.as_slice(), kind, through, mode],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn branch_scope(&self, branch_id: BranchId) -> Result<Option<BranchScopeRecord>> {
        if self.role() != StoreRole::Branch {
            return Err(StorageError::WrongStoreRole);
        }
        let connection = self.connection()?;
        branch_scope_on(&connection, branch_id)
    }

    pub fn pin_branch(&self, branch_id: BranchId) -> Result<Option<PinnedBranchRecord>> {
        if self.role() != StoreRole::Branch {
            return Err(StorageError::WrongStoreRole);
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let Some(branch) = branch_row_on(&transaction, branch_id)? else {
            return Ok(None);
        };
        let Some(scope) = branch_scope_on(&transaction, branch_id)? else {
            return Ok(None);
        };
        if let BranchScope::Remote {
            through_commit_id, ..
        } = scope.scope
        {
            if branch.head_commit_id != Some(through_commit_id) {
                return Err(StorageError::Integrity("remote Branch scope pointer"));
            }
        }
        let root_id = match branch.head_commit_id {
            Some(commit_id) => {
                commit_row_on(&transaction, commit_id)?
                    .ok_or(StorageError::Integrity("Branch head Commit"))?
                    .root_id
            }
            None => {
                layer_row_on(&transaction, branch.base_layer_id)?
                    .ok_or(StorageError::Integrity("Branch base Layer"))?
                    .root_id
            }
        };
        let pinned = PinnedBranchRecord {
            branch,
            scope,
            root_id,
        };
        transaction.commit()?;
        Ok(Some(pinned))
    }

    pub fn verify_and_record_complete_roots<I>(&self, roots: I) -> Result<u64>
    where
        I: IntoIterator<Item = ObjectId>,
    {
        if self.role() != StoreRole::Branch {
            return Err(StorageError::WrongStoreRole);
        }
        let objects = LocalObjects(self);
        let mut verifier = crate::admission::RootVerifier::new(&objects)?;
        let mut inserted = 0_u64;
        for root in roots {
            verifier.verify(root)?;
            inserted += self.connection()?.execute(
                "INSERT INTO complete_roots(root_id) VALUES(?1) ON CONFLICT DO NOTHING",
                [root.as_bytes().as_slice()],
            )? as u64;
        }
        Ok(inserted)
    }

    pub fn verify_complete_roots<I>(&self, roots: I) -> Result<()>
    where
        I: IntoIterator<Item = ObjectId>,
    {
        let objects = LocalObjects(self);
        let mut verifier = crate::admission::RootVerifier::new(&objects)?;
        for root in roots {
            verifier.verify(root)?;
        }
        Ok(())
    }

    pub fn verify_complete_transition(&self, old: ObjectId, new: ObjectId) -> Result<()> {
        let objects = LocalObjects(self);
        let mut seen = crate::SpillableObjectSet::empty()?;
        let mut active = BTreeSet::new();
        verify_transition(&objects, Some(old), new, &mut seen, &mut active)
    }

    pub fn complete_root(&self, root: ObjectId) -> Result<bool> {
        if self.role() != StoreRole::Branch {
            return Err(StorageError::WrongStoreRole);
        }
        Ok(self
            .connection()?
            .query_row(
                "SELECT 1 FROM complete_roots WHERE root_id=?1",
                [root.as_bytes().as_slice()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn branch_effective_root(&self, branch: &BranchRecord) -> Result<ObjectId> {
        match branch.head_commit_id {
            Some(id) => self
                .commit(id)?
                .map(|commit| commit.root_id)
                .ok_or(StorageError::NotFound("Branch head Commit")),
            None => self
                .layer(branch.base_layer_id)?
                .map(|layer| layer.root_id)
                .ok_or(StorageError::NotFound("Branch base Layer")),
        }
    }

    pub fn lane_contains(&self, branch: &BranchRecord, commit_id: CommitId) -> Result<bool> {
        let Some(mut cursor) = branch.head_commit_id else {
            return Ok(false);
        };
        let boundary = branch.forked_from_commit_id;
        loop {
            if cursor == commit_id {
                return Ok(true);
            }
            if Some(cursor) == boundary {
                return Ok(false);
            }
            let commit = self
                .commit(cursor)?
                .ok_or(StorageError::Integrity("Branch lane Commit"))?;
            let Some(parent) = commit.parent_commit_id else {
                return Ok(false);
            };
            cursor = parent;
        }
    }

    pub fn layer_prefix_page(
        &self,
        through_layer_id: LayerId,
        cursor: Option<LayerId>,
        limit: u16,
    ) -> Result<LayerPrefixPage> {
        self.layer_ancestry_page(through_layer_id, None, cursor, limit)
    }

    pub fn layer_ancestry_page(
        &self,
        through_layer_id: LayerId,
        stop_exclusive: Option<LayerId>,
        cursor: Option<LayerId>,
        limit: u16,
    ) -> Result<LayerPrefixPage> {
        validate_history_limit(limit)?;
        let through = self
            .layer(through_layer_id)?
            .ok_or(StorageError::NotFound("Layer prefix boundary"))?;
        let mut next = cursor.unwrap_or(through_layer_id);
        if Some(next) == stop_exclusive {
            return Ok(LayerPrefixPage {
                records: Vec::new(),
                continuation: None,
            });
        }
        let mut records = Vec::with_capacity(usize::from(limit));
        while records.len() < usize::from(limit) {
            let layer = self
                .layer(next)?
                .ok_or(StorageError::Integrity("Layer prefix"))?;
            if layer.layer_stack_id != through.layer_stack_id {
                return Err(StorageError::Integrity("Layer prefix ownership"));
            }
            let parent = layer.parent_layer_id;
            records.push(layer);
            let Some(parent) = parent else {
                return Ok(LayerPrefixPage {
                    records,
                    continuation: None,
                });
            };
            if Some(parent) == stop_exclusive {
                return Ok(LayerPrefixPage {
                    records,
                    continuation: None,
                });
            }
            next = parent;
        }
        Ok(LayerPrefixPage {
            records,
            continuation: Some(next),
        })
    }

    pub fn commit_ancestry_page(
        &self,
        through_commit_id: CommitId,
        stop_exclusive: Option<CommitId>,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitHistoryPage> {
        validate_history_limit(limit)?;
        let mut next = cursor.unwrap_or(through_commit_id);
        if Some(next) == stop_exclusive {
            return Ok(CommitHistoryPage {
                records: Vec::new(),
                continuation: None,
            });
        }
        let mut records = Vec::with_capacity(usize::from(limit));
        while records.len() < usize::from(limit) {
            let commit = self
                .commit(next)?
                .ok_or(StorageError::Integrity("Commit history"))?;
            let parent = commit.parent_commit_id;
            records.push(commit);
            let Some(parent) = parent else {
                return Ok(CommitHistoryPage {
                    records,
                    continuation: None,
                });
            };
            if Some(parent) == stop_exclusive {
                return Ok(CommitHistoryPage {
                    records,
                    continuation: None,
                });
            }
            next = parent;
        }
        Ok(CommitHistoryPage {
            records,
            continuation: Some(next),
        })
    }

    pub fn commit_history_page(
        &self,
        branch_id: BranchId,
        through_commit_id: CommitId,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitHistoryPage> {
        if cursor.is_none() {
            let branch = self
                .branch(branch_id)?
                .ok_or(StorageError::NotFound("Branch"))?;
            let head = branch
                .head_commit_id
                .ok_or(StorageError::NotFound("Branch head Commit"))?;
            if !self.commit_is_ancestor(through_commit_id, head)? {
                return Err(StorageError::NotFound("Commit in Branch history"));
            }
        }
        self.commit_ancestry_page(through_commit_id, None, cursor, limit)
    }

    pub fn owned_commit_page(
        &self,
        branch_id: BranchId,
        through_commit_id: CommitId,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitHistoryPage> {
        let branch = self
            .branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        if cursor.is_none() && !self.lane_contains(&branch, through_commit_id)? {
            return Err(StorageError::NotFound("Commit in Branch lane"));
        }
        self.commit_ancestry_page(
            through_commit_id,
            branch.forked_from_commit_id,
            cursor,
            limit,
        )
    }

    pub fn commit_is_ancestor(&self, ancestor: CommitId, descendant: CommitId) -> Result<bool> {
        let connection = self.connection()?;
        commit_is_ancestor_on(&connection, ancestor, descendant)
    }

    pub fn commit_branch(
        &self,
        branch_id: BranchId,
        expected_head: Option<CommitId>,
        expected_base: LayerId,
        commit: CommitRecord,
        new_base: LayerId,
        complete: bool,
    ) -> Result<()> {
        if self.role() != StoreRole::Branch
            || commit.parent_commit_id != expected_head
            || commit.base_layer_id != new_base
        {
            return Err(StorageError::InvalidInput("Branch Commit"));
        }
        validate_commit_identity(&commit)?;
        if complete {
            let started = Instant::now();
            let verified = self.verify_complete_roots([commit.root_id]);
            crate::note_workspace_commit_phase(
                crate::WorkspaceCommitPhase::CompletenessVerify,
                elapsed_ns(started),
            );
            verified?;
        }
        let scope = self
            .branch_scope(branch_id)?
            .ok_or(StorageError::NotFound("Branch scope"))?;
        if !matches!(scope.scope, BranchScope::Local) {
            return Err(StorageError::ReadOnlyBranch(branch_id));
        }
        let total_started = Instant::now();
        let started = Instant::now();
        let mut connection = self.connection()?;
        let connection_wait_ns = elapsed_ns(started);
        let started = Instant::now();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let writer_acquire_ns = elapsed_ns(started);
        let publication_started = Instant::now();
        let mut statement_count = 1_u64;
        let mut rows = u64::from(insert_commit(&transaction, commit)?);
        if complete {
            rows += transaction.execute(
                "INSERT INTO complete_roots(root_id) VALUES(?1) ON CONFLICT DO NOTHING",
                [commit.root_id.as_bytes().as_slice()],
            )? as u64;
            statement_count += 1;
        }
        let changed = transaction.execute(
            "UPDATE branches SET head_commit_id=?1,base_layer_id=?2
             WHERE branch_id=?3 AND head_commit_id IS ?4 AND base_layer_id=?5",
            params![
                commit.id.as_slice(),
                new_base.as_slice(),
                branch_id.as_slice(),
                expected_head.map(|id| id.to_bytes().to_vec()),
                expected_base.as_slice()
            ],
        )?;
        statement_count += 1;
        rows += changed as u64;
        if changed != 1 {
            let actual = branch_head(&transaction, branch_id)?;
            return Err(StorageError::CommitHeadMoved {
                expected: expected_head,
                actual,
            });
        }
        let publication_ns = elapsed_ns(publication_started);
        let started = Instant::now();
        transaction.commit()?;
        let commit_sync_ns = elapsed_ns(started);
        let total_ns = elapsed_ns(total_started);
        crate::record_database(database_receipt(
            self,
            crate::DatabaseOperation::CommitCas,
            total_ns,
            connection_wait_ns,
            writer_acquire_ns,
            0,
            publication_ns,
            commit_sync_ns,
            statement_count,
            rows,
            Fact::Commit(commit).encoded_size() as u64,
        ))?;
        crate::note_workspace_commit_phase(crate::WorkspaceCommitPhase::Publication, total_ns);
        Ok(())
    }

    pub fn authority_publish_branch(
        &self,
        branch: &BranchRecord,
        observed_head: Option<CommitId>,
    ) -> Result<crate::PushResult> {
        if self.role() != StoreRole::LayerStack {
            return Err(StorageError::WrongStoreRole);
        }
        let incoming = branch
            .head_commit_id
            .ok_or(StorageError::InvalidInput("pushed Branch head"))?;
        branch.fact().validate_origin()?;
        EntityName::validate(branch.name.as_str())?;
        let base = self
            .layer(branch.base_layer_id)?
            .ok_or(StorageError::NotFound("pushed Branch base Layer"))?;
        if base.layer_stack_id != branch.layer_stack_id {
            return Err(StorageError::Integrity("Branch ownership"));
        }
        let incoming_record = self
            .commit(incoming)?
            .ok_or(StorageError::NotFound("pushed Branch head Commit"))?;
        if incoming_record.base_layer_id != branch.base_layer_id {
            return Err(StorageError::Integrity("Branch head base"));
        }
        let observed_in_lane = observed_head
            .map(|observed| self.commit_in_lane(incoming, observed, branch.forked_from_commit_id))
            .transpose()?
            .unwrap_or(true);
        let total_started = Instant::now();
        let started = Instant::now();
        let mut connection = self.connection()?;
        let connection_wait_ns = elapsed_ns(started);
        let started = Instant::now();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let writer_acquire_ns = elapsed_ns(started);
        let publication_started = Instant::now();
        let mut statement_count = 2_u64;
        let mut rows = 0_u64;
        let current = branch_head(&transaction, branch.id)?;
        if let Some(existing) = branch_row(&transaction, branch.id)? {
            if !same_origin(&existing, branch) {
                return Err(StorageError::Integrity("Branch origin mismatch"));
            }
        } else {
            reject_branch_name_conflict(&transaction, &branch.fact())?;
            statement_count += 1;
        }
        let outcome = match current {
            None => {
                if observed_head.is_some() {
                    return Err(StorageError::Integrity("Push observed head"));
                }
                insert_branch(&transaction, branch)?;
                statement_count += 1;
                rows += 1;
                crate::PushResult::Created {
                    commit_id: incoming,
                }
            }
            Some(current) if current == incoming => crate::PushResult::UpToDate {
                commit_id: incoming,
            },
            Some(current) if Some(current) == observed_head => {
                if !observed_in_lane {
                    crate::PushResult::HeadMoved {
                        authority_head: current,
                        local_head: incoming,
                    }
                } else {
                    let changed = transaction.execute(
                        "UPDATE branches SET head_commit_id=?1,base_layer_id=?2
                         WHERE branch_id=?3 AND head_commit_id=?4",
                        params![
                            incoming.as_slice(),
                            branch.base_layer_id.as_slice(),
                            branch.id.as_slice(),
                            current.as_slice()
                        ],
                    )?;
                    statement_count += 1;
                    rows += changed as u64;
                    if changed != 1 {
                        return Err(StorageError::CommitHeadMoved {
                            expected: Some(current),
                            actual: branch_head(&transaction, branch.id)?,
                        });
                    }
                    crate::PushResult::Advanced {
                        previous: current,
                        commit_id: incoming,
                    }
                }
            }
            Some(authority_head) => crate::PushResult::HeadMoved {
                authority_head,
                local_head: incoming,
            },
        };
        let publication_ns = elapsed_ns(publication_started);
        let started = Instant::now();
        transaction.commit()?;
        let commit_sync_ns = elapsed_ns(started);
        let total_ns = elapsed_ns(total_started);
        crate::record_database(database_receipt(
            self,
            crate::DatabaseOperation::AuthorityPublish,
            total_ns,
            connection_wait_ns,
            writer_acquire_ns,
            0,
            publication_ns,
            commit_sync_ns,
            statement_count,
            rows,
            0,
        ))?;
        crate::note_push_phase(crate::PushPhase::Publication, total_ns);
        Ok(outcome)
    }

    fn commit_in_lane(
        &self,
        descendant: CommitId,
        candidate: CommitId,
        stop_exclusive: Option<CommitId>,
    ) -> Result<bool> {
        let mut cursor = descendant;
        loop {
            if Some(cursor) == stop_exclusive {
                return Ok(false);
            }
            if cursor == candidate {
                return Ok(true);
            }
            let commit = self
                .commit(cursor)?
                .ok_or(StorageError::Integrity("Branch lane Commit"))?;
            let Some(parent) = commit.parent_commit_id else {
                return Ok(false);
            };
            cursor = parent;
        }
    }

    pub fn layer_by_source(
        &self,
        branch_id: BranchId,
        commit_id: CommitId,
    ) -> Result<Option<LayerRecord>> {
        if self.role() != StoreRole::LayerStack {
            return Err(StorageError::WrongStoreRole);
        }
        let id = self
            .connection()?
            .query_row(
                "SELECT layer_id FROM layers WHERE source_branch_id=?1 AND source_commit_id=?2",
                params![branch_id.as_slice(), commit_id.as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?;
        id.as_deref()
            .map(LayerId::from_slice)
            .transpose()?
            .map_or(Ok(None), |id| self.layer(id))
    }

    pub fn add_layer_cas(&self, checked_head: LayerId, layer: LayerRecord) -> Result<()> {
        if self.role() != StoreRole::LayerStack || layer.parent_layer_id != Some(checked_head) {
            return Err(StorageError::InvalidInput("Add Layer"));
        }
        validate_layer_identity(&layer)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        insert_layer(&transaction, layer)?;
        let changed = transaction.execute(
            "UPDATE layer_stacks SET head_layer_id=?1
             WHERE layer_stack_id=?2 AND head_layer_id=?3",
            params![
                layer.id.as_slice(),
                layer.layer_stack_id.as_slice(),
                checked_head.as_slice()
            ],
        )?;
        if changed != 1 {
            let actual: Vec<u8> = transaction.query_row(
                "SELECT head_layer_id FROM layer_stacks WHERE layer_stack_id=?1",
                [layer.layer_stack_id.as_slice()],
                |row| row.get(0),
            )?;
            return Err(StorageError::LayerHeadMoved {
                expected: checked_head,
                actual: LayerId::from_slice(&actual)?,
            });
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn fact_page(
        &self,
        kind: FactKind,
        after: Option<&[u8]>,
        limit: u16,
    ) -> Result<(Vec<Fact>, Option<Vec<u8>>)> {
        if limit == 0 || limit > 512 {
            return Err(StorageError::InvalidInput("fact query page"));
        }
        if after.is_some_and(|value| value.len() != fact_id_length(kind)) {
            return Err(StorageError::InvalidInput("fact query cursor"));
        }
        let after = after.unwrap_or(&[]);
        let fetch = i64::from(limit) + 1;
        let connection = self.connection()?;
        let mut facts = match kind {
            FactKind::LayerStack => {
                let sql = if self.role() == StoreRole::Branch {
                    "SELECT s.layer_stack_id,s.name FROM layer_stacks s
                     JOIN layer_stack_scopes p USING(layer_stack_id)
                     WHERE s.layer_stack_id>?1 ORDER BY s.layer_stack_id LIMIT ?2"
                } else {
                    "SELECT layer_stack_id,name FROM layer_stacks
                     WHERE layer_stack_id>?1 ORDER BY layer_stack_id LIMIT ?2"
                };
                connection
                    .prepare_cached(sql)?
                    .query_map(params![after, fetch], |row| {
                        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
                    })?
                    .map(|row| {
                        let (id, name) = row?;
                        Ok(Fact::LayerStack(LayerStackFact {
                            id: LayerStackId::from_slice(&id)?,
                            name: EntityName::new(name)?,
                        }))
                    })
                    .collect::<Result<Vec<_>>>()?
            }
            FactKind::Branch => {
                let sql = if self.role() == StoreRole::Branch {
                    "SELECT b.branch_id,b.layer_stack_id,b.name,b.forked_from_layer_id,
                            b.forked_from_branch_id,b.forked_from_commit_id
                     FROM branches b JOIN branch_scopes p USING(branch_id)
                     WHERE b.branch_id>?1 ORDER BY b.branch_id LIMIT ?2"
                } else {
                    "SELECT branch_id,layer_stack_id,name,forked_from_layer_id,
                            forked_from_branch_id,forked_from_commit_id
                     FROM branches WHERE branch_id>?1 ORDER BY branch_id LIMIT ?2"
                };
                connection
                    .prepare_cached(sql)?
                    .query_map(params![after, fetch], |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, Vec<u8>>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<Vec<u8>>>(3)?,
                            row.get::<_, Option<Vec<u8>>>(4)?,
                            row.get::<_, Option<Vec<u8>>>(5)?,
                        ))
                    })?
                    .map(|row| {
                        let (id, stack, name, from_layer, from_branch, from_commit) = row?;
                        Ok(Fact::Branch(BranchFact {
                            id: BranchId::from_slice(&id)?,
                            layer_stack_id: LayerStackId::from_slice(&stack)?,
                            name: EntityName::new(name)?,
                            forked_from_layer_id: from_layer
                                .as_deref()
                                .map(LayerId::from_slice)
                                .transpose()?,
                            forked_from_branch_id: from_branch
                                .as_deref()
                                .map(BranchId::from_slice)
                                .transpose()?,
                            forked_from_commit_id: from_commit
                                .as_deref()
                                .map(CommitId::from_slice)
                                .transpose()?,
                        }))
                    })
                    .collect::<Result<Vec<_>>>()?
            }
            FactKind::Layer => {
                let sql = if self.role() == StoreRole::Branch {
                    "WITH RECURSIVE visible(id) AS (
                     SELECT through_layer_id FROM layer_stack_scopes
                     UNION
                     SELECT l.parent_layer_id FROM layers l JOIN visible v ON l.layer_id=v.id
                     WHERE l.parent_layer_id IS NOT NULL
                 )
                     SELECT l.layer_id,l.layer_stack_id,l.parent_layer_id,l.root_id,
                            l.source_branch_id,l.source_commit_id
                     FROM layers l JOIN visible v ON v.id=l.layer_id
                     WHERE l.layer_id>?1 ORDER BY l.layer_id LIMIT ?2"
                } else {
                    "SELECT layer_id,layer_stack_id,parent_layer_id,root_id,
                            source_branch_id,source_commit_id
                     FROM layers WHERE layer_id>?1 ORDER BY layer_id LIMIT ?2"
                };
                connection
                    .prepare_cached(sql)?
                    .query_map(params![after, fetch], |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, Vec<u8>>(1)?,
                            row.get::<_, Option<Vec<u8>>>(2)?,
                            row.get::<_, Vec<u8>>(3)?,
                            row.get::<_, Option<Vec<u8>>>(4)?,
                            row.get::<_, Option<Vec<u8>>>(5)?,
                        ))
                    })?
                    .map(|row| {
                        let (id, stack, parent, root, source_branch, source_commit) = row?;
                        Ok(Fact::Layer(LayerRecord {
                            id: LayerId::from_slice(&id)?,
                            layer_stack_id: LayerStackId::from_slice(&stack)?,
                            parent_layer_id: parent
                                .as_deref()
                                .map(LayerId::from_slice)
                                .transpose()?,
                            root_id: ObjectId::from_bytes(&root)?,
                            source_branch_id: source_branch
                                .as_deref()
                                .map(BranchId::from_slice)
                                .transpose()?,
                            source_commit_id: source_commit
                                .as_deref()
                                .map(CommitId::from_slice)
                                .transpose()?,
                        }))
                    })
                    .collect::<Result<Vec<_>>>()?
            }
            FactKind::Commit => {
                let sql = if self.role() == StoreRole::Branch {
                    "WITH RECURSIVE visible(id) AS (
                     SELECT b.head_commit_id FROM branches b JOIN branch_scopes p USING(branch_id)
                     WHERE b.head_commit_id IS NOT NULL
                     UNION
                     SELECT c.parent_commit_id FROM commits c JOIN visible v ON c.commit_id=v.id
                     WHERE c.parent_commit_id IS NOT NULL
                 )
                     SELECT c.commit_id,c.root_id,c.parent_commit_id,c.base_layer_id
                     FROM commits c JOIN visible v ON v.id=c.commit_id
                     WHERE c.commit_id>?1 ORDER BY c.commit_id LIMIT ?2"
                } else {
                    "SELECT commit_id,root_id,parent_commit_id,base_layer_id
                     FROM commits WHERE commit_id>?1 ORDER BY commit_id LIMIT ?2"
                };
                connection
                    .prepare_cached(sql)?
                    .query_map(params![after, fetch], |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, Vec<u8>>(1)?,
                            row.get::<_, Option<Vec<u8>>>(2)?,
                            row.get::<_, Vec<u8>>(3)?,
                        ))
                    })?
                    .map(|row| {
                        let (id, root, parent, base) = row?;
                        Ok(Fact::Commit(CommitRecord {
                            id: CommitId::from_slice(&id)?,
                            root_id: ObjectId::from_bytes(&root)?,
                            parent_commit_id: parent
                                .as_deref()
                                .map(CommitId::from_slice)
                                .transpose()?,
                            base_layer_id: LayerId::from_slice(&base)?,
                        }))
                    })
                    .collect::<Result<Vec<_>>>()?
            }
        };
        let has_more = facts.len() > usize::from(limit);
        facts.truncate(usize::from(limit));
        let continuation = has_more.then(|| facts.last().expect("nonempty fact page").id());
        Ok((facts, continuation))
    }
}

const fn fact_id_length(kind: FactKind) -> usize {
    match kind {
        FactKind::Commit | FactKind::Layer => 33,
        FactKind::Branch | FactKind::LayerStack => 17,
    }
}

fn validate_object_batch(objects: &[CanonicalObject]) -> Result<()> {
    let bytes = objects
        .iter()
        .map(|object| object.bytes.len())
        .sum::<usize>();
    let max = layerfs_content::limits::MAX_OBJECT_BYTES;
    if objects.len() > OBJECT_BATCH_COUNT
        || objects
            .iter()
            .map(|object| object.id)
            .collect::<BTreeSet<_>>()
            .len()
            != objects.len()
        || objects.iter().any(|object| object.bytes.len() > max)
        || (objects.len() > 1 && bytes > OBJECT_BATCH_BYTES)
    {
        return Err(StorageError::InvalidInput("object batch"));
    }
    Ok(())
}

fn existing_object_rows_on(
    connection: &rusqlite::Connection,
    ids: &[ObjectId],
) -> Result<BTreeMap<ObjectId, Vec<u8>>> {
    let sql = fixed_membership_sql("objects", "object_id", "object_id,bytes");
    let mut values = ids
        .iter()
        .map(|id| Value::Blob(id.as_bytes().to_vec()))
        .collect::<Vec<_>>();
    values.resize(ID_BATCH_COUNT, Value::Null);
    let mut statement = connection.prepare_cached(&sql)?;
    let rows = statement
        .query_map(params_from_iter(values), |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?
        .map(|row| {
            let (id, bytes) = row?;
            let id = ObjectId::from_bytes(&id)?;
            layerfs_content::authenticate_identity(&bytes, id)?;
            Ok((id, bytes))
        })
        .collect();
    rows
}

#[allow(clippy::too_many_arguments)]
fn database_receipt(
    db: &StoreDb,
    operation: crate::DatabaseOperation,
    total_ns: u64,
    connection_wait_ns: u64,
    writer_acquire_ns: u64,
    statement_ns: u64,
    publication_ns: u64,
    commit_sync_ns: u64,
    statement_count: u64,
    rows: u64,
    bytes: u64,
) -> crate::DatabaseReceipt {
    let attributed = connection_wait_ns
        .saturating_add(writer_acquire_ns)
        .saturating_add(statement_ns)
        .saturating_add(publication_ns)
        .saturating_add(commit_sync_ns);
    crate::DatabaseReceipt {
        store_id: db.store_id(),
        role: db.role(),
        operation,
        total_ns,
        connection_wait_ns,
        writer_acquire_ns,
        statement_ns,
        publication_ns,
        commit_sync_ns,
        unattributed_ns: total_ns.saturating_sub(attributed),
        statement_count,
        rows,
        bytes,
    }
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

fn validate_fact_batch(role: StoreRole, facts: &[Fact]) -> Result<()> {
    let bytes = facts.iter().map(|fact| fact.encoded_size()).sum::<usize>();
    let ids = facts.iter().map(Fact::id).collect::<BTreeSet<_>>();
    if facts.len() > FACT_BATCH_COUNT
        || bytes > FACT_BATCH_BYTES
        || ids.len() != facts.len()
        || facts
            .windows(2)
            .any(|pair| pair[0].kind() != pair[1].kind())
        || facts
            .first()
            .is_some_and(|fact| fact_table(role, fact.kind()).is_err())
    {
        return Err(StorageError::InvalidInput("fact batch"));
    }
    Ok(())
}

fn validate_fact(fact: &Fact) -> Result<()> {
    match fact {
        Fact::Commit(record) => validate_commit_identity(record),
        Fact::Layer(record) => validate_layer_identity(record),
        Fact::Branch(record) => {
            EntityName::validate(record.name.as_str())?;
            record.validate_origin()
        }
        Fact::LayerStack(record) => EntityName::validate(record.name.as_str()),
    }
}

fn validate_layer_identity(layer: &LayerRecord) -> Result<()> {
    if LayerId::derive(layer.layer_stack_id, layer.parent_layer_id, layer.root_id) != layer.id {
        return Err(StorageError::Integrity("Layer identity"));
    }
    Ok(())
}

fn validate_commit_identity(commit: &CommitRecord) -> Result<()> {
    if CommitId::derive(
        commit.root_id,
        commit.parent_commit_id,
        commit.base_layer_id,
    ) != commit.id
    {
        return Err(StorageError::Integrity("Commit identity"));
    }
    Ok(())
}

fn fact_table(role: StoreRole, kind: FactKind) -> Result<(&'static str, &'static str)> {
    match (role, kind) {
        (_, FactKind::LayerStack) => Ok(("layer_stacks", "layer_stack_id")),
        (_, FactKind::Layer) => Ok(("layers", "layer_id")),
        (_, FactKind::Commit) => Ok(("commits", "commit_id")),
        (_, FactKind::Branch) => Ok(("branches", "branch_id")),
    }
}

fn fixed_membership_sql(table: &str, key: &str, columns: &str) -> String {
    let parameters = (1..=ID_BATCH_COUNT)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("SELECT {columns} FROM {table} WHERE {key} IN ({parameters})")
}

fn existing_facts(
    connection: &rusqlite::Connection,
    table: &str,
    key: &str,
    kind: FactKind,
    values: Vec<Value>,
) -> Result<BTreeMap<Vec<u8>, Fact>> {
    let columns = match kind {
        FactKind::Commit => "commit_id,root_id,parent_commit_id,base_layer_id",
        FactKind::Branch => {
            "branch_id,layer_stack_id,name,forked_from_layer_id,forked_from_branch_id,forked_from_commit_id"
        }
        FactKind::LayerStack => "layer_stack_id,name",
        FactKind::Layer => {
            "layer_id,layer_stack_id,parent_layer_id,root_id,source_branch_id,source_commit_id"
        }
    };
    let sql = fixed_membership_sql(table, key, columns);
    let mut statement = connection.prepare_cached(&sql)?;
    let mut facts = BTreeMap::new();
    match kind {
        FactKind::Commit => {
            let rows = statement.query_map(params_from_iter(values), |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            })?;
            for row in rows {
                let (id, root, parent, base) = row?;
                facts.insert(
                    id.clone(),
                    Fact::Commit(CommitRecord {
                        id: CommitId::from_slice(&id)?,
                        root_id: ObjectId::from_bytes(&root)?,
                        parent_commit_id: parent
                            .as_deref()
                            .map(CommitId::from_slice)
                            .transpose()?,
                        base_layer_id: LayerId::from_slice(&base)?,
                    }),
                );
            }
        }
        FactKind::Branch => {
            let rows = statement.query_map(params_from_iter(values), |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                ))
            })?;
            for row in rows {
                let (id, stack, name, from_layer, from_branch, from_commit) = row?;
                facts.insert(
                    id.clone(),
                    Fact::Branch(BranchFact {
                        id: BranchId::from_slice(&id)?,
                        layer_stack_id: LayerStackId::from_slice(&stack)?,
                        name: EntityName::new(name)?,
                        forked_from_layer_id: from_layer
                            .as_deref()
                            .map(LayerId::from_slice)
                            .transpose()?,
                        forked_from_branch_id: from_branch
                            .as_deref()
                            .map(BranchId::from_slice)
                            .transpose()?,
                        forked_from_commit_id: from_commit
                            .as_deref()
                            .map(CommitId::from_slice)
                            .transpose()?,
                    }),
                );
            }
        }
        FactKind::LayerStack => {
            let rows = statement.query_map(params_from_iter(values), |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, name) = row?;
                facts.insert(
                    id.clone(),
                    Fact::LayerStack(LayerStackFact {
                        id: LayerStackId::from_slice(&id)?,
                        name: EntityName::new(name)?,
                    }),
                );
            }
        }
        FactKind::Layer => {
            let rows = statement.query_map(params_from_iter(values), |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                ))
            })?;
            for row in rows {
                let (id, stack, parent, root, source_branch, source_commit) = row?;
                facts.insert(
                    id.clone(),
                    Fact::Layer(LayerRecord {
                        id: LayerId::from_slice(&id)?,
                        layer_stack_id: LayerStackId::from_slice(&stack)?,
                        parent_layer_id: parent.as_deref().map(LayerId::from_slice).transpose()?,
                        root_id: ObjectId::from_bytes(&root)?,
                        source_branch_id: source_branch
                            .as_deref()
                            .map(BranchId::from_slice)
                            .transpose()?,
                        source_commit_id: source_commit
                            .as_deref()
                            .map(CommitId::from_slice)
                            .transpose()?,
                    }),
                );
            }
        }
    }
    Ok(facts)
}

fn insert_fact(transaction: &Transaction<'_>, role: StoreRole, fact: &Fact) -> Result<bool> {
    match fact {
        Fact::Commit(record) => insert_commit(transaction, *record),
        Fact::Layer(record) => insert_layer(transaction, *record),
        Fact::Branch(record) => insert_branch_fact(transaction, role, record),
        Fact::LayerStack(record) => insert_layer_stack_fact(transaction, role, record),
    }
}

fn insert_layer(transaction: &Transaction<'_>, layer: LayerRecord) -> Result<bool> {
    let changed = transaction.execute(
        "INSERT INTO layers(layer_id,layer_stack_id,parent_layer_id,root_id,source_branch_id,source_commit_id)
         VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT DO NOTHING",
        params![
            layer.id.as_slice(),
            layer.layer_stack_id.as_slice(),
            layer.parent_layer_id.map(|id| id.to_bytes().to_vec()),
            layer.root_id.as_bytes().as_slice(),
            layer.source_branch_id.map(|id| id.to_bytes().to_vec()),
            layer.source_commit_id.map(|id| id.to_bytes().to_vec())
        ],
    )?;
    if changed == 0 {
        let known =
            layer_row(transaction, layer.id)?.ok_or(StorageError::Integrity("Layer admission"))?;
        if known != layer {
            return Err(StorageError::Integrity("Layer collision"));
        }
    }
    Ok(changed == 1)
}

fn insert_commit(transaction: &Transaction<'_>, commit: CommitRecord) -> Result<bool> {
    let changed = transaction.execute(
        "INSERT INTO commits(commit_id,root_id,parent_commit_id,base_layer_id)
         VALUES(?1,?2,?3,?4) ON CONFLICT DO NOTHING",
        params![
            commit.id.as_slice(),
            commit.root_id.as_bytes().as_slice(),
            commit.parent_commit_id.map(|id| id.to_bytes().to_vec()),
            commit.base_layer_id.as_slice()
        ],
    )?;
    if changed == 0 {
        let known = commit_row(transaction, commit.id)?
            .ok_or(StorageError::Integrity("Commit admission"))?;
        if known != commit {
            return Err(StorageError::Integrity("Commit collision"));
        }
    }
    Ok(changed == 1)
}

fn insert_layer_stack_fact(
    transaction: &Transaction<'_>,
    role: StoreRole,
    fact: &LayerStackFact,
) -> Result<bool> {
    if let Some(known) = layer_stack_fact_on(transaction, fact.id)? {
        if known != *fact {
            return Err(StorageError::Integrity("LayerStack collision"));
        }
        return Ok(false);
    }
    reject_layer_stack_name_conflict(transaction, fact)?;
    if role == StoreRole::LayerStack {
        return Err(StorageError::InvalidInput(
            "incomplete authority LayerStack fact",
        ));
    }
    transaction.execute(
        "INSERT INTO layer_stacks(layer_stack_id,name) VALUES(?1,?2)",
        params![fact.id.as_slice(), fact.name.as_str()],
    )?;
    Ok(true)
}

fn insert_branch_fact(
    transaction: &Transaction<'_>,
    role: StoreRole,
    fact: &BranchFact,
) -> Result<bool> {
    if let Some(known) = branch_fact_on(transaction, fact.id)? {
        if known != *fact {
            return Err(StorageError::Integrity("Branch collision"));
        }
        return Ok(false);
    }
    reject_branch_name_conflict(transaction, fact)?;
    if role == StoreRole::LayerStack {
        return Err(StorageError::InvalidInput(
            "incomplete authority Branch fact",
        ));
    }
    transaction.execute(
        "INSERT INTO branches(branch_id,layer_stack_id,name,base_layer_id,head_commit_id,
                              forked_from_layer_id,forked_from_branch_id,forked_from_commit_id)
         VALUES(?1,?2,?3,NULL,NULL,?4,?5,?6)",
        params![
            fact.id.as_slice(),
            fact.layer_stack_id.as_slice(),
            fact.name.as_str(),
            fact.forked_from_layer_id.map(|id| id.to_bytes().to_vec()),
            fact.forked_from_branch_id.map(|id| id.to_bytes().to_vec()),
            fact.forked_from_commit_id.map(|id| id.to_bytes().to_vec()),
        ],
    )?;
    Ok(true)
}

fn insert_branch(transaction: &Transaction<'_>, branch: &BranchRecord) -> Result<bool> {
    let existed = branch_fact_on(transaction, branch.id)?.is_some();
    publish_branch_record(transaction, branch)?;
    Ok(!existed)
}

fn publish_branch_record(transaction: &Transaction<'_>, branch: &BranchRecord) -> Result<()> {
    let fact = branch.fact();
    if let Some(known) = branch_fact_on(transaction, branch.id)? {
        if known != fact {
            return Err(StorageError::Integrity("Branch collision"));
        }
        transaction.execute(
            "UPDATE branches SET base_layer_id=?1,head_commit_id=?2 WHERE branch_id=?3",
            params![
                branch.base_layer_id.as_slice(),
                branch.head_commit_id.map(|id| id.to_bytes().to_vec()),
                branch.id.as_slice()
            ],
        )?;
    } else {
        reject_branch_name_conflict(transaction, &fact)?;
        transaction.execute(
            "INSERT INTO branches(branch_id,layer_stack_id,name,base_layer_id,head_commit_id,
                                  forked_from_layer_id,forked_from_branch_id,forked_from_commit_id)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                branch.id.as_slice(),
                branch.layer_stack_id.as_slice(),
                branch.name.as_str(),
                branch.base_layer_id.as_slice(),
                branch.head_commit_id.map(|id| id.to_bytes().to_vec()),
                branch.forked_from_layer_id.map(|id| id.to_bytes().to_vec()),
                branch
                    .forked_from_branch_id
                    .map(|id| id.to_bytes().to_vec()),
                branch
                    .forked_from_commit_id
                    .map(|id| id.to_bytes().to_vec()),
            ],
        )?;
    }
    Ok(())
}

fn layer_row_on(connection: &rusqlite::Connection, id: LayerId) -> Result<Option<LayerRecord>> {
    connection
        .query_row(
            "SELECT layer_stack_id,parent_layer_id,root_id,source_branch_id,source_commit_id
             FROM layers WHERE layer_id=?1",
            [id.as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                ))
            },
        )
        .optional()?
        .map(|(stack, parent, root, source_branch, source_commit)| {
            Ok(LayerRecord {
                id,
                layer_stack_id: LayerStackId::from_slice(&stack)?,
                parent_layer_id: parent.as_deref().map(LayerId::from_slice).transpose()?,
                root_id: ObjectId::from_bytes(&root)?,
                source_branch_id: source_branch
                    .as_deref()
                    .map(BranchId::from_slice)
                    .transpose()?,
                source_commit_id: source_commit
                    .as_deref()
                    .map(CommitId::from_slice)
                    .transpose()?,
            })
        })
        .transpose()
}

fn layer_row(transaction: &Transaction<'_>, id: LayerId) -> Result<Option<LayerRecord>> {
    layer_row_on(transaction, id)
}

fn commit_row_on(connection: &rusqlite::Connection, id: CommitId) -> Result<Option<CommitRecord>> {
    connection
        .query_row(
            "SELECT root_id,parent_commit_id,base_layer_id FROM commits WHERE commit_id=?1",
            [id.as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            },
        )
        .optional()?
        .map(|(root, parent, base)| {
            Ok(CommitRecord {
                id,
                root_id: ObjectId::from_bytes(&root)?,
                parent_commit_id: parent.as_deref().map(CommitId::from_slice).transpose()?,
                base_layer_id: LayerId::from_slice(&base)?,
            })
        })
        .transpose()
}

fn commit_row(transaction: &Transaction<'_>, id: CommitId) -> Result<Option<CommitRecord>> {
    commit_row_on(transaction, id)
}

fn layer_stack_fact_on(
    connection: &rusqlite::Connection,
    id: LayerStackId,
) -> Result<Option<LayerStackFact>> {
    connection
        .query_row(
            "SELECT name FROM layer_stacks WHERE layer_stack_id=?1",
            [id.as_slice()],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|name| {
            Ok(LayerStackFact {
                id,
                name: EntityName::new(name)?,
            })
        })
        .transpose()
}

fn branch_fact_on(connection: &rusqlite::Connection, id: BranchId) -> Result<Option<BranchFact>> {
    connection
        .query_row(
            "SELECT layer_stack_id,name,forked_from_layer_id,forked_from_branch_id,
                    forked_from_commit_id FROM branches WHERE branch_id=?1",
            [id.as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                ))
            },
        )
        .optional()?
        .map(|(stack, name, from_layer, from_branch, from_commit)| {
            Ok(BranchFact {
                id,
                layer_stack_id: LayerStackId::from_slice(&stack)?,
                name: EntityName::new(name)?,
                forked_from_layer_id: from_layer.as_deref().map(LayerId::from_slice).transpose()?,
                forked_from_branch_id: from_branch
                    .as_deref()
                    .map(BranchId::from_slice)
                    .transpose()?,
                forked_from_commit_id: from_commit
                    .as_deref()
                    .map(CommitId::from_slice)
                    .transpose()?,
            })
        })
        .transpose()
}

fn branch_row_on(connection: &rusqlite::Connection, id: BranchId) -> Result<Option<BranchRecord>> {
    let row = connection
        .query_row(
            "SELECT layer_stack_id,name,base_layer_id,head_commit_id,forked_from_layer_id,
                    forked_from_branch_id,forked_from_commit_id FROM branches WHERE branch_id=?1",
            [id.as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, Option<Vec<u8>>>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((stack, name, base, head, from_layer, from_branch, from_commit)) = row else {
        return Ok(None);
    };
    let Some(base) = base else {
        return Ok(None);
    };
    Ok(Some(BranchRecord {
        id,
        layer_stack_id: LayerStackId::from_slice(&stack)?,
        name: EntityName::new(name)?,
        base_layer_id: LayerId::from_slice(&base)?,
        head_commit_id: head.as_deref().map(CommitId::from_slice).transpose()?,
        forked_from_layer_id: from_layer.as_deref().map(LayerId::from_slice).transpose()?,
        forked_from_branch_id: from_branch
            .as_deref()
            .map(BranchId::from_slice)
            .transpose()?,
        forked_from_commit_id: from_commit
            .as_deref()
            .map(CommitId::from_slice)
            .transpose()?,
    }))
}

type BranchRecordRaw = (
    Vec<u8>,
    Vec<u8>,
    String,
    Vec<u8>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
);

fn branch_record_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<BranchRecordRaw> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ))
}

fn decode_branch_record(raw: BranchRecordRaw) -> Result<BranchRecord> {
    let (id, stack, name, base, head, from_layer, from_branch, from_commit) = raw;
    Ok(BranchRecord {
        id: BranchId::from_slice(&id)?,
        layer_stack_id: LayerStackId::from_slice(&stack)?,
        name: EntityName::new(name)?,
        base_layer_id: LayerId::from_slice(&base)?,
        head_commit_id: head.as_deref().map(CommitId::from_slice).transpose()?,
        forked_from_layer_id: from_layer.as_deref().map(LayerId::from_slice).transpose()?,
        forked_from_branch_id: from_branch
            .as_deref()
            .map(BranchId::from_slice)
            .transpose()?,
        forked_from_commit_id: from_commit
            .as_deref()
            .map(CommitId::from_slice)
            .transpose()?,
    })
}

fn decode_branch_scope(
    branch_id: BranchId,
    kind: &str,
    through: Option<Vec<u8>>,
    mode: Option<String>,
) -> Result<BranchScopeRecord> {
    let scope = match (kind, through, mode) {
        ("local", None, None) => BranchScope::Local,
        ("remote", Some(through), Some(mode)) => BranchScope::Remote {
            through_commit_id: CommitId::from_slice(&through)?,
            serving_mode: RemotePlacement::parse(&mode)?,
        },
        _ => return Err(StorageError::Integrity("Branch scope")),
    };
    Ok(BranchScopeRecord { branch_id, scope })
}

fn branch_row(transaction: &Transaction<'_>, id: BranchId) -> Result<Option<BranchRecord>> {
    branch_row_on(transaction, id)
}

fn branch_scope_on(
    connection: &rusqlite::Connection,
    id: BranchId,
) -> Result<Option<BranchScopeRecord>> {
    connection
        .query_row(
            "SELECT scope_kind,through_commit_id,serving_mode FROM branch_scopes
             WHERE branch_id=?1",
            [id.as_slice()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?
        .map(|(kind, through, mode)| {
            let scope = match (kind.as_str(), through, mode) {
                ("local", None, None) => BranchScope::Local,
                ("remote", Some(through), Some(mode)) => BranchScope::Remote {
                    through_commit_id: CommitId::from_slice(&through)?,
                    serving_mode: RemotePlacement::parse(&mode)?,
                },
                _ => return Err(StorageError::Integrity("Branch scope")),
            };
            Ok(BranchScopeRecord {
                branch_id: id,
                scope,
            })
        })
        .transpose()
}

fn layer_stack_scope_on(
    connection: &rusqlite::Connection,
    layer_stack_id: LayerStackId,
) -> Result<Option<LayerStackScopeRecord>> {
    connection
        .query_row(
            "SELECT through_layer_id,serving_mode FROM layer_stack_scopes
             WHERE layer_stack_id=?1",
            [layer_stack_id.as_slice()],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .map(|(through, mode)| {
            Ok(LayerStackScopeRecord {
                layer_stack_id,
                through_layer_id: LayerId::from_slice(&through)?,
                serving_mode: RemotePlacement::parse(&mode)?,
            })
        })
        .transpose()
}

fn layer_is_ancestor_on(
    connection: &rusqlite::Connection,
    ancestor: LayerId,
    descendant: LayerId,
) -> Result<bool> {
    let mut cursor = descendant;
    loop {
        if cursor == ancestor {
            return Ok(true);
        }
        let layer =
            layer_row_on(connection, cursor)?.ok_or(StorageError::Integrity("Layer ancestry"))?;
        let Some(parent) = layer.parent_layer_id else {
            return Ok(false);
        };
        cursor = parent;
    }
}

fn reject_layer_stack_name_conflict(
    connection: &rusqlite::Connection,
    fact: &LayerStackFact,
) -> Result<()> {
    let existing = connection
        .query_row(
            "SELECT layer_stack_id FROM layer_stacks WHERE name=?1",
            [fact.name.as_str()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
        .as_deref()
        .map(LayerStackId::from_slice)
        .transpose()?;
    if let Some(existing_id) = existing.filter(|id| *id != fact.id) {
        return Err(StorageError::LayerStackNameConflict {
            name: fact.name.clone(),
            existing_id,
            incoming_id: fact.id,
        });
    }
    Ok(())
}

fn reject_branch_name_conflict(connection: &rusqlite::Connection, fact: &BranchFact) -> Result<()> {
    let existing = connection
        .query_row(
            "SELECT branch_id FROM branches WHERE layer_stack_id=?1 AND name=?2",
            params![fact.layer_stack_id.as_slice(), fact.name.as_str()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
        .as_deref()
        .map(BranchId::from_slice)
        .transpose()?;
    if let Some(existing_id) = existing.filter(|id| *id != fact.id) {
        return Err(StorageError::BranchNameConflict {
            layer_stack_id: fact.layer_stack_id,
            name: fact.name.clone(),
            existing_id,
            incoming_id: fact.id,
        });
    }
    Ok(())
}

fn branch_head(transaction: &Transaction<'_>, id: BranchId) -> Result<Option<CommitId>> {
    transaction
        .query_row(
            "SELECT head_commit_id FROM branches WHERE branch_id=?1",
            [id.as_slice()],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .optional()?
        .flatten()
        .as_deref()
        .map(CommitId::from_slice)
        .transpose()
}

fn commit_is_ancestor_on(
    connection: &rusqlite::Connection,
    ancestor: CommitId,
    descendant: CommitId,
) -> Result<bool> {
    let mut cursor = descendant;
    loop {
        if cursor == ancestor {
            return Ok(true);
        }
        let commit =
            commit_row_on(connection, cursor)?.ok_or(StorageError::Integrity("Commit ancestry"))?;
        let Some(parent) = commit.parent_commit_id else {
            return Ok(false);
        };
        cursor = parent;
    }
}

struct LocalObjects<'a>(&'a StoreDb);

impl ObjectSource for LocalObjects<'_> {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        self.0.read_object_row(id)
    }
}

fn verify_transition(
    source: &dyn ObjectSource,
    old: Option<ObjectId>,
    new: ObjectId,
    seen: &mut crate::SpillableObjectSet,
    active: &mut BTreeSet<ObjectId>,
) -> Result<()> {
    if old == Some(new) {
        return Ok(());
    }
    if seen.insert_page(&[new])?.is_empty() {
        return Ok(());
    }
    if !active.insert(new) {
        return Err(StorageError::Integrity("object cycle"));
    }
    let canonical = source.read_object(new)?;
    let mut new_children = layerfs_content::object::references::referenced_objects(&canonical)?;
    let mut unique = BTreeSet::new();
    new_children.retain(|child| unique.insert(*child));
    if new_children.is_empty() {
        active.remove(&new);
        return Ok(());
    }
    let mut old_children = match old {
        Some(old) => {
            let canonical = source.read_object(old)?;
            layerfs_content::object::references::referenced_objects(&canonical)?
        }
        None => Vec::new(),
    };
    unique.clear();
    old_children.retain(|child| unique.insert(*child));
    let old_set = old_children.iter().copied().collect::<BTreeSet<_>>();
    let new_set = new_children.iter().copied().collect::<BTreeSet<_>>();
    new_children.retain(|child| !old_set.contains(child));
    old_children.retain(|child| !new_set.contains(child));
    for (index, child) in new_children.into_iter().enumerate() {
        verify_transition(
            source,
            old_children.get(index).copied(),
            child,
            seen,
            active,
        )?;
    }
    active.remove(&new);
    Ok(())
}

fn same_origin(left: &BranchRecord, right: &BranchRecord) -> bool {
    left.id == right.id
        && left.layer_stack_id == right.layer_stack_id
        && left.name == right.name
        && left.forked_from_layer_id == right.forked_from_layer_id
        && left.forked_from_branch_id == right.forked_from_branch_id
        && left.forked_from_commit_id == right.forked_from_commit_id
}

fn validate_history_limit(limit: u16) -> Result<()> {
    if limit == 0 || limit > 128 {
        return Err(StorageError::InvalidInput("history page"));
    }
    Ok(())
}

fn validate_record_limit(limit: u16) -> Result<()> {
    if limit == 0 || limit > 512 {
        return Err(StorageError::InvalidInput("record page"));
    }
    Ok(())
}
