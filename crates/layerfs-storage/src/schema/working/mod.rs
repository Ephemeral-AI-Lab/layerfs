//! Exact Working-family manifest assembly.

mod binding;
mod branch;
mod index;
mod kernel;
mod layer_candidate;
mod operation;
mod sync;

use super::{SchemaContract, SchemaIdentity, WORKING_FORMAT_MARKER, WORKING_SCHEMA_VERSION};

pub(super) const TABLE_NAMES: [&str; 14] = [
    "layerfs_branch_deltas",
    "layerfs_branch_transitions",
    "layerfs_branches",
    "layerfs_deltas",
    "layerfs_objects",
    "layerfs_operation_versions",
    "layerfs_operations",
    "layerfs_push_outbox",
    "layerfs_retained_roots",
    "layerfs_store_meta",
    "layerfs_transfer_state",
    "layerfs_version_leases",
    "layerfs_working_base_bindings",
    "layerfs_working_layer_candidates",
];

const TABLE_PARTITIONS: [&[(&str, &str)]; 6] = [
    &kernel::SCHEMAS,
    &binding::SCHEMAS,
    &branch::SCHEMAS,
    &operation::SCHEMAS,
    &layer_candidate::SCHEMAS,
    &sync::SCHEMAS,
];

pub(super) const CONTRACT: SchemaContract = SchemaContract {
    identity: SchemaIdentity::Working,
    format_marker: WORKING_FORMAT_MARKER,
    schema_version: WORKING_SCHEMA_VERSION,
    table_names: &TABLE_NAMES,
    table_partitions: &TABLE_PARTITIONS,
    index_schemas: &index::INDEX_SCHEMAS,
};
