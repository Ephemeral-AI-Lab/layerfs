//! Exact named-index manifest validation.

use crate::{schema_shape, EngineError, EngineResult};
use rusqlite::Connection;

pub(crate) fn validate_index_schemas(
    connection: &Connection,
    expected: &[(&str, &str)],
) -> EngineResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT name, sql FROM sqlite_schema
             WHERE type = 'index' AND sql IS NOT NULL ORDER BY name",
        )
        .map_err(|_| EngineError::SchemaMismatch)?;
    let mut rows = statement
        .query([])
        .map_err(|_| EngineError::SchemaMismatch)?;
    for (expected_name, expected_sql) in expected {
        let row = rows
            .next()
            .map_err(|_| EngineError::SchemaMismatch)?
            .ok_or(EngineError::SchemaMismatch)?;
        let name = row
            .get::<_, String>(0)
            .map_err(|_| EngineError::SchemaMismatch)?;
        let sql = row
            .get::<_, String>(1)
            .map_err(|_| EngineError::SchemaMismatch)?;
        if name != *expected_name || schema_shape(&sql) != schema_shape(expected_sql) {
            return Err(EngineError::SchemaMismatch);
        }
    }
    if rows
        .next()
        .map_err(|_| EngineError::SchemaMismatch)?
        .is_some()
    {
        return Err(EngineError::SchemaMismatch);
    }
    Ok(())
}
