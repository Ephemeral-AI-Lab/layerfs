//! A worker writes a Sandbox call's Store record under its root only when
//! it changed (#535 R5): the same record sent again leaves the file as it
//! was; a changed one replaces it.
use layerfs_bridge::control::SdkValue;
use layerfs_control::common::home::{self, paths, read_record};
use layerfs_control::machine::worker::{self, Endpoint};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-r5-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One Sandbox operation for project `shop/api` with `store_record`.
fn sandbox_call(endpoint: &Endpoint, store_record: SdkValue) {
    let args = SdkValue::map([
        ("op", SdkValue::text("sandbox.list_workspaces")),
        ("store", SdkValue::text("shop")),
        ("project", SdkValue::text("api")),
        ("store_record", store_record),
        (
            "project_record",
            SdkValue::map([("name", SdkValue::text("api"))]),
        ),
        ("sandbox", SdkValue::text("1")),
    ]);
    // no daemon runs here: the call itself fails after the records
    let _ = worker::call(endpoint, "sandbox.host", args, &mut |_| {});
}

fn modified(path: &std::path::Path) -> SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

#[test]
fn the_store_record_is_written_only_when_it_changed() {
    let scratch = tempdir("scratch");
    let root = scratch.join("data");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint {
        address: listener.local_addr().unwrap(),
        secret: "k".to_owned(),
    };
    let guest = root.clone();
    std::thread::spawn(move || worker::serve(listener, "k".to_owned(), guest));
    for _ in 0..200 {
        if worker::ping(&endpoint).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // the caller's home id travels with each request
    home::open(&scratch.join("home")).unwrap();
    let sent = SdkValue::map([
        ("name", SdkValue::text("shop")),
        ("kind", SdkValue::text("sqlite")),
        ("binding", SdkValue::Bytes(vec![7; 16])),
        ("machine", SdkValue::text("dev")),
    ]);
    sandbox_call(&endpoint, sent.clone());
    let path = paths::store_record(&root, "shop");
    let written = read_record(&path).unwrap().expect("written the first time");
    assert_eq!(written.get_text("machine"), Some("native"));

    // the same record again: the file is not rewritten
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    sandbox_call(&endpoint, sent.clone());
    assert_eq!(modified(&path), old, "an unchanged record is not written");

    // a changed record replaces it
    let changed = home::with_field(sent, "profile", SdkValue::text("disposable"));
    sandbox_call(&endpoint, changed);
    assert_ne!(modified(&path), old);
    let written = read_record(&path).unwrap().unwrap();
    assert_eq!(written.get_text("profile"), Some("disposable"));
    std::fs::remove_dir_all(scratch).unwrap();
}
