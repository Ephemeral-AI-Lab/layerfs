//! Test-only re-exports for unit checks that must live outside product source
//! (the product-boundary guard refuses test-only attributes in `src/`).
//!
//! Nothing in the product calls these; they exist so an integration test can
//! cover the same §7/§8 helpers the product source used to test in place.
pub use crate::backend::postgresql::compact::{decode_kept_waste, encode_kept_waste, report};
pub use crate::backend::postgresql::compact::{
    effective_rank, standby_satisfied, sync_name, sync_rank, takes_rebuild_path, Replica,
};
pub use crate::backend::postgresql::probe::{
    reads as postgresql_reads, Outcome as PostgresReadOutcome, Read as PostgresRead,
};
pub use crate::backend::postgresql::session::rows::sqlstate;
pub use crate::backend::records::BackendError;
pub use crate::backend::sqlite::compact::{
    cap_delta_references, io_error, peak_temp_bytes, receipt, reserve, source_mmap, staged_windows,
    trim, verify_inode_blocks, windows, PackLengths, PackScan, Rewritten, StagedLocators, Window,
    DELTA_BASE_REFERENCES_CAP, GAP_BYTES, SRC_MMAP_BYTES, WINDOW_BYTES,
};
pub use crate::backend::sqlite::convert_v1::{
    live_definition as sqlite_definition, new_source as converted_source,
    old_source as schema_version_1_source, CANDIDATES as SCHEMA_VERSION_1_SOURCES,
};
pub use crate::history::spill::{Space, BUDGET_BYTES, LEAST_BUDGET_BYTES};

/// One inode value block's values (`blocks::values::decode`).
pub fn decode_value_block(
    first: u32,
    last: u32,
    body: &[u8],
) -> Result<Vec<[u8; layerfs_content::inode_leaf::INODE_VALUE_BYTES]>, BackendError> {
    crate::blocks::values::decode(first, last, body)
}

/// One binding hint block's hints (`blocks::hints::decode`).
pub fn decode_hint_block(
    group: i64,
    body: &[u8],
) -> Result<Vec<layerfs_history::records::BindingHint>, BackendError> {
    crate::blocks::hints::decode(group, body)
}
