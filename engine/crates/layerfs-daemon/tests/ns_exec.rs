//! The daemon's hidden `ns-exec` helper (prototype for #3/#499).
//!
//! A command started by the Engine is in the container's initial mount
//! namespace; when the daemon serves from a private one, the Workspace mounts
//! are absent there. The helper receives the daemon's mount-namespace
//! descriptor over the daemon's socket, joins it, drops to the ordinary
//! command identity, and execs the command. These tests build that private
//! namespace with `unshare -m`: a tmpfs is mounted in it only, and a marker
//! written there is invisible to the test's own namespace, so a command that
//! finds it proves it joined.
//!
//! Needs root with CAP_SYS_ADMIN (a Sandbox container has both) and
//! `unshare`/`mount`. A missing socket, a served non-namespace descriptor and a
//! refused peer must refuse, never run the command in the wrong namespace.
#![cfg(target_os = "linux")]
use layerfs_bridge::daemon_setup::{DaemonLimits, DaemonSetup};
use layerfs_daemon::application::ns_fd::NsFd;
use nix::sys::socket::{
    recvmsg, sendmsg, ControlMessage, ControlMessageOwned, MsgFlags, SockaddrStorage,
};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, IoSlice, IoSliceMut, Write},
    os::{
        fd::{AsRawFd, RawFd},
        unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
            process::CommandExt,
        },
    },
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
    /// This child's mount namespace, which root may open because the child is
    /// dumpable; the daemon is not, which is why the daemon serves the fd.
    fn namespace(&self) -> File {
        File::open(format!("/proc/{}/ns/mnt", self.child.id())).unwrap()
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

fn run_socket(
    socket: &Path,
    directory: Option<&Path>,
    command: &[&str],
    identity: Option<(u32, u32)>,
) -> (i32, String, String) {
    run_command(socket, directory, command, identity, None)
}

fn run_command(
    socket: &Path,
    directory: Option<&Path>,
    command: &[&str],
    identity: Option<(u32, u32)>,
    as_uid: Option<u32>,
) -> (i32, String, String) {
    let mut helper = Command::new(BINARY);
    helper.arg("ns-exec").arg("--socket").arg(socket);
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
    if let Some(uid) = as_uid {
        helper.uid(uid);
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

/// A socket that serves one already-open descriptor to its first connection.
fn serve_once(path: &Path, namespace: File) -> thread::JoinHandle<()> {
    let listener = listen_once(path);
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let descriptors = [namespace.as_raw_fd()];
        let control = [ControlMessage::ScmRights(&descriptors)];
        sendmsg(
            stream.as_raw_fd(),
            &[IoSlice::new(&[0u8])],
            &control,
            MsgFlags::empty(),
            None::<&SockaddrStorage>,
        )
        .unwrap();
    })
}

/// A socket that accepts one connection and closes it without sending.
fn close_once(path: &Path) -> thread::JoinHandle<()> {
    let listener = listen_once(path);
    thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
    })
}

