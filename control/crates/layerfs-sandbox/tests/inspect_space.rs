//! `machine.space` on the Local route (a `native` machine, in this
//! process): the walk counts each part of LayerFS's folders once, by
//! blocks, and never enters a run folder's `workspaces/`, a private root
//! `<name>.root`, another device or the home's own other names; it opens no
//! file and writes nothing; and a request with a bad Store name is refused.
//!
//! Every size the test expects is the same blocks sum (`st_blocks` × 512 of
//! each file and folder, a file with two names once) taken by the test
//! itself over the paths it made.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::process::LocalProcess;
use layerfs_home::home::{self, paths, write_record};
use layerfs_home::names::MachineName;
use layerfs_sandbox::inspect;
use layerfs_sandbox::record::{DaemonRecord, Local};
use std::collections::{BTreeMap, HashSet};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

static PROCESS: Mutex<()> = Mutex::new(());
static NEXT: AtomicU32 = AtomicU32::new(0);

/// The test's turn, once no earlier test's helper is still running.
fn serial() -> MutexGuard<'static, ()> {
    let guard = PROCESS.lock().unwrap_or_else(|poison| poison.into_inner());
    let started = Instant::now();
    while inspect::running() != 0 {
        assert!(started.elapsed() < Duration::from_secs(15), "helpers");
        std::thread::sleep(Duration::from_millis(10));
    }
    guard
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A new home under the build's own temporary folder.
fn new_home(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "lfs-space-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    home::private_dir(&dir).unwrap();
    home::ensure(&dir).unwrap();
    dir
}

fn me() -> LocalProcess {
    LocalProcess::observe(std::process::id()).unwrap()
}

/// A daemon record whose run folder is `directory`.
fn daemon(directory: &Path) -> DaemonRecord {
    let process = me();
    DaemonRecord::local(Local {
        platform: "linux".to_owned(),
        name: "layerfs-0a1b2c3d-s".to_owned(),
        container: "ab".repeat(32),
        runtime: "native",
        uid: home::flock::euid(),
        gid: home::flock::egid(),
        directory: directory.display().to_string(),
        process: (
            process.pid,
            process.start,
            format!("{:032x}", process.boot.unwrap()),
        ),
        port: 1,
        image: "local".to_owned(),
        labels: Vec::new(),
        network: None,
    })
}

/// Sandbox `name` on `machine`, with a daemon record naming `run` when given.
fn sandbox(home: &Path, name: &str, machine: &str, run: Option<&Path>) {
    let mut fields = vec![
        ("name", t(name)),
        ("project", t("shop/api")),
        ("machine", t(machine)),
        ("id", t(name)),
        ("runtime", t("native")),
    ];
    fields.extend(run.map(|run| ("daemon", daemon(run).to_record())));
    let record = SdkValue::Map(
        (fields.into_iter())
            .map(|(field, value)| (field.to_owned(), value))
            .collect(),
    );
    write_record(&paths::sandbox_record(home, name), &record).unwrap();
}

/// sqlite Store `name` on `machine` as the home records it.
fn store_record(home: &Path, name: &str, machine: &str) {
    write_record(
        &paths::store_record(home, name),
        &SdkValue::map([
            ("name", t(name)),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("normal")),
            ("machine", t(machine)),
        ]),
    )
    .unwrap();
}

fn write(path: &Path, bytes: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, vec![5u8; bytes]).unwrap();
}

/// The blocks under `path` (itself included), a file with several names
/// once, entering no child named in `skip`.
fn blocks(path: &Path, skip: &[&str], seen: &mut HashSet<(u64, u64)>) -> u64 {
    let Ok(found) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !found.is_dir() && found.nlink() > 1 && !seen.insert((found.dev(), found.ino())) {
        return 0;
    }
    let mut bytes = found.blocks() * 512;
    if found.is_dir() {
        for entry in std::fs::read_dir(path).unwrap().flatten() {
            if !skip.iter().any(|skip| entry.file_name() == *skip) {
                bytes += blocks(&entry.path(), &[], seen);
            }
        }
    }
    bytes
}

fn native() -> MachineName {
    MachineName::parse("native").unwrap()
}

/// Every file under `root` with its length and mode.
fn files(root: &Path) -> BTreeMap<PathBuf, (u64, u32)> {
    let mut out = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let found = std::fs::symlink_metadata(entry.path()).unwrap();
            if found.is_dir() {
                pending.push(entry.path());
            } else {
                out.insert(entry.path(), (found.len(), found.mode()));
            }
        }
    }
    out
}

fn int(value: &SdkValue, field: &str) -> u64 {
    match value.get(field) {
        Some(SdkValue::Int(n)) => u64::try_from(*n).unwrap(),
        other => panic!("{field}: {other:?} in {value:?}"),
    }
}

