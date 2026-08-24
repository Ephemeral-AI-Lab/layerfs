#![cfg(target_os = "macos")]

use layerfs_sdk::{IntegrityMode, LayerFs};
use std::fs;
use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn public_direct_routes_stream_and_preserve_history() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-stage1-routes-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let opened =
        LayerFs::open_with_integrity(&base.join("store"), IntegrityMode::TrustedLocalDev).unwrap();
    let counters_only = opened.fs.counter_snapshot().unwrap();
    assert_eq!(counters_only.database_bytes, None);
    assert_eq!(counters_only.logical_engine_bytes, None);
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("file"), b"0123456789").unwrap();
    let root = source.capture_quiescent().unwrap();
    let original = opened.fs.current_head("main").unwrap();
    assert_eq!(original.root, root);

    let mut range = Vec::new();
    let counters = opened
        .fs
        .read_range(root, "file", 3..7, &mut range)
        .unwrap();
    assert_eq!(range, b"3456");
    assert_eq!(counters.native.bytes_read, 0);
    assert_eq!(counters.native.bytes_written, 0);
    assert_eq!(counters.operation_q_current_bytes, 4 * 1024 * 1024);
    assert_eq!(counters.operation_q_high_water_bytes, 4 * 1024 * 1024);
    assert_eq!(counters.operation_q_terminal_bytes, 0);

    let (edited, counters) = opened
        .fs
        .replace_range_observed(&original, "file", 4, 2, Cursor::new(b"ABCD"))
        .unwrap();
    assert_eq!(edited.generation, original.generation + 1);
    assert!(counters.rope.cdc_bytes_scanned <= 4);
    assert_eq!(counters.rope.payload_bytes_read, 0);
    assert_eq!(counters.native.bytes_read, 0);
    assert_eq!(counters.native.bytes_written, 0);
    assert_eq!(counters.operation_q_current_bytes, 4 * 1024 * 1024);
    assert_eq!(counters.operation_q_high_water_bytes, 4 * 1024 * 1024);
    assert_eq!(counters.operation_q_terminal_bytes, 0);

    let mut old = Vec::new();
    opened.fs.read_to(root, "file", &mut old).unwrap();
    assert_eq!(old, b"0123456789");
    let mut current = Vec::new();
    opened
        .fs
        .read_to(edited.root, "file", &mut current)
        .unwrap();
    assert_eq!(current, b"0123ABCD6789");

    let (replaced, counters) = opened
        .fs
        .replace_file_observed(&edited, "file", Cursor::new(b"replacement"))
        .unwrap();
    assert_eq!(counters.operation_q_current_bytes, 4 * 1024 * 1024);
    assert_eq!(counters.operation_q_high_water_bytes, 4 * 1024 * 1024);
    assert_eq!(counters.operation_q_terminal_bytes, 0);
    let (created, counters) = opened
        .fs
        .replace_file_observed(&replaced, "new", Cursor::new(b"streamed-new"))
        .unwrap();
    assert_eq!(counters.operation_q_current_bytes, 4 * 1024 * 1024);
    assert_eq!(counters.operation_q_high_water_bytes, 4 * 1024 * 1024);
    assert_eq!(counters.operation_q_terminal_bytes, 0);
    let mut bytes = Vec::new();
    opened.fs.read_to(created.root, "new", &mut bytes).unwrap();
    assert_eq!(bytes, b"streamed-new");
    assert_eq!(opened.fs.current_head("main").unwrap(), created);
    assert!(opened
        .fs
        .replace_range(&original, "file", 0, 0, Cursor::new(b"stale"))
        .is_err());
    let diagnostics = opened.fs.diagnostics().unwrap();
    assert_eq!(
        diagnostics.fetched_rows,
        diagnostics.fetched_row_authentication_passes
    );
    assert_eq!(
        diagnostics.fetched_rows,
        diagnostics.fetched_row_role_decode_passes
    );

    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn repeated_ranges_reuse_only_the_exact_resolved_root_and_path() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-stage1-resolved-read-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let store = base.join("store");
    let opened = LayerFs::open_with_integrity(&store, IntegrityMode::TrustedLocalDev).unwrap();
    let bytes = (0..256 * 1024)
        .map(|index| (index as u8).wrapping_mul(31))
        .collect::<Vec<_>>();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("file"), &bytes).unwrap();
    fs::write(source.path().join("other"), b"other").unwrap();
    let root = source.capture_quiescent().unwrap();
    drop(source);

    let mut first = Vec::new();
    let first_counters = opened
        .fs
        .read_range(root, "file", 0..64 * 1024, &mut first)
        .unwrap();
    assert_eq!(first, bytes[..64 * 1024]);
    assert!(first_counters.namespace.nodes_read > 0);
    assert!(first_counters.inode_table.nodes_read > 0);

    let before = opened.fs.diagnostics().unwrap();
    let mut second = Vec::new();
    let second_counters = opened
        .fs
        .read_range(root, "file", 64 * 1024..128 * 1024, &mut second)
        .unwrap();
    let after = opened.fs.diagnostics().unwrap();
    assert_eq!(second, bytes[64 * 1024..128 * 1024]);
    assert_eq!(second_counters.namespace.nodes_read, 0);
    assert_eq!(second_counters.inode_table.nodes_read, 0);
    assert_eq!(second_counters.operation_q_current_bytes, 4 * 1024 * 1024);
    assert_eq!(second_counters.operation_q_terminal_bytes, 0);
    assert_eq!(
        after.statements - before.statements,
        second_counters.rope.nodes_read + after.payload_batch_queries
            - before.payload_batch_queries
    );
    assert_eq!(
        after.fetched_rows - before.fetched_rows,
        second_counters.rope.nodes_read + after.payload_batch_references
            - before.payload_batch_references
    );

    let other = opened
        .fs
        .read_range(root, "other", 0..5, Vec::new())
        .unwrap();
    assert!(other.namespace.nodes_read > 0);
    assert!(other.inode_table.nodes_read > 0);
    drop(opened);

    let reopened = LayerFs::open_with_integrity(&store, IntegrityMode::TrustedLocalDev).unwrap();
    let miss = reopened
        .fs
        .read_range(root, "file", 0..1, Vec::new())
        .unwrap();
    assert!(miss.namespace.nodes_read > 0);
    assert!(miss.inode_table.nodes_read > 0);
    drop(reopened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn managed_workspace_rotates_one_hundred_checkpoints_without_rematerializing() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-stage1-checkpoints-{}-{}",
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
    fs::write(source.path().join("file"), vec![0_u8; 8192]).unwrap();
    let root = source.capture_quiescent().unwrap();
    let (mut managed, materialize) = opened.fs.materialize_managed_observed(root).unwrap();
    assert_eq!(materialize.workspace_materializations, 1);
    assert_eq!(materialize.rematerializations, 0);
    assert!(materialize.native.temp_calls > 0);
    assert!(materialize.native.metadata_calls > 0);
    assert!(materialize.native.replace_calls > 0);
    assert!(materialize.native.sync_calls > 0);
    let before = opened.fs.diagnostics().unwrap();
    let mut state = opened.fs.current_head("main").unwrap();
    let selected_old = state.clone();
    for serial in 0..100_u32 {
        managed
            .replace("file", 4096, 4, &serial.to_be_bytes())
            .unwrap();
        let (next, counters) = managed.checkpoint_observed().unwrap();
        assert_eq!(next.generation, state.generation + 1);
        assert_eq!(counters.workspace_reuses, 1);
        assert_eq!(counters.rematerializations, 0);
        assert_eq!(counters.descriptor_resets, 1);
        state = next;
    }
    let before_no_op = opened.fs.diagnostics().unwrap();
    let no_op = managed.ensure_exact(&state).unwrap();
    let after_no_op = opened.fs.diagnostics().unwrap();
    assert_eq!(
        no_op.native.route,
        Some(layerfs_sdk::NativeRoute::ExactNoop)
    );
    assert_eq!(no_op.rope.payload_bytes_read, 0);
    assert_eq!(no_op.rope.payload_bytes_written, 0);
    assert_eq!(no_op.rope.cdc_bytes_scanned, 0);
    assert_eq!(no_op.native.bytes_read, 0);
    assert_eq!(no_op.native.bytes_written, 0);
    assert_eq!(no_op.descriptor_resets, 0);
    assert_eq!(no_op.rematerializations, 0);
    assert_eq!(
        after_no_op.transactions_started,
        before_no_op.transactions_started
    );
    assert_eq!(
        after_no_op.transactions_committed,
        before_no_op.transactions_committed
    );
    assert_eq!(
        after_no_op.publication_commits,
        before_no_op.publication_commits
    );
    let after = opened.fs.diagnostics().unwrap();
    assert_eq!(
        after.transactions_committed - before.transactions_committed,
        100
    );
    assert_eq!(after.publication_commits - before.publication_commits, 100);

    let mut historical = Vec::new();
    opened
        .fs
        .read_range(selected_old.root, "file", 4096..4100, &mut historical)
        .unwrap();
    assert_eq!(historical, [0; 4]);
    let mut terminal = Vec::new();
    opened
        .fs
        .read_range(state.root, "file", 4096..4100, &mut terminal)
        .unwrap();
    assert_eq!(terminal, 99_u32.to_be_bytes());
    managed.discard().unwrap();

    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn managed_refresh_applies_only_the_changed_canonical_range() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-stage1-refresh-{}-{}",
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
    fs::write(source.path().join("file"), vec![0x51_u8; 1024 * 1024]).unwrap();
    let root_a = source.capture_quiescent().unwrap();
    let state_a = opened.fs.current_head("main").unwrap();
    let mut managed = opened.fs.materialize_managed(root_a).unwrap();
    let replacement = [0xa5_u8; 4096];
    let state_b = opened
        .fs
        .replace_range(
            &state_a,
            "file",
            512 * 1024 - 2048,
            4096,
            Cursor::new(replacement),
        )
        .unwrap();
    let before = opened.fs.diagnostics().unwrap();
    let refresh = managed.refresh(&state_b).unwrap();
    let after = opened.fs.diagnostics().unwrap();
    assert_eq!(
        refresh.native.route,
        Some(layerfs_sdk::NativeRoute::ClonePatch)
    );
    assert_eq!(refresh.native.patch_bytes, 4096);
    assert_eq!(refresh.native.suffix_bytes_shifted, 0);
    assert_eq!(refresh.rope.cdc_bytes_scanned, 0);
    assert_eq!(refresh.full_fallback_files, 0);
    assert_eq!(refresh.rematerializations, 0);
    assert_eq!(refresh.scratch_tables, 1);
    assert!(refresh.scratch_rows > 0);
    assert!(refresh.scratch_high_water_bytes > 0);
    assert_eq!(
        refresh.plan_scratch_high_water_bytes,
        refresh.scratch_high_water_bytes
    );
    assert_eq!(after.transactions_started, before.transactions_started);
    assert_eq!(after.publication_commits, before.publication_commits);
    let no_op = managed.ensure_exact(&state_b).unwrap();
    assert_eq!(
        no_op.native.route,
        Some(layerfs_sdk::NativeRoute::ExactNoop)
    );

    let external = managed.into_external().unwrap();
    let bytes = fs::read(external.path().join("file")).unwrap();
    assert_eq!(&bytes[512 * 1024 - 2048..512 * 1024 + 2048], &replacement);
    drop(external);
    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}
