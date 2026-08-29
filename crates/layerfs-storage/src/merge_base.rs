use crate::{
    AddResultRecord, BaseId, BranchId, BranchRecord, CanonicalObject, CommitId, CommitRecord, Fact,
    FactKind, LayerHistoryId, LayerHistoryRecord, LayerId, LayerRecord, ObjectSource, Result,
    ResultId, SourceId, StackHistoryId, StackHistoryRecord, StackId, StackRecord, StorageError,
    StorageId, StoreDb, ID_BATCH_COUNT,
};
use layerfs_content::ObjectId;
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, OptionalExtension};
use std::collections::BTreeMap;

type CommitVisitor<'a> = dyn FnMut(&dyn ObjectSource, &[CommitRecord]) -> Result<()> + 'a;

#[doc(hidden)]
pub struct StackPositions {
    memory: BTreeMap<StackId, u64>,
    spill: Option<rusqlite::Connection>,
}

impl StackPositions {
    pub fn position(&self, id: StackId) -> Result<Option<u64>> {
        if let Some(position) = self.memory.get(&id) {
            return Ok(Some(*position));
        }
        self.spill
            .as_ref()
            .map(|connection| {
                let position = connection
                    .query_row(
                        "SELECT position FROM positions WHERE stack_id=?1",
                        [id.as_slice()],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(StorageError::from)?;
                position
                    .map(u64::try_from)
                    .transpose()
                    .map_err(|_| StorageError::Integrity("Stack position"))
            })
            .unwrap_or(Ok(None))
    }

    fn insert_page(&mut self, page: &[(StackId, u64)]) -> Result<()> {
        if self.spill.is_none() && (self.memory.len() + page.len()) * 64 <= 8 * 1024 * 1024 {
            for (id, position) in page {
                if self.memory.insert(*id, *position).is_some() {
                    return Err(StorageError::Integrity("Stack ancestry cycle"));
                }
            }
            return Ok(());
        }
        if self.spill.is_none() {
            let mut connection = rusqlite::Connection::open("")?;
            connection.pragma_update(None, "journal_mode", "OFF")?;
            connection.pragma_update(None, "synchronous", "OFF")?;
            connection.pragma_update(None, "temp_store", "FILE")?;
            connection.execute_batch(
                "CREATE TABLE positions(
                    stack_id BLOB PRIMARY KEY NOT NULL,
                    position INTEGER NOT NULL
                 ) WITHOUT ROWID",
            )?;
            let existing = self
                .memory
                .iter()
                .map(|(id, at)| (*id, *at))
                .collect::<Vec<_>>();
            for page in existing.chunks(ID_BATCH_COUNT) {
                insert_position_rows(&mut connection, page)?;
            }
            self.memory.clear();
            self.spill = Some(connection);
        }
        insert_position_rows(self.spill.as_mut().unwrap(), page)
    }
}

fn insert_position_rows(
    connection: &mut rusqlite::Connection,
    page: &[(StackId, u64)],
) -> Result<()> {
    static SQL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let sql = SQL.get_or_init(|| {
        let rows = (0..ID_BATCH_COUNT)
            .map(|index| format!("(?{},?{})", index * 2 + 1, index * 2 + 2))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "INSERT INTO positions(stack_id,position)
             SELECT column1,column2 FROM (VALUES {rows}) WHERE column1 IS NOT NULL"
        )
    });
    let mut values = Vec::with_capacity(ID_BATCH_COUNT * 2);
    for (id, position) in page {
        values.push(Value::Blob(id.as_slice().to_vec()));
        values.push(Value::Integer(*position as i64));
    }
    values.resize(ID_BATCH_COUNT * 2, Value::Null);
    let transaction = connection.transaction()?;
    transaction.execute(sql, params_from_iter(values))?;
    transaction.commit()?;
    Ok(())
}

