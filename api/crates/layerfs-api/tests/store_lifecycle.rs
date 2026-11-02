//! Store lifecycle races and machines (#533 review, group store): a lease
//! and a move or remove never overlap, even inside the mover's own process;
//! `store.status` starts no machine; `store.remove` refuses a Store whose
//! machine is unknown and never starts a machine under the home lock. All
//! from records: no Store is opened, so these run on any host.
use layerfs_api::calls;
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_api::store::lease;
use layerfs_bridge::control::SdkValue;
use layerfs_sandbox::local::LocalProcess;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-life-{tag}-{}-{}",
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

fn machine(home: &Path, name: &str, platform: &str) {
    write_record(
        &paths::machine_record(home, name),
        &SdkValue::map([("name", t(name)), ("platform", t(platform))]),
    )
    .unwrap();
}

/// A home with `native`, a worker machine `box` served in this process,
/// and Store `shop` created on native with project `api` recorded.
fn home_with_shop() -> (PathBuf, PathBuf) {
    let home = tempdir("home");
    home::open(&home).unwrap();
    machine(&home, "native", "native");
    machine(&home, "box", "lima");
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
    call(&home, "store.create", &[("name", t("shop"))]).unwrap();
    write_record(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    (home, root)
}

fn record(home: &Path) -> SdkValue {
    read_record(&paths::store_record(home, "shop"))
        .unwrap()
        .unwrap()
}

fn set_in_progress(home: &Path, value: SdkValue) {
    let path = paths::store_record(home, "shop");
    write_record(&path, &home::with_field(record(home), "in_progress", value)).unwrap();
}

/// `in_progress` naming this process, its start time moved by `skew`.
fn this_process_moving(skew: u64) -> SdkValue {
    let me = LocalProcess::observe(std::process::id()).unwrap();
    SdkValue::map([
        ("op", t("move")),
        ("from", t("native")),
        ("to", t("box")),
        ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ("pid_start", t(&(me.start + skew).to_string())),
        ("pid_boot", t(&format!("{:032x}", me.boot.unwrap()))),
    ])
}

fn read(home: &Path) -> Result<SdkValue, (String, String)> {
    call(
        home,
        "file.read",
        &[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("path", t("/a")),
        ],
    )
}

#[test]
fn a_store_call_during_a_move_is_refused_even_from_the_movers_process() {
    let (home, root) = home_with_shop();
    // a move this process runs: its other threads' calls and leases wait
    set_in_progress(&home, this_process_moving(0));
    let (code, message) = read(&home).unwrap_err();
    assert_eq!(code, "InUse", "{message}");
    assert!(message.contains("being moved"), "{message}");
    assert_eq!(
        lease::hold(&home, "shop").err().map(|r| r.code).as_deref(),
        Some("InUse")
    );
    assert_eq!(
        call(&home, "store.remove", &[("name", t("shop"))])
            .unwrap_err()
            .0,
        "InUse"
    );
    assert!(
        !paths::lease_dir(&home, "shop")
            .join(std::process::id().to_string())
            .exists(),
        "no lease was taken"
    );
    let status = call(&home, "store.status", &[("name", t("shop"))]).unwrap();
    assert_eq!(status.get_text("state"), Some("moving"));

    // the same id with another start time is a mover that is gone
    set_in_progress(&home, this_process_moving(1));
    let refused = read(&home).err().map(|(code, _)| code);
    assert_ne!(refused.as_deref(), Some("InUse"));
    drop(lease::hold(&home, "shop").unwrap());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_move_or_remove_waits_for_a_lease_another_thread_of_its_process_holds() {
    let (home, root) = home_with_shop();
    let held = lease::hold(&home, "shop").unwrap();
    for (name, args) in [
        (
            "store.move",
            vec![("name", t("shop")), ("machine", t("box"))],
        ),
        ("store.remove", vec![("name", t("shop"))]),
    ] {
        let (code, message) = call(&home, name, &args).unwrap_err();
        assert_eq!(code, "InUse", "{name}: {message}");
        assert!(message.contains("this process"), "{name}: {message}");
        // nothing moved: the refusal is certain, so `in_progress` is cleared
        assert_eq!(record(&home).get("in_progress"), Some(&SdkValue::Null));
        assert_eq!(record(&home).get_text("machine"), Some("native"));
        assert!(paths::store_file(&home, "shop").is_file());
    }
    drop(held);
    call(
        &home,
        "store.move",
        &[("name", t("shop")), ("machine", t("box"))],
    )
    .unwrap();
    call(&home, "store.remove", &[("name", t("shop"))]).unwrap();
    assert!(!paths::store_dir(&home, "shop").exists());
    assert!(!paths::store_dir(&root, "shop").exists());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// A home whose Store `shop` is on machine `vm` (a lima VM that is not
/// running: no worker recorded) or on an unknown machine (`Null`).
fn home_with_stopped(machine_field: SdkValue) -> PathBuf {
    let home = tempdir("stopped");
    home::open(&home).unwrap();
    machine(&home, "native", "native");
    machine(&home, "vm", "lima");
    write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", machine_field),
        ]),
    )
    .unwrap();
    home
}

#[test]
fn status_of_a_store_on_a_stopped_machine_starts_nothing_and_says_down() {
    let home = home_with_stopped(t("vm"));
    let status = call(&home, "store.status", &[("name", t("shop"))]).unwrap();
    assert_eq!(status.get_text("state"), Some("down"));
    let reason = status.get_text("down_reason").unwrap_or("");
    assert!(reason.contains("not running"), "{reason}");
    assert!(
        !paths::machine_dir(&home, "vm").join("start.lock").exists(),
        "no start was attempted"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn remove_refuses_a_store_whose_machine_is_unknown() {
    let home = home_with_stopped(SdkValue::Null);
    let (code, message) = call(&home, "store.remove", &[("name", t("shop"))]).unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("import-env"), "{message}");
    assert!(paths::store_record(&home, "shop").is_file(), "records kept");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn remove_reaches_a_stopped_machine_before_it_takes_the_home_lock() {
    let home = home_with_stopped(t("vm"));
    // this thread holds the home lock: a remove that took it before
    // starting the machine would wait 120 s for it
    let lock = home::lock::take(&home).unwrap();
    let started = Instant::now();
    let (code, message) = call(&home, "store.remove", &[("name", t("shop"))]).unwrap_err();
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{code}: {message}"
    );
    assert!(!message.contains("layerfs.lock"), "{code}: {message}");
    drop(lock);
    assert_eq!(record(&home).get("in_progress"), None, "nothing was marked");
    assert!(paths::store_record(&home, "shop").is_file());
    std::fs::remove_dir_all(home).unwrap();
}

/// `docker` on Linux opens its Stores in this process, from the home: a
/// remove deletes the Store's directory once and answers success.
#[cfg(all(target_os = "linux", feature = "machine"))]
#[test]
fn remove_of_a_store_on_an_in_process_machine_deletes_it_once() {
    let (home, root) = home_with_shop();
    machine(&home, "docker", "docker");
    let path = paths::store_record(&home, "shop");
    write_record(
        &path,
        &home::with_field(record(&home), "machine", t("docker")),
    )
    .unwrap();
    call(&home, "store.remove", &[("name", t("shop"))]).unwrap();
    assert!(!paths::store_dir(&home, "shop").exists());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}
