//! Routes by records and leases (#533 step 7): a Store call goes where its
//! Store's records say, a worker gets the records with the call, and leases
//! are held, read and cleaned.
use layerfs_api::home::{self, paths, write_record};
use layerfs_api::store::lease;
use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;
use std::path::PathBuf;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-route-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
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
fn a_missing_store_or_project_is_not_found() {
    let home = home_with(sqlite_record(t("native")));
    assert_eq!(read(&home, "nope/api").unwrap_err().0, "NotFound");
    assert_eq!(read(&home, "shop/web").unwrap_err().0, "NotFound");
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
fn a_postgresql_store_reads_its_url_from_the_named_variable() {
    let home = home_with(SdkValue::map([
        ("name", t("shop")),
        ("kind", t("postgresql")),
        ("url_env", t("LAYERFS_TEST_ROUTE_PG_UNSET")),
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
    // no worker yet: Unavailable, nothing sent
    assert_eq!(read(&home, "shop/api").unwrap_err().0, "Unavailable");

    let root = tempdir("guest");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let guest = root.clone();
    std::thread::spawn(move || layerfs_api::worker::serve(listener, "s3cret".to_owned(), guest));
    let worker =
        |secret: &str| SdkValue::map([("address", t(&address.to_string())), ("secret", t(secret))]);
    write_record(&paths::worker_record(&home, "box"), &worker("wrong")).unwrap();
    assert_eq!(read(&home, "shop/api").unwrap_err().0, "Denied");

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