impl StoreDb {
    pub fn layer(&self, id: LayerId) -> Result<Option<LayerRecord>> {
        crate::schema::require_full(self)?;
        self.connection()?
            .query_row(
                "SELECT history_id,parent_id,root_id FROM layers WHERE layer_id=?1",
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
            .map(|(history, parent, root)| {
                Ok(LayerRecord {
                    id,
                    history_id: LayerHistoryId::from_slice(&history)?,
                    parent_id: parent.as_deref().map(LayerId::from_slice).transpose()?,
                    root_id: ObjectId::from_bytes(&root)?,
                })
            })
            .transpose()
    }

    pub fn stack(&self, id: StackId) -> Result<Option<StackRecord>> {
        crate::schema::require_full(self)?;
        self.connection()?
            .query_row(
                "SELECT history_id,parent_id,root_id FROM stacks WHERE stack_id=?1",
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
            .map(|(history, parent, root)| {
                Ok(StackRecord {
                    id,
                    history_id: StackHistoryId::from_slice(&history)?,
                    parent_id: parent.as_deref().map(StackId::from_slice).transpose()?,
                    root_id: ObjectId::from_bytes(&root)?,
                })
            })
            .transpose()
    }

    pub fn layer_history(&self, id: LayerHistoryId) -> Result<Option<LayerHistoryRecord>> {
        crate::schema::require_full(self)?;
        self.connection()?
            .query_row(
                "SELECT head_layer_id FROM layer_histories WHERE history_id=?1",
                [id.as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|head| {
                Ok(LayerHistoryRecord {
                    id,
                    head_layer_id: LayerId::from_slice(&head)?,
                })
            })
            .transpose()
    }

    pub fn stack_history(&self, id: StackHistoryId) -> Result<Option<StackHistoryRecord>> {
        crate::schema::require_full(self)?;
        self.connection()?
            .query_row(
                "SELECT base_layer_id,head_stack_id FROM stack_histories WHERE history_id=?1",
                [id.as_slice()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?
            .map(|(base, head)| {
                Ok(StackHistoryRecord {
                    id,
                    base_layer_id: LayerId::from_slice(&base)?,
                    head_stack_id: StackId::from_slice(&head)?,
                })
            })
            .transpose()
    }

    pub fn add_result(&self, source: SourceId) -> Result<Option<AddResultRecord>> {
        crate::schema::require_full(self)?;
        self.connection()?
            .query_row(
                "SELECT result_id FROM add_results WHERE source_id=?1",
                [source.as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|result| {
                Ok(AddResultRecord {
                    source_id: source,
                    result_id: ResultId::from_slice(&result)?,
                })
            })
            .transpose()
    }

    #[doc(hidden)]
    pub fn publication_facts(
        &self,
        kind: FactKind,
        ids: &[Vec<u8>],
    ) -> Result<BTreeMap<Vec<u8>, Fact>> {
        crate::schema::require_full(self)?;
        if ids.len() > ID_BATCH_COUNT || ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(StorageError::Integrity("publication membership ordering"));
        }
        let (table, key, columns) = match kind {
            FactKind::Branch => ("branches", "branch_id", "branch_id,head_commit_id,base_id"),
            FactKind::AddResult => ("add_results", "source_id", "source_id,result_id,NULL"),
            _ => return Err(StorageError::WrongSourceRoute),
        };
        let sql = crate::schema::membership_sql(table, key).replacen(key, columns, 1);
        let mut parameters = ids.iter().cloned().map(Value::Blob).collect::<Vec<_>>();
        parameters.resize(ID_BATCH_COUNT, Value::Null);
        let connection = self.connection()?;
        let mut statement = connection.prepare_cached(&sql)?;
        let rows = statement
            .query_map(params_from_iter(parameters), |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                ))
            })?
            .map(|row| {
                let (id, d1, d2) = row?;
                let fact = match kind {
                    FactKind::Branch => Fact::Branch(BranchRecord {
                        id: BranchId::from_slice(&id)?,
                        head_commit_id: CommitId::from_slice(&d1)?,
                        base_id: BaseId::from_slice(
                            d2.as_deref()
                                .ok_or(StorageError::Integrity("Branch base"))?,
                        )?,
                    }),
                    FactKind::AddResult => Fact::AddResult(AddResultRecord {
                        source_id: SourceId::from_slice(&id)?,
                        result_id: ResultId::from_slice(&d1)?,
                    }),
                    _ => unreachable!(),
                };
                Ok((id, fact))
            })
            .collect();
        drop(statement);
        rows
    }

    #[doc(hidden)]
    pub fn validate_stack_publication(
        &self,
        push: &crate::StackPush,
        pairs: &[(BranchRecord, AddResultRecord)],
        positions: &StackPositions,
    ) -> Result<()> {
        const PAIRS: usize = 64;
        crate::schema::require_full(self)?;
        if pairs.len() > PAIRS {
            return Err(StorageError::InvalidInput("Stack publication page"));
        }
        let expected_position = push
            .expected_head
            .map(|id| positions.position(id))
            .transpose()?
            .flatten();
        for (branch, result) in pairs {
            let (BaseId::Stack(base), ResultId::Stack(result_stack)) =
                (branch.base_id, result.result_id)
            else {
                return Err(StorageError::Integrity("Stack publication relationship"));
            };
            let base_position = positions
                .position(base)?
                .ok_or(StorageError::Integrity("Stack publication base"))?;
            let result_position = positions
                .position(result_stack)?
                .ok_or(StorageError::Integrity("Stack publication result"))?;
            if result.source_id != SourceId::Branch(branch.id)
                || base_position <= result_position
                || expected_position.is_some_and(|expected| result_position >= expected)
            {
                return Err(StorageError::Integrity("Stack publication relationship"));
            }
        }
        let rows = (0..PAIRS)
            .map(|index| {
                let first = index * 5 + 1;
                format!(
                    "(?{first},?{},?{},?{},?{})",
                    first + 1,
                    first + 2,
                    first + 3,
                    first + 4
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let history_parameter = PAIRS * 5 + 1;
        let sql = format!(
            "WITH input(branch_id,head_id,base_id,source_id,result_id) AS (VALUES {rows})
             SELECT count(*) FROM input i
             JOIN stacks result_stack ON result_stack.stack_id=i.result_id
             JOIN stacks base_stack ON base_stack.stack_id=i.base_id
             LEFT JOIN branches known_branch ON known_branch.branch_id=i.branch_id
             LEFT JOIN add_results known_result ON known_result.source_id=i.source_id
             WHERE i.branch_id IS NOT NULL AND i.branch_id=i.source_id
               AND result_stack.history_id=?{history_parameter}
               AND base_stack.history_id=?{history_parameter}
               AND (known_branch.branch_id IS NULL OR
                    (known_branch.head_commit_id=i.head_id AND known_branch.base_id=i.base_id))
               AND (known_result.source_id IS NULL OR known_result.result_id=i.result_id)"
        );
        let mut values = Vec::with_capacity(history_parameter);
        for (branch, result) in pairs {
            values.push(Value::Blob(branch.id.as_slice().to_vec()));
            values.push(Value::Blob(branch.head_commit_id.as_slice().to_vec()));
            values.push(Value::Blob(branch.base_id.as_slice().to_vec()));
            values.push(Value::Blob(result.source_id.as_slice().to_vec()));
            values.push(Value::Blob(result.result_id.as_slice().to_vec()));
        }
        values.resize(PAIRS * 5, Value::Null);
        values.push(Value::Blob(push.history_id.as_slice().to_vec()));
        let connection = self.connection()?;
        let mut statement = connection.prepare_cached(&sql)?;
        let valid: i64 = statement.query_row(params_from_iter(values), |row| row.get(0))?;
        if valid == pairs.len() as i64 {
            Ok(())
        } else {
            Err(StorageError::Integrity("Stack publication relationship"))
        }
    }

    #[allow(clippy::let_and_return)]
    pub fn add_results_for_result(&self, result: ResultId) -> Result<Vec<AddResultRecord>> {
        crate::schema::require_full(self)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT source_id FROM add_results WHERE result_id=?1 ORDER BY source_id")?;
        let records = statement
            .query_map([result.as_slice()], |row| row.get::<_, Vec<u8>>(0))?
            .map(|row| {
                Ok(AddResultRecord {
                    source_id: SourceId::from_slice(&row?)?,
                    result_id: result,
                })
            })
            .collect();
        records
    }

    #[doc(hidden)]
    pub fn preflight_branch_push(&self, branch: BranchRecord) -> Result<(Option<CommitId>, bool)> {
        let connection = self.connection()?;
        let existing = connection
            .query_row(
                "SELECT head_commit_id,base_id FROM branches WHERE branch_id=?1",
                [branch.id.as_slice()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?;
        let Some((head, base)) = existing else {
            return Ok((None, false));
        };
        let head = CommitId::from_slice(&head)?;
        if BaseId::from_slice(&base)? != branch.base_id {
            return Err(StorageError::Integrity("Branch base moved"));
        }
        let accepted = self.kind() == crate::SchemaKind::Full
            && connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM add_results WHERE source_id=?1)",
                [branch.id.as_slice()],
                |row| row.get::<_, bool>(0),
            )?;
        if accepted {
            return if head == branch.head_commit_id {
                Ok((Some(head), true))
            } else {
                Err(StorageError::Integrity("accepted Branch moved"))
            };
        }
        let contained = head == branch.head_commit_id
            || connection.query_row(
                COMMIT_ANCESTOR_SQL,
                [head.as_slice(), branch.head_commit_id.as_slice()],
                |row| row.get(0),
            )?;
        Ok((Some(head), contained))
    }

    #[doc(hidden)]
    pub fn preflight_stack_push(
        &self,
        history_id: StackHistoryId,
        base_layer_id: LayerId,
        incoming: StackId,
    ) -> Result<(Option<StackId>, bool)> {
        crate::schema::require_full(self)?;
        let connection = self.connection()?;
        let existing = connection
            .query_row(
                "SELECT base_layer_id,head_stack_id FROM stack_histories WHERE history_id=?1",
                [history_id.as_slice()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?;
        let Some((base, head)) = existing else {
            return Ok((None, false));
        };
        if LayerId::from_slice(&base)? != base_layer_id {
            return Err(StorageError::Integrity("StackHistory base"));
        }
        let head = StackId::from_slice(&head)?;
        let contained = head == incoming
            || crate::sql::linear_ancestor(
                &connection,
                "stacks",
                "stack_id",
                incoming.as_slice(),
                head.as_slice(),
            )?;
        Ok((Some(head), contained))
    }

    pub fn visit_commit_ancestry(
        &self,
        head: CommitId,
        fallback: Option<&dyn ObjectSource>,
        visitor: &mut CommitVisitor<'_>,
    ) -> Result<()> {
        let connection = self.connection()?;
        let source = CursorSource {
            connection: &connection,
            fallback,
        };
        let mut statement = connection.prepare(COMMIT_ANCESTRY_SQL)?;
        let mut rows = statement.query([head.as_slice()])?;
        let mut page = Vec::with_capacity(crate::ID_BATCH_COUNT);
        let mut found_head = false;
        while let Some(row) = rows.next()? {
            let id = CommitId::from_slice(&row.get::<_, Vec<u8>>(0)?)?;
            found_head |= id == head;
            page.push(CommitRecord {
                id,
                root_id: ObjectId::from_bytes(&row.get::<_, Vec<u8>>(1)?)?,
                parent_id: row
                    .get::<_, Option<Vec<u8>>>(2)?
                    .as_deref()
                    .map(CommitId::from_slice)
                    .transpose()?,
                merge_parent_id: row
                    .get::<_, Option<Vec<u8>>>(3)?
                    .as_deref()
                    .map(CommitId::from_slice)
                    .transpose()?,
            });
            if page.len() == crate::ID_BATCH_COUNT {
                visitor(&source, &page)?;
                page.clear();
            }
        }
        if !page.is_empty() {
            visitor(&source, &page)?;
        }
        if !found_head {
            return Err(StorageError::MissingBaseData);
        }
        Ok(())
    }

    pub fn is_commit_ancestor(&self, ancestor: CommitId, head: CommitId) -> Result<bool> {
        Ok(self.connection()?.query_row(
            COMMIT_ANCESTOR_SQL,
            [head.as_slice(), ancestor.as_slice()],
            |row| row.get(0),
        )?)
    }

    pub fn is_stack_ancestor(&self, ancestor: StackId, head: StackId) -> Result<bool> {
        crate::schema::require_full(self)?;
        let connection = self.connection()?;
        crate::sql::linear_ancestor(
            &connection,
            "stacks",
            "stack_id",
            ancestor.as_slice(),
            head.as_slice(),
        )
    }

    #[doc(hidden)]
    pub fn stack_positions(
        &self,
        history: StackHistoryId,
        head: StackId,
    ) -> Result<StackPositions> {
        crate::schema::require_full(self)?;
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "WITH RECURSIVE path(stack_id,parent_id,position) AS (
                SELECT stack_id,parent_id,0 FROM stacks
                WHERE stack_id=?1 AND history_id=?2
                UNION ALL
                SELECT s.stack_id,s.parent_id,p.position+1 FROM stacks s
                JOIN path p ON s.stack_id=p.parent_id WHERE s.history_id=?2
             ) SELECT stack_id,position FROM path ORDER BY position",
        )?;
        let mut rows = statement.query(params![head.as_slice(), history.as_slice()])?;
        let mut positions = StackPositions {
            memory: BTreeMap::new(),
            spill: None,
        };
        let mut page = Vec::with_capacity(ID_BATCH_COUNT);
        while let Some(row) = rows.next()? {
            let position = u64::try_from(row.get::<_, i64>(1)?)
                .map_err(|_| StorageError::Integrity("Stack position"))?;
            page.push((StackId::from_slice(&row.get::<_, Vec<u8>>(0)?)?, position));
            if page.len() == ID_BATCH_COUNT {
                positions.insert_page(&page)?;
                page.clear();
            }
        }
        if !page.is_empty() {
            positions.insert_page(&page)?;
        }
        if positions.position(head)? == Some(0) {
            Ok(positions)
        } else {
            Err(StorageError::MissingBaseData)
        }
    }

    pub fn visit_layers(
        &self,
        history_id: LayerHistoryId,
        through: LayerId,
        visitor: &mut dyn FnMut(&[LayerRecord]) -> Result<()>,
    ) -> Result<()> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(LAYER_PATH_SQL)?;
        let mut rows = statement.query([through.as_slice(), history_id.as_slice()])?;
        let mut page = Vec::with_capacity(crate::ID_BATCH_COUNT);
        let mut found = false;
        while let Some(row) = rows.next()? {
            found = true;
            page.push(LayerRecord {
                id: LayerId::from_slice(&row.get::<_, Vec<u8>>(0)?)?,
                history_id,
                parent_id: row
                    .get::<_, Option<Vec<u8>>>(1)?
                    .as_deref()
                    .map(LayerId::from_slice)
                    .transpose()?,
                root_id: ObjectId::from_bytes(&row.get::<_, Vec<u8>>(2)?)?,
            });
            if page.len() == crate::ID_BATCH_COUNT {
                visitor(&page)?;
                page.clear();
            }
        }
        finish_path(page, found, visitor)
    }

    pub fn visit_stacks(
        &self,
        history_id: StackHistoryId,
        through: StackId,
        visitor: &mut dyn FnMut(&[StackRecord]) -> Result<()>,
    ) -> Result<()> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(STACK_PATH_SQL)?;
        let mut rows = statement.query([through.as_slice(), history_id.as_slice()])?;
        let mut page = Vec::with_capacity(crate::ID_BATCH_COUNT);
        let mut found = false;
        while let Some(row) = rows.next()? {
            found = true;
            page.push(StackRecord {
                id: StackId::from_slice(&row.get::<_, Vec<u8>>(0)?)?,
                history_id,
                parent_id: row
                    .get::<_, Option<Vec<u8>>>(1)?
                    .as_deref()
                    .map(StackId::from_slice)
                    .transpose()?,
                root_id: ObjectId::from_bytes(&row.get::<_, Vec<u8>>(2)?)?,
            });
            if page.len() == crate::ID_BATCH_COUNT {
                visitor(&page)?;
                page.clear();
            }
        }
        finish_path(page, found, visitor)
    }
}

fn finish_path<T>(
    page: Vec<T>,
    found: bool,
    visitor: &mut dyn FnMut(&[T]) -> Result<()>,
) -> Result<()> {
    if !found {
        return Err(StorageError::MissingBaseData);
    }
    if !page.is_empty() {
        visitor(&page)?;
    }
    Ok(())
}

struct CursorSource<'a> {
    connection: &'a rusqlite::Connection,
    fallback: Option<&'a dyn ObjectSource>,
}

impl ObjectSource for CursorSource<'_> {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        if let Some(bytes) = crate::admission::object_rows(self.connection, &[id])?.remove(&id) {
            Ok(bytes)
        } else {
            self.fallback
                .ok_or(StorageError::MissingBaseData)?
                .read_object(id)
        }
    }

    fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        let mut rows = crate::admission::object_rows(self.connection, ids)?;
        let missing = ids
            .iter()
            .filter(|id| !rows.contains_key(id))
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            for object in self
                .fallback
                .ok_or(StorageError::MissingBaseData)?
                .read_objects(&missing)?
            {
                rows.insert(object.id, object.bytes);
            }
        }
        ids.iter()
            .map(|id| {
                Ok(CanonicalObject {
                    id: *id,
                    bytes: rows.get(id).cloned().ok_or(StorageError::MissingBaseData)?,
                })
            })
            .collect()
    }

    fn visit_objects(
        &self,
        ids: &[ObjectId],
        visitor: &mut dyn FnMut(CanonicalObject) -> Result<()>,
    ) -> Result<()> {
        let mut rows = crate::admission::object_rows(self.connection, ids)?;
        let mut missing = Vec::new();
        for id in ids {
            if let Some(bytes) = rows.remove(id) {
                visitor(CanonicalObject { id: *id, bytes })?;
            } else {
                missing.push(*id);
            }
        }
        if missing.is_empty() {
            Ok(())
        } else {
            self.fallback
                .ok_or(StorageError::MissingBaseData)?
                .visit_objects(&missing, visitor)
        }
    }
}

