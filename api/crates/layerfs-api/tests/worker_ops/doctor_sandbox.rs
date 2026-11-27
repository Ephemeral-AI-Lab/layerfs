//! Saved-route diagnostics use real authenticated channels without leases or
//! replay: the Sandbox's 17 checks, then one node per Workspace from its
//! daemon's list and its folder's records (`workspace.mounted`,
//! `workspace.activity`).
#[path = "../../../../../control/crates/layerfs-inspect/tests/support/doctor_own.rs"]
mod doctor_own;
use doctor_own::complete;
use layerfs_protocol::{provision::install::{Answer, Call, DaemonPhase, DaemonStatus, Reply, Request, SdkRefusal, SdkValue}, duplex};
use layerfs_inspect::{Kind, Options};
use layerfs_home::home::{paths, private_dir, write_entity, write_record};
use layerfs_home::process;
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}
fn home() -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "doctor-sandbox-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
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
        &SdkValue::map([
            ("kind", t("sqlite")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ]),
    )
    .unwrap();
    write_entity(&paths::project_record(&home, "s", "p"), &SdkValue::map([])).unwrap();
    home
}
/// Store `s` on machine `vm1`, beside the Sandbox there.
fn on_vm1(home: &Path) {
    write_entity(
        &paths::store_record(home, "s"),
        &SdkValue::map([("kind", t("sqlite")), ("machine", t("vm1"))]),
    )
    .unwrap();
}
fn tree(home: &Path) -> BTreeMap<PathBuf, (u64, i64, i64, u32)> {
    let mut found = BTreeMap::new();
    let mut pending = vec![home.to_owned()];
    while let Some(path) = pending.pop() {
        let meta = std::fs::symlink_metadata(&path).unwrap();
        found.insert(
            path.clone(),
            (meta.len(), meta.mtime(), meta.mtime_nsec(), meta.mode()),
        );
        if meta.is_dir() {
            for e in std::fs::read_dir(path).unwrap() {
                pending.push(e.unwrap().path());
            }
        }
    }
    found
}
/// Sandbox `sb`'s own checks under `budget`.
fn within(home: &Path, budget: Duration) -> SdkValue {
    doctor_own::own_within(home, Kind::Sandbox, "sb", &Options::default(), Some(budget))
}
fn run(home: &Path) -> SdkValue {
    within(home, Duration::from_secs(2))
}
fn check(report: &SdkValue, id: &str) -> SdkValue {
    doctor_own::row(report, id).unwrap_or_else(|| panic!("missing {id}: {report:?}"))
}
fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}
/// The Workspace nodes of the report's tree.
fn workspaces(report: &SdkValue) -> Vec<SdkValue> {
    list(report.get("tree").unwrap(), "workspaces").to_vec()
}
/// The Sandbox's own `not_checked` entry, naming no check.
fn whole(report: &SdkValue) -> Option<SdkValue> {
    (list(report, "not_checked").iter())
        .find(|entry| {
            entry.get_text("sandbox") == Some("sb") && entry.get("check") == Some(&SdkValue::Null)
        })
        .cloned()
}
/// Workspace record `name` of Sandbox `sb` with id `id`.
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
/// The 64-hex id of the `n`th listed Workspace.
fn id(n: i64) -> String {
    format!("{n:064x}")
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
        ("runtime", t("process")),
        ("pid", SdkValue::Int(std::process::id().into())),
        ("start", t(&start.to_string())),
        ("boot", t(&format!("{boot:032x}"))),
    ])
}
#[derive(Clone, Copy)]
enum Mode {
    Pages,
    Mixed,
    Stale,
    Busy,
    Lost,
    LostPage,
    Unsupported,
    NotReady,
    Changed,
    Stalled,
}
fn daemon(home: &Path, mode: Mode) -> (String, std::thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let keys = SdkValue::map([
        ("endpoint", t(&address)),
        ("secret", SdkValue::Bytes(vec![51; 32])),
        (
            "peer",
            SdkValue::Bytes(duplex::public_key(&[52; 32]).unwrap().to_vec()),
        ),
        ("instance", SdkValue::Bytes(vec![9; 32])),
    ]);
    write_record(&paths::sandbox_control(home, "sb"), &keys).unwrap();
    let handle = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(3);
        let socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(_) => return 0,
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let Ok(mut channel) =
            duplex::accept(socket, &[52; 32], duplex::public_key(&[51; 32]).unwrap())
        else {
            return 0;
        };
        let call = Call::decode(channel.receive.receive().unwrap()).unwrap();
        assert!(matches!(call.request, Request::Hello(_)));
        let reply = Reply::Hello(DaemonStatus {
            instance: if matches!(mode, Mode::Stale) {
                [8; 32]
            } else {
                [9; 32]
            },
            phase: if matches!(mode, Mode::NotReady) {
                DaemonPhase::Retained
            } else {
                DaemonPhase::ControlReady
            },
            overlay_sqlite: Some("fixture-sqlite".into()),
            store_sqlite: Some("fixture-sqlite".into()),
        });
        channel
            .send
            .send(&Answer { id: call.id, reply }.encode().unwrap())
            .unwrap();
        let mut calls = 0;
        loop {
            let Ok(bytes) = channel.receive.receive() else {
                break;
            };
            let call = Call::decode(bytes).unwrap();
            if matches!(call.request, Request::EndSession) {
                channel
                    .send
                    .send(
                        &Answer {
                            id: call.id,
                            reply: Reply::SessionEnded,
                        }
                        .encode()
                        .unwrap(),
                    )
                    .unwrap();
                // the caller closes its side after the end: wait for it
                // (bounded by the socket's read timeout)
                let _ = channel.receive.receive();
                break;
            }
            let Request::Sdk(sdk) = &call.request else {
                panic!("only read-only SDK request")
            };
            assert_eq!(sdk.method, "sandbox.doctor_workspaces");
            calls += 1;
            if matches!(mode, Mode::Lost) || matches!(mode, Mode::LostPage) && calls == 2 {
                break;
            }
            if matches!(mode, Mode::Stalled) {
                std::thread::sleep(Duration::from_millis(180));
                break;
            }
            let reply = if matches!(mode, Mode::Busy | Mode::Unsupported)
                || matches!(mode, Mode::Changed) && calls == 2
            {
                Reply::SdkRefused(SdkRefusal {
                    code: if matches!(mode, Mode::Busy) {
                        "Busy"
                    } else if matches!(mode, Mode::Unsupported) {
                        "UnknownMethod"
                    } else {
                        "Changed"
                    }
                    .into(),
                    message: "not echoed".into(),
                    detail: SdkValue::Null,
                })
            } else {
                let offset = sdk.args.get_int("offset").unwrap();
                if matches!(mode, Mode::Mixed) {
                    let row = |n: i64, activity: &str, mounted: bool| {
                        SdkValue::map([
                            ("workspace", t(&id(n))),
                            ("activity", t(activity)),
                            ("mounted", SdkValue::Bool(mounted)),
                        ])
                    };
                    let rows = vec![
                        row(1, "idle", true),
                        row(2, "failed", true),
                        row(3, "uncertain", false),
                    ];
                    let reply = Reply::Sdk(SdkValue::map([
                        ("generation", t("0000000000000001")),
                        ("total", SdkValue::Int(3)),
                        ("next_offset", SdkValue::Null),
                        ("workspaces", SdkValue::List(rows)),
                    ]));
                    if channel
                        .send
                        .send(&Answer { id: call.id, reply }.encode().unwrap())
                        .is_err()
                    {
                        break;
                    }
                    continue;
                }
                if offset > 0 {
                    assert_eq!(sdk.args.get_text("generation"), Some("0000000000000001"));
                }
                let count = if offset == 0 { 32 } else { 1 };
                let rows = (offset..offset + count)
                    .map(|n| {
                        SdkValue::map([
                            ("workspace", t(&id(n + 1))),
                            ("activity", t("idle")),
                            ("mounted", SdkValue::Bool(true)),
                        ])
                    })
                    .collect();
                Reply::Sdk(SdkValue::map([
                    ("generation", t("0000000000000001")),
                    ("total", SdkValue::Int(33)),
                    (
                        "next_offset",
                        if offset == 0 {
                            SdkValue::Int(32)
                        } else {
                            SdkValue::Null
                        },
                    ),
                    ("workspaces", SdkValue::List(rows)),
                ]))
            };
            if channel
                .send
                .send(&Answer { id: call.id, reply }.encode().unwrap())
                .is_err()
            {
                break;
            }
        }
        calls
    });
    (address, handle)
}
#[test]
fn local_daemon_pages_are_complete_authenticated_and_leave_every_file_unchanged() {
    let home = home();
    sandbox(&home, layerfs_home::names::HOST_MACHINE, local_record());
    let (_, server) = daemon(&home, Mode::Pages);
    let before = tree(&home);
    let report = run(&home);
    assert_eq!(before, tree(&home));
    assert_eq!(
        check(&report, "sandbox.daemon_auth").get_text("outcome"),
        Some("pass")
    );
    assert_eq!(
        check(&report, "sandbox.runtime").get_text("outcome"),
        Some("pass")
    );
    // every check of the Sandbox ran and passed, in the kind's order
    let checks: Vec<String> = doctor_own::rows(&report)
        .iter()
        .map(|row| row.get_text("id").unwrap().to_owned())
        .collect();
    assert_eq!(
        checks,
        [
            "sandbox.record",
            "sandbox.machine_reference",
            "sandbox.store_reference",
            "sandbox.project_reference",
            "sandbox.store_placement",
            "sandbox.operation",
            "sandbox.runtime",
            "sandbox.route",
            "sandbox.daemon_keys",
            "sandbox.daemon_connect",
            "sandbox.daemon_auth",
            "sandbox.daemon_instance",
            "sandbox.daemon_ready",
            "sandbox.session",
            "sandbox.observation",
        ]
    );
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(true)), "{report:?}");
    // one node per listed Workspace, across both pages, each by its id
    let nodes = workspaces(&report);
    assert_eq!(nodes.len(), 33);
    assert_eq!(
        nodes[0].get_text("workspace"),
        Some(&*format!("sb:{}", id(1)))
    );
    for node in &nodes {
        assert_eq!(node.get_text("status"), Some("ok"), "{node:?}");
        assert_eq!(node.get_text("coverage"), Some("shallow"));
        let ids: Vec<&str> = list(node, "checks")
            .iter()
            .filter_map(|row| row.get_text("check"))
            .collect();
        assert_eq!(ids, ["workspace.mounted", "workspace.activity"]);
    }
    assert_eq!(server.join().unwrap(), 2);
    assert!(!paths::lease_dir(&home, "s").exists());
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn stale_instance_and_retained_startup_are_not_daemon_health_passes() {
    for (mode, id) in [
        (Mode::Stale, "sandbox.daemon_instance"),
        (Mode::NotReady, "sandbox.daemon_ready"),
    ] {
        let home = home();
        sandbox(&home, layerfs_home::names::HOST_MACHINE, local_record());
        let (_, server) = daemon(&home, mode);
        let report = run(&home);
        assert_eq!(check(&report, id).get_text("outcome"), Some("fail"));
        assert_eq!(server.join().unwrap(), 0);
        std::fs::remove_dir_all(home).unwrap();
    }
}
#[test]
fn busy_changed_lost_reply_and_deadline_remain_incomplete_without_replay() {
    for mode in [
        Mode::Busy,
        Mode::Changed,
        Mode::Lost,
        Mode::LostPage,
        Mode::Unsupported,
        Mode::Stalled,
    ] {
        let home = home();
        sandbox(&home, layerfs_home::names::HOST_MACHINE, local_record());
        let (_, server) = daemon(&home, mode);
        let budget = if matches!(mode, Mode::Stalled) {
            Duration::from_millis(70)
        } else {
            Duration::from_secs(2)
        };
        let report = within(&home, budget);
        assert!(!complete(&report));
        // no Workspace is recorded: the Sandbox answers for its list
        let entry = whole(&report).unwrap_or_else(|| panic!("{report:?}"));
        let reason = match mode {
            Mode::Busy => "busy",
            Mode::Stalled => "timeout",
            _ => "lost",
        };
        assert_eq!(entry.get_text("reason"), Some(reason), "{report:?}");
        assert!(workspaces(&report).is_empty());
        assert_eq!(
            server.join().unwrap(),
            if matches!(mode, Mode::LostPage | Mode::Changed) {
                2
            } else {
                1
            }
        );
        std::fs::remove_dir_all(home).unwrap();
    }
}
#[test]
fn forwarded_daemon_needs_no_worker_rpc_and_validates_carrier_identity() {
    let home = home();
    let (address, server) = daemon(&home, Mode::Pages);
    write_entity(
        &paths::machine_record(&home, "vm1"),
        &SdkValue::map([
            ("platform", t("lima")),
            ("vm_name", t("layerfs-test-vm1")),
            ("state", t("running")),
        ]),
    )
    .unwrap();
    on_vm1(&home);
    // No server exists here; any accidental worker RPC would fail this flow.
    write_record(
        &paths::worker_record(&home, "vm1"),
        &SdkValue::map([("address", t("127.0.0.1:1")), ("secret", t("never-reveal"))]),
    )
    .unwrap();
    let identity = SdkValue::Map(
        process::identity()
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    );
    let mut forward = vec![("address", t(&address)), ("machine", t("vm1"))];
    for key in ["pid", "pid_start", "pid_boot"] {
        forward.push((key, identity.get(key).unwrap().clone()));
    }
    sandbox(
        &home,
        "vm1",
        SdkValue::map([(
            "forward",
            SdkValue::Map(
                forward
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v))
                    .collect(),
            ),
        )]),
    );
    let before = tree(&home);
    let report = run(&home);
    assert_eq!(before, tree(&home));
    for id in [
        "sandbox.route",
        "sandbox.forward_process",
        "sandbox.daemon_ready",
    ] {
        assert_eq!(check(&report, id).get_text("outcome"), Some("pass"));
    }
    // the runtime is its machine's: no process check here
    assert_eq!(doctor_own::outcome(&report, "sandbox.runtime"), None);
    assert_eq!(workspaces(&report).len(), 33);
    assert_eq!(server.join().unwrap(), 2);
    assert!(!layerfs_home::json::print(&report).contains("never-reveal"));
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn missing_worker_record_and_reused_forward_pid_block_without_connecting() {
    for worker in [false, true] {
        let home = home();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        write_entity(
            &paths::machine_record(&home, "vm1"),
            &SdkValue::map([
                ("platform", t("lima")),
                ("vm_name", t("layerfs-test-vm1")),
                ("state", t("running")),
            ]),
        )
        .unwrap();
        on_vm1(&home);
        if worker {
            write_record(&paths::worker_record(&home, "vm1"), &SdkValue::Null).unwrap();
        }
        let (_, boot) = process::me().unwrap();
        sandbox(
            &home,
            "vm1",
            SdkValue::map([(
                "forward",
                SdkValue::map([
                    ("address", t(&listener.local_addr().unwrap().to_string())),
                    ("machine", t("vm1")),
                    ("pid", SdkValue::Int(std::process::id().into())),
                    ("pid_start", t("0")),
                    ("pid_boot", t(&format!("{boot:032x}"))),
                ]),
            )]),
        );
        let report = run(&home);
        // the route's own finding; the daemon behind it does not apply
        assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)), "{report:?}");
        assert_eq!(doctor_own::outcome(&report, "sandbox.daemon_connect"), None);
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        std::fs::remove_dir_all(home).unwrap();
    }
}
#[test]
fn invalid_references_and_missing_runtime_are_reported_independently() {
    let home = home();
    sandbox(
        &home,
        layerfs_home::names::HOST_MACHINE,
        SdkValue::map([
            ("runtime", t("process")),
            ("pid", SdkValue::Int(0)),
            ("start", t("0")),
            ("boot", t(&"0".repeat(32))),
        ]),
    );
    std::fs::remove_file(paths::project_record(&home, "s", "p")).unwrap();
    let report = run(&home);
    assert_eq!(
        check(&report, "sandbox.project_reference").get_text("outcome"),
        Some("fail")
    );
    assert_eq!(
        check(&report, "sandbox.runtime").get_text("outcome"),
        Some("fail")
    );
    assert_eq!(
        check(&report, "sandbox.runtime").get_text("fix"),
        Some("layerfs sandbox remove sb")
    );
    // a runtime seen gone is not dialled
    assert_eq!(doctor_own::outcome(&report, "sandbox.daemon_keys"), None);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
#[ignore = "bounded worker child, launched and killed by worker_route_uses_fixed_root_without_lease_or_record_changes"]
fn worker_fixture_server() {
    let root = PathBuf::from(std::env::var_os("LFS_DOCTOR_TEST_ROOT").unwrap());
    let listener = TcpListener::bind(std::env::var("LFS_DOCTOR_TEST_ADDRESS").unwrap()).unwrap();
    layerfs_home::home::guest::serve_root(&root);
    std::fs::write(root.join("ready"), b"ready").unwrap();
    layerfs_api::worker::serve::serve(listener, "worker-secret".into(), root).unwrap();
}
struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn worker_route_uses_fixed_root_without_lease_or_record_changes() {
    let home = home();
    let guest = home.join("guest-data");
    private_dir(&guest).unwrap();
    sandbox(&guest, layerfs_home::names::HOST_MACHINE, local_record());
    recorded(&guest, &id(1), "agent");
    let (_, daemon) = daemon(&guest, Mode::Pages);
    write_entity(
        &paths::machine_record(&home, "vm1"),
        &SdkValue::map([
            ("platform", t("lima")),
            ("vm_name", t("layerfs-test-vm1")),
            ("state", t("running")),
        ]),
    )
    .unwrap();
    on_vm1(&home);
    sandbox(&home, "vm1", SdkValue::Null);
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap().to_string();
    drop(reservation);
    write_record(
        &paths::worker_record(&home, "vm1"),
        &SdkValue::map([("address", t(&address)), ("secret", t("worker-secret"))]),
    )
    .unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "worker_fixture_server", "--ignored"])
        .env("LFS_DOCTOR_TEST_ROOT", &guest)
        .env("LFS_DOCTOR_TEST_ADDRESS", address)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut child = ChildGuard(child);
    let until = Instant::now() + Duration::from_secs(2);
    while !guest.join("ready").exists() {
        assert!(child.0.try_wait().unwrap().is_none(), "worker child exited");
        assert!(Instant::now() < until, "worker child readiness deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    let before = tree(&home);
    let report = run(&home);
    assert_eq!(before, tree(&home));
    for id in [
        "sandbox.route",
        "sandbox.worker_compatibility",
        "sandbox.runtime",
        "sandbox.daemon_ready",
    ] {
        assert_eq!(check(&report, id).get_text("outcome"), Some("pass"), "{id}");
    }
    // the worker names each Workspace by its own root's records
    let nodes = workspaces(&report);
    assert_eq!(nodes.len(), 33);
    assert!(nodes
        .iter()
        .any(|node| node.get_text("workspace") == Some("sb:agent")));
    assert!(!nodes
        .iter()
        .any(|node| node.get_text("workspace") == Some(&*format!("sb:{}", id(1)))));
    assert!(nodes
        .iter()
        .all(|node| node.get_text("status") == Some("ok")));
    assert_eq!(daemon.join().unwrap(), 2);
    assert!(!paths::lease_dir(&guest, "s").exists());
    drop(child);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn incorrect_peer_close_keeps_authentication_unknown_without_echoing_keys() {
    let home = home();
    sandbox(&home, layerfs_home::names::HOST_MACHINE, local_record());
    let (_, server) = daemon(&home, Mode::Pages);
    let file = paths::sandbox_control(&home, "sb");
    let keys = layerfs_home::home::read_record(&file).unwrap().unwrap();
    let keys = layerfs_home::home::with_field(
        keys,
        "peer",
        SdkValue::Bytes(duplex::public_key(&[53; 32]).unwrap().to_vec()),
    );
    write_record(&file, &keys).unwrap();
    let before = tree(&home);
    let report = run(&home);
    assert_eq!(before, tree(&home));
    assert_eq!(
        check(&report, "sandbox.daemon_auth").get_text("outcome"),
        Some("unknown")
    );
    assert!(!complete(&report));
    assert_eq!(server.join().unwrap(), 0);
    std::fs::remove_dir_all(home).unwrap();
}

/// The doctor of Sandbox `sb` as `sandbox doctor` makes it: where it is and
/// what is on it from the home's records.
fn doctor(home: &Path) -> SdkValue {
    let mut sink = |_| {};
    let mut events = layerfs_home::events::Events::new(&mut sink);
    let thing = layerfs_inspect::Thing::new(Kind::Sandbox, "sb");
    let report = layerfs_inspect::run(home, thing, &Options::default(), &mut events);
    report.unwrap().to_sdk()
}

fn finding<'a>(report: &'a SdkValue, workspace: &str, check: &str) -> &'a SdkValue {
    list(report, "findings")
        .iter()
        .find(|f| f.get_text("workspace") == Some(workspace) && f.get_text("check") == Some(check))
        .unwrap_or_else(|| panic!("no {check} finding on {workspace}: {report:?}"))
}

