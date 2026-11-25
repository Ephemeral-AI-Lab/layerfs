//! Every fail, warning and notice the Sandbox, Workspace and backup doctors
//! make carries the fix its cause has (or none, with the reason in the slice's
//! decisions file) and redacted evidence: observed against expected facts, no
//! secrets, no `Debug` text. Doctor stays observational: no case runs a fix.
#[path = "support/doctor_own.rs"]
mod doctor_own;
mod target {
    pub use layerfs_control::doctor::target::{Budget, Checks};
}
#[path = "../src/doctor/backup_target.rs"]
mod backup_target;
#[path = "../../layerfs-bucket/tests/support/doctor_s3.rs"]
mod peer;
use layerfs_bridge::{
    control::{Answer, Call, DaemonPhase, DaemonStatus, Reply, Request, SdkValue},
    duplex,
};
use layerfs_control::doctor::{Kind, Options};
use layerfs_home::home::{self, paths, private_dir, write_entity, write_record};
use layerfs_home::process;
use peer::{Mode, Server};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}
fn fresh(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn home() -> PathBuf {
    let home = fresh("doctor-fixes-sb");
    private_dir(&home).unwrap();
    write_record(
        &home.join("layerfs.json"),
        &SdkValue::map([
            ("layout", SdkValue::Int(3)),
            ("id", t("0123456789abcdef0123456789abcdef")),
        ]),
    )
    .unwrap();
    write_entity(
        &paths::store_record(&home, "s"),
        &SdkValue::map([("kind", t("sqlite")), ("machine", t("native"))]),
    )
    .unwrap();
    write_entity(&paths::project_record(&home, "s", "p"), &SdkValue::map([])).unwrap();
    home
}
fn run(home: &Path) -> SdkValue {
    doctor_own::own_within(
        home,
        Kind::Sandbox,
        "sb",
        &Options::default(),
        Some(Duration::from_secs(2)),
    )
}
/// The doctor of Sandbox `sb` with its Workspaces.
fn doctor(home: &Path) -> SdkValue {
    let mut sink = |_| {};
    let mut events = layerfs_home::events::Events::new(&mut sink);
    let thing = layerfs_control::doctor::Thing::new(Kind::Sandbox, "sb");
    layerfs_control::doctor::run(home, thing, &Options::default(), &mut events)
        .unwrap()
        .to_sdk()
}
fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}
fn row(report: &SdkValue, id: &str) -> SdkValue {
    doctor_own::row(report, id).unwrap_or_else(|| panic!("missing {id}: {report:?}"))
}
/// The finding row `id` must be a failure with `severity`, `fix` and evidence
/// holding exactly `keys`.
fn finding(
    report: &SdkValue,
    id: &str,
    severity: &str,
    fix: Option<&str>,
    keys: &[&str],
) -> SdkValue {
    let row = row(report, id);
    assert_eq!(row.get_text("outcome"), Some("fail"), "{id}: {row:?}");
    assert_eq!(row.get_text("severity"), Some(severity), "{id}: {row:?}");
    assert_eq!(row.get_text("fix"), fix, "{id}: {row:?}");
    evidence_has(&row, keys);
    row
}
fn evidence_has(row: &SdkValue, keys: &[&str]) {
    let evidence = row.get("evidence").cloned().unwrap_or_default();
    for key in keys {
        assert!(evidence.get(key).is_some(), "{key} missing: {row:?}");
    }
    // redacted: nothing of a secret and no Rust `Debug` text
    let text = layerfs_home::json::print(&evidence);
    assert!(!text.contains("Some(") && !text.contains("Err("), "{text}");
}
fn sandbox(home: &Path, machine: &str, daemon: SdkValue) {
    write_entity(
        &paths::sandbox_record(home, "sb"),
        &SdkValue::map([
            ("machine", t(machine)),
            ("project", t("s/p")),
            ("id", t("sb")),
            ("daemon", daemon),
        ]),
    )
    .unwrap();
}
fn local_record() -> SdkValue {
    let (start, boot) = process::me().unwrap();
    SdkValue::map([
        ("runtime", t("native")),
        ("pid", SdkValue::Int(std::process::id().into())),
        ("start", t(&start.to_string())),
        ("boot", t(&format!("{boot:032x}"))),
    ])
}
fn id(n: i64) -> String {
    format!("{n:064x}")
}
fn recorded(home: &Path, id: &str, name: &str) {
    write_entity(
        &paths::workspace_dir(home, "sb").join(format!("{id}.json")),
        &SdkValue::map([
            ("id", t(id)),
            ("name", t(name)),
            ("branch", t("main")),
            ("path", t(&format!("/w/{name}"))),
            ("namespace", SdkValue::Int(1)),
        ]),
    )
    .unwrap();
}
fn keys_record(home: &Path, endpoint: &str, secret: Vec<u8>) {
    let keys = SdkValue::map([
        ("endpoint", t(endpoint)),
        ("secret", SdkValue::Bytes(secret)),
        (
            "peer",
            SdkValue::Bytes(duplex::public_key(&[52; 32]).unwrap().to_vec()),
        ),
        ("instance", SdkValue::Bytes(vec![9; 32])),
    ]);
    write_record(&paths::sandbox_control(home, "sb"), &keys).unwrap();
}
/// A daemon on a loopback port that answers one `Hello` with `instance` and
/// `phase`, then its Workspace `rows` (`n`, activity, mounted), and ends the
/// session. It accepts with static key `accept`; `52` is the saved one.
fn daemon(
    home: &Path,
    accept: u8,
    instance: u8,
    phase: DaemonPhase,
    rows: Vec<(i64, &'static str, bool)>,
) -> std::thread::JoinHandle<()> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    keys_record(
        home,
        &listener.local_addr().unwrap().to_string(),
        vec![51; 32],
    );
    std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(3);
        let socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(_) => return,
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let Ok(mut channel) = duplex::accept(
            socket,
            &[accept; 32],
            duplex::public_key(&[51; 32]).unwrap(),
        ) else {
            return;
        };
        let Ok(bytes) = channel.receive.receive() else {
            return;
        };
        let call = Call::decode(bytes).unwrap();
        assert!(matches!(call.request, Request::Hello(_)));
        let reply = Reply::Hello(DaemonStatus {
            instance: [instance; 32],
            phase,
            overlay_sqlite: Some("fixture-sqlite".into()),
            store_sqlite: Some("fixture-sqlite".into()),
        });
        let sent = Answer { id: call.id, reply }.encode().unwrap();
        channel.send.send(&sent).unwrap();
        while let Ok(bytes) = channel.receive.receive() {
            let call = Call::decode(bytes).unwrap();
            let reply = match &call.request {
                Request::EndSession => Reply::SessionEnded,
                _ => Reply::Sdk(SdkValue::map([
                    ("generation", t("0000000000000001")),
                    ("total", SdkValue::Int(rows.len() as i64)),
                    ("next_offset", SdkValue::Null),
                    (
                        "workspaces",
                        SdkValue::List(
                            rows.iter()
                                .map(|(n, activity, mounted)| {
                                    SdkValue::map([
                                        ("workspace", t(&id(*n))),
                                        ("activity", t(activity)),
                                        ("mounted", SdkValue::Bool(*mounted)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                ])),
            };
            let end = matches!(call.request, Request::EndSession);
            let sent = Answer { id: call.id, reply }.encode().unwrap();
            if channel.send.send(&sent).is_err() || end {
                // the caller closes its side after the end
                let _ = channel.receive.receive();
                break;
            }
        }
    })
}

// ---------------------------------------------------------------- sandbox

#[test]
fn dead_runtime_names_the_remove_fix_says_what_is_lost_and_leaves_daemon_checks_out() {
    let home = home();
    // a real process that has ended: its pid is recorded, nothing runs there
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    let (_, boot) = process::me().unwrap();
    sandbox(
        &home,
        "native",
        SdkValue::map([
            ("runtime", t("native")),
            ("pid", SdkValue::Int(pid.into())),
            ("start", t("1")),
            ("boot", t(&format!("{boot:032x}"))),
        ]),
    );
    recorded(&home, &id(1), "agent");
    let report = run(&home);
    let found = finding(
        &report,
        "sandbox.runtime",
        "error",
        Some("layerfs sandbox remove sb"),
        &["pid", "runtime", "observed", "workspaces_recorded"],
    );
    assert!(
        found
            .get_text("message")
            .unwrap()
            .contains("removing it loses the uncommitted changes of its 1 recorded Workspace(s)"),
        "{found:?}"
    );
    let evidence = found.get("evidence").unwrap();
    assert_eq!(evidence.get_int("pid"), Some(i64::from(pid)));
    assert_eq!(evidence.get_int("workspaces_recorded"), Some(1));
    assert_eq!(evidence.get_text("observed"), Some("ended_or_changed"));
    // plan T5c(1): what the report says of the daemon behind a dead runtime.
    // The frozen call surface has no `sandbox.daemon`; its checks
    // `sandbox.daemon_*` do not apply once the runtime is gone (DECISIONS R3:
    // "a check that does not apply is left out"), so none is a bare unknown.
    for check in [
        "sandbox.daemon_keys",
        "sandbox.daemon_connect",
        "sandbox.daemon_auth",
        "sandbox.daemon_instance",
        "sandbox.daemon_ready",
    ] {
        assert_eq!(doctor_own::outcome(&report, check), None, "{check}");
    }
    assert!(
        list(&report, "not_checked").is_empty(),
        "{:?}",
        list(&report, "not_checked")
    );
    // observed today: the Sandbox's own node names no `blocked_by` either
    assert_eq!(
        report.get("tree").unwrap().get("blocked_by"),
        Some(&SdkValue::Null)
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn runtime_with_an_incomplete_identity_is_a_warning_with_no_fix() {
    let home = home();
    sandbox(
        &home,
        "native",
        SdkValue::map([("runtime", t("native")), ("pid", SdkValue::Int(1))]),
    );
    let report = run(&home);
    let found = finding(
        &report,
        "sandbox.runtime",
        "warning",
        None,
        &["pid_present", "start_present", "boot_present"],
    );
    let evidence = found.get("evidence").unwrap();
    assert_eq!(evidence.get("pid_present"), Some(&SdkValue::Bool(true)));
    assert_eq!(evidence.get("start_present"), Some(&SdkValue::Bool(false)));
    assert_eq!(evidence.get("boot_present"), Some(&SdkValue::Bool(false)));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn record_findings_name_their_cause_and_no_fix() {
    // no record: `sandbox remove` answers NoSuchSandbox, so no fix
    let home = home();
    let report = run(&home);
    let found = finding(&report, "sandbox.record", "error", None, &["cause"]);
    assert_eq!(
        found.get("evidence").unwrap().get_text("cause"),
        Some("absent")
    );
    // a record of another version: the build that wrote it reads it
    write_record(
        &paths::sandbox_record(&home, "sb"),
        &SdkValue::map([("v", SdkValue::Int(99))]),
    )
    .unwrap();
    let report = run(&home);
    let found = finding(
        &report,
        "sandbox.record",
        "error",
        None,
        &["cause", "v", "expected_v"],
    );
    let evidence = found.get("evidence").unwrap();
    assert_eq!(evidence.get_text("cause"), Some("unsupported_version"));
    assert_eq!(evidence.get_int("v"), Some(99));
    // a record that does not parse
    std::fs::write(paths::sandbox_record(&home, "sb"), b"{not json").unwrap();
    let report = run(&home);
    let found = finding(&report, "sandbox.record", "error", None, &["cause"]);
    assert_eq!(
        found.get("evidence").unwrap().get_text("cause"),
        Some("unreadable")
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn references_keep_the_remove_fix_and_carry_evidence() {
    let home = home();
    sandbox(&home, "vm9", SdkValue::map([("runtime", t("native"))]));
    std::fs::remove_file(paths::project_record(&home, "s", "p")).unwrap();
    let report = run(&home);
    let machine = finding(
        &report,
        "sandbox.machine_reference",
        "error",
        Some("layerfs sandbox remove sb"),
        &["cause", "machine"],
    );
    assert_eq!(
        machine.get("evidence").unwrap().get_text("machine"),
        Some("vm9")
    );
    let project = finding(
        &report,
        "sandbox.project_reference",
        "error",
        Some("layerfs sandbox remove sb"),
        &["cause", "project"],
    );
    assert_eq!(
        project.get("evidence").unwrap().get_text("project"),
        Some("s/p")
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_store_on_another_machine_is_fixed_by_moving_it() {
    let home = home();
    write_entity(
        &paths::store_record(&home, "s"),
        &SdkValue::map([("kind", t("sqlite")), ("machine", t("vm1"))]),
    )
    .unwrap();
    sandbox(&home, "native", local_record());
    let report = run(&home);
    let found = finding(
        &report,
        "sandbox.store_placement",
        "error",
        Some("layerfs store move s native"),
        &["store", "store_machine", "sandbox_machine"],
    );
    let evidence = found.get("evidence").unwrap();
    assert_eq!(evidence.get_text("store_machine"), Some("vm1"));
    assert_eq!(evidence.get_text("sandbox_machine"), Some("native"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn daemon_keys_findings_carry_evidence() {
    let home = home();
    sandbox(&home, "native", local_record());
    // no keys file
    let report = run(&home);
    let found = finding(
        &report,
        "sandbox.daemon_keys",
        "error",
        Some("layerfs sandbox remove sb"),
        &["cause"],
    );
    assert_eq!(
        found.get("evidence").unwrap().get_text("cause"),
        Some("missing_or_unreadable")
    );
    // a keys file with a good endpoint and a secret of the wrong length
    keys_record(&home, "127.0.0.1:9", vec![1; 5]);
    let report = run(&home);
    let found = finding(
        &report,
        "sandbox.daemon_keys",
        "error",
        Some("layerfs sandbox remove sb"),
        &["endpoint_ok", "secret_ok", "peer_ok", "instance_ok"],
    );
    let evidence = found.get("evidence").unwrap();
    assert_eq!(evidence.get("endpoint_ok"), Some(&SdkValue::Bool(true)));
    assert_eq!(evidence.get("secret_ok"), Some(&SdkValue::Bool(false)));
    assert_eq!(evidence.get("peer_ok"), Some(&SdkValue::Bool(true)));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn daemon_connect_refused_carries_the_remove_fix_and_evidence() {
    let home = home();
    sandbox(&home, "native", local_record());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    keys_record(&home, &address, vec![51; 32]);
    let report = run(&home);
    let found = finding(
        &report,
        "sandbox.daemon_connect",
        "error",
        Some("layerfs sandbox remove sb"),
        &["cause"],
    );
    assert_eq!(
        found.get("evidence").unwrap().get_text("cause"),
        Some("connection_refused")
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn daemon_instance_and_ready_findings() {
    // `sandbox.daemon_auth` (a Noise refusal) is NOT_RUN: a daemon that
    // does not know the saved identities closes the socket before any Noise
    // message the caller can authenticate, which the doctor reads as `lost`.
    // another instance answers
    let home = home();
    sandbox(&home, "native", local_record());
    let server = daemon(&home, 52, 8, DaemonPhase::ControlReady, Vec::new());
    let report = run(&home);
    let found = finding(
        &report,
        "sandbox.daemon_instance",
        "error",
        Some("layerfs sandbox remove sb"),
        &["cause"],
    );
    assert_eq!(
        found.get("evidence").unwrap().get_text("cause"),
        Some("instance_mismatch")
    );
    server.join().unwrap();
    std::fs::remove_dir_all(home).unwrap();
    // a startup that failed and is retained
    let home = self::home();
    sandbox(&home, "native", local_record());
    let server = daemon(&home, 52, 9, DaemonPhase::Retained, Vec::new());
    let report = run(&home);
    let found = finding(
        &report,
        "sandbox.daemon_ready",
        "error",
        Some("layerfs sandbox remove sb"),
        &["phase"],
    );
    assert_eq!(
        found.get("evidence").unwrap().get_text("phase"),
        Some("retained")
    );
    server.join().unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_daemon_still_starting_is_a_warning_fixed_by_asking_again() {
    let home = home();
    sandbox(&home, "native", local_record());
    let server = daemon(&home, 52, 9, DaemonPhase::Installing, Vec::new());
    let report = run(&home);
    let found = finding(
        &report,
        "sandbox.daemon_ready",
        "warning",
        Some("layerfs sandbox doctor sb"),
        &["phase"],
    );
    assert_eq!(
        found.get("evidence").unwrap().get_text("phase"),
        Some("installing")
    );
    server.join().unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

// -------------------------------------------------------------- workspace

fn workspace_finding<'a>(report: &'a SdkValue, workspace: &str, check: &str) -> &'a SdkValue {
    list(report, "findings")
        .iter()
        .find(|f| f.get_text("workspace") == Some(workspace) && f.get_text("check") == Some(check))
        .unwrap_or_else(|| panic!("no {check} finding on {workspace}: {report:?}"))
}

#[test]
fn workspace_findings_carry_the_unmount_fix_and_evidence() {
    let home = home();
    sandbox(&home, "native", local_record());
    recorded(&home, &id(2), "build");
    recorded(&home, &id(9), "gone");
    let server = daemon(
        &home,
        52,
        9,
        DaemonPhase::ControlReady,
        vec![
            (1, "idle", true),
            (2, "failed", true),
            (3, "uncertain", false),
        ],
    );
    let report = doctor(&home);
    server.join().unwrap();
    let gone = workspace_finding(&report, "sb:gone", "workspace.mounted");
    assert_eq!(
        gone.get_text("fix"),
        Some("layerfs workspace unmount sb:gone --discard --force")
    );
    assert_eq!(
        gone.get("evidence"),
        Some(&SdkValue::map([
            ("recorded", SdkValue::Bool(true)),
            ("listed", SdkValue::Bool(false)),
        ]))
    );
    let failed = workspace_finding(&report, "sb:build", "workspace.activity");
    assert_eq!(
        failed.get("evidence"),
        Some(&SdkValue::map([
            ("id", t(&id(2))),
            ("activity", t("failed")),
            ("mounted", SdkValue::Bool(true)),
        ]))
    );
    let handle = format!("sb:{}", id(3));
    let absent = workspace_finding(&report, &handle, "workspace.mounted");
    assert_eq!(
        absent.get_text("fix"),
        Some(&*format!(
            "layerfs workspace unmount {handle} --discard --force"
        ))
    );
    assert_eq!(
        absent.get("evidence").unwrap().get("mounted"),
        Some(&SdkValue::Bool(false))
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_mount_being_made_is_a_notice_with_evidence_and_no_fix() {
    let home = home();
    sandbox(&home, "native", local_record());
    let server = daemon(
        &home,
        52,
        9,
        DaemonPhase::ControlReady,
        vec![(1, "attaching", false)],
    );
    let report = doctor(&home);
    server.join().unwrap();
    let handle = format!("sb:{}", id(1));
    let notice = workspace_finding(&report, &handle, "workspace.mounted");
    assert_eq!(notice.get_text("severity"), Some("notice"));
    assert_eq!(notice.get_text("fix"), None);
    assert_eq!(
        notice.get("evidence").unwrap().get_text("activity"),
        Some("attaching")
    );
    std::fs::remove_dir_all(home).unwrap();
}

// ----------------------------------------------------------------- backup

struct Backup {
    home: PathBuf,
    /// Held so the fixture S3 answers until the report is made.
    #[allow(dead_code)]
    server: Server,
}
impl Backup {
    /// A bucket `production` on a fixture S3; `key` saves its secret.
    fn new(mode: Mode, key: bool, provider: &str) -> Self {
        let home = fresh("doctor-fixes-backup");
        home::ensure(&home).unwrap();
        if key {
            layerfs_store::backup::secret::write(&home, "production", "mock-secret-key").unwrap();
        }
        let server = Server::new(mode);
        write_entity(
            &paths::bucket_record(&home, "production"),
            &SdkValue::map([
                ("name", t("production")),
                ("provider", t(provider)),
                ("endpoint", t(&server.endpoint)),
                ("bucket", t("bucket")),
                ("prefix", t("saved/prefix")),
                ("region", t("us-east-1")),
                ("access_key", t("mock-access-key")),
            ]),
        )
        .unwrap();
        Self { home, server }
    }
    fn report(&self) -> SdkValue {
        let mut checks = target::Checks::default();
        backup_target::diagnose(
            &self.home,
            "production",
            false,
            &target::Budget::new(Some(Duration::from_secs(10))),
            &mut checks,
        );
        let report = checks.to_wire();
        let text = format!("{report:?}");
        assert!(!text.contains("mock-access-key") && !text.contains("mock-secret-key"));
        report
    }
    fn connect_again(&self) -> String {
        layerfs_store::backup::record::BucketRecord::of(&self.home, "production")
            .unwrap()
            .connect_again()
    }
}
impl Drop for Backup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
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

#[test]
fn a_missing_bucket_record_is_fixed_by_connecting_it() {
    let home = fresh("doctor-fixes-backup-none");
    home::ensure(&home).unwrap();
    let mut checks = target::Checks::default();
    backup_target::diagnose(
        &home,
        "production",
        false,
        &target::Budget::new(Some(Duration::from_secs(10))),
        &mut checks,
    );
    let report = checks.to_wire();
    let row = check(&report, "bucket.configuration");
    assert_eq!(row.get_text("outcome"), Some("fail"));
    assert_eq!(
        row.get_text("fix"),
        Some("layerfs backup connect production <url> --access-key <key> --key-stdin")
    );
    assert_eq!(
        row.get("evidence").unwrap().get_text("cause"),
        Some("record_unavailable")
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn an_unsupported_provider_is_fixed_by_connecting_again() {
    let f = Backup::new(Mode::default(), true, "gcs");
    let report = f.report();
    let row = check(&report, "bucket.configuration");
    assert_eq!(row.get_text("outcome"), Some("fail"));
    assert_eq!(row.get_text("fix"), Some(&*f.connect_again()));
    assert!(row.get("evidence").unwrap().get_text("cause").is_some());
}

#[test]
fn a_missing_key_keeps_its_fix_and_gains_evidence() {
    let f = Backup::new(Mode::default(), false, "minio");
    let report = f.report();
    let row = check(&report, "bucket.credentials");
    assert_eq!(row.get_text("outcome"), Some("fail"));
    assert_eq!(row.get_text("fix"), Some(&*f.connect_again()));
    assert!(
        row.get("evidence").unwrap().get_text("cause").is_some(),
        "{row:?}"
    );
}

#[test]
fn a_refused_key_is_fixed_by_connecting_again_other_refusals_have_no_fix() {
    let f = Backup::new(
        Mode {
            list_denied: true,
            ..Mode::default()
        },
        true,
        "minio",
    );
    let report = f.report();
    let row = check(&report, "bucket.list");
    assert_eq!(row.get_text("outcome"), Some("fail"));
    let evidence = row.get("evidence").unwrap();
    assert!(matches!(evidence.get_int("http_status"), Some(401 | 403)));
    assert_eq!(row.get_text("fix"), Some(&*f.connect_again()));
    for key in ["cause", "http_status", "phase", "s3_code"] {
        assert!(evidence.get(key).is_some(), "{key}: {row:?}");
    }
    // a refusal that is not about the key names no fix
    let f = Backup::new(
        Mode {
            region_failure: true,
            ..Mode::default()
        },
        true,
        "minio",
    );
    let report = f.report();
    let row = check(&report, "bucket.list");
    assert_eq!(
        row.get("evidence").unwrap().get_int("http_status"),
        Some(400)
    );
    assert_eq!(row.get_text("fix").unwrap_or(""), "", "{row:?}");
}