const COMMIT_ANCESTRY_SQL: &str = "WITH RECURSIVE ids(id) AS (
    SELECT ?1
    UNION SELECT c.parent_id FROM commits c JOIN ids ON c.commit_id=ids.id WHERE c.parent_id IS NOT NULL
    UNION SELECT c.merge_parent_id FROM commits c JOIN ids ON c.commit_id=ids.id WHERE c.merge_parent_id IS NOT NULL
), ordered(id,depth) AS (
    SELECT c.commit_id,0 FROM ids JOIN commits c ON c.commit_id=ids.id
    WHERE (c.parent_id IS NULL OR NOT EXISTS(SELECT 1 FROM ids p WHERE p.id=c.parent_id))
      AND (c.merge_parent_id IS NULL OR NOT EXISTS(SELECT 1 FROM ids p WHERE p.id=c.merge_parent_id))
    UNION
    SELECT c.commit_id,ordered.depth+1 FROM ordered
    JOIN commits c ON c.parent_id=ordered.id OR c.merge_parent_id=ordered.id
    JOIN ids ON ids.id=c.commit_id
), depths(id,depth) AS (SELECT id,max(depth) FROM ordered GROUP BY id)
SELECT c.commit_id,c.root_id,c.parent_id,c.merge_parent_id
FROM depths JOIN commits c ON c.commit_id=depths.id ORDER BY depths.depth,c.commit_id";

