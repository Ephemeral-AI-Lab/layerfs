use crate::{
    AdmissionSetReceipt, AdmissionStats, CanonicalObject, DatabaseReceipt, DeferredObjectStore,
    Fact, FactKind, LocalAdmissionReceipt, MissingBitmap, Result, StorageError, StorageId,
    FACT_BATCH_BYTES, FACT_BATCH_COUNT, ID_BATCH_COUNT, OBJECT_BATCH_BYTES, OBJECT_BATCH_COUNT,
};
use layerfs_content::object::references::referenced_objects;
use layerfs_content::{authenticate_identity, ObjectId};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Transaction};
use std::collections::{BTreeMap, BTreeSet};

impl crate::StoreDb {
    pub fn missing_objects(&self, ids: &[ObjectId]) -> Result<MissingBitmap> {
        let bytes = ids
            .iter()
            .map(|id| id.as_bytes().to_vec())
            .collect::<Vec<_>>();
        let connection = self.connection()?;
        missing(&connection, "objects", "object_id", &bytes)
    }

    pub fn missing_facts(&self, kind: FactKind, ids: &[Vec<u8>]) -> Result<MissingBitmap> {
        let (table, key, _) = fact_shape(kind, self.kind())?;
        let connection = self.connection()?;
        missing(&connection, table, key, ids)
    }

    pub(crate) fn admit_remote(&self, objects: &[CanonicalObject]) -> Result<AdmissionStats> {
        self.admit_objects_with_authentication(objects, true)
    }

    pub(crate) fn admit_local(&self, objects: &[CanonicalObject]) -> Result<AdmissionStats> {
        self.admit_objects_with_authentication(objects, false)
    }

