//! Canonical Full-store object verification.

use crate::{map_sqlite_error, EngineError, EngineResult};
use layerfs_core::content::rope::ObjectRead;
use layerfs_core::{authenticate_identity, validate_object_from, ObjectId, ObjectKind};
use rusqlite::{params, Connection};
use std::cell::Cell;
use std::io::Cursor;

pub(crate) fn authenticate_object_table(connection: &Connection) -> EngineResult<u64> {
    let mut statement = connection
        .prepare(
            "SELECT object_id, kind, canonical_length, canonical_bytes
             FROM layerfs_objects ORDER BY object_id",
        )
        .map_err(map_sqlite_error)?;
    let mut rows = statement.query([]).map_err(map_sqlite_error)?;
    let mut count = 0_u64;
    while let Some(row) = rows.next().map_err(map_sqlite_error)? {
        let id = row.get::<_, Vec<u8>>(0).map_err(map_sqlite_error)?;
        let object_id = ObjectId::from_bytes(&id).map_err(EngineError::Core)?;
        let kind = row.get::<_, i64>(1).map_err(map_sqlite_error)?;
        let length = row.get::<_, i64>(2).map_err(map_sqlite_error)?;
        let bytes = match row.get_ref(3).map_err(map_sqlite_error)? {
            rusqlite::types::ValueRef::Blob(bytes) => bytes,
            _ => return Err(EngineError::InvalidRecord("object bytes")),
        };
        authenticate_object_row(object_id, kind, length, bytes)?;
        count = count.checked_add(1).ok_or(EngineError::CounterOverflow)?;
    }
    Ok(count)
}