const COMMIT_ANCESTOR_SQL: &str = "WITH RECURSIVE ids(id) AS (
    SELECT ?1
    UNION SELECT c.parent_id FROM commits c JOIN ids ON c.commit_id=ids.id WHERE c.parent_id IS NOT NULL
    UNION SELECT c.merge_parent_id FROM commits c JOIN ids ON c.commit_id=ids.id WHERE c.merge_parent_id IS NOT NULL
) SELECT EXISTS(SELECT 1 FROM ids WHERE id=?2)";

const LAYER_PATH_SQL: &str = "WITH RECURSIVE path(id,parent_id,root_id,depth) AS (
    SELECT layer_id,parent_id,root_id,0 FROM layers WHERE layer_id=?1 AND history_id=?2
    UNION ALL
    SELECT l.layer_id,l.parent_id,l.root_id,path.depth+1 FROM layers l JOIN path ON l.layer_id=path.parent_id
    WHERE l.history_id=?2
) SELECT id,parent_id,root_id FROM path ORDER BY depth DESC";

const STACK_PATH_SQL: &str = "WITH RECURSIVE path(id,parent_id,root_id,depth) AS (
    SELECT stack_id,parent_id,root_id,0 FROM stacks WHERE stack_id=?1 AND history_id=?2
    UNION ALL
    SELECT s.stack_id,s.parent_id,s.root_id,path.depth+1 FROM stacks s JOIN path ON s.stack_id=path.parent_id
    WHERE s.history_id=?2
) SELECT id,parent_id,root_id FROM path ORDER BY depth DESC";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MergeBaseOutcome {
    Commit(CommitId),
    None,
}

