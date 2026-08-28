use super::super::native::command_ok;
use super::super::receipt::{
    assert_one_commit, operation_receipt, operation_receipt_observed, stage_receipt,
    OperationReceipt, StageReceipt,
};
use super::super::tree::deterministic_byte;
use crate::legacy_full::{OpenedLayerFs, RootId};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, UNIX_EPOCH};

const MAX_FIXTURE_FILE_BYTES: usize = 100 * 1024 * 1024;
const LARGE_FIXTURE_BYTES: usize = 3 * 1024 * 1024;
const MANAGED_FIXTURE_BYTES: usize = 1024 * 1024;
const _: () = assert!(LARGE_FIXTURE_BYTES <= MAX_FIXTURE_FILE_BYTES);
const _: () = assert!(MANAGED_FIXTURE_BYTES <= MAX_FIXTURE_FILE_BYTES);

pub(super) fn import_capture(
    base: &Path,
    opened: &OpenedLayerFs,
    stages: &mut Vec<StageReceipt>,
    operations: &mut Vec<OperationReceipt>,
) -> Result<(RootId, PathBuf), Box<dyn std::error::Error>> {
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))?;
    fs::create_dir_all(source.path().join("nested/scripts"))?;
    fs::write(source.path().join("empty"), [])?;
    let mut initial_large = vec![0_u8; LARGE_FIXTURE_BYTES];
    for (index, byte) in initial_large.iter_mut().enumerate() {
        *byte = deterministic_byte(index);
    }
    fs::write(source.path().join("nested/large.bin"), &initial_large)?;
    drop(initial_large);
    fs::write(
        source.path().join("nested/managed.bin"),
        vec![0x6d; MANAGED_FIXTURE_BYTES],
    )?;
    fs::write(
        source.path().join("nested/scripts/run.sh"),
        b"#!/bin/bash\nprintf layerfs-poc\n",
    )?;
    fs::set_permissions(
        source.path().join("nested/scripts/run.sh"),
        fs::Permissions::from_mode(0o755),
    )?;
    let script = source.path().join("nested/scripts/run.sh");
    command_ok(
        Command::new("chmod")
            .args(["+a", "everyone allow read"])
            .arg(&script),
    )?;
    command_ok(Command::new("chflags").arg("hidden").arg(&script))?;
    command_ok(
        Command::new("xattr")
            .args([
                "-wx",
                "com.apple.FinderInfo",
                "0000000000000000000000000000000000000000000000000000000000000001",
            ])
            .arg(&script),
    )?;
    command_ok(
        Command::new("xattr")
            .args(["-w", "com.apple.ResourceFork", "resource-fork"])
            .arg(&script),
    )?;
    fs::File::open(&script)?.set_times(
        fs::FileTimes::new().set_modified(UNIX_EPOCH.checked_sub(Duration::from_secs(2)).unwrap()),
    )?;
    fs::hard_link(
        source.path().join("nested/large.bin"),
        source.path().join("large-hardlink"),
    )?;
    symlink("nested/large.bin", source.path().join("relative-link"))?;
    symlink(
        "/private/tmp/layerfs-absolute-target",
        source.path().join("absolute-link"),
    )?;
    symlink("missing", source.path().join("dangling-link"))?;
    let before_capture = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    let (root_a, operation_diagnostics) = source.capture_quiescent_observed()?;
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    let after_capture = opened.fs.diagnostics()?;
    assert_one_commit(before_capture, after_capture)?;
    operations.push(operation_receipt_observed(
        "import-capture",
        "external-capture",
        operation_wall_ns,
        opened.head,
        root_a,
        before_capture,
        after_capture,
        operation_diagnostics,
    ));
    let before_fork = opened.fs.diagnostics()?;
    let operation_started = Instant::now();
    assert_eq!(opened.fs.fork(root_a, "root-a")?, root_a);
    let operation_wall_ns = operation_started.elapsed().as_nanos();
    operations.push(operation_receipt(
        "fork",
        "api-fork",
        operation_wall_ns,
        root_a,
        root_a,
        before_fork,
        opened.fs.diagnostics()?,
    ));
    stages.push(stage_receipt("S1", root_a, &opened.fs)?);
    let source_path = source.path().to_owned();
    drop(source);

    Ok((root_a, source_path))
}
