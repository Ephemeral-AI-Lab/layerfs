//! Frozen schema-family contracts plus the unchanged legacy runtime manifest.

mod full;
mod identity;
pub(crate) mod legacy_full;
mod working;

pub use identity::{
    SchemaContract, SchemaIdentity, StoreRole, FULL_FORMAT_MARKER, FULL_SCHEMA_VERSION,
    LEGACY_FULL_FORMAT_MARKER, LEGACY_FULL_SCHEMA_VERSION, WORKING_FORMAT_MARKER,
    WORKING_SCHEMA_VERSION,
};

pub const LEGACY_FULL_SCHEMA: SchemaContract = SchemaContract {
    identity: SchemaIdentity::LegacyFull,
    format_marker: LEGACY_FULL_FORMAT_MARKER,
    schema_version: LEGACY_FULL_SCHEMA_VERSION,
    table_names: &legacy_full::CURRENT_TABLE_NAMES,
    table_partitions: &legacy_full::CONTRACT_SCHEMAS,
    index_schemas: &[],
};

pub const FULL_SCHEMA: SchemaContract = full::CONTRACT;
pub const WORKING_SCHEMA: SchemaContract = working::CONTRACT;

pub use legacy_full::TRANSITION_FORMAT_VERSION;
pub(crate) use legacy_full::{
    admitted_store_id_counted, initialize_schema_counted, note_statement, BASE_SCHEMAS,
    CURRENT_TABLE_NAMES, FORMAT_MARKER, LEGACY_DELTA_SCHEMA, LEGACY_SCHEMA_VERSION,
    LEGACY_TABLE_NAMES, PRODUCT_SCHEMAS, SCHEMA_VERSION,
};
