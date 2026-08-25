#![cfg(target_os = "macos")]

use layerfs_sdk::{IntegrityMode, LayerFs, OPERATION_Q_BOUND_BYTES};
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
    assert_eq!(counters_only.active_connections, 1);
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
    assert_eq!(counters.operation_q_current_bytes, OPERATION_Q_BOUND_BYTES);
    assert_eq!(
        counters.operation_q_high_water_bytes,
        OPERATION_Q_BOUND_BYTES
    );
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
    assert_eq!(counters.operation_q_current_bytes, OPERATION_Q_BOUND_BYTES);
    assert_eq!(
        counters.operation_q_high_water_bytes,
        OPERATION_Q_BOUND_BYTES
    );
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
    assert_eq!(counters.operation_q_current_bytes, OPERATION_Q_BOUND_BYTES);
    assert_eq!(
        counters.operation_q_high_water_bytes,
        OPERATION_Q_BOUND_BYTES
    );
    assert_eq!(counters.operation_q_terminal_bytes, 0);
    let (created, counters) = opened
        .fs
        .replace_file_observed(&replaced, "new", Cursor::new(b"streamed-new"))
        .unwrap();
    assert_eq!(counters.operation_q_current_bytes, OPERATION_Q_BOUND_BYTES);
    assert_eq!(
        counters.operation_q_high_water_bytes,
        OPERATION_Q_BOUND_BYTES
    );
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
    assert!(
        diagnostics.fetched_row_authentication_passes < diagnostics.fetched_rows,
        "Trusted reads skip hashes while publication traversal stays authenticated"
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
    assert_eq!(
        second_counters.operation_q_current_bytes,
        OPERATION_Q_BOUND_BYTES
    );
    assert_eq!(second_counters.operation_q_terminal_bytes, 0);
    assert_eq!(
        after.statements - before.statements,
        second_counters.rope.nodes_read + after.payload_batch_queries
            - before.payload_batch_queries
            + 3 // the trailing diagnostics call's three storage-observation SELECTs
    );
    assert_eq!(
        after.primary_read_statements - before.primary_read_statements,
        after.statements - before.statements
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
    let source_metadata = source.read_metadata("file").unwrap();
    let root = source.capture_quiescent().unwrap();
    let (mut managed, materialize) = opened.fs.materialize_managed_observed(root).unwrap();
    assert_eq!(managed.read_metadata("file").unwrap(), source_metadata);
    assert_eq!(materialize.workspace_materializations, 1);
    assert_eq!(materialize.rematerializations, 0);
    assert!(materialize.native.temp_calls > 0);
    assert!(materialize.native.metadata_calls > 0);
    assert!(materialize.native.replace_calls > 0);
    assert!(materialize.native.sync_calls > 0);
    assert!(materialize.metadata_rope.payload_bytes_read > 0);
    assert_eq!(materialize.content_payload_bytes_read(), Some(8192));
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
        refresh.scratch_high_water_bytes,
        refresh.plan_scratch_high_water_bytes + 33_304
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

#[test]
fn accepted_random_splices_refresh_without_full_fallback() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-random-splice-refresh-{}-{}",
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
    let mut expected = vec![0x51_u8; 256 * 1024];
    fs::write(source.path().join("file"), &expected).unwrap();
    let root = source.capture_quiescent().unwrap();
    let mut state = opened.fs.current_head("main").unwrap();
    let mut managed = opened.fs.materialize_managed(root).unwrap();
    let mut random = 0x4c46_532d_5350_4c43_u64;

    for serial in 0..16_u8 {
        random = random.wrapping_add(0x9e37_79b9_7f4a_7c15).rotate_left(17) ^ 0xbf58_476d_1ce4_e5b9;
        let offset = usize::try_from(random % (expected.len() as u64 + 1)).unwrap();
        let available = expected.len() - offset;
        let delete = usize::try_from((random >> 17) % 2049)
            .unwrap()
            .min(available);
        let mut insert = usize::try_from((random >> 41) % 2049).unwrap();
        if insert == delete {
            insert = (insert + 1) % 2049;
        }
        let replacement = (0..insert)
            .map(|index| serial.wrapping_add(index as u8).wrapping_mul(31))
            .collect::<Vec<_>>();
        let before_len = expected.len() as u64;
        let suffix = before_len - offset as u64 - delete as u64;

        let (accepted, logical) = opened
            .fs
            .replace_range_for_refresh_observed(
                &state,
                "file",
                offset as u64,
                delete as u64,
                Cursor::new(&replacement),
            )
            .unwrap();
        assert_eq!(accepted.before(), &state);
        assert_eq!(accepted.start(), offset as u64);
        assert_eq!(accepted.delete_len(), delete as u64);
        assert_eq!(accepted.insert_len(), insert as u64);
        assert_eq!(logical.rope.payload_bytes_written, insert as u64);
        expected.splice(offset..offset + delete, replacement);

        let refresh = managed.refresh_splice(&accepted).unwrap();
        assert!(matches!(
            refresh.native.route,
            Some(layerfs_sdk::NativeRoute::CloneShift | layerfs_sdk::NativeRoute::InPlaceShift)
        ));
        assert_eq!(refresh.full_fallback_files, 0);
        assert_eq!(refresh.native.suffix_bytes_shifted, suffix);
        assert_eq!(refresh.native.bytes_read, suffix);
        assert_eq!(refresh.native.bytes_written, suffix + insert as u64);
        assert_eq!(refresh.native.patch_bytes, insert as u64);
        let mut physical = Vec::new();
        managed.read_to("file", &mut physical).unwrap();
        assert_eq!(physical, expected);
        state = accepted.after().clone();
    }

    let fallback = opened
        .fs
        .replace_range(&state, "file", 7, 0, Cursor::new(b"unknown-provenance"))
        .unwrap();
    let refresh = managed.refresh(&fallback).unwrap();
    assert_eq!(
        refresh.native.route,
        Some(layerfs_sdk::NativeRoute::FullFallback)
    );
    assert_eq!(refresh.full_fallback_files, 1);

    managed.discard().unwrap();
    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn accepted_eof_append_and_truncate_skip_the_clone_without_skipping_durability() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-eof-splice-refresh-{}-{}",
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
    let mut expected = vec![0x63_u8; 64 * 1024];
    fs::write(source.path().join("file"), &expected).unwrap();
    let root = source.capture_quiescent().unwrap();
    let mut state = opened.fs.current_head("main").unwrap();
    let mut managed = opened.fs.materialize_managed(root).unwrap();

    let appended = vec![0xa7_u8; 16 * 1024];
    let (accepted, _) = opened
        .fs
        .replace_range_for_refresh_observed(
            &state,
            "file",
            expected.len() as u64,
            0,
            Cursor::new(&appended),
        )
        .unwrap();
    let refresh = managed.refresh_splice(&accepted).unwrap();
    assert_eq!(
        refresh.native.route,
        Some(layerfs_sdk::NativeRoute::InPlaceShift)
    );
    assert_eq!(refresh.native.clone_attempts, 0);
    assert_eq!(refresh.native.suffix_bytes_shifted, 0);
    assert_eq!(refresh.native.bytes_read, 0);
    assert_eq!(refresh.native.bytes_written, appended.len() as u64);
    assert_eq!(refresh.native.sync_calls, 1);
    assert_eq!(refresh.full_fallback_files, 0);
    expected.extend_from_slice(&appended);
    state = accepted.after().clone();

    let deleted = 24 * 1024_usize;
    let (accepted, _) = opened
        .fs
        .replace_range_for_refresh_observed(
            &state,
            "file",
            (expected.len() - deleted) as u64,
            deleted as u64,
            Cursor::new([]),
        )
        .unwrap();
    let refresh = managed.refresh_splice(&accepted).unwrap();
    assert_eq!(
        refresh.native.route,
        Some(layerfs_sdk::NativeRoute::InPlaceShift)
    );
    assert_eq!(refresh.native.clone_attempts, 0);
    assert_eq!(refresh.native.suffix_bytes_shifted, 0);
    assert_eq!(refresh.native.bytes_read, 0);
    assert_eq!(refresh.native.bytes_written, 0);
    assert_eq!(refresh.native.sync_calls, 1);
    assert_eq!(refresh.full_fallback_files, 0);
    expected.truncate(expected.len() - deleted);

    let mut physical = Vec::new();
    managed.read_to("file", &mut physical).unwrap();
    assert_eq!(physical, expected);
    assert_eq!(opened.fs.current_head("main").unwrap(), *accepted.after());

    managed.discard().unwrap();
    let mut rebuilt = opened
        .fs
        .materialize_managed(accepted.after().root)
        .unwrap();
    let mut rebuilt_bytes = Vec::new();
    rebuilt.read_to("file", &mut rebuilt_bytes).unwrap();
    assert_eq!(rebuilt_bytes, expected);
    rebuilt.discard().unwrap();
    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn accepted_splice_prioritizes_the_edited_hard_link_alias() {
    use std::os::unix::fs::MetadataExt;

    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-hard-link-splice-refresh-{}-{}",
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
    let mut expected = vec![0x39_u8; 64 * 1024];
    fs::write(source.path().join("a"), &expected).unwrap();
    fs::hard_link(source.path().join("a"), source.path().join("z")).unwrap();
    let root = source.capture_quiescent().unwrap();
    let state = opened.fs.current_head("main").unwrap();
    let mut managed = opened.fs.materialize_managed(root).unwrap();
    let replacement = b"hard-link-size-change";
    let offset = 7_003_usize;
    let deleted = 5_usize;
    let suffix = expected.len() as u64 - offset as u64 - deleted as u64;
    let (accepted, _) = opened
        .fs
        .replace_range_for_refresh_observed(
            &state,
            "z",
            offset as u64,
            deleted as u64,
            Cursor::new(replacement),
        )
        .unwrap();
    expected.splice(offset..offset + deleted, *replacement);

    let refresh = managed.refresh_splice(&accepted).unwrap();
    assert_eq!(
        refresh.native.route,
        Some(layerfs_sdk::NativeRoute::InPlaceShift)
    );
    assert_eq!(refresh.full_fallback_files, 0);
    assert_eq!(refresh.native.suffix_bytes_shifted, suffix);
    assert_eq!(refresh.native.bytes_read, suffix);
    assert_eq!(
        refresh.native.bytes_written,
        suffix + replacement.len() as u64
    );
    let mut external = managed.into_external().unwrap();
    assert_eq!(fs::read(external.path().join("a")).unwrap(), expected);
    assert_eq!(fs::read(external.path().join("z")).unwrap(), expected);
    let a = fs::metadata(external.path().join("a")).unwrap();
    let z = fs::metadata(external.path().join("z")).unwrap();
    assert_eq!(a.ino(), z.ino());
    assert_eq!(a.nlink(), 2);
    external.discard().unwrap();

    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}
