#![cfg(target_os = "macos")]

use layerfs_sdk::{IntegrityMode, LayerFs, VfsError};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn checkpoint_syncs_the_final_token_after_mixed_same_path_edits() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-managed-mixed-edits-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let opened =
        LayerFs::open_with_integrity(&base.join("store"), IntegrityMode::TrustedLocalDev).unwrap();
    let mut expected = vec![0x51_u8; 16 * 1024];
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::create_dir(source.path().join("nested")).unwrap();
    fs::write(source.path().join("file"), &expected).unwrap();
    let root = source.capture_quiescent().unwrap();

    let mut managed = opened.fs.materialize_managed(root).unwrap();
    let edit = managed.replace_observed("file", 0, 4, b"EDIT").unwrap();
    assert!(edit.operation_q_current_bytes > 0);
    assert!(edit.operation_q_high_water_bytes >= edit.operation_q_current_bytes);
    assert_eq!(edit.operation_q_terminal_bytes, 0);
    assert_eq!(edit.owned_temp_current, 1);
    assert!(edit.descriptor_spool_bytes_current >= 4);
    assert_eq!(edit.scratch_tables, 0);
    assert_eq!(edit.scratch_owner_setup_statements, 0);
    assert_eq!(edit.scratch_derived_setup_statements, 0);
    assert_eq!(edit.scratch_operation_statements, 3);
    assert_eq!(edit.scratch_statements, 3);
    assert_eq!(edit.scratch_rows, 3);
    assert!(edit.scratch_high_water_bytes > 0);
    expected.splice(0..4, *b"EDIT");
    managed.replace("file", 100, 0, b"INS").unwrap();
    expected.splice(100..100, *b"INS");
    managed.replace("file", 200, 2, b"").unwrap();
    expected.splice(200..202, []);
    let renamed = managed.rename_observed("file", "intermediate").unwrap();
    assert_eq!(renamed.scratch_tables, 0);
    assert_eq!(renamed.scratch_owner_setup_statements, 0);
    assert_eq!(renamed.scratch_derived_setup_statements, 0);
    assert_eq!(renamed.scratch_operation_statements, 3);
    assert_eq!(renamed.scratch_statements, 3);
    assert_eq!(renamed.scratch_rows, 3);
    assert!(renamed.scratch_high_water_bytes > 0);
    managed.rename("intermediate", "nested/moved").unwrap();

    let (next, counters) = managed.checkpoint_observed().unwrap();
    assert_eq!(counters.descriptor_resets, 1);
    assert_eq!(counters.operation_q_terminal_bytes, 0);
    assert_eq!(counters.owned_temp_current, 1);
    assert_eq!(counters.owned_temp_terminal, 1);
    assert_eq!(counters.descriptor_spool_bytes_current, 0);
    assert_eq!(counters.descriptor_spool_bytes_terminal, 0);
    assert!(counters.metadata_rope.cdc_bytes_scanned > 0);
    assert_eq!(
        counters.rope.cdc_bytes_scanned - counters.metadata_rope.cdc_bytes_scanned,
        7
    );
    assert_eq!(
        counters.rope.payload_bytes_written - counters.metadata_rope.payload_bytes_written,
        7
    );
    let mut actual = Vec::new();
    opened
        .fs
        .read_to(next.root, "nested/moved", &mut actual)
        .unwrap();
    assert_eq!(actual, expected);

    let target = opened
        .fs
        .replace_range(&next, "nested/moved", 10, 1, std::io::Cursor::new([0xa5]))
        .unwrap();
    let refreshed_counters = managed.refresh(&target).unwrap();
    assert_eq!(refreshed_counters.scratch_tables, 1);
    assert_eq!(
        refreshed_counters.scratch_statements,
        refreshed_counters.scratch_owner_setup_statements
            + refreshed_counters.scratch_derived_setup_statements
            + refreshed_counters.scratch_operation_statements
    );
    assert!(refreshed_counters.scratch_operation_statements > 0);
    let equal_refresh = managed.refresh(&target).unwrap();
    assert_eq!(equal_refresh.scratch_tables, 0);
    assert_eq!(equal_refresh.scratch_statements, 0);
    assert_eq!(equal_refresh.scratch_rows, 0);
    expected[10] = 0xa5;
    let mut refreshed = Vec::new();
    managed.read_to("nested/moved", &mut refreshed).unwrap();
    assert_eq!(refreshed, expected);

    let discarded = managed.discard_observed().unwrap();
    assert_eq!(discarded.operation_q_terminal_bytes, 0);
    assert_eq!(discarded.owned_temp_current, 0);
    assert_eq!(discarded.owned_temp_terminal, 0);
    assert_eq!(discarded.descriptor_spool_bytes_current, 0);
    assert_eq!(discarded.descriptor_spool_bytes_terminal, 0);
    assert_eq!(discarded.scratch_tables, 0);
    assert_eq!(discarded.scratch_derived_setup_statements, 1);
    assert_eq!(
        discarded.scratch_statements,
        discarded.scratch_owner_setup_statements
            + discarded.scratch_derived_setup_statements
            + discarded.scratch_operation_statements
    );

    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn live_noop_revalidates_the_native_root_binding_in_constant_work() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-managed-root-binding-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let opened =
        LayerFs::open_with_integrity(&base.join("store"), IntegrityMode::TrustedLocalDev).unwrap();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("file"), b"root binding").unwrap();
    let root = source.capture_quiescent().unwrap();
    let state = opened.fs.current_head("main").unwrap();
    let mut managed = opened.fs.materialize_managed(root).unwrap();
    let managed_path = fs::read_dir(base.join("store"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".layerfs-managed-"))
        })
        .unwrap();
    let displaced = base.join("displaced-managed-root");
    fs::rename(&managed_path, &displaced).unwrap();
    fs::create_dir(&managed_path).unwrap();

    assert!(matches!(
        managed.ensure_exact(&state),
        Err(VfsError::ExternalDirtyConflict)
    ));
    assert!(matches!(
        managed.checkpoint(),
        Err(VfsError::ExternalDirtyConflict)
    ));

    drop(managed);
    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn equal_root_refresh_revalidates_the_native_root_binding() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-managed-refresh-root-binding-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let opened =
        LayerFs::open_with_integrity(&base.join("store"), IntegrityMode::TrustedLocalDev).unwrap();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("file"), b"root binding").unwrap();
    let root = source.capture_quiescent().unwrap();
    let state = opened.fs.current_head("main").unwrap();
    let mut managed = opened.fs.materialize_managed(root).unwrap();
    let managed_path = fs::read_dir(base.join("store"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".layerfs-managed-"))
        })
        .unwrap();
    let displaced = base.join("displaced-managed-refresh-root");
    fs::rename(&managed_path, &displaced).unwrap();
    fs::create_dir(&managed_path).unwrap();

    assert!(matches!(
        managed.refresh(&state),
        Err(VfsError::ExternalDirtyConflict)
    ));

    drop(managed);
    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn rename_collision_is_refused_before_visibility_and_preserves_authority() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-managed-rename-collision-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let opened =
        LayerFs::open_with_integrity(&base.join("store"), IntegrityMode::TrustedLocalDev).unwrap();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("source"), b"source-bytes").unwrap();
    fs::write(source.path().join("destination"), b"destination-bytes").unwrap();
    let root = source.capture_quiescent().unwrap();
    let state = opened.fs.current_head("main").unwrap();
    let mut managed = opened.fs.materialize_managed(root).unwrap();

    assert!(matches!(
        managed.rename("source", "destination"),
        Err(VfsError::InvalidState)
    ));
    let mut source_bytes = Vec::new();
    managed.read_to("source", &mut source_bytes).unwrap();
    assert_eq!(source_bytes, b"source-bytes");
    let mut destination_bytes = Vec::new();
    managed
        .read_to("destination", &mut destination_bytes)
        .unwrap();
    assert_eq!(destination_bytes, b"destination-bytes");
    assert_eq!(managed.checkpoint().unwrap(), state);
    managed.discard().unwrap();

    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}
