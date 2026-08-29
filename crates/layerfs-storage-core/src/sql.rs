use crate::admission::{
    admit_fact_rows, admit_object_rows, insert_commit, insert_layer, insert_stack,
};
use crate::merkle::object_batches;
use crate::schema::require_full;
use crate::{
    AdmissionStats, BaseId, BranchId, BranchRecord, CanonicalObject, CommitId, CommitRecord,
    DeferredObjectStore, Fact, LayerHistoryRecord, LayerId, LayerRecord, ObjectSource, Result,
    SchemaKind, SourceId, StackHistoryRecord, StackId, StackRecord, StorageError, StorageId,
    StoreDb, TransferIntent, TransferOutcome, FACT_BATCH_BYTES, FACT_BATCH_COUNT, ID_BATCH_COUNT,
};
use layerfs_core::ObjectId;
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

#[doc(hidden)]
pub fn fact_batches(facts: &[Fact]) -> Result<Vec<&[Fact]>> {
    let mut batches = Vec::new();
    let mut start = 0;
    while start < facts.len() {
        let kind = facts[start].kind();
        let mut end = start;
        let mut bytes = 0;
        while end < facts.len()
            && facts[end].kind() == kind
            && end - start < FACT_BATCH_COUNT
            && (end == start || bytes + facts[end].encoded_size() <= FACT_BATCH_BYTES)
        {
            bytes += facts[end].encoded_size();
            end += 1;
        }
        if end == start {
            return Err(StorageError::Integrity("fact batch"));
        }
        batches.push(&facts[start..end]);
        start = end;
    }
    Ok(batches)
}

