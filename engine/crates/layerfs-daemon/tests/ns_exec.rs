//! The daemon's hidden `ns-exec` helper (prototype for #3/#499).
//!
//! A command started by the Engine is in the container's initial mount
//! namespace; when the daemon serves from a private one, the Workspace mounts
//! are absent there. The helper joins the namespace of the daemon whose pid the
//! pid file names, drops to the ordinary command identity, and execs the
//! command. These tests build that private namespace with `unshare -m`: a
//! tmpfs is mounted in it only, and a marker written there is invisible to the
//! test's own namespace, so a command that finds it proves it joined.
//!
//! Needs root with CAP_SYS_ADMIN (a Sandbox container has both) and
//! `unshare`/`mount`. A missing or invalid pid file must refuse, never run the
//! command in the wrong namespace.
#![cfg(target_os = "linux")]
use layerfs_bridge::daemon_setup::{DaemonLimits, DaemonSetup};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

/// Every bounded observation of a spawned process.
const WAIT: Duration = Duration::from_secs(10);
/// The marker only the private namespace's tmpfs holds.
const MARKER: &str = "only-in-the-private-namespace\n";
/// The bin under test, built with this test.
const BINARY: &str = env!("CARGO_BIN_EXE_layerfs-daemon");

/// One process in a private copy of the mount namespace, with a tmpfs mounted
/// over the directory it was given.
struct Private {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    directory: PathBuf,
    private: PathBuf,
}

