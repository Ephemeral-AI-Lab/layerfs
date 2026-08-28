use super::role::{validate_authority, validate_schema_metadata, SchemaState};
use crate::{map_sqlite_error, note_statement, schema, EngineError, EngineResult, SchemaContract};
use rusqlite::{params, Connection};

pub(crate) fn preflight_schema(connection: &Connection) -> EngineResult<()> {
    let mut ignored = 0;
    preflight_schema_counted(connection, &mut ignored)
}

pub(crate) fn admit_legacy_full_migration_source(
    connection: &Connection,
) -> EngineResult<[u8; 32]> {
    let mut statements = 0;
    if admit_schema_counted(connection, &mut statements)? != SchemaState::Current {
        return Err(EngineError::SchemaMismatch);
    }
    schema::admitted_store_id_counted(connection, &mut statements)
}

pub(crate) fn preflight_schema_counted(
    connection: &Connection,
    statements: &mut u64,
) -> EngineResult<()> {
    if admit_schema_counted(connection, statements)? == SchemaState::Current {
        Ok(())
    } else {
        Err(EngineError::SchemaMismatch)
    }
}

pub(crate) fn admit_schema_counted(
    connection: &Connection,
    statements: &mut u64,
) -> EngineResult<SchemaState> {
    note_statement(statements)?;
    let count = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(map_sqlite_error)?;
    if count == 0 {
        return Ok(SchemaState::Empty);
    }

    let state = match usize::try_from(count).ok() {
        Some(count) if count == schema::LEGACY_TABLE_NAMES.len() => SchemaState::Legacy,
        Some(count) if count == schema::CURRENT_TABLE_NAMES.len() => SchemaState::Current,
        _ => return Err(EngineError::SchemaMismatch),
    };
    let names = match state {
        SchemaState::Legacy => schema::LEGACY_TABLE_NAMES.as_slice(),
        SchemaState::Current => schema::CURRENT_TABLE_NAMES.as_slice(),
        SchemaState::Empty => unreachable!(),
    };
    note_statement(statements)?;
    let mut statement = connection
        .prepare(
            "SELECT type, name FROM sqlite_schema
             WHERE name NOT GLOB 'sqlite_*'
             ORDER BY type, name",
        )
        .map_err(map_sqlite_error)?;
    let mut rows = statement.query([]).map_err(map_sqlite_error)?;
    for expected_name in names {
        let row = rows
            .next()
            .map_err(map_sqlite_error)?
            .ok_or(EngineError::SchemaMismatch)?;
        if row.get::<_, String>(0).map_err(map_sqlite_error)? != "table"
            || row.get::<_, String>(1).map_err(map_sqlite_error)? != *expected_name
        {
            return Err(EngineError::SchemaMismatch);
        }
    }
    if rows.next().map_err(map_sqlite_error)?.is_some() {
        return Err(EngineError::SchemaMismatch);
    }
    for (name, expected) in schema::BASE_SCHEMAS {
        let expected = if state == SchemaState::Legacy && name == "layerfs_deltas" {
            schema::LEGACY_DELTA_SCHEMA
        } else {
            expected
        };
        validate_table_shape(connection, statements, name, expected)?;
    }
    if state == SchemaState::Current {
        for &(name, expected) in schema::PRODUCT_SCHEMAS.into_iter().flatten() {
            validate_table_shape(connection, statements, name, expected)?;
        }
    }
    validate_schema_metadata(connection, statements, state)?;
    validate_authority(connection, statements)?;
    Ok(state)
}

pub(super) fn validate_contract_tables(
    connection: &Connection,
    contract: SchemaContract,
) -> EngineResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT name, sql FROM sqlite_schema
             WHERE type = 'table' AND name NOT GLOB 'sqlite_*' ORDER BY name",
        )
        .map_err(|_| EngineError::SchemaMismatch)?;
    let mut rows = statement
        .query([])
        .map_err(|_| EngineError::SchemaMismatch)?;
    for expected_name in contract.table_names {
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
        let expected_sql = contract
            .table_partitions
            .iter()
            .flat_map(|partition| partition.iter())
            .find_map(|(candidate, sql)| (*candidate == *expected_name).then_some(*sql))
            .ok_or(EngineError::SchemaMismatch)?;
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
    let extra = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema
             WHERE name NOT GLOB 'sqlite_*' AND type NOT IN ('table', 'index'))",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| EngineError::SchemaMismatch)?;
    if extra {
        Err(EngineError::SchemaMismatch)
    } else {
        Ok(())
    }
}

fn validate_table_shape(
    connection: &Connection,
    statements: &mut u64,
    name: &str,
    expected: &str,
) -> EngineResult<()> {
    note_statement(statements)?;
    let actual = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            params![name],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| EngineError::SchemaMismatch)?;
    if schema_shape(&actual) != schema_shape(expected) {
        return Err(EngineError::SchemaMismatch);
    }
    Ok(())
}

pub(crate) fn schema_shape(sql: &str) -> String {
    sql.chars()
        .filter(|character| !character.is_ascii_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .replace("ifnotexists", "")
}
