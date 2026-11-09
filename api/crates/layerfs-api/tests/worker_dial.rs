//! A worker call dials the recorded worker once, with no ping first (#535
//! R1), and never starts its machine (D11): a failure before the request
//! is sent (a refused connection, a refused secret) is `Unavailable`
//! naming `layerfs machine start`; a call whose answer is lost after it
//! was sent is `Unknown`. Stand-in workers here; no `start.lock` is ever
//! taken. One test, because it sets the process's environment.
#![cfg(unix)] // unix stat/lock semantics; the Windows face is its own port

#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths, write_record};
use layerfs_api::worker::read_frame;
use layerfs_bridge::control::SdkValue;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-dial-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A home with sqlite Store `shop` (project `api`) on Lima machine `box`,
/// whose worker is recorded at `address`.
fn home_on_box(tag: &str, address: SocketAddr) -> PathBuf {
    let home = tempdir(tag);
    home::open(&home).unwrap();
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([("name", t("box")), ("platform", t("lima"))]),
    )
    .unwrap();
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&address.to_string())), ("secret", t("5e"))]),
    )
    .unwrap();
    write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", t("box")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    home
}

fn read(home: &Path) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::map([
        ("project", t("shop/api")),
        ("branch", t("main")),
        ("path", t("/a")),
    ]);
    layerfs_api::call(Some(home), "file.read", args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn started(home: &Path) -> bool {
    paths::machine_dir(home, "box").join("start.lock").exists()
}

/// How a stand-in worker meets each connection.
#[derive(Clone, Copy)]
enum Meet {
    /// Takes the secret and answers the request.
    Answer,
    /// Takes the secret, reads the request and closes without an answer.
    Lose,
    /// Refuses the secret.
    Deny,
}

/// A stand-in worker; its log holds each request's `op`, or `hello` for a
/// connection that sent none.
fn worker(meet: Meet) -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
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
            let Some(hello) = layerfs_api::worker::read_frame(&mut stream).unwrap() else {
                continue;
            };
            assert_eq!(hello, SdkValue::map([("secret", t("5e"))]));
            if let Meet::Deny = meet {
                seen.lock().unwrap().push("hello".to_owned());
                layerfs_api::worker::write_frame(
                    &mut stream,
                    &SdkValue::map([(
                        "refused",
                        SdkValue::map([
                            ("code", t("Denied")),
                            ("message", t("the worker's secret does not match")),
                            ("detail", SdkValue::Null),
                        ]),
                    )]),
                )
                .unwrap();
                continue;
            }
            // this build's hello: its protocol, no sealed bundle
            layerfs_api::worker::write_frame(
                &mut stream,
                &SdkValue::map([
                    ("ready", SdkValue::Bool(true)),
                    ("protocol", SdkValue::Int(layerfs_api::worker::PROTOCOL)),
                    ("bundle", SdkValue::Null),
                ]),
            )
            .unwrap();
            let request = read_frame(&mut stream).ok().flatten();
            let op = request
                .as_ref()
                .and_then(|request| request.get_text("op"))
                .unwrap_or_default();
            seen.lock().unwrap().push(op.to_owned());
            if let Meet::Answer = meet {
                layerfs_api::worker::write_frame(
                    &mut stream,
                    &SdkValue::map([(
                        "value",
                        SdkValue::map([("answered", SdkValue::Bool(true))]),
                    )]),
                )
                .unwrap();
            }
        }
    });
    (address, log)
}

/// An address nothing listens on.
fn closed() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}

#[test]
fn a_worker_call_dials_once_and_never_starts_its_machine() {
    let cache = tempdir("cache");
    std::env::set_var("XDG_CACHE_HOME", &cache);
    std::env::remove_var("LAYERFS_BUNDLE");

    // a worker that answers: one connection, the call itself, no ping
    let (address, log) = worker(Meet::Answer);
    let home = home_on_box("answer", address);
    let value = read(&home).unwrap();
    assert_eq!(value.get("answered"), Some(&SdkValue::Bool(true)));
    let requests = log.lock().unwrap().clone();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0], "store.call", "{requests:?}");
    assert!(!started(&home), "an answering worker starts nothing");
    std::fs::remove_dir_all(&home).unwrap();

    // an answer lost after the request was sent: Unknown, nothing resent,
    // nothing started
    let (address, log) = worker(Meet::Lose);
    let home = home_on_box("lose", address);
    let (code, message) = read(&home).unwrap_err();
    assert_eq!(code, "Unknown", "{message}");
    assert!(message.contains("not resent"), "{message}");
    assert_eq!(log.lock().unwrap().len(), 1, "sent once");
    assert!(!started(&home), "a sent call never starts the machine");
    std::fs::remove_dir_all(&home).unwrap();

    // nothing listens: nothing was sent, and nothing is started; the
    // refusal names the call that starts the machine
    let home = home_on_box("closed", closed());
    let (code, message) = read(&home).unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("layerfs machine start box"), "{message}");
    assert!(!started(&home), "a call started the machine");
    std::fs::remove_dir_all(&home).unwrap();

    // a worker that refuses the secret was not sent the request either
    let (address, log) = worker(Meet::Deny);
    let home = home_on_box("deny", address);
    let (code, message) = read(&home).unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("layerfs machine start box"), "{message}");
    assert!(!started(&home), "a call started the machine");
    assert_eq!(log.lock().unwrap().as_slice(), ["hello"], "no request");
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(cache).unwrap();
}
