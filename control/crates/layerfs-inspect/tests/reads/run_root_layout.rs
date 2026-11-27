//! #819 R: a side's run root is named for its kernel and role. A home's
//! is its host machine's folder (`machine/macos-host/run/` or
//! `machine/linux-host/run/`), a worker's root's the guest's
//! (`machine/linux-guest/run/`), whichever spelling of the root the worker
//! was given. The sweep, the inspection reads and a forced close use that
//! root; the earlier `machine/native/run/` is not read.
//!
//! Nothing runs: a Sandbox is its record and its run folder, and its
//! recorded daemon is no process. Scratch folders are under the build's
//! own temporary folder.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_home::disk::{self, paths, write_record};
use layerfs_home::names::HOST_MACHINE;
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::deploy::Side;
use layerfs_sandbox::record::{DaemonRecord, Local};
use layerfs_sandbox::{lifecycle::force_remove, record::sweep};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

static NEXT: AtomicU32 = AtomicU32::new(0);

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn scratch(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "runroot-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A home made by this build, as a call opens it.
fn new_home(scratch: &Path) -> PathBuf {
    let home = scratch.join("home");
    disk::private_dir(&home).unwrap();
    disk::ensure(&home).unwrap();
    home
}

/// A worker's root, as a worker of this process serves it.
fn guest_root(scratch: &Path) -> PathBuf {
    let root = scratch.join("data");
    std::fs::create_dir_all(&root).unwrap();
    disk::guest::serve_root(&root);
    root
}

/// A run folder `name` in run root `run` of side root `root`, as a start
/// leaves it, whose `owner` marker names Sandbox `sandbox`'s record folder
/// there and was written an hour ago.
fn run_folder(root: &Path, run: &Path, name: &str, sandbox: &str) -> PathBuf {
    let folder = run.join(name);
    for part in ["tmp", "workspaces", "config", "overlay"] {
        std::fs::create_dir_all(folder.join(part)).unwrap();
    }
    std::fs::write(folder.join("daemon.log"), "listening\n").unwrap();
    std::fs::write(folder.join("config/daemon.setup"), "private key").unwrap();
    std::fs::write(folder.join("tmp/notes.txt"), "by a command\n").unwrap();
    std::fs::create_dir_all(root.join("sandbox")).unwrap();
    let records = std::fs::canonicalize(root.join("sandbox")).unwrap();
    let owner = folder.join("owner");
    std::fs::write(&owner, records.join(sandbox).as_os_str().as_encoded_bytes()).unwrap();
    (std::fs::File::open(&owner).unwrap())
        .set_modified(SystemTime::now() - Duration::from_secs(3600))
        .unwrap();
    folder
}

/// Sandbox `name` of side root `root`, recorded with its run folder
/// `directory`; its daemon is no process.
fn recorded(root: &Path, name: &str, directory: &Path) {
    let daemon = DaemonRecord::local(Local {
        platform: "macos".to_owned(),
        name: (directory.file_name().unwrap().to_string_lossy()).into_owned(),
        container: "ab".repeat(32),
        runtime: "process",
        uid: layerfs_home::lock::flock::euid(),
        gid: layerfs_home::lock::flock::egid(),
        directory: directory.display().to_string(),
        process: (4_000_000, 1, format!("{:032x}", 7)),
        port: 1,
        image: "local".to_owned(),
        labels: Vec::new(),
        network: None,
    });
    write_record(
        &paths::sandbox_record(root, name),
        &SdkValue::map([
            ("v", SdkValue::Int(disk::RECORD_VERSION)),
            ("name", t(name)),
            ("project", t("shop/api")),
            ("machine", t(HOST_MACHINE)),
            ("id", t(name)),
            ("runtime", t("process")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
}

/// What a sweep removed, by name.
fn removed(swept: &sweep::Swept) -> Vec<String> {
    let mut names: Vec<String> = (swept.removed.iter())
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_home_s_run_root_is_its_host_machine_s() {
    let scratch = scratch("home");
    let home = new_home(&scratch);
    let named = home.join("machine").join(HOST_MACHINE).join("run");
    assert_eq!(paths::run_root(&home), named);
    assert!(!named.exists(), "naming the run root made it");
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_worker_root_s_run_root_is_linux_guest_by_any_spelling() {
    let scratch = scratch("guest");
    let root = guest_root(&scratch);
    assert_eq!(paths::run_root(&root), root.join("machine/linux-guest/run"));
    assert!(!root.join("machine").join(HOST_MACHINE).exists());
    // another spelling of the same folder is the same worker root
    let link = scratch.join("link");
    std::os::unix::fs::symlink(&root, &link).unwrap();
    assert_eq!(paths::run_root(&link), link.join("machine/linux-guest/run"));
    // and a root served by a link is known by its real path
    let other = scratch.join("other");
    std::fs::create_dir_all(&other).unwrap();
    let other_link = scratch.join("other-link");
    std::os::unix::fs::symlink(&other, &other_link).unwrap();
    disk::guest::serve_root(&other_link);
    assert_eq!(
        paths::run_root(&other),
        other.join("machine/linux-guest/run")
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_sweep_covers_the_named_run_root_and_not_machine_native() {
    let scratch = scratch("sweep");
    let home = new_home(&scratch);
    let guest = guest_root(&scratch);
    for root in [&home, &guest] {
        let run = paths::run_root(root);
        let kept = run_folder(root, &run, "layerfs-0123abcd-kept", "kept");
        recorded(root, "kept", &kept);
        let gone = run_folder(root, &run, "layerfs-0123abcd-gone", "gone");
        // what an earlier build left in `machine/native/run` is not read
        let earlier = root.join("machine/native/run");
        let stale = run_folder(root, &earlier, "layerfs-0123abcd-stale", "stale");
        let swept = sweep::run_root(root);
        assert!(swept.complete && swept.failed.is_empty(), "{swept:?}");
        assert_eq!(removed(&swept), ["layerfs-0123abcd-gone"]);
        assert!(!gone.exists() && kept.join("daemon.log").is_file());
        assert!(stale.join("daemon.log").is_file());
        let log = std::fs::read_to_string(paths::log_dir(root).join("sweep.jsonl")).unwrap();
        assert!(log.contains(&gone.display().to_string()), "{log}");
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn inspection_reads_run_folders_in_the_named_run_root_only() {
    let _turn = crate::permits();
    let scratch = scratch("inspect");
    let home = new_home(&scratch);
    let run = paths::run_root(&home);
    let folder = run_folder(&home, &run, "layerfs-0123abcd-s1", "s1");
    recorded(&home, "s1", &folder);
    let side = Side::Home(&home);
    let args = SdkValue::map([("sandbox", t("s1"))]);
    let page = layerfs_inspect::sandbox_page(&side, &args, &|| false).unwrap();
    let shown = format!("{page:?}");
    assert!(
        shown.contains(&folder.join("tmp").display().to_string()),
        "{shown}"
    );
    assert!(shown.contains(&folder.join("daemon.log").display().to_string()));
    let args = SdkValue::map([
        ("sandbox", t("s1")),
        (
            "path",
            t(&folder.join("tmp/notes.txt").display().to_string()),
        ),
    ]);
    let read = layerfs_inspect::sandbox_page(&side, &args, &|| false).unwrap();
    assert!(format!("{read:?}").contains("by a command"), "{read:?}");
    // its daemon's key is LayerFS's own: never read
    let key = SdkValue::map([(
        "path",
        t(&folder.join("config/daemon.setup").display().to_string()),
    )]);
    let refused = layerfs_inspect::machine_page(&side, &key, &|| false).unwrap_err();
    assert!(
        !format!("{refused:?}").contains("private key"),
        "{refused:?}"
    );
    // the machine's folders list the named run root
    let roots =
        layerfs_inspect::machine_page(&side, &SdkValue::Map(Vec::new()), &|| false).unwrap();
    assert!(
        format!("{roots:?}").contains(&run.display().to_string()),
        "{roots:?}"
    );
    // a Sandbox recorded in `machine/native/run` is outside the run root
    let earlier = home.join("machine/native/run");
    let stale = run_folder(&home, &earlier, "layerfs-0123abcd-old", "old");
    recorded(&home, "old", &stale);
    let args = SdkValue::map([("sandbox", t("old"))]);
    let refused = layerfs_inspect::sandbox_page(&side, &args, &|| false).unwrap_err();
    assert_eq!(refused.code, "InvalidInput", "{refused:?}");

    // the machine's space measures the run folder, not as outside
    let request = SdkValue::map([
        ("kind", t("machine_space")),
        ("stores", SdkValue::List(Vec::new())),
        ("sandboxes", SdkValue::List(vec![t("s1")])),
    ]);
    let home_for = home.clone();
    let space = layerfs_inspect::admit(move |budget| {
        layerfs_inspect::serve(&Side::Home(&home_for), &request, budget)
    })
    .unwrap();
    let SdkValue::List(rows) = space.get("sandboxes").cloned().unwrap() else {
        panic!("{space:?}");
    };
    assert_eq!(rows.len(), 1, "{space:?}");
    assert!(rows[0].get_text("reason").is_none(), "{rows:?}");
    assert!(rows[0].get_int("bytes").unwrap() > 0, "{rows:?}");
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_forced_close_uses_the_named_run_root() {
    let scratch = scratch("force");
    let home = new_home(&scratch);
    let run = paths::run_root(&home);
    let folder = run_folder(&home, &run, "layerfs-0123abcd-s1", "s1");
    recorded(&home, "s1", &folder);
    let host = force_remove::local_host(&home, "s1").unwrap();
    let local = host.deploy().and_then(|deploy| deploy.local_dir.clone());
    assert_eq!(local.as_deref().map(Path::new), Some(run.as_path()));
    let _ = std::fs::remove_dir_all(&scratch);
}
