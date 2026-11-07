//! Direct Workspace calls (#535 G4, phase 6) with stand-ins only: no VM,
//! container or daemon runs. A stand-in worker answers `sandbox.create`
//! with its daemon's endpoint and keys, `workspace.exec` and
//! `sandbox.close`, and refuses everything else.
//!
//! What it proves: a Sandbox on an `ssh` machine left by an older home is
//! refused `Unsupported` (the platform was removed in 2.0) before the worker
//! is asked; on `docker` (namespaces in its worker container, D-8), whose
//! adapter forwards nothing, every call goes through the worker as before. What it does not prove: a real daemon answering
//! (`sandbox_direct_live.rs` dials a host daemon). One test, because it
//! sets the process's environment.
#![cfg(all(unix, feature = "machine"))]
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_api::worker::{read_frame, write_frame};
use layerfs_bridge::control::SdkValue;
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-direct-{tag}-{}", std::process::id()));
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

/// What the stand-in worker was sent: each Sandbox operation's name and
/// its `more` arguments.
type Log = Arc<Mutex<Vec<(String, SdkValue)>>>;

/// A stand-in worker whose Sandbox daemon is at `endpoint` (nothing
/// listens there); it answers `sandbox.create`, `workspace.exec` and
/// `sandbox.close`, and refuses every other operation.
fn worker(endpoint: SocketAddr) -> (SocketAddr, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let log: Log = Arc::default();
    let seen = log.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            if read_frame(&mut stream).ok().flatten().is_none() {
                continue;
            }
            write_frame(
                &mut stream,
                // this build's hello: the protocol, and no sealed bundle
                &SdkValue::map([
                    ("ready", SdkValue::Bool(true)),
                    ("protocol", SdkValue::Int(layerfs_api::worker::PROTOCOL)),
                    ("bundle", SdkValue::Null),
                ]),
            )
            .unwrap();
            let Some(request) = read_frame(&mut stream).ok().flatten() else {
                continue;
            };
            let args = request.get("args").cloned().unwrap_or_default();
            let op = match request.get_text("op") {
                Some("sandbox.host") => args.get_text("op").unwrap_or("").to_owned(),
                other => other.unwrap_or("").to_owned(),
            };
            let more = args.get("more").cloned().unwrap_or_default();
            seen.lock().unwrap().push((op.clone(), more.clone()));
            let answer = match op.as_str() {
                "sandbox.create" => SdkValue::map([(
                    "value",
                    SdkValue::map([
                        ("sandbox", more.get("sandbox").cloned().unwrap_or_default()),
                        ("name", t("lfs-x-t1")),
                        ("image", t("local")),
                        ("platform", t("ssh")),
                        ("runtime", t("namespaces")),
                        ("route", t("fuse")),
                        ("runtime_id", t("lfs-x-t1")),
                        (
                            "control",
                            SdkValue::map([
                                ("endpoint", t(&endpoint.to_string())),
                                ("secret", SdkValue::Bytes(vec![1; 32])),
                                ("peer", SdkValue::Bytes(vec![2; 32])),
                                ("instance", SdkValue::Bytes(vec![3; 32])),
                            ]),
                        ),
                    ]),
                )]),
                "workspace.exec" => SdkValue::map([(
                    "value",
                    SdkValue::map([
                        ("kind", t("exit:0")),
                        ("exitCode", SdkValue::Int(0)),
                        ("stdout", t("")),
                        ("stderr", t("")),
                    ]),
                )]),
                "sandbox.close" => SdkValue::map([(
                    "value",
                    SdkValue::map([("kind", t("Closed")), ("unknown", SdkValue::List(vec![]))]),
                )]),
                _ => SdkValue::map([(
                    "refused",
                    SdkValue::map([
                        ("code", t("Failed")),
                        ("message", t(&format!("the stand-in worker refuses {op}"))),
                        ("detail", SdkValue::Null),
                    ]),
                )]),
            };
            let _ = write_frame(&mut stream, &answer);
        }
    });
    (address, log)
}

/// Machine `machine` (`platform`, with `extra` fields) whose worker is at
/// `worker`, holding sqlite Store `store` with project `api`.
fn machine(
    home: &Path,
    machine: &str,
    platform: &str,
    extra: &[(&str, SdkValue)],
    worker: SocketAddr,
    store: &str,
) {
    let mut fields = vec![
        ("name".to_owned(), t(machine)),
        ("platform".to_owned(), t(platform)),
    ];
    fields.extend(extra.iter().map(|(n, v)| ((*n).to_owned(), v.clone())));
    write_record(
        &paths::machine_record(home, machine),
        &SdkValue::Map(fields),
    )
    .unwrap();
    write_record(
        &paths::worker_record(home, machine),
        &SdkValue::map([("address", t(&worker.to_string())), ("secret", t("5e"))]),
    )
    .unwrap();
    write_record(
        &paths::store_record(home, store),
        &SdkValue::map([
            ("name", t(store)),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", t(machine)),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(home, store, "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
}

fn ops(log: &Log) -> Vec<String> {
    log.lock()
        .unwrap()
        .iter()
        .map(|(op, _)| op.clone())
        .collect()
}

#[test]
fn ssh_is_refused_and_docker_goes_through_its_worker() {
    let cache = tempdir("cache");
    std::env::set_var("XDG_CACHE_HOME", &cache);
    let home = tempdir("home");
    home::open(&home).unwrap();
    // nothing listens at the daemon's endpoint in the "machine"
    let gone = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();

    // ssh was removed in 2.0: a Sandbox on a machine still recorded on it
    // is refused before the worker is asked
    let (address, log) = worker(gone);
    machine(
        &home,
        "box",
        "ssh",
        &[(
            "ssh",
            SdkValue::map([
                ("host", t("example.test")),
                ("user", t("u")),
                ("port", SdkValue::Int(22)),
            ]),
        )],
        address,
        "shop",
    );
    let (code, message) = call(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/api")),
            ("name", t("t1")),
            ("machine", t("box")),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "Unsupported", "{message}");
    assert_eq!(message, "platform `ssh` was removed in LayerFS 2.0");
    assert!(ops(&log).is_empty(), "{:?}", ops(&log));

    // docker: its worker container runs namespaces Sandboxes (D-8), whose
    // daemon ports Docker cannot publish after the container started, so
    // it records no keys or forward here and its
    // Workspace calls go through the worker
    let (address, log) = worker(gone);
    machine(&home, "docker", "docker", &[], address, "dock");
    call(
        &home,
        "sandbox.create",
        &[
            ("project", t("dock/api")),
            ("name", t("d1")),
            ("machine", t("docker")),
        ],
    )
    .unwrap();
    let dir = paths::sandbox_dir(&home, "d1");
    assert!(!dir.join("control.json").exists());
    let record = read_record(&dir.join("record.json")).unwrap().unwrap();
    assert!(record.get("daemon").is_none(), "{record:?}");
    let (code, message) = call(&home, "workspace.status", &[("ws", t("d1:/w/x"))]).unwrap_err();
    assert_eq!(code, "Failed", "{message}");
    assert!(message.contains("stand-in worker refuses"), "{message}");
    assert_eq!(
        ops(&log),
        ["sandbox.create", "backend.daemon.workspace.status"]
    );
    call(&home, "sandbox.close", &[("name", t("d1"))]).unwrap();

    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&cache).unwrap();
}
