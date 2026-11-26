//! Every fail, warning and notice of the home's and a machine's own checks
//! carries redacted evidence, and a fix where one runnable line repairs that
//! cause (#159). Where the fix runs without a provider or daemon it is
//! applied and the doctor run again; where none exists the finding says so
//! with an empty `fix`. Fixtures: saved records, stand-in `docker` and
//! stand-in workers, as `doctor_host.rs` and `doctor_machine_target.rs`.
#![cfg(all(unix, feature = "machine"))]
#[path = "support/doctor_own.rs"]
mod doctor_own;
use doctor_own::{own, own_within, rows};
use layerfs_bridge::control::SdkValue;
use layerfs_control::doctor::{Kind, Options};
use layerfs_home::events::Events;
use layerfs_home::home::{self, paths, private_dir, write_entity, write_record};
use layerfs_home::names::MachineName;
use layerfs_home::names::HOST_MACHINE;
use layerfs_machine::record::MachineRecord;
use layerfs_machine::worker::{read_frame_within, write_frame, MAX_HELLO, PROTOCOL};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// The tests share PATH, DOCKER_HOST and LAYERFS_BUNDLE: one at a time.
static LOCK: Mutex<()> = Mutex::new(());
static NEXT: AtomicU64 = AtomicU64::new(0);
const SECOND: Duration = Duration::from_secs(5);
const OURS: &str = "1111111111111111111111111111111111111111";

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}
fn temp(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "lfs-fixes-hm-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn home(layout: i64) -> PathBuf {
    let home = temp("home");
    private_dir(&home).unwrap();
    write_record(
        &home.join("layerfs.json"),
        &SdkValue::map([
            ("layout", SdkValue::Int(layout)),
            ("id", t("0123456789abcdef0123456789abcdef")),
        ]),
    )
    .unwrap();
    home
}
fn host(home: &Path, commit: Option<&'static str>) -> SdkValue {
    let options = Options {
        build: layerfs_control::home::Build {
            commit,
            dirty: false,
        },
        ..Options::default()
    };
    own(home, Kind::Home, &home.display().to_string(), &options)
}
/// The findings of check `id` whose message contains `needle`.
fn found(report: &SdkValue, id: &str, needle: &str) -> SdkValue {
    let Some(SdkValue::List(all)) = report.get("findings") else {
        panic!("no findings: {report:?}")
    };
    all.iter()
        .find(|f| {
            f.get_text("check") == Some(id)
                && f.get_text("message").is_some_and(|m| m.contains(needle))
        })
        .cloned()
        .unwrap_or_else(|| panic!("no {id} finding with `{needle}`: {report:?}"))
}
fn fix(finding: &SdkValue) -> String {
    finding.get_text("fix").unwrap_or("").to_owned()
}
/// The finding's evidence holds exactly these keys, none of them a secret.
fn keys(finding: &SdkValue, expected: &[&str]) {
    let Some(SdkValue::Map(map)) = finding.get("evidence") else {
        panic!("no evidence: {finding:?}")
    };
    let mut got: Vec<&str> = map.iter().map(|(k, _)| k.as_str()).collect();
    got.sort_unstable();
    let mut want = expected.to_vec();
    want.sort_unstable();
    assert_eq!(got, want, "{finding:?}");
    let printed = layerfs_home::json::print(&finding.get("evidence").cloned().unwrap_or_default());
    assert!(!printed.contains("secret"), "{printed}");
    assert!(!printed.contains("Some("), "{printed}");
}
fn ev_text<'a>(finding: &'a SdkValue, key: &str) -> Option<&'a str> {
    finding.get("evidence").and_then(|e| e.get_text(key))
}
fn ev_int(finding: &SdkValue, key: &str) -> Option<i64> {
    finding.get("evidence").and_then(|e| e.get_int(key))
}
fn gone(report: &SdkValue, id: &str, needle: &str) -> bool {
    let Some(SdkValue::List(all)) = report.get("findings") else {
        return true;
    };
    !all.iter().any(|f| {
        f.get_text("check") == Some(id) && f.get_text("message").is_some_and(|m| m.contains(needle))
    })
}
/// Runs a fix line as the user would.
fn apply(fix: &str) {
    let done = std::process::Command::new("sh")
        .arg("-c")
        .arg(fix)
        .status()
        .unwrap();
    assert!(done.success(), "{fix}");
}
fn daemon_file(path: &Path, commit: &str, mode: u32) {
    std::fs::write(
        path,
        format!("binary layerfs-build-identity:{commit}:clean more"),
    )
    .unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

struct Env {
    _guard: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    roots: Vec<PathBuf>,
}
impl Env {
    fn new() -> Self {
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = [
            "PATH",
            "DOCKER_HOST",
            "LAYERFS_BUNDLE",
            "LAYERFS_TOOL_FALLBACKS",
            "XDG_CACHE_HOME",
        ]
        .map(|k| (k, std::env::var_os(k)));
        Self {
            _guard: guard,
            saved: saved.to_vec(),
            roots: Vec::new(),
        }
    }
    fn dir(&mut self, label: &str) -> PathBuf {
        let dir = temp(label);
        std::fs::create_dir_all(&dir).unwrap();
        self.roots.push(dir.clone());
        dir
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        for root in &self.roots {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

// ---- home.layout, home.records, home.artifacts, home.native_daemon, home.bundle

#[test]
fn home_layout_findings_carry_evidence_and_the_move_fix() {
    let mut env = Env::new();
    let foreign = home(2);
    env.roots.push(foreign.clone());
    let report = host(&foreign, None);
    let layout = found(&report, "home.layout", "not a LayerFS 2.0 home");
    assert_eq!(
        fix(&layout),
        layerfs_home::home::layout::foreign_fix(&foreign)
    );
    keys(&layout, &["path", "layout"]);
    assert_eq!(ev_text(&layout, "layout"), Some("foreign"));
    // a folder in place of layerfs.json: the OS error, no fix
    let unreadable = home(3);
    env.roots.push(unreadable.clone());
    std::fs::remove_file(unreadable.join("layerfs.json")).unwrap();
    std::fs::create_dir(unreadable.join("layerfs.json")).unwrap();
    let report = host(&unreadable, None);
    let layout = found(&report, "home.layout", "Is a directory");
    assert_eq!(fix(&layout), "");
    keys(&layout, &["path", "code"]);
}

#[test]
fn home_records_findings_carry_evidence_and_their_fix() {
    let mut env = Env::new();
    let home = home(3);
    env.roots.push(home.clone());
    // a Store on a machine with no record
    write_entity(
        &paths::store_record(&home, "orphan"),
        &SdkValue::map([("kind", t("sqlite")), ("machine", t("missing"))]),
    )
    .unwrap();
    // a stale lease, an unresolved operation and a partial copy
    write_entity(
        &paths::store_record(&home, "dead"),
        &SdkValue::map([
            ("kind", t("sqlite")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            (
                "op",
                SdkValue::map([
                    ("id", t("op-00000000000000aa")),
                    ("name", t("store.remove")),
                    ("phase", t("retire")),
                    ("owner", SdkValue::map([("pid", SdkValue::Int(0))])),
                    ("uncertain", SdkValue::Bool(true)),
                ]),
            ),
        ]),
    )
    .unwrap();
    private_dir(&paths::lease_dir(&home, "dead")).unwrap();
    let lease = paths::lease_dir(&home, "dead").join("99999999");
    std::fs::write(&lease, "1\n").unwrap();
    write_entity(
        &paths::store_record(&home, "partial"),
        &SdkValue::map([
            ("kind", t("sqlite")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ]),
    )
    .unwrap();
    let incoming = paths::store_file(&home, "partial").with_file_name("store.sqlite.incoming");
    std::fs::create_dir_all(incoming.parent().unwrap()).unwrap();
    std::fs::write(&incoming, "x").unwrap();
    // a Sandbox naming no project, and a machine whose daemon is not there
    write_entity(
        &paths::sandbox_record(&home, "sb"),
        &SdkValue::map([
            ("name", t("sb")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("id", t("sb")),
        ]),
    )
    .unwrap();
    write_entity(
        &paths::machine_record(&home, layerfs_home::names::HOST_MACHINE),
        &SdkValue::map([
            ("name", t(layerfs_home::names::HOST_MACHINE)),
            ("platform", t("host")),
            ("daemon", t("/no/such/layerfs-daemon")),
        ]),
    )
    .unwrap();
    let report = host(&home, Some(OURS));

    let orphan = found(
        &report,
        "home.records",
        "Store orphan is on machine missing",
    );
    assert_eq!(fix(&orphan), "", "no fix: both remedies are by hand");
    keys(&orphan, &["store", "machine", "machine_record"]);

    let stale = found(&report, "home.records", "stale lease");
    assert_eq!(fix(&stale), "");
    keys(&stale, &["store", "pid", "path"]);
    assert_eq!(ev_int(&stale, "pid"), Some(99999999));

    let op = found(&report, "home.records", "store dead has an unresolved");
    assert!(fix(&op).starts_with("layerfs store "), "{}", fix(&op));
    keys(&op, &["op", "phase", "op_id", "standing"]);
    assert_eq!(ev_text(&op, "standing"), Some("unresolved"));

    let partial = found(&report, "home.records", "partial copy");
    assert_eq!(fix(&partial), format!("rm {}", incoming.display()));
    keys(&partial, &["store", "path"]);

    let sandbox = found(&report, "home.records", "names no project");
    assert_eq!(fix(&sandbox), "layerfs sandbox remove sb");
    keys(&sandbox, &["sandbox", "reference", "expected"]);

    // the daemon of `native` is not there, and this build has none to record
    let daemon = found(&report, "home.records", "recorded daemon");
    assert_eq!(fix(&daemon), "", "no daemon of this build is known");
    keys(&daemon, &["machine", "daemon", "present"]);

    // The partial-copy fix leaves the observed stale lease untouched.
    apply(&fix(&partial));
    let after = host(&home, Some(OURS));
    found(&after, "home.records", "stale lease");
    assert!(lease.is_file());
    assert!(gone(&after, "home.records", "partial copy"), "{after:?}");
    found(&after, "home.records", "store dead has an unresolved");
}

#[test]
fn home_daemon_findings_name_this_builds_daemon_to_record() {
    let mut env = Env::new();
    let home = home(3);
    env.roots.push(home.clone());
    // this build's daemon, found in this os' bundle folder: the cache
    // place (XDG_CACHE_HOME), LAYERFS_BUNDLE naming the Linux bundle only
    let cache = env.dir("cache");
    std::env::set_var("XDG_CACHE_HOME", &cache);
    let bundle = layerfs_machine::bundle_dir(std::env::consts::OS).unwrap();
    std::fs::create_dir_all(&bundle).unwrap();
    let ours = bundle.join("layerfs-daemon");
    daemon_file(&ours, OURS, 0o755);
    // built-in `docker` configured with a daemon that is not there, and
    // `native` with one that is there and not executable
    let missing = home.join("docker-daemon");
    write_entity(
        &paths::machine_record(&home, "docker"),
        &SdkValue::map([
            ("name", t("docker")),
            ("platform", t("docker")),
            ("daemon", t(&missing.display().to_string())),
        ]),
    )
    .unwrap();
    let plain = home.join("native-daemon");
    daemon_file(&plain, OURS, 0o644);
    write_entity(
        &paths::machine_record(&home, layerfs_home::names::HOST_MACHINE),
        &SdkValue::map([
            ("name", t(layerfs_home::names::HOST_MACHINE)),
            ("platform", t("host")),
            ("daemon", t(&plain.display().to_string())),
        ]),
    )
    .unwrap();
    // a created machine: `machine update --daemon` is for built-ins only
    write_entity(
        &paths::machine_record(&home, "vm1"),
        &SdkValue::map([
            ("name", t("vm1")),
            ("platform", t("lima")),
            ("daemon", t("/no/such/vm-daemon")),
        ]),
    )
    .unwrap();
    // a created machine's daemon that is there and not executable
    let vm2 = home.join("vm2-daemon");
    daemon_file(&vm2, OURS, 0o644);
    write_entity(
        &paths::machine_record(&home, "vm2"),
        &SdkValue::map([
            ("name", t("vm2")),
            ("platform", t("lima")),
            ("daemon", t(&vm2.display().to_string())),
        ]),
    )
    .unwrap();
    let report = host(&home, Some(OURS));
    let plain_vm = found(&report, "home.artifacts", "vm2's daemon");
    assert_eq!(fix(&plain_vm), format!("chmod +x {}", vm2.display()));
    keys(&plain_vm, &["machine", "path", "executable"]);
    apply(&fix(&plain_vm));
    let update = format!(
        "layerfs machine update {HOST_MACHINE} --daemon {}",
        ours.display()
    );

    // only `native` runs this host's daemon: no fix names it for others
    let artifact = found(&report, "home.artifacts", "docker's daemon");
    assert_eq!(fix(&artifact), "");
    keys(&artifact, &["machine", "path", "present"]);
    let recorded = found(&report, "home.records", "machine docker's recorded daemon");
    assert_eq!(fix(&recorded), "");
    let vm = found(&report, "home.artifacts", "vm1's daemon");
    assert_eq!(fix(&vm), "", "not a built-in machine");

    let native = found(&report, "home.native_daemon", "not executable");
    assert_eq!(fix(&native), format!("chmod +x {}", plain.display()));
    apply(&fix(&native));
    let after = host(&home, Some(OURS));
    assert!(
        gone(&after, "home.native_daemon", "not executable"),
        "{after:?}"
    );

    // a daemon of another commit is recorded over by this build's
    daemon_file(&plain, "2222222222222222222222222222222222222222", 0o755);
    let report = host(&home, Some(OURS));
    let other = found(&report, "home.native_daemon", "was built from");
    assert_eq!(
        fix(&other),
        format!(
            "layerfs machine update {HOST_MACHINE} --daemon {}",
            ours.display()
        )
    );
    // with no daemon of this build to point at, there is no fix
    std::fs::remove_file(&ours).unwrap();
    let report = host(&home, Some(OURS));
    assert_eq!(
        fix(&found(&report, "home.native_daemon", "was built from")),
        ""
    );
    // a recorded daemon that is gone: this build's is recorded over it
    daemon_file(&ours, OURS, 0o755);
    std::fs::remove_file(&plain).unwrap();
    let report = host(&home, Some(OURS));
    assert_eq!(
        fix(&found(&report, "home.native_daemon", "not there")),
        update
    );
    let recorded = format!("machine {HOST_MACHINE}'s recorded daemon");
    assert_eq!(fix(&found(&report, "home.records", &recorded)), update);
}

#[test]
fn home_bundle_findings_carry_evidence_and_no_fix() {
    let mut env = Env::new();
    let home = home(3);
    env.roots.push(home.clone());
    write_entity(
        &paths::machine_record(&home, "docker"),
        &SdkValue::map([("name", t("docker")), ("platform", t("docker"))]),
    )
    .unwrap();
    let bundle = env.dir("bundle");
    std::env::set_var("LAYERFS_BUNDLE", &bundle);
    let report = host(&home, None);
    let invalid = found(&report, "home.bundle", "missing, malformed");
    assert_eq!(fix(&invalid), "", "no call installs a bundle");
    keys(
        &invalid,
        &[
            "path",
            "platform",
            "expected_version",
            "found_version",
            "manifest",
        ],
    );
    assert_eq!(ev_text(&invalid, "manifest"), Some("missing"));
    write_record(
        &bundle.join("bundle.json"),
        &SdkValue::map([
            ("version", t(env!("CARGO_PKG_VERSION"))),
            ("platform", t(&format!("linux-{}", std::env::consts::ARCH))),
            ("hash", t(&"a".repeat(64))),
        ]),
    )
    .unwrap();
    let report = host(&home, None);
    let incomplete = found(&report, "home.bundle", "lacks a program");
    assert_eq!(fix(&incomplete), "");
    keys(&incomplete, &["path", "missing"]);
}

// ---- machine.*

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    id: String,
    engine: Option<(Arc<AtomicBool>, std::thread::JoinHandle<()>)>,
}
impl Fixture {
    fn new(env: &mut Env) -> Self {
        let root = env.dir("machine");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let home = root.join("home");
        let id = home::ensure(&home).unwrap().id;
        std::env::set_var("PATH", root.join("bin"));
        std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
        let socket = root.join("e.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let engine = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let until = Instant::now() + Duration::from_secs(60);
            while !stopping.load(Ordering::Relaxed) && Instant::now() < until {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                        let _ = stream.read(&mut [0u8; 256]);
                        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\nOK");
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(1)),
                }
            }
        });
        std::env::set_var("DOCKER_HOST", format!("unix://{}", socket.display()));
        Self {
            root,
            home,
            id,
            engine: Some((stop, engine)),
        }
    }
    fn script(&self, body: &str) {
        let script = format!("#!/bin/sh\ncase \"$*\" in {body} *) exit 71;; esac\n");
        let file = self.root.join("bin/docker");
        std::fs::write(&file, script).unwrap();
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn provider(&self, state: &str) {
        self.script(&format!(
            "*version*) echo 29.0;; *inspect*) printf '{state}\\tmachine-worker\\n';;"
        ));
    }
    fn provider_missing(&self) {
        self.script("*version*) echo 29.0;; *inspect*) echo 'Error: No such object' >&2; exit 1;;");
    }
    fn report(&self, name: &str) -> SdkValue {
        own_within(
            &self.home,
            Kind::Machine,
            name,
            &Options::default(),
            Some(SECOND),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some((stop, engine)) = self.engine.take() {
            stop.store(true, Ordering::Relaxed);
            let _ = engine.join();
        }
    }
}

fn machine_record(f: &Fixture, name: &str, fields: Vec<(&str, SdkValue)>) {
    let mut all = vec![("name", t(name))];
    all.extend(fields);
    let record = SdkValue::Map(all.into_iter().map(|(k, v)| (k.to_owned(), v)).collect());
    write_entity(&paths::machine_record(&f.home, name), &record).unwrap();
}
fn remove_machine(f: &Fixture, name: &str) {
    let mut sink = |_| {};
    let mut events = Events::new(&mut sink);
    layerfs_control::machine::remove(&f.home, &mut events, &MachineName::parse(name).unwrap())
        .unwrap();
}

#[test]
fn machine_record_findings_carry_evidence_and_the_fix_that_works() {
    let mut env = Env::new();
    let f = Fixture::new(&mut env);
    // no record, not a built-in machine: creating needs a platform
    let absent = found(&f.report("nope"), "machine.record", "no record");
    assert_eq!(fix(&absent), "");
    keys(&absent, &["record", "built_in"]);
    // an unparseable record is refused by the frame's own locator before
    // any check (machine_target.rs:138 is reached through the CLI only):
    // `machine remove` reads it too and refuses the same error, so no fix
    // is named there
    // an invalid record that names a VM: remove would delete the VM and its
    // disk, so no fix
    machine_record(
        &f,
        "bad",
        vec![
            ("platform", t("lima")),
            ("vm_name", t("layerfs-bad")),
            ("cpus", SdkValue::Int(-1)),
        ],
    );
    let invalid = found(&f.report("bad"), "machine.record", "not valid");
    assert_eq!(fix(&invalid), "");
    keys(&invalid, &["problem", "path"]);
    // an invalid record with no VM: remove takes records alone
    machine_record(
        &f,
        "nameless",
        vec![("platform", t("lima")), ("cpus", SdkValue::Int(-1))],
    );
    let nameless = found(&f.report("nameless"), "machine.record", "not valid");
    assert_eq!(fix(&nameless), "layerfs machine remove nameless");
    keys(&nameless, &["problem", "path"]);
    remove_machine(&f, "nameless");
    assert!(gone(&f.report("nameless"), "machine.record", "not valid"));
    // built-in `docker` with invalid sizes: remove refuses it, so no fix
    machine_record(
        &f,
        "docker",
        vec![("platform", t("docker")), ("cpus", SdkValue::Int(-1))],
    );
    let builtin = found(&f.report("docker"), "machine.record", "not valid");
    assert_eq!(fix(&builtin), "");
    // a removed platform: remove clears it
    machine_record(&f, "fc", vec![("platform", t("firecracker"))]);
    let removed = found(&f.report("fc"), "machine.record", "firecracker");
    assert_eq!(fix(&removed), "layerfs machine remove fc");
    keys(&removed, &["platform", "code"]);
    remove_machine(&f, "fc");
    assert!(gone(&f.report("fc"), "machine.record", "firecracker"));
    // a platform no build knows: remove refuses it too
    machine_record(&f, "zz", vec![("platform", t("zzz"))]);
    let unknown = found(&f.report("zz"), "machine.record", "");
    assert_eq!(fix(&unknown), "");
    keys(&unknown, &["code"]);
}

#[test]
fn machine_state_and_endpoint_findings_carry_evidence() {
    let mut env = Env::new();
    let f = Fixture::new(&mut env);
    MachineRecord::built_in("docker", &f.id)
        .with("state", t("running"))
        .write(&f.home)
        .unwrap();
    f.provider("exited");
    let state = found(&f.report("docker"), "machine.state", "its record says");
    assert_eq!(fix(&state), "layerfs machine get docker --with live");
    keys(&state, &["recorded", "observed"]);
    assert_eq!(ev_text(&state, "observed"), Some("stopped"));
    // running, and no worker saved
    f.provider("running");
    let unsaved = found(
        &f.report("docker"),
        "machine.worker.endpoint",
        "no worker endpoint",
    );
    assert_eq!(fix(&unsaved), "layerfs machine start docker");
    keys(&unsaved, &["saved"]);
    write_record(
        &paths::worker_record(&f.home, "docker"),
        &SdkValue::map([("address", t("malformed"))]),
    )
    .unwrap();
    let unreadable = found(
        &f.report("docker"),
        "machine.worker.endpoint",
        "does not read",
    );
    assert_eq!(fix(&unreadable), "layerfs machine start docker");
    keys(&unreadable, &["saved"]);
}

#[test]
fn machine_ping_findings_carry_evidence() {
    let mut env = Env::new();
    let f = Fixture::new(&mut env);
    MachineRecord::built_in("docker", &f.id)
        .write(&f.home)
        .unwrap();
    f.provider("running");
    let server = worker(&f.home, "version");
    let report = f.report("docker");
    server.join().unwrap();
    let ping = found(&report, "machine.worker.ping", "another process or build");
    assert_eq!(
        fix(&ping),
        "layerfs machine stop docker && layerfs machine start docker"
    );
    keys(&ping, &["kind", "version"]);
    let server = worker(&f.home, "refusal");
    let report = f.report("docker");
    server.join().unwrap();
    let refused = found(&report, "machine.worker.ping", "refused a request");
    assert_eq!(fix(&refused), "layerfs machine start docker");
    keys(&refused, &["code"]);
    assert_eq!(ev_text(&refused, "code"), Some("Denied"));
}

#[test]
fn never_started_docker_is_stopped_not_a_finding() {
    // spec v6: "Stopped: stopped by machine stop, or created and never
    // started"; `machine get` says stopped, and so the doctor finds nothing
    let mut env = Env::new();
    let f = Fixture::new(&mut env);
    MachineRecord::built_in("docker", &f.id)
        .write(&f.home)
        .unwrap();
    f.provider_missing();
    let report = f.report("docker");
    assert!(
        matches!(report.get("findings"), Some(SdkValue::List(l)) if l.is_empty()),
        "{report:?}"
    );
    assert!(
        matches!(report.get("not_checked"), Some(SdkValue::List(l)) if l.is_empty()),
        "{report:?}"
    );
    let state = doctor_own::row(&report, "machine.state").unwrap();
    assert_eq!(state.get_text("outcome"), Some("pass"), "{state:?}");
    assert!(rows(&report).len() >= 5);
}

/// A stand-in worker of this build for machine `docker`: `version` answers
/// the ping as another build, `refusal` refuses it.
fn worker(home: &Path, mode: &'static str) -> std::thread::JoinHandle<()> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    write_record(
        &paths::worker_record(home, "docker"),
        &SdkValue::map([
            ("address", t(&listener.local_addr().unwrap().to_string())),
            ("secret", t("test-secret")),
        ]),
    )
    .unwrap();
    std::thread::spawn(move || {
        use std::io::Read;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(e) => panic!("accept: {e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(SECOND)).unwrap();
        stream.set_write_timeout(Some(SECOND)).unwrap();
        assert!(read_frame_within(&mut stream, MAX_HELLO).unwrap().is_some());
        write_frame(
            &mut stream,
            &SdkValue::map([
                ("ready", SdkValue::Bool(true)),
                ("protocol", SdkValue::Int(PROTOCOL)),
            ]),
        )
        .unwrap();
        let _ = read_frame_within(&mut stream, MAX_HELLO).unwrap().unwrap();
        let answer = match mode {
            "version" => SdkValue::map([(
                "value",
                SdkValue::map([
                    ("kind", t("worker")),
                    ("pid", SdkValue::Int(42)),
                    ("version", t("different-version")),
                    ("free_disk", SdkValue::Int(1234)),
                ]),
            )]),
            _ => SdkValue::map([(
                "refused",
                SdkValue::map([
                    ("code", t("Denied")),
                    ("message", t("owned fixture refusal")),
                ]),
            )]),
        };
        write_frame(&mut stream, &answer).unwrap();
        assert_eq!(stream.read(&mut [0; 1]).unwrap(), 0);
    })
}
