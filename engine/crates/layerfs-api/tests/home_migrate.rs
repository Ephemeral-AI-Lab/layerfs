//! The one conversion of a 1.1.0 home into the 2.0 layout (#533 step 4): a
//! 1.1.0 home with every kind of record converts, and a second run changes
//! nothing.
use layerfs_api::home::{self, paths, read_record, Layout};
use layerfs_bridge::control::{Answer, Reply, SdkValue};
use layerfs_bridge::provision::{ProviderKind, StoreManifest, StoreProfile};
use layerfs_store_service::remote::storage::{minio::MinioSettings, CredentialRef};
use layerfs_store_service::remote::{state, Direction, Provider, Remotes, Transfer};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-migrate-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .unwrap();
    dir
}

fn dir(path: &Path) {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .unwrap();
}

/// A private binary record, as 1.1.0's binding wrote it.
fn record(path: &Path, value: SdkValue) {
    use std::io::Write;
    let bytes = Answer {
        id: 1,
        reply: Reply::Sdk(value),
    }
    .encode()
    .unwrap();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(&bytes).unwrap();
}

fn manifest(binding: u8) -> Vec<u8> {
    StoreManifest {
        provider: ProviderKind::Sqlite,
        locator: "/layerfs-store/global/store.sqlite".to_owned(),
        profile: StoreProfile::Disposable,
        binding: vec![binding; 16],
        cursor_key: [binding; 32],
        stack: [1; 17],
        branch: [2; 17],
        root: [3; 32],
        bytes: 4096,
        host_sqlite: "3.50.0".to_owned(),
        daemon_sqlite: None,
    }
    .encode()
    .unwrap()
}

fn project_record(dir_path: &Path, binding: u8) {
    dir(dir_path);
    record(
        &dir_path.join("project.record"),
        SdkValue::map([
            ("manifest", SdkValue::Bytes(manifest(binding))),
            ("installed", SdkValue::Null),
        ]),
    );
}

/// Every file under `root`, with its bytes, for comparing two runs.
fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_owned()];
    while let Some(path) = stack.pop() {
        for entry in std::fs::read_dir(&path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else if entry.file_name() != "layerfs.lock" {
                out.push((entry.path(), std::fs::read(entry.path()).unwrap()));
            }
        }
    }
    out.sort();
    out
}

/// A 1.1.0 home with every kind of record.
fn old_home() -> PathBuf {
    let home = tempdir();
    // a Store with a record of its own and one project
    dir(&home.join(".stores/shop/projects"));
    record(
        &home.join(".stores/shop/store.record"),
        SdkValue::map([
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("kind", SdkValue::text("sqlite")),
        ]),
    );
    project_record(&home.join(".stores/shop/projects/api"), 7);
    // a project with a Store of its own, and one of its numbered Sandboxes
    project_record(&home.join("solo"), 9);
    dir(&home.join("solo/sandboxes/1"));
    // the runtime's registry: shop is in a Docker volume
    std::fs::write(
        home.join("stores.json"),
        r#"{"version":2,"stores":[{"name":"shop","kind":"sqlite","volume":"lfs-shop","created":1700000000000,"projects":[{"name":"api"}],"link":null}]}"#,
    )
    .unwrap();
    // remotes and shop's link
    let mut remotes = Remotes::default();
    remotes
        .set(
            "minio1",
            Provider::Minio(MinioSettings {
                endpoint: "http://127.0.0.1:9000".to_owned(),
                bucket: "lfs".to_owned(),
                prefix: "stores".to_owned(),
                region: "us-east-1".to_owned(),
                credentials: CredentialRef::parse("env:LAYERFS_MINIO1").unwrap(),
                part_size: 8 << 20,
            }),
        )
        .unwrap();
    remotes
        .set("folder", Provider::LocalFolder(home.join("elsewhere")))
        .unwrap();
    remotes.save(&Remotes::home_path(&home)).unwrap();
    state::save(
        &state::state_path(&home, "shop").unwrap(),
        &Transfer::now("minio1", "shop", Direction::Push, [5; 32], 4096).with_heads(Some([6; 32])),
    )
    .unwrap();
    // the runtime's state and traces, and one file nobody knows
    dir(&home.join(".runtime/projects"));
    std::fs::write(
        home.join(".runtime/init-failures.jsonl"),
        "{\"project\":\"x\"}\n",
    )
    .unwrap();
    dir(&home.join("traces"));
    std::fs::write(home.join("traces/a.jsonl"), "{}\n").unwrap();
    std::fs::write(home.join("notes.txt"), "mine").unwrap();
    home
}

