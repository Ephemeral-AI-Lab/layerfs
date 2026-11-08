//! A project Inited through a worker keeps its manifest when a Sandbox
//! call brings the caller's record (#535 final terminal pass, lane L2
//! L2-D1): the worker's own record (manifest, no `created`) takes the
//! caller's fields; a record of a removed project of the same name (another
//! `created`) is replaced.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{paths, read_record, write_record};
use layerfs_machine::worker::{self, Endpoint};
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-worker-project-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sandbox_call(endpoint: &Endpoint, created: i64) {
    let args = SdkValue::map([
        ("op", SdkValue::text("sandbox.list_workspaces")),
        ("store", SdkValue::text("shop")),
        ("project", SdkValue::text("api")),
        (
            "store_record",
            SdkValue::map([("name", SdkValue::text("shop"))]),
        ),
        (
            "project_record",
            SdkValue::map([
                ("v", SdkValue::Int(1)),
                ("name", SdkValue::text("api")),
                ("stack", SdkValue::text("ab")),
                ("created", SdkValue::Int(created)),
            ]),
        ),
        ("sandbox", SdkValue::text("1")),
    ]);
    // the operation itself fails later (no such Sandbox); the record is
    // settled before it runs
    let _ = worker::call(endpoint, "sandbox.host", args, &mut |_| {});
}

fn record(root: &Path) -> SdkValue {
    read_record(&paths::project_record(root, "shop", "api"))
        .unwrap()
        .unwrap()
}

#[test]
fn a_worker_inited_project_keeps_its_manifest() {
    let scratch = tempdir("scratch");
    let root = scratch.join("data");
    std::fs::create_dir_all(&root).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint {
        address: listener.local_addr().unwrap(),
        secret: "k".to_owned(),
    };
    let guest = root.clone();
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, "k".to_owned(), guest)
    });
    for _ in 0..200 {
        if worker::ping(&endpoint).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    layerfs_control::migrate::open(&scratch.join("home")).unwrap();
    let manifest = SdkValue::Bytes(vec![1, 2, 3]);
    let path = paths::project_record(&root, "shop", "api");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // what the worker's own Init wrote: the manifest, no `created`
    write_record(
        &path,
        &SdkValue::map([
            ("manifest", manifest.clone()),
            ("installed", SdkValue::Null),
        ]),
    )
    .unwrap();

    sandbox_call(&endpoint, 5);
    let kept = record(&root);
    assert_eq!(kept.get("manifest"), Some(&manifest), "{kept:?}");
    assert_eq!(kept.get("created"), Some(&SdkValue::Int(5)), "{kept:?}");
    assert_eq!(kept.get_text("stack"), Some("ab"), "{kept:?}");

    // the same project again: nothing changes
    sandbox_call(&endpoint, 5);
    assert_eq!(record(&root), kept);

    // a project of that name made again (another `created`): replaced
    sandbox_call(&endpoint, 9);
    let replaced = record(&root);
    assert_eq!(
        replaced.get("created"),
        Some(&SdkValue::Int(9)),
        "{replaced:?}"
    );
    assert_eq!(replaced.get("manifest"), None, "{replaced:?}");
    std::fs::remove_dir_all(scratch).unwrap();
}