fn refused(outcome: Result<SdkValue, SdkRefusal>) -> String {
    match outcome {
        Ok(value) => format!("ok {value:?}"),
        Err(refusal) => refusal.code,
    }
}

#[test]
fn machine_space_counts_each_part_once_and_skips_mounts_and_private_roots() {
    let _turn = serial();
    let home = new_home("parts");
    let run = paths::run_root(&home);
    std::fs::create_dir_all(&run).unwrap();

    // two Stores here (one with files nobody may open), one elsewhere
    store_record(&home, "shop", "native");
    store_record(&home, "empty", "native");
    store_record(&home, "far", "box");
    let shop = paths::store_file(&home, "shop");
    write(&shop, 40_960);
    write(&shop.with_file_name("store.sqlite-wal"), 8_192);
    for file in [&shop, &shop.with_file_name("store.sqlite-wal")] {
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    write(&paths::store_file(&home, "far"), 1 << 20);

    // Sandbox `up`: each part of its run folder, a Workspace mount point
    // nobody may list, and its private root with a large file
    let up = run.join("layerfs-0a1b2c3d-up");
    write(&up.join("overlay").join("1.sqlite"), 65_536);
    write(
        &up.join("overlay").join("upper").join("usr").join("big"),
        131_072,
    );
    std::fs::create_dir_all(up.join("overlay").join("work")).unwrap();
    write(&up.join("tmp").join(".cache").join("pip"), 16_384);
    write(&up.join("daemon.log"), 4_096);
    write(&up.join("daemon.log.1"), 4_096);
    write(&up.join("bin").join("layerfs-daemon"), 32_768);
    write(&up.join("config").join("helper.spec"), 512);
    let mount = up.join("workspaces").join("1");
    write(&mount.join("never"), 1 << 20);
    std::fs::set_permissions(&mount, std::fs::Permissions::from_mode(0o000)).unwrap();
    write(&run.join("layerfs-0a1b2c3d-up.root").join("never"), 1 << 20);
    sandbox(&home, "up", "native", Some(&up));
    // Sandbox `gone`: recorded, its run folder removed; `far` elsewhere
    sandbox(
        &home,
        "gone",
        "native",
        Some(&run.join("layerfs-0a1b2c3d-gone")),
    );
    sandbox(&home, "far", "box", Some(&up));
    // a run folder nobody records, with its own mount point
    let old = run.join("layerfs-0a1b2c3d-old");
    write(&old.join("tmp").join("left"), 8_192);
    write(&old.join("workspaces").join("2").join("never"), 1 << 20);
    // the image cache, with a file under two names
    let image = run.join("images").join("abc.2").join("bin");
    write(&image.join("busybox"), 20_480);
    std::fs::hard_link(image.join("busybox"), image.join("sh")).unwrap();
    // LayerFS's logs, and a name an integration keeps in the home
    write(&paths::log_dir(&home).join("exec.jsonl"), 2_048);
    write(&home.join("runtime").join("never"), 1 << 20);

    let mut seen = HashSet::new();
    let mut sum = |path: &Path, skip: &[&str]| blocks(path, skip, &mut seen);
    let want_shop = sum(&paths::store_dir(&home, "shop"), &[]);
    let want_empty = sum(&paths::store_dir(&home, "empty"), &[]);
    let overlays = sum(&up.join("overlay"), &["upper", "work"]);
    let writes =
        sum(&up.join("overlay").join("upper"), &[]) + sum(&up.join("overlay").join("work"), &[]);
    let home_part = sum(&up.join("tmp"), &[]);
    let log = sum(&up.join("daemon.log"), &[]) + sum(&up.join("daemon.log.1"), &[]);
    let runtime = std::fs::symlink_metadata(&up).unwrap().blocks() * 512
        + sum(&up.join("bin"), &[])
        + sum(&up.join("config"), &[]);
    let images = sum(&run.join("images"), &[]);
    let other_runs = sum(&old, &["workspaces"]);
    let logs = sum(&paths::log_dir(&home), &[]) + sum(&paths::trace_dir(&home), &[]);
    let other = sum(&home.join("incoming"), &[])
        + sum(&home.join("backup"), &[])
        + sum(&home.join("sandbox"), &[]);
    let before = files(&home);

    let space = inspect::machine_space(&home, &native()).unwrap();
    let SdkValue::Map(fields) = &space else {
        panic!("{space:?}")
    };
    let keys: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        keys,
        [
            "machine",
            "platform",
            "observed_at",
            "disk",
            "layerfs",
            "outside",
            "parts",
            "stores",
            "sandboxes",
            "unreadable",
            "complete"
        ]
    );
    assert_eq!(space.get_text("platform"), Some("native"));
    assert_eq!(space.get("complete"), Some(&SdkValue::Bool(true)));
    assert_eq!(int(&space, "unreadable"), 0, "{space:?}");

    // the stores of this machine, each its folder
    let SdkValue::List(stores) = space.get("stores").unwrap() else {
        panic!("{space:?}")
    };
    let stores: Vec<(&str, u64)> = (stores.iter())
        .map(|row| (row.get_text("name").unwrap(), int(row, "bytes")))
        .collect();
    assert_eq!(stores, [("empty", want_empty), ("shop", want_shop)]);
    assert!(want_shop >= 49_152, "{want_shop}");

    // one row per recorded Sandbox here, `up` split by part
    let SdkValue::List(rows) = space.get("sandboxes").unwrap() else {
        panic!("{space:?}")
    };
    let row = |name: &str| {
        (rows.iter())
            .find(|row| row.get_text("name") == Some(name))
            .unwrap_or_else(|| panic!("{name} in {rows:?}"))
    };
    assert_eq!(rows.len(), 2, "{rows:?}");
    let up_row = row("up");
    assert_eq!(int(up_row, "overlays"), overlays);
    assert_eq!(int(up_row, "writes"), writes);
    assert_eq!(int(up_row, "home"), home_part);
    assert_eq!(int(up_row, "log"), log);
    assert_eq!(int(up_row, "runtime"), runtime);
    let sandbox_bytes = overlays + writes + home_part + log + runtime;
    assert_eq!(int(up_row, "bytes"), sandbox_bytes);
    assert_eq!(up_row.get("reason"), None);
    let gone = row("gone");
    assert_eq!(gone.get_text("reason"), Some("no_run_folder"));
    assert_eq!(gone.get("bytes"), Some(&SdkValue::Null));

    // parts and the whole
    let parts = space.get("parts").unwrap();
    let total = want_shop + want_empty + sandbox_bytes + images + other_runs + logs + other;
    assert_eq!(int(parts, "stores"), want_shop + want_empty);
    assert_eq!(int(parts, "sandboxes"), sandbox_bytes);
    assert_eq!(int(parts, "images"), images);
    assert_eq!(int(parts, "other_runs"), other_runs);
    assert_eq!(int(parts, "logs"), logs);
    assert_eq!(int(parts, "other"), other);
    assert_eq!(int(&space, "layerfs"), total);
    // nothing under a mount point, a private root or `runtime/` counted
    assert!(total < 1 << 20, "{total}");
    let disk = space.get("disk").unwrap();
    assert_eq!(disk.get_text("scope"), Some("host_fs"));
    assert_eq!(
        int(&space, "outside"),
        int(disk, "used").saturating_sub(total)
    );

    // nothing written or changed, nothing opened (the 0000 files stayed so)
    let after = files(&home);
    std::fs::set_permissions(&mount, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(after, before, "machine.space changed a file");
    std::fs::remove_dir_all(&home).unwrap_or_else(|_| {
        for file in [&shop, &shop.with_file_name("store.sqlite-wal")] {
            let _ = std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::remove_dir_all(&home).unwrap();
    });
}

#[test]
fn a_folder_that_cannot_be_listed_is_counted_as_unreadable() {
    if home::flock::euid() == 0 {
        // root lists every folder: the case cannot be made here
        return;
    }
    let _turn = serial();
    let home = new_home("unreadable");
    let run = paths::run_root(&home);
    let up = run.join("layerfs-0a1b2c3d-up");
    let closed = up.join("tmp").join("closed");
    write(&closed.join("hidden"), 4_096);
    std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o000)).unwrap();
    sandbox(&home, "up", "native", Some(&up));

    let space = inspect::machine_space(&home, &native()).unwrap();
    std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(int(&space, "unreadable"), 1, "{space:?}");
    assert_eq!(space.get_text("reason"), Some("unreadable"));
    // the walk itself finished: `outside` is still there
    assert_eq!(space.get("complete"), Some(&SdkValue::Bool(true)));
    assert!(matches!(space.get("outside"), Some(SdkValue::Int(_))));
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn a_request_with_a_bad_store_or_sandbox_name_is_refused() {
    let _turn = serial();
    let home = new_home("refused");
    for request in [
        SdkValue::map([("stores", SdkValue::List(vec![t("../x")]))]),
        SdkValue::map([("sandboxes", SdkValue::List(vec![t("a/b")]))]),
        SdkValue::map([("stores", t("shop"))]),
    ] {
        let asked = inspect::ask(&home, "native", "machine_space", request);
        assert_eq!(refused(asked), "InvalidInput");
    }
    std::fs::remove_dir_all(&home).unwrap();
}
