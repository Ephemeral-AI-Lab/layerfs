//! A machine's own checks (`machine.doctor`'s ten frozen ids) through the
//! doctor frame, against saved records, a stand-in `docker` and stand-in
//! workers: a Docker Engine that does not answer is `engine_down` with the
//! fix that starts Docker; a stopped machine gets no worker checks; an
//! outdated worker's fix is `stop && start`, the stop forced and the
//! Sandboxes counted while some are recorded on it; a running or
//! unresolved operation stops the probes; a nearly full data disk is a
//! warning. Nothing is written to the home.
#![cfg(all(unix, feature = "machine"))]
#[path = "../support/doctor_own.rs"]
mod doctor_own;
use doctor_own::{outcome, own_within, row};
use layerfs_home::disk::{self, paths, write_entity, write_record};
use layerfs_inspect::{Kind, Options};
use layerfs_machine::record::machine::MachineRecord;
use layerfs_machine::route::{read_frame_within, write_frame, MAX_HELLO, PROTOCOL};
use layerfs_protocol::sdk::SdkValue;
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    /// The home's id, which a built-in machine's first record is made with.
    id: String,
    path: Option<std::ffi::OsString>,
    /// A stand-in Engine socket that answers pings, named by DOCKER_HOST,
    /// so the docker probe never reaches this host's own Docker.
    engine: Option<(Arc<AtomicBool>, std::thread::JoinHandle<()>)>,
    docker_host: Option<std::ffi::OsString>,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("lfs-doctor-machine-target-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let home = root.join("home");
        let id = disk::ensure(&home).unwrap().id;
        let previous = std::env::var_os("PATH");
        std::env::set_var("PATH", root.join("bin"));
        // only the stand-ins: no fallback folder after PATH
        std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
        let socket = root.join("e.sock");
        let _ = std::fs::remove_file(&socket);
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let engine = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let until = Instant::now() + Duration::from_secs(90);
            while !stopping.load(Ordering::Relaxed) && Instant::now() < until {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                        let mut request = [0u8; 256];
                        let _ = stream.read(&mut request);
                        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\nOK");
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(1)),
                }
            }
        });
        let docker_host = std::env::var_os("DOCKER_HOST");
        std::env::set_var("DOCKER_HOST", format!("unix://{}", socket.display()));
        Self {
            root,
            home,
            id,
            path: previous,
            engine: Some((stop, engine)),
            docker_host,
        }
    }
    fn script(&self, body: &str) {
        let script = format!(
            "#!/bin/sh\necho \"$*\" >> '{}'/calls\ncase \"$*\" in {body} *) exit 71;; esac\n",
            self.root.display()
        );
        let file = self.root.join("bin/docker");
        std::fs::write(&file, script).unwrap();
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    /// An Engine that answers, with the worker container in `state`.
    fn provider(&self, state: &str) {
        self.script(&format!(
            "*version*) echo 29.0;; *inspect*) printf '{state}\\tmachine-worker\\n';;"
        ));
    }
    /// An Engine that answers and holds no worker container.
    fn provider_missing(&self) {
        self.script("*version*) echo 29.0;; *inspect*) echo 'Error: No such object' >&2; exit 1;;");
    }
    /// An Engine that does not answer: Docker is not running.
    fn engine_down(&self) {
        self.script("*version*) echo 'Cannot connect to the Docker daemon' >&2; exit 1;;");
    }
    fn report(&self, name: &str, timeout: Duration) -> SdkValue {
        let before = contents(&self.home);
        let shown = own_within(
            &self.home,
            Kind::Machine,
            name,
            &Options::default(),
            Some(timeout),
        );
        assert_eq!(
            before,
            contents(&self.home),
            "diagnosis modified saved home state"
        );
        shown
    }
    fn calls(&self) -> Vec<u8> {
        std::fs::read(self.root.join("calls")).unwrap_or_default()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some((stop, engine)) = self.engine.take() {
            stop.store(true, Ordering::Relaxed);
            let _ = engine.join();
        }
        match &self.docker_host {
            Some(host) => std::env::set_var("DOCKER_HOST", host),
            None => std::env::remove_var("DOCKER_HOST"),
        }
        match &self.path {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn contents(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(contents(&path));
        } else {
            result.insert(path.clone(), std::fs::read(path).unwrap());
        }
    }
    result
}
fn is(report: &SdkValue, id: &str, value: Option<&str>) {
    assert_eq!(outcome(report, id).as_deref(), value, "{id}: {report:?}");
}
/// The finding of check `id`: its severity, message and fix.
fn finding(report: &SdkValue, id: &str) -> (String, String, String) {
    let found = row(report, id).unwrap_or_else(|| panic!("no {id}: {report:?}"));
    let text = |key: &str| found.get_text(key).unwrap_or("").to_owned();
    (text("severity"), text("message"), text("fix"))
}
/// The ids the report's rows name, in order.
fn ids(report: &SdkValue) -> Vec<String> {
    doctor_own::rows(report)
        .iter()
        .map(|row| row.get_text("id").unwrap_or("").to_owned())
        .collect()
}
/// A machine's budget here: long enough for the stand-in's few commands
/// on a loaded host; a report waits only as long as its probes take.
const SECOND: Duration = Duration::from_secs(5);

// One test owns PATH for the entire fixture matrix; provider child commands are recorded.
#[test]
fn machine_checks_report_in_order_and_preserve_the_home() {
    let f = Fixture::new();
    is(&f.report("missing", SECOND), "machine.record", Some("fail"));
    // Native uses the host and its optional worker.
    MachineRecord::built_in(layerfs_home::names::HOST_MACHINE, &f.id)
        .write(&f.home)
        .unwrap();
    let native = f.report(layerfs_home::names::HOST_MACHINE, SECOND);
    let mut native_checks = vec![
        "machine.record",
        "machine.operation",
        "machine.state",
        "machine.worker.endpoint",
        "machine.worker.retained",
    ];
    let host_network = cfg!(target_os = "linux") && nix::unistd::Uid::effective().is_root();
    if host_network {
        native_checks.push("machine.network");
    }
    assert_eq!(ids(&native), native_checks);
    is(&native, "machine.worker.endpoint", Some("pass"));
    if !host_network || outcome(&native, "machine.network").as_deref() == Some("pass") {
        assert_eq!(json(&native), r#"{"ok":true}"#);
    }
    // Docker is not running: engine_down, with the fix that starts it
    MachineRecord::built_in("docker", &f.id)
        .write(&f.home)
        .unwrap();
    f.engine_down();
    let down = f.report("docker", SECOND);
    let (severity, _, fix) = finding(&down, "machine.provider");
    assert_eq!(severity, "error");
    assert_eq!(fix, layerfs_machine::status::state::start_docker());
    assert_eq!(
        ids(&down),
        [
            "machine.record",
            "machine.operation",
            "machine.configuration",
            "machine.provider"
        ]
    );
    // a never-started docker has no container: stopped, as its record says
    f.provider_missing();
    let never = f.report("docker", SECOND);
    assert_eq!(
        ids(&never),
        [
            "machine.record",
            "machine.operation",
            "machine.configuration",
            "machine.provider",
            "machine.state"
        ],
        "{never:?}"
    );
    assert_eq!(json(&never), r#"{"ok":true}"#, "stopped is not a finding");
    // its container exited while its record says it runs: the record is
    // out of date, and a live read records what is seen
    MachineRecord::built_in("docker", &f.id)
        .with("state", SdkValue::text("running"))
        .write(&f.home)
        .unwrap();
    f.provider("exited");
    let stale = f.report("docker", SECOND);
    is(&stale, "machine.provider", Some("pass"));
    let (severity, message, fix) = finding(&stale, "machine.state");
    assert_eq!(severity, "warning");
    assert!(message.contains("stopped"), "{message}");
    assert_eq!(fix, "layerfs machine get docker --with live");
    is(&stale, "machine.worker.endpoint", None);
    // a dead container
    f.provider("dead");
    let (severity, _, fix) = finding(&f.report("docker", SECOND), "machine.provider");
    assert_eq!(
        (severity.as_str(), fix.as_str()),
        ("error", "layerfs machine start docker")
    );
    // running, and no worker saved
    f.provider("running");
    let unsaved = f.report("docker", SECOND);
    let (_, _, fix) = finding(&unsaved, "machine.worker.endpoint");
    assert_eq!(fix, "layerfs machine start docker");
    is(&unsaved, "machine.worker.auth", None);
    write_record(
        &paths::worker_record(&f.home, "docker"),
        &SdkValue::map([("address", SdkValue::text("malformed"))]),
    )
    .unwrap();
    is(
        &f.report("docker", SECOND),
        "machine.worker.endpoint",
        Some("fail"),
    );
    // a worker of this build whose data disk is nearly full
    let server = worker(&f.home, "answer");
    let running = f.report("docker", SECOND);
    server.join().unwrap();
    assert_eq!(ids(&running).len(), 11, "{running:?}");
    assert_eq!(
        ids(&running),
        [
            "machine.record",
            "machine.operation",
            "machine.configuration",
            "machine.provider",
            "machine.state",
            "machine.worker.endpoint",
            "machine.worker.auth",
            "machine.worker.compatibility",
            "machine.worker.ping",
            "machine.network",
            "machine.capacity"
        ],
        "{running:?}"
    );
    is(&running, "machine.worker.ping", Some("pass"));
    // the Sandbox network is read on a connection of its own: this
    // stand-in answers one request, so the read has no verdict
    is(&running, "machine.network", Some("unknown"));
    let (severity, message, fix) = finding(&running, "machine.capacity");
    assert_eq!(severity, "warning");
    assert!(message.starts_with("its data disk has "), "{message}");
    // v19: the prune is the fix only when the image listing names bytes it
    // frees; this stand-in worker answers one request, so the listing is
    // not known and the evidence says so
    assert_eq!(fix, "", "no prune fix without reclaimable bytes");
    let evidence = doctor_own::row(&running, "machine.capacity")
        .and_then(|row| row.get("evidence").cloned())
        .unwrap_or_default();
    assert_eq!(
        evidence.get("reclaimable_image_bytes"),
        Some(&SdkValue::Null),
        "{evidence:?}"
    );
    assert_eq!(evidence.get_int("disk_free_bytes"), Some(1234));
    // a link no Sandbox of the home owns is a leftover, with its delete fix
    let server = worker(&f.home, "network");
    let leftover = f.report("docker", SECOND);
    server.join().unwrap();
    let (severity, message, fix) = finding(&leftover, "machine.network");
    assert_eq!(severity, "warning", "{leftover:?}");
    assert!(message.contains("lfs00000000000f"), "{message}");
    assert!(fix.contains("ip link delete lfs00000000000f"), "{fix}");
    // but with a Sandbox record that cannot be read, no link can be told a
    // leftover: no verdict, and never a delete fix (the link may be that
    // Sandbox's, live); `owned_in`'s unit test checks the reason it names
    let unreadable = paths::sandbox_record(&f.home, "unreadable");
    std::fs::create_dir_all(unreadable.parent().unwrap()).unwrap();
    std::fs::write(&unreadable, "{not json").unwrap();
    let server = worker(&f.home, "network");
    let unread = f.report("docker", SECOND);
    server.join().unwrap();
    is(&unread, "machine.network", Some("unknown"));
    let printed = json(&unread);
    assert!(!printed.contains("ip link delete"), "{printed}");
    std::fs::remove_dir_all(paths::sandbox_dir(&f.home, "unreadable")).unwrap();
    // a worker of another build: stop and start, no request sent
    let server = worker(&f.home, "protocol");
    let old = f.report("docker", Duration::from_millis(250));
    server.join().unwrap();
    let (severity, message, fix) = finding(&old, "machine.worker.compatibility");
    assert_eq!(severity, "error");
    assert_eq!(message, "worker is from another build");
    assert_eq!(
        fix,
        "layerfs machine stop docker && layerfs machine start docker"
    );
    is(&old, "machine.worker.ping", None);
    // with a Sandbox recorded on the machine the stop is forced
    write_entity(
        &paths::sandbox_record(&f.home, "sb"),
        &SdkValue::map([
            ("name", SdkValue::text("sb")),
            ("project", SdkValue::text("shop/api")),
            ("machine", SdkValue::text("docker")),
            ("id", SdkValue::text("sb")),
        ]),
    )
    .unwrap();
    let server = worker(&f.home, "protocol");
    let forced = f.report("docker", Duration::from_millis(250));
    server.join().unwrap();
    let (_, message, fix) = finding(&forced, "machine.worker.compatibility");
    assert_eq!(message, "worker is from another build; 1 Sandbox is on it");
    assert_eq!(
        fix,
        "layerfs machine stop docker --force && layerfs machine start docker"
    );
    std::fs::remove_dir_all(paths::sandbox_dir(&f.home, "sb")).unwrap();
    for (mode, ping) in [
        ("version", "fail"),
        ("refusal", "fail"),
        ("timeout", "unknown"),
    ] {
        let server = worker(&f.home, mode);
        let report = f.report("docker", Duration::from_millis(250));
        server.join().unwrap();
        is(&report, "machine.worker.ping", Some(ping));
        is(&report, "machine.capacity", None);
    }
    // a live operation holds it: busy, and its provider is not asked
    let record = MachineRecord::of(&f.home, "docker").unwrap();
    let op = layerfs_home::record::intent::Op::begin("machine.create", "prepare", SdkValue::Null)
        .unwrap();
    record.with_op(&op).write(&f.home).unwrap();
    let asked = f.calls();
    let active = f.report("docker", SECOND);
    is(&active, "machine.operation", Some("unknown"));
    let busy = row(&active, "machine.operation").unwrap();
    assert_eq!(busy.get_text("reason"), Some("busy"));
    is(&active, "machine.provider", None);
    assert_eq!(f.calls(), asked);
    // an uncertain start whose owner is gone: a live read settles it (C13)
    let unresolved = SdkValue::map([
        ("id", SdkValue::text("op-00000000000000aa")),
        ("name", SdkValue::text("machine.start")),
        ("phase", SdkValue::text("start")),
        ("owner", SdkValue::map([("pid", SdkValue::Int(0))])),
        ("started", SdkValue::Int(1)),
        ("uncertain", SdkValue::Bool(true)),
    ]);
    let record = MachineRecord::of(&f.home, "docker").unwrap().without_op();
    record.with("op", unresolved).write(&f.home).unwrap();
    let left = f.report("docker", SECOND);
    let (severity, message, fix) = finding(&left, "machine.operation");
    assert_eq!(severity, "error");
    assert!(message.contains("is not known"), "{message}");
    assert_eq!(fix, "layerfs machine get docker --with live");
    assert_eq!(f.calls(), asked);
    let record = MachineRecord::of(&f.home, "docker").unwrap().without_op();
    record.write(&f.home).unwrap();
    // no time left before the provider is asked: no verdict
    let late = f.report("docker", Duration::ZERO);
    is(&late, "machine.provider", Some("unknown"));
    // corrupt record fields are concrete errors before any probe
    write_entity(
        &paths::machine_record(&f.home, "bad"),
        &SdkValue::map([
            ("name", SdkValue::text("bad")),
            ("platform", SdkValue::text("lima")),
            ("cpus", SdkValue::Int(-1)),
        ]),
    )
    .unwrap();
    is(&f.report("bad", SECOND), "machine.record", Some("fail"));
    // a removed platform: its record names one this build does not run
    write_entity(
        &paths::machine_record(&f.home, "fc"),
        &SdkValue::map([
            ("name", SdkValue::text("fc")),
            ("platform", SdkValue::text("firecracker")),
        ]),
    )
    .unwrap();
    let (_, _, fix) = finding(&f.report("fc", SECOND), "machine.record");
    assert_eq!(fix, "layerfs machine remove fc");
    // malformed saved state and operation metadata must not become report text
    let sensitive = "private-saved-value-must-not-be-displayed";
    for state in [
        SdkValue::text(sensitive),
        SdkValue::map([("secret", SdkValue::text(sensitive))]),
    ] {
        MachineRecord::built_in(layerfs_home::names::HOST_MACHINE, &f.id)
            .with("state", state)
            .write(&f.home)
            .unwrap();
        let report = f.report(layerfs_home::names::HOST_MACHINE, SECOND);
        is(&report, "machine.record", Some("fail"));
        assert!(!format!("{report:?}").contains(sensitive));
    }
    for state in ["running", "stopped", "failed"] {
        MachineRecord::built_in(layerfs_home::names::HOST_MACHINE, &f.id)
            .with("state", SdkValue::text(state))
            .write(&f.home)
            .unwrap();
        is(
            &f.report(layerfs_home::names::HOST_MACHINE, SECOND),
            "machine.record",
            Some("pass"),
        );
    }
    let malformed_op = SdkValue::map([
        ("id", SdkValue::map([("secret", SdkValue::text(sensitive))])),
        ("name", SdkValue::text(sensitive)),
        ("phase", SdkValue::text(sensitive)),
        (
            "started",
            SdkValue::map([("secret", SdkValue::text(sensitive))]),
        ),
        ("args", SdkValue::text(sensitive)),
        ("owner", SdkValue::map([("pid", SdkValue::Int(0))])),
        ("uncertain", SdkValue::Bool(true)),
    ]);
    MachineRecord::built_in(layerfs_home::names::HOST_MACHINE, &f.id)
        .with("op", malformed_op)
        .write(&f.home)
        .unwrap();
    let report = f.report(layerfs_home::names::HOST_MACHINE, SECOND);
    assert!(!format!("{report:?}").contains(sensitive));
    assert_eq!(f.calls(), asked);
}

fn json(report: &SdkValue) -> String {
    let clean = matches!(report.get("findings"), Some(SdkValue::List(l)) if l.is_empty())
        && matches!(report.get("not_checked"), Some(SdkValue::List(l)) if l.is_empty());
    match clean {
        true => r#"{"ok":true}"#.to_owned(),
        false => layerfs_home::json::print(report),
    }
}

/// A stand-in worker for machine `docker`, saved in its `worker.json`:
/// `answer` is this build's worker (a ping answers 1234 free bytes);
/// `network` is too, and answers the Sandbox network read on a second
/// connection with one `lfs` link its report names no Sandbox for;
/// `protocol` answers hello as another build and takes no request;
/// `version`, `refusal` and `timeout` answer the ping wrongly, refuse it
/// or never answer it.
fn worker(home: &Path, mode: &'static str) -> std::thread::JoinHandle<()> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    write_record(
        &paths::worker_record(home, "docker"),
        &SdkValue::map([
            (
                "address",
                SdkValue::text(listener.local_addr().unwrap().to_string()),
            ),
            ("secret", SdkValue::text("test-secret")),
        ]),
    )
    .unwrap();
    std::thread::spawn(move || {
        use std::io::Read;
        let deadline = Instant::now() + Duration::from_secs(3);
        let accept = || loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(error) => panic!("accept: {error}"),
            }
        };
        let hello = |stream: &mut std::net::TcpStream, protocol: i64| {
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(SECOND)).unwrap();
            stream.set_write_timeout(Some(SECOND)).unwrap();
            assert!(read_frame_within(stream, MAX_HELLO).unwrap().is_some());
            write_frame(
                stream,
                &SdkValue::map([
                    ("ready", SdkValue::Bool(true)),
                    ("protocol", SdkValue::Int(protocol)),
                ]),
            )
            .unwrap();
        };
        let mut stream = accept();
        let protocol = match mode {
            "protocol" => PROTOCOL + 1,
            _ => PROTOCOL,
        };
        hello(&mut stream, protocol);
        if mode != "protocol" {
            let request = read_frame_within(&mut stream, MAX_HELLO).unwrap().unwrap();
            assert_eq!(request.get_text("op"), Some("worker.ping"));
            let version = match mode {
                "version" => "different-version",
                _ => env!("CARGO_PKG_VERSION"),
            };
            let answer = match mode {
                "answer" | "network" | "version" => Some(SdkValue::map([(
                    "value",
                    SdkValue::map([
                        ("kind", SdkValue::text("worker")),
                        ("pid", SdkValue::Int(42)),
                        ("version", SdkValue::text(version)),
                        ("free_disk", SdkValue::Int(1234)),
                    ]),
                )])),
                "refusal" => Some(SdkValue::map([(
                    "refused",
                    SdkValue::map([
                        ("code", SdkValue::text("Denied")),
                        ("message", SdkValue::text("owned fixture refusal")),
                    ]),
                )])),
                _ => None,
            };
            if let Some(answer) = answer {
                write_frame(&mut stream, &answer).unwrap();
            }
        }
        if mode == "answer" {
            return;
        }
        if mode == "network" {
            let mut stream = accept();
            hello(&mut stream, PROTOCOL);
            let request = read_frame_within(&mut stream, MAX_HELLO).unwrap().unwrap();
            assert_eq!(request.get_text("op"), Some("network.inspect"));
            let link = SdkValue::map([
                ("link", SdkValue::text("lfs00000000000f")),
                ("addresses", SdkValue::List(Vec::new())),
                ("sandbox", SdkValue::Null),
            ]);
            let report = SdkValue::map([
                ("table", SdkValue::Bool(true)),
                ("forwarding", SdkValue::Bool(true)),
                ("links", SdkValue::List(vec![link])),
            ]);
            write_frame(&mut stream, &SdkValue::map([("value", report)])).unwrap();
            return;
        }
        // no second request and no abandoned peer after a timeout or refusal
        assert_eq!(stream.read(&mut [0; 1]).unwrap(), 0);
    })
}
