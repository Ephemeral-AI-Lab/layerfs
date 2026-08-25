#![cfg(target_os = "macos")]

use layerfs_sdk::{IntegrityMode, LayerFs, NativeMetadata, NativeRoute, NativeXattrs};
use std::fs;
use std::io::Cursor;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn attribution_arms_reuse_full_source_and_native_routes() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-materialization-attribution-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let opened = LayerFs::open(&base.join("store")).unwrap();
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    let payload = b"shared full-materializer payload";
    fs::write(source.path().join("payload.bin"), payload).unwrap();
    let root = source.capture_quiescent().unwrap();
    let admitted_store_id_queries = opened.fs.counter_snapshot().unwrap().store_id_queries;

    let mut sink = Vec::new();
    let source_only = opened
        .fs
        .materialize_authenticated_to(root, &mut sink)
        .unwrap();
    let (_projected, complete) = opened
        .fs
        .materialize_external_observed(root, &base.join("complete"))
        .unwrap();
    assert_eq!(sink, payload);
    assert_eq!(source_only.rope, complete.rope);
    assert_eq!(source_only.metadata_rope, complete.metadata_rope);
    assert_eq!(source_only.namespace, complete.namespace);
    assert_eq!(source_only.inode_table, complete.inode_table);
    assert_eq!(source_only.scratch_tables, complete.scratch_tables);
    assert_eq!(source_only.scratch_tables, 1);
    assert_eq!(
        source_only.scratch_statements,
        complete.scratch_statements + 1
    );
    assert_eq!(source_only.scratch_rows, complete.scratch_rows);
    assert_eq!(
        source_only.scratch_owner_setup_statements,
        complete.scratch_owner_setup_statements
    );
    assert_eq!(source_only.scratch_owner_setup_statements, 15);
    // Source-only has finished its scratch transaction; the projected
    // workspace keeps its scratch authority live for refreshes.
    assert_eq!(source_only.scratch_derived_setup_statements, 3);
    assert_eq!(complete.scratch_derived_setup_statements, 2);
    assert_eq!(
        source_only.scratch_operation_statements,
        complete.scratch_operation_statements
    );
    assert_eq!(source_only.scratch_store_reopens, 0);
    assert_eq!(source_only.scratch_store_inspection_statements, 0);
    assert_eq!(complete.scratch_store_reopens, 0);
    assert_eq!(complete.scratch_store_inspection_statements, 0);
    assert_eq!(source_only.native, Default::default());
    assert_eq!(source_only.operation_q_terminal_bytes, 0);

    let metadata = NativeMetadata {
        mode: 0o640,
        mtime_seconds: 1_700_000_123,
        mtime_nanoseconds: 456_789_123,
        xattrs: NativeXattrs::new(),
        acl: None,
        bsd_flags: 0,
    };
    let native_root = base.join("native-only");
    let native = opened
        .fs
        .native_durable_output(
            &native_root,
            b"payload.bin",
            &metadata,
            payload.len() as u64,
            Cursor::new(payload),
        )
        .unwrap();
    let native_file = native_root.join("payload.bin");
    assert_eq!(fs::read(&native_file).unwrap(), payload);
    let actual = fs::metadata(&native_file).unwrap();
    assert_eq!(actual.permissions().mode() & 0o777, metadata.mode);
    assert_eq!(actual.mtime(), metadata.mtime_seconds);
    assert_eq!(actual.mtime_nsec() as u32, metadata.mtime_nanoseconds);
    assert_eq!(native.native.route, Some(NativeRoute::NativeDurableOutput));
    assert_eq!(native.native.bytes_written, payload.len() as u64);
    assert_eq!(native.native.temp_calls, 1);
    assert_eq!(native.native.replace_calls, 1);
    assert_eq!(native.native.metadata_calls, 1);
    assert_eq!(native.operation_q_terminal_bytes, 0);
    assert_eq!(native.projection.content_write.bytes, payload.len() as u64);
    assert_eq!(native.projection.temp_create.attempts, 1);
    assert_eq!(native.projection.replace.attempts, 1);
    assert_eq!(
        opened.fs.counter_snapshot().unwrap().store_id_queries,
        admitted_store_id_queries
    );

    drop(source);
    drop(opened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn trusted_materialization_skips_read_hashes_and_verified_reopen_scrubs() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-trusted-materialization-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let store = base.join("store");
    let trusted = LayerFs::open_with_integrity(&store, IntegrityMode::TrustedLocalDev).unwrap();
    let mut source = trusted
        .fs
        .materialize_external(trusted.head, &base.join("source"))
        .unwrap();
    let payload = vec![0x5a; 2 * 1024 * 1024];
    fs::write(source.path().join("payload.bin"), &payload).unwrap();
    let root = source.capture_quiescent().unwrap();
    let written = trusted.fs.counter_snapshot().unwrap();
    assert!(written.new_object_authentication_passes > 0);
    assert_eq!(
        written.new_object_authentication_passes,
        written.created_rows + written.reused_rows
    );
    assert_eq!(written.incumbent_authentication_passes, written.reused_rows);
    drop(source);

    let before = trusted.fs.counter_snapshot().unwrap();
    let (projected, _) = trusted
        .fs
        .materialize_external_observed(root, &base.join("trusted-output"))
        .unwrap();
    let after = trusted.fs.counter_snapshot().unwrap();
    let fetched = after.fetched_rows - before.fetched_rows;
    assert!(fetched > 0);
    assert_eq!(
        after.fetched_row_authentication_passes - before.fetched_row_authentication_passes,
        0
    );
    assert_eq!(
        after.fetched_row_role_decode_passes - before.fetched_row_role_decode_passes,
        fetched
    );
    assert_eq!(
        after.identity_authentication_ns - before.identity_authentication_ns,
        0
    );
    assert_eq!(
        fs::read(projected.path().join("payload.bin")).unwrap(),
        payload
    );
    drop(projected);
    drop(trusted);

    let verified = LayerFs::open(&store).unwrap();
    let scrubbed = verified.fs.counter_snapshot().unwrap();
    assert_eq!(scrubbed.retained_union_scrubs, 1);
    assert!(scrubbed.fetched_rows > 0);
    assert_eq!(
        scrubbed.fetched_rows,
        scrubbed.fetched_row_authentication_passes
    );
    assert_eq!(
        scrubbed.fetched_rows,
        scrubbed.fetched_row_role_decode_passes
    );
    let before = verified.fs.counter_snapshot().unwrap();
    let output = verified
        .fs
        .materialize_external(root, &base.join("verified-output"))
        .unwrap();
    let after = verified.fs.counter_snapshot().unwrap();
    assert!(after.fetched_rows > before.fetched_rows);
    assert_eq!(
        after.fetched_rows - before.fetched_rows,
        after.fetched_row_authentication_passes - before.fetched_row_authentication_passes
    );
    assert_eq!(
        fs::read(output.path().join("payload.bin")).unwrap(),
        payload
    );
    drop(output);
    drop(verified);
    fs::remove_dir_all(base).unwrap();
}