fn listen_once(path: &Path) -> UnixListener {
    if path.exists() {
        fs::remove_file(path).unwrap();
    }
    let listener = UnixListener::bind(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    listener
}

/// One received message: its payload bytes and every descriptor it carried.
fn receive_message(stream: &UnixStream) -> (usize, Vec<RawFd>) {
    let mut byte = [0u8; 8];
    let mut iov = [IoSliceMut::new(&mut byte)];
    let mut space = nix::cmsg_space!([RawFd; 4]);
    let message = recvmsg::<()>(
        stream.as_raw_fd(),
        &mut iov,
        Some(&mut space),
        MsgFlags::empty(),
    )
    .unwrap();
    let mut descriptors = Vec::new();
    for control in message.cmsgs().unwrap() {
        if let ControlMessageOwned::ScmRights(items) = control {
            descriptors.extend(items);
        }
    }
    (message.bytes, descriptors)
}

#[test]
fn a_command_joins_the_namespace_served_over_the_daemons_socket() {
    let mut private = Private::spawn();
    let marker = private.private.join("marker");
    assert!(!marker.exists(), "the tmpfs is absent from this namespace");
    let socket = private.directory.join("ns-exec.sock");
    let server = serve_once(&socket, private.namespace());
    let (code, out, err) = run_socket(
        &socket,
        None,
        &["sh", "-c", &format!("cat {}", marker.display())],
        None,
    );
    server.join().unwrap();
    assert_eq!(code, 0, "helper stderr: {err}");
    assert_eq!(out, MARKER, "the command read the served namespace's file");
    private.stop();
}

#[test]
fn the_socket_command_starts_in_the_served_namespaces_directory() {
    let mut private = Private::spawn();
    let socket = private.directory.join("ns-exec.sock");
    let server = serve_once(&socket, private.namespace());
    let (code, out, err) = run_socket(
        &socket,
        Some(&private.private),
        &["sh", "-c", "cat marker"],
        None,
    );
    server.join().unwrap();
    assert_eq!(code, 0, "helper stderr: {err}");
    assert_eq!(out, MARKER);
    private.stop();
}

#[test]
fn the_socket_command_runs_as_the_identity_the_helper_drops_to() {
    let mut private = Private::spawn();
    let socket = private.directory.join("ns-exec.sock");
    let server = serve_once(&socket, private.namespace());
    let (code, out, err) = run_socket(
        &socket,
        None,
        &["sh", "-c", "id -u; id -g"],
        Some((65534, 65534)),
    );
    server.join().unwrap();
    assert_eq!(code, 0, "helper stderr: {err}");
    assert_eq!(out, "65534\n65534\n");
    private.stop();
}

#[test]
fn a_missing_socket_refuses_and_runs_nothing() {
    let private = Private::spawn();
    let side_effect = private.directory.join("ran-anyway");
    let (code, _, err) = run_socket(
        &private.directory.join("absent.sock"),
        None,
        &["sh", "-c", &format!("touch {}", side_effect.display())],
        None,
    );
    assert_eq!(code, 125);
    assert!(err.contains("receive mount namespace"), "stderr: {err}");
    assert!(!side_effect.exists(), "no command ran");
}

#[test]
fn a_socket_that_sends_nothing_refuses_and_runs_nothing() {
    let private = Private::spawn();
    let socket = private.directory.join("silent.sock");
    let server = close_once(&socket);
    let side_effect = private.directory.join("ran-anyway");
    let (code, _, err) = run_socket(
        &socket,
        None,
        &["sh", "-c", &format!("touch {}", side_effect.display())],
        None,
    );
    server.join().unwrap();
    assert_eq!(code, 125);
    assert!(
        err.contains("sent no mount namespace descriptor"),
        "stderr: {err}"
    );
    assert!(!side_effect.exists(), "no command ran");
}

#[test]
fn a_socket_that_sends_another_file_refuses_and_runs_nothing() {
    let private = Private::spawn();
    let socket = private.directory.join("wrong.sock");
    let server = serve_once(&socket, File::open("/dev/null").unwrap());
    let side_effect = private.directory.join("ran-anyway");
    let (code, _, err) = run_socket(
        &socket,
        None,
        &["sh", "-c", &format!("touch {}", side_effect.display())],
        None,
    );
    server.join().unwrap();
    assert_eq!(code, 125);
    assert!(err.contains("not a mount namespace"), "stderr: {err}");
    assert!(!side_effect.exists(), "no command ran");
}

#[test]
fn a_helper_without_a_namespace_source_refuses_and_runs_nothing() {
    let directory = unique_directory("no-source");
    let side_effect = directory.join("ran-anyway");
    let output = Command::new(BINARY)
        .args(["ns-exec", "--", "sh", "-c"])
        .arg(format!("touch {}", side_effect.display()))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("usage: layerfs-daemon ns-exec"),
        "stderr: {err}"
    );
    assert!(!side_effect.exists(), "no command ran");
    fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn malformed_helper_argv_refuses_with_a_reason_and_runs_nothing() {
    let cases: [(&str, &[&str]); 5] = [
        ("usage: layerfs-daemon ns-exec", &["--socket", "/s"]),
        ("usage: layerfs-daemon ns-exec", &["--socket", "/s", "--"]),
        (
            "--uid and --gid must be given together",
            &["--socket", "/s", "--uid", "501", "--", "true"],
        ),
        (
            "--uid and --gid must name the non-root command identity",
            &["--socket", "/s", "--uid", "0", "--gid", "0", "--", "true"],
        ),
        (
            "--uid is not a number",
            &[
                "--socket", "/s", "--uid", "ordinary", "--gid", "20", "--", "true",
            ],
        ),
    ];
    for (reason, argv) in cases {
        let output = Command::new(BINARY)
            .arg("ns-exec")
            .args(argv)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(125), "argv: {argv:?}");
        let err = String::from_utf8_lossy(&output.stderr);
        assert!(err.contains(reason), "argv: {argv:?}, stderr: {err}");
    }
}

