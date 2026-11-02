//! Routes by records and leases (#533 step 7): a Store call goes where its
//! Store's records say, a worker gets the records with the call, and leases
//! are held, read and cleaned.
use layerfs_api::home::{self, paths, write_record};
use layerfs_api::store::lease;
use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;
use std::path::PathBuf;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-route-{tag}-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A home with Store `shop` (record `store`) holding project `api`.
fn home_with(store: SdkValue) -> PathBuf {
    let home = tempdir("home");
    home::open(&home).unwrap();
    write_record(&paths::store_record(&home, "shop"), &store).unwrap();
    write_record(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    home
}

fn sqlite_record(machine: SdkValue) -> SdkValue {
    SdkValue::map([
        ("name", t("shop")),
        ("kind", t("sqlite")),
        ("binding", SdkValue::Bytes(vec![7; 16])),
        ("cursor", SdkValue::Bytes(vec![8; 32])),
        ("profile", t("disposable")),
        ("machine", machine),
    ])
}

fn read(home: &std::path::Path, project: &str) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::map([
        ("project", t(project)),
        ("branch", t("main")),
        ("path", t("/a")),
    ]);
    layerfs_api::calls::call(Some(home), "file.read", args, &mut |_| {})
        .map_err(|r| (r.code, r.message))
}

#[test]
fn a_missing_store_or_project_keeps_the_1_1_0_codes() {
    let home = home_with(sqlite_record(t("native")));
    assert_eq!(read(&home, "nope/api").unwrap_err().0, "NoSuchStore");
    assert_eq!(read(&home, "shop/web").unwrap_err().0, "NoSuchProject");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_converted_store_with_no_machine_says_how_to_place_it() {
    let home = home_with(sqlite_record(SdkValue::Null));
    let (code, message) = read(&home, "shop/api").unwrap_err();
    assert_eq!(code, "Unavailable");
    assert!(message.contains("import-env"), "{message}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_postgresql_store_resolves_its_password_reference_per_call() {
    let home = home_with(SdkValue::map([
        ("name", t("shop")),
        ("kind", t("postgresql")),
        ("url", t("postgresql://lfs@127.0.0.1:1/db")),
        ("password", t("env:LAYERFS_TEST_ROUTE_PG_UNSET")),
        ("schema", t("shop")),
        ("synchronous_commit", t("off")),
        ("binding", SdkValue::Bytes(vec![7; 16])),
        ("cursor", SdkValue::Bytes(vec![8; 32])),
    ]));
    let (code, message) = read(&home, "shop/api").unwrap_err();
    assert_eq!(code, "Unavailable");
    assert!(message.contains("LAYERFS_TEST_ROUTE_PG_UNSET"), "{message}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_store_on_another_machine_goes_to_its_worker_with_its_record() {
    let home = home_with(sqlite_record(t("box")));
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([("name", t("box")), ("platform", t("lima"))]),
    )
    .unwrap();
    // no worker yet: refused, nothing sent (with the machine adapters the
    // caller first tries to start the machine, which has no VM here)
    assert!(read(&home, "shop/api").is_err());

    let root = tempdir("guest");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let guest = root.clone();
    std::thread::spawn(move || layerfs_api::worker::serve(listener, "s3cret".to_owned(), guest));
    let worker =
        |secret: &str| SdkValue::map([("address", t(&address.to_string())), ("secret", t(secret))]);
    // a wrong secret is refused by the worker (with the adapters, a worker
    // that does not answer its secret counts as not running)
    #[cfg(not(feature = "machine"))]
    {
        write_record(&paths::worker_record(&home, "box"), &worker("wrong")).unwrap();
        assert_eq!(read(&home, "shop/api").unwrap_err().0, "Denied");
    }

    write_record(&paths::worker_record(&home, "box"), &worker("s3cret")).unwrap();
    let (code, message) = read(&home, "shop/api").unwrap_err();
    // the guest has no Store file under its root: the worker's own refusal,
    // naming its path (or, on a kernel that cannot open sqlite Stores
    // directly, Unsupported)
    assert!(
        matches!(code.as_str(), "NoStore" | "Unsupported" | "Failed"),
        "{code}: {message}"
    );
    if code != "Unsupported" {
        assert!(
            message.contains(&root.display().to_string()) || message.contains("store"),
            "{message}"
        );
    }
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn leases_are_held_counted_and_stale_ones_removed() {
    let home = home_with(sqlite_record(t("native")));
    let mine = paths::lease_dir(&home, "shop").join(std::process::id().to_string());
    let first = lease::hold(&home, "shop").unwrap();
    let second = lease::hold(&home, "shop").unwrap();
    assert!(mine.is_file());
    drop(first);
    assert!(mine.is_file(), "the second holder keeps it");
    drop(second);
    assert!(!mine.exists());

    // a lease of a process that is gone is stale: removed and logged
    let dead = paths::lease_dir(&home, "shop").join("2147483646");
    std::fs::write(&dead, "1\n").unwrap();
    assert!(!lease::alive(2147483646));
    assert_eq!(lease::holders(&home, "shop").unwrap(), Vec::<u32>::new());
    assert!(!dead.exists());
    let log = std::fs::read_to_string(home.join("log/leases.jsonl")).unwrap();
    assert!(log.contains("2147483646"), "{log}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_lease_names_its_process_so_a_reused_or_group_pid_never_holds() {
    // ids 0 and above i32::MAX are process groups to kill(2): never alive
    for pid in [0, i32::MAX as u32 + 1, u32::MAX] {
        assert!(!lease::alive(pid), "{pid}");
    }
    let home = home_with(sqlite_record(t("native")));
    let dir = paths::lease_dir(&home, "shop");
    std::fs::create_dir_all(&dir).unwrap();
    for pid in ["0", "4294967295"] {
        std::fs::write(dir.join(pid), "1\n").unwrap();
    }
    assert_eq!(lease::holders(&home, "shop").unwrap(), Vec::<u32>::new());
    assert!(!dir.join("0").exists() && !dir.join("4294967295").exists());

    // a live process holds with its start time; the same id with another
    // start time is a reused id: stale
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = child.id();
    let seen = layerfs_sandbox::local::LocalProcess::observe(pid).unwrap();
    let boot = seen.boot.unwrap();
    let lease = dir.join(pid.to_string());
    std::fs::write(&lease, format!("1 {} {boot:032x}\n", seen.start)).unwrap();
    assert_eq!(lease::holders(&home, "shop").unwrap(), vec![pid]);
    std::fs::write(&lease, format!("1 {} {boot:032x}\n", seen.start + 1)).unwrap();
    assert_eq!(lease::holders(&home, "shop").unwrap(), Vec::<u32>::new());
    assert!(!lease.exists(), "the reused id's lease is removed");

    // this process's own lease records its start time
    let mine = lease::hold(&home, "shop").unwrap();
    let text = std::fs::read_to_string(dir.join(std::process::id().to_string())).unwrap();
    let me = layerfs_sandbox::local::LocalProcess::observe(std::process::id()).unwrap();
    assert!(text.contains(&me.start.to_string()), "{text}");
    drop(mine);
    let _ = child.kill();
    let _ = child.wait();
    std::fs::remove_dir_all(home).unwrap();
}
