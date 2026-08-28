//! SQLite schema-shape and role admission.

pub(crate) mod index;
mod role;
mod table;

pub(crate) use role::{admit_full_family_role, admit_full_role_metadata, SchemaState};
pub(crate) use table::{
    admit_legacy_full_migration_source, admit_schema_counted, preflight_schema,
    preflight_schema_counted, schema_shape,
};
