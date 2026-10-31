//! The user-namespace block is a precondition, not a setting: the namespace
//! runtime refuses to start while a command could create a user namespace and
//! hold its own copy of the Workspace mount.
//!
//! This test opens the host limit for its own duration and restores it, so it
//! runs in its own binary (no Sandbox runs beside it) and only as root in the
//! VM or CI host that owns this kernel.
#![cfg(target_os = "linux")]
use layerfs_sandbox::{backend::namespaces::NamespaceProvider, RuntimeError};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, path::PathBuf};

/// The kernel's limit on user namespaces.
const LIMIT: &str = "/proc/sys/user/max_user_namespaces";

/// Restores the limit however the test ends.
struct Restore(String);

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = fs::write(LIMIT, &self.0);
    }
}

fn root(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("layerfs-namespaces-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    root
}

#[test]
fn an_open_user_namespace_limit_is_refused_before_any_effect() {
    if !nix::unistd::Uid::effective().is_root() {
        eprintln!("SKIPPED namespaces user-namespace refusal: not root");
        return;
    }
    let previous = fs::read_to_string(LIMIT).unwrap().trim().to_owned();
    let restore = Restore(previous.clone());
    let root = root("userns");
    // open the limit: a Sandbox must not start, and nothing may be made
    fs::write(LIMIT, "1\n").unwrap();
    let error = NamespaceProvider::new(&root).unwrap_err();
    assert!(
        matches!(&error, RuntimeError::Unsupported(message)
            if message.contains("user namespaces are not blocked")),
        "{error}"
    );
    assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    // block it again: the same root is accepted, given the other host facts
    fs::write(LIMIT, &previous).unwrap();
    if previous == "0" && Path::new("/dev/fuse").exists() {
        assert!(NamespaceProvider::new(&root).is_ok());
    }
    drop(restore);
    assert_eq!(fs::read_to_string(LIMIT).unwrap().trim(), previous);
    let _ = fs::remove_dir_all(&root);
}