pub fn commit_merge_base(
    db: &StoreDb,
    source: CommitId,
    target: CommitId,
) -> Result<MergeBaseOutcome> {
    let connection = db.connection()?;
    let mut statement = connection.prepare(COMMIT_BASE_SQL)?;
    let candidates = statement
        .query_map([source.as_slice(), target.as_slice()], |row| {
            row.get::<_, Vec<u8>>(0)
        })?
        .map(|row| CommitId::from_slice(&row?))
        .collect::<Result<Vec<_>>>()?;
    match candidates.as_slice() {
        [] => Ok(MergeBaseOutcome::None),
        [candidate] => Ok(MergeBaseOutcome::Commit(*candidate)),
        _ => Err(StorageError::AmbiguousMergeBase),
    }
}

pub fn closest_common_stack(
    db: &StoreDb,
    left: StackId,
    right: StackId,
) -> Result<Option<StackId>> {
    closest_linear(db, "stacks", "stack_id", left.as_slice(), right.as_slice())?
        .map(|bytes| StackId::from_slice(&bytes))
        .transpose()
}

pub fn closest_common_layer(
    db: &StoreDb,
    left: LayerId,
    right: LayerId,
) -> Result<Option<LayerId>> {
    closest_linear(db, "layers", "layer_id", left.as_slice(), right.as_slice())?
        .map(|bytes| LayerId::from_slice(&bytes))
        .transpose()
}

