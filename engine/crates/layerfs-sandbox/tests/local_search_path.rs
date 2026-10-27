//! A native (macOS) local Sandbox's daemon and commands start with this
//! process's own PATH, so this user's tools (`node` in `~/.local/bin`,
//! Homebrew's `/opt/homebrew/bin`) are found as in the caller's shell; with
//! no PATH, or an empty one, a container's search path and Homebrew's.
//! A small shell script stands in for the daemon. One test: it changes this
//! process's PATH, which no other test in this binary reads.
#![cfg(target_os = "macos")]
use layerfs_sandbox::{
    backend::{
        docker::ExecRequest,
        local::{Local, Sandbox},
    },
    CommandIdentity,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

const WAIT: Duration = Duration::from_secs(10);
const FALLBACK: &str =
    "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/opt/homebrew/bin";

/// A fresh 0755 runtime root and a stand-in daemon that prints its
/// environment, then its listener line, then waits.
fn fixture() -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("layerfs-search-path-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(base.join("root")).unwrap();
    fs::set_permissions(&base, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(base.join("root"), fs::Permissions::from_mode(0o755)).unwrap();
    let daemon = base.join("daemon.sh");
    fs::write(
        &daemon,
        "#!/bin/sh\n[ \"$1\" = --config ] && [ -f \"$2\" ] || exit 3\n/usr/bin/env >&2\necho \"LAYERFS_DAEMON_LISTEN 127.0.0.1:40404\"\nexec /bin/sleep 60\n",
    )
    .unwrap();
    fs::set_permissions(&daemon, fs::Permissions::from_mode(0o755)).unwrap();
    (base.join("root"), daemon)
}

fn started(root: &Path, daemon: &Path, name: &str) -> Sandbox {
    Local::native(root)
        .unwrap()
        .naming(name)
        .unwrap()
        .create(daemon, b"setup bytes", WAIT, Vec::new())
        .unwrap()
}

/// The daemon's PATH line, from its log.
fn daemon_path(sandbox: &Sandbox) -> String {
    let log = String::from_utf8(sandbox.log_tail(200, 16384).unwrap()).unwrap();
    log.lines()
        .find_map(|line| line.strip_prefix("PATH="))
        .unwrap_or_else(|| panic!("no PATH line: {log}"))
        .to_owned()
}

/// A command's PATH: `printf` by its absolute path, so a lookup is not needed.
fn command_path(sandbox: &Sandbox, environment: &[&str]) -> String {
    let request = ExecRequest {
        identity: CommandIdentity {
            uid: nix::unistd::Uid::effective().as_raw(),
            gid: nix::unistd::Gid::effective().as_raw(),
        },
        arguments: vec!["/bin/sh".into(), "-c".into(), "printf %s \"$PATH\"".into()],
        environment: environment.iter().map(|e| e.to_string()).collect(),
        directory: sandbox.directory().display().to_string(),
        stdin: false,
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let exit = sandbox.run(&request, &mut out, &mut err).unwrap();
    assert_eq!(exit.code, 0, "{}", String::from_utf8_lossy(&err));
    String::from_utf8(out).unwrap()
}

#[test]
fn a_native_daemon_and_its_commands_start_with_this_processs_path() {
    let (root, daemon) = fixture();
    let own = "/Users/someone/.local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin";
    let saved = std::env::var_os("PATH");
    std::env::set_var("PATH", own);
    let mut sandbox = started(&root, &daemon, "layerfs-path-1");
    assert_eq!(daemon_path(&sandbox), own);
    assert_eq!(command_path(&sandbox, &[]), own);
    // a command's own PATH still wins
    assert_eq!(
        command_path(&sandbox, &["PATH=/opt/x/bin:/bin"]),
        "/opt/x/bin:/bin"
    );
    // no PATH, then an empty one: a container's search path and Homebrew's
    std::env::remove_var("PATH");
    assert_eq!(command_path(&sandbox, &[]), FALLBACK);
    std::env::set_var("PATH", "");
    assert_eq!(command_path(&sandbox, &[]), FALLBACK);
    let mut second = started(&root, &daemon, "layerfs-path-2");
    assert_eq!(daemon_path(&second), FALLBACK);
    match saved {
        Some(path) => std::env::set_var("PATH", path),
        None => std::env::remove_var("PATH"),
    }
    for sandbox in [&mut sandbox, &mut second] {
        sandbox.stop(1).unwrap();
        sandbox.delete().unwrap();
    }
    let _ = fs::remove_dir_all(root.parent().unwrap());
}