#[test]
fn workspace_nodes_come_from_the_daemon_s_list_and_the_folder_s_records() {
    let home = home();
    sandbox(&home, layerfs_home::names::HOST_MACHINE, local_record());
    recorded(&home, &id(1), "agent");
    recorded(&home, &id(2), "build");
    recorded(&home, &id(9), "gone");
    let (_, server) = daemon(&home, Mode::Mixed);
    let before = tree(&home);
    let report = doctor(&home);
    assert_eq!(before, tree(&home));
    assert_eq!(server.join().unwrap(), 1);
    let handles: Vec<String> = workspaces(&report)
        .iter()
        .map(|node| node.get_text("workspace").unwrap().to_owned())
        .collect();
    // the recorded ones first, then those only its daemon lists
    let daemon_only = format!("sb:{}", id(3));
    assert_eq!(
        handles,
        ["sb:agent", "sb:build", "sb:gone", daemon_only.as_str()]
    );
    let status = |handle: &str| {
        let nodes = workspaces(&report);
        let node = nodes
            .iter()
            .find(|node| node.get_text("workspace") == Some(handle))
            .unwrap()
            .clone();
        node.get_text("status").unwrap().to_owned()
    };
    assert_eq!(status("sb:agent"), "ok");
    assert_eq!(status("sb:build"), "error");
    let failed = finding(&report, "sb:build", "workspace.activity");
    assert_eq!(failed.get_text("severity"), Some("error"));
    assert_eq!(
        failed.get_text("message"),
        Some("a local failure stopped its writes")
    );
    assert_eq!(
        failed.get_text("fix"),
        Some("layerfs workspace unmount sb:build --discard --force")
    );
    let gone = finding(&report, "sb:gone", "workspace.mounted");
    assert_eq!(
        gone.get_text("message"),
        Some("on record, but its daemon does not hold it")
    );
    let uncertain = finding(&report, &daemon_only, "workspace.activity");
    assert_eq!(
        uncertain.get_text("fix"),
        Some(&*format!(
            "layerfs workspace unmount {daemon_only} --discard --force"
        ))
    );
    finding(&report, &daemon_only, "workspace.mounted");
    // the Sandbox itself is fine; the answer's status is its worst thing
    assert_eq!(report.get("tree").unwrap().get_text("status"), Some("ok"));
    assert_eq!(report.get_text("status"), Some("error"));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_lost_daemon_blocks_its_workspaces_without_trying_them() {
    let home = home();
    sandbox(&home, layerfs_home::names::HOST_MACHINE, local_record());
    recorded(&home, &id(1), "agent");
    // a saved endpoint nothing listens on any more
    let (address, server) = daemon(&home, Mode::Pages);
    let _ = std::net::TcpStream::connect(&address);
    server.join().unwrap();
    let report = doctor(&home);
    let connect = check(&report, "sandbox.daemon_connect");
    assert_eq!(connect.get_text("outcome"), Some("fail"), "{report:?}");
    assert_eq!(connect.get_text("fix"), Some("layerfs sandbox remove sb"));
    // the checks that needed it are not tried, and are left out
    assert_eq!(doctor_own::outcome(&report, "sandbox.daemon_auth"), None);
    let nodes = workspaces(&report);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].get_text("workspace"), Some("sb:agent"));
    assert_eq!(nodes[0].get_text("status"), Some("not_checked"));
    assert_eq!(nodes[0].get_text("coverage"), Some("none"));
    assert!(list(&nodes[0], "checks").is_empty());
    assert_eq!(
        layerfs_home::json::print(nodes[0].get("blocked_by").unwrap()),
        r#"{"sandbox":"sb","reason":"not_answering"}"#
    );
    std::fs::remove_dir_all(home).unwrap();
}

