//! One traced normal unmount whose connection outlived its mount (issue #2).
//!
//! The actual daemon executable mounts a Workspace, a second mount namespace
//! holds a copy of it, so the daemon's plain detach is followed by a stalled
//! drain, and the unmount ends its own connection with one abort write
//! (#426, #460). A traced reply must then carry the `unmount` telemetry
//! section, and the daemon must write exactly one structured stderr line for
//! it. A second Workspace, unmounted without a copy, reports neither.
//!
//! The second test takes the same observation through the retained custody a
//! later Revoke or Close stop keeps for the reply that reports it. A retained
//! stop can only be staged in-process (the entry's owner is not a control
//! request), so it drives the production service over the same traced
//! exchange; the reply envelope is the test's, the outcome and section are the
//! daemon's. This target mounts FUSE, so `tools/ci_tests.py` routes it to the
//! nightly mount lane and the fast lane never runs it.
#![cfg(target_os = "linux")]
#[path = "support/daemon_binary.rs"]
mod daemon_binary;
#[allow(dead_code)]
#[path = "../../layerfs-daemon/tests/support/fusectl.rs"]
mod fusectl;
#[allow(dead_code)]
#[path = "../../layerfs-daemon/tests/support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "../../layerfs-daemon/tests/support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "../../layerfs-daemon/tests/support/host_install.rs"]
mod support;
use layerfs_api::internal::{take_telemetry, trace, Control};
use layerfs_bridge::{
    control::{
        Answer, Call, Reply, Request, SdkValue, TeardownStage, UnmountControl, WorkspaceToken,
    },
    daemon_setup::{DaemonLimits, DaemonSetup},
    duplex,
};
use layerfs_daemon::{
    bootstrap::open_store,
    control::{Failure, Service},
    telemetry, ServiceClass,
};
use layerfs_history::WorkspaceId;
use layerfs_storage::ReservationBlocks;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{mpsc, Arc},
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

/// Removes a fixture's directory on every exit, including a panic, so an
/// aborted run leaves no kernel tree behind.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The daemon's own startup accounts for every missing LISTEN line: a runner
/// without `/dev/fuse` or mount privileges exits before printing it.
fn startup(error: &str, log: &Path) -> ! {
    let stderr = fs::read_to_string(log).unwrap_or_else(|read| format!("<unreadable: {read}>"));
    panic!("{error}; daemon stderr:\n{stderr}")
}

/// Starts the actual executable, installs the fixture's Store through it, and
/// returns the control owner that receives traced replies.
fn start(f: &support::Fixture, log: &Path) -> (Process, Control) {
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
            .stderr(Stdio::from(File::create(log).unwrap()))
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
    let line = match receive.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(line)) => line,
        Ok(Err(error)) => startup(&format!("no daemon stdout: {error}"), log),
        // The reader thread still waits for the child, which Process kills.
        Err(_) => startup("no LAYERFS_DAEMON_LISTEN within 5 s", log),
    };
    reader.join().unwrap();
    let Some(address) = line
        .strip_prefix("LAYERFS_DAEMON_LISTEN ")
        .and_then(|rest| rest.trim().parse::<SocketAddr>().ok())
    else {
        startup("daemon printed no LAYERFS_DAEMON_LISTEN", log)
    };
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
    let _scratch = Scratch(f.directory.clone());
    let log = f.directory.join("stderr.log");
    let (process, mut control) = start(&f, &log);
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

/// The reply form the daemon application composes for these outcomes
/// (`application/diagnostics.rs`): a retained stop answers `Reply::Retained`,
/// and a traced reply carries the `unmount` section built from the abort
/// observation the entry kept. The application's startup owner is not a
/// library API and a retained Revoke or Close stop can only be staged
/// in-process, so the test runs the real `Service` and wraps its real outcome
/// here; only the envelope around the production observation is the test's.
fn observed(service: &Service, call: Call) -> Answer {
    let operation = call
        .request
        .without_observation()
        .expect("the client sent a one-layer observed request");
    let outcome = service.execute_control(operation);
    let abort = match &outcome {
        Ok(success) => success.abort.clone(),
        Err(Failure::Retained(custody)) => service.retained_abort(custody.token),
        Err(other) => panic!("unexpected control outcome: {other:?}"),
    };
    let reply = match &outcome {
        Ok(success) => success.reply.clone(),
        Err(Failure::Retained(custody)) => Reply::Retained(custody.clone()),
        Err(other) => panic!("unexpected control outcome: {other:?}"),
    };
    let abort = abort.expect("a retained stop that aborted its connection keeps the observation");
    Answer {
        id: call.id,
        reply: Reply::Observed {
            reply: Box::new(reply),
            telemetry: SdkValue::Map(vec![(
                "unmount".to_owned(),
                telemetry::unmount_abort(&abort),
            )]),
        },
    }
}

