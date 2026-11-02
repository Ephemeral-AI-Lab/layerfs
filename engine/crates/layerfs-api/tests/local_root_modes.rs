//! The folders above a namespaces Sandbox's local root (#533 terminal pass):
//! its commands run as another identity, so the worker's root and the home's
//! folders down to `machine/native/sandbox-root` must let others pass (o+x),
//! while staying unlistable. Folders above the home are left alone.
use layerfs_api::home;
use layerfs_api::worker;
use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-lrm-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn set(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn the_home_s_folders_down_to_the_local_root_let_others_pass() {
    let top = tempdir("home");
    let home = top.join("h");
    let local = home.join("machine/native/sandbox-root");
    std::fs::create_dir_all(&local).unwrap();
    for folder in [&home, &home.join("machine"), &home.join("machine/native")] {
        set(folder, 0o700);
    }
    set(&local, 0o755);
    set(&top, 0o750);
    home::traversable(&home, &local).unwrap();
    for folder in [&home, &home.join("machine"), &home.join("machine/native")] {
        assert_eq!(mode(folder), 0o701, "{}", folder.display());
    }
    assert_eq!(
        mode(&local),
        0o755,
        "an already passable folder is unchanged"
    );
    assert_eq!(mode(&top), 0o750, "a folder above the home is not changed");
    std::fs::remove_dir_all(&top).unwrap();
}

#[test]
fn a_worker_s_root_is_passable_and_unlistable() {
    let top = tempdir("worker");
    let root = top.join("data");
    std::fs::create_dir(&root).unwrap();
    set(&root, 0o700);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let served = root.clone();
    std::thread::spawn(move || {
        worker::serve_until_idle(
            listener,
            "s".to_owned(),
            served,
            Duration::from_secs(30),
            Duration::from_millis(200),
        )
    });
    let endpoint = worker::Endpoint {
        address,
        secret: "s".to_owned(),
    };
    worker::call(
        &endpoint,
        "worker.ping",
        SdkValue::Map(Vec::new()),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(mode(&root), 0o711);
    let _ = std::fs::remove_dir_all(&top);
}
