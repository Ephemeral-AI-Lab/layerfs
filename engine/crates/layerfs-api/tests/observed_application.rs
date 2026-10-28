//! Authentic daemon observations preserve ordinary effects and explicit identities.
#![cfg(target_os = "linux")]
#[path = "support/daemon_binary.rs"]
mod daemon_binary;
#[allow(dead_code)]
#[path = "../../layerfs-daemon/tests/support/host_install.rs"]
mod support;
use layerfs_api::internal::Control;
#[cfg(target_os = "linux")]
use layerfs_bridge::control::{CleanupObservation, ControlCode};
use layerfs_bridge::{
    control::{Reply, Request},
    daemon_setup::{DaemonLimits, DaemonSetup},
    duplex,
    provision::StoreManifest,
};
use layerfs_history::WorkspaceId;
use std::{
    collections::BTreeSet,
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
        idle_exit_s: 0,
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
    idle_exit_s: u32,
}
/// The private configuration record of one start, written beside the Store.
fn configure(f: &support::Fixture, deployment: &Deployment) -> PathBuf {
    fs::set_permissions(&f.directory, fs::Permissions::from_mode(0o700)).unwrap();
    let setup = DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: support::SERVER_PRIVATE,
        control_peer: duplex::public_key(&support::CLIENT_PRIVATE).unwrap(),
        store: f.project.manifest.locator.clone(),
        overlay: deployment.overlay.to_str().unwrap().into(),
        mounts: deployment.mounts.to_str().unwrap().into(),
        command_uid: 65534,
        command_gid: 65534,
        limits: DaemonLimits {
            connections: 4,
            handshake_ms: 3000,
            cache_bytes: 64 * 1024,
            namespaces: 8,
            pager_kib: 1024,
            serial_low_water: deployment.serial_low_water,
            idle_exit_s: deployment.idle_exit_s,
        },
        existing_store: deployment.existing_store.clone(),
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
            layerfs_api::internal::install(&f.project, &mut connection)
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
fn validate(rows: &[&str]) {
    assert!(!rows.is_empty());
    assert!(rows.len() <= 68);
    for line in rows {
        assert!(line.len() < 4096);
        assert!(!line.contains("private_key") && !line.contains("control_peer"));
        let decoded = values(line);
        if line.contains("\"available\":false") {
            assert!(decoded.is_empty());
        }
    }
    let footer = values(rows.last().unwrap());
    assert_eq!(number(rows.last().unwrap(), "section"), 99);
    assert_eq!(footer[0] as usize, rows.len() - 1);
    assert_eq!(
        footer[1] as usize,
        rows[..rows.len() - 1]
            .iter()
            .map(|line| values(line).len())
            .sum::<usize>()
    );
}
fn identity_words(line: &str, name: &str) -> [u64; 4] {
    let text = line
        .split_once(&format!("\"{name}\":["))
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0;
    text.split(',')
        .map(|value| value.parse().unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}
fn accepts_unique_completed_groups(log: &str) -> bool {
    let mut completed = BTreeSet::new();
    for line in log
        .lines()
        .filter(|line| line.starts_with("{\"layerfs_observation\":1,"))
    {
        if number(line, "section") == 99
            && !completed.insert((
                identity_words(line, "daemon"),
                identity_words(line, "scope"),
                number(line, "call"),
            ))
        {
            return false;
        }
    }
    true
}
fn task_ids(process: &Process) -> BTreeSet<u32> {
    fs::read_dir(format!("/proc/{}/task", process.0.id()))
        .unwrap()
        .map(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_str()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect()
}
#[test]
fn equal_calls_and_reused_slot_have_distinct_public_observation_identity() {
    support::shared_control();
    let f = support::Fixture::new("observed-correlation", None);
    let log = f.directory.join("stderr.log");
    let (process, address, mut initial) = start(&f, File::create(&log).unwrap());
    let bound = layerfs_api::internal::workspace(&mut initial)
        .bind(
            WorkspaceId::from_authority([111; 32]).unwrap(),
            f.project.branch.branch.id,
        )
        .unwrap();
    let before = task_ids(&process);
    let mut first = Control::new(connect(address));
    let after = task_ids(&process);
    let created: Vec<_> = after.difference(&before).copied().collect();
    assert_eq!(created.len(), 1);
    let first_thread = created[0];
    let mut second = Control::new(connect(address));
    for (channel, scope) in [(&mut first, [7; 32]), (&mut second, [8; 32])] {
        assert_eq!(
            layerfs_api::internal::with_observations(channel, &scope)
                .status(bound.token)
                .unwrap()
                .token,
            bound.token
        );
    }
    assert_eq!(
        first.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    // Readiness observation only. Worker slot becomes Vacant before this own
    // daemon thread exits; no control operation, connection or effect is retried.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let thread_path = format!("/proc/{}/task/{first_thread}", process.0.id());
    while std::path::Path::new(&thread_path).exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "ended control worker failed to exit"
        );
        thread::sleep(Duration::from_millis(1));
    }
    let mut later = Control::new(connect(address));
    assert_eq!(
        layerfs_api::internal::with_observations(&mut later, &[9; 32])
            .status(bound.token)
            .unwrap()
            .token,
        bound.token
    );
    let text = fs::read_to_string(&log).unwrap();
    assert!(accepts_unique_completed_groups(&text));
    let groups: Vec<_> = [7u8, 8, 9]
        .into_iter()
        .map(|byte| {
            let word = u64::from_be_bytes([byte; 8]);
            text.lines()
                .filter(|line| {
                    identity_words(line, "scope") == [word; 4] && number(line, "call") == 1
                })
                .collect::<Vec<_>>()
        })
        .collect();
    for rows in &groups {
        validate(rows);
        assert_eq!(number(rows[0], "index"), bound.token.namespace as u64);
        assert_eq!(
            identity_words(rows[0], "daemon"),
            identity_words(groups[0][0], "daemon")
        );
    }
    assert_ne!(number(groups[0][0], "slot"), number(groups[1][0], "slot"));
    assert_eq!(number(groups[0][0], "slot"), number(groups[2][0], "slot"));
    let mut duplicate = text.clone();
    for line in &groups[0] {
        duplicate.push_str(line);
        duplicate.push('\n');
    }
    assert!(
        !accepts_unique_completed_groups(&duplicate),
        "duplicate complete group accepted"
    );
    assert_eq!(
        second.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    assert_eq!(
        later.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    layerfs_api::internal::workspace(&mut initial)
        .unmount(bound.token, false)
        .unwrap();
    assert_eq!(
        initial.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    drop(process);
    f.cleanup();
}
#[test]
fn diagnostic_sink_failure_does_not_repeat_or_hide_successful_bind() {
    support::shared_control();
    let f = support::Fixture::new("observed-failed-sink", None);
    let (process, address, mut control) =
        start(&f, File::options().write(true).open("/dev/full").unwrap());
    let workspace = WorkspaceId::from_authority([99; 32]).unwrap();
    let bound = layerfs_api::internal::with_observations(&mut control, &[7; 32])
        .bind(workspace, f.project.branch.branch.id)
        .unwrap();
    // A fresh authenticated observer establishes existing state; it neither
    // replays the bind nor settles a previously unknown reply.
    let mut observer = Control::new(connect(address));
    assert_eq!(
        layerfs_api::internal::workspace(&mut observer)
            .status(bound.token)
            .unwrap()
            .token,
        bound.token
    );
    assert!(
        matches!(observer.call(Request::Mount { workspace, branch:f.project.branch.branch.id }).unwrap(),
        Reply::Refused(refusal) if refusal.code == ControlCode::Busy)
    );
    assert_eq!(
        layerfs_api::internal::workspace(&mut observer)
            .cleanup(bound.token)
            .unwrap(),
        CleanupObservation::Live
    );
    layerfs_api::internal::workspace(&mut observer)
        .unmount(bound.token, false)
        .unwrap();
    assert_eq!(
        observer.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    drop(process);
    f.cleanup();
}
/// The names of a traced reply's timing nodes.
fn timing_names(telemetry: &layerfs_bridge::control::SdkValue) -> Vec<String> {
    use layerfs_bridge::control::SdkValue;
    match telemetry
        .get("timing")
        .and_then(|timing| timing.get("nodes"))
    {
        Some(SdkValue::List(nodes)) => nodes
            .iter()
            .filter_map(|node| match node {
                SdkValue::List(fields) => match fields.first() {
                    Some(SdkValue::Text(name)) => Some(name.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}
#[test]
fn traced_diagnostic_failure_is_reported_and_keeps_the_connection() {
    support::shared_control();
    let f = support::Fixture::new("observed-traced-sink", None);
    let (process, _address, mut control) =
        start(&f, File::options().write(true).open("/dev/full").unwrap());
    layerfs_api::internal::trace(&mut control, Some([7; 32]));
    let bound = layerfs_api::internal::workspace(&mut control)
        .bind(
            WorkspaceId::from_authority([77; 32]).unwrap(),
            f.project.branch.branch.id,
        )
        .unwrap();
    let telemetry = layerfs_api::internal::take_telemetry(&mut control);
    assert_eq!(telemetry.len(), 1);
    let error = telemetry[0]
        .get("diagnosticError")
        .expect("reported write failure");
    assert!(error.get_text("kind").is_some() && error.get_text("message").is_some());
    let names = timing_names(&telemetry[0]);
    assert_eq!(
        names.first().map(String::as_str),
        Some("sandbox.mount.bind")
    );
    assert!(names.iter().any(|name| name == "store.bind"));
    // The same connection keeps serving, traced and then untraced.
    let status = layerfs_api::internal::workspace(&mut control)
        .status(bound.token)
        .unwrap();
    assert_eq!(status.token, bound.token);
    let telemetry = layerfs_api::internal::take_telemetry(&mut control);
    assert_eq!(telemetry.len(), 1);
    assert!(telemetry[0].get("diagnosticError").is_some());
    layerfs_api::internal::trace(&mut control, None);
    let status = layerfs_api::internal::workspace(&mut control)
        .status(bound.token)
        .unwrap();
    assert_eq!(status.binding, bound.binding);
    assert!(layerfs_api::internal::take_telemetry(&mut control).is_empty());
    layerfs_api::internal::workspace(&mut control)
        .unmount(bound.token, false)
        .unwrap();
    assert_eq!(
        control.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    drop(process);
    f.cleanup();
}
#[test]
fn traced_records_share_the_scope_and_untraced_replies_stay_plain() {
    support::shared_control();
    let f = support::Fixture::new("observed-traced-records", None);
    let log = f.directory.join("stderr.log");
    let (process, _address, mut control) = start(&f, File::create(&log).unwrap());
    let bound = layerfs_api::internal::workspace(&mut control)
        .bind(
            WorkspaceId::from_authority([78; 32]).unwrap(),
            f.project.branch.branch.id,
        )
        .unwrap();
    assert!(layerfs_api::internal::take_telemetry(&mut control).is_empty());
    layerfs_api::internal::trace(&mut control, Some([5; 32]));
    let status = layerfs_api::internal::workspace(&mut control)
        .status(bound.token)
        .unwrap();
    assert_eq!(status.token, bound.token);
    let telemetry = layerfs_api::internal::take_telemetry(&mut control);
    assert_eq!(telemetry.len(), 1);
    assert!(telemetry[0].get("diagnosticError").is_none());
    assert!(telemetry[0].get("storage").is_none());
    layerfs_api::internal::trace(&mut control, None);
    let text = fs::read_to_string(&log).unwrap();
    assert!(accepts_unique_completed_groups(&text));
    let word = u64::from_be_bytes([5; 8]);
    let rows: Vec<_> = text
        .lines()
        .filter(|line| identity_words(line, "scope") == [word; 4])
        .collect();
    validate(&rows);
    layerfs_api::internal::workspace(&mut control)
        .unmount(bound.token, false)
        .unwrap();
    assert_eq!(
        control.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    drop(process);
    f.cleanup();
}
/// One counter of a traced reply's map at `path` (`sql` › `writer`, `fuse`, …).
fn counter(telemetry: &layerfs_bridge::control::SdkValue, path: &[&str], name: &str) -> i64 {
    path.iter()
        .try_fold(telemetry, |value, step| value.get(step))
        .and_then(|group| group.get_int(name))
        .unwrap_or_else(|| panic!("{path:?}.{name} in {telemetry:?}"))
}
/// The size of a traced reply's map at `path`.
fn width(telemetry: &layerfs_bridge::control::SdkValue, path: &[&str]) -> usize {
    use layerfs_bridge::control::SdkValue;
    match path
        .iter()
        .try_fold(telemetry, |value, step| value.get(step))
    {
        Some(SdkValue::Map(fields)) => fields.len(),
        other => panic!("{path:?} is not a map: {other:?}"),
    }
}
/// #352: a traced call carries its Store SQL work (the sessions' totals after
/// it minus before it), and a traced status or unmount of an attached
/// Workspace carries its kernel opcode counts since attach; a traced Commit
/// carries its delta selection work.
#[test]
fn traced_calls_carry_sql_work_and_status_unmount_carry_opcodes() {
    support::shared_control();
    let f = support::Fixture::new("observed-traced-work", None);
    let log = f.directory.join("stderr.log");
    let (process, _address, mut control) = start(&f, File::create(&log).unwrap());
    layerfs_api::internal::trace(&mut control, Some([6; 32]));
    let bound = layerfs_api::internal::workspace(&mut control)
        .bind(
            WorkspaceId::from_authority([79; 32]).unwrap(),
            f.project.branch.branch.id,
        )
        .unwrap();
    let telemetry = layerfs_api::internal::take_telemetry(&mut control);
    assert_eq!(telemetry.len(), 1);
    let bind = &telemetry[0];
    assert_eq!(width(bind, &["sql", "writer"]), 27);
    assert_eq!(width(bind, &["sql", "readers"]), 27);
    // #367: and the request thread's own share of it.
    assert_eq!(width(bind, &["sql", "call"]), 27);
    // Bind opens the Workspace's reader: its opening statements are this call's.
    assert!(counter(bind, &["sql", "readers"], "statements") > 0);
    assert!(bind.get("fuse").is_none(), "bind carries no opcode counts");
    assert!(bind.get("delta").is_none(), "bind finishes no Save");
    let ready = layerfs_api::internal::workspace(&mut control)
        .attach(bound.token)
        .unwrap();
    let guard = MountGuard(ready.directory.clone());
    let attach = layerfs_api::internal::take_telemetry(&mut control);
    assert!(attach.iter().all(|value| value.get("fuse").is_none()));
    let file = Path::new(&ready.directory).join("traced.txt");
    fs::write(&file, b"traced").unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"traced");
    // A Commit finishes a Save: its delta selection work is the Store's Save
    // totals after it minus before it.
    layerfs_api::internal::workspace(&mut control)
        .commit(bound.token)
        .unwrap();
    let commit = layerfs_api::internal::take_telemetry(&mut control);
    assert_eq!(commit.len(), 1);
    assert_eq!(width(&commit[0], &["delta", "payload"]), 11);
    assert_eq!(width(&commit[0], &["delta", "metadata"]), 7);
    assert!(counter(&commit[0], &["delta", "metadata"], "leaves") > 0);
    // Unmount is admitted on a quiet connection: observe untraced, bounded.
    layerfs_api::internal::trace(&mut control, None);
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    let mut last = None;
    loop {
        let work = layerfs_api::internal::workspace(&mut control)
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
    layerfs_api::internal::trace(&mut control, Some([6; 32]));
    layerfs_api::internal::workspace(&mut control)
        .status(bound.token)
        .unwrap();
    let status = layerfs_api::internal::take_telemetry(&mut control);
    assert_eq!(status.len(), 1);
    let status = &status[0];
    // 39 opcode counts, 10 counters, and the `ns` and `phases` maps.
    assert_eq!(width(status, &["fuse"]), 39 + 10 + 2);
    for name in ["lookup", "create", "write", "release", "served"] {
        assert!(counter(status, &["fuse"], name) > 0, "{name}");
    }
    layerfs_api::internal::workspace(&mut control)
        .unmount(bound.token, false)
        .unwrap();
    let unmount = layerfs_api::internal::take_telemetry(&mut control);
    assert_eq!(unmount.len(), 1);
    // Counts are since attach: the final ones are at least the status ones.
    for name in ["lookup", "write", "served"] {
        assert!(counter(&unmount[0], &["fuse"], name) >= counter(status, &["fuse"], name));
    }
    assert!(width(&unmount[0], &["sql"]) > 0);
    drop(guard);
    layerfs_api::internal::trace(&mut control, None);
    assert_eq!(
        control.call(Request::EndSession).unwrap(),
        Reply::SessionEnded
    );
    drop(process);
    f.cleanup();
}
#[test]
fn resource_and_output_failure_preserve_original_status_reply() {
    support::shared_control();
    let f = support::Fixture::new("observed-resource-and-sink", None);
    let (process, address, mut control) =
        start(&f, File::options().write(true).open("/dev/full").unwrap());
    let bound = layerfs_api::internal::workspace(&mut control)
        .bind(
            WorkspaceId::from_authority([123; 32]).unwrap(),
            f.project.branch.branch.id,
        )
        .unwrap();
    let backing = f.directory.join("overlay.sqlite");
    fs::rename(&backing, f.directory.join("original-overlay.sqlite")).unwrap();
    fs::write(&backing, b"external physical identity change").unwrap();
    let original = layerfs_api::internal::with_observations(&mut control, &[17; 32])
        .status(bound.token)
        .unwrap();
    assert_eq!(original.token, bound.token);
    assert_eq!(original.binding, bound.binding);
    // The diagnostic conversation is fenced while its exact failed observer
    // completion/output error stay held. A new ordinary observation still sees
    // the original live state and does not repeat any mutation or binding.
    let mut observer = Control::new(connect(address));
    assert_eq!(
        layerfs_api::internal::workspace(&mut observer)
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
    const LOW: u64 = layerfs_workspace::SERIAL_REFILL - 1;
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
                idle_exit_s: 0,
            },
        );
        assert_eq!(manifest.is_some(), route == "existing");
        assert!(served.daemon_sqlite.is_some());
        manifest = Some(served);
        let bound = layerfs_api::internal::workspace(&mut control)
            .bind(
                WorkspaceId::from_authority([140 + run; 32]).unwrap(),
                f.project.branch.branch.id,
            )
            .unwrap();
        let ready = layerfs_api::internal::workspace(&mut control)
            .attach(bound.token)
            .unwrap();
        let guard = MountGuard(ready.directory.clone());
        let observe = |control: &mut Control| {
            layerfs_api::internal::with_observations(control, &[21; 32])
                .status(bound.token)
                .unwrap();
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
            let work = layerfs_api::internal::workspace(&mut control)
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
        layerfs_api::internal::workspace(&mut control)
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
        layerfs_workspace::SERIAL_REFILL,
        layerfs_workspace::SERIAL_REFILL + 1,
        u64::MAX,
    ] {
        let config = configure(
            &f,
            &Deployment {
                overlay: overlay.clone(),
                mounts: f.directory.join("mounts"),
                serial_low_water: refused,
                existing_store: None,
                idle_exit_s: 0,
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

/// #373 idle close: a daemon ends itself once it has held no Workspace and
/// run no call for its idle period. A bound Workspace keeps it however long
/// it stays; an open connection does not.
#[test]
fn an_idle_daemon_exits_and_a_workspace_keeps_it() {
    support::shared_control();
    let f = support::Fixture::new("observed-idle-exit", None);
    let log = f.directory.join("stderr.log");
    let mounts = f.directory.join("mounts");
    fs::create_dir(&mounts).unwrap();
    let (mut process, _address, mut control, _) = start_with(
        &f,
        File::create(&log).unwrap(),
        Deployment {
            overlay: f.directory.join("overlay.sqlite"),
            mounts: mounts.clone(),
            serial_low_water: 0,
            existing_store: None,
            idle_exit_s: 2,
        },
    );
    let bound = layerfs_api::internal::workspace(&mut control)
        .bind(
            WorkspaceId::from_authority([81; 32]).unwrap(),
            f.project.branch.branch.id,
        )
        .unwrap();
    thread::sleep(Duration::from_secs(4));
    assert!(
        process.0.try_wait().unwrap().is_none(),
        "a Workspace keeps the daemon"
    );
    layerfs_api::internal::workspace(&mut control)
        .unmount(bound.token, false)
        .unwrap();
    let unmounted = std::time::Instant::now();
    // The connection stays open; the daemon still ends within its period
    // and the one-second check, with a bounded wait here.
    let deadline = unmounted + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = process.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the idle daemon did not exit"
        );
        thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "{status:?}");
    // The period counts from the last check that saw the Workspace, at
    // most one check before the unmount.
    let waited = unmounted.elapsed();
    println!("IDLE exit {waited:?} after the unmount");
    assert!(waited >= Duration::from_secs(1), "{waited:?}");
    assert!(fs::read_to_string(&log)
        .unwrap()
        .contains("LAYERFS_DAEMON_IDLE_EXIT after 2 s"));
    drop(control);
    drop(process);
    fs::remove_dir(&mounts).unwrap();
    f.cleanup();
}
