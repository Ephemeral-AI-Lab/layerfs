use super::evidence::{fd_count, owned_residue, tree_bytes};
use super::native::{assert_apple_metadata, assert_managed_root};
use super::receipt::{
    operation_receipt, operation_receipt_observed, operation_without_diagnostics, stage_receipt,
    PocReceipt,
};
use super::tree::{assert_tree_equal, deterministic_byte, read_range};
use crate::legacy_full::LayerFs;
use compaction::compact_and_verify;
use external_mutation::external_mutation;
use import_capture::import_capture;
use managed_edits::managed_edits;
use materialize::materialize_live_views;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

mod compaction;
mod external_mutation;
mod import_capture;
mod managed_edits;
mod materialize;

pub(super) fn run_workflow(base: &Path) -> Result<PocReceipt, Box<dyn std::error::Error>> {
    let fd_baseline = fd_count()?;
    fs::create_dir(base)?;
    let started = Instant::now();
    let store = base.join("store");
    let opened = LayerFs::open(&store)?;
    let mut stages = vec![stage_receipt("S0", opened.head, &opened.fs)?];
    let mut operations = Vec::new();
    let (root_a, source_path) = import_capture(base, &opened, &mut stages, &mut operations)?;
    let (cold, warm) = materialize_live_views(
        base,
        &opened,
        root_a,
        &source_path,
        &mut stages,
        &mut operations,
    )?;
    let (root_s3, root_s4, root_s5, root_s6) =
        managed_edits(base, &opened, root_a, &mut stages, &mut operations)?;
    let (root_final, diagnostics, final_native_path) = external_mutation(
        base,
        &store,
        &opened,
        cold,
        warm,
        root_s6,
        &mut stages,
        &mut operations,
    )?;
    drop(opened);
    let operation_started = Instant::now();
    let reopened = LayerFs::open(&store)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_without_diagnostics(
        "reopen",
        "store-open",
        operation_wall_ns,
        root_final,
        reopened.head,
    ));
    if reopened.head != root_final {
        return Err("S10 reopened head mismatch".into());
    }
    let historical = reopened
        .fs
        .materialize_external(root_a, &base.join("historical"))?;
    let before_range = reopened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let historical_range = read_range(&historical.path().join("nested/large.bin"), 4096, 4096)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    assert_eq!(
        historical_range,
        (4096..8192).map(deterministic_byte).collect::<Vec<_>>()
    );
    operations.push(operation_receipt(
        "historical-range-read",
        "native-file-read",
        operation_wall_ns,
        root_a,
        root_a,
        before_range,
        reopened.fs.diagnostics()?,
    ));
    let reopened_final = reopened
        .fs
        .materialize_external(root_final, &base.join("reopened-final"))?;
    assert_tree_equal(&final_native_path, reopened_final.path())?;
    assert_eq!(
        fs::read(reopened_final.path().join("made/moved.txt"))?,
        b"shell"
    );
    assert_eq!(
        &fs::read(reopened_final.path().join("nested/large.bin"))?[32768..32772],
        b"MMAP"
    );
    assert_eq!(
        fs::read(reopened_final.path().join("append.txt"))?,
        b"append"
    );
    assert_eq!(
        fs::metadata(reopened_final.path().join("nested/large.bin"))?.ino(),
        fs::metadata(reopened_final.path().join("bash-hardlink"))?.ino()
    );
    let xattr = Command::new("xattr")
        .args(["-p", "com.layerfs.eval"])
        .arg(reopened_final.path().join("nested/large.bin"))
        .output()?;
    if !xattr.status.success() || xattr.stdout != b"exact\n" {
        return Err("S10 xattr oracle mismatch".into());
    }
    assert_apple_metadata(&reopened_final.path().join("nested/scripts/run.sh"))?;
    for (index, root) in [root_s3, root_s4, root_s5, root_s6].into_iter().enumerate() {
        let retained = reopened
            .fs
            .materialize_external(root, &base.join(format!("retained-s{}", index + 3)))?;
        assert_managed_root(retained.path(), index + 3)?;
        drop(retained);
    }
    stages.push(stage_receipt("S10", root_final, &reopened.fs)?);
    let before_fork = reopened.fs.diagnostics()?;
    let operation_started = Instant::now();
    assert_eq!(reopened.fs.fork(root_final, "root-final")?, root_final);
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt(
        "fork",
        "api-fork",
        operation_wall_ns,
        root_final,
        root_final,
        before_fork,
        reopened.fs.diagnostics()?,
    ));
    let before_rollback = reopened.fs.diagnostics()?;
    let expected_rollback = reopened.fs.current_head("main")?;
    if expected_rollback.root != root_final {
        return Err("S11 rollback base changed before request".into());
    }
    let operation_started = Instant::now();
    assert_eq!(
        reopened.fs.rollback(&expected_rollback, root_a)?.root,
        root_a
    );
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt(
        "rollback",
        "api-rollback",
        operation_wall_ns,
        root_final,
        root_a,
        before_rollback,
        reopened.fs.diagnostics()?,
    ));
    let before_edit = reopened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let mut diverged = reopened.fs.materialize_managed(root_a)?;
    let operation_diagnostics = diverged.replace_observed("nested/managed.bin", 0, 1, b"D")?;
    let (root_diverged, capture_diagnostics) = diverged.capture_observed()?;
    let operation_diagnostics = operation_diagnostics.merge(capture_diagnostics)?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt_observed(
        "divergent-edit",
        "managed-replace-capture",
        operation_wall_ns,
        root_a,
        root_diverged,
        before_edit,
        reopened.fs.diagnostics()?,
        operation_diagnostics,
    ));
    if root_diverged == root_a || root_diverged == root_final {
        return Err("S11 divergent branch reused a non-divergent root".into());
    }
    let before_fork = reopened.fs.diagnostics()?;
    let operation_started = Instant::now();
    assert_eq!(
        reopened.fs.fork(root_diverged, "root-diverged")?,
        root_diverged
    );
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt(
        "fork",
        "api-fork",
        operation_wall_ns,
        root_diverged,
        root_diverged,
        before_fork,
        reopened.fs.diagnostics()?,
    ));
    let diverged_oracle = reopened
        .fs
        .materialize_external(root_diverged, &base.join("root-diverged"))?;
    assert_eq!(
        read_range(&diverged_oracle.path().join("nested/managed.bin"), 0, 1)?,
        b"D"
    );
    drop(diverged_oracle);
    let before_rollback = reopened.fs.diagnostics()?;
    let expected_rollback = reopened.fs.current_head("main")?;
    if expected_rollback.root != root_diverged {
        return Err("S11 divergent rollback base changed before request".into());
    }
    let operation_started = Instant::now();
    assert_eq!(
        reopened.fs.rollback(&expected_rollback, root_final)?.root,
        root_final
    );
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt(
        "rollback",
        "api-rollback",
        operation_wall_ns,
        root_diverged,
        root_final,
        before_rollback,
        reopened.fs.diagnostics()?,
    ));
    stages.push(stage_receipt("S11", root_final, &reopened.fs)?);
    let reopened_final_path = reopened_final.path().to_owned();
    drop(historical);
    drop(reopened_final);
    let compaction_evidence = compact_and_verify(
        &store,
        base,
        reopened,
        root_a,
        root_final,
        root_diverged,
        [root_s3, root_s4, root_s5, root_s6],
        &source_path,
        &reopened_final_path,
        &mut stages,
        &mut operations,
    )?;

    if stages.iter().any(|stage| {
        stage.diagnostics.operation_q_current_bytes != 0
            || stage.diagnostics.operation_q_high_water_bytes
                > stage.diagnostics.operation_q_bound_bytes
    }) {
        return Err(
            "operation-owned Q reservation did not return to zero or exceeded its bound".into(),
        );
    }

    let store_bytes = tree_bytes(&store)?;
    let residue = owned_residue(base)?;
    let fd_terminal = fd_count()?;
    if fd_terminal != fd_baseline {
        return Err(format!(
            "file descriptor leak: baseline={fd_baseline}, terminal={fd_terminal}"
        )
        .into());
    }
    Ok(PocReceipt {
        root_a,
        root_final,
        wall: started.elapsed(),
        store_bytes,
        residue,
        page_size: compaction_evidence.page_size,
        cache_pages: compaction_evidence.cache_pages,
        database_bytes: compaction_evidence.database_bytes,
        logical_engine_bytes: compaction_evidence.logical_engine_bytes,
        diagnostics,
        compaction: compaction_evidence.compaction,
        fd_baseline,
        fd_terminal,
        stages,
        operations,
        operation_q_bound_bytes: compaction_evidence.operation_q_bound_bytes,
    })
}
