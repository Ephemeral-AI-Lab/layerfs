//! The hidden `ns-exec-supervisor` session skeleton (#538).
//!
//! The supervisor receives the daemon's mount-namespace descriptor over the
//! existing `SCM_RIGHTS` seam, starts a child that joins that namespace and the
//! Workspace directory, and reports on its control socket. These tests build a
//! private namespace with `unshare -m` (as `ns_exec.rs` does): a marker is
//! written into a tmpfs mounted only there, so a command that reads it proves
//! the child joined the served namespace, while the supervisor itself stayed
//! outside.
//!
//! Needs root with CAP_SYS_ADMIN (a Sandbox container has both) and
//! `unshare`/`mount`; the mount lane runs each binary in its own container.
#![cfg(target_os = "linux")]
use layerfs_daemon::application::ns_exec_control::{self, Message};
use nix::{
    sys::{
        signal::{kill, Signal},
        socket::{sendmsg, ControlMessage, MsgFlags, SockaddrStorage},
    },
    unistd::Pid,
};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, IoSlice, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{MetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio},
    sync::mpsc,
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
/// over the directory it was given. The supervisor is handed this namespace's
/// descriptor, exactly as the daemon hands its own.
struct Private {
    child: Child,
    stdin: ChildStdin,
    directory: PathBuf,
    private: PathBuf,
}

impl Private {
    fn spawn() -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let label = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "layerfs-exec-supervisor-{}-{label}",
            std::process::id()
        ));
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
        let private = Self {
            child,
            stdin,
            directory,
            private,
        };
        let ready = lines
            .recv_timeout(WAIT)
            .expect("the private namespace is ready");
        assert_eq!(ready, "ready");
        private
    }

    /// This child's mount namespace; root may open it because the child is
    /// dumpable. The daemon is not, which is why the daemon serves its own.
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
/// report readiness, then hold until told to stop.
const PRIVATE: &str = r#"set -e
mount -t tmpfs tmpfs "$1"
printf 'only-in-the-private-namespace\n' > "$1/marker"
printf 'ready\n'
while read line; do [ "$line" = stop ] && exit 0; done"#;

