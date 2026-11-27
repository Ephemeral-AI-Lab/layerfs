#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;

use fixture::Fixture;
use layerfs_api::home::{self, lease, paths};
use layerfs_api::worker::{self, HostHello, HostPaths, HOST_MAX_REPLY, MAX_HELLO};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::fs;
use std::io;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CHILD_HOME: &str = "LAYERFS_TEST_MAINTENANCE_HOME";
const CHILD_ROLE: &str = "LAYERFS_TEST_MAINTENANCE_ROLE";
const STORE: &str = "shop";
const IO: Duration = Duration::from_secs(5);

fn t(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn call(home: &Path, op: &str, fields: &[(&str, &str)]) -> Result<SdkValue, SdkRefusal> {
    let args = SdkValue::Map(
        fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), t(value)))
            .collect(),
    );
    layerfs_api::call(Some(home), op, args, &mut |_| {})
}

fn new_home() -> Fixture {
    let home = Fixture::new();
    call(&home.paths.home, "store.create", &[("name", STORE)]).unwrap();
    home
}

struct ProofChild(Child);

impl ProofChild {
    fn start(home: &Path, role: &str) -> Self {
        let child = Self(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "peer_fixture", "--nocapture"])
                .env(CHILD_HOME, home)
                .env(CHILD_ROLE, role)
                .stdin(Stdio::null())
                .spawn()
                .unwrap(),
        );
        wait_for(&home.join(format!("ready-{role}")));
        child
    }
}

