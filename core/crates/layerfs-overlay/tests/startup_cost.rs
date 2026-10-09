//! Real finite startup, definite refusal and incomplete-schema receipts.
use layerfs_overlay::*;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-startup-cost-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn startup_counts_profile_schema_accounting_and_entire_reservation() {
    let temp = Temp::new();
    let path = temp.0.join("overlay.sqlite");
    let created = Overlay::create_observed(&path, ProfileConfig::default());
    let db = created.result.unwrap();
    let work = created.work;
    assert_eq!(work.file_create_calls, 1);
    assert_filesystem_probe(&work, &path);
    assert_eq!(work.sqlite_open_calls, 1);
    assert_eq!(work.connection_configuration_calls, 2);
    assert_eq!(work.cache_configuration_calls, 1);
    assert_eq!(work.sql, db.diagnostics());
    assert!(work.sql.total().attempts > 40);
    assert!(work.sql.total().vm_steps > 100);
    assert_eq!(
        work.sql.total().attempts,
        work.sql.total().statement_memory_samples
    );
    assert!(work.sql.total().statement_memory_sample_bytes > 0);
    assert!(work.sql.total().sql_bytes > 1000);
    assert_eq!(work.allocation.attempts, 1);
    assert_eq!(
        work.allocation.requested_bytes,
        MUTATION_GROWTH + CLEANUP_HEADROOM
    );
    let physical = work.allocation_state.as_ref().unwrap().as_ref().unwrap();
    assert!(physical.high_water_allocated_bytes >= MUTATION_GROWTH + CLEANUP_HEADROOM);
    assert_eq!(physical.work, work.allocation);
    assert_eq!(db.profile().schema_version, 23);
    println!("S7_STARTUP_SUCCESS {work:?}");
}

#[test]
fn existing_artifact_refusal_has_no_sql_or_allocation_replay() {
    let temp = Temp::new();
    let path = temp.0.join("overlay.sqlite");
    std::fs::write(&path, b"owned existing artifact").unwrap();
    let created = Overlay::create_observed(&path, ProfileConfig::default());
    assert!(matches!(created.result, Err(OverlayError::Io(_))));
    assert_eq!(created.work.file_create_calls, 1);
    assert_eq!(created.work.filesystem_open_calls, 0);
    assert_eq!(created.work.filesystem_identity_calls, 0);
    assert_eq!(created.work.filesystem_probe_calls, 0);
    assert!(created.work.linux_filesystem_type.is_none());
    assert_eq!(created.work.sqlite_open_calls, 0);
    assert_eq!(created.work.sql.total().attempts, 0);
    assert_eq!(created.work.allocation.attempts, 0);
    assert!(created.work.allocation_state.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), b"owned existing artifact");
    println!("S7_STARTUP_REFUSAL {:?}", created.work);
}

#[test]
fn real_schema_full_failure_retains_original_error_work_and_artifact() {
    let temp = Temp::new();
    let path = temp.0.join("overlay.sqlite");
    let created = Overlay::create_observed(
        &path,
        ProfileConfig {
            max_pages: Some(32),
            ..ProfileConfig::default()
        },
    );
    assert!(matches!(&created.result, Err(OverlayError::Sql(_))));
    assert!(created.work.sql.total().attempts > 20);
    assert!(created.work.sql.total().vm_steps > 0);
    assert_eq!(created.work.file_create_calls, 1);
    assert_filesystem_probe(&created.work, &path);
    assert_eq!(created.work.sqlite_open_calls, 1);
    assert_eq!(created.work.cache_configuration_calls, 0);
    assert!(path.exists());
    println!(
        "S7_STARTUP_FAILURE error={:?} work={:?}",
        created.result.err(),
        created.work
    );
}

fn assert_filesystem_probe(work: &CreationWork, path: &Path) {
    #[cfg(target_os = "linux")]
    {
        let file = std::fs::File::open(path).unwrap();
        let observed = nix::sys::statfs::fstatfs(&file).unwrap();
        assert_eq!(work.filesystem_open_calls, 1);
        assert_eq!(work.filesystem_identity_calls, 2);
        assert_eq!(work.filesystem_probe_calls, 1);
        assert_eq!(
            i128::from(work.linux_filesystem_type.unwrap()),
            i128::from(observed.filesystem_type().0)
        );
        assert_ne!(work.linux_filesystem_type, Some(0x6a656a63));
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        assert_eq!(work.filesystem_open_calls, 0);
        assert_eq!(work.filesystem_identity_calls, 0);
        assert_eq!(work.filesystem_probe_calls, 0);
        assert!(work.linux_filesystem_type.is_none());
    }
}
