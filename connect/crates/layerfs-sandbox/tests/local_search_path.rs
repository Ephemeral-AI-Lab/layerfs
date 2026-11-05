//! A native (macOS) local Sandbox's daemon and commands start with this
//! process's own PATH, so this user's tools (`node` in `~/.local/bin`,
//! Homebrew's `/opt/homebrew/bin`) are found as in the caller's shell; with
//! no PATH, or an empty one, a container's search path and Homebrew's.
//! Commands also receive `RUSTUP_HOME`, `CARGO_HOME` and `GIT_CONFIG_GLOBAL`
//! from the caller's environment or real `HOME` when present, while `HOME`
//! stays the Sandbox's scratch directory. A small shell script stands in for
//! the daemon.
#![cfg(target_os = "macos")]
use layerfs_sandbox::{docker::ExecRequest, host_process::Sandbox, local::Local, CommandIdentity};
use std::{
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::Duration,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());
static NEXT: AtomicUsize = AtomicUsize::new(0);
const WAIT: Duration = Duration::from_secs(10);
const FALLBACK: &str =
    "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/opt/homebrew/bin";

/// A fresh 0755 runtime root and a stand-in daemon that prints its
/// environment, then its listener line, then waits.
fn fixture() -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "layerfs-search-path-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
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
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

fn command_script(sandbox: &Sandbox, script: &str, environment: &[&str]) -> String {
    let request = ExecRequest {
        identity: CommandIdentity {
            uid: nix::unistd::Uid::effective().as_raw(),
            gid: nix::unistd::Gid::effective().as_raw(),
        },
        arguments: vec!["/bin/sh".into(), "-c".into(), script.into()],
        environment: environment.iter().map(|e| e.to_string()).collect(),
        directory: sandbox.directory().display().to_string(),
        stdin: false,
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let exit = sandbox.run(&request, &mut out, &mut err).unwrap();
    assert_eq!(exit.code, 0, "{}", String::from_utf8_lossy(&err));
    String::from_utf8(out).unwrap()
}

struct SavedEnv(Vec<(&'static str, Option<OsString>)>);

impl SavedEnv {
    fn capture(names: &[&'static str]) -> Self {
        Self(
            names
                .iter()
                .map(|&name| (name, std::env::var_os(name)))
                .collect(),
        )
    }
}

impl Drop for SavedEnv {
    fn drop(&mut self) {
        for (name, value) in &self.0 {
            match value {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
    }
}

#[test]
fn a_native_command_receives_existing_user_toolchain_and_git_locations() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _saved = SavedEnv::capture(&[
        "HOME",
        "RUSTUP_HOME",
        "CARGO_HOME",
        "GIT_CONFIG_GLOBAL",
        "XDG_CONFIG_HOME",
    ]);
    let (root, daemon) = fixture();
    let base = root.parent().unwrap();
    let user_home = base.join("user-home");
    fs::create_dir_all(&user_home).unwrap();
    for name in [
        "RUSTUP_HOME",
        "CARGO_HOME",
        "GIT_CONFIG_GLOBAL",
        "XDG_CONFIG_HOME",
    ] {
        std::env::remove_var(name);
    }
    std::env::set_var("HOME", &user_home);

    let mut sandbox = started(&root, &daemon, "layerfs-user-env-1");
    let scratch_home = format!("HOME={}", sandbox.tmp().display());
    let probe =
        "printf '%s|%s|%s|%s' \"$HOME\" \"${RUSTUP_HOME:-unset}\" \"${CARGO_HOME:-unset}\" \"${GIT_CONFIG_GLOBAL:-unset}\"";

    // absent under HOME: none of the three is set, and HOME stays the Sandbox's tmp
    assert_eq!(
        command_script(&sandbox, probe, &[&scratch_home]),
        format!("{}|unset|unset|unset", sandbox.tmp().display())
    );

    // XDG git config under ~/.config/git/config when ~/.gitconfig is absent
    let xdg_default = user_home.join(".config/git/config");
    fs::create_dir_all(xdg_default.parent().unwrap()).unwrap();
    fs::write(&xdg_default, "[user]\n\tname = XdgUser\n").unwrap();
    assert_eq!(
        command_script(&sandbox, probe, &[&scratch_home]),
        format!(
            "{}|unset|unset|{}",
            sandbox.tmp().display(),
            xdg_default.display()
        )
    );

    // XDG_CONFIG_HOME/git/config when XDG_CONFIG_HOME is set
    let custom_xdg = base.join("xdg-config");
    let custom_xdg_git = custom_xdg.join("git/config");
    fs::create_dir_all(custom_xdg_git.parent().unwrap()).unwrap();
    fs::write(&custom_xdg_git, "[user]\n\tname = CustomXdg\n").unwrap();
    std::env::set_var("XDG_CONFIG_HOME", &custom_xdg);
    assert_eq!(
        command_script(&sandbox, probe, &[&scratch_home]),
        format!(
            "{}|unset|unset|{}",
            sandbox.tmp().display(),
            custom_xdg_git.display()
        )
    );

    // ~/.rustup, ~/.cargo and ~/.gitconfig present: all three passed through,
    // with ~/.gitconfig chosen ahead of XDG, while HOME remains the Sandbox's tmp
    let rustup = user_home.join(".rustup");
    let cargo = user_home.join(".cargo");
    let gitconfig = user_home.join(".gitconfig");
    fs::create_dir(&rustup).unwrap();
    fs::create_dir(&cargo).unwrap();
    fs::write(&gitconfig, "[user]\n\tname = HomeUser\n").unwrap();
    assert_eq!(
        command_script(&sandbox, probe, &[&scratch_home]),
        format!(
            "{}|{}|{}|{}",
            sandbox.tmp().display(),
            rustup.display(),
            cargo.display(),
            gitconfig.display()
        )
    );

    // caller environment variables take precedence over HOME defaults
    std::env::set_var("RUSTUP_HOME", "/custom/env/rustup");
    std::env::set_var("CARGO_HOME", "/custom/env/cargo");
    std::env::set_var("GIT_CONFIG_GLOBAL", "/custom/env/gitconfig");
    assert_eq!(
        command_script(&sandbox, probe, &[&scratch_home]),
        format!(
            "{}|/custom/env/rustup|/custom/env/cargo|/custom/env/gitconfig",
            sandbox.tmp().display()
        )
    );

    // explicit request environment entries take precedence over both
    assert_eq!(
        command_script(
            &sandbox,
            probe,
            &[
                &scratch_home,
                "RUSTUP_HOME=/req/rustup",
                "CARGO_HOME=/req/cargo",
                "GIT_CONFIG_GLOBAL=/req/gitconfig",
            ]
        ),
        format!(
            "{}|/req/rustup|/req/cargo|/req/gitconfig",
            sandbox.tmp().display()
        )
    );

    sandbox.stop(1).unwrap();
    sandbox.delete().unwrap();
    let _ = fs::remove_dir_all(base);
}
