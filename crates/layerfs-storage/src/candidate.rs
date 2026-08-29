use crate::{ObjectSource, Result};
use layerfs_content::filesystem::{self, ContentConflict};
use layerfs_content::ObjectId;

pub struct CandidateMerge {
    pub root_id: ObjectId,
    pub objects: build::DeferredObjectStore,
}

pub enum CandidateMergeOutcome {
    Clean(CandidateMerge),
    Conflict(ContentConflict),
}

pub fn merge_candidate(
    source: &dyn ObjectSource,
    base_root: ObjectId,
    current_root: ObjectId,
    candidate_root: ObjectId,
) -> Result<CandidateMergeOutcome> {
    let mut objects = build::ObjectBuffer::new(source)?;
    match filesystem::three_way(&mut objects, base_root, current_root, candidate_root)? {
        filesystem::ThreeWayOutcome::Clean(root_id) => {
            let built = objects.finish(root_id, 0)?;
            Ok(CandidateMergeOutcome::Clean(CandidateMerge {
                root_id,
                objects: built.objects,
            }))
        }
        filesystem::ThreeWayOutcome::Conflict(conflict) => {
            Ok(CandidateMergeOutcome::Conflict(conflict))
        }
    }
}

mod build {
    use crate::{CanonicalObject, ObjectSource, Result, StorageError};
    use layerfs_content::file::rope::{ObjectRead, ObjectStore};
    use layerfs_content::filesystem::{self as logical, ContentChange};
    use layerfs_content::object::references::referenced_objects;
    use layerfs_content::{CoreError, CoreResult, ObjectId};
    use rusqlite::OptionalExtension;
    use std::collections::{BTreeMap, BTreeSet, VecDeque};

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct BuildCounters {
        pub cdc_bytes_scanned: u64,
        pub encode_hash_invocations: u64,
    }

    pub struct BuiltRoot {
        pub root_id: ObjectId,
        pub objects: DeferredObjectStore,
        pub counters: BuildCounters,
    }

