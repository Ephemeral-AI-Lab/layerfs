//! The Stores a process keeps open (#533 review, group store): a worker's
//! open copy holds a lease under its root and is closed by the steps that
//! read, replace or delete the file, so a Store moves back at once after a
//! write and a Store that returns is never served from a stale handle; and
//! (Linux, where a writer waits on its Store directory's lock) a call that
//! is running keeps its Store open against a move or remove.
use layerfs_api::calls;
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-cache-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
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
    calls::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

/// A private home with `native` and worker machine `box` (served in this
/// process), and project `shop/api` Inited on native from a README, with
/// Branch `fix` forked from `main`.
fn home_with_project() -> (PathBuf, PathBuf, PathBuf) {
    let home = tempdir("home");
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&home).unwrap();
    for (name, platform) in [("native", "native"), ("box", "lima")] {
        write_record(
            &paths::machine_record(&home, name),
            &SdkValue::map([("name", t(name)), ("platform", t(platform))]),
        )
        .unwrap();
    }
    let root = tempdir("guest");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let guest = root.clone();
    std::thread::spawn(move || layerfs_api::worker::serve(listener, "k".to_owned(), guest));
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&address)), ("secret", t("k"))]),
    )
    .unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "zero\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    ok(
        &home,
        "branch.fork",
        &[
            ("project", t("shop/api")),
            ("name", t("fix")),
            ("from_branch", t("main")),
        ],
    );
    (home, root, folder)
}

fn write(home: &Path, text: &str) -> Result<SdkValue, (String, String)> {
    call(
        home,
        "file.write",
        &[
            ("project", t("shop/api")),
            ("branch", t("fix")),
            ("path", t("/README.md")),
            ("content", t(text)),
            ("message", t("write")),
        ],
    )
}

fn read(home: &Path) -> String {
    let read = ok(
        home,
        "file.read",
        &[
            ("project", t("shop/api")),
            ("branch", t("fix")),
            ("path", t("/README.md")),
        ],
    );
    match read.get("text") {
        Some(SdkValue::Text(text)) => text.clone(),
        Some(SdkValue::Bytes(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
        other => panic!("no content: {other:?}"),
    }
}

fn to(home: &Path, machine: &str) -> Result<SdkValue, (String, String)> {
    call(
        home,
        "store.move",
        &[("name", t("shop")), ("machine", t(machine))],
    )
}

#[test]
fn a_worker_closes_its_open_copy_so_a_store_moves_back_at_once_and_returns_fresh() {
    let (home, root, folder) = home_with_project();
    to(&home, "box").unwrap();
    // a write in the worker leaves the Store open there, under its lease
    write(&home, "one\n").unwrap();
    let worker_lease = paths::lease_dir(&root, "shop").join(std::process::id().to_string());
    assert!(worker_lease.is_file(), "the worker holds its open copy");
    // within the worker's idle minute: the snapshot closes that copy first
    to(&home, "native").unwrap();
    assert!(!worker_lease.exists());
    assert!(!paths::store_file(&root, "shop").exists());
    assert_eq!(read(&home), "one\n");

    // the Store changes on native and returns: the worker opens it afresh
    write(&home, "two\n").unwrap();
    to(&home, "box").unwrap();
    assert_eq!(read(&home), "two\n", "never the retired copy");
    ok(&home, "store.remove", &[("name", t("shop"))]);
    assert!(!paths::store_dir(&root, "shop").exists());
    assert!(!paths::store_dir(&home, "shop").exists());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(folder);
}

/// A write waits for the Store directory's writer lock (Linux): holding it
/// here keeps one call running for as long as this test wants.
#[cfg(target_os = "linux")]
#[test]
fn a_running_call_keeps_its_store_open_against_a_move_or_remove() {
    use nix::fcntl::{Flock, FlockArg};
    use std::time::Duration;
    let (home, root, folder) = home_with_project();
    // the Store opens first (opening it takes the writer lock once); then
    // the writer lock is held here and a write waits for it, running
    assert_eq!(read(&home), "zero\n");
    let directory = std::fs::File::open(paths::store_dir(&home, "shop")).unwrap();
    let writer = Flock::lock(directory, FlockArg::LockExclusive).unwrap();
    let running = {
        let home = home.clone();
        std::thread::spawn(move || write(&home, "slow\n"))
    };
    std::thread::sleep(Duration::from_secs(1));
    assert!(
        !running.is_finished(),
        "the write waits for the writer lock"
    );
    for (name, args) in [
        ("store.remove", vec![("name", t("shop"))]),
        (
            "store.move",
            vec![("name", t("shop")), ("machine", t("box"))],
        ),
    ] {
        let (code, message) = call(&home, name, &args).unwrap_err();
        assert_eq!(code, "InUse", "{name}: {message}");
        assert!(message.contains("a call is running"), "{name}: {message}");
    }
    let lease = paths::lease_dir(&home, "shop").join(std::process::id().to_string());
    assert!(lease.is_file(), "the running call keeps its lease");
    assert!(paths::store_file(&home, "shop").is_file());
    drop(writer.unlock().unwrap());
    running.join().unwrap().unwrap();
    assert_eq!(read(&home), "slow\n");
    ok(&home, "store.remove", &[("name", t("shop"))]);
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(folder);
}
