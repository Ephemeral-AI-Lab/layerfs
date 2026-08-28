use super::super::native::assert_managed_root;
use super::super::receipt::{
    operation_receipt_observed, stage_receipt, OperationReceipt, StageReceipt,
};
use crate::legacy_full::{OpenedLayerFs, RootId};
use std::path::Path;
use std::time::Instant;

pub(super) fn managed_edits(
    base: &Path,
    opened: &OpenedLayerFs,
    root_a: RootId,
    stages: &mut Vec<StageReceipt>,
    operations: &mut Vec<OperationReceipt>,
) -> Result<(RootId, RootId, RootId, RootId), Box<dyn std::error::Error>> {
    let before_edit = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mut managed = opened.fs.materialize_managed(root_a)?;
    let operation_diagnostics =
        managed.replace_observed("nested/managed.bin", 4096, 4096, &vec![0xa5; 4096])?;
    let (root_s3, capture_diagnostics) = managed.capture_observed()?;
    let operation_diagnostics = operation_diagnostics.merge(capture_diagnostics)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "managed-overwrite",
        "managed-replace-capture",
        operation_wall_ns,
        root_a,
        root_s3,
        before_edit,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    stages.push(stage_receipt("S3", root_s3, &opened.fs)?);
    let before_edit = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mut managed = opened.fs.materialize_managed(root_s3)?;
    let operation_diagnostics =
        managed.replace_observed("nested/managed.bin", 8192, 0, &vec![0x3c; 8192])?;
    let (root_s4, capture_diagnostics) = managed.capture_observed()?;
    let operation_diagnostics = operation_diagnostics.merge(capture_diagnostics)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "managed-insert",
        "managed-replace-capture",
        operation_wall_ns,
        root_s3,
        root_s4,
        before_edit,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    stages.push(stage_receipt("S4", root_s4, &opened.fs)?);
    let before_edit = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mut managed = opened.fs.materialize_managed(root_s4)?;
    let operation_diagnostics =
        managed.replace_observed("nested/managed.bin", 16_384, 4096, &[])?;
    let (root_s5, capture_diagnostics) = managed.capture_observed()?;
    let operation_diagnostics = operation_diagnostics.merge(capture_diagnostics)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "managed-delete",
        "managed-replace-capture",
        operation_wall_ns,
        root_s4,
        root_s5,
        before_edit,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    stages.push(stage_receipt("S5", root_s5, &opened.fs)?);
    let before_edit = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mut managed = opened.fs.materialize_managed(root_s5)?;
    let operation_diagnostics =
        managed.replace_observed("nested/managed.bin", 1_048_576, 4096, &[])?;
    let (root_s6_truncated, capture_diagnostics) = managed.capture_observed()?;
    let operation_diagnostics = operation_diagnostics.merge(capture_diagnostics)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "managed-truncate",
        "managed-replace-capture",
        operation_wall_ns,
        root_s5,
        root_s6_truncated,
        before_edit,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    let before_edit = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mut managed = opened.fs.materialize_managed(root_s6_truncated)?;
    let operation_diagnostics = managed.rename_observed("relative-link", "managed-link")?;
    let (root_s6, capture_diagnostics) = managed.capture_observed()?;
    let operation_diagnostics = operation_diagnostics.merge(capture_diagnostics)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "managed-rename",
        "managed-rename-capture",
        operation_wall_ns,
        root_s6_truncated,
        root_s6,
        before_edit,
        opened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    let s6_oracle = opened
        .fs
        .materialize_external(root_s6, &base.join("s6-oracle"))?;
    assert_managed_root(s6_oracle.path(), 6)?;
    drop(s6_oracle);
    stages.push(stage_receipt("S6", root_s6, &opened.fs)?);

    Ok((root_s3, root_s4, root_s5, root_s6))
}
