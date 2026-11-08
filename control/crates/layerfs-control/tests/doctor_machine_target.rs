//! Target-family verification against saved records, stand-in providers and authenticated workers.
#![cfg(all(unix, feature = "machine"))]
mod target {
    pub use layerfs_control::doctor::target::{Budget, Checks};
}
#[path = "../src/doctor/machine_target.rs"]
mod machine_target;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, paths, write_entity, write_record};
use layerfs_machine::record::MachineRecord;
use layerfs_machine::worker::{read_frame_within, write_frame, MAX_HELLO, PROTOCOL};
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    path: Option<std::ffi::OsString>,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("lfs-doctor-machine-target-{}", std::process::id()));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let home = root.join("home");
        home::ensure(&home).unwrap();
        let previous = std::env::var_os("PATH");
        std::env::set_var("PATH", root.join("bin"));
        Self {
            root,
            home,
            path: previous,
        }
    }
    fn provider(&self, state: &str) {
        let script = format!("#!/bin/sh\necho \"$*\" >> '{}'/calls\ncase \"$*\" in *version*) echo 29.0;; *inspect*) printf '{}\\tmachine-worker\\n';; *) exit 71;; esac\n", self.root.display(), state);
        let file = self.root.join("bin/docker");
        std::fs::write(&file, script).unwrap();
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn report(&self, name: &str, timeout: Duration) -> SdkValue {
        let before = contents(&self.home);
        let mut checks = target::Checks::new("machine", name);
        machine_target::diagnose(
            &self.home,
            name,
            &target::Budget::new(Some(timeout)),
            &mut checks,
        );
        assert_eq!(
            before,
            contents(&self.home),
            "diagnosis modified saved home state"
        );
        checks.to_sdk()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
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
fn check<'a>(report: &'a SdkValue, id: &str) -> &'a SdkValue {
    let Some(SdkValue::List(checks)) = report.get("checks") else {
        panic!("missing checks")
    };
    checks
        .iter()
        .find(|row| row.get_text("id") == Some(id))
        .unwrap_or_else(|| panic!("missing {id}: {report:?}"))
}
fn outcome(report: &SdkValue, id: &str, value: &str) {
    assert_eq!(
        check(report, id).get_text("outcome"),
        Some(value),
        "{report:?}"
    );
}

