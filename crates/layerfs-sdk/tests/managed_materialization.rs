#![cfg(target_os = "macos")]

use layerfs_sdk::{BranchId, CommitResult, IntegrityMode, LayerFs, LayerId, LayerStackId};
use std::fs;
use std::io::Cursor;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
    let native = managed.managed_replace_range("file", 2, 2, b"XY").unwrap();
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
