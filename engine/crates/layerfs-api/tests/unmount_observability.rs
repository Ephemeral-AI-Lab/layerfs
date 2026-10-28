//! One traced normal unmount whose connection outlived its mount (issue #2).
//!
//! The actual daemon executable mounts a Workspace, a second mount namespace
//! holds a copy of it, so the daemon's plain detach is followed by a stalled
//! drain, and the unmount ends its own connection with one abort write
//! (#426, #460). A traced reply must then carry the `unmount` telemetry
//! section, and the daemon must write exactly one structured stderr line for
//! it. A second Workspace, unmounted without a copy, reports neither.
#![cfg(target_os = "linux")]
#[path = "support/daemon_binary.rs"]
mod daemon_binary;
#[allow(dead_code)]
#[path = "../../layerfs-daemon/tests/support/host_install.rs"]
mod support;
use layerfs_api::internal::Control;
use layerfs_bridge::{
    control::{Reply, Request, SdkValue},
    daemon_setup::{DaemonLimits, DaemonSetup},
    duplex,
};
use layerfs_history::WorkspaceId;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Starts the actual executable, installs the fixture's Store through it, and
/// returns the control owner that receives traced replies.
fn start(f: &support::Fixture, stderr: File) -> (Process, Control) {
    fs::set_permissions(&f.directory, fs::Permissions::from_mode(0o700)).unwrap();
    let setup = DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: support::SERVER_PRIVATE,
        control_peer: duplex::public_key(&support::CLIENT_PRIVATE).unwrap(),
        store: f.project.manifest.locator.clone(),
        overlay: f.directory.join("overlay.sqlite").to_str().unwrap().into(),
        mounts: f.directory.join("mounts").to_str().unwrap().into(),
        command_uid: 65534,
        command_gid: 65534,
        limits: DaemonLimits {
            connections: 4,
            handshake_ms: 3000,
            cache_bytes: 64 * 1024,
            namespaces: 8,
            pager_kib: 1024,
            serial_low_water: 0,
            idle_exit_s: 0,
        },
        existing_store: None,
    };
    let config = f.directory.join("daemon.config");
    fs::write(&config, setup.encode().unwrap()).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let mut process = Process(
        Command::new(daemon_binary::path())
            .arg("--config")
            .arg(&config)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr))
            .spawn()
            .unwrap(),
    );
    let stdout = process.0.stdout.take().unwrap();
    let (send, receive) = mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout.take(256))
            .read_line(&mut line)
            .map(|_| line);
        let _ = send.send(result);
    });
    let line = receive
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    reader.join().unwrap();
    let address: SocketAddr = line
        .strip_prefix("LAYERFS_DAEMON_LISTEN ")
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // The stalled drain waits out `drain_wait` (5 s here) before the abort, so
    // this control owner waits longer than the fixture's handshake timeout.
    let stream =
        support::socket(TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap());
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    let mut connection = duplex::initiate(
        stream,
        &support::CLIENT_PRIVATE,
        duplex::public_key(&support::SERVER_PRIVATE).unwrap(),
    )
    .unwrap();
    layerfs_api::internal::install(&f.project, &mut connection).unwrap();
    (process, Control::new(connection))
}

/// Runs in a private copy of the mount namespace, so it holds a copy of the
/// Workspace mount without touching it. It answers one line for each word sent
/// and ends on any other word.
const COPY: &str = r#"echo held
while read word; do
  [ "$word" = probe ] || exit 0
  if out=$(stat "$1/stall-probe" 2>&1); then echo ok; else echo "$out"; fi
done"#;

/// The process holding the copied mount. Dropped, it is killed and reaped.
struct Copy {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<String>,
}

