//! Row conversion and structured PostgreSQL error classification.
use crate::backend::records::{BackendError, Record, Value};
use postgres::{types::Type, Row};

/// Classifies one failed call. A server error carries its SQLSTATE and is a
/// definite outcome; a closed connection or failed I/O may have lost a
/// reply and is `Unknown`, never guessed either way.
pub(crate) fn error(e: &postgres::Error) -> BackendError {
    if let Some(code) = e.code() {
        return state(code.code());
    }
    if e.is_closed() {
        return BackendError::Unknown;
    }
    match std::error::Error::source(e) {
        // A value that failed to convert never reached the server.
        Some(source) if !source.is::<std::io::Error>() => BackendError::Integrity,
        _ => BackendError::Unknown,
    }
}

fn state(code: &str) -> BackendError {
    match code {
        // lock_not_available (lock_timeout), deadlock, serialization failure.
        "55P03" | "40P01" | "40001" => BackendError::Busy,
        "25006" => BackendError::ReadOnly,
        "57014" => BackendError::Cancelled,
        _ if code.starts_with("53") || code.starts_with("54") => BackendError::Capacity,
        _ if code.starts_with("23") || code.starts_with("22") || code.starts_with("42") => {
            BackendError::Integrity
        }
        // Connection, shutdown and internal failures: the outcome is unknown.
        _ => BackendError::Unknown,
    }
}

/// One result row as owned engine-neutral values.
pub(crate) fn record(row: &Row) -> Result<Record, BackendError> {
    let mut cells = Vec::with_capacity(row.len());
    for (index, column) in row.columns().iter().enumerate() {
        let failed = |e: postgres::Error| error(&e);
        let ty = column.type_();
        let value = if *ty == Type::INT8 {
            row.try_get::<_, Option<i64>>(index)
                .map_err(failed)?
                .map(Value::Integer)
        } else if *ty == Type::INT4 {
            row.try_get::<_, Option<i32>>(index)
                .map_err(failed)?
                .map(|v| Value::Integer(v.into()))
        } else if *ty == Type::INT2 {
            row.try_get::<_, Option<i16>>(index)
                .map_err(failed)?
                .map(|v| Value::Integer(v.into()))
        } else if *ty == Type::BOOL {
            row.try_get::<_, Option<bool>>(index)
                .map_err(failed)?
                .map(|v| Value::Integer(v.into()))
        } else if *ty == Type::BYTEA {
            row.try_get::<_, Option<Vec<u8>>>(index)
                .map_err(failed)?
                .map(Value::Bytes)
        } else if [Type::TEXT, Type::VARCHAR, Type::NAME].contains(ty) {
            row.try_get::<_, Option<String>>(index)
                .map_err(failed)?
                .map(Value::Text)
        } else {
            return Err(BackendError::Integrity);
        };
        cells.push(value.unwrap_or(Value::Null));
    }
    Ok(Record { cells })
}
