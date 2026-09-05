use crate::objects::CoreReader;
use crate::records::{decode_branch, decode_commit, decode_layer, decode_layer_stack};
use crate::{
    BranchId, BranchRecord, BranchRecordPage, CanonicalStorage, CommitId, CommitRecord,
    CommitRecordPage, DiffRequest, LayerId, LayerRecord, LayerRecordPage, LayerStackId,
    LayerStackRecord, LayerStackRecordPage, LayerStackStore, Result, StoreCounts, StoreError,
    StoreStorageSnapshot,
};
use layerfs_content::ObjectId;
use rusqlite::OptionalExtension;
use std::collections::BTreeSet;

const ENTITY_PAGE_MAX: u16 = 512;
const HISTORY_PAGE_MAX: u16 = 128;

impl LayerStackStore {
    pub fn layer_stack(&self, id: LayerStackId) -> Result<Option<LayerStackRecord>> {
        Ok(self
            .db
            .reader()?
            .query_row(
                crate::statements::layerstack::GET,
                [id.as_slice()],
                decode_layer_stack,
            )
            .optional()?)
    }

    pub fn layer_stack_by_name(
        &self,
        name: &crate::EntityName,
    ) -> Result<Option<LayerStackRecord>> {
        Ok(self
            .db
            .reader()?
            .query_row(
                crate::statements::layerstack::GET_BY_NAME,
                [name.as_str()],
                decode_layer_stack,
            )
            .optional()?)
    }

