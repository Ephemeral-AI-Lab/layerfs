//! A docker Sandbox's deploy settings are resolved by the side that runs
//! the call (#535 phase 4): the caller sends only what the docker machine's
//! record chooses (`backend`, `image`, the worker's `data_volume`), and the
//! worker resolves the rest under its own root. Here the worker's step that
//! needs no Docker is seen: it makes the Store's folder and file the
//! container root user's alone (0700, 0600) before anything else runs.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::paths;
use layerfs_machine::worker::{self, Endpoint};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-deploy-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn mode(path: &std::path::Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn the_worker_resolves_a_docker_sandbox_s_store_folder() {
    let scratch = tempdir("scratch");
    let root = scratch.join("data");
    let folder = paths::store_dir(&root, "shop");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    let file = paths::store_file(&root, "shop");
    std::fs::write(&file, b"").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();

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
        std::thread::sleep(Duration::from_millis(10));
    }
    layerfs_control::migrate::open(&scratch.join("home")).unwrap();
    let digest = format!("sha256:{}", "a".repeat(64));
    let args = SdkValue::map([
        ("op", SdkValue::text("sandbox.list_workspaces")),
        ("store", SdkValue::text("shop")),
        ("project", SdkValue::text("api")),
        (
            "store_record",
            SdkValue::map([
                ("name", SdkValue::text("shop")),
                ("kind", SdkValue::text("sqlite")),
            ]),
        ),
        (
            "project_record",
            SdkValue::map([("name", SdkValue::text("api"))]),
        ),
        (
            "deploy",
            SdkValue::map([
                ("backend", SdkValue::text("docker")),
                ("image", SdkValue::text(&digest)),
                ("data_volume", SdkValue::text("lfs-0000-docker-data")),
            ]),
        ),
        ("more", SdkValue::map([("sandbox", SdkValue::text("1"))])),
    ]);
    // no Docker or Store here: the call fails after the settings resolved
    let _ = worker::call(&endpoint, "sandbox.host", args, &mut |_| {});
    assert_eq!(mode(&folder), 0o700);
    assert_eq!(mode(&file), 0o600);
    std::fs::remove_dir_all(scratch).unwrap();
}
