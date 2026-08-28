use crate::{
    checked_add, map_sqlite_error, with_authenticated_canonical_on_connection,
    with_read_canonical_on_connection, DeltaRecord, Engine, EngineError, EngineResult, FullStorage,
    ObjectRecord, RootId, RootRecord,
};
use layerfs_core::content::rope::ObjectRead;
use layerfs_core::CoreError;
use layerfs_core::{ObjectId, ObjectKind};
use rusqlite::{params, Connection, OptionalExtension};
use std::ops::Range;

#[cfg(test)]
const ROOT_RECORD_BASE_BYTES: u64 = 64;

impl ObjectRead for Engine {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, CoreError> {
        self.load_object(id)
            .map(|object| object.canonical_bytes)
            .map_err(core_store_error)
    }

    fn get_authenticated_batch<F>(&self, ids: &[ObjectId], mut callback: F) -> Result<(), CoreError>
    where
        F: FnMut(ObjectId, &[u8]) -> Result<(), CoreError>,
    {
        self.for_each_authenticated_payload_batch(ids, |id, bytes| {
            callback(id, bytes).map_err(EngineError::Core)
        })
        .map_err(core_store_error)
    }

    fn get_authenticated_payload_ranges_batch<F>(
        &self,
        requests: &[(ObjectId, Range<u64>)],
        maximum_payload_len: u64,
        mut callback: F,
    ) -> Result<(), CoreError>
    where
        F: FnMut(ObjectId, &[u8]) -> Result<(), CoreError>,
    {
        self.for_each_authenticated_payload_range_batch(
            requests,
            maximum_payload_len,
            |id, bytes| callback(id, bytes).map_err(EngineError::Core),
        )
        .map_err(core_store_error)
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> Result<T, CoreError>
    where
        F: FnOnce(&[u8]) -> Result<T, CoreError>,
    {
        let connection = self.lock_connection().map_err(core_store_error)?;
        with_read_canonical_on_connection(self, &connection, id, true, true, |_, bytes| {
            callback(bytes).map_err(EngineError::Core)
        })
        .map_err(core_store_error)
    }
}

impl ObjectRead for FullStorage {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, CoreError> {
        self.load_canonical_authenticated_bounded(id, usize::MAX)
            .map_err(core_store_error)
    }
}

pub(crate) fn core_store_error(error: EngineError) -> CoreError {
    match error {
        EngineError::Core(error) | EngineError::MalformedObject { cause: error, .. } => error,
        EngineError::MissingObject(_) => CoreError::MissingObject,
        EngineError::IdentityMismatch { .. } | EngineError::ImmutableConflict(_, _) => {
            CoreError::IdentityMismatch
        }
        EngineError::CounterOverflow => CoreError::LengthOverflow,
        EngineError::InvalidRange { start, end, length } => {
            CoreError::InvalidRange { start, end, length }
        }
        _ => CoreError::Io,
    }
}

impl Engine {
    pub fn load_visible_root(&self) -> EngineResult<Option<RootId>> {
        let connection = self.lock_connection()?;
        let root = visible_root_on_connection(self, &connection)?;
        if let Some(root) = root {
            let record = load_root_on_connection(self, &connection, root)?;
            read_directory_object(self, &connection, record.directory_object)?;
        }
        Ok(root)
    }

    pub fn load_root(&self, id: RootId) -> EngineResult<RootRecord> {
        let connection = self.lock_connection()?;
        let record = load_root_on_connection(self, &connection, id)?;
        read_directory_object(self, &connection, record.directory_object)?;
        Ok(record)
    }

    pub fn load_delta(&self, id: ObjectId) -> EngineResult<DeltaRecord> {
        let connection = self.lock_connection()?;
        load_delta_on_connection(self, &connection, id)
    }

    pub fn load_object(&self, id: ObjectId) -> EngineResult<ObjectRecord> {
        let connection = self.lock_connection()?;
        with_read_canonical_on_connection(self, &connection, id, false, false, |kind, bytes| {
            Ok(ObjectRecord {
                id,
                kind,
                canonical_len: bytes.len() as u64,
                canonical_bytes: bytes.to_vec(),
            })
        })
    }

    pub fn object_length(&self, id: ObjectId) -> EngineResult<u64> {
        let connection = self.lock_connection()?;
        with_read_canonical_on_connection(self, &connection, id, false, false, |_, bytes| {
            Ok(bytes.len() as u64)
        })
    }

