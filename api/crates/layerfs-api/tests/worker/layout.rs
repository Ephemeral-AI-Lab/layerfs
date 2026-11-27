//! A worker's root under layout 3 (#536 H10, §6): a Sandbox call writes the
//! Store's thin record `{ v, name, kind }` only when it changed (#535 R5).
use layerfs_home::home::{self, paths, read_record};
use layerfs_machine::worker::{self, Endpoint};
use layerfs_protocol::sdk::SdkValue;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-wl-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A worker serving `root`, answering pings.
fn worker(root: &Path) -> Endpoint {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint {
        address: listener.local_addr().unwrap(),
        secret: "k".to_owned(),
    };
    let guest = root.to_owned();
    std::thread::spawn(move || layerfs_api::worker::serve::serve(listener, "k".to_owned(), guest));
    for _ in 0..200 {
        if worker::ping(&endpoint).is_ok() {
            return endpoint;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the worker did not answer within 2 s");
}

/// One Sandbox operation for project `shop/api` with `store_record`: a
/// Workspace status, which keeps the records as every operation does but
/// `sandbox.get`'s two reads (layerfs-project#181).
fn sandbox_call(endpoint: &Endpoint, store_record: SdkValue) {
    let args = SdkValue::map([
        ("op", SdkValue::text("backend.daemon.workspace.status")),
        ("store", SdkValue::text("shop")),
        ("project", SdkValue::text("api")),
        ("store_record", store_record),
        (
            "project_record",
            SdkValue::map([("name", SdkValue::text("api"))]),
        ),
        ("sandbox", SdkValue::text("one")),
    ]);
    // no daemon runs here: the call itself fails after the records
    let _ = worker::call(endpoint, "sandbox.host", args, &mut |_| {});
}

fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

#[test]
fn the_store_record_is_thin_and_written_only_when_it_changed() {
    let scratch = tempdir("thin");
    let root = scratch.join("data");
    let endpoint = worker(&root);
    // the caller's home id travels with each request
    layerfs_home::home::open(&scratch.join("home")).unwrap();
    let sent = SdkValue::map([
        ("name", SdkValue::text("shop")),
        ("kind", SdkValue::text("sqlite")),
        ("binding", SdkValue::Bytes(vec![7; 16])),
        ("machine", SdkValue::text("dev")),
    ]);
    sandbox_call(&endpoint, sent.clone());
    let path = paths::store_record(&root, "shop");
    let thin = SdkValue::map([
        ("v", SdkValue::Int(1)),
        ("name", SdkValue::text("shop")),
        ("kind", SdkValue::text("sqlite")),
    ]);
    assert_eq!(read_record(&path).unwrap(), Some(thin.clone()));

    // the same record again, or one differing only in what the thin record
    // leaves out: the file is not rewritten
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    sandbox_call(&endpoint, sent.clone());
    let changed = home::with_field(sent, "profile", SdkValue::text("disposable"));
    sandbox_call(&endpoint, changed);
    assert_eq!(
        modified(&path),
        old,
        "an unchanged thin record is not written"
    );
    assert_eq!(read_record(&path).unwrap(), Some(thin));
    std::fs::remove_dir_all(scratch).unwrap();
}
