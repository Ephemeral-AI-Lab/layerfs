use layerfs_daemon::{Owner, OwnerConfig, OwnerError};
use layerfs_overlay::{OverlayError, ProfileConfig};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-owner-startup-{}-{}",
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
fn readiness_retains_startup_separately_from_foreground_and_maintenance() {
    let temp = Temp::new();
    let started = Owner::start_observed(
        &temp.0.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    );
    let owner = started.result.unwrap();
    assert!(started.startup.sql.total().attempts > 40);
    assert_eq!(started.startup.sql, owner.startup_work().sql);
    let queued = owner.client().diagnostics().unwrap();
    assert_eq!(queued.sql_foreground.total().attempts, 0);
    assert_eq!(queued.sql_maintenance.total().attempts, 0);
    println!(
        "S7_DAEMON_READINESS startup={:?} wall_ns={}",
        started.startup, started.elapsed_ns
    );
    owner.stop().unwrap();
}
#[test]
fn failed_owner_start_exposes_original_creation_receipt_without_retry() {
    let temp = Temp::new();
    let path = temp.0.join("overlay.sqlite");
    let started = Owner::start_observed(
        &path,
        ProfileConfig {
            max_pages: Some(32),
            ..ProfileConfig::default()
        },
        OwnerConfig::default(),
    );
    assert!(matches!(
        &started.result,
        Err(OwnerError::Overlay(OverlayError::Sql(_)))
    ));
    assert!(started.startup.sql.total().attempts > 20);
    assert_eq!(started.startup.file_create_calls, 1);
    assert!(path.exists());
    println!(
        "S7_DAEMON_STARTUP_FAILURE error={:?} work={:?}",
        started.result.err(),
        started.startup
    );
}
