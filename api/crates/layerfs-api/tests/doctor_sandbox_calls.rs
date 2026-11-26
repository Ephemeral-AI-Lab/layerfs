//! `sandbox.doctor` (call surface 2.0) through the call table against
//! temporary homes, short and long: a native Sandbox stand-in whose daemon
//! answers and lists its Workspaces (one node each, `workspace.mounted` and
//! `workspace.activity`), and one whose daemon is lost (its Workspaces are
//! `not_checked`, blocked by the Sandbox, and not tried). The records-only
//! Sandbox on a stopped machine is in `doctor_targets.rs`.
use layerfs_api::home::{paths, process, write_entity, write_record};
use layerfs_api::json;
use layerfs_bridge::control::{Answer, Call, DaemonPhase, DaemonStatus, Reply, Request, SdkValue};
use layerfs_bridge::duplex;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}

/// `sandbox.doctor sb`, short or long, and its progress events.
fn doctor(home: &Path, detail: bool) -> (SdkValue, Vec<SdkValue>) {
    let mut events = Vec::new();
    let mut args = vec![("sandbox".to_owned(), t("sb"))];
    if detail {
        args.push(("detail".to_owned(), SdkValue::Bool(true)));
    }
    let answer = layerfs_api::call(
        Some(home),
        "sandbox.doctor",
        SdkValue::Map(args),
        &mut |event| events.push(event),
    )
    .unwrap_or_else(|refusal| panic!("{}: {}", refusal.code, refusal.message));
    (answer, events)
}

fn id(n: u8) -> String {
    format!("{n:064x}")
}

/// A home with Store `s` and project `s/p` on `native`, Sandbox `sb` on
/// `native` whose runtime is this test process, and Workspace records
/// `agent` (id 1) and `build` (id 2).
fn native_home() -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "layerfs-doctor-sandbox-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&home);
    layerfs_api::call(Some(&home), "home.init", SdkValue::map([]), &mut |_| {}).unwrap();
    write_entity(
        &paths::store_record(&home, "s"),
        &SdkValue::map([
            ("name", t("s")),
            ("kind", t("sqlite")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("binding", SdkValue::Bytes(vec![1; 16])),
            ("cursor", SdkValue::Bytes(vec![7; 32])),
        ]),
    )
    .unwrap();
    write_entity(
        &paths::project_record(&home, "s", "p"),
        &SdkValue::map([("name", t("p"))]),
    )
    .unwrap();
    let (start, boot) = process::me().unwrap();
    write_entity(
        &paths::sandbox_record(&home, "sb"),
        &SdkValue::map([
            ("name", t("sb")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("project", t("s/p")),
            ("id", t("sb")),
            (
                "daemon",
                SdkValue::map([
                    ("runtime", t("process")),
                    ("pid", SdkValue::Int(std::process::id().into())),
                    ("start", t(&start.to_string())),
                    ("boot", t(&format!("{boot:032x}"))),
                ]),
            ),
        ]),
    )
    .unwrap();
    for (n, name) in [(1, "agent"), (2, "build")] {
        write_entity(
            &paths::workspace_dir(&home, "sb").join(format!("{}.json", id(n))),
            &SdkValue::map([
                ("id", t(&id(n))),
                ("name", t(name)),
                ("branch", t("main")),
                ("path", t(&format!("/w/{name}"))),
                ("namespace", SdkValue::Int(1)),
            ]),
        )
        .unwrap();
    }
    home
}

/// Saves keys for a daemon at a fresh loopback port and, when `answer`,
/// serves one doctor session there: ready, one page listing Workspace 1
/// idle and mounted and Workspace 2 failed and mounted, then the end of
/// the session. Without `answer` nothing listens on the saved port.
fn daemon(home: &Path, answer: bool) -> std::thread::JoinHandle<()> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let keys = SdkValue::map([
        ("endpoint", t(&listener.local_addr().unwrap().to_string())),
        ("secret", SdkValue::Bytes(vec![51; 32])),
        (
            "peer",
            SdkValue::Bytes(duplex::public_key(&[52; 32]).unwrap().to_vec()),
        ),
        ("instance", SdkValue::Bytes(vec![9; 32])),
    ]);
    write_record(&paths::sandbox_control(home, "sb"), &keys).unwrap();
    if !answer {
        drop(listener);
        return std::thread::spawn(|| {});
    }
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(5);
        let socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(_) if Instant::now() < until => std::thread::sleep(Duration::from_millis(2)),
                Err(_) => return,
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let peer = duplex::public_key(&[51; 32]).unwrap();
        let Ok(mut channel) = duplex::accept(socket, &[52; 32], peer) else {
            return;
        };
        while let Ok(bytes) = channel.receive.receive() {
            let call = Call::decode(bytes).unwrap();
            let reply = match &call.request {
                Request::Hello(_) => Reply::Hello(DaemonStatus {
                    instance: [9; 32],
                    phase: DaemonPhase::ControlReady,
                    overlay_sqlite: Some("fixture-sqlite".into()),
                    store_sqlite: Some("fixture-sqlite".into()),
                }),
                Request::Sdk(sdk) => {
                    assert_eq!(sdk.method, "sandbox.doctor_workspaces");
                    let row = |n: u8, activity: &str| {
                        SdkValue::map([
                            ("workspace", t(&id(n))),
                            ("activity", t(activity)),
                            ("mounted", SdkValue::Bool(true)),
                        ])
                    };
                    Reply::Sdk(SdkValue::map([
                        ("generation", t("0000000000000001")),
                        ("total", SdkValue::Int(2)),
                        ("next_offset", SdkValue::Null),
                        (
                            "workspaces",
                            SdkValue::List(vec![row(1, "idle"), row(2, "failed")]),
                        ),
                    ]))
                }
                Request::EndSession => Reply::SessionEnded,
                other => panic!("the doctor sends only reads: {other:?}"),
            };
            let end = matches!(reply, Reply::SessionEnded);
            let answer = Answer { id: call.id, reply }.encode().unwrap();
            if channel.send.send(&answer).is_err() || end {
                // the caller closes its side after the end: wait for it
                let _ = channel.receive.receive();
                return;
            }
        }
    })
}

