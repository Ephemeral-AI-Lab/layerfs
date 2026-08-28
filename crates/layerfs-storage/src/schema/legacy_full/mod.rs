//! Assembly of the current 29-table migration-source manifest.

mod history;
mod kernel;
mod sync;

pub use kernel::TRANSITION_FORMAT_VERSION;
pub(crate) use kernel::{
    admitted_store_id_counted, initialize_schema_counted, note_statement, BASE_SCHEMAS,
    FORMAT_MARKER, LEGACY_DELTA_SCHEMA, LEGACY_SCHEMA_VERSION, SCHEMA_VERSION,
};

pub(crate) const PRODUCT_SCHEMAS: [&[(&str, &str)]; 2] = [&history::SCHEMAS, &sync::SCHEMAS];
pub(crate) const CONTRACT_SCHEMAS: [&[(&str, &str)]; 3] =
    [&BASE_SCHEMAS, &history::SCHEMAS, &sync::SCHEMAS];

pub(crate) const CURRENT_TABLE_NAMES: [&str; 29] = [
    "layerfs_authority",
    "layerfs_branch_deltas",
    "layerfs_branch_push_pages",
    "layerfs_branch_transitions",
    "layerfs_branches",
    "layerfs_deltas",
    "layerfs_durable_storages",
    "layerfs_durable_tracking_refs",
    "layerfs_fetch_closure_items",
    "layerfs_fetch_staging_heads",
    "layerfs_layer_deltas",
    "layerfs_layer_stack_transitions",
    "layerfs_layer_stacks",
    "layerfs_layers",
    "layerfs_objects",
    "layerfs_operation_deltas",
    "layerfs_operation_versions",
    "layerfs_operations",
    "layerfs_push_outbox",
    "layerfs_refs",
    "layerfs_released_versions",
    "layerfs_retained_roots",
    "layerfs_roots",
    "layerfs_store_meta",
    "layerfs_sync_batch_receipts",
    "layerfs_sync_object_pins",
    "layerfs_sync_receipts",
    "layerfs_transfer_state",
    "layerfs_version_leases",
];

pub(crate) const LEGACY_TABLE_NAMES: [&str; 7] = [
    "layerfs_authority",
    "layerfs_deltas",
    "layerfs_objects",
    "layerfs_refs",
    "layerfs_retained_roots",
    "layerfs_roots",
    "layerfs_store_meta",
];