// One test owns PATH for the entire fixture matrix; provider child commands are recorded.
#[test]
fn machine_target_reports_dependencies_and_preserves_home() {
    let f = Fixture::new();
    outcome(
        &f.report("missing", Duration::from_secs(1)),
        "machine.record",
        "fail",
    );
    MachineRecord::built_in("native").write(&f.home).unwrap();
    let native = f.report("native", Duration::from_secs(1));
    #[cfg(target_os = "macos")]
    outcome(&native, "machine.provider", "pass");
    #[cfg(target_os = "macos")]
    outcome(&native, "machine.worker", "skipped");
    MachineRecord::built_in("docker").write(&f.home).unwrap();
    outcome(
        &f.report("docker", Duration::from_secs(1)),
        "machine.provider",
        "fail",
    );
    f.provider("exited");
    let stopped = f.report("docker", Duration::from_secs(1));
    outcome(&stopped, "machine.worker.auth", "skipped");
    f.provider("running");
    let missing_endpoint = f.report("docker", Duration::from_secs(1));
    outcome(&missing_endpoint, "machine.worker.endpoint", "fail");
    outcome(&missing_endpoint, "machine.worker.auth", "blocked");
    write_record(
        &paths::worker_record(&f.home, "docker"),
        &SdkValue::map([("address", SdkValue::text("malformed"))]),
    )
    .unwrap();
    outcome(
        &f.report("docker", Duration::from_secs(1)),
        "machine.worker.endpoint",
        "fail",
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    write_record(
        &paths::worker_record(&f.home, "docker"),
        &SdkValue::map([
            (
                "address",
                SdkValue::text(listener.local_addr().unwrap().to_string()),
            ),
            ("secret", SdkValue::text("test-secret")),
        ]),
    )
    .unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut stream = loop {
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
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        assert!(read_frame_within(&mut stream, MAX_HELLO).unwrap().is_some());
        write_frame(
            &mut stream,
            &SdkValue::map([
                ("ready", SdkValue::Bool(true)),
                ("protocol", SdkValue::Int(PROTOCOL)),
            ]),
        )
        .unwrap();
        assert_eq!(
            read_frame_within(&mut stream, MAX_HELLO)
                .unwrap()
                .unwrap()
                .get_text("op"),
            Some("worker.ping")
        );
        write_frame(
            &mut stream,
            &SdkValue::map([(
                "value",
                SdkValue::map([
                    ("kind", SdkValue::text("worker")),
                    ("pid", SdkValue::Int(42)),
                    ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
                    ("free_disk", SdkValue::Int(1234)),
                ]),
            )]),
        )
        .unwrap();
    });
    let running = f.report("docker", Duration::from_secs(1));
    outcome(&running, "machine.worker.ping", "pass");
    outcome(&running, "machine.capacity", "pass");
    assert_eq!(running.get("complete"), Some(&SdkValue::Bool(true)));
    server.join().unwrap();
    let record = MachineRecord::of(&f.home, "docker").unwrap();
    let op = layerfs_home::op::Op::begin("machine.create", "prepare", SdkValue::Null).unwrap();
    record.with_op(&op).write(&f.home).unwrap();
    let provider_calls = std::fs::read(f.root.join("calls")).unwrap();
    let active = f.report("docker", Duration::from_secs(1));
    outcome(&active, "machine.provider", "blocked");
    assert_eq!(std::fs::read(f.root.join("calls")).unwrap(), provider_calls);
    record_without_op(&f.home);
    outcome(
        &f.report("docker", Duration::ZERO),
        "machine.provider",
        "unknown",
    );
    // Corrupt record fields are concrete errors before any external probes.
    write_entity(
        &paths::machine_record(&f.home, "bad"),
        &SdkValue::map([
            ("name", SdkValue::text("bad")),
            ("platform", SdkValue::text("lima")),
            ("cpus", SdkValue::Int(-1)),
        ]),
    )
    .unwrap();
    outcome(
        &f.report("bad", Duration::from_secs(1)),
        "machine.record",
        "fail",
    );
    // Malformed saved state and operation metadata must not become report text.
    let sensitive = "private-saved-value-must-not-be-displayed";
    for state in [
        SdkValue::text(sensitive),
        SdkValue::map([("secret", SdkValue::text(sensitive))]),
    ] {
        MachineRecord::built_in("native")
            .with("state", state)
            .write(&f.home)
            .unwrap();
        let report = f.report("native", Duration::from_secs(1));
        outcome(&report, "machine.record", "fail");
        assert!(!format!("{report:?}").contains(sensitive));
    }
    for state in ["running", "stopped", "failed"] {
        MachineRecord::built_in("native")
            .with("state", SdkValue::text(state))
            .write(&f.home)
            .unwrap();
        outcome(
            &f.report("native", Duration::from_secs(1)),
            "machine.record",
            "pass",
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
    MachineRecord::built_in("native")
        .with("op", malformed_op)
        .write(&f.home)
        .unwrap();
    let report = f.report("native", Duration::from_secs(1));
    outcome(&report, "machine.provider", "blocked");
    assert!(!format!("{report:?}").contains(sensitive));
    assert_eq!(std::fs::read(f.root.join("calls")).unwrap(), provider_calls);
    assert!(home::filesystem_available(&f.home).unwrap() > 0);
}
fn record_without_op(home: &Path) {
    MachineRecord::of(home, "docker")
        .unwrap()
        .without_op()
        .write(home)
        .unwrap();
}
