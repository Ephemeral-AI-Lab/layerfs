//! A Sandbox recorded under the retired docker backend (D-8,
//! layerfs-project#111), seen from the home: every call on it but
//! `sandbox.close` is refused `Unsupported` before anything is routed, with
//! how to recreate it; `sandbox.close` forgets it (its machine's worker is
//! asked once to forget its own record) and answers its container's name
//! to remove by hand. A stand-in worker; no Docker or daemon runs.
#![cfg(all(unix, feature = "machine"))]
use layerfs_api::home::{self, paths, write_record};
use layerfs_api::worker::{read_frame, write_frame};
use layerfs_bridge::control::SdkValue;
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-retired-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

/// A stand-in docker worker: it records each Sandbox operation's name and
/// answers `sandbox.close` as a worker that forgot its record.
fn worker() -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
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
            let hello = SdkValue::map([
                ("ready", SdkValue::Bool(true)),
                ("protocol", SdkValue::Int(layerfs_api::worker::PROTOCOL)),
                ("bundle", SdkValue::Null),
            ]);
            write_frame(&mut stream, &hello).unwrap();
            let Some(request) = read_frame(&mut stream).ok().flatten() else {
                continue;
            };
            let args = request.get("args").cloned().unwrap_or_default();
            let op = match request.get_text("op") {
                Some("sandbox.host") => args.get_text("op").unwrap_or(""),
                other => other.unwrap_or(""),
            }
            .to_owned();
            seen.lock().unwrap().push(op.clone());
            let answer = match op.as_str() {
                "sandbox.close" => SdkValue::map([(
                    "value",
                    SdkValue::map([("kind", t("Forgotten")), ("sandbox", t("d1"))]),
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

/// A home with the docker machine (its worker at `worker`), sqlite Store
/// `dock` on it with project `api`, and Sandbox `d1` as the docker backend
/// recorded it: `runtime` and `backend` `docker`, or (`converted`) a record
/// converted from before runtimes were kept, with neither.
fn home_with_retired(tag: &str, worker: SocketAddr, converted: bool) -> PathBuf {
    let home = tempdir(tag);
    home::open(&home).unwrap();
    write_record(
        &paths::machine_record(&home, "docker"),
        &SdkValue::map([("name", t("docker")), ("platform", t("docker"))]),
    )
    .unwrap();
    write_record(
        &paths::worker_record(&home, "docker"),
        &SdkValue::map([("address", t(&worker.to_string())), ("secret", t("5e"))]),
    )
    .unwrap();
    write_record(
        &paths::store_record(&home, "dock"),
        &SdkValue::map([
            ("name", t("dock")),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", t("docker")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(&home, "dock", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    let mut fields = vec![
        ("name", t("d1")),
        ("project", t("dock/api")),
        ("machine", t("docker")),
        ("created", SdkValue::Int(1)),
        ("id", t("d1")),
        ("platform", t("docker")),
        ("image", t(&format!("sha256:{}", "c".repeat(64)))),
        ("route", t("fuse")),
        ("runtime_id", t(&"e".repeat(64))),
    ];
    if !converted {
        fields.extend([("runtime", t("docker")), ("backend", t("docker"))]);
    }
    let dir = paths::sandbox_dir(&home, "d1");
    std::fs::create_dir_all(&dir).unwrap();
    write_record(
        &paths::sandbox_record(&home, "d1"),
        &SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect()),
    )
    .unwrap();
    home
}

#[test]
fn a_retired_docker_sandbox_is_refused_and_close_forgets_it() {
    for converted in [false, true] {
        let (address, log) = worker();
        let home = home_with_retired(&format!("home-{converted}"), address, converted);
        let id = home::opened(&home).unwrap().id;
        let container = format!("lfs-{}-d1", &id[..8]);
        for (name, args) in [
            ("sandbox.get", vec![("name", t("d1"))]),
            (
                "workspace.mount",
                vec![("sandbox", t("d1")), ("branch", t("main"))],
            ),
            ("workspace.status", vec![("ws", t("d1:/w/x"))]),
            (
                "workspace.exec",
                vec![("ws", t("d1:/w/x")), ("command", t("true"))],
            ),
            ("workspace.unmount", vec![("ws", t("d1:/w/x"))]),
        ] {
            let (code, message) = call(&home, name, &args).unwrap_err();
            assert_eq!(code, "Unsupported", "{name}: {message}");
            assert!(
                message.contains("docker Sandbox backend"),
                "{name}: {message}"
            );
            assert!(
                message.contains(&format!("`docker rm -f {container}`")),
                "{name}: {message}"
            );
            assert!(message.contains("`layerfs sandbox create`"), "{message}");
        }
        assert!(log.lock().unwrap().is_empty(), "routed: {:?}", log.lock());
        let closed = call(&home, "sandbox.close", &[("name", t("d1"))]).unwrap();
        assert_eq!(closed.get_text("closed"), Some("d1"), "{closed:?}");
        assert!(closed.get_bool("forgotten"), "{closed:?}");
        assert_eq!(closed.get_text("container"), Some(container.as_str()));
        assert_eq!(
            closed.get_text("remove"),
            Some(format!("docker rm -f {container}").as_str())
        );
        assert!(closed.get("left").is_none(), "{closed:?}");
        assert_eq!(*log.lock().unwrap(), ["sandbox.close"]);
        assert!(!paths::sandbox_dir(&home, "d1").exists());
        // the name is free: a get says so
        let (code, _) = call(&home, "sandbox.get", &[("name", t("d1"))]).unwrap_err();
        assert_eq!(code, "NoSuchSandbox");
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[test]
fn close_forgets_it_when_the_worker_cannot_be_reached_and_says_so() {
    // nothing listens where the worker is recorded
    let gone = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let home = home_with_retired("unreached", gone, false);
    let closed = call(&home, "sandbox.close", &[("name", t("d1"))]).unwrap();
    assert!(closed.get_bool("forgotten"), "{closed:?}");
    let left = closed.get_text("left").unwrap_or("");
    assert!(
        left.contains("machine docker's worker keeps its record"),
        "{left}"
    );
    assert!(!paths::sandbox_dir(&home, "d1").exists());
    std::fs::remove_dir_all(home).unwrap();
}