    pub fn read_object_range(&self, id: ObjectId, range: Range<u64>) -> EngineResult<Vec<u8>> {
        let connection = self.lock_connection()?;
        with_read_canonical_on_connection(self, &connection, id, false, false, |_, bytes| {
            let length = bytes.len() as u64;
            if range.start > range.end || range.end > length {
                return Err(EngineError::InvalidRange {
                    start: range.start,
                    end: range.end,
                    length,
                });
            }
            let start = usize::try_from(range.start).map_err(|_| EngineError::CounterOverflow)?;
            let end = usize::try_from(range.end).map_err(|_| EngineError::CounterOverflow)?;
            let output = bytes[start..end].to_vec();
            let requested = range.end - range.start;
            self.bump(|counters| {
                checked_add(&mut counters.range_bytes_requested, requested)?;
                checked_add(&mut counters.range_bytes_returned, requested)
            })?;
            Ok(output)
        })
    }
}

impl FullStorage {
    pub fn contains_authenticated_object(&self, id: ObjectId) -> EngineResult<bool> {
        let connection = self.lock_connection()?;
        let row = connection
            .query_row(
                "SELECT kind, canonical_length, canonical_bytes
                 FROM layerfs_objects WHERE object_id = ?1",
                params![id.as_bytes()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite_error)?;
        let Some((kind, length, bytes)) = row else {
            return Ok(false);
        };
        crate::integrity::full::object::authenticate_object_row(id, kind, length, &bytes)?;
        Ok(true)
    }

    pub fn load_canonical_authenticated_bounded(
        &self,
        id: ObjectId,
        maximum: usize,
    ) -> EngineResult<Vec<u8>> {
        let connection = self.lock_connection()?;
        let row = connection
            .query_row(
                "SELECT kind, canonical_length, canonical_bytes
                 FROM layerfs_objects WHERE object_id = ?1",
                params![id.as_bytes()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or(EngineError::MissingObject(id))?;
        if usize::try_from(row.1)
            .ok()
            .is_none_or(|length| length > maximum)
        {
            return Err(EngineError::InvalidRecord("object transfer bound"));
        }
        crate::integrity::full::object::authenticate_object_row(id, row.0, row.1, &row.2)?;
        Ok(row.2)
    }
}

impl Engine {
    pub fn load_canonical_authenticated(&self, id: ObjectId) -> EngineResult<Vec<u8>> {
        let connection = self.lock_connection()?;
        with_authenticated_canonical_on_connection(
            self,
            &connection,
            id,
            true,
            true,
            |_, canonical| Ok(canonical.to_vec()),
        )
    }

    pub fn load_canonical_authenticated_bounded(
        &self,
        id: ObjectId,
        maximum: usize,
    ) -> EngineResult<Vec<u8>> {
        let connection = self.lock_connection()?;
        let length = connection
            .query_row(
                "SELECT canonical_length FROM layerfs_objects WHERE object_id = ?1",
                params![id.as_bytes()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or(EngineError::MissingObject(id))?;
        if usize::try_from(length)
            .ok()
            .is_none_or(|length| length > maximum)
        {
            return Err(EngineError::InvalidRecord("object transfer bound"));
        }
        with_authenticated_canonical_on_connection(
            self,
            &connection,
            id,
            true,
            true,
            |_, canonical| Ok(canonical.to_vec()),
        )
    }
}

pub(crate) fn read_directory_object(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
) -> EngineResult<()> {
    with_read_canonical_on_connection(engine, connection, id, false, false, |kind, _| {
        if kind == ObjectKind::Directory {
            Ok(())
        } else {
            Err(EngineError::InvalidRecord("root directory object"))
        }
    })
}

pub(crate) fn visible_root_on_connection(
    engine: &Engine,
    connection: &Connection,
) -> EngineResult<Option<RootId>> {
    engine.mark_statement()?;
    let mut statement = connection
        .prepare_cached("SELECT visible_root FROM layerfs_store_meta WHERE store_id = 1")
        .map_err(map_sqlite_error)?;
    let bytes = statement
        .query_row([], |row| row.get::<_, Option<Vec<u8>>>(0))
        .map_err(map_sqlite_error)?;
    bytes
        .map(|bytes| ObjectId::from_bytes(&bytes).map_err(EngineError::Core))
        .transpose()
}

pub(crate) fn load_root_on_connection(
    engine: &Engine,
    connection: &Connection,
    id: RootId,
) -> EngineResult<RootRecord> {
    engine.mark_statement()?;
    let mut statement = connection
        .prepare_cached(
            "SELECT directory_object, parent_root FROM layerfs_roots WHERE root_id = ?1",
        )
        .map_err(map_sqlite_error)?;
    let row = statement
        .query_row(params![id.as_bytes().as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Option<Vec<u8>>>(1)?))
        })
        .optional()
        .map_err(map_sqlite_error)?
        .ok_or(EngineError::MissingRoot(id))?;
    let directory_object = ObjectId::from_bytes(&row.0).map_err(EngineError::Core)?;
    let parent = row
        .1
        .map(|bytes| ObjectId::from_bytes(&bytes).map_err(EngineError::Core))
        .transpose()?;
    Ok(RootRecord {
        id,
        directory_object,
        parent,
    })
}

pub(crate) fn load_delta_on_connection(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
) -> EngineResult<DeltaRecord> {
    engine.mark_statement()?;
    let mut statement = connection
        .prepare_cached(
            "SELECT format_version, parent_root, child_root, payload
             FROM layerfs_deltas WHERE delta_id = ?1",
        )
        .map_err(map_sqlite_error)?;
    let row = statement
        .query_row(params![id.as_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<Vec<u8>>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })
        .optional()
        .map_err(map_sqlite_error)?
        .ok_or(EngineError::MissingDelta(id))?;
    if row.0 != 0 {
        return Err(EngineError::InvalidRecord("legacy delta format"));
    }
    decode_delta_parts(id, row.1, row.2, row.3)
}

pub(crate) fn decode_delta_parts(
    id: ObjectId,
    parent: Option<Vec<u8>>,
    child: Vec<u8>,
    payload: Vec<u8>,
) -> EngineResult<DeltaRecord> {
    let parent = parent
        .map(|bytes| ObjectId::from_bytes(&bytes).map_err(EngineError::Core))
        .transpose()?;
    let child = ObjectId::from_bytes(&child).map_err(EngineError::Core)?;
    let delta = DeltaRecord {
        id,
        parent,
        child,
        payload,
    };
    delta.validate()?;
    Ok(delta)
}

#[cfg(test)]
pub(crate) fn write_root_on_connection(
    engine: &Engine,
    connection: &Connection,
    root: &RootRecord,
) -> EngineResult<()> {
    engine.mark_statement()?;
    let mut select = connection
        .prepare_cached(
            "SELECT directory_object, parent_root FROM layerfs_roots WHERE root_id = ?1",
        )
        .map_err(map_sqlite_error)?;
    let existing = select
        .query_row(params![root.id.as_bytes().as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Option<Vec<u8>>>(1)?))
        })
        .optional()
        .map_err(map_sqlite_error)?;
    if let Some((directory_object, parent)) = existing {
        let existing = RootRecord {
            id: root.id,
            directory_object: ObjectId::from_bytes(&directory_object).map_err(EngineError::Core)?,
            parent: parent
                .map(|bytes| ObjectId::from_bytes(&bytes).map_err(EngineError::Core))
                .transpose()?,
        };
        if existing != *root {
            return Err(EngineError::ImmutableConflict("root", root.id));
        }
        return Ok(());
    }
    engine.mark_statement()?;
    let mut insert = connection
        .prepare_cached(
            "INSERT INTO layerfs_roots (root_id, directory_object, parent_root)
             VALUES (?1, ?2, ?3)",
        )
        .map_err(map_sqlite_error)?;
    insert
        .execute(params![
            root.id.as_bytes().as_slice(),
            root.directory_object.as_bytes().as_slice(),
            root.parent.map(|id| id.to_bytes().to_vec()),
        ])
        .map_err(map_sqlite_error)?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn root_record_len(root: &RootRecord) -> EngineResult<u64> {
    ROOT_RECORD_BASE_BYTES
        .checked_add(if root.parent.is_some() { 32 } else { 0 })
        .ok_or(EngineError::CounterOverflow)
}

#[cfg(test)]
pub(crate) fn delta_record_len(delta: &DeltaRecord) -> EngineResult<u64> {
    let payload = u64::try_from(delta.payload.len()).map_err(|_| EngineError::CounterOverflow)?;
    let parent = if delta.parent.is_some() { 32 } else { 0 };
    payload
        .checked_add(64)
        .and_then(|value| value.checked_add(parent))
        .ok_or(EngineError::CounterOverflow)
}
