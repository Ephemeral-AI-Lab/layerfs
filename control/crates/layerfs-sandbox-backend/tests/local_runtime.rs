//! The local runtime root on Linux (`layerfs_sandbox_backend::shared`), which
//! the namespaces backend builds on: where a root may be, how its Sandboxes
//! are named and where their parts go, and the process identity a record
//! keeps (`LocalProcess`: id, start time and boot). The native
//! (host-process) runtime is macOS only (RV3 FAIL 3); its tests are
//! `native_provider`, `local_search_path` and `local_stop_mount`. Nothing
//! here needs root.
#![cfg(target_os = "linux")]
use layerfs_bridge::process::LocalProcess;
use layerfs_sandbox_backend::{shared::Local, RuntimeError};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const WAIT: Duration = Duration::from_secs(10);

/// A fresh 0755 runtime root.
fn fixture() -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "layerfs-local-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(base.join("root")).unwrap();
    fs::set_permissions(&base, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(base.join("root"), fs::Permissions::from_mode(0o755)).unwrap();
    base.join("root")
}

#[test]
fn a_root_must_be_absolute_owned_private_to_write_and_traversable() {
    let root = fixture();
    assert!(matches!(
        Local::new("relative/root"),
        Err(RuntimeError::Protocol(_))
    ));
    assert!(matches!(
        Local::new(root.join("..").join("root")),
        Err(RuntimeError::Protocol(_))
    ));
    assert!(matches!(
        Local::new(root.join("missing")),
        Err(RuntimeError::Io(_))
    ));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o775)).unwrap();
    assert!(matches!(Local::new(&root), Err(RuntimeError::Protocol(_))));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(Local::new(&root), Err(RuntimeError::Protocol(_))));
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    let local = Local::new(&root).unwrap();
    for bad in ["", ".hidden", "-x", "a/b", "..", "a b", &"x".repeat(129)] {
        assert!(local.clone().naming(bad).is_err(), "{bad:?} admitted");
    }
    assert!(local.paths().is_err(), "paths before a name");
    let paths = local.naming("layerfs-shop-1").unwrap().paths().unwrap();
    assert_eq!(paths.directory, root.join("layerfs-shop-1"));
    assert_eq!(paths.config, paths.directory.join("config/daemon.setup"));
    assert_eq!(
        paths.overlay,
        paths.directory.join("overlay/overlay.sqlite")
    );
    assert_eq!(paths.mounts, paths.directory.join("workspaces"));
    assert_eq!(paths.log, paths.directory.join("daemon.log"));
    assert_eq!(paths.tmp, paths.directory.join("tmp"));
}

#[test]
fn a_zombie_is_not_alive() {
    let mut child = Command::new("true").spawn().unwrap();
    let process = LocalProcess::observe(child.id()).unwrap();
    let deadline = Instant::now() + WAIT;
    while process.alive() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!process.alive(), "an exited, unreaped child counted alive");
    child.wait().unwrap();
}

#[test]
fn every_directory_above_a_root_must_be_traversable_by_others() {
    let root = fixture();
    let base = root.parent().unwrap().to_owned();
    // a root inside a directory others cannot pass, as `/root` (0700) is
    let closed = base.join("closed");
    fs::create_dir(&closed).unwrap();
    fs::create_dir(closed.join("root")).unwrap();
    fs::set_permissions(closed.join("root"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&closed, fs::Permissions::from_mode(0o700)).unwrap();
    let refused = |path: &Path| match Local::new(path) {
        Err(RuntimeError::Io(error)) => {
            assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
            error.to_string()
        }
        other => panic!("{} admitted: {other:?}", path.display()),
    };
    let message = refused(&closed.join("root"));
    assert!(
        message.contains(&format!("{} is not traversable", closed.display())),
        "{message}"
    );
    // the same through a symlinked parent: the directory it resolves to counts
    std::os::unix::fs::symlink(&closed, base.join("link")).unwrap();
    let message = refused(&base.join("link").join("root"));
    assert!(message.contains("is not traversable"), "{message}");
    // traversable without being listable is enough
    fs::set_permissions(&closed, fs::Permissions::from_mode(0o711)).unwrap();
    Local::new(closed.join("root")).unwrap();
    Local::new(base.join("link").join("root")).unwrap();
}

#[test]
fn a_process_of_another_boot_is_not_alive() {
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();
    let process = LocalProcess::observe(child.id()).unwrap();
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap();
    let boot = u128::from_str_radix(&boot.trim().replace('-', ""), 16).unwrap();
    assert_eq!(process.boot, Some(boot));
    assert!(process.alive());
    // the same id and start time in another boot (a reboot or a restored
    // snapshot) is another process
    let other = LocalProcess {
        boot: Some(boot ^ 1),
        ..process
    };
    assert!(!other.alive());
    child.kill().unwrap();
    child.wait().unwrap();
}
