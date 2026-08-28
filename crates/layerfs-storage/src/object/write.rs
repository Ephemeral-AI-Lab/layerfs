//! Immutable object, root, and delta write records.

use crate::{
    checked_add, map_sqlite_error, observe_time, with_authenticated_canonical_on_connection,
    Engine, EngineError, EngineResult,
};
use layerfs_core::{decode_object, validate_identity, ObjectId, ObjectKind};
use rusqlite::{params, Connection, OptionalExtension};
use std::time::Instant;

pub type RootId = ObjectId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PutOutcome {
    Created,
    Reused,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectRecord {
    pub id: ObjectId,
    pub kind: ObjectKind,
    pub canonical_len: u64,
    pub canonical_bytes: Vec<u8>,
}

impl ObjectRecord {
    pub fn new(id: ObjectId, canonical_bytes: Vec<u8>) -> EngineResult<Self> {
        let object = validate_identity(&canonical_bytes, id)
            .map_err(|cause| EngineError::MalformedObject { id, cause })?;
        let canonical_len =
            u64::try_from(canonical_bytes.len()).map_err(|_| EngineError::CounterOverflow)?;
        Ok(Self {
            id,
            kind: object.kind(),
            canonical_len,
            canonical_bytes,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootRecord {
    pub id: ObjectId,
    pub directory_object: ObjectId,
    pub parent: Option<ObjectId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeltaRecord {
    pub id: ObjectId,
    pub parent: Option<ObjectId>,
    pub child: ObjectId,
    pub payload: Vec<u8>,
}

impl DeltaRecord {
    pub fn new(parent: Option<ObjectId>, child: ObjectId, payload: Vec<u8>) -> Self {
        Self {
            id: delta_identity(parent, child, &payload),
            parent,
            child,
            payload,
        }
    }

    pub(crate) fn validate(&self) -> EngineResult<()> {
        let actual = delta_identity(self.parent, self.child, &self.payload);
        if actual != self.id {
            return Err(EngineError::IdentityMismatch {
                expected: self.id,
                actual,
            });
        }
        Ok(())
    }
}

fn delta_identity(parent: Option<ObjectId>, child: ObjectId, payload: &[u8]) -> ObjectId {
    let mut identity = Vec::with_capacity(1 + 32 + 32 + payload.len());
    identity.extend_from_slice(b"layerfs-phase4a-delta-v1");
    match parent {
        Some(parent) => {
            identity.push(1);
            identity.extend_from_slice(parent.as_bytes());
        }
        None => identity.push(0),
    }
    identity.extend_from_slice(child.as_bytes());
    identity.extend_from_slice(payload);
    ObjectId::for_bytes(&identity)
}

#[cfg(test)]
pub(crate) fn put_object_on_connection(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
    canonical_bytes: &[u8],
) -> EngineResult<PutOutcome> {
    let object = validate_identity(canonical_bytes, id)
        .map_err(|cause| EngineError::MalformedObject { id, cause })?;
    put_validated_object_on_connection(
        engine,
        connection,
        id,
        object.kind(),
        canonical_bytes,
        false,
    )
}

pub(crate) fn put_canonical_object_on_connection(
    engine: &Engine,
    connection: &Connection,
    canonical_bytes: &[u8],
) -> EngineResult<(ObjectId, PutOutcome)> {
    let id = ObjectId::for_bytes(canonical_bytes);
    let object = decode_object(canonical_bytes)
        .map_err(|cause| EngineError::MalformedObject { id, cause })?;
    let outcome = put_validated_object_on_connection(
        engine,
        connection,
        id,
        object.kind(),
        canonical_bytes,
        true,
    )?;
    Ok((id, outcome))
}

fn put_validated_object_on_connection(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
    kind: ObjectKind,
    canonical_bytes: &[u8],
    candidate_id_derived: bool,
) -> EngineResult<PutOutcome> {
    let canonical_len =
        u64::try_from(canonical_bytes.len()).map_err(|_| EngineError::CounterOverflow)?;
    engine.bump(|counters| {
        checked_add(&mut counters.objects_validated, 1)?;
        checked_add(&mut counters.new_object_authentication_passes, 1)?;
        checked_add(&mut counters.put_lookup_statements, 1)
    })?;
    let incumbent = if candidate_id_derived && canonical_bytes.len() <= 1_048_576 {
        match exact_candidate_on_connection(engine, connection, id, kind, canonical_bytes)? {
            Some(true) => {
                engine.bump(|counters| {
                    checked_add(&mut counters.objects_validated, 1)?;
                    checked_add(&mut counters.object_bytes_read, canonical_len)
                })?;
                Ok(())
            }
            Some(false) => authenticate_incumbent(engine, connection, id, kind, canonical_bytes),
            None => Err(EngineError::MissingObject(id)),
        }
    } else {
        authenticate_incumbent(engine, connection, id, kind, canonical_bytes)
    };
    match incumbent {
        Ok(()) => {
            engine.bump(|counters| {
                checked_add(&mut counters.objects_reused, 1)?;
                checked_add(&mut counters.reused_rows, 1)?;
                checked_add(&mut counters.incumbent_authentication_passes, 1)
            })?;
            return Ok(PutOutcome::Reused);
        }
        Err(EngineError::MissingObject(missing)) if missing == id => {}
        Err(error) => return Err(error),
    }

    engine.mark_statement()?;
    engine.bump(|counters| checked_add(&mut counters.put_insert_statements, 1))?;
    let mut insert = connection
        .prepare_cached(
            "INSERT INTO layerfs_objects (object_id, kind, canonical_length, canonical_bytes)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .map_err(map_sqlite_error)?;
    insert
        .execute(params![
            id.as_bytes().as_slice(),
            i64::from(kind as u8),
            i64::try_from(canonical_len).map_err(|_| EngineError::CounterOverflow)?,
            canonical_bytes,
        ])
        .map_err(map_sqlite_error)?;
    engine.bump(|counters| {
        checked_add(&mut counters.objects_created, 1)?;
        checked_add(&mut counters.created_rows, 1)?;
        checked_add(&mut counters.object_bytes_written, canonical_len)?;
        checked_add(&mut counters.logical_object_bytes, canonical_len)
    })?;
    Ok(PutOutcome::Created)
}

fn exact_candidate_on_connection(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
    kind: ObjectKind,
    canonical_bytes: &[u8],
) -> EngineResult<Option<bool>> {
    let query_started = Instant::now();
    engine.mark_statement()?;
    let result = connection
        .query_row(
            "SELECT kind = ?2 AND canonical_length = ?3 AND canonical_bytes = ?4
             FROM layerfs_objects WHERE object_id = ?1",
            params![
                id.as_bytes().as_slice(),
                i64::from(kind as u8),
                i64::try_from(canonical_bytes.len()).map_err(|_| EngineError::CounterOverflow)?,
                canonical_bytes,
            ],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .map_err(map_sqlite_error);
    observe_time(&engine.timings.nonpayload_query_ns, query_started);
    result
}

fn authenticate_incumbent(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
    kind: ObjectKind,
    canonical_bytes: &[u8],
) -> EngineResult<()> {
    with_authenticated_canonical_on_connection(
        engine,
        connection,
        id,
        false,
        false,
        |stored_kind, stored| {
            if stored_kind != kind || stored != canonical_bytes {
                return Err(EngineError::ImmutableConflict("object", id));
            }
            Ok(())
        },
    )
}

#[cfg(test)]
pub(crate) fn authenticate_directory_object(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
) -> EngineResult<()> {
    with_authenticated_canonical_on_connection(engine, connection, id, false, false, |kind, _| {
        if kind == ObjectKind::Directory {
            Ok(())
        } else {
            Err(EngineError::InvalidRecord("root directory object"))
        }
    })
}