#[test]
fn a_1_1_0_home_converts_once() {
    let home = old_home();
    assert_eq!(home::detect(&home).unwrap(), Layout::Old);
    let file = home::open(&home).unwrap();
    assert!(matches!(home::detect(&home).unwrap(), Layout::Current(_)));

    let shop = read_record(&paths::store_record(&home, "shop"))
        .unwrap()
        .unwrap();
    assert_eq!(shop.get_text("kind"), Some("sqlite"));
    assert_eq!(shop.get("binding"), Some(&SdkValue::Bytes(vec![7; 16])));
    assert_eq!(shop.get_text("machine"), Some("docker"));
    assert_eq!(shop.get_text("volume"), Some("lfs-shop"));
    assert_eq!(shop.get_text("profile"), Some("disposable"));
    let link = shop
        .get("backups")
        .and_then(|b| b.get("minio1"))
        .expect("shop's link");
    assert_eq!(link.get_text("file"), Some("shop"));
    assert_eq!(link.get("heads"), Some(&SdkValue::Bytes(vec![6; 32])));
    assert_eq!(
        link.get("last").and_then(|l| l.get_text("op")),
        Some("create")
    );

    let api = read_record(&paths::project_record(&home, "shop", "api"))
        .unwrap()
        .unwrap();
    assert_eq!(api.get("manifest"), Some(&SdkValue::Bytes(manifest(7))));
    assert!(!paths::project_dir(&home, "shop", "api")
        .join("project.record")
        .exists());

    let solo = read_record(&paths::store_record(&home, "solo"))
        .unwrap()
        .unwrap();
    assert_eq!(solo.get("binding"), Some(&SdkValue::Bytes(vec![9; 16])));
    assert_eq!(
        solo.get("machine"),
        Some(&SdkValue::Null),
        "machine unknown until import-env"
    );
    assert!(paths::project_record(&home, "solo", "solo").is_file());
    assert!(
        paths::project_dir(&home, "solo", "solo")
            .join("sandboxes/1")
            .is_dir(),
        "Sandboxes move in step 12"
    );
    assert!(!home.join("solo").exists());

    let bucket = read_record(&paths::bucket_record(&home, "minio1"))
        .unwrap()
        .unwrap();
    assert_eq!(bucket.get_text("provider"), Some("minio"));
    assert_eq!(bucket.get_text("key"), Some("env:LAYERFS_MINIO1"));
    assert!(
        !paths::bucket_record(&home, "folder").exists(),
        "local_folder is not in 2.0"
    );

    assert!(home.join("log/init-failures.jsonl").is_file());
    assert!(home.join("trace/a.jsonl").is_file());
    for old in [
        ".stores",
        "stores.json",
        "remotes",
        "remote-state",
        ".runtime",
    ] {
        assert!(
            home.join("log/1.1.0").join(old).exists(),
            "{old} kept under log/1.1.0"
        );
        assert!(!home.join(old).exists(), "{old} moved");
    }
    assert!(
        home.join("notes.txt").is_file(),
        "an unknown file is left alone"
    );
    let report = read_record(&home.join("log/migration.json"))
        .unwrap()
        .unwrap();
    let unknown = report.get("unknown").unwrap();
    assert_eq!(unknown, &SdkValue::List(vec![SdkValue::text("notes.txt")]));
    let notes = layerfs_api::calls::json::print(report.get("notes").unwrap());
    assert!(notes.contains("local_folder"), "{notes}");

    // a second run changes nothing
    let before = snapshot(&home);
    assert_eq!(home::open(&home).unwrap(), file);
    assert_eq!(snapshot(&home), before);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn an_empty_home_is_created_and_the_lock_waits_rather_than_refusing() {
    let home = tempdir();
    let file = home::open(&home).unwrap();
    assert_eq!(file.id.len(), 32);
    // one holder at a time: the second take waits for the first to drop
    let held = home::lock::take(&home).unwrap();
    let path = home.clone();
    let waiter = std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let lock = home::lock::take(&path).map(|_| ());
        (lock, started.elapsed())
    });
    std::thread::sleep(std::time::Duration::from_millis(200));
    drop(held);
    let (taken, waited) = waiter.join().unwrap();
    assert!(taken.is_ok());
    assert!(
        waited >= std::time::Duration::from_millis(150),
        "{waited:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
}