/// A worker on `vm1` whose one `doctor.sandbox` report names Workspace
/// `sb:agent` from a daemon list it never asked for.
fn not_asking_worker(home: &Path) -> std::thread::JoinHandle<()> {
    use layerfs_machine::worker::{read_frame_within, write_frame, MAX_HELLO, PROTOCOL};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    write_record(
        &paths::worker_record(home, "vm1"),
        &SdkValue::map([("address", t(&address)), ("secret", t("worker-secret"))]),
    )
    .unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert!(read_frame_within(&mut stream, MAX_HELLO).unwrap().is_some());
        let hello = SdkValue::map([
            ("ready", SdkValue::Bool(true)),
            ("protocol", SdkValue::Int(PROTOCOL)),
        ]);
        write_frame(&mut stream, &hello).unwrap();
        let request = read_frame_within(&mut stream, 64 * 1024).unwrap().unwrap();
        assert_eq!(request.get_text("op"), Some("doctor.sandbox"));
        let workspace = SdkValue::map([
            ("workspace", t("sb:agent")),
            ("recorded", SdkValue::Bool(true)),
        ]);
        let report = SdkValue::map([
            ("checks", SdkValue::List(Vec::new())),
            ("listing", t("not_asked")),
            ("workspaces", SdkValue::List(vec![workspace])),
        ]);
        write_frame(&mut stream, &SdkValue::map([("value", report)])).unwrap();
    })
}

