//! The one conversion of a 1.1.0 home into the 2.0 layout (#533 step 4),
//! then on into layout 3 (#536 §5): a 1.1.0 home with every kind of record
//! converts, and a second run changes nothing.
use layerfs_api::home::{self, paths, read_record, Layout};
use layerfs_bridge::control::{Answer, Reply, SdkValue};
use layerfs_bridge::provision::{ProviderKind, StoreManifest, StoreProfile};
use layerfs_bucket::backup::{link as state, Direction, Provider, Remotes, Transfer};
use layerfs_bucket::bucket::{minio::MinioSettings, CredentialRef};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-migrate-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
    let file = home::open_from(&home, &home.join("no-volumes")).unwrap();
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
        !paths::project_dir(&home, "solo", "solo")
            .join("sandboxes")
            .exists(),
        "layout 3 keeps no numbered Sandbox folder (#536 §5)"
    );
    assert_eq!(file.layout, home::LAYOUT_VERSION);
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
    let notes = layerfs_api::json::print(report.get("notes").unwrap());
    assert!(notes.contains("local_folder"), "{notes}");
    assert!(
        notes.contains("stays in Docker volume lfs-shop"),
        "a Store file this host cannot read is reported: {notes}"
    );

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

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {})
        .unwrap_or_else(|r| panic!("{name}: {} {}", r.code, r.message))
}

/// A real Store made by this build, holding project `shop/api`, closed:
/// its file and its Store and project records. It is made on `native`
/// and moved to a worker machine, which leaves it closed under the
/// worker's root.
fn closed_store(scratch: &Path) -> (PathBuf, SdkValue, SdkValue) {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch.join("made");
    let root = scratch.join("guest");
    let folder = scratch.join("folder");
    for path in [&home, &root, &folder] {
        dir(path);
    }
    home::open(&home).unwrap();
    home::write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([("name", t("box")), ("platform", t("lima"))]),
    )
    .unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let guest = root.clone();
    std::thread::spawn(move || layerfs_api::worker::serve(listener, "k".to_owned(), guest));
    home::write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&address)), ("secret", t("k"))]),
    )
    .unwrap();
    std::fs::write(folder.join("README.md"), "hello\n").unwrap();
    call(&home, "store.create", &[("name", t("shop"))]);
    call(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    let store = read_record(&paths::store_record(&home, "shop"))
        .unwrap()
        .unwrap();
    let project = read_record(&paths::project_record(&home, "shop", "api"))
        .unwrap()
        .unwrap();
    call(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("box"))],
    );
    (paths::store_file(&root, "shop"), store, project)
}