pub(crate) fn authenticate_object_row(
    object_id: ObjectId,
    kind: i64,
    length: i64,
    bytes: &[u8],
) -> EngineResult<ObjectKind> {
    let authenticated =
        authenticate_identity(bytes, object_id).map_err(|cause| EngineError::MalformedObject {
            id: object_id,
            cause,
        })?;
    let decoded = validate_object_from(Cursor::new(bytes)).map_err(EngineError::Core)?;
    let stored_kind = ObjectKind::try_from(
        u8::try_from(kind).map_err(|_| EngineError::InvalidRecord("object kind"))?,
    )?;
    if authenticated != decoded
        || decoded.kind != stored_kind
        || i64::try_from(bytes.len()).ok() != Some(length)
    {
        Err(EngineError::InvalidRecord("authenticated object row"))
    } else {
        Ok(stored_kind)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct VerificationObservation {
    pub(crate) objects: u64,
    pub(crate) bytes: u64,
    pub(crate) statements: u64,
    pub(crate) fetched_rows: u64,
    pub(crate) authentication_passes: u64,
    pub(crate) role_decode_passes: u64,
    pub(crate) scratch_tables: u64,
    pub(crate) scratch_statements: u64,
    pub(crate) scratch_rows: u64,
    pub(crate) scratch_bytes: u64,
    pub(crate) namespace_graphs: u64,
    pub(crate) retained_roots_validated: u64,
}

impl VerificationObservation {
    pub(super) fn merge(&mut self, source: Self) -> EngineResult<()> {
        self.objects = self
            .objects
            .checked_add(source.objects)
            .ok_or(EngineError::CounterOverflow)?;
        self.bytes = self
            .bytes
            .checked_add(source.bytes)
            .ok_or(EngineError::CounterOverflow)?;
        self.statements = self
            .statements
            .checked_add(source.statements)
            .ok_or(EngineError::CounterOverflow)?;
        self.fetched_rows = self
            .fetched_rows
            .checked_add(source.fetched_rows)
            .ok_or(EngineError::CounterOverflow)?;
        self.authentication_passes = self
            .authentication_passes
            .checked_add(source.authentication_passes)
            .ok_or(EngineError::CounterOverflow)?;
        self.role_decode_passes = self
            .role_decode_passes
            .checked_add(source.role_decode_passes)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_tables = self
            .scratch_tables
            .checked_add(source.scratch_tables)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_statements = self
            .scratch_statements
            .checked_add(source.scratch_statements)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_rows = self
            .scratch_rows
            .checked_add(source.scratch_rows)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_bytes = self
            .scratch_bytes
            .checked_add(source.scratch_bytes)
            .ok_or(EngineError::CounterOverflow)?;
        self.namespace_graphs = self
            .namespace_graphs
            .checked_add(source.namespace_graphs)
            .ok_or(EngineError::CounterOverflow)?;
        self.retained_roots_validated = self
            .retained_roots_validated
            .checked_add(source.retained_roots_validated)
            .ok_or(EngineError::CounterOverflow)?;
        Ok(())
    }

    pub(super) fn add_scratch(
        &mut self,
        source: crate::scratch::ScratchObservation,
    ) -> EngineResult<()> {
        self.scratch_tables = self
            .scratch_tables
            .checked_add(source.tables)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_statements = self
            .scratch_statements
            .checked_add(source.statements)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_rows = self
            .scratch_rows
            .checked_add(source.rows)
            .ok_or(EngineError::CounterOverflow)?;
        self.scratch_bytes = self
            .scratch_bytes
            .checked_add(source.high_water_bytes)
            .ok_or(EngineError::CounterOverflow)?;
        Ok(())
    }
}

pub(super) fn merge_failed_observation(
    failed: &Cell<VerificationObservation>,
    observation: VerificationObservation,
) {
    let mut merged = failed.get();
    if merged.merge(observation).is_ok() {
        failed.set(merged);
    }
}

pub(super) struct ConnectionStore<'a> {
    connection: &'a Connection,
    observation: Cell<VerificationObservation>,
    statements: &'a Cell<u64>,
    failed: &'a Cell<VerificationObservation>,
}

impl<'a> ConnectionStore<'a> {
    pub(super) fn new(
        connection: &'a Connection,
        statements: &'a Cell<u64>,
        failed: &'a Cell<VerificationObservation>,
    ) -> Self {
        Self {
            connection,
            observation: Cell::new(VerificationObservation::default()),
            statements,
            failed,
        }
    }

    pub(super) fn observation(&self) -> VerificationObservation {
        self.observation.get()
    }

    pub(super) fn with_record<T>(
        &self,
        id: ObjectId,
        role_decode: bool,
        callback: impl FnOnce(ObjectKind, &[u8]) -> Result<T, layerfs_core::CoreError>,
    ) -> Result<T, layerfs_core::CoreError> {
        self.statements.set(
            self.statements
                .get()
                .checked_add(1)
                .ok_or(layerfs_core::CoreError::LengthOverflow)?,
        );
        let mut statement = self
            .connection
            .prepare_cached(
                "SELECT kind, canonical_length, canonical_bytes FROM layerfs_objects WHERE object_id = ?1",
            )
            .map_err(|_| layerfs_core::CoreError::Io)?;
        let mut rows = statement
            .query(params![id.as_bytes().as_slice()])
            .map_err(|_| layerfs_core::CoreError::Io)?;
        let row = rows
            .next()
            .map_err(|_| layerfs_core::CoreError::Io)?
            .ok_or(layerfs_core::CoreError::MissingObject)?;
        let kind = row
            .get::<_, i64>(0)
            .map_err(|_| layerfs_core::CoreError::Io)?;
        let length = row
            .get::<_, i64>(1)
            .map_err(|_| layerfs_core::CoreError::Io)?;
        let bytes = match row.get_ref(2).map_err(|_| layerfs_core::CoreError::Io)? {
            rusqlite::types::ValueRef::Blob(bytes) => bytes,
            _ => return Err(layerfs_core::CoreError::InvalidRecord("object bytes")),
        };
        let expected_kind = ObjectKind::try_from(
            u8::try_from(kind)
                .map_err(|_| layerfs_core::CoreError::InvalidRecord("object kind"))?,
        )?;
        let expected_length = u64::try_from(length)
            .map_err(|_| layerfs_core::CoreError::InvalidRecord("object length"))?;
        let object = authenticate_identity(bytes, id)?;
        if object.kind != expected_kind || bytes.len() as u64 != expected_length {
            return Err(layerfs_core::CoreError::LengthMismatch {
                expected: expected_length,
                actual: bytes.len() as u64,
            });
        }
        if !role_decode && validate_object_from(Cursor::new(bytes))? != object {
            return Err(layerfs_core::CoreError::InvalidRecord("object summary"));
        }
        let mut observation = self.observation.get();
        observation.fetched_rows = observation
            .fetched_rows
            .checked_add(1)
            .ok_or(layerfs_core::CoreError::LengthOverflow)?;
        observation.authentication_passes = observation
            .authentication_passes
            .checked_add(1)
            .ok_or(layerfs_core::CoreError::LengthOverflow)?;
        observation.objects = observation
            .objects
            .checked_add(1)
            .ok_or(layerfs_core::CoreError::LengthOverflow)?;
        observation.bytes = observation
            .bytes
            .checked_add(expected_length)
            .ok_or(layerfs_core::CoreError::LengthOverflow)?;
        self.observation.set(observation);
        let value = callback(expected_kind, bytes)?;
        if role_decode {
            self.note_role_decode()?;
        }
        Ok(value)
    }

    pub(super) fn fetch(&self, id: ObjectId) -> Result<Vec<u8>, layerfs_core::CoreError> {
        self.with_record(id, false, |_, bytes| Ok(bytes.to_vec()))
    }

    fn note_role_decode(&self) -> Result<(), layerfs_core::CoreError> {
        let mut observation = self.observation.get();
        observation.role_decode_passes = observation
            .role_decode_passes
            .checked_add(1)
            .ok_or(layerfs_core::CoreError::LengthOverflow)?;
        self.observation.set(observation);
        Ok(())
    }
}

impl Drop for ConnectionStore<'_> {
    fn drop(&mut self) {
        merge_failed_observation(self.failed, self.observation.get());
    }
}

impl ObjectRead for ConnectionStore<'_> {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, layerfs_core::CoreError> {
        self.fetch(id)
    }

    fn with_authenticated_canonical<T, F>(
        &self,
        id: ObjectId,
        callback: F,
    ) -> Result<T, layerfs_core::CoreError>
    where
        F: FnOnce(&[u8]) -> Result<T, layerfs_core::CoreError>,
    {
        self.with_record(id, true, |_, bytes| callback(bytes))
    }
}