fn closest_linear(
    db: &StoreDb,
    table: &str,
    key: &str,
    left: &[u8],
    right: &[u8],
) -> Result<Option<Vec<u8>>> {
    if db.kind() != crate::SchemaKind::Full {
        return Err(StorageError::WrongSourceRoute);
    }
    let sql = format!(
        "WITH RECURSIVE
         left_path(id,depth) AS (
            SELECT ?1,0 UNION ALL
            SELECT n.parent_id,left_path.depth+1 FROM {table} n JOIN left_path ON n.{key}=left_path.id WHERE n.parent_id IS NOT NULL
         ),
         right_path(id,depth) AS (
            SELECT ?2,0 UNION ALL
            SELECT n.parent_id,right_path.depth+1 FROM {table} n JOIN right_path ON n.{key}=right_path.id WHERE n.parent_id IS NOT NULL
         )
         SELECT left_path.id FROM left_path JOIN right_path USING(id)
         ORDER BY left_path.depth+right_path.depth LIMIT 1"
    );
    Ok(db
        .connection()?
        .query_row(&sql, [left, right], |row| row.get::<_, Vec<u8>>(0))
        .optional()?)
}

const COMMIT_BASE_SQL: &str = "WITH RECURSIVE
source_anc(id) AS (
    SELECT ?1
    UNION SELECT c.parent_id FROM commits c JOIN source_anc a ON c.commit_id=a.id WHERE c.parent_id IS NOT NULL
    UNION SELECT c.merge_parent_id FROM commits c JOIN source_anc a ON c.commit_id=a.id WHERE c.merge_parent_id IS NOT NULL
),
target_anc(id) AS (
    SELECT ?2
    UNION SELECT c.parent_id FROM commits c JOIN target_anc a ON c.commit_id=a.id WHERE c.parent_id IS NOT NULL
    UNION SELECT c.merge_parent_id FROM commits c JOIN target_anc a ON c.commit_id=a.id WHERE c.merge_parent_id IS NOT NULL
),
common(id) AS (SELECT id FROM source_anc INTERSECT SELECT id FROM target_anc)
SELECT common.id FROM common
WHERE NOT EXISTS(
    SELECT 1 FROM commits child JOIN common newer ON newer.id=child.commit_id
    WHERE child.parent_id=common.id OR child.merge_parent_id=common.id
)
ORDER BY common.id LIMIT 2";

