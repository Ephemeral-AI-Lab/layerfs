//! Backup refusals that need no bucket (#533 review P0-8): a restore over
//! a Store with a running Sandbox is `InUse` before any request is sent,
//! and the Sandbox's project records stay. The bucket record names a port
//! nothing listens on, so a request would fail as `Failed` or `Unknown`.
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-backup-refusal-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
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
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

/// A connected bucket whose endpoint answers nothing; its key's secret is
/// in the home's secret folder.
fn dead_bucket(home: &Path) {
    layerfs_store::backup::secret::write(home, "bucket/dead", "secret").unwrap();
    write_record(
        &paths::bucket_record(home, "dead"),
        &SdkValue::map([
            ("name", t("dead")),
            ("provider", t("minio")),
            ("endpoint", t("http://127.0.0.1:9")),
            ("bucket", t("nothing")),
            ("prefix", t("")),
            ("region", t("us-east-1")),
            ("access_key", t("user")),
        ]),
    )
    .unwrap();
}

#[test]
fn restore_over_a_store_with_a_running_sandbox_is_in_use() {
    let home = tempdir("home");
    home::open(&home).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "kept\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    dead_bucket(&home);
    // the Store links the bucket's file at its current heads, so only the
    // Sandbox stands in the way
    let heads = layerfs_bucket::backup::heads(&paths::store_file(&home, "shop")).unwrap();
    let path = paths::store_record(&home, "shop");
    let record = read_record(&path).unwrap().unwrap();
    let linked = SdkValue::map([(
        "dead",
        SdkValue::map([
            ("file", t("shop")),
            ("heads", SdkValue::Bytes(heads.to_vec())),
        ]),
    )]);
    write_record(&path, &home::with_field(record, "backups", linked)).unwrap();
    write_record(
        &paths::sandbox_record(&home, "tests"),
        &SdkValue::map([
            ("name", t("tests")),
            ("project", t("shop/api")),
            ("machine", t("docker")),
        ]),
    )
    .unwrap();

    let refused = call(
        &home,
        "backup.restore",
        &[
            ("bucket", t("dead")),
            ("file", t("shop")),
            ("store", t("shop")),
        ],
    )
    .unwrap_err();
    assert_eq!(refused.0, "InUse", "{refused:?}");
    assert!(refused.1.contains("tests"), "{refused:?}");
    assert!(paths::project_record(&home, "shop", "api").is_file());

    // without the Sandbox the same restore goes on to the bucket, which
    // does not answer: the refusal above came before any request
    std::fs::remove_dir_all(paths::sandbox_dir(&home, "tests")).unwrap();
    let failed = call(
        &home,
        "backup.restore",
        &[
            ("bucket", t("dead")),
            ("file", t("shop")),
            ("store", t("shop")),
        ],
    )
    .unwrap_err();
    assert_ne!(failed.0, "InUse", "{failed:?}");
    assert!(paths::project_record(&home, "shop", "api").is_file());
    // the replace was the Store's `op` while it ran; its certain failure
    // dropped it and left what was tried on the link
    let record = read_record(&path).unwrap().unwrap();
    assert!(record.get("op").is_none(), "{record:?}");
    let attempt = record.get("backups").unwrap().get("dead").unwrap();
    let attempt = attempt.get("attempt").expect("an attempt");
    assert_eq!(attempt.get_text("call"), Some("restore"));
    // the failed download left no staging folder behind
    let hidden: Vec<_> = std::fs::read_dir(home.join("store"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
        .collect();
    assert!(hidden.is_empty(), "{hidden:?}");

    for dir in [home, folder] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// `store/<name>` already there but not the daemon's private directory: the
/// restore refuses it with `store.create`'s own code and remedy before it
/// stages anything, writes nothing and leaves the user's directory as it was
/// (#570 P1). The same folder held `0700`, and an absent one, pass the check
/// and go on into the download.
#[test]
fn restore_refuses_a_store_folder_that_is_not_private() {
    let home = tempdir("folder-mode");
    home::open(&home).unwrap();
    dead_bucket(&home);
    let directory = paths::store_dir(&home, "open-dir");
    let store = home.join("store");
    let restore = || {
        call(
            &home,
            "backup.restore",
            &[
                ("bucket", t("dead")),
                ("file", t("open-dir")),
                ("store", t("open-dir")),
            ],
        )
        .unwrap_err()
    };
    let mode = |dir: &Path| std::fs::metadata(dir).unwrap().permissions().mode() & 0o7777;

    std::fs::create_dir_all(&directory).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (code, message) = restore();
    assert_eq!(code, "Unsupported", "{message}");
    assert!(
        message.contains(&directory.display().to_string()),
        "{message}"
    );
    assert!(
        message.contains("0755") && message.contains("0700"),
        "{message}"
    );
    assert!(
        message.contains(&format!("chmod 700 {}", directory.display())),
        "{message}"
    );
    // nothing was written: no staging folder, no Store file or record, and
    // the user's own directory keeps its mode
    let names: Vec<_> = std::fs::read_dir(&store)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["open-dir"], "a refused restore stages nothing");
    assert_eq!(mode(&directory), 0o755, "the folder's mode is not changed");
    assert!(!paths::store_file(&home, "open-dir").exists());
    assert!(!paths::store_record(&home, "open-dir").exists());

    // the daemon's own private directory passes: the restore goes on into
    // the download (`remote::load`), which fails there as it would without
    // this check
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    let failed = restore();
    failed_in_the_download(&failed.0, &failed.1);
    assert!(!paths::store_file(&home, "open-dir").exists());
    assert!(!paths::store_record(&home, "open-dir").exists());

    // and an absent folder passes the check the same way; the install that
    // would make it `0700` never runs without install permission, and is
    // covered by `layerfs-store`'s `restore_install`
    std::fs::remove_dir_all(&directory).unwrap();
    let failed = restore();
    failed_in_the_download(&failed.0, &failed.1);
    assert!(!directory.exists(), "no install ran for this call");
    // both calls reached the download: each made its staging folder under
    // the home lock and removed it when it failed
    let staged: Vec<_> = std::fs::read_dir(&store)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".restore-"))
        .collect();
    assert!(
        staged.is_empty(),
        "a failed download left its staging folder: {staged:?}"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// A restore that passed the directory check failed inside the download:
/// `remote::load`'s own first checks run there, and which one stops it says
/// how far it went — the installer user in a Linux process that cannot
/// install (before any request is sent), else the dead endpoint's own
/// request. Neither is this directory guard.
fn failed_in_the_download(code: &str, message: &str) {
    assert_eq!(code, "Failed", "{message}");
    if installs_as_the_daemons_user() {
        assert!(
            !message.contains("chmod 700"),
            "the download failed, not the directory guard: {message}"
        );
    } else {
        assert!(
            message.contains("must run as user 0"),
            "the download's own installer-user check: {message}"
        );
    }
}

/// Whether this process is the user a Store install requires: on Linux only
/// uid 0 is (`layerfs-store-service` `install`), so every other run stops at
/// that check instead of reaching the bucket.
fn installs_as_the_daemons_user() -> bool {
    #[cfg(target_os = "linux")]
    {
        nix::unistd::geteuid().is_root()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// A restore killed mid-download leaves its folder; the next restore removes
/// it, and keeps a folder whose restore still holds its lock and one with
/// no lock file at all.
#[test]
#[allow(deprecated)] // nix's `Flock` consumes the file; this test keeps it
fn the_next_restore_removes_a_killed_restores_folder() {
    use nix::fcntl::{flock, FlockArg};
    use std::os::unix::io::AsRawFd;
    let home = tempdir("sweep");
    home::open(&home).unwrap();
    dead_bucket(&home);
    let store = home.join("store");
    let folder = |name: &str, lock: bool| {
        let dir = store.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("store.sqlite.pulling"), vec![0u8; 4096]).unwrap();
        if lock {
            std::fs::write(dir.join("lock"), b"").unwrap();
        }
        dir
    };
    let killed = folder(".restore-killed", true);
    let running = folder(".restore-running", true);
    let unknown = folder(".restore-unknown", false);
    let held = std::fs::File::options()
        .write(true)
        .open(running.join("lock"))
        .unwrap();
    flock(held.as_raw_fd(), FlockArg::LockExclusiveNonblock).unwrap();

    let failed = call(
        &home,
        "backup.restore",
        &[
            ("bucket", t("dead")),
            ("file", t("fresh")),
            ("store", t("fresh")),
        ],
    )
    .unwrap_err();
    assert_ne!(failed.0, "Exists", "{failed:?}");
    assert!(!killed.exists(), "the killed restore's folder stayed");
    assert!(running.join("store.sqlite.pulling").is_file());
    assert!(unknown.join("store.sqlite.pulling").is_file());
    // the failed restore's own folder is gone too
    let mut left: Vec<_> = std::fs::read_dir(&store)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".restore-"))
        .collect();
    left.sort();
    assert_eq!(left, vec![".restore-running", ".restore-unknown"]);
    drop(held);
    let _ = std::fs::remove_dir_all(home);
}

/// Two creates of one Store would stage their snapshots at the same path:
/// while one holds the Store's backup lock, another is `Busy` before it
/// stages anything or sends a request.
#[test]
#[allow(deprecated)] // nix's `Flock` consumes the file; this test keeps it
fn a_second_create_of_a_store_is_busy() {
    use nix::fcntl::{flock, FlockArg};
    use std::os::unix::io::AsRawFd;
    let home = tempdir("busy");
    home::open(&home).unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    dead_bucket(&home);
    let staged = home.join("store/shop/store.sqlite.backup");
    std::fs::write(&staged, b"the running create's snapshot").unwrap();
    let held = std::fs::File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(home.join("store/shop/backup.lock"))
        .unwrap();
    flock(held.as_raw_fd(), FlockArg::LockExclusiveNonblock).unwrap();

    let busy = call(
        &home,
        "backup.create",
        &[("store", t("shop")), ("bucket", t("dead"))],
    )
    .unwrap_err();
    assert_eq!(busy.0, "Busy", "{busy:?}");
    assert!(busy.1.contains("backup.create"), "{busy:?}");
    assert_eq!(
        std::fs::read(&staged).unwrap(),
        b"the running create's snapshot"
    );

    // once it ends, the next create goes on to the bucket
    drop(held);
    let next = call(
        &home,
        "backup.create",
        &[("store", t("shop")), ("bucket", t("dead"))],
    )
    .unwrap_err();
    assert_ne!(next.0, "Busy", "{next:?}");
    // the backup was the Store's `op`; its failure dropped it and wrote
    // what was tried to the link's `attempt`, which store.get reads
    let record = read_record(&paths::store_record(&home, "shop"))
        .unwrap()
        .unwrap();
    assert!(record.get("op").is_none(), "{record:?}");
    let got = layerfs_api::call(
        Some(&home),
        "store.get",
        SdkValue::map([("store", t("shop")), ("detail", SdkValue::Bool(true))]),
        &mut |_| {},
    )
    .unwrap();
    let Some(SdkValue::List(rows)) = got.get("backups") else {
        panic!("{got:?}")
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get_text("bucket"), Some("dead"));
    assert_eq!(rows[0].get("last"), Some(&SdkValue::Null));
    let attempt = rows[0].get("attempt").unwrap();
    assert_eq!(attempt.get_text("call"), Some("create"));
    assert!(attempt
        .get_text("ended_at")
        .is_some_and(|at| at.ends_with('Z')));
    let _ = std::fs::remove_dir_all(home);
}

/// A bucket whose record cannot be read (conformance R1 N10) is named in
/// `not_listed` with `reason: refused` (it was not asked; `lost` is only for
/// a request that got no answer, RF re-check N-E); it does not fail the
/// listing of the others.
#[test]
fn an_unreadable_bucket_record_is_not_listed() {
    let home = tempdir("unreadable-record");
    home::open(&home).unwrap();
    let record = paths::bucket_record(&home, "broken");
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, b"{ not json").unwrap();
    let listed = ok(&home, "backup.list", &[]);
    let skipped = match listed.get("not_listed") {
        Some(SdkValue::List(rows)) => rows.clone(),
        other => panic!("not_listed: {other:?}"),
    };
    assert_eq!(skipped.len(), 1, "{listed:?}");
    assert_eq!(skipped[0].get_text("bucket"), Some("broken"));
    assert_eq!(skipped[0].get_text("reason"), Some("refused"));
    let message = skipped[0].get_text("message").unwrap_or_default();
    assert!(
        message.starts_with("the record of bucket broken could not be read: "),
        "{message}"
    );
    std::fs::remove_dir_all(home).unwrap();
}
