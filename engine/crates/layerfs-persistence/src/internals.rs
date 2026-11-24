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
pub use crate::backend::postgresql::session::pipeline::{
    Refusal as PipelineRefusal, Reply as PipelineReply, Request as PipelineRequest,
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

/// One pipelined flight on the session of `handles`
/// (`postgresql::session::pipeline`): every request sent before any reply is
/// awaited, every reply returned in order. Stage 2 of #768 sends a Commit
/// this way; until then only the pipeline's own test calls it.
pub fn postgresql_pipeline(
    handles: &crate::PostgresHandles,
    requests: &[PipelineRequest<'_>],
) -> Result<Vec<Result<PipelineReply, PipelineRefusal>>, BackendError> {
    handles.storage.session.pipeline(requests)
}

/// One write transaction on the session of `handles` whose body runs
/// `scripts` in order and ignores every refusal, then answers success: what
/// the session reports when a body leaves its transaction aborted (#768).
pub fn postgresql_write_ignoring(
    handles: &crate::PostgresHandles,
    scripts: &[&str],
) -> Result<(), BackendError> {
    use crate::backend::postgresql::session::transaction::Mode;
    handles
        .storage
        .session
        .run_as(Mode::Write, |tx| -> Result<(), BackendError> {
            for script in scripts {
                let _ = tx.batch(script);
            }
            Ok(())
        })
}

/// A fused Commit made at `time_ms` (`Committer::commit_at`): the
/// PostgreSQL identity is then the same for the same request, so a test can
/// make one that already exists.
pub fn fused_commit_at(
    committer: &crate::Committer,
    tail: Option<&layerfs_storage::port::Publication>,
    stack: layerfs_history::LayerStackId,
    request: &layerfs_history::records::CommitRequest,
    time_ms: i64,
) -> Result<layerfs_history::records::CommitOutcome, crate::Fused> {
    committer.commit_at(tail, stack, request, &[], Some(time_ms))
}