    pub struct CoreReader<'a>(pub &'a dyn ObjectSource);

    impl ObjectRead for CoreReader<'_> {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            self.0.read_object(id).map_err(|_| CoreError::MissingObject)
        }

        fn get_authenticated_batch<F>(&self, ids: &[ObjectId], mut callback: F) -> CoreResult<()>
        where
            F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
        {
            let objects = self
                .0
                .read_objects(ids)
                .map_err(|_| CoreError::MissingObject)?;
            if objects.len() != ids.len() {
                return Err(CoreError::MissingObject);
            }
            for (expected, object) in ids.iter().zip(objects) {
                if object.id != *expected || ObjectId::for_bytes(&object.bytes) != object.id {
                    return Err(CoreError::IdentityMismatch);
                }
                callback(
                    object.id,
                    layerfs_content::decode_bytes_object(&object.bytes)?,
                )?;
            }
            Ok(())
        }
    }

    #[doc(hidden)]
    pub struct DeferredObjectStore {
        storage: DeferredObjects,
        count: u64,
        encoded_bytes: u64,
    }

    enum DeferredObjects {
        Memory {
            order: VecDeque<ObjectId>,
            rows: BTreeMap<ObjectId, Vec<u8>>,
            bytes: usize,
        },
        Spill {
            connection: rusqlite::Connection,
            pending: Vec<(ObjectId, Vec<u8>)>,
            pending_bytes: usize,
            read_page: VecDeque<CanonicalObject>,
            cursor: i64,
        },
    }

    const DEFERRED_MEMORY_BYTES: usize = 8 * 1024 * 1024;

    pub(crate) enum SeenIds {
        Memory(BTreeSet<ObjectId>),
        Spill(rusqlite::Connection),
    }

    impl SeenIds {
        pub(crate) fn new(root: ObjectId) -> Result<Self> {
            Ok(Self::Memory(BTreeSet::from([root])))
        }

        pub(crate) fn insert_page(&mut self, ids: &[ObjectId]) -> Result<Vec<ObjectId>> {
            if ids.len() > crate::ID_BATCH_COUNT || ids.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err(StorageError::Integrity("seen-ID page"));
            }
            if let Self::Memory(seen) = self {
                let inserted = ids
                    .iter()
                    .copied()
                    .filter(|id| seen.insert(*id))
                    .collect::<Vec<_>>();
                if seen.len() <= DEFERRED_MEMORY_BYTES / 64 {
                    return Ok(inserted);
                }
                let mut connection = scratch_seen()?;
                for page in seen
                    .iter()
                    .copied()
                    .collect::<Vec<_>>()
                    .chunks(crate::ID_BATCH_COUNT)
                {
                    insert_seen_rows(&mut connection, page)?;
                }
                *self = Self::Spill(connection);
                return Ok(inserted);
            }
            let Self::Spill(connection) = self else {
                unreachable!()
            };
            insert_seen_rows(connection, ids)
        }
    }

    fn scratch_seen() -> Result<rusqlite::Connection> {
        let connection = rusqlite::Connection::open("")?;
        connection.pragma_update(None, "journal_mode", "OFF")?;
        connection.pragma_update(None, "synchronous", "OFF")?;
        connection.pragma_update(None, "temp_store", "FILE")?;
        connection.pragma_update(None, "cache_size", -8192_i64)?;
        connection.execute_batch(
            "CREATE TABLE seen(object_id BLOB PRIMARY KEY NOT NULL) WITHOUT ROWID",
        )?;
        Ok(connection)
    }

    fn insert_seen_rows(
        connection: &mut rusqlite::Connection,
        ids: &[ObjectId],
    ) -> Result<Vec<ObjectId>> {
        static SQL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let sql = SQL.get_or_init(|| {
            let values = (1..=crate::ID_BATCH_COUNT)
                .map(|index| format!("(?{index})"))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "INSERT INTO seen(object_id) SELECT column1 FROM (VALUES {values})
             WHERE column1 IS NOT NULL
             ON CONFLICT DO NOTHING RETURNING object_id"
            )
        });
        let mut parameters = ids
            .iter()
            .map(|id| rusqlite::types::Value::Blob(id.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        parameters.resize(crate::ID_BATCH_COUNT, rusqlite::types::Value::Null);
        let transaction = connection.transaction()?;
        let mut statement = transaction.prepare_cached(sql)?;
        let mut inserted = statement
            .query_map(rusqlite::params_from_iter(parameters), |row| {
                row.get::<_, Vec<u8>>(0)
            })?
            .map(|row| ObjectId::from_bytes(&row?).map_err(StorageError::Core))
            .collect::<Result<Vec<_>>>()?;
        drop(statement);
        transaction.commit()?;
        inserted.sort();
        Ok(inserted)
    }

    pub(crate) fn transfer_closure<S: ObjectSource + ?Sized>(
        source: &S,
        root: CanonicalObject,
        transfer: &mut crate::TransferPipeline<'_>,
    ) -> Result<()> {
        let root_id = root.id;
        let mut walk = ObjectTransfer {
            source,
            seen: SeenIds::new(root_id)?,
            active: BTreeSet::from([root_id]),
            transfer,
        };
        walk.visit(root)
    }

    pub fn object_batches(objects: &[CanonicalObject]) -> Result<Vec<&[CanonicalObject]>> {
        let mut batches = Vec::new();
        let mut start = 0;
        while start < objects.len() {
            let mut end = start;
            let mut bytes = 0;
            while end < objects.len() && end - start < crate::OBJECT_BATCH_COUNT {
                let next = objects[end].bytes.len();
                if next > layerfs_content::limits::MAX_OBJECT_BYTES {
                    return Err(StorageError::Integrity("object size"));
                }
                if end > start && bytes + next > crate::OBJECT_BATCH_BYTES {
                    break;
                }
                bytes += next;
                end += 1;
                if next > crate::OBJECT_BATCH_BYTES {
                    break;
                }
            }
            batches.push(&objects[start..end]);
            start = end;
        }
        Ok(batches)
    }

    struct ObjectTransfer<'source, 'borrow, 'destination, S: ObjectSource + ?Sized> {
        source: &'source S,
        seen: SeenIds,
        active: BTreeSet<ObjectId>,
        transfer: &'borrow mut crate::TransferPipeline<'destination>,
    }

    impl<S: ObjectSource + ?Sized> ObjectTransfer<'_, '_, '_, S> {
        fn visit(&mut self, object: CanonicalObject) -> Result<()> {
            crate::note_traversal_authentication();
            layerfs_content::authenticate_identity(&object.bytes, object.id)?;
            let mut children = BTreeSet::new();
            for child in referenced_objects(&object.bytes)? {
                if self.active.contains(&child) {
                    return Err(StorageError::Integrity("object cycle"));
                }
                children.insert(child);
            }
            let children = children.into_iter().collect::<Vec<_>>();
            for page in children.chunks(crate::ID_BATCH_COUNT) {
                let ids = self.seen.insert_page(page)?;
                if ids.is_empty() {
                    continue;
                }
                let missing = self.transfer.announce_objects(&ids)?;
                let mut selected = Vec::new();
                for (index, id) in ids.iter().enumerate() {
                    if missing.is_missing(index)? {
                        selected.push(*id);
                    }
                }
                let mut fetched = DeferredObjectStore::new()?;
                self.source
                    .visit_objects(&selected, &mut |object| fetched.stage(object))?;
                while let Some(child) = fetched.pop_first()? {
                    self.active.insert(child.id);
                    self.visit(child)?;
                }
            }
            self.active.remove(&object.id);
            self.transfer.stage_object(object)
        }
    }

    impl DeferredObjectStore {
        pub(crate) fn new() -> Result<Self> {
            Ok(Self {
                storage: DeferredObjects::Memory {
                    order: VecDeque::new(),
                    rows: BTreeMap::new(),
                    bytes: 0,
                },
                count: 0,
                encoded_bytes: 0,
            })
        }

        pub fn len(&self) -> u64 {
            self.count
        }

        pub fn is_empty(&self) -> bool {
            self.count == 0
        }

        pub fn encoded_bytes(&self) -> u64 {
            self.encoded_bytes
        }

        fn reachable_from(self, root: ObjectId) -> Result<Self> {
            let mut reachable = Self::new()?;
            let mut seen = SeenIds::new(root)?;
            let mut stack = vec![(root, false)];
            while let Some((id, expanded)) = stack.pop() {
                let Some(canonical) = self.get(id)? else {
                    continue;
                };
                if expanded {
                    reachable.put(id, &canonical)?;
                    continue;
                }
                stack.push((id, true));
                let mut children = referenced_objects(&canonical)?;
                children.sort();
                children.dedup();
                let mut inserted = Vec::with_capacity(children.len());
                for page in children.chunks(crate::ID_BATCH_COUNT) {
                    inserted.extend(seen.insert_page(page)?);
                }
                stack.extend(inserted.into_iter().rev().map(|child| (child, false)));
            }
            Ok(reachable)
        }

        fn get(&self, id: ObjectId) -> Result<Option<Vec<u8>>> {
            match &self.storage {
                DeferredObjects::Memory { rows, .. } => Ok(rows.get(&id).cloned()),
                DeferredObjects::Spill {
                    connection,
                    pending,
                    ..
                } => {
                    if let Some((_, bytes)) =
                        pending.iter().rev().find(|(pending, _)| *pending == id)
                    {
                        return Ok(Some(bytes.clone()));
                    }
                    Ok(connection
                        .query_row(
                            "SELECT bytes FROM objects WHERE object_id=?1",
                            [id.as_bytes().as_slice()],
                            |row| row.get(0),
                        )
                        .optional()?)
                }
            }
        }

        fn put(&mut self, id: ObjectId, canonical: &[u8]) -> Result<()> {
            let charge = canonical.len() + 64;
            if matches!(
                &self.storage,
                DeferredObjects::Memory { bytes, .. } if *bytes + charge > DEFERRED_MEMORY_BYTES
            ) {
                self.spill()?;
            }
            match &mut self.storage {
                DeferredObjects::Memory { order, rows, bytes } => {
                    order.push_back(id);
                    rows.insert(id, canonical.to_vec());
                    *bytes += charge;
                }
                DeferredObjects::Spill {
                    connection,
                    pending,
                    pending_bytes,
                    ..
                } => {
                    if !pending.is_empty()
                        && (pending.len() == crate::OBJECT_BATCH_COUNT
                            || *pending_bytes + canonical.len() > crate::OBJECT_BATCH_BYTES)
                    {
                        flush_objects(connection, pending, pending_bytes)?;
                    }
                    pending.push((id, canonical.to_vec()));
                    *pending_bytes += canonical.len();
                    if pending.len() == crate::OBJECT_BATCH_COUNT
                        || *pending_bytes >= crate::OBJECT_BATCH_BYTES
                    {
                        flush_objects(connection, pending, pending_bytes)?;
                    }
                }
            }
            self.count += 1;
            self.encoded_bytes = self
                .encoded_bytes
                .checked_add(canonical.len() as u64)
                .ok_or(StorageError::Integrity("candidate bytes"))?;
            Ok(())
        }

        fn spill(&mut self) -> Result<()> {
            let DeferredObjects::Memory { order, rows, .. } = std::mem::replace(
                &mut self.storage,
                DeferredObjects::Memory {
                    order: VecDeque::new(),
                    rows: BTreeMap::new(),
                    bytes: 0,
                },
            ) else {
                return Ok(());
            };
            let mut connection = rusqlite::Connection::open("")?;
            connection.pragma_update(None, "journal_mode", "OFF")?;
            connection.pragma_update(None, "synchronous", "OFF")?;
            connection.pragma_update(None, "temp_store", "FILE")?;
            connection.pragma_update(None, "cache_size", -8192_i64)?;
            connection.execute_batch(
                "CREATE TABLE objects(
                sequence INTEGER PRIMARY KEY,
                object_id BLOB NOT NULL UNIQUE,
                bytes BLOB NOT NULL
             ) STRICT",
            )?;
            let transaction = connection.transaction()?;
            {
                let mut insert =
                    transaction.prepare("INSERT INTO objects(object_id,bytes) VALUES(?1,?2)")?;
                for id in order {
                    insert.execute(rusqlite::params![
                        id.as_bytes().as_slice(),
                        rows.get(&id)
                            .ok_or(StorageError::Integrity("deferred object"))?
                    ])?;
                }
            }
            transaction.commit()?;
            self.storage = DeferredObjects::Spill {
                connection,
                pending: Vec::new(),
                pending_bytes: 0,
                read_page: VecDeque::new(),
                cursor: 0,
            };
            Ok(())
        }

        pub(crate) fn stage(&mut self, object: CanonicalObject) -> Result<()> {
            if let Some(known) = self.get(object.id)? {
                return if known == object.bytes {
                    Ok(())
                } else {
                    Err(StorageError::Integrity("deferred object collision"))
                };
            }
            self.put(object.id, &object.bytes)
        }

        pub(crate) fn pop_first(&mut self) -> Result<Option<CanonicalObject>> {
            let object = match &mut self.storage {
                DeferredObjects::Memory { order, rows, .. } => order
                    .pop_front()
                    .and_then(|id| rows.remove(&id).map(|bytes| CanonicalObject { id, bytes })),
                DeferredObjects::Spill {
                    connection,
                    pending,
                    pending_bytes,
                    read_page,
                    cursor,
                } => {
                    if read_page.is_empty() {
                        flush_objects(connection, pending, pending_bytes)?;
                        let mut statement = connection.prepare(
                            "SELECT sequence,object_id,bytes FROM objects
                         WHERE sequence>?1 ORDER BY sequence LIMIT 128",
                        )?;
                        let rows = statement.query_map([*cursor], |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, Vec<u8>>(1)?,
                                row.get::<_, Vec<u8>>(2)?,
                            ))
                        })?;
                        for row in rows {
                            let (sequence, id, bytes) = row?;
                            *cursor = sequence;
                            read_page.push_back(CanonicalObject {
                                id: ObjectId::from_bytes(&id)?,
                                bytes,
                            });
                        }
                    }
                    read_page.pop_front()
                }
            };
            if let Some(object) = &object {
                self.count -= 1;
                self.encoded_bytes -= object.bytes.len() as u64;
            }
            Ok(object)
        }

        pub fn visit_batches(
            &self,
            visitor: &mut dyn FnMut(&[CanonicalObject], bool) -> Result<()>,
        ) -> Result<()> {
            let mut batch = Vec::with_capacity(crate::OBJECT_BATCH_COUNT);
            let mut bytes = 0_usize;
            let mut emit = |object: CanonicalObject| -> Result<()> {
                if !batch.is_empty()
                    && (batch.len() == crate::OBJECT_BATCH_COUNT
                        || bytes + object.bytes.len() > crate::OBJECT_BATCH_BYTES)
                {
                    visitor(&batch, false)?;
                    batch.clear();
                    bytes = 0;
                }
                bytes += object.bytes.len();
                batch.push(object);
                Ok(())
            };
            match &self.storage {
                DeferredObjects::Memory { order, rows, .. } => {
                    for id in order {
                        emit(CanonicalObject {
                            id: *id,
                            bytes: rows
                                .get(id)
                                .ok_or(StorageError::Integrity("deferred object"))?
                                .clone(),
                        })?;
                    }
                }
                DeferredObjects::Spill {
                    connection,
                    pending,
                    ..
                } => {
                    let mut statement = connection
                        .prepare("SELECT object_id,bytes FROM objects ORDER BY sequence")?;
                    let rows = statement.query_map([], |row| {
                        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                    })?;
                    for row in rows {
                        let (id, bytes) = row?;
                        emit(CanonicalObject {
                            id: ObjectId::from_bytes(&id)?,
                            bytes,
                        })?;
                    }
                    for (id, bytes) in pending {
                        emit(CanonicalObject {
                            id: *id,
                            bytes: bytes.clone(),
                        })?;
                    }
                }
            }
            if !batch.is_empty() {
                visitor(&batch, true)?;
            }
            Ok(())
        }

        #[cfg(test)]
        fn spilled(&self) -> bool {
            matches!(self.storage, DeferredObjects::Spill { .. })
        }
    }

    fn flush_objects(
        connection: &mut rusqlite::Connection,
        pending: &mut Vec<(ObjectId, Vec<u8>)>,
        pending_bytes: &mut usize,
    ) -> Result<()> {
        if pending.is_empty() {
            return Ok(());
        }
        let transaction = connection.transaction()?;
        {
            let mut insert =
                transaction.prepare("INSERT INTO objects(object_id,bytes) VALUES(?1,?2)")?;
            for (id, bytes) in pending.drain(..) {
                insert.execute(rusqlite::params![id.as_bytes().as_slice(), bytes])?;
            }
        }
        transaction.commit()?;
        *pending_bytes = 0;
        Ok(())
    }

    pub struct ObjectBuffer<'a> {
        source: Option<&'a dyn ObjectSource>,
        objects: DeferredObjectStore,
    }

    impl<'a> ObjectBuffer<'a> {
        pub fn new(source: &'a dyn ObjectSource) -> Result<Self> {
            Ok(Self {
                source: Some(source),
                objects: DeferredObjectStore::new()?,
            })
        }

        pub fn empty() -> Result<Self> {
            Ok(Self {
                source: None,
                objects: DeferredObjectStore::new()?,
            })
        }

        pub fn finish(self, root_id: ObjectId, cdc_bytes_scanned: u64) -> Result<BuiltRoot> {
            let encode_hash_invocations = self.objects.len();
            Ok(BuiltRoot {
                root_id,
                counters: BuildCounters {
                    cdc_bytes_scanned,
                    encode_hash_invocations,
                },
                objects: self.objects.reachable_from(root_id)?,
            })
        }
    }

    impl ObjectStore for ObjectBuffer<'_> {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            if let Some(bytes) = self.objects.get(id).map_err(|_| CoreError::MissingObject)? {
                return Ok(bytes);
            }
            self.source
                .ok_or(CoreError::MissingObject)?
                .read_object(id)
                .map_err(|_| CoreError::MissingObject)
        }

        fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
            let id = ObjectId::for_bytes(canonical);
            if let Some(bytes) = self.objects.get(id).map_err(|_| CoreError::MissingObject)? {
                if bytes != canonical {
                    return Err(CoreError::IdentityMismatch);
                }
                return Ok(id);
            }
            self.objects
                .put(id, canonical)
                .map_err(|_| CoreError::MissingObject)?;
            Ok(id)
        }
    }

    impl ObjectSource for ObjectBuffer<'_> {
        fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
            ObjectStore::get(self, id).map_err(StorageError::Core)
        }
    }

    pub fn empty_root(seed: [u8; 32]) -> Result<BuiltRoot> {
        let mut store = ObjectBuffer::empty()?;
        let root_id = logical::empty_root(&mut store, seed)?;
        store.finish(root_id, 0)
    }

    pub fn apply_changes(
        source: &dyn ObjectSource,
        base_root: ObjectId,
        changes: &[ContentChange],
        seed: [u8; 32],
    ) -> Result<BuiltRoot> {
        let mut store = ObjectBuffer::new(source)?;
        let applied = logical::apply_changes(&mut store, base_root, changes, seed)?;
        store.finish(applied.root_id, applied.counters.cdc_bytes_scanned)
    }

    pub fn dependency_order(
        source: &(impl ObjectSource + ?Sized),
        root: ObjectId,
    ) -> Result<Vec<ObjectId>> {
        let mut ordered = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack = vec![(root, false)];
        while let Some((id, expanded)) = stack.pop() {
            if expanded {
                ordered.push(id);
                continue;
            }
            if !seen.insert(id) {
                continue;
            }
            let canonical = source.read_object(id)?;
            layerfs_content::authenticate_identity(&canonical, id)?;
            stack.push((id, true));
            let children = referenced_objects(&canonical)?;
            stack.extend(children.into_iter().rev().map(|child| (child, false)));
        }
        Ok(ordered)
    }

    #[cfg(test)]
    mod scratch_tests {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/merkle_unit.rs"
        ));
    }
}

pub use build::*;