    pub fn layer_stack_record_page(
        &self,
        after: Option<LayerStackId>,
        limit: u16,
    ) -> Result<LayerStackRecordPage> {
        validate_entity_limit(limit)?;
        let cursor = after.map_or([0; 17], LayerStackId::to_bytes);
        let connection = self.db.reader()?;
        let mut statement = connection.prepare_cached(crate::statements::layerstack::LIST)?;
        let mut records = statement
            .query_map(
                rusqlite::params![cursor.as_slice(), i64::from(limit) + 1],
                decode_layer_stack,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(finish_page(&mut records, limit, |record| record.id))
    }

    pub fn layer(&self, id: LayerId) -> Result<Option<LayerRecord>> {
        Ok(self
            .db
            .reader()?
            .query_row(
                crate::statements::layerstack::GET_LAYER,
                [id.as_slice()],
                decode_layer,
            )
            .optional()?)
    }

    pub fn layer_record_page(
        &self,
        layer_stack_id: Option<LayerStackId>,
        after: Option<LayerId>,
        limit: u16,
    ) -> Result<LayerRecordPage> {
        validate_entity_limit(limit)?;
        let cursor = after.map_or([0; 33], LayerId::to_bytes);
        let connection = self.db.reader()?;
        let mut statement =
            connection.prepare_cached(crate::statements::layerstack::LIST_LAYERS)?;
        let mut records = statement
            .query_map(
                rusqlite::params![
                    layer_stack_id.map(|id| id.to_bytes().to_vec()),
                    cursor.as_slice(),
                    i64::from(limit) + 1
                ],
                decode_layer,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(finish_page(&mut records, limit, |record| record.id))
    }

    pub fn layer_history_page(&self, start: LayerId, limit: u16) -> Result<LayerRecordPage> {
        validate_history_limit(limit)?;
        let connection = self.db.reader()?;
        let mut statement =
            connection.prepare_cached(crate::statements::layerstack::HISTORY_PAGE)?;
        let mut records = statement
            .query_map(
                rusqlite::params![start.as_slice(), i64::from(limit) + 1],
                decode_layer,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(finish_page(&mut records, limit, |record| record.id))
    }

    pub fn branch(&self, id: BranchId) -> Result<Option<BranchRecord>> {
        Ok(self
            .db
            .reader()?
            .query_row(
                crate::statements::branch::GET,
                [id.as_slice()],
                decode_branch,
            )
            .optional()?)
    }

    pub fn branch_by_name(
        &self,
        layer_stack_id: LayerStackId,
        name: &crate::EntityName,
    ) -> Result<Option<BranchRecord>> {
        Ok(self
            .db
            .reader()?
            .query_row(
                crate::statements::branch::GET_BY_NAME,
                rusqlite::params![layer_stack_id.as_slice(), name.as_str()],
                decode_branch,
            )
            .optional()?)
    }

    pub fn branch_record_page(
        &self,
        layer_stack_id: Option<LayerStackId>,
        after: Option<BranchId>,
        limit: u16,
    ) -> Result<BranchRecordPage> {
        validate_entity_limit(limit)?;
        let cursor = after.map_or([0; 17], BranchId::to_bytes);
        let connection = self.db.reader()?;
        let mut statement = connection.prepare_cached(crate::statements::branch::LIST)?;
        let mut records = statement
            .query_map(
                rusqlite::params![
                    layer_stack_id.map(|id| id.to_bytes().to_vec()),
                    cursor.as_slice(),
                    i64::from(limit) + 1
                ],
                decode_branch,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(finish_page(&mut records, limit, |record| record.id))
    }

    pub fn commit(&self, id: CommitId) -> Result<Option<CommitRecord>> {
        Ok(self
            .db
            .reader()?
            .query_row(
                crate::statements::branch::GET_COMMIT,
                [id.as_slice()],
                decode_commit,
            )
            .optional()?)
    }

    pub fn commit_record_page(
        &self,
        after: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitRecordPage> {
        validate_entity_limit(limit)?;
        let cursor = after.map_or([0; 33], CommitId::to_bytes);
        let connection = self.db.reader()?;
        let mut statement = connection.prepare_cached(crate::statements::branch::LIST_COMMITS)?;
        let mut records = statement
            .query_map(
                rusqlite::params![cursor.as_slice(), i64::from(limit) + 1],
                decode_commit,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(finish_page(&mut records, limit, |record| record.id))
    }

    pub fn commit_history_page(&self, start: CommitId, limit: u16) -> Result<CommitRecordPage> {
        validate_history_limit(limit)?;
        let connection = self.db.reader()?;
        let mut statement = connection.prepare_cached(crate::statements::branch::HISTORY_PAGE)?;
        let mut records = statement
            .query_map(
                rusqlite::params![start.as_slice(), i64::from(limit) + 1],
                decode_commit,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(finish_page(&mut records, limit, |record| record.id))
    }

    pub fn visit_diff(
        &self,
        request: DiffRequest,
        mut visitor: impl FnMut(layerfs_content::filesystem::DiffEntry) -> Result<()>,
    ) -> Result<()> {
        let (from, to) = match request {
            DiffRequest::BranchCommits {
                branch_id,
                from_commit_id,
                to_commit_id,
            } => {
                if !self.branch_contains_commit(branch_id, from_commit_id)?
                    || !self.branch_contains_commit(branch_id, to_commit_id)?
                {
                    return Err(StoreError::InvalidInput("Commit outside Branch history"));
                }
                let from = self
                    .commit(from_commit_id)?
                    .ok_or(StoreError::NotFound("Commit"))?;
                let to = self
                    .commit(to_commit_id)?
                    .ok_or(StoreError::NotFound("Commit"))?;
                (from.root_id, to.root_id)
            }
            DiffRequest::BranchLayer {
                branch_id,
                layer_id,
            } => {
                let pinned = self.pin_branch(branch_id)?;
                let layer = self.layer(layer_id)?.ok_or(StoreError::NotFound("Layer"))?;
                if pinned.branch.layer_stack_id != layer.layer_stack_id {
                    return Err(StoreError::InvalidInput("LayerStack mismatch"));
                }
                (pinned.root, layer.root_id)
            }
            DiffRequest::Layers {
                from_layer_id,
                to_layer_id,
            } => {
                let from = self
                    .layer(from_layer_id)?
                    .ok_or(StoreError::NotFound("Layer"))?;
                let to = self
                    .layer(to_layer_id)?
                    .ok_or(StoreError::NotFound("Layer"))?;
                if from.layer_stack_id != to.layer_stack_id {
                    return Err(StoreError::InvalidInput("LayerStack mismatch"));
                }
                (from.root_id, to.root_id)
            }
        };
        layerfs_content::filesystem::diff_roots(&CoreReader(self), from, to, |entry| {
            visitor(entry).map_err(|_| layerfs_content::CoreError::Io)
        })?;
        Ok(())
    }

    pub fn store_counts(&self) -> Result<StoreCounts> {
        let values =
            self.db
                .reader()?
                .query_row(crate::statements::query::STORE_COUNTS, [], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                })?;
        Ok(StoreCounts {
            objects: count(values.0)?,
            commits: count(values.1)?,
            branches: count(values.2)?,
            layer_stacks: count(values.3)?,
            layers: count(values.4)?,
        })
    }

    pub fn canonical_storage(&self) -> Result<CanonicalStorage> {
        let (objects, bytes) = self.db.reader()?.query_row(
            crate::statements::query::CANONICAL_STORAGE,
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )?;
        Ok(CanonicalStorage {
            objects: count(objects)?,
            encoded_bytes: count(bytes)?,
        })
    }

    pub fn reachable_storage(&self) -> Result<CanonicalStorage> {
        let mut seen = crate::SpillableObjectSet::empty()?;
        let mut active = BTreeSet::new();
        let mut objects = 0_u64;
        let mut encoded_bytes = 0_u64;

        let mut cursor = [0; 33];
        loop {
            let connection = self.db.reader()?;
            let mut statement =
                connection.prepare_cached(crate::statements::query::LAYER_ROOTS_PAGE)?;
            let rows = statement
                .query_map(rusqlite::params![cursor.as_slice(), 128_i64], |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(statement);
            drop(connection);
            for (_, root) in &rows {
                traverse_root(
                    self,
                    ObjectId::from_bytes(root)?,
                    &mut seen,
                    &mut active,
                    &mut objects,
                    &mut encoded_bytes,
                )?;
            }
            let Some((last, _)) = rows.last() else { break };
            cursor.copy_from_slice(last);
            if rows.len() < 128 {
                break;
            }
        }

        let mut cursor = [0; 33];
        loop {
            let connection = self.db.reader()?;
            let mut statement =
                connection.prepare_cached(crate::statements::query::COMMIT_ROOTS_PAGE)?;
            let rows = statement
                .query_map(rusqlite::params![cursor.as_slice(), 128_i64], |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(statement);
            drop(connection);
            for (_, root) in &rows {
                traverse_root(
                    self,
                    ObjectId::from_bytes(root)?,
                    &mut seen,
                    &mut active,
                    &mut objects,
                    &mut encoded_bytes,
                )?;
            }
            let Some((last, _)) = rows.last() else { break };
            cursor.copy_from_slice(last);
            if rows.len() < 128 {
                break;
            }
        }

        let mut cursor = [0; 17];
        loop {
            let connection = self.db.reader()?;
            let mut statement =
                connection.prepare_cached(crate::statements::query::BRANCH_ROOTS_PAGE)?;
            let rows = statement
                .query_map(rusqlite::params![cursor.as_slice(), 128_i64], |row| {
                    Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(statement);
            drop(connection);
            for (_, root) in &rows {
                traverse_root(
                    self,
                    ObjectId::from_bytes(root)?,
                    &mut seen,
                    &mut active,
                    &mut objects,
                    &mut encoded_bytes,
                )?;
            }
            let Some((last, _)) = rows.last() else { break };
            cursor.copy_from_slice(last);
            if rows.len() < 128 {
                break;
            }
        }

        Ok(CanonicalStorage {
            objects,
            encoded_bytes,
        })
    }

    pub fn reachable_root_storage(&self, root: ObjectId) -> Result<CanonicalStorage> {
        let mut seen = crate::SpillableObjectSet::empty()?;
        let mut active = BTreeSet::new();
        let mut objects = 0_u64;
        let mut encoded_bytes = 0_u64;
        traverse_root(
            self,
            root,
            &mut seen,
            &mut active,
            &mut objects,
            &mut encoded_bytes,
        )?;
        Ok(CanonicalStorage {
            objects,
            encoded_bytes,
        })
    }

    pub fn storage_snapshot(&self) -> Result<StoreStorageSnapshot> {
        fn len(path: &std::path::Path) -> Result<u64> {
            match std::fs::metadata(path) {
                Ok(metadata) => Ok(metadata.len()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
                Err(error) => Err(error.into()),
            }
        }
        Ok(StoreStorageSnapshot {
            database_bytes: len(self.path())?,
        })
    }
}

impl crate::objects::ObjectSource for LayerStackStore {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        self.db.read_object_row(id)
    }

    fn read_authenticated_objects(
        &self,
        ids: &[ObjectId],
    ) -> Result<Vec<crate::objects::CanonicalObject>> {
        self.db.read_object_rows(ids)
    }
}

fn validate_entity_limit(limit: u16) -> Result<()> {
    if limit == 0 || limit > ENTITY_PAGE_MAX {
        return Err(StoreError::InvalidInput("entity page"));
    }
    Ok(())
}

fn validate_history_limit(limit: u16) -> Result<()> {
    if limit == 0 || limit > HISTORY_PAGE_MAX {
        return Err(StoreError::InvalidInput("history page"));
    }
    Ok(())
}

fn finish_page<T, I: Copy>(
    records: &mut Vec<T>,
    limit: u16,
    id: impl Fn(&T) -> I,
) -> crate::Page<T, I> {
    let has_more = records.len() > usize::from(limit);
    let continuation = has_more.then(|| id(&records[usize::from(limit) - 1]));
    records.truncate(usize::from(limit));
    crate::Page {
        records: std::mem::take(records),
        continuation,
    }
}

fn count(value: i64) -> Result<u64> {
    value.try_into().map_err(|_| StoreError::Integrity("count"))
}

fn traverse_root(
    store: &LayerStackStore,
    id: ObjectId,
    seen: &mut crate::SpillableObjectSet,
    active: &mut BTreeSet<ObjectId>,
    objects: &mut u64,
    encoded_bytes: &mut u64,
) -> Result<()> {
    if seen.insert_page(&[id])?.is_empty() {
        return Ok(());
    }
    if !active.insert(id) {
        return Err(StoreError::Integrity("object cycle"));
    }
    let canonical = store.db.read_object_row(id)?;
    *objects = objects
        .checked_add(1)
        .ok_or(StoreError::Integrity("reachable storage overflow"))?;
    *encoded_bytes = encoded_bytes
        .checked_add(
            canonical
                .len()
                .try_into()
                .map_err(|_| StoreError::Integrity("reachable storage overflow"))?,
        )
        .ok_or(StoreError::Integrity("reachable storage overflow"))?;
    let mut children = layerfs_content::object::references::referenced_objects(&canonical)?;
    children.sort();
    children.dedup();
    for child in children {
        if active.contains(&child) {
            return Err(StoreError::Integrity("object cycle"));
        }
        traverse_root(store, child, seen, active, objects, encoded_bytes)?;
    }
    active.remove(&id);
    Ok(())
}
