//! The unmount guard's record file across a forced attempt (layerfs-project#136):
//! an attempt that changed nothing must leave the earlier record as it was.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_sandbox::unmount_guard::{begin_at, cancel, finish, forceable};
use layerfs_store::project::discovery;
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

/// A Sandbox folder with its `workspace/` subfolder, both private.
fn fixture(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-unmount-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let workspace = dir.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    for path in [&dir, &workspace] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    dir
}

fn record_file(dir: &Path) -> PathBuf {
    dir.join("workspace").join("01.unmount")
}

/// A completed record a force may replace.
fn completed() -> SdkValue {
    SdkValue::map([
        ("state", SdkValue::text("Retained")),
        ("startedMs", SdkValue::Int(7)),
        ("path", SdkValue::text("/w")),
        ("forced", SdkValue::Bool(false)),
        ("code", SdkValue::text("Failed")),
        ("message", SdkValue::text("detach not established")),
        ("detail", SdkValue::Null),
    ])
}

fn refused(code: &str) -> SdkRefusal {
    SdkRefusal {
        code: code.into(),
        message: code.into(),
        detail: SdkValue::Null,
    }
}

/// The earlier record comes back exactly, and still answers as it did.
fn assert_restored(file: &Path, prior: &SdkValue) {
    let kept = discovery::load(file).unwrap();
    assert_eq!(kept.get_text("state"), prior.get_text("state"));
    assert_eq!(kept.get_text("code"), prior.get_text("code"));
    assert_eq!(kept.get("forced"), prior.get("forced"));
    assert_eq!(kept.get_int("startedMs"), prior.get_int("startedMs"));
    assert!(forceable(&kept), "the earlier record answers as it did");
}

#[test]
fn a_busy_force_keeps_the_record_it_replaced() {
    let dir = fixture("busy");
    let file = record_file(&dir);
    let prior = completed();
    discovery::save(&file, prior.clone()).unwrap();
    let attempt = begin_at(file.clone(), "/w", true).unwrap();
    assert_eq!(
        discovery::load(&file).unwrap().get_text("state"),
        Some("Unmounting")
    );
    finish(&attempt, "/w", &Err(refused("Busy"))).unwrap();
    assert_restored(&file, &prior);
    drop(attempt);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_dirty_force_keeps_the_record_it_replaced() {
    let dir = fixture("dirty");
    let file = record_file(&dir);
    let prior = completed();
    discovery::save(&file, prior.clone()).unwrap();
    let attempt = begin_at(file.clone(), "/w", true).unwrap();
    finish(&attempt, "/w", &Err(refused("Dirty"))).unwrap();
    assert_restored(&file, &prior);
    drop(attempt);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_call_that_sent_nothing_keeps_the_record_it_replaced() {
    let dir = fixture("cancel");
    let file = record_file(&dir);
    let prior = completed();
    discovery::save(&file, prior.clone()).unwrap();
    let attempt = begin_at(file.clone(), "/w", false).unwrap();
    cancel(&attempt).unwrap();
    assert_restored(&file, &prior);
    drop(attempt);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_first_attempt_that_does_not_run_leaves_no_record() {
    let dir = fixture("first");
    let file = record_file(&dir);
    let attempt = begin_at(file.clone(), "/w", false).unwrap();
    finish(&attempt, "/w", &Err(refused("Busy"))).unwrap();
    assert!(!file.exists());
    drop(attempt);
    std::fs::remove_dir_all(dir).unwrap();
}