#[test]
fn a_1_1_0_store_file_is_placed_and_opens() {
    let scratch = tempdir();
    let (file, store, project) = closed_store(&scratch);
    // the 1.1.0 layout: the file in its Docker volume, the records in the home
    let volumes = scratch.join("volumes");
    let in_volume = volumes.join("lfs-shop/_data/global/store.sqlite");
    dir(in_volume.parent().unwrap());
    std::fs::rename(&file, &in_volume).unwrap();
    let home = scratch.join("old");
    dir(&home.join(".stores/shop/projects/api"));
    record(
        &home.join(".stores/shop/store.record"),
        SdkValue::map([
            ("binding", store.get("binding").unwrap().clone()),
            ("cursor", store.get("cursor").unwrap().clone()),
            ("kind", t("sqlite")),
        ]),
    );
    record(
        &home.join(".stores/shop/projects/api/project.record"),
        SdkValue::map([
            ("manifest", project.get("manifest").unwrap().clone()),
            ("installed", project.get("installed").unwrap().clone()),
        ]),
    );
    // a second Store whose 1.1.0 file was left with a write-ahead log
    dir(&home.join(".stores/busy"));
    record(
        &home.join(".stores/busy/store.record"),
        SdkValue::map([
            ("binding", SdkValue::Bytes(vec![1; 16])),
            ("cursor", SdkValue::Bytes(vec![2; 32])),
            ("kind", t("sqlite")),
        ]),
    );
    let busy = volumes.join("lfs-busy/_data/global/store.sqlite");
    dir(busy.parent().unwrap());
    std::fs::write(&busy, "main").unwrap();
    std::fs::write(
        volumes.join("lfs-busy/_data/global/store.sqlite-wal"),
        "log",
    )
    .unwrap();
    std::fs::write(
        home.join("stores.json"),
        r#"{"version":2,"stores":[{"name":"shop","kind":"sqlite","volume":"lfs-shop"},{"name":"busy","kind":"sqlite","volume":"lfs-busy"}]}"#,
    )
    .unwrap();

    home::open_from(&home, &volumes).unwrap();
    let shop = read_record(&paths::store_record(&home, "shop"))
        .unwrap()
        .unwrap();
    assert_eq!(shop.get_text("machine"), Some("native"));
    assert!(paths::store_file(&home, "shop").is_file());
    assert!(in_volume.is_file(), "the 1.1.0 file stays in its volume");
    let busy_record = read_record(&paths::store_record(&home, "busy"))
        .unwrap()
        .unwrap();
    assert_eq!(busy_record.get_text("machine"), Some("docker"));
    assert!(!paths::store_file(&home, "busy").exists());
    assert_eq!(std::fs::read(&busy).unwrap(), b"main", "left as it was");
    let report = read_record(&home.join("log/migration.json"))
        .unwrap()
        .unwrap();
    assert_eq!(report.get("placed"), Some(&SdkValue::List(vec![t("shop")])));
    let notes = layerfs_api::json::print(report.get("notes").unwrap());
    assert!(notes.contains("write-ahead log"), "{notes}");

    // Store calls open the migrated Store
    let branches = call(&home, "branch.list", &[("project", t("shop/api"))]);
    let printed = layerfs_api::json::print(&branches);
    assert!(printed.contains("\"main\""), "{printed}");
    let read = call(
        &home,
        "file.read",
        &[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("path", t("/README.md")),
        ],
    );
    assert_eq!(read.get_text("text"), Some("hello\n"), "{read:?}");
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn a_new_home_is_not_a_1_1_0_home() {
    // a fresh folder, and one a stopped first call left with its lock and a
    // log line, and one holding a file of the user's
    for leftover in [None, Some("layerfs.lock"), Some("notes.txt")] {
        let home = tempdir();
        if let Some(name) = leftover {
            std::fs::write(home.join(name), "").unwrap();
            dir(&home.join("log"));
        }
        assert_eq!(home::detect(&home).unwrap(), Layout::Empty);
        let file = home::open(&home).unwrap();
        assert_eq!(home::detect(&home).unwrap(), Layout::Current(file));
        assert!(
            !home.join("log/migration.json").exists(),
            "{leftover:?}: no conversion report"
        );
        assert!(!home.join("log/1.1.0").exists(), "{leftover:?}");
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[test]
fn a_1_1_0_environment_becomes_records_once() {
    let home = old_home();
    let env: Vec<(String, String)> = [
        ("LAYERFS_SANDBOX", "docker"),
        ("LAYERFS_VOLUME", "lfs-solo"),
        ("LAYERFS_DOCKER", "unix:///tmp/docker.sock"),
        ("LAYERFS_IMAGE", "sha256:abc"),
        ("LAYERFS_DAEMON", "/opt/layerfs/layerfs-daemon"),
        ("LAYERFS_REMOTE_ENDPOINT", "http://127.0.0.1:9000"),
        ("LAYERFS_REMOTE_BUCKET", "lfs"),
        ("LAYERFS_REMOTE_ACCESS_KEY", "secret-access"),
        ("LAYERFS_REMOTE_SECRET_KEY", "secret-key"),
        ("LAYERFS_SSH_HOST", "ec2.example"),
        ("LAYERFS_SSH_USER", "ubuntu"),
        ("LAYERFS_STORE_POSTGRESQL", "postgres://u:pw@db/x"),
    ]
    .iter()
    .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
    .collect();
    let report = home::import_env(&home, &env).unwrap();
    let printed = layerfs_api::json::print(&report);
    let read = |path: PathBuf| read_record(&path).unwrap().unwrap();

    let native = read(paths::machine_record(&home, "native"));
    assert_eq!(
        native.get_text("daemon"),
        Some("/opt/layerfs/layerfs-daemon")
    );
    assert_eq!(native.get_text("platform"), Some("native"));
    let docker = read(paths::machine_record(&home, "docker"));
    assert_eq!(docker.get_text("docker"), Some("/tmp/docker.sock"));
    assert_eq!(docker.get_text("image"), Some("sha256:abc"));
    // solo had no machine after the migration; shop kept its docker volume
    let solo = read(paths::store_record(&home, "solo"));
    assert_eq!(solo.get_text("machine"), Some("docker"));
    assert_eq!(solo.get_text("volume"), Some("lfs-solo"));
    let shop = read(paths::store_record(&home, "shop"));
    assert_eq!(shop.get_text("volume"), Some("lfs-shop"));
    let bucket = read(paths::bucket_record(&home, "remote"));
    assert_eq!(bucket.get_text("key"), Some("env:LAYERFS_REMOTE"));
    // No secret reaches the home; the removed SSH platform is reported, never connected.
    for secret in ["secret-access", "secret-key", "pw@db"] {
        assert!(!printed.contains(secret), "{printed}");
        assert!(snapshot(&home)
            .iter()
            .all(|(_, bytes)| !String::from_utf8_lossy(bytes).contains(secret)));
    }
    assert!(
        printed.contains(layerfs_sandbox_backend::platform_policy::SSH_REMOVED),
        "{printed}"
    );
    assert!(!paths::machine_record(&home, "ssh").exists());
    assert!(printed.contains("LAYERFS_STORE_POSTGRESQL"), "{printed}");

    // with no setting at all, the home answers from its records
    let listed = layerfs_api::call(Some(&home), "store.list", SdkValue::Null, &mut |_| {}).unwrap();
    let listed = layerfs_api::json::print(&listed);
    assert!(
        listed.contains("\"solo\"") && listed.contains("\"docker\""),
        "{listed}"
    );

    // once: a second run with other values changes nothing
    let other: Vec<(String, String)> = vec![("LAYERFS_DAEMON".to_owned(), "/elsewhere".to_owned())];
    let again = home::import_env(&home, &other).unwrap();
    assert_eq!(
        read(paths::machine_record(&home, "native")).get_text("daemon"),
        Some("/opt/layerfs/layerfs-daemon")
    );
    assert!(layerfs_api::json::print(&again).contains("\"kept\":[{"));
    std::fs::remove_dir_all(home).unwrap();
}
