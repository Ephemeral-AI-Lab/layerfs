//! Daemon observations (sent as the engine protocol carries them) preserve
//! ordinary effects, and the serial low-water reaches the served Store.
#![cfg(target_os = "linux")]
#[path = "../support/daemon_binary.rs"]
mod daemon_binary;
#[allow(dead_code)]
#[path = "../../../../../engine/crates/layerfs-daemon/tests/support/install/host.rs"]
mod support;
use layerfs_history::WorkspaceId;
use layerfs_protocol::daemon::{Reply, Request};
use layerfs_protocol::{
    duplex,
    provision::{
        daemon_setup::{DaemonLimits, DaemonSetup},
        manifest::StoreManifest,
    },
};
#[cfg(target_os = "linux")]
use layerfs_protocol::{DaemonStore, KernelRoute};
use layerfs_sandbox::internal::Control;
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    net::{SocketAddr, TcpStream},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

/// One Status of `token` sent as an observation under `scope`, as the
/// engine's protocol carries it: the daemon logs it as one, and its reply
/// is the Status itself.
fn observed_status(
    control: &mut Control,
    scope: [u8; 32],
    token: layerfs_protocol::daemon::WorkspaceToken,
) -> layerfs_protocol::daemon::WorkspaceStatus {
    let request = Request::Observed {
        scope,
        request: Box::new(Request::Status(token)),
        traced: false,
    };
    match control.call(request).unwrap() {
        Reply::Status(status) => *status,
        other => panic!("{other:?}"),
    }
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn connect(address: SocketAddr) -> duplex::Connection {
    duplex::initiate(
        support::socket(TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap()),
        &support::CLIENT_PRIVATE,
        duplex::public_key(&support::SERVER_PRIVATE).unwrap(),
    )
    .unwrap()
}
fn start(f: &support::Fixture, stderr: File) -> (Process, SocketAddr, Control) {
    let deployment = Deployment {
        overlay: f.directory.join("overlay.sqlite"),
        mounts: f.directory.join("mounts"),
        serial_low_water: 0,
        existing_store: None,
    };
    let (process, address, control, _) = start_with(f, stderr, deployment);
    (process, address, control)
}
/// What one daemon start of a test selects beside the fixture's own values.
struct Deployment {
    overlay: PathBuf,
    mounts: PathBuf,
    serial_low_water: u64,
    /// None installs the fixture's Store; Some opens the installed one.
    existing_store: Option<StoreManifest>,
}
/// The private configuration record of one start, written beside the Store.
fn configure(f: &support::Fixture, deployment: &Deployment) -> PathBuf {
    fs::set_permissions(&f.directory, fs::Permissions::from_mode(0o700)).unwrap();
    let setup = DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: support::SERVER_PRIVATE,
        control_peer: duplex::public_key(&support::CLIENT_PRIVATE).unwrap(),
        route: KernelRoute::Fuse,
        overlay: deployment.overlay.to_str().unwrap().into(),
        mounts: deployment.mounts.to_str().unwrap().into(),
        command_uid: 65534,
        command_gid: 65534,
        limits: DaemonLimits {
            connections: 4,
            handshake_ms: 3000,
            idle_ttl_ms: 60_000,
            request_deadline_ms: 60_000,
            cache_bytes: 64 * 1024,
            namespaces: 8,
            pager_kib: 1024,
            serial_low_water: deployment.serial_low_water,
        },
        store: DaemonStore::Sqlite {
            locator: f.project.manifest.locator.clone(),
            install: deployment.existing_store.clone(),
        },
    };
    let config = f.directory.join("daemon.config");
    fs::write(&config, setup.encode().unwrap()).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    config
}
/// Starts the actual executable. Returns the manifest the daemon serves.
fn start_with(
    f: &support::Fixture,
    stderr: File,
    deployment: Deployment,
) -> (Process, SocketAddr, Control, StoreManifest) {
    let config = configure(f, &deployment);
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
    // If startup stops, Process Drop closes the child's pipe. This reader's
    // exit never depends on a peer thread finishing without panicking.
    let line = receive
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    reader.join().unwrap();
    let address = line
        .strip_prefix("LAYERFS_DAEMON_LISTEN ")
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let mut connection = connect(address);
    let manifest = match deployment.existing_store {
        Some(manifest) => manifest,
        None => {
            layerfs_sandbox::internal::install(&f.project, &mut connection)
                .unwrap()
                .manifest
        }
    };
    (process, address, Control::new(connection), manifest)
}
fn number(line: &str, name: &str) -> u64 {
    line.split_once(&format!("\"{name}\":"))
        .unwrap()
        .1
        .split([',', '}'])
        .next()
        .unwrap()
        .parse()
        .unwrap()
}
fn values(line: &str) -> Vec<u64> {
    let text = line
        .split_once("\"values\":[")
        .unwrap()
        .1
        .strip_suffix("]}")
        .unwrap();
    if text.is_empty() {
        Vec::new()
    } else {
        text.split(',').map(|v| v.parse().unwrap()).collect()
    }
}
/// #352: a traced call carries its Store SQL work (the sessions' totals after
/// it minus before it), and a traced status or unmount of an attached
/// Workspace carries its kernel opcode counts since attach; a traced Commit
/// carries its delta selection work.
#[test]
fn resource_and_output_failure_preserve_original_status_reply() {
    support::shared_control();
    let f = support::Fixture::new("observed-resource-and-sink", None);
    let (process, address, mut control) =
        start(&f, File::options().write(true).open("/dev/full").unwrap());
    let bound = layerfs_sandbox::internal::workspace(&mut control)
        .bind(
            WorkspaceId::from_authority([123; 32]).unwrap(),
            f.project.branch.branch.id,
        )
        .unwrap();
    let backing = f.directory.join("overlay.sqlite");
    fs::rename(&backing, f.directory.join("original-overlay.sqlite")).unwrap();
    fs::write(&backing, b"external physical identity change").unwrap();
    let original = observed_status(&mut control, [17; 32], bound.token);
    assert_eq!(original.token, bound.token);
    assert_eq!(original.binding, bound.binding);
    // The diagnostic conversation is fenced while its exact failed observer
    // completion/output error stay held. A new ordinary observation still sees
    // the original live state and does not repeat any mutation or binding.
    let mut observer = Control::new(connect(address));
    assert_eq!(
        layerfs_sandbox::internal::workspace(&mut observer)
            .status(bound.token)
            .unwrap()
            .token,
        bound.token
    );
    assert_eq!(
        observer.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    // The backing precondition remains failed; explicit owned process stop is
    // crash scope, rather than a guessed normal unmount or corrective replay.
    drop(process);
    f.cleanup();
}

/// The Store's own count of serial reservation attempts, from the newest
/// observation in the daemon's log (section 8, fifth value).
fn serial_reservations(log: &Path) -> u64 {
    let text = fs::read_to_string(log).unwrap();
    let row = text
        .lines()
        .filter(|line| {
            line.starts_with("{\"layerfs_observation\":1,") && number(line, "section") == 8
        })
        .next_back()
        .expect("a Store row in the observation");
    values(row)[4]
}
/// Never leaves a kernel mount behind: one lazy umount(8), launched and
/// reaped here, of a directory that is still a mount point.
struct MountGuard(String);
impl Drop for MountGuard {
    fn drop(&mut self) {
        let mounted = fs::read_to_string("/proc/self/mountinfo")
            .unwrap()
            .lines()
            .any(|line| line.split(' ').nth(4) == Some(self.0.as_str()));
        if mounted {
            let done = Command::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "LOW_WATER_GUARD lazy umount of {}: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// Owner ruling P-1 through the actual executable (R8b track P-A): the
/// serial low-water of the private configuration record reaches the Store
/// the daemon serves, on both startup routes. With 1,023 configured, the
/// second create of a mounted Workspace makes one early reservation; with 0
/// it makes none. Creates are ordinary processes of the command identity.
#[test]
fn configured_serial_low_water_reaches_a_mounted_create_on_both_startup_routes() {
    support::shared_control();
    use std::os::unix::{fs::MetadataExt, process::CommandExt};
    const LOW: u64 = layerfs_vfs::SERIAL_REFILL - 1;
    let f = support::Fixture::new("observed-low-water", None);
    // The fixture directory is private to the daemon; the command identity
    // must reach the mount home, so it is beside it.
    let mounts = std::env::temp_dir().join(format!(
        "layerfs-observed-low-water-mounts-{}",
        std::process::id()
    ));
    let mut manifest: Option<StoreManifest> = None;
    let mut seen = Vec::new();
    for (run, route, low_water) in [
        (0u8, "install", LOW),
        (1, "existing", LOW),
        (2, "existing", 0),
    ] {
        let log = f.directory.join(format!("stderr-{run}.log"));
        let local = f.directory.join(format!("overlay-{run}"));
        fs::create_dir(&local).unwrap();
        fs::set_permissions(&local, fs::Permissions::from_mode(0o700)).unwrap();
        let (process, _, mut control, served) = start_with(
            &f,
            File::create(&log).unwrap(),
            Deployment {
                overlay: local.join("template.sqlite"),
                mounts: mounts.clone(),
                serial_low_water: low_water,
                existing_store: manifest.clone(),
            },
        );
        assert_eq!(manifest.is_some(), route == "existing");
        assert!(served.daemon_sqlite.is_some());
        manifest = Some(served);
        let bound = layerfs_sandbox::internal::workspace(&mut control)
            .bind(
                WorkspaceId::from_authority([140 + run; 32]).unwrap(),
                f.project.branch.branch.id,
            )
            .unwrap();
        let ready = layerfs_sandbox::internal::workspace(&mut control)
            .attach(bound.token)
            .unwrap();
        let guard = MountGuard(ready.directory.clone());
        let observe = |control: &mut Control| {
            observed_status(control, [21; 32], bound.token);
            serial_reservations(&log)
        };
        let mut counts = vec![observe(&mut control)];
        let mut serials = Vec::new();
        for name in ["first.txt", "second.txt", "third.txt"] {
            let created = Command::new("bash")
                .arg("-c")
                .arg(format!(": > {name}"))
                .current_dir(&ready.directory)
                .uid(65534)
                .gid(65534)
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert!(
                created.status.success(),
                "{route} low_water={low_water} create {name}: {:?} {:?}",
                created.status,
                String::from_utf8_lossy(&created.stderr)
            );
            let made = fs::symlink_metadata(Path::new(&ready.directory).join(name)).unwrap();
            assert!(made.is_file() && made.len() == 0 && made.uid() == 65534);
            // A created name's kernel inode number is its serial plus one.
            serials.push(made.ino() - 1);
            counts.push(observe(&mut control));
        }
        println!(
            "LOW_WATER application route={route} configured={low_water}: store serial_reservations after 0..3 creates={counts:?} serials={serials:?}"
        );
        // Unmount is admitted on a quiet connection: observe, bounded.
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut last = None;
        loop {
            let work = layerfs_sandbox::internal::workspace(&mut control)
                .status(bound.token)
                .unwrap()
                .kernel
                .unwrap()
                .work
                .unwrap();
            let now = (work.active, work.completed);
            if work.active == 0 && last == Some(now) {
                break;
            }
            last = Some(now);
            assert!(
                std::time::Instant::now() < deadline,
                "the connection did not become quiet: {work:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
        layerfs_sandbox::internal::workspace(&mut control)
            .unmount(bound.token, false)
            .unwrap();
        drop(guard);
        assert_eq!(
            control.call(Request::EndSession).unwrap(),
            Reply::SessionEnded
        );
        drop(process);
        assert_eq!(
            serials,
            [serials[0], serials[0] + 1, serials[0] + 2],
            "each create consumed the next serial of the first window"
        );
        seen.push((route, low_water, counts));
    }
    // The mount reserves the first window before readiness, so the first
    // create reserves nothing. Below 1,023 the second makes the one early
    // reservation and the third makes none; with 0 no create inside the
    // window reserves.
    assert_eq!(
        seen,
        [
            ("install", LOW, vec![1, 1, 2, 2]),
            ("existing", LOW, vec![1, 1, 2, 2]),
            ("existing", 0, vec![1, 1, 1, 1]),
        ]
    );
    fs::remove_dir(&mounts).unwrap();
    f.cleanup();
}
/// Decision L-3 of the R8b plan: a serial low-water at or above the refill
/// window is invalid configuration. The executable refuses it before it
/// listens or creates its engine; the largest value below the window is the
/// one the test above starts with.
#[test]
fn startup_refuses_a_serial_low_water_at_or_above_the_refill_window() {
    support::shared_control();
    let f = support::Fixture::new("observed-low-water-refused", None);
    let overlay = f.directory.join("overlay.sqlite");
    for refused in [
        layerfs_vfs::SERIAL_REFILL,
        layerfs_vfs::SERIAL_REFILL + 1,
        u64::MAX,
    ] {
        let config = configure(
            &f,
            &Deployment {
                overlay: overlay.clone(),
                mounts: f.directory.join("mounts"),
                serial_low_water: refused,
                existing_store: None,
            },
        );
        let log = f.directory.join("refused.log");
        let mut process = Process(
            Command::new(daemon_binary::path())
                .arg("--config")
                .arg(&config)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::from(File::create(&log).unwrap()))
                .spawn()
                .unwrap(),
        );
        // The refusal is the process's own exit: observed, bounded.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = process.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the daemon did not refuse serial_low_water={refused}"
            );
            thread::sleep(Duration::from_millis(2));
        };
        let mut stdout = String::new();
        process
            .0
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut stdout)
            .unwrap();
        let stderr = fs::read_to_string(&log).unwrap();
        println!(
            "LOW_WATER startup configured={refused}: status={status:?} stdout={stdout:?} stderr={:?}",
            stderr.trim()
        );
        assert_eq!(status.code(), Some(1));
        assert!(stdout.is_empty(), "the daemon listened: {stdout:?}");
        assert_eq!(
            stderr.trim(),
            "daemon I/O: serial low-water must be below the refill window"
        );
        assert!(!overlay.exists(), "an engine was created");
    }
    f.cleanup();
}
