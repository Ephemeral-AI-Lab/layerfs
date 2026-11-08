//! A worker's root under layout 3 (#536 H10, §6): a Sandbox call writes the
//! Store's thin record `{ v, name, kind }` only when it changed (#535 R5),
//! and the first start from a layout 3 bundle deletes what a layout 2
//! bundle left (numbered Sandbox state, `machine/*/sandbox/`, old local
//! Sandbox directories; the image cache kept under `machine/native/run/`).
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, paths, read_record, write_record};
use layerfs_machine::worker::{self, Endpoint};
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
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, "k".to_owned(), guest)
    });
    for _ in 0..200 {
        if worker::ping(&endpoint).is_ok() {
            return endpoint;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the worker did not answer within 2 s");
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
    layerfs_control::migrate::open(&scratch.join("home")).unwrap();
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

#[test]
fn a_first_start_deletes_what_layout_2_left() {
    let scratch = tempdir("retire");
    let root = scratch.join("data");
    let numbered = paths::store_dir(&root, "shop").join("project/api/sandboxes/1");
    std::fs::create_dir_all(&numbered).unwrap();
    std::fs::write(numbered.join("sandbox.record"), b"old").unwrap();
    let old_records = paths::machine_dir(&root, "native").join("sandbox/a");
    std::fs::create_dir_all(&old_records).unwrap();
    let old_run = paths::machine_dir(&root, "native").join("sandbox-root");
    std::fs::create_dir_all(old_run.join("images")).unwrap();
    std::fs::write(old_run.join("images/x"), b"image").unwrap();
    std::fs::create_dir_all(old_run.join("lfs-1234abcd-shop.api-1/rootfs")).unwrap();
    let full = SdkValue::map([
        ("name", SdkValue::text("shop")),
        ("kind", SdkValue::text("sqlite")),
        ("binding", SdkValue::Bytes(vec![7; 16])),
    ]);
    write_record(&paths::store_record(&root, "shop"), &full).unwrap();
    // a record a layout 3 caller wrote stays
    let project = paths::project_record(&root, "shop", "api");
    write_record(&project, &SdkValue::map([("name", SdkValue::text("api"))])).unwrap();

    let _endpoint = worker(&root);
    assert!(
        !numbered.parent().unwrap().exists(),
        "numbered state removed"
    );
    assert!(
        !old_records.parent().unwrap().exists(),
        "machine/*/sandbox removed"
    );
    assert!(!old_run.exists(), "sandbox-root renamed");
    let run = paths::run_root(&root);
    assert_eq!(std::fs::read(run.join("images/x")).unwrap(), b"image");
    assert_eq!(paths::names(&run).unwrap(), ["images"]);
    let thin = read_record(&paths::store_record(&root, "shop"))
        .unwrap()
        .unwrap();
    assert_eq!(thin.get("binding"), None);
    assert_eq!(thin.get_int("v"), Some(1));
    assert!(project.exists());
    assert!(root.join(".layout-3").exists());

    // a later start leaves what is there
    std::fs::create_dir_all(&numbered).unwrap();
    let _again = worker(&root);
    assert!(numbered.exists(), "the marker keeps it from running again");
    std::fs::remove_dir_all(scratch).unwrap();
}