/// One traced Unmount through the SDK's control owner, answered by the
/// in-process service above: the original reply and its telemetry.
fn traced_unmount(service: Arc<Service>, token: WorkspaceToken) -> (Reply, Vec<SdkValue>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client =
        TcpStream::connect_timeout(&listener.local_addr().unwrap(), Duration::from_secs(2))
            .unwrap();
    let (server, _) = listener.accept().unwrap();
    for socket in [&client, &server] {
        // The stalled drain waits seconds before the reply; the bounded wait
        // is far above it and below the test ceiling.
        socket
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(60)))
            .unwrap();
    }
    let worker = thread::spawn(move || {
        let mut channel = duplex::accept(
            server,
            &support::SERVER_PRIVATE,
            duplex::public_key(&support::CLIENT_PRIVATE).unwrap(),
        )
        .unwrap();
        let call = Call::decode(channel.receive.receive().unwrap()).unwrap();
        let answer = observed(&service, call);
        channel.send.send(&answer.encode().unwrap()).unwrap();
    });
    let connection = duplex::initiate(
        client,
        &support::CLIENT_PRIVATE,
        duplex::public_key(&support::SERVER_PRIVATE).unwrap(),
    )
    .unwrap();
    let mut control = Control::new(connection);
    trace(&mut control, Some([9; 32]));
    let reply = control
        .call(Request::Unmount(UnmountControl {
            token,
            discard: true,
        }))
        .unwrap();
    let telemetry = take_telemetry(&mut control);
    worker.join().unwrap();
    (reply, telemetry)
}

/// Stages the later boundary a retained stop reports: a failed Revoke job or an
/// already-closed overlay, as the product's own teardown sees them.
fn stage_stop(harness: &mounted::Harness, token: WorkspaceToken, stage: TeardownStage) {
    let operation = harness.service.operation(token).unwrap();
    match stage {
        TeardownStage::Revoke => {
            let owner = operation.overlay().clone();
            let route = operation.workspace().route();
            // A failed owner job poisons only this Workspace's connection.
            assert!(thread::spawn(move || {
                owner.call::<(), _>(route, ServiceClass::General, None, |_| {
                    panic!("staged owner failure before Revoke")
                })
            })
            .join()
            .unwrap()
            .is_err());
        }
        TeardownStage::Close => {
            // Revoke accepts a closed Workspace; a second Close refuses.
            operation
                .overlay()
                .close(operation.workspace().route())
                .unwrap();
        }
        _ => unreachable!("{stage:?}"),
    }
}

/// The abort path a retained Revoke or Close stop keeps, reported through a
/// traced reply: the same section a completed unmount carries, at the later
/// boundary the retained custody named.
fn retained_stop_reports_its_abort_path(stage: TeardownStage) {
    fusectl::mount();
    let fixture = installed::Fixture::new(2, &format!("unmount-observability-{stage:?}"));
    let _scratch = Scratch(fixture.directory.clone());
    let store = open_store(
        fixture.config.clone(),
        installed::BINDING,
        installed::CURSOR,
        4,
        2 * 1024 * 1024,
        ReservationBlocks::default(),
    )
    .unwrap();
    let harness = mounted::Harness::new(store.clone(), &fixture.directory, fixture.branch);
    let ready = harness.mount(1);
    let token = ready.token;
    let mut copy = Copy::spawn(Path::new(&ready.directory));
    stage_stop(&harness, token, stage);

    let (reply, telemetry) = traced_unmount(harness.service.clone(), token);
    let left = copy.leave();
    drop(copy);
    harness.stop_retained();
    drop(store);

    assert!(left, "the copy did not exit by itself");
    let Reply::Retained(custody) = &reply else {
        panic!("the retained stop answered {reply:?}")
    };
    assert_eq!(custody.token, token);
    assert_eq!(custody.stage, stage);
    assert_eq!(telemetry.len(), 1, "{telemetry:?}");
    let section = telemetry[0]
        .get("unmount")
        .expect("the retained stop's telemetry section")
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
    assert_eq!(
        section.get_text("stage"),
        Some(format!("{stage:?}").as_str())
    );
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
}

#[test]
fn a_retained_revoke_stop_reports_its_abort_path_in_a_traced_reply() {
    retained_stop_reports_its_abort_path(TeardownStage::Revoke);
}

#[test]
fn a_retained_close_stop_reports_its_abort_path_in_a_traced_reply() {
    retained_stop_reports_its_abort_path(TeardownStage::Close);
}