#[test]
fn a_native_sandbox_s_daemon_lists_its_workspaces_one_node_each() {
    let home = native_home();
    let server = daemon(&home, true);
    let (short, events) = doctor(&home, false);
    server.join().unwrap();
    assert_eq!(
        json::print(&short),
        r#"{"findings":[{"workspace":"sb:build","check":"workspace.activity","severity":"error","message":"a local failure stopped its writes","fix":"layerfs workspace unmount sb:build --discard --force"}]}"#
    );
    // one event per finished thing: the Sandbox, then each Workspace
    let phases: Vec<(String, i64, i64)> = (events.iter())
        .map(|event| {
            let event = event.get("progress").unwrap();
            (
                event.get_text("phase").unwrap().to_owned(),
                event.get_int("done").unwrap(),
                event.get_int("total").unwrap(),
            )
        })
        .collect();
    assert_eq!(
        phases,
        [
            ("self".to_owned(), 1, 3),
            ("workspaces".to_owned(), 2, 3),
            ("workspaces".to_owned(), 3, 3)
        ]
    );
    let server = daemon(&home, true);
    let (long, _) = doctor(&home, true);
    server.join().unwrap();
    let tree = long.get("tree").unwrap();
    assert_eq!(tree.get_text("status"), Some("ok"));
    assert_eq!(tree.get_text("coverage"), Some("shallow"));
    assert_eq!(list(tree, "checks").len(), 15, "{tree:?}");
    let nodes: Vec<String> = list(tree, "workspaces").iter().map(json::print).collect();
    assert_eq!(
        nodes,
        [
            r#"{"workspace":"sb:agent","status":"ok","coverage":"shallow","checks":[{"check":"workspace.mounted","outcome":"pass","from":null},{"check":"workspace.activity","outcome":"pass","from":null}],"blocked_by":null,"reason":null}"#,
            r#"{"workspace":"sb:build","status":"error","coverage":"shallow","checks":[{"check":"workspace.mounted","outcome":"pass","from":null},{"check":"workspace.activity","outcome":"error","from":null}],"blocked_by":null,"reason":null}"#,
        ]
    );
    assert_eq!(long.get_text("status"), Some("error"));
    assert_eq!(
        json::print(long.get("counts").unwrap()),
        r#"{"ok":2,"notice":0,"warning":0,"error":1,"not_checked":0}"#
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_lost_daemon_leaves_its_workspaces_blocked_by_the_sandbox() {
    let home = native_home();
    daemon(&home, false).join().unwrap();
    let (short, _) = doctor(&home, false);
    let blocked = r#"{"sandbox":"sb","reason":"not_answering"}"#;
    assert_eq!(
        json::print(&short),
        format!(
            r#"{{"findings":[{{"sandbox":"sb","check":"sandbox.daemon_connect","severity":"error","message":"its daemon does not answer: nothing listens on its saved endpoint","fix":"layerfs sandbox remove sb"}}],"not_checked":[{{"workspace":"sb:agent","blocked_by":{blocked}}},{{"workspace":"sb:build","blocked_by":{blocked}}}]}}"#
        )
    );
    let (long, _) = doctor(&home, true);
    let tree = long.get("tree").unwrap();
    let checks: Vec<&str> = (list(tree, "checks").iter())
        .filter_map(|row| row.get_text("check"))
        .collect();
    // what needed its daemon was not tried, and is left out
    assert!(checks.contains(&"sandbox.daemon_connect"));
    assert!(!checks.contains(&"sandbox.daemon_auth"), "{checks:?}");
    for node in list(tree, "workspaces") {
        assert_eq!(node.get_text("coverage"), Some("none"));
        assert!(list(node, "checks").is_empty());
    }
    let _ = std::fs::remove_dir_all(home);
}