pub fn commit_merge_base_plan(db: &StoreDb) -> Result<Vec<String>> {
    let connection = db.connection()?;
    let sql = format!("EXPLAIN QUERY PLAN {COMMIT_BASE_SQL}");
    let mut statement = connection.prepare(&sql)?;
    let plan = statement
        .query_map([&[0_u8; 33][..], &[1_u8; 33][..]], |row| {
            row.get::<_, String>(3)
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(plan)
}

#[doc(hidden)]
pub fn visit_stack_push_facts(
    db: &StoreDb,
    history: StackHistoryId,
    expected: Option<StackId>,
    incoming: StackId,
    visitor: &mut dyn FnMut(&[Fact]) -> Result<()>,
) -> Result<()> {
    let connection = db.connection()?;
    let sql = format!(
        "{} , facts(kind,id,d1,d2,d3,ordinal) AS (
         SELECT 0,c.commit_id,c.root_id,c.parent_id,c.merge_parent_id,d.depth FROM depths d JOIN commits c ON c.commit_id=d.id
         UNION ALL SELECT 1,stack_id,?3,parent_id,root_id,-depth FROM suffix
         UNION ALL SELECT 2,b.branch_id,b.head_commit_id,b.base_id,NULL,0 FROM mapped m JOIN branches b ON b.branch_id=m.source_id
         UNION ALL SELECT 3,source_id,result_id,NULL,NULL,0 FROM mapped
         ) SELECT kind,id,d1,d2,d3 FROM facts ORDER BY kind,ordinal,id",
        provenance_cte()
    );
    let expected = expected.map(|id| id.to_bytes().to_vec());
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query(params![incoming.as_slice(), expected, history.as_slice()])?;
    let mut page = Vec::with_capacity(ID_BATCH_COUNT);
    while let Some(row) = rows.next()? {
        let kind = row.get::<_, i64>(0)?;
        let id = row.get::<_, Vec<u8>>(1)?;
        let d1 = row.get::<_, Vec<u8>>(2)?;
        let d2 = row.get::<_, Option<Vec<u8>>>(3)?;
        let d3 = row.get::<_, Option<Vec<u8>>>(4)?;
        page.push(match kind {
            0 => Fact::Commit(CommitRecord {
                id: CommitId::from_slice(&id)?,
                root_id: ObjectId::from_bytes(&d1)?,
                parent_id: d2.as_deref().map(CommitId::from_slice).transpose()?,
                merge_parent_id: d3.as_deref().map(CommitId::from_slice).transpose()?,
            }),
            1 => Fact::Stack(StackRecord {
                id: StackId::from_slice(&id)?,
                history_id: StackHistoryId::from_slice(&d1)?,
                parent_id: d2.as_deref().map(StackId::from_slice).transpose()?,
                root_id: ObjectId::from_bytes(
                    d3.as_deref().ok_or(StorageError::Integrity("Stack root"))?,
                )?,
            }),
            2 => Fact::Branch(BranchRecord {
                id: BranchId::from_slice(&id)?,
                head_commit_id: CommitId::from_slice(&d1)?,
                base_id: BaseId::from_slice(
                    d2.as_deref()
                        .ok_or(StorageError::Integrity("Branch base"))?,
                )?,
            }),
            3 => Fact::AddResult(AddResultRecord {
                source_id: SourceId::from_slice(&id)?,
                result_id: ResultId::from_slice(&d1)?,
            }),
            _ => return Err(StorageError::Integrity("provenance fact")),
        });
        if page.len() == ID_BATCH_COUNT {
            visitor(&page)?;
            page.clear();
        }
    }
    if page.is_empty() {
        Ok(())
    } else {
        visitor(&page)
    }
}

fn provenance_cte() -> &'static str {
    "WITH RECURSIVE suffix(stack_id,parent_id,root_id,depth) AS (
     SELECT stack_id,parent_id,root_id,0 FROM stacks WHERE stack_id=?1 AND history_id=?3
     UNION ALL SELECT s.stack_id,s.parent_id,s.root_id,x.depth+1 FROM stacks s JOIN suffix x ON s.stack_id=x.parent_id
     WHERE s.history_id=?3 AND (?2 IS NULL OR x.parent_id!=?2)
     ), mapped(source_id,result_id) AS (
     SELECT a.source_id,a.result_id FROM add_results a JOIN suffix s ON a.result_id=s.stack_id
     ), commit_ids(id) AS (
     SELECT b.head_commit_id FROM branches b JOIN mapped m ON b.branch_id=m.source_id
     UNION SELECT c.parent_id FROM commits c JOIN commit_ids i ON c.commit_id=i.id WHERE c.parent_id IS NOT NULL
     UNION SELECT c.merge_parent_id FROM commits c JOIN commit_ids i ON c.commit_id=i.id WHERE c.merge_parent_id IS NOT NULL
     ), ordered(id,depth) AS (
     SELECT c.commit_id,0 FROM commit_ids i JOIN commits c ON c.commit_id=i.id
     WHERE (c.parent_id IS NULL OR NOT EXISTS(SELECT 1 FROM commit_ids p WHERE p.id=c.parent_id))
       AND (c.merge_parent_id IS NULL OR NOT EXISTS(SELECT 1 FROM commit_ids p WHERE p.id=c.merge_parent_id))
     UNION SELECT c.commit_id,o.depth+1 FROM ordered o JOIN commits c ON c.parent_id=o.id OR c.merge_parent_id=o.id JOIN commit_ids i ON i.id=c.commit_id
     ), depths(id,depth) AS (SELECT id,max(depth) FROM ordered GROUP BY id)"
}
