#![cfg(target_os = "macos")]

use layerfs_sdk::{IntegrityMode, LayerFs};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn external_capture_reuses_unchanged_file_roots_and_cdc_scans_only_changes() {
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-external-identity-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let opened =
        LayerFs::open_with_integrity(&base.join("store"), IntegrityMode::TrustedLocalDev).unwrap();
    let bytes = vec![0x51_u8; 128 * 1024];
    let mut source = opened
        .fs
        .materialize_external(opened.head, &base.join("source"))
        .unwrap();
    fs::write(source.path().join("file"), &bytes).unwrap();
    let root = source.capture_quiescent().unwrap();
    let before_unchanged = opened.fs.current_head("main").unwrap();

    let mut unchanged = opened
        .fs
        .materialize_external(root, &base.join("unchanged"))
        .unwrap();
    let (same_root, same) = unchanged.capture_quiescent_observed().unwrap();
    assert_eq!(same_root, root);
    assert_eq!(same.current_digest_bytes, bytes.len() as u64);
    assert_eq!(same.uncached_prior_digest_bytes, bytes.len() as u64);
    assert_eq!(same.changed_current_cdc_bytes, 0);
    assert_eq!(same.unchanged_file_roots_reused, 1);
    assert_eq!(same.native.bytes_read, bytes.len() as u64);
    assert_eq!(same.authority_full_scans, 1);
    assert_eq!(same.scratch_tables, 4);
    assert!(same.scratch_statements > 0);
    assert!(same.scratch_rows > 0);
    assert!(same.scratch_high_water_bytes > 0);
    assert!(same.operation_q_current_bytes > 0);
    assert_eq!(same.operation_q_terminal_bytes, 0);
    assert_eq!(opened.fs.current_head("main").unwrap(), before_unchanged);

    let mut cached = opened
        .fs
        .materialize_external(root, &base.join("cached"))
        .unwrap();
    let (_, cache_hit) = cached.capture_quiescent_observed().unwrap();
    assert_eq!(cache_hit.current_digest_bytes, bytes.len() as u64);
    assert_eq!(cache_hit.uncached_prior_digest_bytes, 0);
    assert_eq!(cache_hit.changed_current_cdc_bytes, 0);
    assert_eq!(cache_hit.unchanged_file_roots_reused, 1);

    let mut changed = opened
        .fs
        .materialize_external(root, &base.join("changed"))
        .unwrap();
    let mut changed_bytes = bytes;
    changed_bytes[64 * 1024] ^= 0xff;
    fs::write(changed.path().join("file"), &changed_bytes).unwrap();
    let (changed_root, changed_counters) = changed.capture_quiescent_observed().unwrap();
    assert_ne!(changed_root, root);
    assert_eq!(
        changed_counters.current_digest_bytes,
        changed_bytes.len() as u64
    );
    assert_eq!(changed_counters.uncached_prior_digest_bytes, 0);
    assert_eq!(
        changed_counters.changed_current_cdc_bytes,
        changed_bytes.len() as u64
    );
    assert_eq!(changed_counters.unchanged_file_roots_reused, 0);
    assert_eq!(
        changed_counters.native.bytes_read,
        2 * changed_bytes.len() as u64
    );
    let after_changed = opened.fs.current_head("main").unwrap();
    assert_eq!(after_changed.root, changed_root);
    assert_eq!(after_changed.generation, before_unchanged.generation + 1);

    drop(changed);
    drop(cached);
    drop(unchanged);
    drop(source);
    drop(opened);

    let reopened_store =
        LayerFs::open_with_integrity(&base.join("store"), IntegrityMode::TrustedLocalDev).unwrap();
    let mut cache_miss = reopened_store
        .fs
        .materialize_external(changed_root, &base.join("reopen-cache-miss"))
        .unwrap();
    let (_, reopened_counters) = cache_miss.capture_quiescent_observed().unwrap();
    assert_eq!(
        reopened_counters.current_digest_bytes,
        changed_bytes.len() as u64
    );
    assert_eq!(
        reopened_counters.uncached_prior_digest_bytes,
        changed_bytes.len() as u64
    );
    assert_eq!(reopened_counters.changed_current_cdc_bytes, 0);
    drop(cache_miss);
    drop(reopened_store);

    let linked =
        LayerFs::open_with_integrity(&base.join("linked-store"), IntegrityMode::TrustedLocalDev)
            .unwrap();
    let external_path = base.join("caller-owned");
    fs::create_dir(&external_path).unwrap();
    fs::write(external_path.join("z-old"), b"linked").unwrap();
    let mut initial = linked.fs.open_external(&external_path).unwrap();
    initial.capture_quiescent().unwrap();
    drop(initial);
    fs::hard_link(external_path.join("z-old"), external_path.join("a-new")).unwrap();
    let mut reopened = linked.fs.open_external(&external_path).unwrap();
    let (_, linked_counters) = reopened.capture_quiescent_observed().unwrap();
    assert_eq!(linked_counters.current_digest_bytes, 6);
    assert_eq!(linked_counters.uncached_prior_digest_bytes, 6);
    assert_eq!(linked_counters.changed_current_cdc_bytes, 0);
    assert_eq!(linked_counters.unchanged_file_roots_reused, 1);
    drop(reopened);
    drop(linked);

    fs::remove_dir_all(base).unwrap();
}
