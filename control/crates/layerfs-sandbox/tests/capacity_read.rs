//! The `backend.daemon.connection_failures` read model's bounds (#540,
//! #543): one query ends its control session instead of dropping the socket,
//! a daemon that trickles is fenced at the deadline, and a read on the
//! Sandbox's pool takes no second connection.
//!
//! Nothing runs: a stand-in daemon answers the query on the connections it
//! accepts, and counts what it saw.
use layerfs_bridge::control::{
    AdmissionModel, Answer, Call, CapacityModel, DaemonPhase, DaemonStatus, Reply, Request,
    RetainedModel, SdkValue, CAPACITY_OPERATION,
};
use layerfs_bridge::duplex;
use layerfs_home::home::paths;
use layerfs_home::secrets;
use layerfs_sandbox::daemon::capacity;
use layerfs_sandbox::daemon::pool::{Controls, Dial};
use layerfs_sandbox::host::{Call as HostCall, Host, Message};
use layerfs_sandbox::record::SandboxRecord;
use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CLIENT_PRIVATE: [u8; 32] = [2; 32];
const DAEMON_PRIVATE: [u8; 32] = [1; 32];
const DAEMON_INSTANCE: [u8; 32] = [3; 32];

/// Names the homes of one test process apart.
static HOMES: AtomicUsize = AtomicUsize::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let sequence = HOMES.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("lfs-capr-{tag}-{}-{sequence}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A model of a pool with no vacant slot (and no retained failure).
fn model() -> SdkValue {
    CapacityModel {
        admission: AdmissionModel {
            pool_vacant: 0,
            pool_running: 8,
            reserved_vacant: 1,
            reserved_running: 0,
        },
        retained: RetainedModel::default(),
    }
    .to_sdk()
}

fn dial(endpoint: SocketAddr) -> Dial {
    Dial {
        endpoint,
        secret: CLIENT_PRIVATE,
        peer: duplex::public_key(&DAEMON_PRIVATE).unwrap(),
        instance: DAEMON_INSTANCE,
    }
}

/// What a stand-in daemon saw, and how it treats the query.
#[derive(Default)]
struct Seen {
    connections: AtomicUsize,
    queries: AtomicUsize,
    ended: AtomicBool,
    fenced: AtomicBool,
}

/// How the stand-in answers the capacity query.
#[derive(Clone, Copy)]
enum Behavior {
    /// With the model.
    Model,
    /// Never: it trickles one byte every `TRICKLE` until the far side closes
    /// the socket (`Seen::fenced`).
    Trickle,
}

const TRICKLE: Duration = Duration::from_millis(400);

/// A stand-in daemon at its own loopback port.
fn daemon(answer: Behavior) -> (SocketAddr, Arc<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let seen = Arc::new(Seen::default());
    let counted = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            counted.connections.fetch_add(1, Ordering::SeqCst);
            let seen = counted.clone();
            std::thread::spawn(move || serve(stream, answer, &seen));
        }
    });
    (endpoint, seen)
}

/// One stand-in connection: its Hello, then each call's answer; a query ends
/// the session when the caller sends its end.
fn serve(stream: TcpStream, answer: Behavior, seen: &Seen) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    // kept for a trickling daemon: ciphertext is the channel's to write
    let mut raw = stream.try_clone().ok();
    let Ok(mut channel) = duplex::accept(
        stream,
        &DAEMON_PRIVATE,
        duplex::public_key(&CLIENT_PRIVATE).unwrap(),
    ) else {
        return;
    };
    while let Ok(bytes) = channel.receive.receive() {
        let Ok(call) = Call::decode(bytes) else { break };
        let id = call.id;
        let reply = match call.request {
            Request::Hello(_) => Reply::Hello(DaemonStatus {
                instance: DAEMON_INSTANCE,
                phase: DaemonPhase::ControlReady,
                overlay_sqlite: Some("3.51.0".into()),
                store_sqlite: Some("3.51.0".into()),
            }),
            Request::EndSession => {
                seen.ended.store(true, Ordering::SeqCst);
                Reply::SessionEnded
            }
            Request::Sdk(sdk) if sdk.method == CAPACITY_OPERATION => {
                seen.queries.fetch_add(1, Ordering::SeqCst);
                if matches!(answer, Behavior::Trickle) {
                    if let Some(raw) = raw.as_mut() {
                        trickle(raw, seen);
                    }
                    break;
                }
                Reply::Sdk(model())
            }
            _ => continue,
        };
        if channel
            .send
            .send(&Answer { id, reply }.encode().unwrap())
            .is_err()
        {
            break;
        }
    }
}

/// Keeps the far side waiting without ever completing a record, until it ends
/// the connection itself.
fn trickle(raw: &mut TcpStream, seen: &Seen) {
    let byte = [0u8; 1];
    for _ in 0..20 {
        std::thread::sleep(TRICKLE);
        if raw.write_all(&byte).and_then(|()| raw.flush()).is_err() {
            seen.fenced.store(true, Ordering::SeqCst);
            return;
        }
    }
}