/// A socket that serves one already-open descriptor to its first connection.
fn serve_once(path: &Path, namespace: File) -> thread::JoinHandle<()> {
    if path.exists() {
        fs::remove_file(path).unwrap();
    }
    let listener = UnixListener::bind(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
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

/// One running supervisor and the caller's control connection.
struct Run {
    process: Child,
    stdout: ChildStdout,
    client: UnixStream,
}

impl Run {
    fn start(
        private: &Private,
        command: &[&str],
        directory: bool,
        identity: Option<(u32, u32)>,
    ) -> Self {
        let socket = private.directory.join("ns-exec.sock");
        let _server = serve_once(&socket, private.namespace());
        let control = private.directory.join("control.sock");
        let mut helper = Command::new(BINARY);
        helper
            .arg("ns-exec-supervisor")
            .arg("--socket")
            .arg(&socket)
            .arg("--control")
            .arg(&control);
        if directory {
            helper.arg("--directory").arg(&private.private);
        }
        if let Some((uid, gid)) = identity {
            helper
                .arg("--uid")
                .arg(uid.to_string())
                .arg("--gid")
                .arg(gid.to_string());
        }
        let mut process = helper
            .arg("--")
            .args(command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the daemon binary runs");
        let stdout = process.stdout.take().unwrap();
        let client = connect(&control);
        let mode = fs::metadata(&control).unwrap().mode() & 0o777;
        assert_eq!(mode, 0o600, "the control socket is private");
        Self {
            process,
            stdout,
            client,
        }
    }

    fn recv(&mut self) -> Message {
        ns_exec_control::receive(&mut self.client)
            .expect("a framed control message")
            .expect("the supervisor keeps the connection open")
    }

    fn send(&mut self, message: Message) {
        ns_exec_control::send(&mut self.client, message).expect("write a control message");
    }

    fn started(&mut self) -> (i32, i32) {
        match self.recv() {
            Message::Started { pid, pgid } => (pid, pgid),
            other => panic!("expected Started, got {other:?}"),
        }
    }

    fn exited(&mut self) -> (i32, i32, bool) {
        match self.recv() {
            Message::Exited {
                code,
                signal,
                group_alive,
            } => (code, signal, group_alive),
            other => panic!("expected Exited, got {other:?}"),
        }
    }

    fn finish(mut self) -> (ExitStatus, String) {
        let status = self.process.wait().unwrap();
        let mut stdout = String::new();
        self.stdout.read_to_string(&mut stdout).unwrap();
        (status, stdout)
    }
}

fn connect(path: &Path) -> UnixStream {
    let deadline = Instant::now() + WAIT;
    loop {
        match UnixStream::connect(path) {
            Ok(stream) => {
                stream.set_read_timeout(Some(WAIT)).unwrap();
                return stream;
            }
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Err(error) => panic!("connect {}: {error}", path.display()),
        }
    }
}

#[test]
fn a_command_joins_the_namespace_enters_the_directory_and_reports_exit_7() {
    let mut private = Private::spawn();
    let marker = private.private.join("marker");
    assert!(!marker.exists(), "the tmpfs is absent from this namespace");
    let mut run = Run::start(
        &private,
        &[
            "sh",
            "-c",
            &format!("cat {}; sleep 1; exit 7", marker.display()),
        ],
        true,
        None,
    );
    let supervisor = run.process.id();
    let (pid, pgid) = run.started();
    assert_eq!(pgid, pid, "the child has its own session and process group");
    assert_ne!(pid, supervisor as i32, "the child is not the supervisor");
    let (code, signal, group_alive) = run.exited();
    assert_eq!((code, signal, group_alive), (7, 0, false));
    let (status, stdout) = run.finish();
    assert_eq!(
        status.code(),
        Some(7),
        "the supervisor exits with the child"
    );
    assert_eq!(
        stdout, MARKER,
        "the command read the private namespace's marker"
    );
    private.stop();
}

#[test]
fn a_signal_reaches_the_child_group_and_is_reported() {
    let mut private = Private::spawn();
    let mut run = Run::start(&private, &["sleep", "30"], false, None);
    let (pid, pgid) = run.started();
    assert_eq!(pgid, pid);
    run.send(Message::Signal { signo: 15 });
    let (code, signal, group_alive) = run.exited();
    assert_eq!((code, signal, group_alive), (0, 15, false));
    let (status, _) = run.finish();
    assert_eq!(status.code(), Some(143), "128 + SIGTERM");
    private.stop();
}

#[test]
fn status_reports_the_running_child_and_a_kill_still_reaches_it() {
    let mut private = Private::spawn();
    let mut run = Run::start(&private, &["sleep", "30"], false, None);
    let (pid, pgid) = run.started();
    run.send(Message::Status);
    match run.recv() {
        Message::StatusReply {
            running,
            pid: reported_pid,
            pgid: reported_pgid,
            group_alive,
            ..
        } => {
            assert!(running);
            assert_eq!((reported_pid, reported_pgid), (pid, pgid));
            assert!(group_alive);
        }
        other => panic!("expected StatusReply, got {other:?}"),
    }
    run.send(Message::Signal { signo: 9 });
    let (_, signal, group_alive) = run.exited();
    assert_eq!((signal, group_alive), (9, false));
    let (status, _) = run.finish();
    assert_eq!(status.code(), Some(137), "128 + SIGKILL");
    private.stop();
}

#[test]
fn a_surviving_grandchild_is_reported_rather_than_a_clean_exit() {
    let mut private = Private::spawn();
    let mut run = Run::start(&private, &["sh", "-c", "sleep 5 & sleep 1"], false, None);
    let (_, pgid) = run.started();
    let (code, signal, group_alive) = run.exited();
    assert_eq!((code, signal), (0, 0));
    assert!(
        group_alive,
        "the background sleep is still in the child's process group"
    );
    let (status, _) = run.finish();
    assert_eq!(status.code(), Some(0));
    // This slice does not reap grandchildren; the test only cleans its own up.
    let _ = kill(Pid::from_raw(-pgid), Signal::SIGKILL);
    private.stop();
}

#[test]
fn the_child_runs_as_the_identity_it_drops_to() {
    let mut private = Private::spawn();
    let mut run = Run::start(
        &private,
        &["sh", "-c", "id -u; id -g; sleep 1"],
        false,
        Some((65534, 65534)),
    );
    run.started();
    let (code, signal, group_alive) = run.exited();
    assert_eq!((code, signal, group_alive), (0, 0, false));
    let (status, stdout) = run.finish();
    assert_eq!(status.code(), Some(0));
    assert_eq!(stdout, "65534\n65534\n");
    private.stop();
}

#[test]
fn the_supervisor_stays_outside_the_workspace_namespace_and_directory() {
    let mut private = Private::spawn();
    let mut run = Run::start(&private, &["sleep", "30"], false, None);
    run.started();
    let supervisor = run.process.id();
    let supervisor_namespace = fs::read_link(format!("/proc/{supervisor}/ns/mnt")).unwrap();
    let private_namespace = fs::read_link(format!("/proc/{}/ns/mnt", private.child.id())).unwrap();
    assert_ne!(
        supervisor_namespace, private_namespace,
        "the supervisor must not join the Workspace namespace"
    );
    assert_eq!(
        supervisor_namespace,
        fs::read_link("/proc/self/ns/mnt").unwrap(),
        "the supervisor stays in the caller's namespace"
    );
    assert_eq!(
        fs::read_link(format!("/proc/{supervisor}/cwd")).unwrap(),
        fs::read_link("/proc/self/cwd").unwrap(),
        "the supervisor must not enter the Workspace directory"
    );
    run.send(Message::Signal { signo: 9 });
    run.exited();
    let (status, _) = run.finish();
    assert_eq!(status.code(), Some(137));
    private.stop();
}
