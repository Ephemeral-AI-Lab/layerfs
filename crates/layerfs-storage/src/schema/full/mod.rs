//! Exact Full-family manifest assembly.

mod branch;
mod index;
mod kernel;
mod layer_stack;
mod operation;
mod sync;

use super::{SchemaContract, SchemaIdentity, FULL_FORMAT_MARKER, FULL_SCHEMA_VERSION};

pub(super) const TABLE_NAMES: [&str; 21] = [
    "layerfs_branch_deltas",
    "layerfs_branch_push_pages",
    "layerfs_branch_transitions",
    "layerfs_branches",
    "layerfs_deltas",
    "layerfs_durable_tracking_refs",
    "layerfs_fetch_closure_items",
    "layerfs_layer_stack_transitions",
    "layerfs_layer_stacks",
    "layerfs_layers",
    "layerfs_objects",
    "layerfs_operation_versions",
    "layerfs_operations",
    "layerfs_released_versions",
    "layerfs_retained_roots",
    "layerfs_store_meta",
    "layerfs_sync_batch_receipts",
    "layerfs_sync_object_pins",
    "layerfs_sync_receipts",
    "layerfs_transfer_state",
    "layerfs_version_leases",
];

const TABLE_PARTITIONS: [&[(&str, &str)]; 5] = [
    &kernel::SCHEMAS,
    &branch::SCHEMAS,
    &layer_stack::SCHEMAS,
    &operation::SCHEMAS,
    &sync::SCHEMAS,
];

pub(super) const CONTRACT: SchemaContract = SchemaContract {
    identity: SchemaIdentity::Full,
    format_marker: FULL_FORMAT_MARKER,
    schema_version: FULL_SCHEMA_VERSION,
    table_names: &TABLE_NAMES,
    table_partitions: &TABLE_PARTITIONS,
    index_schemas: &index::INDEX_SCHEMAS,
};