impl Drop for ProofChild {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn wait_for(file: &Path) {
    let until = Instant::now() + IO;
    while !file.is_file() {
        assert!(Instant::now() < until, "{} did not appear", file.display());
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn active_intent(home: &Path) -> SdkValue {
    home::read_record(&paths::store_record(home, STORE))
        .unwrap()
        .unwrap()
        .get("op")
        .cloned()
        .unwrap_or(SdkValue::Null)
}

#[test]
fn peer_fixture() {
    let Some(home) = std::env::var_os(CHILD_HOME).map(PathBuf::from) else {
        return;
    };
    let role = std::env::var(CHILD_ROLE).unwrap();
    let mut lease = Some(lease::hold(&home, STORE).unwrap());
    let ready = home.join(format!("ready-{role}"));
    let end = Instant::now() + Duration::from_secs(20);
    if role == "offline" {
        let path = paths::lease_dir(&home, STORE).join(std::process::id().to_string());
        let existing = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("{} host-worker\n", existing.trim_end())).unwrap();
    }
    if matches!(role.as_str(), "foreign" | "offline") {
        fs::write(ready, b"").unwrap();
        while Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        return;
    }

    let paths = HostPaths::new(&home).unwrap();
    let guard = paths.try_hold().unwrap().unwrap();
    let listener = paths.bind(true, &guard).unwrap();
    listener.set_nonblocking(true).unwrap();
    let hello = HostHello::own(&paths).unwrap();
    fs::write(ready, b"").unwrap();
    while Instant::now() < end {
        let release = home.join("release");
        if release.is_file() && lease.is_some() {
            drop(lease.take());
            fs::write(home.join("released"), b"").unwrap();
        }
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(error) => panic!("{error}"),
        };
        stream.set_nonblocking(false).unwrap();
        worker::host_check_peer(&stream).unwrap();
        let deadline = Instant::now() + IO;
        worker::write_host_frame(&stream, &hello.to_frame(), MAX_HELLO, deadline).unwrap();
        let request = worker::read_host_frame(&stream, MAX_HELLO, Some(deadline))
            .unwrap()
            .unwrap();
        let answer = match request.get_text("op") {
            Some("ping") => {
                let mut fields = home::process::identity();
                if role == "wrong-id" {
                    fields
                        .iter_mut()
                        .find(|(name, _)| *name == "pid_start")
                        .unwrap()
                        .1 = t("0");
                }
                fields.push(("generation", t(&hello.generation)));
                SdkValue::Map(
                    fields
                        .into_iter()
                        .map(|(key, value)| (key.to_owned(), value))
                        .collect(),
                )
            }
            Some("store.close") => {
                let on_disk = active_intent(&home);
                assert_eq!(on_disk.get_text("phase"), Some("worker-close"));
                assert_eq!(
                    request.get("args").unwrap().get_text("op"),
                    on_disk.get_text("id")
                );
                let refused = lease::hold(&home, STORE).err().unwrap();
                assert_eq!(refused.code, "Busy");
                let observed = home.join("one-close");
                assert!(!observed.exists(), "worker close was replayed");
                fs::write(observed, on_disk.get_text("id").unwrap()).unwrap();
                if role == "working" {
                    let reply = SdkValue::map([
                        ("request_id", request.get("request_id").unwrap().clone()),
                        (
                            "error",
                            SdkValue::map([
                                ("code", t("InUse")),
                                ("message", t("a call is running")),
                                ("detail", SdkValue::Null),
                            ]),
                        ),
                    ]);
                    worker::write_host_frame(&stream, &reply, HOST_MAX_REPLY, deadline).unwrap();
                    continue;
                }
                if role == "lost" {
                    continue;
                }
                if role == "malformed" {
                    fs::write(
                        home.join("wire-request-id"),
                        request.get_text("request_id").unwrap(),
                    )
                    .unwrap();
                    SdkValue::map([])
                } else {
                    drop(lease.take());
                    SdkValue::map([("closed", SdkValue::Bool(true))])
                }
            }
            other => panic!("unexpected operation {other:?}"),
        };
        let reply = SdkValue::map([
            ("request_id", request.get("request_id").unwrap().clone()),
            ("ok", answer),
        ]);
        worker::write_host_frame(&stream, &reply, HOST_MAX_REPLY, deadline).unwrap();
    }
}

#[test]
fn an_unreachable_worker_lease_refuses_local_compaction_before_work() {
    let home = new_home();
    let home = &home.paths.home;
    let worker = ProofChild::start(home, "offline");
    let file = paths::store_file(home, STORE);
    let before = fs::read(&file).unwrap();
    let refused = call(home, "store.compact", &[("store", STORE)]).unwrap_err();
    assert_eq!(refused.code, "InUse");
    let pid = i64::from(worker.0.id());
    assert!(refused.message.contains(&pid.to_string()));
    assert_eq!(
        refused
            .detail
            .get("next")
            .and_then(|next| next.get("worker_pids")),
        Some(&SdkValue::List(vec![SdkValue::Int(pid)]))
    );
    assert_eq!(active_intent(home), SdkValue::Null);
    assert_eq!(fs::read(file).unwrap(), before);
    assert!(!home.join("one-close").exists());
}

#[test]
fn a_malformed_close_ack_leaves_a_correlated_unknown_and_is_never_replayed() {
    let home = new_home();
    let home = &home.paths.home;
    let _worker = ProofChild::start(home, "malformed");
    let refusal = call(home, "store.remove", &[("store", STORE)]).unwrap_err();
    assert_eq!(refusal.code, "Unknown");
    let wire_id = fs::read_to_string(home.join("wire-request-id")).unwrap();
    assert_eq!(
        refusal.detail.get_text("request_id"),
        Some(wire_id.as_str())
    );
    let pending = active_intent(home);
    assert_eq!(pending.get_text("phase"), Some("worker-close"));
    assert!(pending.get_bool("uncertain"));
    let refusal = call(home, "store.remove", &[("store", STORE)]).unwrap_err();
    assert_eq!(refusal.code, "Unresolved");
    assert_eq!(active_intent(home), pending);
}

#[test]
fn remove_compact_and_upgrade_close_only_the_worker_behind_the_fence() {
    for op in ["store.remove", "store.compact", "store.upgrade"] {
        let home = new_home();
        let home = &home.paths.home;
        let _peer = ProofChild::start(home, "ack");
        let out = call(home, op, &[("store", STORE)]);
        assert!(out.is_ok(), "{op}: {out:?}");
        wait_for(&home.join("one-close"));
        if op == "store.remove" {
            assert!(!paths::store_record(home, STORE).exists());
        } else {
            assert_eq!(active_intent(home), SdkValue::Null);
        }
    }
}

#[test]
fn move_hands_off_the_host_lease_and_keeps_orchestration_in_the_caller() {
    let home = new_home();
    let home = &home.paths.home;
    let guest = home.join("guest");
    home::private_dir(&guest).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    home::write_entity(
        &paths::machine_record(home, "box"),
        &SdkValue::map([
            ("name", t("box")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-test-box")),
        ]),
    )
    .unwrap();
    home::write_record(
        &paths::worker_record(home, "box"),
        &SdkValue::map([("address", t(&address)), ("secret", t("k"))]),
    )
    .unwrap();
    let root = guest.clone();
    std::thread::spawn(move || worker::serve(listener, "k".to_owned(), root));

    let _peer = ProofChild::start(home, "ack");
    call(home, "store.move", &[("store", STORE), ("machine", "box")]).unwrap();
    wait_for(&home.join("one-close"));
    assert!(paths::store_file(&guest, STORE).is_file());
    assert!(!paths::store_file(home, STORE).exists());
}

#[test]
fn a_foreign_lease_blocks_maintenance_and_is_named() {
    let home = new_home();
    let home = &home.paths.home;
    let peer = ProofChild::start(home, "ack");
    let foreign = ProofChild::start(home, "foreign");
    let refused = call(home, "store.remove", &[("store", STORE)]).unwrap_err();
    assert_eq!(refused.code, "InUse");
    assert!(refused.message.contains(&foreign.0.id().to_string()));
    assert!(!refused.message.contains(&peer.0.id().to_string()));
    assert_eq!(active_intent(home), SdkValue::Null);
    assert!(!home.join("one-close").exists());
}

#[test]
fn a_lost_close_stays_fenced_until_release_is_verified_without_replay() {
    let home = new_home();
    let home = &home.paths.home;
    let _peer = ProofChild::start(home, "lost");
    let refused = call(home, "store.remove", &[("store", STORE)]).unwrap_err();
    assert_eq!(refused.code, "Unknown");
    let original = fs::read_to_string(home.join("one-close")).unwrap();
    let pending = active_intent(home);
    assert_eq!(pending.get_text("id"), Some(original.as_str()));
    assert_eq!(pending.get_text("phase"), Some("worker-close"));
    assert!(pending.get_bool("uncertain"));
    let refused = call(home, "store.remove", &[("store", STORE)]).unwrap_err();
    assert_eq!(refused.code, "Unresolved");
    assert_eq!(active_intent(home), pending);

    fs::write(home.join("release"), b"").unwrap();
    wait_for(&home.join("released"));
    call(home, "store.remove", &[("store", STORE)]).unwrap();
    assert!(!paths::store_record(home, STORE).exists());
    assert_eq!(
        fs::read_to_string(home.join("one-close")).unwrap(),
        original
    );
}

#[test]
fn the_same_pid_with_a_different_start_is_not_the_worker_holder() {
    let home = new_home();
    let home = &home.paths.home;
    let peer = ProofChild::start(home, "wrong-id");
    let refused = call(home, "store.remove", &[("store", STORE)]).unwrap_err();
    assert_eq!(refused.code, "InUse");
    assert!(refused.message.contains(&peer.0.id().to_string()));
    assert_eq!(active_intent(home), SdkValue::Null);
    assert!(!home.join("one-close").exists());
}

#[test]
fn a_new_maintenance_command_settles_a_proved_dead_worker() {
    let home = new_home();
    let home = &home.paths.home;
    let peer = ProofChild::start(home, "lost");
    let refused = call(home, "store.remove", &[("store", STORE)]).unwrap_err();
    assert_eq!(refused.code, "Unknown");
    let original = fs::read_to_string(home.join("one-close")).unwrap();
    drop(peer);

    call(home, "store.remove", &[("store", STORE)]).unwrap();
    assert!(!paths::store_record(home, STORE).exists());
    assert_eq!(
        fs::read_to_string(home.join("one-close")).unwrap(),
        original
    );
}

#[test]
fn a_known_close_refusal_clears_the_fence_and_names_the_worker() {
    let home = new_home();
    let home = &home.paths.home;
    let peer = ProofChild::start(home, "working");
    let refused = call(home, "store.remove", &[("store", STORE)]).unwrap_err();
    assert_eq!(refused.code, "InUse");
    assert_eq!(
        refused.detail.get("worker").unwrap().get_int("pid"),
        Some(i64::from(peer.0.id()))
    );
    assert_eq!(active_intent(home), SdkValue::Null);
    assert!(paths::store_record(home, STORE).exists());
}
