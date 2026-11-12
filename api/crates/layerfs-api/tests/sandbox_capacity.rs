//! `sandbox.get`'s `daemon_capacity` (#540, #543): the daemon's
//! `backend.daemon.connection_failures` read model, shown best-effort and
//! bounded on the connection the Workspace list lent from this process's
//! pool. Nothing runs: a stand-in daemon answers the Sandbox's Workspace list
//! and its capacity model on the same connection, a second one answers the
//! list and then says nothing, and a Sandbox whose machine or forward is
//! already down is not dialed at all.
#![cfg(unix)]
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::{
    AdmissionModel, Answer, Call, DaemonPhase, DaemonStatus, Reply, Request, RetainedModel,
    SdkValue, OPERATION,
};
use layerfs_bridge::duplex;
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
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
        std::env::temp_dir().join(format!("lfs-scap-{tag}-{}-{sequence}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

/// What the stand-in daemon answers `sandbox.list_workspaces` with: no
/// Workspaces, like a Sandbox nothing has mounted in.
fn no_workspaces() -> SdkValue {
    SdkValue::map([
        ("kind", t("workspaces")),
        ("version", SdkValue::Int(1)),
        ("workspaces", SdkValue::List(vec![])),
    ])
}

/// A stand-in daemon at its own loopback port, and how many connections it
/// served: it answers this Sandbox's Workspace list and its capacity model
/// with `model` on the connection it was given. With `stall` it answers the
/// Workspace list, reads the capacity query and never answers it, which is
/// what a daemon stuck behind its own slots looks like.
fn daemon(model: SdkValue, stall: bool) -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let model = Arc::new(model);
    let served = Arc::new(AtomicUsize::new(0));
    let counted = served.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            counted.fetch_add(1, Ordering::SeqCst);
            let model = model.clone();
            // one worker per connection: the pool keeps one and reuses it
            std::thread::spawn(move || serve(stream, &model, stall));
        }
    });
    (endpoint, served)
}

/// One stand-in connection: its Hello, then each call's answer until the
/// caller closes it.
fn serve(stream: std::net::TcpStream, model: &SdkValue, stall: bool) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
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
            Request::Sdk(sdk) if sdk.method == "sandbox.list_workspaces" => {
                Reply::Sdk(no_workspaces())
            }
            Request::Sdk(sdk) if sdk.method == OPERATION => {
                if stall {
                    // read but never answered: the read model's own fence, not
                    // a closed socket, ends its wait
                    std::thread::sleep(Duration::from_secs(6));
                    break;
                }
                Reply::Sdk(model.clone())
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

/// A record from `fields`, in order.
fn map(fields: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        fields
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    )
}

