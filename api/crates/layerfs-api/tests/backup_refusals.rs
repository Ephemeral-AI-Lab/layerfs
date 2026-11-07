//! Backup refusals that need no bucket (#533 review P0-8): a restore over
//! a Store with a running Sandbox is `InUse` before any request is sent,
//! and the Sandbox's project records stay. The bucket record names a port
//! nothing listens on, so a request would fail as `Failed` or `Unknown`.
#![cfg(unix)] // unix stat/lock semantics; the Windows face is its own port

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

/// A connected bucket whose endpoint answers nothing.
fn dead_bucket(home: &Path) {
    std::env::set_var("LAYERFS_TEST_DEAD_ACCESS_KEY", "user");
    std::env::set_var("LAYERFS_TEST_DEAD_SECRET_KEY", "secret");
    write_record(
        &paths::bucket_record(home, "dead"),
        &SdkValue::map([
            ("name", t("dead")),
            ("provider", t("minio")),
            ("endpoint", t("http://127.0.0.1:9")),
            ("bucket", t("nothing")),
            ("prefix", t("")),
            ("region", t("us-east-1")),
            ("key", t("env:LAYERFS_TEST_DEAD")),
            ("part_size", SdkValue::Int(64 << 20)),
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
    let heads = layerfs_store_backup::backup::heads(&paths::store_file(&home, "shop")).unwrap();
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
        &[("store", t("shop")), ("bucket", t("dead"))],
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
        &[("store", t("shop")), ("bucket", t("dead"))],
    )
    .unwrap_err();
    assert_ne!(failed.0, "InUse", "{failed:?}");
    assert!(paths::project_record(&home, "shop", "api").is_file());
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
        &[("store", t("fresh")), ("bucket", t("dead"))],
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
    let _ = std::fs::remove_dir_all(home);
}
