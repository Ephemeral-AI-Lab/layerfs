//! Engine row conversion and structured SQLite error classification.
use crate::backend::error::BackendError;
use crate::backend::seam::{Param, Record, Value};
use rusqlite::{
    types::{Value as SqlValue, ValueRef},
    Error, ErrorCode, Row,
};
pub(crate) fn bind(p: Param) -> SqlValue {
    match p {
        Param::I64(v) => SqlValue::Integer(v),
        Param::Text(v) => SqlValue::Text(v),
        Param::Bytes(v) => SqlValue::Blob(v),
        Param::OptionalBytes(v) => v.map(SqlValue::Blob).unwrap_or(SqlValue::Null),
        Param::OptionalText(v) => v.map(SqlValue::Text).unwrap_or(SqlValue::Null),
    }
}
pub(crate) fn blob<'a>(row: &'a Row<'_>, index: usize) -> Result<&'a [u8], BackendError> {
    row.get_ref(index)
        .map_err(error)?
        .as_blob()
        .map_err(|_| BackendError::Integrity)
}
pub(crate) fn record(row: &Row<'_>, count: usize) -> Result<Record, BackendError> {
    let mut cells = Vec::with_capacity(count);
    for index in 0..count {
        cells.push(match row.get_ref(index).map_err(error)? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(v) => Value::Integer(v),
            ValueRef::Text(v) => Value::Text(
                std::str::from_utf8(v)
                    .map_err(|_| BackendError::Integrity)?
                    .to_owned(),
            ),
            ValueRef::Blob(v) => Value::Bytes(v.to_vec()),
            ValueRef::Real(_) => return Err(BackendError::Integrity),
        });
    }
    Ok(Record { cells })
}
pub(crate) fn error(e: Error) -> BackendError {
    match e {
        Error::SqliteFailure(code, _) => match code.code {
            ErrorCode::DatabaseBusy
            | ErrorCode::DatabaseLocked
            | ErrorCode::FileLockingProtocolFailed => BackendError::Busy,
            ErrorCode::ReadOnly => BackendError::ReadOnly,
            // §7 step 8: `SQLITE_FULL`, `SQLITE_TOOBIG` and `SQLITE_NOMEM` are
            // the report's `Capacity` refusal. NOMEM is a definite failure, not
            // an uncertain outcome: compaction must remove its staged artifact
            // set rather than keep it as a leftover.
            ErrorCode::DiskFull | ErrorCode::TooBig | ErrorCode::OutOfMemory => {
                BackendError::Capacity
            }
            ErrorCode::SystemIoFailure | ErrorCode::CannotOpen => BackendError::Unknown,
            ErrorCode::ConstraintViolation
            | ErrorCode::TypeMismatch
            | ErrorCode::DatabaseCorrupt
            | ErrorCode::NotADatabase => BackendError::Integrity,
            _ => BackendError::Unknown,
        },
        _ => BackendError::Integrity,
    }
}