/// The home's machine, Store `shop` and project `api`; the machine's `worker`
/// record only while it runs (its absence is a stopped machine, #535 D11).
fn home_records(home: &Path, worker: bool) {
    write_record(
        &paths::machine_record(home, "native"),
        &map(&[("name", t("native")), ("platform", t("native"))]),
    )
    .unwrap();
    if worker {
        // the forward's machine answers: a forward with no recorded process is
        // not checked against one
        write_record(
            &paths::worker_record(home, "native"),
            &map(&[("address", t("127.0.0.1:1")), ("secret", t("5e"))]),
        )
        .unwrap();
    }
    write_record(
        &paths::store_record(home, "shop"),
        &map(&[
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", t("native")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(home, "shop", "api"),
        &map(&[("name", t("api"))]),
    )
    .unwrap();
}

/// Sandbox `s1` on `native`, recorded with its `id` (`None`: gone, its create
/// never recorded one) whose daemon this host reaches through the forward to
/// `endpoint`, carried by forward process `pid` (`None`: a port nothing
/// carries).
fn sandbox_record(home: &Path, endpoint: SocketAddr, id: Option<&str>, pid: Option<i64>) {
    let mut forward = vec![
        ("address", t(&endpoint.to_string())),
        ("machine", t("native")),
    ];
    if let Some(pid) = pid {
        forward.push(("pid", SdkValue::Int(pid)));
    }
    let mut fields = vec![
        ("name", t("s1")),
        ("project", t("shop/api")),
        ("machine", t("native")),
    ];
    if let Some(id) = id {
        fields.push(("id", t(id)));
    }
    fields.push((
        "daemon",
        map(&[("name", t("lfs-x-s1")), ("forward", map(&forward))]),
    ));
    write_record(&paths::sandbox_record(home, "s1"), &map(&fields)).unwrap();
    write_record(
        &paths::sandbox_control(home, "s1"),
        &map(&[
            ("endpoint", t(&endpoint.to_string())),
            ("secret", SdkValue::Bytes(CLIENT_PRIVATE.to_vec())),
            (
                "peer",
                SdkValue::Bytes(duplex::public_key(&DAEMON_PRIVATE).unwrap().to_vec()),
            ),
            ("instance", SdkValue::Bytes(DAEMON_INSTANCE.to_vec())),
        ]),
    )
    .unwrap();
}

/// A model with no vacant pool slot and one retained failure.
fn busy() -> SdkValue {
    let model = layerfs_bridge::control::CapacityModel {
        admission: AdmissionModel {
            pool_vacant: 0,
            pool_running: 8,
            reserved_vacant: 1,
            reserved_running: 0,
        },
        retained: RetainedModel {
            limit: 64,
            ttl_ms: 3_600_000,
            count: 1,
            dropped: 0,
            recorded: 1,
            failures: vec![layerfs_bridge::control::RetainedFailureModel {
                slot: 0,
                class: "pool".into(),
                generation: 6,
                owner: "daemon instance, pid 4, thread layerfs-control-0".into(),
                received_bytes: None,
                request: Some("unmount".into()),
                reply_sent: false,
                cause: "control: the kernel control did not answer".into(),
                fence_error: None,
                diagnostic_error: None,
                age_ms: 12,
                expires_in_ms: 3_599_988,
            }],
        },
    };
    model.to_sdk()
}

#[test]
fn a_running_sandbox_s_get_shows_its_daemon_capacity() {
    let (endpoint, served) = daemon(busy(), false);
    let home = tempdir("shown");
    home::open(&home).unwrap();
    home_records(&home, true);
    sandbox_record(&home, endpoint, Some("s-1"), None);
    let got = call(&home, "sandbox.get", &[("name", t("s1"))]).unwrap();
    assert_eq!(got.get_text("state"), Some("running"), "{got:?}");
    let capacity = got
        .get("daemon_capacity")
        .unwrap_or_else(|| panic!("{got:?}"));
    let admission = capacity.get("admission").unwrap();
    assert_eq!(admission.get_int("pool_vacant"), Some(0), "{capacity:?}");
    assert_eq!(admission.get_int("pool_running"), Some(8), "{capacity:?}");
    assert_eq!(
        admission.get_int("reserved_vacant"),
        Some(1),
        "{capacity:?}"
    );
    let retained = capacity.get("retained").unwrap();
    assert_eq!(retained.get_int("count"), Some(1), "{capacity:?}");
    assert_eq!(retained.get_int("limit"), Some(64), "{capacity:?}");
    let Some(SdkValue::List(failures)) = retained.get("failures") else {
        panic!("{capacity:?}")
    };
    assert_eq!(failures.len(), 1, "{capacity:?}");
    assert_eq!(
        failures[0].get_text("cause"),
        Some("control: the kernel control did not answer")
    );
    assert_eq!(failures[0].get_int("generation"), Some(6));
    assert_eq!(failures[0].get_text("request"), Some("unmount"));
    // the Workspace list and the capacity read share the one pooled
    // connection (#543): a get takes no second control slot of its own
    assert_eq!(served.load(Ordering::SeqCst), 1, "{got:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_daemon_that_does_not_answer_leaves_daemon_capacity_out() {
    let (endpoint, _served) = daemon(busy(), true);
    let home = tempdir("stalled");
    home::open(&home).unwrap();
    home_records(&home, true);
    sandbox_record(&home, endpoint, Some("s-1"), None);
    let at = Instant::now();
    let got = call(&home, "sandbox.get", &[("name", t("s1"))]).unwrap();
    let elapsed = at.elapsed();
    // the read model's own bound ends the wait; the get still answers
    assert!(
        elapsed < Duration::from_secs(4),
        "sandbox.get stalled for {elapsed:?}"
    );
    assert!(
        elapsed >= Duration::from_secs(1),
        "the bounded query did not wait: {elapsed:?}"
    );
    assert!(got.get("daemon_capacity").is_none(), "{got:?}");
    assert_eq!(got.get_text("state"), Some("running"), "{got:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_stopped_machine_is_never_dialed() {
    let (endpoint, served) = daemon(busy(), false);
    let home = tempdir("stopped");
    home::open(&home).unwrap();
    // the machine records no worker: nothing starts it (#535 D11)
    home_records(&home, false);
    sandbox_record(&home, endpoint, Some("s-1"), None);
    let at = Instant::now();
    let got = call(&home, "sandbox.get", &[("name", t("s1"))]).unwrap();
    let elapsed = at.elapsed();
    assert!(got.get("down_reason").is_some(), "{got:?}");
    assert!(got.get("daemon_capacity").is_none(), "{got:?}");
    // a down machine is answered from its records: no port is dialed and
    // nothing waits for a daemon that is not there
    assert_eq!(served.load(Ordering::SeqCst), 0, "{got:?}");
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn an_ended_forward_is_never_dialed() {
    let (endpoint, served) = daemon(busy(), false);
    let home = tempdir("forward-ended");
    home::open(&home).unwrap();
    home_records(&home, true);
    // a recorded forward process that is not alive: its port may be reused by
    // another process, so it is never dialed
    sandbox_record(&home, endpoint, Some("s-1"), Some(3_999_999));
    let at = Instant::now();
    let got = call(&home, "sandbox.get", &[("name", t("s1"))]).unwrap();
    let elapsed = at.elapsed();
    assert!(got.get("down_reason").is_some(), "{got:?}");
    assert!(got.get("daemon_capacity").is_none(), "{got:?}");
    assert_eq!(served.load(Ordering::SeqCst), 0, "{got:?}");
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_sandbox_with_no_id_is_never_dialed() {
    let (endpoint, served) = daemon(busy(), false);
    let home = tempdir("gone");
    home::open(&home).unwrap();
    home_records(&home, true);
    sandbox_record(&home, endpoint, None, None);
    let got = call(&home, "sandbox.get", &[("name", t("s1"))]).unwrap();
    assert_eq!(got.get_text("state"), Some("gone"), "{got:?}");
    assert!(got.get("daemon_capacity").is_none(), "{got:?}");
    assert_eq!(served.load(Ordering::SeqCst), 0, "{got:?}");
    std::fs::remove_dir_all(home).unwrap();
}
