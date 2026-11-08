//! Saved-route diagnostics use real authenticated channels without leases or replay.
use layerfs_bridge::{
    control::{Answer, Call, DaemonPhase, DaemonStatus, Reply, Request, SdkRefusal, SdkValue},
    duplex,
};
use layerfs_control::doctor::{doctor_target, DoctorRequest, DoctorTarget};
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
        &SdkValue::map([("kind", t("sqlite")), ("machine", t("native"))]),
    )
    .unwrap();
    write_entity(&paths::project_record(&home, "s", "p"), &SdkValue::map([])).unwrap();
    home
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
fn request(timeout: Duration) -> DoctorRequest {
    DoctorRequest {
        target: DoctorTarget::Sandbox("sb".into()),
        timeout: Some(timeout),
        quick: false,
        write_test: false,
    }
}
fn run(home: &Path) -> SdkValue {
    doctor_target(home, &request(Duration::from_secs(2))).unwrap()
}
fn check<'a>(report: &'a SdkValue, id: &str) -> &'a SdkValue {
    let Some(SdkValue::List(rows)) = report.get("checks") else {
        panic!("checks")
    };
    rows.iter()
        .find(|r| r.get_text("id") == Some(id))
        .unwrap_or_else(|| panic!("missing {id}: {report:?}"))
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
#[derive(Clone, Copy)]
enum Mode {
    Pages,
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
                if offset > 0 {
                    assert_eq!(sdk.args.get_text("generation"), Some("0000000000000001"));
                }
                let count = if offset == 0 { 32 } else { 1 };
                let rows = (offset..offset + count)
                    .map(|n| {
                        SdkValue::map([
                            ("workspace", t(&format!("{:064x}", n + 1))),
                            ("activity", t("idle")),
                            ("mounted", SdkValue::Bool(false)),
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
    sandbox(&home, "native", local_record());
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
    assert_eq!(
        check(&report, "sandbox.workspaces").get_text("outcome"),
        Some("pass")
    );
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
        sandbox(&home, "native", local_record());
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
        sandbox(&home, "native", local_record());
        let (_, server) = daemon(&home, mode);
        let request = request(if matches!(mode, Mode::Stalled) {
            Duration::from_millis(70)
        } else {
            Duration::from_secs(2)
        });
        let report = doctor_target(&home, &request).unwrap();
        assert!(!report.get_bool("complete"));
        assert_eq!(
            check(&report, "sandbox.workspaces").get_text("outcome"),
            Some("unknown")
        );
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
        &SdkValue::map([("platform", t("lima"))]),
    )
    .unwrap();
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
    assert_eq!(
        check(&report, "sandbox.workspaces").get_text("outcome"),
        Some("pass")
    );
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
            &SdkValue::map([("platform", t("lima"))]),
        )
        .unwrap();
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
        assert!(!report.get_bool("complete"));
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
        "native",
        SdkValue::map([
            ("runtime", t("native")),
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
        check(&report, "sandbox.daemon_keys").get_text("outcome"),
        Some("fail")
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
#[ignore = "bounded worker child, launched and killed by worker_route_uses_fixed_root_without_lease_or_record_changes"]
fn worker_fixture_server() {
    let root = PathBuf::from(std::env::var_os("LFS_DOCTOR_TEST_ROOT").unwrap());
    let listener = TcpListener::bind(std::env::var("LFS_DOCTOR_TEST_ADDRESS").unwrap()).unwrap();
    layerfs_home::home::guest::serve_root(&root);
    std::fs::write(root.join("ready"), b"ready").unwrap();
    layerfs_control::worker_server::serve(listener, "worker-secret".into(), root).unwrap();
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
    sandbox(&guest, "native", local_record());
    let (_, daemon) = daemon(&guest, Mode::Pages);
    write_entity(
        &paths::machine_record(&home, "vm1"),
        &SdkValue::map([("platform", t("lima"))]),
    )
    .unwrap();
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
    assert_eq!(
        check(&report, "sandbox.workspaces").get_text("outcome"),
        Some("pass")
    );
    assert_eq!(daemon.join().unwrap(), 2);
    assert!(!paths::lease_dir(&guest, "s").exists());
    drop(child);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn incorrect_peer_close_keeps_authentication_unknown_without_echoing_keys() {
    let home = home();
    sandbox(&home, "native", local_record());
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
    assert!(!report.get_bool("complete"));
    assert_eq!(server.join().unwrap(), 0);
    std::fs::remove_dir_all(home).unwrap();
}
