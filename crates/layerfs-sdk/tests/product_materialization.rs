#![cfg(target_os = "macos")]

use layerfs_sdk::{
    BranchId, CommitResult, IntegrityMode, LayerFs, LayerId, LayerStackId, LeaseKind,
};
use std::fs;
use std::io::Cursor;
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
        receipt.timers.complete_wall_ns,
        receipt.timers.working_recorded_ns
            + receipt.timers.cleanup_ns
            + receipt.timers.unattributed_ns
    );
    assert_eq!(receipt.counters.operation_q_terminal_bytes, 0);
    assert_eq!(receipt.counters.owned_temp_terminal, 0);
    assert_eq!(receipt.counters.descriptor_spool_bytes_terminal, 0);
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

#[cfg(feature = "test-hooks")]
#[test]
fn startup_cleanup_refusal_preserves_operation_and_owned_residue() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-startup-cleanup-refusal-{}-{}",
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
            LayerStackId::from_bytes([0x71; 32]),
            LayerId::from_bytes([0x72; 32]),
            "startup-refusal",
            root,
        )
        .unwrap();
    let branch = fs
        .create_top_level_branch(
            BranchId::from_bytes([0x73; 32]),
            Some("startup-refusal"),
            stack,
        )
        .unwrap();
    layerfs_materialization::inject_start_failure_for_test();
    layerfs_workspace::inject_remove_owned_failure_for_test();

    assert!(fs.begin_materialization(branch).is_err());
    let recovery = fs.recoverable_operations(8).unwrap();
    assert_eq!(recovery.len(), 1);
    let operation_prefix = recovery[0]
        .operation_id
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let residues = fs::read_dir(base.join("working/workspaces"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(residues.len(), 1);
    let residue = &residues[0];
    assert!(residue
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with(&operation_prefix));
    assert!(residue.join("owner").is_file());
    assert!(residue.join("recovery").is_file());
    assert!(residue.join("view").is_dir());
    assert!(residue.join("spool").is_dir());
    assert_eq!(fs.branch_head(branch.branch_id).unwrap(), Some(branch));

    drop(fs);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn mixed_managed_and_external_changes_are_both_captured() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-managed-materialization-{}-{}",
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
            LayerStackId::from_bytes([0x41; 32]),
            LayerId::from_bytes([0x42; 32]),
            "managed",
            root,
        )
        .unwrap();
    let branch = fs
        .create_top_level_branch(BranchId::from_bytes([0x43; 32]), Some("managed"), stack)
        .unwrap();
    let mut seed = fs.begin_direct(branch).unwrap();
    seed.replace_file("file", Cursor::new(b"abcdef")).unwrap();
    seed.replace_file("external", Cursor::new(b"before"))
        .unwrap();
    let seeded = match seed.commit().unwrap().outcome {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("seed conflicted"),
    };

    let mut managed = fs.begin_materialization(seeded).unwrap();
    #[cfg(feature = "test-hooks")]
    layerfs_materialization::apfs::inject_clone_unsupported_for_test();
    let native = managed.managed_replace_range("file", 2, 2, b"XY").unwrap();
    #[cfg(feature = "test-hooks")]
    {
        assert_eq!(
            native.native.route,
            Some(layerfs_sdk::NativeRoute::FullFallback)
        );
        assert_eq!(native.native.clone_attempts, 1);
        assert_eq!(native.native.clone_fallbacks, 1);
        assert_eq!(native.native.clone_successes, 0);
        assert_eq!(native.full_fallback_files, 1);
        assert_eq!(native.native.bytes_read, 6);
        assert_eq!(native.native.bytes_written, 8);
    }
    #[cfg(not(feature = "test-hooks"))]
    assert!(matches!(
        native.native.route,
        Some(layerfs_sdk::NativeRoute::ClonePatch | layerfs_sdk::NativeRoute::FullFallback)
    ));
    let renamed = managed.managed_rename("file", "moved").unwrap();
    assert_eq!(renamed.native.route, Some(layerfs_sdk::NativeRoute::Rename));
    let shifted = managed.managed_replace_range("moved", 2, 0, b"++").unwrap();
    assert!(matches!(
        shifted.native.route,
        Some(layerfs_sdk::NativeRoute::CloneShift | layerfs_sdk::NativeRoute::FullFallback)
    ));
    assert!(shifted.native.suffix_bytes_shifted >= 4);
    assert_eq!(fs::read(managed.path().join("moved")).unwrap(), b"ab++XYef");
    assert!(!managed.path().join("file").exists());
    fs::write(managed.path().join("external"), b"outside").unwrap();
    let receipt = managed.commit().unwrap();
    assert!(receipt.cleanup.is_ok());
    assert_eq!(receipt.counters.authority_full_scans, 1);
    assert_eq!(receipt.counters.native.patch_bytes, 4);
    assert!(receipt.counters.full_fallback_files >= 1);
    assert!(receipt.timers.equation_closed);
    assert_eq!(
        receipt.timers.complete_wall_ns,
        receipt.timers.working_recorded_ns
            + receipt.timers.cleanup_ns
            + receipt.timers.unattributed_ns
    );
    assert_eq!(receipt.counters.operation_q_terminal_bytes, 0);
    assert_eq!(receipt.counters.owned_temp_terminal, 0);
    assert_eq!(receipt.counters.descriptor_spool_bytes_terminal, 0);
    let accepted = match receipt.outcome {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("managed patch conflicted"),
    };
    let mut bytes = Vec::new();
    fs.stream(
        fs.pin_branch_version(accepted).unwrap(),
        "moved",
        &mut bytes,
    )
    .unwrap();
    assert_eq!(bytes, b"ab++XYef");
    bytes.clear();
    fs.stream(
        fs.pin_branch_version(accepted).unwrap(),
        "external",
        &mut bytes,
    )
    .unwrap();
    assert_eq!(bytes, b"outside");

    let mut noop = fs.begin_materialization(accepted).unwrap();
    let noop_native = noop.managed_replace_range("moved", 4, 2, b"XY").unwrap();
    assert_eq!(
        noop_native.native.route,
        Some(layerfs_sdk::NativeRoute::ExactNoop)
    );
    let noop_receipt = noop.commit().unwrap();
    assert_eq!(
        noop_receipt.counters.native.route,
        Some(layerfs_sdk::NativeRoute::CaptureStream)
    );
    assert_eq!(noop_receipt.counters.authority_full_scans, 1);
    assert!(noop_receipt.timers.equation_closed);
    let noop_head = match noop_receipt.outcome {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("managed no-op conflicted"),
    };
    assert_eq!(noop_head.root, accepted.root);
    drop(fs);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn exclusive_managed_noop_and_merkle_refresh_avoid_full_capture() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-managed-refresh-{}-{}",
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
            LayerStackId::from_bytes([0x51; 32]),
            LayerId::from_bytes([0x52; 32]),
            "refresh",
            root,
        )
        .unwrap();
    let branch = fs
        .create_top_level_branch(BranchId::from_bytes([0x53; 32]), Some("refresh"), stack)
        .unwrap();
    let mut seed = fs.begin_direct(branch).unwrap();
    seed.replace_file("file", Cursor::new(vec![b'a'; 1024 * 1024]))
        .unwrap();
    let seeded_receipt = seed.commit().unwrap();
    let (seeded, seeded_record) = match seeded_receipt.outcome {
        CommitResult::WorkingRecorded { head, record, .. } => (head, record),
        CommitResult::Conflict { .. } => panic!("seed conflicted"),
    };
    let child = fs
        .create_child_branch(
            BranchId::from_bytes([0x54; 32]),
            Some("refresh-target"),
            seeded_record,
        )
        .unwrap();
    let mut target = fs.begin_direct(child).unwrap();
    target
        .replace_range("file", 512 * 1024, 4096, Cursor::new(vec![b'b'; 4096]))
        .unwrap();
    let target = match target.commit().unwrap().outcome {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("target conflicted"),
    };
    let mut target2 = fs.begin_direct(target).unwrap();
    target2
        .replace_range("file", 768 * 1024, 4096, Cursor::new(vec![b'c'; 4096]))
        .unwrap();
    let target2 = match target2.commit().unwrap().outcome {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("second target conflicted"),
    };

    let mut noop = fs.begin_managed_materialization(seeded).unwrap();
    let noop_delta = noop
        .refresh_to(fs.pin_branch_version(seeded).unwrap())
        .unwrap();
    assert_eq!(
        noop_delta.native.route,
        Some(layerfs_sdk::NativeRoute::ExactNoop)
    );
    assert_eq!(noop_delta.native.bytes_read, 0);
    assert_eq!(noop_delta.native.bytes_written, 0);
    assert_eq!(noop_delta.rope.cdc_bytes_scanned, 0);
    assert_eq!(noop_delta.authority_full_scans, 0);
    let noop_receipt = noop.commit().unwrap();
    assert!(noop_receipt.outcome.is_none());
    assert_eq!(noop_receipt.refresh_counters, noop_delta);
    assert_eq!(fs.branch_head(seeded.branch_id).unwrap(), Some(seeded));
    assert!(fs.recoverable_operations(8).unwrap().is_empty());

    let mut refresh = fs.begin_managed_materialization(seeded).unwrap();
    let delta = refresh
        .refresh_to(fs.pin_branch_version(target).unwrap())
        .unwrap();
    assert!(matches!(
        delta.native.route,
        Some(layerfs_sdk::NativeRoute::ClonePatch | layerfs_sdk::NativeRoute::InPlacePatch)
    ));
    assert_eq!(delta.changed_paths, 1);
    assert_eq!(delta.authority_full_scans, 0);
    assert_eq!(delta.rope.cdc_bytes_scanned, 0);
    assert!(delta.native.patch_bytes >= 4096);
    assert!(delta.native.patch_bytes <= 1024 * 1024);
    assert_eq!(
        refresh.read("file", 512 * 1024, 4096).unwrap(),
        vec![b'b'; 4096]
    );
    let delta2 = refresh
        .refresh_to(fs.pin_branch_version(target2).unwrap())
        .unwrap();
    assert_eq!(delta2.changed_paths, 1);
    assert_eq!(delta2.authority_full_scans, 0);
    assert_eq!(
        refresh.read("file", 768 * 1024, 4096).unwrap(),
        vec![b'c'; 4096]
    );
    let refreshed = refresh.commit().unwrap();
    assert_eq!(refreshed.refresh_counters, delta2);
    let refreshed = match refreshed.outcome {
        Some(CommitResult::WorkingRecorded { head, .. }) => head,
        Some(CommitResult::Conflict { .. }) => panic!("refresh conflicted"),
        None => panic!("changed refresh was discarded as a no-op"),
    };
    assert_eq!(refreshed.root, target2.root);

    #[cfg(feature = "test-hooks")]
    {
        let mut poisoned = fs.begin_managed_materialization(refreshed).unwrap();
        layerfs_materialization::inject_refresh_failure_for_test();
        assert!(poisoned
            .refresh_to(fs.pin_branch_version(target).unwrap())
            .is_err());
        assert!(poisoned.read("file", 0, 1).is_err());
        assert!(poisoned
            .refresh_to(fs.pin_branch_version(target).unwrap())
            .is_err());
        assert!(poisoned.commit().is_err());
        assert!(fs.recoverable_operations(8).unwrap().is_empty());
        assert_eq!(
            fs::read_dir(base.join("working/workspaces"))
                .unwrap()
                .count(),
            0
        );
    }

    let rebuilt = fs.begin_materialization(refreshed).unwrap();
    assert_eq!(
        fs::read(rebuilt.path().join("file")).unwrap()[512 * 1024..512 * 1024 + 4096],
        vec![b'b'; 4096]
    );
    assert_eq!(
        fs::read(rebuilt.path().join("file")).unwrap()[768 * 1024..768 * 1024 + 4096],
        vec![b'c'; 4096]
    );
    rebuilt.discard().unwrap();
    drop(fs);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn materialization_refuses_unleased_descriptors_and_escaped_processes() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-sdk-materialization-quiescence-{}-{}",
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
            LayerStackId::from_bytes([0x61; 32]),
            LayerId::from_bytes([0x62; 32]),
            "quiescence",
            root,
        )
        .unwrap();
    let branch = fs
        .create_top_level_branch(BranchId::from_bytes([0x63; 32]), Some("quiescence"), stack)
        .unwrap();

    let descriptor_operation = fs.begin_materialization(branch).unwrap();
    let descriptor = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(descriptor_operation.path().join("held"))
        .unwrap();
    assert!(matches!(
        descriptor_operation.commit(),
        Err(layerfs_sdk::Error::Workspace(
            layerfs_workspace::WorkspaceError::Busy
        ))
    ));
    drop(descriptor);

    let process_operation = fs.begin_materialization(branch).unwrap();
    let process_view = process_operation.path().to_owned();
    let mut escaped = Command::new("/bin/sh")
        .current_dir(&process_view)
        .args(["-c", "exec 3>escaped; sleep 5"])
        .spawn()
        .unwrap();
    for _ in 0..100 {
        if process_view.join("escaped").exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(process_view.join("escaped").exists());
    assert!(matches!(
        process_operation.commit(),
        Err(layerfs_sdk::Error::Workspace(
            layerfs_workspace::WorkspaceError::Busy
        ))
    ));
    escaped.kill().unwrap();
    escaped.wait().unwrap();
    assert_eq!(fs.branch_head(branch.branch_id).unwrap(), Some(branch));
    assert!(fs.recoverable_operations(8).unwrap().is_empty());
    drop(fs);
    fs::remove_dir_all(base).unwrap();
}
