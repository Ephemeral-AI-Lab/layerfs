use super::super::artifact::unix_ns;
use super::super::limits::FILE_PATH;
use super::super::oracle::{verify_storage_transition, verify_supported_metadata};
use super::super::resources::residue_count;
use super::super::row_milestone::{terminal_work_residue_count, verify_single_file_destination};
use super::super::source_identity::rust_cargo_source_paths;
use super::synthetic::synthetic_metadata;
use crate::legacy_full::Diagnostics;
use std::fs;

#[test]
fn residue_and_storage_regressions_fail_before_cleanup_or_null_coercion() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-stage1.1-residue-contract-{}-{}",
        std::process::id(),
        unix_ns().unwrap()
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("generation.sqlite-wal"), b"wal").unwrap();
    fs::write(root.join("CURRENT.tmp"), b"selector").unwrap();
    fs::create_dir(root.join(".layerfs-owned-temp")).unwrap();
    assert_eq!(residue_count(&root).unwrap(), 3);
    let work = root.join("work");
    fs::create_dir(&work).unwrap();
    fs::create_dir(work.join("store")).unwrap();
    fs::create_dir(work.join("milestone-R34")).unwrap();
    assert_eq!(terminal_work_residue_count(&work).unwrap(), 1);
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(residue_count(&root).unwrap(), 0);
    let before = Diagnostics {
        database_bytes: Some(100),
        logical_engine_bytes: Some(90),
        object_bytes_written: 80,
        ..Diagnostics::default()
    };
    let unavailable = Diagnostics {
        database_bytes: None,
        logical_engine_bytes: Some(90),
        object_bytes_written: 80,
        ..Diagnostics::default()
    };
    let regressed = Diagnostics {
        database_bytes: Some(99),
        logical_engine_bytes: Some(89),
        object_bytes_written: 79,
        ..Diagnostics::default()
    };
    assert!(verify_storage_transition(&before, &unavailable).is_err());
    assert!(verify_storage_transition(&before, &regressed).is_err());
}
#[test]
fn live_and_fresh_single_file_inventory_and_metadata_are_independently_gated() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-stage1.1-inventory-{}-{}",
        std::process::id(),
        unix_ns().unwrap()
    ));
    fs::create_dir_all(root.join("data")).unwrap();
    fs::write(root.join(FILE_PATH), b"payload").unwrap();
    verify_single_file_destination(&root).unwrap();
    fs::write(root.join("extra"), b"unexpected").unwrap();
    assert!(verify_single_file_destination(&root).is_err());
    fs::remove_dir_all(root).unwrap();
    let mut metadata = synthetic_metadata(34);
    verify_supported_metadata(&metadata, "synthetic R34").unwrap();
    metadata.mode = 0o600;
    assert!(verify_supported_metadata(&metadata, "synthetic R34").is_err());
    metadata = synthetic_metadata(34);
    metadata.xattrs.push(b"user.test", b"value").unwrap();
    assert!(verify_supported_metadata(&metadata, "synthetic R34").is_err());
}
#[test]
fn source_custody_includes_every_workspace_manifest() {
    let paths = rust_cargo_source_paths().unwrap();
    for manifest in [
        "Cargo.toml",
        "Cargo.lock",
        "crates/layerfs-core/Cargo.toml",
        "crates/layerfs-storage/Cargo.toml",
        "crates/layerfs-working-store/Cargo.toml",
        "crates/layerfs-durable-store/Cargo.toml",
        "crates/layerfs-sync/Cargo.toml",
        "crates/layerfs-workspace/Cargo.toml",
        "crates/layerfs-mount/Cargo.toml",
        "crates/layerfs-materialization/Cargo.toml",
        "crates/layerfs-sdk/Cargo.toml",
        "crates/layerfs-service/Cargo.toml",
        "tools/layerfs-eval/Cargo.toml",
    ] {
        assert!(paths.iter().any(|path| path == manifest), "{manifest}");
    }
}