#[test]
fn a_workspace_whose_list_was_not_asked_for_waits_on_its_sandbox_not_lost() {
    let home = home();
    write_entity(
        &paths::machine_record(&home, "vm1"),
        &SdkValue::map([
            ("platform", t("lima")),
            ("vm_name", t("layerfs-test-vm1")),
            ("state", t("running")),
        ]),
    )
    .unwrap();
    on_vm1(&home);
    sandbox(&home, "vm1", SdkValue::Null);
    let worker = not_asking_worker(&home);
    let report = run(&home);
    worker.join().unwrap();
    let nodes = workspaces(&report);
    assert_eq!(nodes.len(), 1, "{report:?}");
    assert_eq!(
        nodes[0].get_text("status"),
        Some("not_checked"),
        "{report:?}"
    );
    let entry = (list(&report, "not_checked").iter())
        .find(|entry| entry.get_text("workspace") == Some("sb:agent"))
        .unwrap_or_else(|| panic!("{report:?}"));
    assert_eq!(entry.get("reason"), Some(&SdkValue::Null), "{report:?}");
    let by = entry
        .get("blocked_by")
        .unwrap_or_else(|| panic!("{report:?}"));
    assert_eq!(by.get_text("sandbox"), Some("sb"), "{report:?}");
    assert_eq!(by.get_text("reason"), Some("not_answering"), "{report:?}");
    assert!(!format!("{report:?}").contains("\"lost\""), "{report:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_workspace_its_sandbox_did_not_observe_waits_on_that_sandbox() {
    let home = home();
    sandbox(&home, layerfs_home::names::HOST_MACHINE, local_record());
    recorded(&home, &id(1), "agent");
    // the `workspace` kind's own run, as the frame visits a Workspace its
    // Sandbox's doctor made no node for: no probe is tried, nothing is lost
    let report = doctor_own::own(&home, Kind::Workspace, "sb:agent", &Options::default());
    let entry = (list(&report, "not_checked").iter())
        .find(|entry| entry.get_text("workspace") == Some("sb:agent"))
        .unwrap_or_else(|| panic!("{report:?}"));
    assert_eq!(entry.get("reason"), Some(&SdkValue::Null), "{report:?}");
    let by = entry
        .get("blocked_by")
        .unwrap_or_else(|| panic!("{report:?}"));
    assert_eq!(by.get_text("sandbox"), Some("sb"), "{report:?}");
    assert_eq!(by.get_text("reason"), Some("not_answering"), "{report:?}");
    let tree = report.get("tree").unwrap();
    assert_eq!(tree.get_text("status"), Some("not_checked"), "{report:?}");
    assert!(!format!("{report:?}").contains("\"lost\""), "{report:?}");
    std::fs::remove_dir_all(home).unwrap();
}