/// Sandbox `s1`'s folder in `home`, recording the daemon keys its queries
/// read (`control.json`) and, with `forward`, the record that reaches its
/// daemon through that forward.
fn sandbox_folder(home: &Path, endpoint: SocketAddr, forward: bool) {
    let dir = paths::sandbox_dir(home, "s1");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let keys = SdkValue::map([
        ("endpoint", t(&endpoint.to_string())),
        ("secret", SdkValue::Bytes(CLIENT_PRIVATE.to_vec())),
        (
            "peer",
            SdkValue::Bytes(duplex::public_key(&DAEMON_PRIVATE).unwrap().to_vec()),
        ),
        ("instance", SdkValue::Bytes(DAEMON_INSTANCE.to_vec())),
    ]);
    secrets::save_daemon_keys(&dir, &keys).unwrap();
    if forward {
        let record = SdkValue::map([
            ("name", t("s1")),
            ("project", t("shop/api")),
            ("machine", t("native")),
            ("id", t("s-1")),
            (
                "daemon",
                SdkValue::map([(
                    "forward",
                    SdkValue::map([
                        ("address", t(&endpoint.to_string())),
                        ("machine", t("native")),
                    ]),
                )]),
            ),
        ]);
        layerfs_home::home::records::write_record(&dir.join("record.json"), &record).unwrap();
        layerfs_home::home::records::write_record(
            &paths::worker_record(home, "native"),
            &SdkValue::map([("address", t("127.0.0.1:1")), ("secret", t("5e"))]),
        )
        .unwrap();
    }
}

fn record(name: &str) -> SandboxRecord {
    SandboxRecord::from_record(name, SdkValue::map([]))
}

#[test]
fn a_query_ends_its_control_session() {
    let (endpoint, seen) = daemon(Behavior::Model);
    let home = tempdir("session");
    sandbox_folder(&home, endpoint, false);
    let got = capacity::read(&home, &record("s1"), endpoint).unwrap();
    assert_eq!(got.admission.pool_running, 8);
    // the one conversation ended before the socket was dropped, which the
    // daemon would otherwise log as an unexpected end
    assert!(seen.ended.load(Ordering::SeqCst), "no EndSession was sent");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_trickling_daemon_is_fenced_at_the_deadline() {
    let (endpoint, seen) = daemon(Behavior::Trickle);
    let home = tempdir("trickle");
    sandbox_folder(&home, endpoint, false);
    let at = Instant::now();
    let refused = capacity::read(&home, &record("s1"), endpoint).err();
    let elapsed = at.elapsed();
    assert!(refused.is_some(), "{refused:?}");
    assert!(
        elapsed < capacity::WAIT + Duration::from_millis(1500),
        "the read outlived its deadline: {elapsed:?}"
    );
    // the socket was shut down, not merely abandoned: the daemon's own write
    // failed
    let until = Instant::now() + Duration::from_secs(3);
    while !seen.fenced.load(Ordering::SeqCst) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        seen.fenced.load(Ordering::SeqCst),
        "the fence did not close the connection"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_pooled_read_takes_no_second_connection() {
    let (endpoint, seen) = daemon(Behavior::Model);
    let dial = dial(endpoint);
    let first = dial.open().unwrap();
    let controls = Controls::new(first, dial);
    let one = capacity::lent(&controls).unwrap();
    let two = capacity::lent(&controls).unwrap();
    assert_eq!(CapacityModel::parse(&one).admission.pool_vacant, 0);
    assert_eq!(CapacityModel::parse(&two).admission.pool_running, 8);
    // both reads went over the one connection the pool holds, so a get takes
    // a single control slot for the list and the capacity read together
    assert_eq!(seen.connections.load(Ordering::SeqCst), 1);
    assert_eq!(seen.queries.load(Ordering::SeqCst), 2);
}

#[test]
fn the_worker_s_side_reads_the_capacity_model() {
    let (endpoint, seen) = daemon(Behavior::Model);
    let home = tempdir("worker");
    std::fs::create_dir_all(&home).unwrap();
    sandbox_folder(&home, endpoint, true);
    // the request a machine's worker decodes (`sandbox.host`) and the host
    // operation it runs: no Store is opened and no image is loaded
    let host = Host {
        home: home.clone(),
        store: "shop".to_owned(),
        project: "api".to_owned(),
        sandbox: Some("s1".to_owned()),
        store_record: None,
        store_path: None,
        deploy: None,
        postgresql: None,
        message: Message::Absent,
        started: None,
    };
    let answer = layerfs_sandbox::dispatch::call(host, HostCall::Capacity).unwrap();
    let model = CapacityModel::parse(&answer);
    assert_eq!(model.admission.pool_vacant, 0, "{answer:?}");
    assert_eq!(model.admission.reserved_vacant, 1, "{answer:?}");
    assert_eq!(seen.queries.load(Ordering::SeqCst), 1);
    std::fs::remove_dir_all(home).unwrap();
}
