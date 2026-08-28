use super::super::native::assert_apple_metadata;
use super::super::receipt::{
    operation_receipt_observed, stage_receipt, OperationReceipt, StageReceipt,
};
use super::super::tree::assert_tree_equal;
use crate::legacy_full::{ExternalWorkspace, OpenedLayerFs, RootId};
use std::path::Path;
use std::time::Instant;

pub(super) fn materialize_live_views(
    base: &Path,
    opened: &OpenedLayerFs,
    root_a: RootId,
    source_path: &Path,
    stages: &mut Vec<StageReceipt>,
    operations: &mut Vec<OperationReceipt>,
) -> Result<(ExternalWorkspace, ExternalWorkspace), Box<dyn std::error::Error>> {
    let before_cold = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let (cold, operation_diagnostics) = opened
        .fs
        .materialize_external_observed(root_a, &base.join("cold"))?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "cold-materialize",
        "external-materialize",
        operation_wall_ns,
        root_a,
        root_a,
        before_cold,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    let before_warm = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let (mut warm, operation_diagnostics) = opened
        .fs
        .materialize_external_observed(root_a, cold.path())?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "live-warm-no-op",
        "external-materialize",
        operation_wall_ns,
        root_a,
        root_a,
        before_warm,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    assert_tree_equal(source_path, cold.path())?;
    assert_apple_metadata(&cold.path().join("nested/scripts/run.sh"))?;
    if warm.capture_quiescent()? != root_a {
        return Err("S2 exact-live no-op changed the canonical root".into());
    }
    stages.push(stage_receipt("S2", root_a, &opened.fs)?);

    Ok((cold, warm))
}
