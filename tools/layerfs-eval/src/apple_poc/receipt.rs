use crate::legacy_full::{
    CompactionDiagnostics, Diagnostics, LayerFs, OperationDiagnostics, RootId, VfsError,
};
use std::path::PathBuf;
use std::time::Duration;

const MAX_FIXTURE_FILE_BYTES: usize = 100 * 1024 * 1024;
const LARGE_FIXTURE_BYTES: usize = 3 * 1024 * 1024;
const MANAGED_FIXTURE_BYTES: usize = 1024 * 1024;
const _: () = assert!(LARGE_FIXTURE_BYTES <= MAX_FIXTURE_FILE_BYTES);
const _: () = assert!(MANAGED_FIXTURE_BYTES <= MAX_FIXTURE_FILE_BYTES);

#[derive(Clone, Debug)]
pub struct PocReceipt {
    pub root_a: RootId,
    pub root_final: RootId,
    pub wall: Duration,
    pub store_bytes: u64,
    pub residue: Vec<PathBuf>,
    pub page_size: i64,
    pub cache_pages: i64,
    pub database_bytes: Option<u64>,
    pub logical_engine_bytes: Option<u64>,
    pub diagnostics: Diagnostics,
    pub compaction: CompactionDiagnostics,
    pub fd_baseline: usize,
    pub fd_terminal: usize,
    pub stages: Vec<StageReceipt>,
    pub operations: Vec<OperationReceipt>,
    pub operation_q_bound_bytes: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct StageReceipt {
    pub label: &'static str,
    pub root: RootId,
    pub diagnostics: Diagnostics,
}

#[derive(Clone, Copy, Debug)]
pub struct OperationReceipt {
    pub operation: &'static str,
    pub api_route: &'static str,
    pub wall_ns: u128,
    pub root_before: RootId,
    pub root_after: RootId,
    pub diagnostics: Option<(Diagnostics, Diagnostics)>,
    pub operation_diagnostics: Option<OperationDiagnostics>,
}

pub(super) fn operation_receipt(
    operation: &'static str,
    api_route: &'static str,
    wall_ns: u128,
    root_before: RootId,
    root_after: RootId,
    before: Diagnostics,
    after: Diagnostics,
) -> OperationReceipt {
    OperationReceipt {
        operation,
        api_route,
        wall_ns,
        root_before,
        root_after,
        diagnostics: Some((before, after)),
        operation_diagnostics: None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn operation_receipt_observed(
    operation: &'static str,
    api_route: &'static str,
    wall_ns: u128,
    root_before: RootId,
    root_after: RootId,
    before: Diagnostics,
    after: Diagnostics,
    operation_diagnostics: OperationDiagnostics,
) -> OperationReceipt {
    OperationReceipt {
        operation,
        api_route,
        wall_ns,
        root_before,
        root_after,
        diagnostics: Some((before, after)),
        operation_diagnostics: Some(operation_diagnostics),
    }
}

pub(super) fn operation_without_diagnostics(
    operation: &'static str,
    api_route: &'static str,
    wall_ns: u128,
    root_before: RootId,
    root_after: RootId,
) -> OperationReceipt {
    OperationReceipt {
        operation,
        api_route,
        wall_ns,
        root_before,
        root_after,
        diagnostics: None,
        operation_diagnostics: None,
    }
}

pub(super) fn assert_one_commit(
    before: Diagnostics,
    after: Diagnostics,
) -> Result<(), Box<dyn std::error::Error>> {
    if after.transactions_started != before.transactions_started + 1
        || after.transactions_committed != before.transactions_committed + 1
    {
        return Err("state-changing capture did not use one transaction/COMMIT".into());
    }
    Ok(())
}

pub(super) fn stage_receipt(
    label: &'static str,
    root: RootId,
    layerfs: &LayerFs,
) -> Result<StageReceipt, VfsError> {
    Ok(StageReceipt {
        label,
        root,
        diagnostics: layerfs.diagnostics()?,
    })
}