impl Private {
    fn spawn() -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let label = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("layerfs-ns-exec-{}-{label}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let private = directory.join("private");
        fs::create_dir(&private).unwrap();
        // `unshare` keeps its pid: it execs the shell after the unshare.
        let mut child = Command::new("unshare")
            .args([
                "-m",
                "--propagation",
                "private",
                "sh",
                "-c",
                PRIVATE,
                "ns-exec-test",
            ])
            .arg(&private)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("unshare -m needs root with CAP_SYS_ADMIN");
        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take().unwrap();
        let (send, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if send.send(line.unwrap_or_default()).is_err() {
                    break;
                }
            }
        });
        let mut private = Self {
            child,
            stdin,
            lines,
            directory,
            private,
        };
        let pid = private.line("the private namespace reported its pid");
        assert_eq!(
            pid.parse::<u32>().ok(),
            Some(private.child.id()),
            "the unshared process keeps its pid"
        );
        private
    }
    fn line(&mut self, what: &str) -> String {
        self.lines
            .recv_timeout(WAIT)
            .unwrap_or_else(|_| panic!("{what}"))
    }
    fn pid_file(&self) -> PathBuf {
        self.directory.join("daemon.pid")
    }
    fn write_pid(&self) -> PathBuf {
        let path = self.pid_file();
        fs::write(&path, format!("{}\n", self.child.id())).unwrap();
        path
    }
    fn stop(&mut self) {
        let _ = writeln!(self.stdin, "stop");
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if self.child.try_wait().unwrap().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Private {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

/// The shell in the new namespace: mount a tmpfs only there, write the marker,
/// report its pid, then hold until told to stop.
const PRIVATE: &str = r#"set -e
mount -t tmpfs tmpfs "$1"
printf 'only-in-the-private-namespace\n' > "$1/marker"
printf '%s\n' $$
while read line; do [ "$line" = stop ] && exit 0; done"#;

fn run(pid_file: &Path, command: &[&str], identity: Option<(u32, u32)>) -> (i32, String, String) {
    run_in(pid_file, None, command, identity)
}

fn run_in(
    pid_file: &Path,
    directory: Option<&Path>,
    command: &[&str],
    identity: Option<(u32, u32)>,
) -> (i32, String, String) {
    let mut helper = Command::new(BINARY);
    helper.args(["ns-exec", "--pid-file"]).arg(pid_file);
    if let Some(directory) = directory {
        helper.arg("--directory").arg(directory);
    }
    if let Some((uid, gid)) = identity {
        helper
            .arg("--uid")
            .arg(uid.to_string())
            .arg("--gid")
            .arg(gid.to_string());
    }
    let output = helper
        .arg("--")
        .args(command)
        .output()
        .expect("the daemon binary runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn a_command_joins_the_daemons_private_mount_namespace() {
    let mut private = Private::spawn();
    let marker = private.private.join("marker");
    // The proof is meaningful only while the marker is absent here.
    assert!(!marker.exists(), "the tmpfs is absent from this namespace");
    let pid_file = private.write_pid();
    let (code, out, err) = run(
        &pid_file,
        &["sh", "-c", &format!("cat {}", marker.display())],
        None,
    );
    assert_eq!(code, 0, "helper stderr: {err}");
    assert_eq!(out, MARKER, "the command read the private namespace's file");
    private.stop();
}

#[test]
fn the_command_starts_in_the_joined_namespaces_view_of_the_directory() {
    let mut private = Private::spawn();
    let pid_file = private.write_pid();
    let (code, out, err) = run_in(
        &pid_file,
        Some(&private.private),
        &["sh", "-c", "cat marker"],
        None,
    );
    assert_eq!(code, 0, "helper stderr: {err}");
    assert_eq!(out, MARKER);
    private.stop();
}

#[test]
fn the_command_runs_as_the_identity_the_helper_drops_to() {
    let mut private = Private::spawn();
    let pid_file = private.write_pid();
    let (code, out, err) = run(
        &pid_file,
        &["sh", "-c", "id -u; id -g"],
        Some((65534, 65534)),
    );
    assert_eq!(code, 0, "helper stderr: {err}");
    assert_eq!(out, "65534\n65534\n");
    private.stop();
}

#[test]
fn a_missing_pid_file_refuses_and_runs_nothing() {
    let private = Private::spawn();
    let side_effect = private.directory.join("ran-anyway");
    let (code, _, err) = run(
        &private.pid_file(),
        &["sh", "-c", &format!("touch {}", side_effect.display())],
        None,
    );
    assert_eq!(code, 125);
    assert!(err.contains("daemon pid file"), "stderr: {err}");
    assert!(!side_effect.exists(), "no command ran");
}

#[test]
fn a_pid_file_that_is_not_a_pid_refuses_and_runs_nothing() {
    let private = Private::spawn();
    let pid_file = private.pid_file();
    fs::write(&pid_file, b"not a pid\n").unwrap();
    let side_effect = private.directory.join("ran-anyway");
    let (code, _, err) = run(
        &pid_file,
        &["sh", "-c", &format!("touch {}", side_effect.display())],
        None,
    );
    assert_eq!(code, 125);
    assert!(err.contains("does not name a pid"), "stderr: {err}");
    assert!(!side_effect.exists(), "no command ran");
}

#[test]
fn a_pid_without_a_mount_namespace_refuses_and_runs_nothing() {
    let private = Private::spawn();
    let pid_file = private.pid_file();
    fs::write(&pid_file, b"4294967294\n").unwrap();
    let side_effect = private.directory.join("ran-anyway");
    let (code, _, err) = run(
        &pid_file,
        &["sh", "-c", &format!("touch {}", side_effect.display())],
        None,
    );
    assert_eq!(code, 125);
    assert!(err.contains("mount namespace"), "stderr: {err}");
    assert!(!side_effect.exists(), "no command ran");
}

/// The private configuration record of one daemon start, beside its own paths.
fn daemon_config(directory: &Path, private_mount_ns: bool) -> PathBuf {
    for path in [directory.join("local"), directory.join("store")] {
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
    let setup = DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: [1; 32],
        control_peer: [2; 32],
        store: directory
            .join("store/sealed.sqlite")
            .to_str()
            .unwrap()
            .into(),
        overlay: directory
            .join("local/overlay/overlay.sqlite")
            .to_str()
            .unwrap()
            .into(),
        mounts: directory.join("workspaces").to_str().unwrap().into(),
        command_uid: 501,
        command_gid: 20,
        limits: DaemonLimits {
            connections: 4,
            handshake_ms: 3000,
            cache_bytes: 64 * 1024,
            namespaces: 2,
            pager_kib: 1024,
            serial_low_water: 0,
            idle_exit_s: 0,
        },
        existing_store: None,
        private_mount_ns,
    };
    let path = directory.join("daemon.setup");
    fs::write(&path, setup.encode().unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    path
}

fn unique_directory(label: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "layerfs-ns-daemon-{}-{label}-{next}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    directory
}

/// The daemon writes the pid file only after it has taken its private mount
/// namespace, and taking it refuses startup on failure: a written pid file
/// therefore proves the unshare ran. Whether the daemon then serves or fails
/// (no `/dev/fuse` in this container) does not matter; the file stays.
#[test]
fn the_daemon_writes_its_pid_file_only_when_asked_for_a_private_namespace() {
    let directory = unique_directory("on");
    let config = daemon_config(&directory, true);
    let mut daemon = Command::new(BINARY)
        .args(["--config"])
        .arg(&config)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid_file = directory.join("local/daemon.pid");
    let deadline = Instant::now() + WAIT;
    while !pid_file.exists() {
        assert!(
            daemon.try_wait().unwrap().is_none() || pid_file.exists(),
            "the daemon ended before writing its pid file"
        );
        assert!(Instant::now() < deadline, "no pid file within {WAIT:?}");
        thread::sleep(Duration::from_millis(10));
    }
    let pid: u32 = fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(pid, daemon.id(), "the pid file names the daemon process");
    let _ = daemon.kill();
    let _ = daemon.wait();
    fs::remove_dir_all(&directory).unwrap();

    let directory = unique_directory("off");
    let config = daemon_config(&directory, false);
    let mut daemon = Command::new(BINARY)
        .args(["--config"])
        .arg(&config)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid_file = directory.join("local/daemon.pid");
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(
            !pid_file.exists(),
            "the flag is off; no pid file is written"
        );
        if daemon.try_wait().unwrap().is_some() || Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let _ = daemon.kill();
    let _ = daemon.wait();
    fs::remove_dir_all(&directory).unwrap();
}