impl Copy {
    fn spawn(mount: &Path) -> Self {
        let mut child = Command::new("unshare")
            .args(["-m", "--propagation", "private", "sh", "-c", COPY, "copy"])
            .arg(mount)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take().unwrap();
        let (send, lines) = mpsc::channel();
        // Ends when the process's output closes, whatever the test does.
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if send.send(line.unwrap_or_default()).is_err() {
                    break;
                }
            }
        });
        let mut copy = Self {
            child,
            stdin,
            lines,
        };
        assert_eq!(copy.line(), "held");
        copy
    }
    fn line(&mut self) -> String {
        self.lines
            .recv_timeout(Duration::from_secs(10))
            .expect("the copy answered within its wait")
    }
    fn ask(&mut self, word: &str) -> String {
        writeln!(self.stdin, "{word}").unwrap();
        self.line()
    }
    /// Asks it to leave and waits a bounded time for its own exit.
    fn leave(&mut self) -> bool {
        let _ = writeln!(self.stdin, "exit");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if self.child.try_wait().unwrap().is_some() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }
}
impl Drop for Copy {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn field_names(section: &SdkValue) -> Vec<String> {
    match section {
        SdkValue::Map(fields) => fields.iter().map(|(name, _)| name.clone()).collect(),
        other => panic!("not a section: {other:?}"),
    }
}

/// The telemetry of the one traced reply received since the last take.
fn one(control: &mut Control) -> SdkValue {
    let mut telemetry = layerfs_api::internal::take_telemetry(control);
    assert_eq!(telemetry.len(), 1, "{telemetry:?}");
    telemetry.pop().unwrap()
}

#[test]
fn a_traced_stalled_unmount_reports_its_abort_path_and_a_plain_one_stays_silent() {
    support::shared_control();
    let f = support::Fixture::new("unmount-observability", None);
    let log = f.directory.join("stderr.log");
    let (process, mut control) = start(&f, File::create(&log).unwrap());
    let branch = f.project.branch.branch.id;

    // The first Workspace is mounted, and a copy of its mount outlives the
    // daemon's plain detach.
    let bound = layerfs_api::internal::workspace(&mut control)
        .bind(WorkspaceId::from_authority([150; 32]).unwrap(), branch)
        .unwrap();
    let ready = layerfs_api::internal::workspace(&mut control)
        .attach(bound.token)
        .unwrap();
    let mut copy = Copy::spawn(Path::new(&ready.directory));

    layerfs_api::internal::trace(&mut control, Some([7; 32]));
    layerfs_api::internal::workspace(&mut control)
        .unmount(ready.token, false)
        .unwrap();
    let after = one(&mut control);
    let section = after
        .get("unmount")
        .expect("the abort path's telemetry section")
        .clone();
    assert_eq!(
        field_names(&section),
        [
            "stage",
            "abort",
            "detached",
            "loops_exited",
            "loops_joined",
            "active",
            "retained",
            "waited_ms"
        ]
    );
    assert_eq!(section.get_text("stage"), Some("Join"));
    assert_eq!(section.get_text("abort"), Some("Written"));
    assert!(section.get_bool("detached"));
    assert_eq!(section.get_int("active"), Some(0));
    assert_eq!(section.get_int("retained"), Some(0));
    let joined = section.get_int("loops_joined").unwrap();
    assert!(joined > 1, "{section:?}");
    assert_eq!(section.get_int("loops_exited"), Some(joined));
    let waited = section.get_int("waited_ms").unwrap();
    assert!((0..60_000).contains(&waited), "{section:?}");
    assert!(section.get("bytes").is_none() && section.get("errno").is_none());

    // The copy's mount died with the connection; it finds nothing to look up.
    let answer = copy.ask("probe");
    assert!(
        answer.contains("No such file or directory")
            || answer.contains("Transport endpoint is not connected"),
        "copy's call answered {answer:?}"
    );
    assert!(copy.leave(), "the copy did not exit by itself");
    drop(copy);

    // A second Workspace unmounts without a copy and takes no abort path.
    layerfs_api::internal::trace(&mut control, None);
    let plain = layerfs_api::internal::workspace(&mut control)
        .bind(WorkspaceId::from_authority([151; 32]).unwrap(), branch)
        .unwrap();
    let plain_ready = layerfs_api::internal::workspace(&mut control)
        .attach(plain.token)
        .unwrap();
    assert_ne!(plain_ready.directory, ready.directory);
    layerfs_api::internal::trace(&mut control, Some([8; 32]));
    layerfs_api::internal::workspace(&mut control)
        .unmount(plain.token, false)
        .unwrap();
    let after = one(&mut control);
    assert!(
        after.get("unmount").is_none(),
        "an unmount that never stalled reports no abort path: {after:?}"
    );

    // One structured line for the one abort path, none for the plain unmount.
    let text = fs::read_to_string(&log).unwrap();
    let lines: Vec<_> = text
        .lines()
        .filter(|line| line.starts_with("LAYERFS_DAEMON_UNMOUNT_ABORT "))
        .collect();
    assert_eq!(lines.len(), 1, "{text}");
    assert!(
        lines[0].contains(" stage=Join abort=Written detached=true "),
        "{}",
        lines[0]
    );
    assert!(
        lines[0].contains(&format!(" loops_joined={joined} ")),
        "{}",
        lines[0]
    );

    layerfs_api::internal::trace(&mut control, None);
    assert_eq!(
        control.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    drop(process);
    f.cleanup();
}