impl StoreDb {
    pub fn read_object_row(&self, id: ObjectId) -> Result<Vec<u8>> {
        self.connection()?
            .query_row(
                "SELECT bytes FROM objects WHERE object_id=?1",
                [id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StorageError::MissingBaseData)
    }

    pub fn has_object(&self, id: ObjectId) -> Result<bool> {
        Ok(self.connection()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM objects WHERE object_id=?1)",
            [id.as_bytes().as_slice()],
            |row| row.get(0),
        )?)
    }

    pub fn existing_object_rows(
        &self,
        ids: &[ObjectId],
    ) -> Result<std::collections::BTreeMap<ObjectId, Vec<u8>>> {
        let connection = self.connection()?;
        crate::admission::object_rows(&connection, ids)
    }

    pub fn read_object_rows(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        let rows = self.existing_object_rows(ids)?;
        ids.iter()
            .map(|id| {
                Ok(CanonicalObject {
                    id: *id,
                    bytes: rows.get(id).cloned().ok_or(StorageError::MissingBaseData)?,
                })
            })
            .collect()
    }

    pub fn object_descriptors(&self, ids: &[ObjectId]) -> Result<Vec<(ObjectId, u64)>> {
        if ids.len() > ID_BATCH_COUNT {
            return Err(StorageError::InvalidInput("object read page"));
        }
        let sql = format!(
            "{} ORDER BY object_id",
            crate::schema::membership_sql("objects", "object_id").replacen(
                "SELECT object_id",
                "SELECT object_id,length(bytes)",
                1,
            )
        );
        let mut parameters = ids
            .iter()
            .map(|id| Value::Blob(id.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        parameters.resize(ID_BATCH_COUNT, Value::Null);
        let connection = self.connection()?;
        let mut statement = connection.prepare_cached(&sql)?;
        let rows = statement
            .query_map(params_from_iter(parameters), |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)? as u64))
            })?
            .map(|row| {
                let (id, len) = row?;
                Ok((ObjectId::from_bytes(&id)?, len))
            })
            .collect::<Result<Vec<_>>>()?;
        if rows.len() == ids.len() {
            Ok(rows)
        } else {
            Err(StorageError::MissingBaseData)
        }
    }

    pub fn visit_object_rows(
        &self,
        ids: &[ObjectId],
        visitor: &mut dyn FnMut(CanonicalObject) -> Result<()>,
    ) -> Result<()> {
        let connection = self.connection()?;
        visit_object_rows_at(&connection, ids, visitor)
    }

    pub(crate) fn validate_add_result_relations(&self, facts: &[Fact]) -> Result<()> {
        require_full(self)?;
        let mut values = Vec::with_capacity(FACT_BATCH_COUNT * 2);
        for fact in facts {
            let Fact::AddResult(value) = fact else {
                return Err(StorageError::Integrity("AddResult batch"));
            };
            values.push(Value::Blob(value.source_id.as_slice().to_vec()));
            values.push(Value::Blob(value.result_id.as_slice().to_vec()));
        }
        values.resize(FACT_BATCH_COUNT * 2, Value::Null);
        let rows = (0..FACT_BATCH_COUNT)
            .map(|index| format!("(?{},?{})", index * 2 + 1, index * 2 + 2))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "WITH RECURSIVE input(source_id,result_id) AS (VALUES {rows}),
             stack_anc(source_id,id) AS (
                SELECT source_id,result_id FROM input WHERE source_id IS NOT NULL AND substr(result_id,1,1)=x'22'
                UNION ALL SELECT a.source_id,s.parent_id FROM stack_anc a JOIN stacks s ON s.stack_id=a.id WHERE s.parent_id IS NOT NULL
             ), layer_anc(source_id,id) AS (
                SELECT source_id,result_id FROM input WHERE source_id IS NOT NULL AND substr(result_id,1,1)=x'32'
                UNION ALL SELECT a.source_id,l.parent_id FROM layer_anc a JOIN layers l ON l.layer_id=a.id WHERE l.parent_id IS NOT NULL
             ), valid(source_id) AS (
                SELECT i.source_id FROM input i JOIN branches b ON b.branch_id=i.source_id
                JOIN stack_anc a ON a.source_id=i.source_id AND a.id=b.base_id WHERE substr(i.result_id,1,1)=x'22'
                UNION SELECT i.source_id FROM input i JOIN branches b ON b.branch_id=i.source_id
                JOIN layer_anc a ON a.source_id=i.source_id AND a.id=b.base_id WHERE substr(i.result_id,1,1)=x'32'
                UNION SELECT i.source_id FROM input i JOIN stacks s ON s.stack_id=i.source_id
                JOIN stack_histories h ON h.history_id=s.history_id
                JOIN layer_anc a ON a.source_id=i.source_id AND a.id=h.base_layer_id WHERE substr(i.result_id,1,1)=x'32'
             ) SELECT count(*) FROM input i JOIN valid v USING(source_id) WHERE i.source_id IS NOT NULL"
        );
        let connection = self.connection()?;
        let mut statement = connection.prepare_cached(&sql)?;
        let valid: i64 = statement.query_row(params_from_iter(values), |row| row.get(0))?;
        if valid == facts.len() as i64 {
            Ok(())
        } else {
            Err(StorageError::Integrity("AddResult relationship"))
        }
    }

    pub fn branch(&self, id: BranchId) -> Result<Option<BranchRecord>> {
        self.connection()?
            .query_row(
                "SELECT head_commit_id, base_id FROM branches WHERE branch_id=?1",
                [id.as_slice()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?
            .map(|(head, base)| {
                Ok(BranchRecord {
                    id,
                    head_commit_id: CommitId::from_slice(&head)?,
                    base_id: BaseId::from_slice(&base)?,
                })
            })
            .transpose()
    }

    pub fn commit(&self, id: CommitId) -> Result<Option<CommitRecord>> {
        self.connection()?
            .query_row(
                "SELECT root_id, parent_id, merge_parent_id FROM commits WHERE commit_id=?1",
                [id.as_slice()],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Option<Vec<u8>>>(1)?,
                        row.get::<_, Option<Vec<u8>>>(2)?,
                    ))
                },
            )
            .optional()?
            .map(|(root, parent, merge)| {
                Ok(CommitRecord {
                    id,
                    root_id: ObjectId::from_bytes(&root)?,
                    parent_id: parent.as_deref().map(CommitId::from_slice).transpose()?,
                    merge_parent_id: merge.as_deref().map(CommitId::from_slice).transpose()?,
                })
            })
            .transpose()
    }

    pub fn create_branch(&self, branch: BranchRecord, anchor: CommitRecord) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        insert_commit(&transaction, anchor)?;
        transaction.execute(
            "INSERT INTO branches(branch_id,head_commit_id,base_id) VALUES(?1,?2,?3)",
            params![
                branch.id.as_slice(),
                branch.head_commit_id.as_slice(),
                branch.base_id.as_slice()
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn commit_branch(
        &self,
        branch_id: BranchId,
        expected: CommitId,
        commit: CommitRecord,
        objects: Option<&DeferredObjectStore>,
    ) -> Result<()> {
        let last = objects
            .map(|objects| final_object_batch(self, objects))
            .transpose()?
            .flatten();
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        if let Some(objects) = last.as_deref() {
            admit_object_rows(&transaction, objects)?;
        }
        insert_commit(&transaction, commit)?;
        let changed = transaction.execute(
            "UPDATE branches SET head_commit_id=?1 WHERE branch_id=?2 AND head_commit_id=?3",
            params![
                commit.id.as_slice(),
                branch_id.as_slice(),
                expected.as_slice()
            ],
        )?;
        if changed != 1 {
            let actual = branch_head(&transaction, branch_id)?;
            return Err(StorageError::CommitHeadMoved(crate::HeadMoved {
                expected: Some(expected),
                actual,
            }));
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn create_subbranch(&self, branch: BranchRecord) -> Result<()> {
        if self.commit(branch.head_commit_id)?.is_none() {
            return Err(StorageError::NotFound("source Commit"));
        }
        self.connection()?.execute(
            "INSERT INTO branches(branch_id,head_commit_id,base_id) VALUES(?1,?2,?3)",
            params![
                branch.id.as_slice(),
                branch.head_commit_id.as_slice(),
                branch.base_id.as_slice()
            ],
        )?;
        Ok(())
    }

    pub fn provision_layer_history(
        &self,
        history: LayerHistoryRecord,
        layer: LayerRecord,
        objects: &DeferredObjectStore,
    ) -> Result<()> {
        require_full(self)?;
        let last = final_object_batch(self, objects)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        if let Some(objects) = last.as_deref() {
            admit_object_rows(&transaction, objects)?;
        }
        insert_layer(&transaction, layer)?;
        transaction.execute(
            "INSERT INTO layer_histories(history_id,head_layer_id) VALUES(?1,?2)",
            params![history.id.as_slice(), history.head_layer_id.as_slice()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn create_stack_history_record(
        &self,
        history: StackHistoryRecord,
        stack: StackRecord,
    ) -> Result<()> {
        require_full(self)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        insert_stack(&transaction, stack)?;
        transaction.execute(
            "INSERT INTO stack_histories(history_id,base_layer_id,head_stack_id) VALUES(?1,?2,?3)",
            params![
                history.id.as_slice(),
                history.base_layer_id.as_slice(),
                history.head_stack_id.as_slice()
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn add_layer_atomic(
        &self,
        source: SourceId,
        checked_head: LayerId,
        layer: Option<LayerRecord>,
        objects: &DeferredObjectStore,
    ) -> Result<LayerId> {
        require_full(self)?;
        let result = layer.map_or(checked_head, |layer| layer.id);
        let history_id = match layer {
            Some(layer) => layer.history_id,
            None => {
                self.layer(checked_head)?
                    .ok_or(StorageError::MissingBaseData)?
                    .history_id
            }
        };
        let last = final_object_batch(self, objects)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        if let Some(objects) = last.as_deref() {
            admit_object_rows(&transaction, objects)?;
        }
        if let Some(layer) = layer {
            insert_layer(&transaction, layer)?;
        }
        transaction.execute(
            "INSERT INTO add_results(source_id,result_id) VALUES(?1,?2)",
            params![source.as_slice(), result.as_slice()],
        )?;
        let changed = transaction.execute(
            "UPDATE layer_histories SET head_layer_id=?1 WHERE history_id=?2 AND head_layer_id=?3",
            params![
                result.as_slice(),
                history_id.as_slice(),
                checked_head.as_slice()
            ],
        )?;
        if changed != 1 {
            let actual = transaction
                .query_row(
                    "SELECT head_layer_id FROM layer_histories WHERE history_id=?1",
                    [history_id.as_slice()],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()?
                .map(|bytes| LayerId::from_slice(&bytes))
                .transpose()?;
            return Err(StorageError::LayerHeadMoved(crate::HeadMoved {
                expected: Some(checked_head),
                actual,
            }));
        }
        transaction.commit()?;
        Ok(result)
    }

    pub fn add_stack_atomic(
        &self,
        branch_id: BranchId,
        checked_head: StackId,
        stack: StackRecord,
        objects: &DeferredObjectStore,
    ) -> Result<StackId> {
        require_full(self)?;
        let result = stack.id;
        let history_id = stack.history_id;
        let last = final_object_batch(self, objects)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        if let Some(objects) = last.as_deref() {
            admit_object_rows(&transaction, objects)?;
        }
        insert_stack(&transaction, stack)?;
        transaction.execute(
            "INSERT INTO add_results(source_id,result_id) VALUES(?1,?2)",
            params![branch_id.as_slice(), result.as_slice()],
        )?;
        let changed = transaction.execute(
            "UPDATE stack_histories SET head_stack_id=?1 WHERE history_id=?2 AND head_stack_id=?3",
            params![
                result.as_slice(),
                history_id.as_slice(),
                checked_head.as_slice()
            ],
        )?;
        if changed != 1 {
            let actual = transaction
                .query_row(
                    "SELECT head_stack_id FROM stack_histories WHERE history_id=?1",
                    [history_id.as_slice()],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()?
                .map(|bytes| StackId::from_slice(&bytes))
                .transpose()?;
            return Err(StorageError::StackHeadMoved(crate::HeadMoved {
                expected: Some(checked_head),
                actual,
            }));
        }
        transaction.commit()?;
        Ok(result)
    }

    pub fn observe_layer_history(
        &self,
        history: LayerHistoryRecord,
    ) -> Result<crate::RefOutcome<LayerId>> {
        require_full(self)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let (outcome, changed) = observe_layer_at(&transaction, history)?;
        if changed {
            transaction.commit()?;
        }
        Ok(outcome)
    }

    pub fn observe_stack_history(
        &self,
        history: StackHistoryRecord,
        expected: Option<StackId>,
    ) -> Result<crate::RefOutcome<StackId>> {
        require_full(self)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let (outcome, changed) = observe_stack_at(&transaction, history, expected)?;
        if changed {
            transaction.commit()?;
        }
        Ok(outcome)
    }

    #[doc(hidden)]
    pub fn finish_transfer(
        &self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        intent: TransferIntent,
    ) -> Result<(crate::TransferExchange, TransferOutcome)> {
        self.finish_transfer_with_authentication(objects, facts, intent, true)
    }

    #[doc(hidden)]
    pub fn finish_transfer_local(
        &self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        intent: TransferIntent,
    ) -> Result<(crate::TransferExchange, TransferOutcome)> {
        self.finish_transfer_with_authentication(objects, facts, intent, false)
    }

    fn finish_transfer_with_authentication(
        &self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        intent: TransferIntent,
        authenticate: bool,
    ) -> Result<(crate::TransferExchange, TransferOutcome)> {
        let object_pages = object_batches(objects)?;
        let fact_pages = fact_batches(facts)?;
        let final_fact = fact_pages.last().copied();
        let final_object = final_fact
            .is_none()
            .then(|| object_pages.last().copied())
            .flatten();
        let mut admission = AdmissionStats::default();
        for page in &object_pages[..object_pages.len() - usize::from(final_object.is_some())] {
            admission.merge(if authenticate {
                self.admit_remote(page)?
            } else {
                self.admit_local(page)?
            });
        }
        for page in &fact_pages[..fact_pages.len() - usize::from(final_fact.is_some())] {
            admission.merge(self.admit_received_facts(page)?);
        }
        if let Some(page) = final_object {
            self.validate_object_batch(page, authenticate)?;
        }
        if let Some(page) = final_fact {
            self.validate_fact_batch(page)?;
        }
        if final_object.is_none() && final_fact.is_none() && intent == TransferIntent::None {
            return Ok((
                crate::TransferExchange::new(
                    admission,
                    crate::MissingBitmap::empty(),
                    crate::MissingBitmap::empty(),
                ),
                TransferOutcome::Unit,
            ));
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        if let Some(page) = final_object {
            admission.merge(admit_object_rows(&transaction, page)?);
        }
        if let Some(page) = final_fact {
            admission.merge(admit_fact_rows(&transaction, self.kind(), page)?);
        }
        let (outcome, changed) = apply_transfer_intent(&transaction, self.kind(), intent)?;
        if final_object.is_some() || final_fact.is_some() || changed {
            transaction.commit()?;
            admission.transactions += 1;
        }
        Ok((
            crate::TransferExchange::new(
                admission,
                crate::MissingBitmap::empty(),
                crate::MissingBitmap::empty(),
            ),
            outcome,
        ))
    }
}

fn apply_transfer_intent(
    connection: &Connection,
    schema: SchemaKind,
    intent: TransferIntent,
) -> Result<(TransferOutcome, bool)> {
    match intent {
        TransferIntent::None => Ok((TransferOutcome::Unit, false)),
        TransferIntent::Branch { branch, expected } => {
            let (outcome, changed) = expose_branch_at(connection, schema, branch, expected)?;
            Ok((TransferOutcome::Commit(outcome), changed))
        }
        TransferIntent::Stack(push) => {
            if schema != SchemaKind::Full {
                return Err(StorageError::WrongSourceRoute);
            }
            verify_stack_intent_at(connection, &push)?;
            let history = StackHistoryRecord {
                id: push.history_id,
                base_layer_id: push.base_layer_id,
                head_stack_id: push.incoming_head,
            };
            let (outcome, changed) = observe_stack_at(connection, history, push.expected_head)?;
            Ok((TransferOutcome::Stack(outcome), changed))
        }
        TransferIntent::ObserveLayer(history) => {
            if schema != SchemaKind::Full {
                return Err(StorageError::WrongSourceRoute);
            }
            let (outcome, changed) = observe_layer_at(connection, history)?;
            Ok((TransferOutcome::Layer(outcome), changed))
        }
        TransferIntent::ObserveStack { history, expected } => {
            if schema != SchemaKind::Full {
                return Err(StorageError::WrongSourceRoute);
            }
            let (outcome, changed) = observe_stack_at(connection, history, expected)?;
            Ok((TransferOutcome::Stack(outcome), changed))
        }
    }
}

fn verify_stack_intent_at(connection: &Connection, push: &crate::StackPush) -> Result<()> {
    let incoming = connection
        .query_row(
            "SELECT history_id FROM stacks WHERE stack_id=?1",
            [push.incoming_head.as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
        .ok_or(StorageError::MissingBaseData)?;
    if crate::StackHistoryId::from_slice(&incoming)? != push.history_id {
        return Err(StorageError::Integrity("Stack history"));
    }
    if let Some(expected) = push.expected_head {
        let prior = connection
            .query_row(
                "SELECT history_id FROM stacks WHERE stack_id=?1",
                [expected.as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .ok_or(StorageError::MissingBaseData)?;
        if crate::StackHistoryId::from_slice(&prior)? != push.history_id {
            return Err(StorageError::Integrity("Stack suffix predecessor"));
        }
    }
    let base_root = connection
        .query_row(
            "SELECT root_id FROM layers WHERE layer_id=?1",
            [push.base_layer_id.as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
        .ok_or(StorageError::MissingBaseData)?;
    let present: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM objects WHERE object_id=?1)",
        [base_root],
        |row| row.get(0),
    )?;
    if present {
        Ok(())
    } else {
        Err(StorageError::MissingBaseData)
    }
}

fn expose_branch_at(
    connection: &Connection,
    schema: SchemaKind,
    branch: BranchRecord,
    expected: Option<CommitId>,
) -> Result<(crate::RefOutcome<CommitId>, bool)> {
    if schema == SchemaKind::Full {
        let accepted: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM add_results WHERE source_id=?1)",
            [branch.id.as_slice()],
            |row| row.get(0),
        )?;
        if accepted {
            let (head, base): (Vec<u8>, Vec<u8>) = connection.query_row(
                "SELECT head_commit_id,base_id FROM branches WHERE branch_id=?1",
                [branch.id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if CommitId::from_slice(&head)? == branch.head_commit_id
                && BaseId::from_slice(&base)? == branch.base_id
            {
                return Ok((crate::RefOutcome::UpToDate(branch.head_commit_id), false));
            }
            return Err(StorageError::Integrity("accepted Branch moved"));
        }
    }
    let existing = connection
        .query_row(
            "SELECT head_commit_id,base_id FROM branches WHERE branch_id=?1",
            [branch.id.as_slice()],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()?;
    match existing {
        None if expected.is_none() => {
            connection.execute(
                "INSERT INTO branches(branch_id,head_commit_id,base_id) VALUES(?1,?2,?3)",
                params![
                    branch.id.as_slice(),
                    branch.head_commit_id.as_slice(),
                    branch.base_id.as_slice()
                ],
            )?;
            Ok((crate::RefOutcome::Created(branch.head_commit_id), true))
        }
        Some((head, base)) => {
            let head = CommitId::from_slice(&head)?;
            if BaseId::from_slice(&base)? != branch.base_id {
                return Err(StorageError::Integrity("Branch base moved"));
            }
            if head == branch.head_commit_id
                || commit_ancestor(connection, branch.head_commit_id, head)?
            {
                Ok((crate::RefOutcome::UpToDate(head), false))
            } else if Some(head) == expected
                && commit_ancestor(connection, head, branch.head_commit_id)?
            {
                connection.execute(
                    "UPDATE branches SET head_commit_id=?1 WHERE branch_id=?2 AND head_commit_id=?3",
                    params![branch.head_commit_id.as_slice(), branch.id.as_slice(), head.as_slice()],
                )?;
                Ok((
                    crate::RefOutcome::FastForwarded(branch.head_commit_id),
                    true,
                ))
            } else if Some(head) == expected {
                Err(StorageError::CommitHeadMoved(crate::HeadMoved {
                    expected: Some(head),
                    actual: Some(branch.head_commit_id),
                }))
            } else {
                Err(StorageError::CommitHeadMoved(crate::HeadMoved {
                    expected,
                    actual: Some(head),
                }))
            }
        }
        None => Err(StorageError::CommitHeadMoved(crate::HeadMoved {
            expected,
            actual: None,
        })),
    }
}

fn observe_layer_at(
    connection: &Connection,
    history: LayerHistoryRecord,
) -> Result<(crate::RefOutcome<LayerId>, bool)> {
    let existing = connection
        .query_row(
            "SELECT head_layer_id FROM layer_histories WHERE history_id=?1",
            [history.id.as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
        .map(|bytes| LayerId::from_slice(&bytes))
        .transpose()?;
    match existing {
        None => {
            connection.execute(
                "INSERT INTO layer_histories(history_id,head_layer_id) VALUES(?1,?2)",
                params![history.id.as_slice(), history.head_layer_id.as_slice()],
            )?;
            Ok((crate::RefOutcome::Created(history.head_layer_id), true))
        }
        Some(head)
            if head == history.head_layer_id
                || linear_ancestor(
                    connection,
                    "layers",
                    "layer_id",
                    history.head_layer_id.as_slice(),
                    head.as_slice(),
                )? =>
        {
            Ok((crate::RefOutcome::UpToDate(head), false))
        }
        Some(head)
            if linear_ancestor(
                connection,
                "layers",
                "layer_id",
                head.as_slice(),
                history.head_layer_id.as_slice(),
            )? =>
        {
            connection.execute("UPDATE layer_histories SET head_layer_id=?1 WHERE history_id=?2 AND head_layer_id=?3", params![history.head_layer_id.as_slice(), history.id.as_slice(), head.as_slice()])?;
            Ok((
                crate::RefOutcome::FastForwarded(history.head_layer_id),
                true,
            ))
        }
        Some(head) => Err(StorageError::LayerHeadMoved(crate::HeadMoved {
            expected: Some(head),
            actual: Some(history.head_layer_id),
        })),
    }
}

fn observe_stack_at(
    connection: &Connection,
    history: StackHistoryRecord,
    expected: Option<StackId>,
) -> Result<(crate::RefOutcome<StackId>, bool)> {
    let existing = connection
        .query_row(
            "SELECT base_layer_id,head_stack_id FROM stack_histories WHERE history_id=?1",
            [history.id.as_slice()],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()?;
    match existing {
        None if expected.is_none() => {
            connection.execute("INSERT INTO stack_histories(history_id,base_layer_id,head_stack_id) VALUES(?1,?2,?3)", params![history.id.as_slice(), history.base_layer_id.as_slice(), history.head_stack_id.as_slice()])?;
            Ok((crate::RefOutcome::Created(history.head_stack_id), true))
        }
        Some((base, head)) => {
            if LayerId::from_slice(&base)? != history.base_layer_id {
                return Err(StorageError::Integrity("StackHistory base"));
            }
            let head = StackId::from_slice(&head)?;
            if head == history.head_stack_id
                || linear_ancestor(
                    connection,
                    "stacks",
                    "stack_id",
                    history.head_stack_id.as_slice(),
                    head.as_slice(),
                )?
            {
                Ok((crate::RefOutcome::UpToDate(head), false))
            } else if Some(head) == expected
                && linear_ancestor(
                    connection,
                    "stacks",
                    "stack_id",
                    head.as_slice(),
                    history.head_stack_id.as_slice(),
                )?
            {
                connection.execute("UPDATE stack_histories SET head_stack_id=?1 WHERE history_id=?2 AND head_stack_id=?3", params![history.head_stack_id.as_slice(), history.id.as_slice(), head.as_slice()])?;
                Ok((
                    crate::RefOutcome::FastForwarded(history.head_stack_id),
                    true,
                ))
            } else {
                Err(StorageError::StackHeadMoved(crate::HeadMoved {
                    expected,
                    actual: Some(head),
                }))
            }
        }
        None => Err(StorageError::StackHeadMoved(crate::HeadMoved {
            expected,
            actual: None,
        })),
    }
}

impl ObjectSource for StoreDb {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        self.read_object_row(id)
    }

    fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        self.read_object_rows(ids)
    }

    fn visit_objects(
        &self,
        ids: &[ObjectId],
        visitor: &mut dyn FnMut(CanonicalObject) -> Result<()>,
    ) -> Result<()> {
        self.visit_object_rows(ids, visitor)
    }
}

pub(crate) fn visit_object_rows_at(
    connection: &Connection,
    ids: &[ObjectId],
    visitor: &mut dyn FnMut(CanonicalObject) -> Result<()>,
) -> Result<()> {
    if ids.len() > ID_BATCH_COUNT {
        return Err(StorageError::InvalidInput("object read page"));
    }
    let sql = format!(
        "{} ORDER BY object_id",
        crate::schema::membership_sql("objects", "object_id").replacen(
            "SELECT object_id",
            "SELECT object_id,bytes",
            1,
        )
    );
    let mut parameters = ids
        .iter()
        .map(|id| Value::Blob(id.as_bytes().to_vec()))
        .collect::<Vec<_>>();
    parameters.resize(ID_BATCH_COUNT, Value::Null);
    let mut missing = ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut statement = connection.prepare_cached(&sql)?;
    let mut rows = statement.query(params_from_iter(parameters))?;
    while let Some(row) = rows.next()? {
        let id = ObjectId::from_bytes(&row.get::<_, Vec<u8>>(0)?)?;
        if !missing.remove(&id) {
            return Err(StorageError::Integrity("object read row"));
        }
        visitor(CanonicalObject {
            id,
            bytes: row.get(1)?,
        })?;
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(StorageError::MissingBaseData)
    }
}

fn final_object_batch(
    db: &StoreDb,
    objects: &DeferredObjectStore,
) -> Result<Option<Vec<CanonicalObject>>> {
    let mut last = None;
    objects.visit_batches(&mut |batch, is_last| {
        if is_last {
            last = Some(batch.to_vec());
        } else {
            db.admit_local(batch)?;
        }
        Ok(())
    })?;
    if let Some(batch) = &last {
        db.validate_object_dependencies(batch)?;
    }
    Ok(last)
}

fn commit_ancestor(connection: &Connection, ancestor: CommitId, head: CommitId) -> Result<bool> {
    Ok(connection.query_row(
        "WITH RECURSIVE ids(id) AS (
            SELECT ?2
            UNION SELECT c.parent_id FROM commits c JOIN ids ON c.commit_id=ids.id WHERE c.parent_id IS NOT NULL
            UNION SELECT c.merge_parent_id FROM commits c JOIN ids ON c.commit_id=ids.id WHERE c.merge_parent_id IS NOT NULL
         ) SELECT EXISTS(SELECT 1 FROM ids WHERE id=?1)",
        params![ancestor.as_slice(), head.as_slice()], |row| row.get(0),
    )?)
}

pub(crate) fn linear_ancestor(
    connection: &Connection,
    table: &str,
    key: &str,
    ancestor: &[u8],
    head: &[u8],
) -> Result<bool> {
    let sql = format!("WITH RECURSIVE ids(id) AS (SELECT ?2 UNION ALL SELECT n.parent_id FROM {table} n JOIN ids ON n.{key}=ids.id WHERE n.parent_id IS NOT NULL) SELECT EXISTS(SELECT 1 FROM ids WHERE id=?1)");
    Ok(connection.query_row(&sql, params![ancestor, head], |row| row.get(0))?)
}

fn branch_head(connection: &Connection, id: BranchId) -> Result<Option<CommitId>> {
    connection
        .query_row(
            "SELECT head_commit_id FROM branches WHERE branch_id=?1",
            [id.as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
        .map(|bytes| CommitId::from_slice(&bytes))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LayerHistoryId;

    #[test]
    fn final_fact_folds_branch_visibility_and_up_to_date_writes_nothing() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-folded-transfer-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db = StoreDb::open(root.join("store.sqlite"), SchemaKind::Branch).unwrap();
        let mut facts = Vec::new();
        let mut parent = None;
        for index in 0..=FACT_BATCH_COUNT {
            let root_id = ObjectId::for_bytes(&(index as u64).to_be_bytes());
            let commit = CommitRecord {
                id: CommitId::derive(root_id, parent, None),
                root_id,
                parent_id: parent,
                merge_parent_id: None,
            };
            parent = Some(commit.id);
            facts.push(Fact::Commit(commit));
        }
        let commit = match facts.last().unwrap() {
            Fact::Commit(commit) => *commit,
            _ => unreachable!(),
        };
        let branch = BranchRecord {
            id: BranchId::new(),
            head_commit_id: commit.id,
            base_id: BaseId::Layer(LayerId::derive(LayerHistoryId::new(), None, commit.root_id)),
        };
        let intent = TransferIntent::Branch {
            branch,
            expected: None,
        };
        let (exchange, outcome) = db.finish_transfer(&[], &facts, intent.clone()).unwrap();
        let admission = exchange.into_parts().0;
        assert_eq!(admission.transactions, 2);
        assert_eq!(
            outcome,
            TransferOutcome::Commit(crate::RefOutcome::Created(commit.id))
        );
        let (exchange, outcome) = db.finish_transfer(&[], &[], intent).unwrap();
        let admission = exchange.into_parts().0;
        assert_eq!(admission.transactions, 0);
        assert_eq!(
            outcome,
            TransferOutcome::Commit(crate::RefOutcome::UpToDate(commit.id))
        );
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
