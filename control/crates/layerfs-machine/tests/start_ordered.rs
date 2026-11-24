//! A start never mints an ordered incarnation over a worker it could not
//! stop: the provider state and the recorded endpoint must both say the old
//! worker is gone. Otherwise the start fails before the token is written, so
//! no Store reconcile can mark that worker's live mounts dead.
#![cfg(feature = "machine")]
use layerfs_bridge::control::SdkValue;
use layerfs_home::home;
use layerfs_machine::record::MachineRecord;
use layerfs_machine::Related;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn none(_: &Path, _: &str) -> Result<Vec<String>, layerfs_bridge::control::SdkRefusal> {
    Ok(Vec::new())
}

fn free(_: &Path, _: &str) -> Result<(), layerfs_bridge::control::SdkRefusal> {
    Ok(())
}

const RELATED: Related = Related {
    stores_on: none,
    sandboxes_on: none,
    hosted_on: none,
    can_remove: free,
    can_stop: free,
    can_change: free,
};


fn scratch(tag: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "layerfs-start-ordered-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn bundle(root: &Path) -> PathBuf {
    let dir = root.join("bundle");
    std::fs::create_dir(&dir).unwrap();
    for name in ["layerfs", "layerfs-daemon", "layerfs-sandbox-helper"] {
        std::fs::write(dir.join(name), b"").unwrap();
    }
    std::fs::write(
        dir.join("bundle.json"),
        r#"{"hash":"0123abcd","protocol":3}"#,
    )
    .unwrap();
    dir
}

/// A docker CLI that always reports a running worker container and accepts
/// a stop without changing that state: a stop this build cannot confirm.
fn docker_that_does_not_stop(root: &Path) -> PathBuf {
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let script = root.join("bin/docker");
    let body = format!(
        r#"#!/bin/sh
echo "$@" >> '{root}/calls'
case "$*" in
  *" inspect "*) printf 'running\tmachine-worker\n' ;;
  *" version "*) printf '25.0.0\n' ;;
esac
"#,
        root = root.display()
    );
    std::fs::write(&script, body).unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();
    script
}

#[test]
fn a_start_that_cannot_stop_the_old_worker_mints_no_incarnation() {
    let root = scratch("unconfirmed-stop");
    let dir = bundle(&root);
    docker_that_does_not_stop(&root);
    let old_path = std::env::var_os("PATH").unwrap();
    std::env::set_var(
        "PATH",
        format!("{}/bin:{}", root.display(), old_path.to_string_lossy()),
    );
    std::env::set_var("LAYERFS_BUNDLE", &dir);

    let home_dir = root.join("home");
    let id = home::ensure(&home_dir).unwrap().id;
    layerfs_machine::ensure_built_ins(&home_dir, &id).unwrap();
    let socket = root.join("docker.sock");
    MachineRecord::of(&home_dir, "docker")
        .unwrap()
        .with("docker", SdkValue::text(socket.display().to_string()))
        .with("os_image", SdkValue::text("debian:test"))
        .write(&home_dir)
        .unwrap();

    let refusal = layerfs_machine::running(&home_dir, "docker", RELATED).unwrap_err();
    assert_eq!(refusal.code, "Failed", "{refusal:?}");
    assert!(
        refusal.message.contains("did not stop"),
        "{}",
        refusal.message
    );

    // No token was minted, so a later admission cannot settle the old
    // worker's mounts, and no endpoint is recorded for a worker never started.
    let record = MachineRecord::of(&home_dir, "docker").unwrap();
    assert_eq!(record.incarnation(), None);
    assert!(!home::paths::worker_record(&home_dir, "docker").is_file());

    std::env::set_var("PATH", old_path);
    std::env::remove_var("LAYERFS_BUNDLE");
    std::fs::remove_dir_all(root).unwrap();
}