    fn admit_objects_with_authentication(
        &self,
        objects: &[CanonicalObject],
        authenticate: bool,
    ) -> Result<AdmissionStats> {
        self.validate_object_batch(objects, authenticate)?;
        if objects.is_empty() {
            return Ok(AdmissionStats::default());
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let objects = admit_object_rows(&transaction, objects)?;
        let commit = std::time::Instant::now();
        transaction.commit()?;
        Ok(AdmissionStats {
            objects,
            database: DatabaseReceipt {
                write_transactions: 1,
                object_admission_transactions: 1,
                commit_sync_elapsed_ns: commit.elapsed().as_nanos().try_into().unwrap_or(u64::MAX),
                ..DatabaseReceipt::default()
            },
            ..AdmissionStats::default()
        })
    }

    pub fn admit_facts(&self, facts: &[Fact]) -> Result<()> {
        self.admit_received_facts(facts).map(drop)
    }

    pub(crate) fn admit_received_facts(&self, facts: &[Fact]) -> Result<AdmissionStats> {
        self.validate_fact_batch(facts)?;
        if facts.is_empty() {
            return Ok(AdmissionStats::default());
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let kind = facts[0].kind();
        let facts = admit_fact_rows(&transaction, self.kind(), facts)?;
        let commit = std::time::Instant::now();
        transaction.commit()?;
        Ok(AdmissionStats {
            facts: BTreeMap::from([(kind, facts)]),
            database: DatabaseReceipt {
                write_transactions: 1,
                fact_admission_transactions: 1,
                commit_sync_elapsed_ns: commit.elapsed().as_nanos().try_into().unwrap_or(u64::MAX),
                ..DatabaseReceipt::default()
            },
            ..AdmissionStats::default()
        })
    }

    pub(crate) fn validate_object_batch(
        &self,
        objects: &[CanonicalObject],
        authenticate: bool,
    ) -> Result<()> {
        if objects.len() > OBJECT_BATCH_COUNT {
            return Err(StorageError::InvalidInput("object batch"));
        }
        let total: usize = objects.iter().map(|object| object.bytes.len()).sum();
        if total > OBJECT_BATCH_BYTES && objects.len() != 1 {
            return Err(StorageError::InvalidInput("object batch bytes"));
        }
        if authenticate {
            for object in objects {
                crate::note_receiver_authentication();
                authenticate_identity(&object.bytes, object.id)?;
            }
        }
        self.validate_object_dependencies(objects)
    }

    pub(crate) fn validate_fact_batch(&self, facts: &[Fact]) -> Result<()> {
        if facts.is_empty() {
            return Ok(());
        }
        if facts.len() > FACT_BATCH_COUNT
            || facts.iter().map(|fact| fact.encoded_size()).sum::<usize>() > FACT_BATCH_BYTES
            || facts.iter().any(|fact| fact.kind() != facts[0].kind())
        {
            return Err(StorageError::InvalidInput("fact batch"));
        }
        facts
            .iter()
            .try_for_each(|fact| validate_fact_identity(*fact))?;
        self.validate_fact_dependencies(facts)?;
        if facts[0].kind() == FactKind::AddResult {
            self.validate_add_result_relations(facts)?;
        }
        Ok(())
    }

    pub(crate) fn validate_object_dependencies(&self, objects: &[CanonicalObject]) -> Result<()> {
        if self.kind() == crate::SchemaKind::Branch {
            return Ok(());
        }
        let batch = objects
            .iter()
            .map(|object| object.id)
            .collect::<BTreeSet<_>>();
        let mut prior = BTreeSet::new();
        let mut external = BTreeSet::new();
        for object in objects {
            for child in referenced_objects(&object.bytes)? {
                if batch.contains(&child) && !prior.contains(&child) {
                    return Err(StorageError::Integrity("parent before child"));
                }
                if !prior.contains(&child) {
                    external.insert(child);
                }
            }
            prior.insert(object.id);
        }
        let external = external.into_iter().collect::<Vec<_>>();
        for page in external.chunks(ID_BATCH_COUNT) {
            let missing = self.missing_objects(page)?;
            if (0..page.len()).any(|index| missing.is_missing(index).unwrap_or(true)) {
                return Err(StorageError::Integrity("parent before child"));
            }
        }
        Ok(())
    }

    fn validate_fact_dependencies(&self, facts: &[Fact]) -> Result<()> {
        let mut typed = BTreeMap::<FactKind, BTreeSet<Vec<u8>>>::new();
        let mut objects = BTreeSet::new();
        let mut prior = BTreeSet::new();
        for fact in facts {
            match *fact {
                Fact::Commit(value) => {
                    if self.kind() == crate::SchemaKind::Full {
                        objects.insert(value.root_id);
                    }
                    for parent in [value.parent_id, value.merge_parent_id]
                        .into_iter()
                        .flatten()
                    {
                        if !prior.contains(parent.as_slice()) {
                            typed
                                .entry(FactKind::Commit)
                                .or_default()
                                .insert(parent.to_bytes().to_vec());
                        }
                    }
                }
                Fact::Branch(value) => {
                    typed
                        .entry(FactKind::Commit)
                        .or_default()
                        .insert(value.head_commit_id.to_bytes().to_vec());
                    let kind = match value.base_id {
                        crate::BaseId::Layer(_) => FactKind::Layer,
                        crate::BaseId::Stack(_) => FactKind::Stack,
                    };
                    typed
                        .entry(kind)
                        .or_default()
                        .insert(value.base_id.as_slice().to_vec());
                }
                Fact::LayerHistory(value) => {
                    typed
                        .entry(FactKind::Layer)
                        .or_default()
                        .insert(value.head_layer_id.to_bytes().to_vec());
                }
                Fact::Layer(value) => {
                    objects.insert(value.root_id);
                    if let Some(parent) = value.parent_id {
                        if !prior.contains(parent.as_slice()) {
                            typed
                                .entry(FactKind::Layer)
                                .or_default()
                                .insert(parent.to_bytes().to_vec());
                        }
                    }
                }
                Fact::StackHistory(value) => {
                    typed
                        .entry(FactKind::Layer)
                        .or_default()
                        .insert(value.base_layer_id.to_bytes().to_vec());
                    typed
                        .entry(FactKind::Stack)
                        .or_default()
                        .insert(value.head_stack_id.to_bytes().to_vec());
                }
                Fact::Stack(value) => {
                    objects.insert(value.root_id);
                    if let Some(parent) = value.parent_id {
                        if !prior.contains(parent.as_slice()) {
                            typed
                                .entry(FactKind::Stack)
                                .or_default()
                                .insert(parent.to_bytes().to_vec());
                        }
                    }
                }
                Fact::AddResult(value) => {
                    let source = match value.source_id {
                        crate::SourceId::Branch(_) => FactKind::Branch,
                        crate::SourceId::Stack(_) => FactKind::Stack,
                    };
                    let result = match value.result_id {
                        crate::ResultId::Stack(_) => FactKind::Stack,
                        crate::ResultId::Layer(_) => FactKind::Layer,
                    };
                    typed
                        .entry(source)
                        .or_default()
                        .insert(value.source_id.as_slice().to_vec());
                    typed
                        .entry(result)
                        .or_default()
                        .insert(value.result_id.as_slice().to_vec());
                }
            }
            prior.insert(fact.id());
        }
        for (kind, ids) in typed {
            let ids = ids.into_iter().collect::<Vec<_>>();
            for page in ids.chunks(ID_BATCH_COUNT) {
                let missing = self.missing_facts(kind, page)?;
                for index in 0..page.len() {
                    if missing.is_missing(index)? {
                        return Err(StorageError::MissingBaseData);
                    }
                }
            }
        }
        let objects = objects.into_iter().collect::<Vec<_>>();
        for page in objects.chunks(ID_BATCH_COUNT) {
            let missing = self.missing_objects(page)?;
            for index in 0..page.len() {
                if missing.is_missing(index)? {
                    return Err(StorageError::MissingBaseData);
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn admit_object_rows(
    transaction: &Transaction<'_>,
    objects: &[CanonicalObject],
) -> Result<AdmissionSetReceipt> {
    let sql = fixed_insert_sql("objects", &["object_id", "bytes"]);
    let mut parameters = Vec::with_capacity(OBJECT_BATCH_COUNT * 2);
    for object in objects {
        parameters.push(Value::Blob(object.id.as_bytes().to_vec()));
        parameters.push(Value::Blob(object.bytes.clone()));
    }
    parameters.resize(OBJECT_BATCH_COUNT * 2, Value::Null);
    let mut statement = transaction.prepare_cached(&sql)?;
    let inserted = statement
        .query_map(params_from_iter(parameters), |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)? as u64))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(statement);
    let inserted_ids = inserted
        .iter()
        .map(|(id, _)| id.clone())
        .collect::<BTreeSet<_>>();
    let mut stats = AdmissionSetReceipt {
        inserted_ids: inserted.len() as u64,
        inserted_bytes: inserted.iter().map(|(_, bytes)| *bytes).sum(),
        ..AdmissionSetReceipt::default()
    };
    for object in objects
        .iter()
        .filter(|object| !inserted_ids.contains(object.id.as_bytes().as_slice()))
    {
        stats.raced_existing_ids += 1;
        stats.raced_existing_bytes += object.bytes.len() as u64;
    }
    Ok(stats)
}

pub(crate) fn admit_fact_rows(
    transaction: &Transaction<'_>,
    schema: crate::SchemaKind,
    facts: &[Fact],
) -> Result<AdmissionSetReceipt> {
    let (table, _, columns) = fact_shape(facts[0].kind(), schema)?;
    let sql = fixed_insert_sql(table, columns);
    let mut parameters = facts
        .iter()
        .flat_map(|fact| fact_values(*fact))
        .collect::<Vec<_>>();
    parameters.resize(FACT_BATCH_COUNT * columns.len(), Value::Null);
    let mut statement = transaction.prepare_cached(&sql)?;
    let inserted = statement
        .query_map(params_from_iter(parameters), |row| row.get::<_, Vec<u8>>(0))?
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    drop(statement);
    let mut stats = AdmissionSetReceipt {
        inserted_ids: inserted.len() as u64,
        inserted_bytes: facts
            .iter()
            .filter(|fact| inserted.contains(&fact.id()))
            .map(|fact| fact.encoded_size() as u64)
            .sum(),
        ..AdmissionSetReceipt::default()
    };
    let raced = facts
        .iter()
        .filter(|fact| !inserted.contains(&fact.id()))
        .copied()
        .collect::<Vec<_>>();
    let existing = existing_facts(transaction, table, columns, &raced)?;
    for fact in raced {
        if existing.get(&fact.id()) != Some(&fact_values(fact)) {
            return Err(StorageError::Integrity("typed ID collision"));
        }
        stats.raced_existing_ids += 1;
        stats.raced_existing_bytes += fact.encoded_size() as u64;
    }
    Ok(stats)
}

fn validate_fact_identity(fact: Fact) -> Result<()> {
    match fact {
        Fact::Commit(value)
            if value.id
                != crate::CommitId::derive(
                    value.root_id,
                    value.parent_id,
                    value.merge_parent_id,
                ) =>
        {
            Err(StorageError::Integrity("Commit ID"))
        }
        Fact::Layer(value)
            if value.id
                != crate::LayerId::derive(value.history_id, value.parent_id, value.root_id) =>
        {
            Err(StorageError::Integrity("Layer ID"))
        }
        Fact::Stack(value)
            if value.id
                != crate::StackId::derive(value.history_id, value.parent_id, value.root_id) =>
        {
            Err(StorageError::Integrity("Stack ID"))
        }
        Fact::AddResult(value)
            if matches!(
                (value.source_id, value.result_id),
                (crate::SourceId::Stack(_), crate::ResultId::Stack(_))
            ) =>
        {
            Err(StorageError::WrongSourceRoute)
        }
        _ => Ok(()),
    }
}

pub(crate) fn insert_commit(
    transaction: &Transaction<'_>,
    commit: crate::CommitRecord,
) -> Result<bool> {
    let inserted = transaction
        .query_row(
            "INSERT INTO commits(commit_id,root_id,parent_id,merge_parent_id) VALUES(?1,?2,?3,?4)
         ON CONFLICT(commit_id) DO NOTHING RETURNING commit_id",
            params![
                commit.id.as_slice(),
                commit.root_id.as_bytes().as_slice(),
                commit.parent_id.map(|id| id.to_bytes().to_vec()),
                commit.merge_parent_id.map(|id| id.to_bytes().to_vec())
            ],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if inserted {
        return Ok(true);
    }
    let existing = transaction
        .query_row(
            "SELECT root_id,parent_id,merge_parent_id FROM commits WHERE commit_id=?1",
            [commit.id.as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                ))
            },
        )
        .optional()?;
    let expected = (
        commit.root_id.as_bytes().to_vec(),
        commit.parent_id.map(|id| id.to_bytes().to_vec()),
        commit.merge_parent_id.map(|id| id.to_bytes().to_vec()),
    );
    if existing == Some(expected) {
        Ok(false)
    } else {
        Err(StorageError::Integrity("Commit collision"))
    }
}

pub(crate) fn insert_layer(transaction: &Transaction<'_>, layer: crate::LayerRecord) -> Result<()> {
    if layer.id != crate::LayerId::derive(layer.history_id, layer.parent_id, layer.root_id) {
        return Err(StorageError::Integrity("Layer ID"));
    }
    transaction.execute(
        "INSERT INTO layers(layer_id,history_id,parent_id,root_id) VALUES(?1,?2,?3,?4)",
        params![
            layer.id.as_slice(),
            layer.history_id.as_slice(),
            layer.parent_id.map(|id| id.to_bytes().to_vec()),
            layer.root_id.as_bytes().as_slice()
        ],
    )?;
    Ok(())
}

pub(crate) fn insert_stack(transaction: &Transaction<'_>, stack: crate::StackRecord) -> Result<()> {
    if stack.id != crate::StackId::derive(stack.history_id, stack.parent_id, stack.root_id) {
        return Err(StorageError::Integrity("Stack ID"));
    }
    transaction.execute(
        "INSERT INTO stacks(stack_id,history_id,parent_id,root_id) VALUES(?1,?2,?3,?4)",
        params![
            stack.id.as_slice(),
            stack.history_id.as_slice(),
            stack.parent_id.map(|id| id.to_bytes().to_vec()),
            stack.root_id.as_bytes().as_slice()
        ],
    )?;
    Ok(())
}

fn missing(
    connection: &rusqlite::Connection,
    table: &str,
    key: &str,
    ids: &[Vec<u8>],
) -> Result<MissingBitmap> {
    if ids.len() > ID_BATCH_COUNT || ids.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(StorageError::Integrity("membership ordering"));
    }
    let mut parameters = ids.iter().cloned().map(Value::Blob).collect::<Vec<_>>();
    parameters.resize(ID_BATCH_COUNT, Value::Null);
    let sql = crate::schema::membership_sql(table, key);
    let mut statement = connection.prepare_cached(&sql)?;
    let present = statement
        .query_map(params_from_iter(parameters), |row| row.get::<_, Vec<u8>>(0))?
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    MissingBitmap::from_missing(ids.len(), |index| !present.contains(&ids[index]))
}

pub(crate) fn final_object_batch(
    db: &crate::StoreDb,
    objects: &DeferredObjectStore,
) -> Result<(Option<Vec<CanonicalObject>>, LocalAdmissionReceipt)> {
    let mut last = None;
    let mut receipt = LocalAdmissionReceipt::default();
    receipt.objects.candidate_ids = objects.len();
    receipt.objects.candidate_bytes = objects.encoded_bytes();
    objects.visit_batches(&mut |batch, is_last| {
        if is_last {
            last = Some(batch.to_vec());
        } else {
            merge_local_admission(&mut receipt, db.admit_local(batch)?);
        }
        Ok(())
    })?;
    if let Some(batch) = &last {
        db.validate_object_dependencies(batch)?;
    }
    Ok((last, receipt))
}

fn merge_local_admission(receipt: &mut LocalAdmissionReceipt, admission: AdmissionStats) {
    merge_local_objects(receipt, admission.objects);
    for (kind, facts) in admission.facts {
        receipt.facts.entry(kind).or_default().merge(facts);
    }
    receipt.database.merge(admission.database);
}

pub(crate) fn merge_local_objects(
    receipt: &mut LocalAdmissionReceipt,
    objects: AdmissionSetReceipt,
) {
    receipt.objects.inserted_ids += objects.inserted_ids;
    receipt.objects.inserted_bytes += objects.inserted_bytes;
    receipt.objects.reused_ids += objects.raced_existing_ids;
    receipt.objects.reused_bytes += objects.raced_existing_bytes;
}

pub(crate) fn note_local_fact(receipt: &mut LocalAdmissionReceipt, fact: Fact, inserted: bool) {
    let bytes = fact.encoded_size() as u64;
    let facts = receipt.facts.entry(fact.kind()).or_default();
    if inserted {
        facts.inserted_ids += 1;
        facts.inserted_bytes += bytes;
    } else {
        facts.raced_existing_ids += 1;
        facts.raced_existing_bytes += bytes;
    }
}

pub(crate) fn finish_local_transaction(
    receipt: &mut LocalAdmissionReceipt,
    objects: bool,
    facts: bool,
    commit_sync: std::time::Duration,
) {
    receipt.database.write_transactions += 1;
    receipt.database.object_admission_transactions += u64::from(objects);
    receipt.database.fact_admission_transactions += u64::from(facts);
    receipt.database.visibility_transactions += 1;
    receipt.database.commit_sync_elapsed_ns +=
        commit_sync.as_nanos().try_into().unwrap_or(u64::MAX);
}

pub(crate) fn fixed_insert_sql(table: &str, columns: &[&str]) -> String {
    let mut parameter = 1;
    let rows = (0..FACT_BATCH_COUNT)
        .map(|_| {
            let row = (0..columns.len())
                .map(|_| {
                    let value = format!("?{parameter}");
                    parameter += 1;
                    value
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("({row})")
        })
        .collect::<Vec<_>>()
        .join(",");
    let returning = if table == "objects" {
        "object_id,length(bytes)"
    } else {
        columns[0]
    };
    format!(
        "WITH input({columns}) AS (VALUES {rows})
         INSERT INTO {table}({columns}) SELECT {columns} FROM input WHERE {key} IS NOT NULL
         ON CONFLICT({key}) DO NOTHING RETURNING {returning}",
        columns = columns.join(","),
        key = columns[0],
    )
}

pub(crate) fn object_rows(
    connection: &Connection,
    ids: &[ObjectId],
) -> Result<BTreeMap<ObjectId, Vec<u8>>> {
    if ids.len() > ID_BATCH_COUNT {
        return Err(StorageError::InvalidInput("object read page"));
    }
    let sql = crate::schema::membership_sql("objects", "object_id").replacen(
        "SELECT object_id",
        "SELECT object_id,bytes",
        1,
    );
    let mut parameters = ids
        .iter()
        .map(|id| Value::Blob(id.as_bytes().to_vec()))
        .collect::<Vec<_>>();
    parameters.resize(ID_BATCH_COUNT, Value::Null);
    let mut statement = connection.prepare_cached(&sql)?;
    let rows = statement
        .query_map(params_from_iter(parameters), |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?
        .map(|row| {
            let (id, bytes) = row?;
            Ok((ObjectId::from_bytes(&id)?, bytes))
        })
        .collect();
    drop(statement);
    rows
}

fn existing_facts(
    connection: &Connection,
    table: &str,
    columns: &[&str],
    facts: &[Fact],
) -> Result<BTreeMap<Vec<u8>, Vec<Value>>> {
    let ids = facts.iter().copied().map(Fact::id).collect::<Vec<_>>();
    let mut parameters = ids.iter().cloned().map(Value::Blob).collect::<Vec<_>>();
    parameters.resize(ID_BATCH_COUNT, Value::Null);
    let sql = crate::schema::membership_sql(table, columns[0]).replacen(
        &format!("SELECT {}", columns[0]),
        &format!("SELECT {}", columns.join(",")),
        1,
    );
    let mut statement = connection.prepare_cached(&sql)?;
    let rows = statement
        .query_map(params_from_iter(parameters), |row| {
            (0..columns.len())
                .map(|index| row.get::<_, Value>(index))
                .collect::<std::result::Result<Vec<_>, _>>()
        })?
        .map(|row| {
            let values = row?;
            let Value::Blob(id) = &values[0] else {
                return Err(StorageError::Integrity("fact key"));
            };
            Ok((id.clone(), values))
        })
        .collect();
    drop(statement);
    rows
}

pub(crate) fn fact_shape(
    kind: FactKind,
    schema: crate::SchemaKind,
) -> Result<(&'static str, &'static str, &'static [&'static str])> {
    let shape = match kind {
        FactKind::Commit => (
            "commits",
            "commit_id",
            &["commit_id", "root_id", "parent_id", "merge_parent_id"] as &[_],
        ),
        FactKind::Branch => (
            "branches",
            "branch_id",
            &["branch_id", "head_commit_id", "base_id"] as &[_],
        ),
        FactKind::LayerHistory => (
            "layer_histories",
            "history_id",
            &["history_id", "head_layer_id"] as &[_],
        ),
        FactKind::Layer => (
            "layers",
            "layer_id",
            &["layer_id", "history_id", "parent_id", "root_id"] as &[_],
        ),
        FactKind::StackHistory => (
            "stack_histories",
            "history_id",
            &["history_id", "base_layer_id", "head_stack_id"] as &[_],
        ),
        FactKind::Stack => (
            "stacks",
            "stack_id",
            &["stack_id", "history_id", "parent_id", "root_id"] as &[_],
        ),
        FactKind::AddResult => (
            "add_results",
            "source_id",
            &["source_id", "result_id"] as &[_],
        ),
    };
    if schema == crate::SchemaKind::Branch && !matches!(kind, FactKind::Commit | FactKind::Branch) {
        Err(StorageError::WrongSourceRoute)
    } else {
        Ok(shape)
    }
}

pub(crate) fn fact_from_values(kind: FactKind, values: &[Value]) -> Result<Fact> {
    let bytes = |index: usize| match values.get(index) {
        Some(Value::Blob(value)) => Ok(value.as_slice()),
        _ => Err(StorageError::Integrity("fact column")),
    };
    let optional = |index: usize| match values.get(index) {
        Some(Value::Null) => Ok(None),
        Some(Value::Blob(value)) => Ok(Some(value.as_slice())),
        _ => Err(StorageError::Integrity("fact column")),
    };
    Ok(match kind {
        FactKind::Commit => Fact::Commit(crate::CommitRecord {
            id: crate::CommitId::from_slice(bytes(0)?)?,
            root_id: layerfs_content::ObjectId::from_bytes(bytes(1)?)?,
            parent_id: optional(2)?.map(crate::CommitId::from_slice).transpose()?,
            merge_parent_id: optional(3)?.map(crate::CommitId::from_slice).transpose()?,
        }),
        FactKind::Branch => Fact::Branch(crate::BranchRecord {
            id: crate::BranchId::from_slice(bytes(0)?)?,
            head_commit_id: crate::CommitId::from_slice(bytes(1)?)?,
            base_id: crate::BaseId::from_slice(bytes(2)?)?,
        }),
        FactKind::LayerHistory => Fact::LayerHistory(crate::LayerHistoryRecord {
            id: crate::LayerHistoryId::from_slice(bytes(0)?)?,
            head_layer_id: crate::LayerId::from_slice(bytes(1)?)?,
        }),
        FactKind::Layer => Fact::Layer(crate::LayerRecord {
            id: crate::LayerId::from_slice(bytes(0)?)?,
            history_id: crate::LayerHistoryId::from_slice(bytes(1)?)?,
            parent_id: optional(2)?.map(crate::LayerId::from_slice).transpose()?,
            root_id: layerfs_content::ObjectId::from_bytes(bytes(3)?)?,
        }),
        FactKind::StackHistory => Fact::StackHistory(crate::StackHistoryRecord {
            id: crate::StackHistoryId::from_slice(bytes(0)?)?,
            base_layer_id: crate::LayerId::from_slice(bytes(1)?)?,
            head_stack_id: crate::StackId::from_slice(bytes(2)?)?,
        }),
        FactKind::Stack => Fact::Stack(crate::StackRecord {
            id: crate::StackId::from_slice(bytes(0)?)?,
            history_id: crate::StackHistoryId::from_slice(bytes(1)?)?,
            parent_id: optional(2)?.map(crate::StackId::from_slice).transpose()?,
            root_id: layerfs_content::ObjectId::from_bytes(bytes(3)?)?,
        }),
        FactKind::AddResult => Fact::AddResult(crate::AddResultRecord {
            source_id: crate::SourceId::from_slice(bytes(0)?)?,
            result_id: crate::ResultId::from_slice(bytes(1)?)?,
        }),
    })
}

fn fact_values(fact: Fact) -> Vec<Value> {
    use crate::StorageId;
    let blob = |bytes: &[u8]| Value::Blob(bytes.to_vec());
    let optional = |bytes: Option<&[u8]>| bytes.map_or(Value::Null, blob);
    match fact {
        Fact::Commit(value) => vec![
            blob(value.id.as_slice()),
            blob(value.root_id.as_bytes()),
            optional(value.parent_id.as_ref().map(StorageId::as_slice)),
            optional(value.merge_parent_id.as_ref().map(StorageId::as_slice)),
        ],
        Fact::Branch(value) => vec![
            blob(value.id.as_slice()),
            blob(value.head_commit_id.as_slice()),
            blob(value.base_id.as_slice()),
        ],
        Fact::LayerHistory(value) => vec![
            blob(value.id.as_slice()),
            blob(value.head_layer_id.as_slice()),
        ],
        Fact::Layer(value) => vec![
            blob(value.id.as_slice()),
            blob(value.history_id.as_slice()),
            optional(value.parent_id.as_ref().map(StorageId::as_slice)),
            blob(value.root_id.as_bytes()),
        ],
        Fact::StackHistory(value) => vec![
            blob(value.id.as_slice()),
            blob(value.base_layer_id.as_slice()),
            blob(value.head_stack_id.as_slice()),
        ],
        Fact::Stack(value) => vec![
            blob(value.id.as_slice()),
            blob(value.history_id.as_slice()),
            optional(value.parent_id.as_ref().map(StorageId::as_slice)),
            blob(value.root_id.as_bytes()),
        ],
        Fact::AddResult(value) => vec![
            blob(value.source_id.as_slice()),
            blob(value.result_id.as_slice()),
        ],
    }
}
