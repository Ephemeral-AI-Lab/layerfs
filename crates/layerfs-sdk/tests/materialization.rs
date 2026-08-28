#![cfg(target_os = "macos")]

use layerfs_sdk::{
    BranchId, CommitResult, IntegrityMode, LayerFs, LayerId, LayerStackId, LeaseKind,
};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn shipped_materialization_route_captures_native_tools_into_working_history() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-product-materialization-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let fs = LayerFs::open(&base.join("working"), IntegrityMode::Verified).unwrap();
    let root = fs.initialize_empty_root().unwrap();
    let stack = fs
        .create_layer_stack(
            LayerStackId::from_bytes([0x21; 32]),
            LayerId::from_bytes([0x22; 32]),
            "main",
            root,
        )
        .unwrap();
    let branch = fs
        .create_top_level_branch(BranchId::from_bytes([0x23; 32]), Some("native"), stack)
        .unwrap();
    let operation = fs.begin_materialization(branch).unwrap();
    let view = operation.path().to_owned();
    let process = operation.leases().acquire(LeaseKind::Process).unwrap();
    let status = Command::new("/bin/bash")
        .current_dir(&view)
        .arg("-c")
        .arg("mkdir -p nested; printf native > nested/result; ln nested/result hardlink; ln -s nested/result symlink; chmod 640 nested/result; xattr -w user.layerfs exact nested/result; dd if=/dev/zero of=mapped bs=4096 count=1 status=none")
        .status()
        .unwrap();
    let mmap_status = Command::new("/usr/bin/python3")
        .current_dir(&view)
        .args([
            "-c",
            "import mmap; f=open('mapped','r+b'); m=mmap.mmap(f.fileno(),0); m[:4]=b'MMAP'; m.flush(); m.close(); f.close()",
        ])
        .status()
        .unwrap();
    drop(process);
    assert!(status.success());
    assert!(mmap_status.success());
    assert_eq!(fs::read(view.join("nested/result")).unwrap(), b"native");
    let receipt = operation.commit().unwrap();
    assert!(receipt.cleanup.is_ok());
    assert!(receipt.acknowledgement.as_ref().is_some_and(Result::is_ok));
    assert!(receipt.timers.equation_closed);
    assert_eq!(
        receipt.timers.working_recorded_ns,
        receipt.timers.quiescence_ns
            + receipt.timers.candidate_ns
            + receipt.timers.working_commit_ns
    );
    assert_eq!(
        receipt.candidate_root,
        receipt.cleanup.as_ref().unwrap().candidate_root.unwrap()
    );
    let accepted = match receipt.outcome {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("materialization conflicted"),
    };
    assert!(!view.exists());
    let mut bytes = Vec::new();
    fs.stream(
        fs.pin_branch_version(accepted).unwrap(),
        "nested/result",
        &mut bytes,
    )
    .unwrap();
    assert_eq!(bytes, b"native");
    bytes.clear();
    fs.read_range(
        fs.pin_branch_version(accepted).unwrap(),
        "mapped",
        0..4,
        &mut bytes,
    )
    .unwrap();
    assert_eq!(bytes, b"MMAP");
    assert_eq!(
        fs.readlink(fs.pin_branch_version(accepted).unwrap(), "symlink")
            .unwrap()
            .0,
        b"nested/result"
    );
    let (nested, _) = fs
        .stat(fs.pin_branch_version(accepted).unwrap(), "nested/result")
        .unwrap();
    let (hardlink, _) = fs
        .stat(fs.pin_branch_version(accepted).unwrap(), "hardlink")
        .unwrap();
    assert_eq!(nested.namespace_ref_count, 2);
    assert_eq!(hardlink.namespace_ref_count, 2);
    assert_eq!(nested.content_root, hardlink.content_root);
    assert_eq!(nested.metadata_root, hardlink.metadata_root);

    let reconstructed = fs.begin_materialization(accepted).unwrap();
    let reconstructed_view = reconstructed.path().to_owned();
    let nested_native = fs::metadata(reconstructed_view.join("nested/result")).unwrap();
    let hardlink_native = fs::metadata(reconstructed_view.join("hardlink")).unwrap();
    assert_eq!(nested_native.ino(), hardlink_native.ino());
    assert_eq!(nested_native.permissions().mode() & 0o777, 0o640);
    assert_eq!(
        fs::read_link(reconstructed_view.join("symlink")).unwrap(),
        std::path::PathBuf::from("nested/result")
    );
    let xattr = Command::new("/usr/bin/xattr")
        .args(["-p", "user.layerfs"])
        .arg(reconstructed_view.join("nested/result"))
        .output()
        .unwrap();
    assert!(xattr.status.success());
    assert_eq!(xattr.stdout, b"exact\n");
    reconstructed.discard().unwrap();
    drop(fs);

    let reopened = LayerFs::open(&base.join("working"), IntegrityMode::Verified).unwrap();
    assert_eq!(
        reopened.branch_head(branch.branch_id).unwrap(),
        Some(accepted)
    );
    drop(reopened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn dropping_busy_materialization_preserves_recoverable_authority() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-busy-materialization-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let fs = LayerFs::open(&base.join("working"), IntegrityMode::TrustedLocalDev).unwrap();
    let root = fs.initialize_empty_root().unwrap();
    let stack = fs
        .create_layer_stack(
            LayerStackId::from_bytes([0x31; 32]),
            LayerId::from_bytes([0x32; 32]),
            "busy",
            root,
        )
        .unwrap();
    let branch = fs
        .create_top_level_branch(BranchId::from_bytes([0x33; 32]), Some("busy"), stack)
        .unwrap();
    let operation = fs.begin_materialization(branch).unwrap();
    let operation_id = operation.operation_id();
    let view = operation.path().to_owned();
    let process = operation.leases().acquire(LeaseKind::Process).unwrap();
    drop(operation);
    assert!(view.exists());
    let recovery = fs.recoverable_operations(8).unwrap();
    assert_eq!(recovery.len(), 1);
    assert_eq!(recovery[0].operation_id, operation_id);
    assert_eq!(fs.branch_head(branch.branch_id).unwrap(), Some(branch));
    drop(process);
    fs.discard_recovered_operation(operation_id).unwrap();
    drop(fs);
    fs::remove_dir_all(base).unwrap();
}
