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
pub use crate::history::spill::{Space, BUDGET_BYTES, LEAST_BUDGET_BYTES};
pub use crate::upgrade::chain::{
    check_chain as upgrade_check_chain, plan as upgrade_plan, run as upgrade_run,
    Step as UpgradeStep,
};

/// One shipped upgrade step's data: `(from, to, raw encodings, variants)`.
pub type StepData = (i64, i64, &'static [(i64, i64)], &'static [&'static str]);

/// The SQLite chain's steps, oldest first, and its floor.
pub fn sqlite_upgrade_steps() -> (Vec<StepData>, i64) {
    let steps = crate::backend::sqlite::upgrade::step::steps();
    let data = steps
        .iter()
        .map(|s| (s.from, s.to, s.raw, s.variants))
        .collect();
    (data, crate::backend::sqlite::upgrade::step::FLOOR)
}

/// `(schema, acquisition * 3 + layout)` of a SQLite `user_version`.
pub fn sqlite_decode_user_version(user_version: i64) -> Option<(i64, i64)> {
    crate::backend::sqlite::upgrade::format::decode(user_version)
}

/// For a schema-3 `user_version`: the last step's frozen target source and
/// the source this build's schema constants write.
pub fn sqlite_target_and_current_source(user_version: i64) -> (String, String) {
    let selection = crate::backend::sqlite::upgrade::verify::selection(user_version);
    (
        crate::backend::sqlite::upgrade::frozen::source_v3(user_version),
        crate::backend::sqlite::schema::source(selection),
    )
}

/// The frozen schema-2 (d) source of a schema-2 `user_version`.
pub fn sqlite_v2_source(user_version: i64) -> String {
    crate::backend::sqlite::upgrade::frozen::source_v2(user_version)
}

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

/// The PostgreSQL chain's steps, oldest first, and its floor.
pub fn postgresql_upgrade_steps() -> (Vec<StepData>, i64) {
    let steps = crate::backend::postgresql::upgrade::step::steps();
    let data = steps
        .iter()
        .map(|s| (s.from, s.to, s.raw, s.variants))
        .collect();
    (data, crate::backend::postgresql::upgrade::step::FLOOR)
}

/// For PostgreSQL with or without the acquisition tables: the last step's
/// frozen target source and the source this build's schema constants write.
pub fn postgresql_target_and_current_source(acquisition: bool) -> (String, String) {
    (
        crate::backend::postgresql::upgrade::frozen::source_v4(acquisition),
        crate::backend::postgresql::schema::recorded_source(acquisition),
    )
}

/// The frozen PostgreSQL column definitions before and after step 3 -> 4.
pub fn postgresql_step_columns(acquisition: bool) -> (String, String) {
    use crate::backend::postgresql::upgrade::frozen;
    (
        frozen::columns_v3(acquisition),
        frozen::columns_v4(acquisition),
    )
}