#[test]
fn a_helper_in_another_pid_namespace_is_refused_by_the_daemons_socket() {
    let directory = unique_directory("foreign-pid-ns");
    let socket = directory.join("ns-exec.sock");
    let _server = NsFd::start(&socket).unwrap();
    // The child is root but in a pid namespace of its own; the daemon's
    // admission requires this process's pid namespace.
    let output = Command::new("unshare")
        .args(["-p", "--fork", BINARY, "ns-exec", "--socket"])
        .arg(&socket)
        .args(["--", "true"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("sent no mount namespace descriptor"),
        "stderr: {err}"
    );
    fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn the_daemons_socket_is_private_replaces_a_stale_path_and_sends_one_message() {
    let directory = unique_directory("socket");
    let socket = directory.join("ns-exec.sock");
    // A stale path from a dead daemon is replaced, not adopted.
    fs::write(&socket, b"stale").unwrap();
    let server = NsFd::start(&socket).unwrap();
    let mode = fs::metadata(&socket).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "only root may reach the socket");
    let stream = UnixStream::connect(&socket).unwrap();
    let (bytes, descriptors) = receive_message(&stream);
    assert_eq!(bytes, 1, "one message");
    assert_eq!(descriptors.len(), 1, "exactly one descriptor");
    assert_eq!(
        fs::read_link(format!("/proc/self/fd/{}", descriptors[0])).unwrap(),
        fs::read_link("/proc/self/ns/mnt").unwrap(),
        "the descriptor is this process's mount namespace"
    );
    nix::unistd::close(descriptors[0]).unwrap();
    // The server closes the connection after the one message.
    let (bytes, descriptors) = receive_message(&stream);
    assert_eq!(bytes, 0);
    assert!(descriptors.is_empty());
    drop(server);
    assert!(!socket.exists(), "the server removed its socket");
    fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn the_daemons_socket_refuses_a_non_root_peer() {
    let directory = unique_directory("non-root");
    let socket = directory.join("ns-exec.sock");
    let server = NsFd::start(&socket).unwrap();
    // The test widens the mode so a non-root peer can reach admission; the
    // server then closes without sending.
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o666)).unwrap();
    let side_effect = directory.join("ran-anyway");
    let (code, _, err) = run_command(
        &socket,
        None,
        &["sh", "-c", &format!("touch {}", side_effect.display())],
        None,
        Some(65534),
    );
    assert_eq!(code, 125);
    assert!(
        err.contains("sent no mount namespace descriptor"),
        "stderr: {err}"
    );
    assert!(!side_effect.exists(), "no command ran");
    drop(server);
    fs::remove_dir_all(&directory).unwrap();
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

/// The daemon writes the pid file and binds the socket only after it has taken
/// its private mount namespace, and taking it refuses startup on failure: a
/// served socket therefore proves the unshare ran. Whether the daemon then
/// serves or fails (no `/dev/fuse` in this container) does not matter; both
/// paths stay.
#[test]
fn the_daemon_serves_its_socket_and_pid_file_only_when_asked() {
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
    let socket = directory.join("local/ns-exec.sock");
    let deadline = Instant::now() + WAIT;
    while !(pid_file.exists() && socket.exists()) {
        assert!(
            daemon.try_wait().unwrap().is_none() || socket.exists(),
            "the daemon ended before serving its socket"
        );
        assert!(
            Instant::now() < deadline,
            "no pid file and socket within {WAIT:?}"
        );
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
    let socket = directory.join("local/ns-exec.sock");
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(
            !pid_file.exists() && !socket.exists(),
            "the flag is off; neither a pid file nor a socket is written"
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
