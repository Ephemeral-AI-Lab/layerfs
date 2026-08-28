use super::index::validate_index_schemas;
use super::table::validate_contract_tables;
use crate::{
    note_statement, schema, EngineError, EngineResult, StoreRole, FORMAT_MARKER, FULL_SCHEMA,
    SCHEMA_VERSION,
};
use rusqlite::{Connection, OptionalExtension};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SchemaState {
    Empty,
    Legacy,
    Current,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FullAdmission {
    pub role: StoreRole,
    pub storage_id: [u8; 32],
    pub durable_storage_id: [u8; 32],
}

pub(crate) fn admit_full_family_role(
    connection: &Connection,
    expected_role: StoreRole,
) -> EngineResult<FullAdmission> {
    if !matches!(expected_role, StoreRole::Durable | StoreRole::DurableCache) {
        return Err(EngineError::SchemaMismatch);
    }
    validate_contract_tables(connection, FULL_SCHEMA)?;
    validate_index_schemas(connection, FULL_SCHEMA.index_schemas)?;
    admit_full_role_metadata(connection, expected_role)
}

pub(crate) fn admit_full_role_metadata(
    connection: &Connection,
    expected_role: StoreRole,
) -> EngineResult<FullAdmission> {
    let metadata = connection
        .query_row(
            "SELECT format_marker, schema_version, store_role, storage_id,
                    durable_storage_id, journal_mode, synchronous, temp_store, mmap_size
             FROM layerfs_store_meta WHERE store_id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )
        .optional()
        .map_err(|_| EngineError::SchemaMismatch)?
        .ok_or(EngineError::SchemaMismatch)?;
    let stored_role = match metadata.2.as_str() {
        "durable" => StoreRole::Durable,
        "durable_cache" => StoreRole::DurableCache,
        _ => return Err(EngineError::SchemaMismatch),
    };
    let storage_id: [u8; 32] = metadata
        .3
        .try_into()
        .map_err(|_| EngineError::SchemaMismatch)?;
    let durable_storage_id: [u8; 32] = metadata
        .4
        .try_into()
        .map_err(|_| EngineError::SchemaMismatch)?;
    if metadata.0 != FULL_SCHEMA.format_marker
        || metadata.1 != FULL_SCHEMA.schema_version
        || !metadata.5.eq_ignore_ascii_case("DELETE")
        || metadata.6 != 2
        || metadata.7 != 1
        || metadata.8 != 0
        || stored_role != expected_role
        || stored_role == StoreRole::Durable && storage_id != durable_storage_id
        || stored_role == StoreRole::DurableCache && storage_id == durable_storage_id
    {
        return Err(EngineError::SchemaMismatch);
    }
    let violation = connection
        .query_row("PRAGMA foreign_key_check", [], |_| Ok(()))
        .optional()
        .map_err(|_| EngineError::SchemaMismatch)?;
    if violation.is_some() {
        return Err(EngineError::SchemaMismatch);
    }
    Ok(FullAdmission {
        role: stored_role,
        storage_id,
        durable_storage_id,
    })
}

pub(super) fn validate_schema_metadata(
    connection: &Connection,
    statements: &mut u64,
    state: SchemaState,
) -> EngineResult<()> {
    note_statement(statements)?;
    let metadata = connection
        .query_row(
            "SELECT format_marker, schema_version, journal_mode, synchronous, temp_store, mmap_size
             FROM layerfs_store_meta WHERE store_id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|_| EngineError::SchemaMismatch)?;
    let expected_version = match state {
        SchemaState::Legacy => schema::LEGACY_SCHEMA_VERSION,
        SchemaState::Current => SCHEMA_VERSION,
        SchemaState::Empty => return Err(EngineError::SchemaMismatch),
    };
    if !matches!(metadata, Some((ref marker, version, ref journal, 2, 1, 0))
        if version == expected_version
            && marker == FORMAT_MARKER
            && journal.eq_ignore_ascii_case("DELETE"))
    {
        return Err(EngineError::SchemaMismatch);
    }
    Ok(())
}

pub(super) fn validate_authority(
    connection: &Connection,
    statements: &mut u64,
) -> EngineResult<()> {
    note_statement(statements)?;
    let authority = connection
        .query_row(
            "SELECT length(store_id), next_inode_serial, trusted_history
             FROM layerfs_authority WHERE authority_id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|_| EngineError::SchemaMismatch)?;
    if !matches!(authority, Some((32, serial, trusted)) if serial >= 0 && matches!(trusted, 0 | 1))
    {
        return Err(EngineError::SchemaMismatch);
    }
    Ok(())
}
