//! The control tree a daemon binds abort controls from (`mount::control_root`).
//!
//! Needs root with `CAP_SYS_ADMIN`, as in the nightly mount lane. Where the
//! shared tree is mounted it is the answer and nothing else is checked here;
//! otherwise, as in a container, the private cases run: one mount at a new
//! directory, reuse of that mount on a later call, and refusal of an
//! existing directory that holds none. Each case removes what it mounted.
//!
//! A user namespace, as rootless engines give a container, refuses this mount
//! though not FUSE mounts. The refusal case runs this binary again inside
//! `unshare -Urm`: the mount's own error comes back and no directory is left,
//! so the daemon starts with its abort control unbound.
#![cfg(target_os = "linux")]
use layerfs_fuse::mount::{control_root, CONTROL};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// The filesystem type mounted at exactly `path` in this namespace.
fn mounted_type(path: &Path) -> Option<String> {
    let table = fs::read_to_string("/proc/self/mountinfo").unwrap();
    table.lines().rev().find_map(|line| {
        let (before, after) = line.split_once(" - ")?;
        (before.split(' ').nth(4) == path.to_str()).then(|| after.split(' ').next().unwrap().into())
    })
}
fn umount(path: &Path) {
    let status = Command::new("umount")
        .arg(path)
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "umount {}: {status:?}", path.display());
}
fn scratch(label: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("layerfs-control-{}-{label}", std::process::id()));
    fs::create_dir(&home).unwrap();
    home
}

#[test]
fn a_private_control_tree_is_mounted_once_reused_and_never_adopted() {
    if mounted_type(Path::new(CONTROL)).as_deref() == Some("fusectl") {
        let home = scratch("shared");
        let root = control_root(&home.join("fusectl")).unwrap();
        assert_eq!(root, Path::new(CONTROL));
        assert!(!home.join("fusectl").exists(), "a private tree was made");
        fs::remove_dir(&home).unwrap();
        println!("CONTROL_ROOT shared tree mounted: private cases not run");
        return;
    }

    // One mount at a new directory, private to its owner.
    let home = scratch("private");
    let private = home.join("fusectl");
    let root = control_root(&private).unwrap();
    assert_eq!(root, private);
    assert_eq!(mounted_type(&private).as_deref(), Some("fusectl"));
    println!("CONTROL_ROOT private={}", private.display());

    // A later call, as after a daemon restart in the same namespace, reuses
    // the mount rather than making a second one.
    let again = control_root(&private).unwrap();
    assert_eq!(again, private);
    let count = fs::read_to_string("/proc/self/mountinfo")
        .unwrap()
        .lines()
        .filter(|line| line.split(' ').nth(4) == private.to_str())
        .count();
    assert_eq!(count, 1, "the tree was mounted again");
    umount(&private);

    // An existing directory without the mount is refused, not adopted.
    let refused = control_root(&private).unwrap_err();
    assert_eq!(refused.kind(), io::ErrorKind::AlreadyExists, "{refused}");
    assert_eq!(mounted_type(&private), None);
    println!("CONTROL_ROOT leftover_directory={:?}", refused.kind());

    fs::remove_dir(&private).unwrap();
    fs::remove_dir(&home).unwrap();
}

/// The refusal half, run only inside a new user namespace by the test below.
#[test]
#[ignore = "run inside `unshare -Urm` by a_refused_control_mount_leaves_no_directory"]
fn refused_inside_a_user_namespace() {
    let home = scratch("refused");
    let private = home.join("fusectl");
    let refused = control_root(&private).unwrap_err();
    assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied, "{refused}");
    assert!(!private.exists(), "a refused mount left its directory");
    fs::remove_dir(&home).unwrap();
    println!("CONTROL_ROOT_REFUSED {refused}");
}

#[test]
fn a_refused_control_mount_leaves_no_directory() {
    let output = Command::new("unshare")
        .arg("-Urm")
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "refused_inside_a_user_namespace",
            "--ignored",
            "--nocapture",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("CONTROL_ROOT_REFUSED"),
        "{:?}\n{stdout}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    println!(
        "{}",
        stdout
            .lines()
            .find(|line| line.starts_with("CONTROL_ROOT_REFUSED"))
            .unwrap()
    );
}
