//! A worker's root is not a home (#533 review P2-32, O33): host code the
//! worker runs on it gets the caller's home id, sent with the request; the
//! root is never converted or given an id of its own, even when it holds
//! files a 1.1.0 home would.
use layerfs_bridge::control::SdkValue;
use layerfs_control::machine::worker::{self, Endpoint};
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-worker-root-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One Sandbox operation for project `shop/api`, as a caller sends it.
fn sandbox_call(endpoint: &Endpoint) -> Result<SdkValue, (String, String)> {
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
            SdkValue::map([("name", SdkValue::text("api"))]),
        ),
        ("sandbox", SdkValue::text("1")),
    ]);
    worker::call(endpoint, "sandbox.host", args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn untouched(root: &Path) {
    assert!(!root.join("layerfs.json").exists(), "no home id of its own");
    assert!(!root.join("log/migration.json").exists(), "not converted");
    assert!(!root.join("log/1.1.0").exists(), "nothing moved");
    assert!(root.join("stores.json").is_file());
    assert!(root.join(".stores/old").is_dir());
}

#[test]
fn a_worker_root_is_not_converted_and_uses_the_callers_home_id() {
    let scratch = tempdir("scratch");
    let root = scratch.join("data");
    // a data disk holding names a 1.1.0 home has
    std::fs::create_dir_all(root.join(".stores/old")).unwrap();
    std::fs::write(root.join("stores.json"), "{}").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint {
        address: listener.local_addr().unwrap(),
        secret: "k".to_owned(),
    };
    let guest = root.clone();
    std::thread::spawn(move || worker::serve(listener, "k".to_owned(), guest));
    // the worker is up once it answers
    for _ in 0..200 {
        if worker::ping(&endpoint).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // a caller that has opened no home sends no id: refused, nothing written
    let without = {
        let endpoint = endpoint.clone();
        std::thread::spawn(move || sandbox_call(&endpoint))
            .join()
            .unwrap()
    };
    let (code, message) = without.unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("no home id"), "{message}");
    untouched(&root);

    // a caller of a home sends its id; the Sandbox code gets past it
    let host = scratch.join("home");
    let id = layerfs_control::migrate::open(&host).unwrap().id;
    let with = sandbox_call(&endpoint);
    if let Err((_, message)) = &with {
        assert!(!message.contains("home id"), "{message}");
    }
    untouched(&root);
    // opening the root answers the id of the home this thread works for
    assert_eq!(layerfs_control::migrate::open(&root).unwrap().id, id);
    untouched(&root);
    std::fs::remove_dir_all(scratch).unwrap();
}

/// The 1.1 worker ops `host.call` and `host.backend` are gone (#536 step
/// 2): a caller from an older bundle is refused before anything runs, once.
#[test]
fn the_retired_host_ops_are_refused() {
    let root = tempdir("retired");
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
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    for op in ["host.call", "host.backend"] {
        let args = SdkValue::map([
            ("op", SdkValue::text("runtime.store.remove_project")),
            ("args", SdkValue::map([("store", SdkValue::text("shop"))])),
        ]);
        let refusal = worker::call(&endpoint, op, args, &mut |_| {}).unwrap_err();
        assert_eq!(refusal.code, "InvalidInput", "{}", refusal.message);
        assert_eq!(refusal.message, format!("the worker has no operation {op}"));
    }
    std::fs::remove_dir_all(root).unwrap();
}
