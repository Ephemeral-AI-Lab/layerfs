//! Canonical object authentication and read accounting.

use crate::integrity::IntegrityMode;
use crate::{checked_add, map_sqlite_error, observe_time, Engine, EngineError, EngineResult};
use layerfs_core::{
    authenticate_identity, validate_object_from, CoreError, ObjectId, ObjectKind, ObjectSummary,
};
use rusqlite::types::ValueRef;
use rusqlite::{params, Connection};
use std::io::Cursor;
use std::time::Instant;

pub(crate) fn with_authenticated_canonical_on_connection<T>(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
    fetched_row: bool,
    role_decode: bool,
    callback: impl FnOnce(ObjectKind, &[u8]) -> EngineResult<T>,
) -> EngineResult<T> {
    with_canonical_on_connection(
        engine,
        connection,
        id,
        fetched_row,
        role_decode,
        true,
        callback,
    )
}

pub(crate) fn with_read_canonical_on_connection<T>(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
    fetched_row: bool,
    role_decode: bool,
    callback: impl FnOnce(ObjectKind, &[u8]) -> EngineResult<T>,
) -> EngineResult<T> {
    with_canonical_on_connection(
        engine,
        connection,
        id,
        fetched_row,
        role_decode,
        engine.mode == IntegrityMode::Verified,
        callback,
    )
}

pub(crate) fn with_canonical_on_connection<T>(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
    fetched_row: bool,
    role_decode: bool,
    authenticate: bool,
    callback: impl FnOnce(ObjectKind, &[u8]) -> EngineResult<T>,
) -> EngineResult<T> {
    if fetched_row != role_decode {
        return Err(EngineError::InvalidRecord("fetched role accounting"));
    }
    let query_started = Instant::now();
    engine.mark_statement()?;
    let mut statement = connection
        .prepare_cached("SELECT kind, canonical_length, canonical_bytes FROM layerfs_objects WHERE object_id = ?1")
        .map_err(map_sqlite_error)?;
    let mut rows = statement
        .query(params![id.as_bytes().as_slice()])
        .map_err(map_sqlite_error)?;
    let row = rows
        .next()
        .map_err(map_sqlite_error)?
        .ok_or(EngineError::MissingObject(id))?;
    let kind = row.get::<_, i64>(0).map_err(map_sqlite_error)?;
    let length = row.get::<_, i64>(1).map_err(map_sqlite_error)?;
    let bytes = match row.get_ref(2).map_err(map_sqlite_error)? {
        ValueRef::Blob(bytes) => bytes,
        _ => return Err(EngineError::InvalidRecord("object bytes")),
    };
    observe_time(&engine.timings.nonpayload_query_ns, query_started);
    let mut authentication_passed = false;
    let mut role_decode_passed = false;
    let result = (|| {
        let kind = if authenticate {
            let kind = authenticate_borrowed(engine, id, kind, length, bytes)?;
            authentication_passed = true;
            kind
        } else {
            validate_borrowed(engine, id, kind, length, bytes)?
        };
        if !role_decode {
            let role_started = Instant::now();
            let decoded = validate_object_from(Cursor::new(bytes));
            observe_time(&engine.timings.role_decode_ns, role_started);
            let summary = decoded?;
            if summary.kind != kind || summary.canonical_len != bytes.len() as u64 {
                return Err(EngineError::InvalidRecord("object summary"));
            }
        }
        let role_started = Instant::now();
        let value = callback(kind, bytes);
        if role_decode {
            observe_time(&engine.timings.role_decode_ns, role_started);
        }
        let value = value?;
        role_decode_passed = role_decode;
        Ok(value)
    })();
    if fetched_row {
        let counter_started = Instant::now();
        engine.bump(|counters| {
            checked_add(&mut counters.fetched_rows, 1)?;
            if authentication_passed {
                checked_add(&mut counters.fetched_row_authentication_passes, 1)?;
            }
            if role_decode_passed {
                checked_add(&mut counters.fetched_row_role_decode_passes, 1)?;
            }
            Ok(())
        })?;
        observe_time(&engine.timings.counter_merge_ns, counter_started);
    }
    result
}

pub(crate) fn validate_borrowed(
    engine: &Engine,
    id: ObjectId,
    kind: i64,
    length: i64,
    bytes: &[u8],
) -> EngineResult<ObjectKind> {
    let role_started = Instant::now();
    let expected_kind = ObjectKind::try_from(
        u8::try_from(kind).map_err(|_| EngineError::InvalidRecord("object kind"))?,
    )?;
    let expected_length =
        u64::try_from(length).map_err(|_| EngineError::InvalidRecord("object length"))?;
    let summary = validate_object_from(Cursor::new(bytes))
        .map_err(|cause| EngineError::MalformedObject { id, cause })?;
    let actual_length = u64::try_from(bytes.len()).map_err(|_| EngineError::CounterOverflow)?;
    observe_time(&engine.timings.role_decode_ns, role_started);
    if summary.kind != expected_kind
        || summary.canonical_len != actual_length
        || actual_length != expected_length
    {
        return Err(EngineError::InvalidRecord("object summary"));
    }
    let counter_started = Instant::now();
    engine.bump(|counters| {
        checked_add(&mut counters.objects_validated, 1)?;
        checked_add(&mut counters.object_bytes_read, actual_length)
    })?;
    observe_time(&engine.timings.counter_merge_ns, counter_started);
    Ok(summary.kind)
}

pub(crate) fn authenticate_borrowed(
    engine: &Engine,
    id: ObjectId,
    kind: i64,
    length: i64,
    bytes: &[u8],
) -> EngineResult<ObjectKind> {
    let authentication_started = Instant::now();
    let authenticated = authenticate_borrowed_unaccounted(id, kind, length, bytes);
    observe_time(
        &engine.timings.identity_authentication_ns,
        authentication_started,
    );
    let (summary, actual_length) = authenticated?;
    let counter_started = Instant::now();
    engine.bump(|counters| {
        checked_add(&mut counters.objects_validated, 1)?;
        checked_add(&mut counters.object_bytes_read, actual_length)?;
        Ok(())
    })?;
    observe_time(&engine.timings.counter_merge_ns, counter_started);
    Ok(summary.kind)
}

pub(crate) fn payload_batch_sql(count: usize) -> EngineResult<String> {
    if !(1..=64).contains(&count) {
        return Err(EngineError::InvalidRecord("payload batch size"));
    }
    Ok((0..count)
        .map(|index| {
            format!(
                "SELECT {index} AS ord, kind, canonical_length, canonical_bytes FROM layerfs_objects WHERE object_id = ?{}",
                index + 1
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ")
        + " ORDER BY 1")
}

pub(crate) fn authenticate_borrowed_unaccounted(
    id: ObjectId,
    kind: i64,
    length: i64,
    bytes: &[u8],
) -> EngineResult<(ObjectSummary, u64)> {
    let expected_kind = ObjectKind::try_from(
        u8::try_from(kind).map_err(|_| EngineError::InvalidRecord("object kind"))?,
    )?;
    let expected_length =
        u64::try_from(length).map_err(|_| EngineError::InvalidRecord("object length"))?;
    let object = authenticate_identity(bytes, id)
        .map_err(|cause| EngineError::MalformedObject { id, cause })?;
    let actual_length = u64::try_from(bytes.len()).map_err(|_| EngineError::CounterOverflow)?;
    if object.kind != expected_kind || actual_length != expected_length {
        return Err(EngineError::MalformedObject {
            id,
            cause: CoreError::LengthMismatch {
                expected: expected_length,
                actual: actual_length,
            },
        });
    }
    Ok((object, actual_length))
}
