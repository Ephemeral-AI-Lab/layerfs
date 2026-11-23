//! Row conversion and structured PostgreSQL error classification.
use crate::backend::records::{BackendError, Record, Value};
use postgres::{types::Type, Row};

/// Classifies one failed call. A server error carries its SQLSTATE and is a
/// definite outcome; a closed connection or failed I/O may have lost a
/// reply and is `Unknown`, never guessed either way.
pub(crate) fn error(e: &postgres::Error) -> BackendError {
    if let Some(code) = e.code() {
        return sqlstate(code.code());
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

/// Classifies one SQLSTATE. Only the codes that definitively mean the Store
/// ran out of a resource are `Capacity`; a class prefix is not enough, because
/// class 53 also carries the transient connection shortage and class 54 also
/// carries statement-complexity refusals. A code without a rule is `Unknown`,
/// never a guessed verdict.
pub fn sqlstate(code: &str) -> BackendError {
    match code {
        // lock_not_available (lock_timeout), deadlock, serialization failure.
        "55P03" | "40P01" | "40001" => BackendError::Busy,
        // too_many_connections: transient, and this caller already holds a
        // session, so it is retryable rather than a capacity verdict.
        "53300" => BackendError::Busy,
        "25006" => BackendError::ReadOnly,
        "57014" => BackendError::Cancelled,
        // Insufficient resources, definite: disk full, out of memory, a
        // configuration limit, and the class-generic program limit.
        "53100" | "53200" | "53400" | "54000" => BackendError::Capacity,
        // 54001/54011/54023 (statement too complex, too many columns or
        // arguments) are statement refusals, not a capacity refusal. They keep
        // the class-54 verdict of a definite failure, so the caller never
        // treats them as retryable or as an unknown outcome.
        _ if code.starts_with("54") => BackendError::Integrity,
        _ if code.starts_with("23") || code.starts_with("22") || code.starts_with("42") => {
            BackendError::Integrity
        }
        // A statement the server refused with a reply: cardinality violation
        // (class 21) and PL/pgSQL errors (class P0), among them `P0001` from
        // the `pack_immutable` trigger's RAISE EXCEPTION. The statement, and
        // the transaction it ran in, definitely failed; nothing was committed.
        _ if code.starts_with("21") || code.starts_with("P0") => BackendError::Integrity,
        // Connection, shutdown, resource-class and internal failures: the
        // outcome is unknown.
        _ => BackendError::Unknown,
    }
}

/// One result row as owned engine-neutral values.
pub(crate) fn record(row: &Row) -> Result<Record, BackendError> {
    record_first(row, row.len())
}

/// The first `count` columns of one row, so a large trailing column (a pack
/// body) is not copied with them.
pub(crate) fn record_first(row: &Row, count: usize) -> Result<Record, BackendError> {
    let mut cells = Vec::with_capacity(count.min(row.len()));
    for (index, column) in row.columns().iter().enumerate().take(count) {
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
