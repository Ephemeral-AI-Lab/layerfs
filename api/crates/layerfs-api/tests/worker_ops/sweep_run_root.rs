//! LayerFS removes what interrupted Sandbox starts and removals left in a
//! run root itself (spec v20, `layerfs_sandbox::sweep`): an orphan run
//! folder and an empty private root go; a run folder a record names, one
//! whose record's daemon lives, one with no `owner` marker or a fresh one,
//! one of a create in flight, a private root beside its run folder or one
//! that holds something, the image cache and anything a link points at
//! outside stay. A machine's worker sweeps its root when it starts and
//! before each `sandbox create` it runs; each removal is a line of
//! `log/sweep.jsonl` under the root.
//!
//! Nothing runs a daemon or a machine: the entries are made by hand, the
//! worker is this build's worker server on a thread, and a `sleep` this
//! test starts stands for a live daemon. A mount point is not made (that
//! needs root).
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_home::home::{self, paths, write_record};
use layerfs_machine::worker::{self, Endpoint};
use layerfs_os::process::LocalProcess;
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::record::{DaemonRecord, Local};
use layerfs_sandbox::sweep::{run_root, Swept};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[path = "../support/worker_store.rs"]
mod worker_store;

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn dir(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path).unwrap();
    path.to_owned()
}

fn file(path: &Path, bytes: usize) {
    dir(path.parent().unwrap());
    std::fs::write(path, vec![b'x'; bytes]).unwrap();
}

/// Sets `path`'s modification time `ago` before now.
fn aged(path: &Path, ago: Duration) {
    let at = SystemTime::now() - ago;
    (std::fs::File::open(path).unwrap())
        .set_modified(at)
        .unwrap();
}

/// A run folder under side root `root` whose `owner` marker (older than a
/// start's window) names Sandbox `sandbox`'s record folder there, with a
/// link out of it to `outside`.
fn run_folder(root: &Path, name: &str, sandbox: &str, outside: &Path) -> PathBuf {
    run_folder_at(root, &paths::run_root(root), name, sandbox, outside)
}

/// [`run_folder`] in run root `run` of side root `root`.
fn run_folder_at(root: &Path, run: &Path, name: &str, sandbox: &str, outside: &Path) -> PathBuf {
    let folder = dir(&run.join(name));
    file(&folder.join("daemon.log"), 10);
    file(&folder.join("overlay/overlay.sqlite"), 20);
    file(&folder.join("workspaces/w1/file"), 5);
    std::os::unix::fs::symlink(outside, folder.join("link-out")).unwrap();
    let records = std::fs::canonicalize(dir(&root.join("sandbox"))).unwrap();
    std::fs::write(
        folder.join("owner"),
        records.join(sandbox).as_os_str().as_encoded_bytes(),
    )
    .unwrap();
    aged(&folder.join("owner"), Duration::from_secs(600));
    folder
}

/// Sandbox `id`'s record under `root`: started, its daemon the process
/// `pid`, in run folder `folder`.
fn started(root: &Path, id: &str, folder: &Path, pid: u32) {
    let process = LocalProcess::observe(pid).unwrap();
    let daemon = DaemonRecord::local(Local {
        platform: "mac".to_owned(),
        name: folder.file_name().unwrap().to_str().unwrap().to_owned(),
        container: "ab".repeat(32),
        runtime: "process",
        uid: home::flock::euid(),
        gid: home::flock::egid(),
        directory: folder.display().to_string(),
        process: (
            process.pid,
            process.start,
            format!("{:032x}", process.boot.unwrap()),
        ),
        port: 9,
        image: "local".to_owned(),
        labels: Vec::new(),
        network: None,
    });
    write_record(
        &paths::sandbox_record(root, id),
        &SdkValue::map([
            ("v", SdkValue::Int(home::RECORD_VERSION)),
            ("name", t(id)),
            ("project", t("shop/api")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("id", t(id)),
            ("runtime", t("process")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
}

/// What a sweep removed, by name.
fn removed(swept: &Swept) -> Vec<String> {
    let mut names: Vec<String> = (swept.removed.iter())
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

/// The `removed` lines of `log/sweep.jsonl` under `root`, by path.
fn logged(root: &Path) -> Vec<String> {
    let text =
        std::fs::read_to_string(paths::log_dir(root).join("sweep.jsonl")).unwrap_or_default();
    (text.lines())
        .map(|line| layerfs_home::json::parse(line).unwrap())
        .filter(|line| line.get_text("outcome") == Some("removed"))
        .map(|line| line.get_text("path").unwrap().to_owned())
        .collect()
}

#[test]
fn the_run_root_sweep_removes_orphans_and_keeps_what_is_owned() {
    let scratch = worker_store::scratch("sweep");
    let root = dir(&scratch.join("root"));
    let outside = dir(&scratch.join("outside"));
    file(&outside.join("precious"), 9);
    let run = paths::run_root(&root);

    // orphans: a run folder no record names, whose owner's record is gone,
    // and an empty private root whose run folder is gone
    let gone = run_folder(&root, "layerfs-0123abcd-gone", "gone", &outside);
    let dead_root = dir(&run.join("layerfs-0123abcd-dead.root"));
    // a closed directory mode does not stop the removal
    std::fs::set_permissions(
        gone.join("overlay"),
        std::os::unix::fs::PermissionsExt::from_mode(0o000),
    )
    .unwrap();

    // owned: a Sandbox whose record names its run folder, with a live daemon
    let mut daemon = std::process::Command::new("sleep")
        .arg("90")
        .spawn()
        .unwrap();
    let live = run_folder(&root, "layerfs-0123abcd-live", "live", &outside);
    started(&root, "live", &live, daemon.id());
    // its earlier run folder: no record names it, but its `owner` marker
    // names a record whose daemon lives
    let earlier = run_folder(&root, "lfs-0123abcd-live", "live", &outside);
    // no `owner` marker: not known to be a finished start's folder
    let bare = dir(&run.join("layerfs-0123abcd-bare"));
    file(&bare.join("daemon.log"), 3);
    // a marker just written: its Sandbox may be starting
    let fresh = run_folder(&root, "layerfs-0123abcd-fresh", "fresh", &outside);
    aged(&fresh.join("owner"), Duration::ZERO);
    // a create in flight: its record is there, naming no run folder yet
    let soon = run_folder(&root, "layerfs-0123abcd-soon", "soon", &outside);
    write_record(
        &paths::sandbox_record(&root, "soon"),
        &SdkValue::map([
            ("v", SdkValue::Int(home::RECORD_VERSION)),
            ("name", t("soon")),
            ("project", t("shop/api")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ]),
    )
    .unwrap();
    // a private root beside its run folder, and one that holds something
    let beside = dir(&run.join("layerfs-0123abcd-bare.root"));
    file(&run.join("layerfs-0123abcd-full.root/etc/x"), 1);
    // the image cache is not the run root's to sweep
    file(&run.join("images/.tmp-x/y"), 1);

    let swept = run_root(&root);
    assert!(swept.complete, "{swept:?}");
    assert!(swept.failed.is_empty(), "{swept:?}");
    assert_eq!(
        removed(&swept),
        ["layerfs-0123abcd-dead.root", "layerfs-0123abcd-gone"]
    );
    assert!(!gone.exists() && !dead_root.exists());
    assert!(
        outside.join("precious").is_file(),
        "a link is never followed"
    );
    for kept in [&live, &earlier, &bare, &fresh, &soon] {
        assert!(kept.join("daemon.log").is_file(), "{}", kept.display());
    }
    assert!(beside.is_dir());
    assert!(run.join("layerfs-0123abcd-full.root/etc/x").is_file());
    assert!(run.join("images/.tmp-x/y").is_file());
    let mut lines = logged(&root);
    lines.sort();
    assert_eq!(
        lines,
        [dead_root.display().to_string(), gone.display().to_string()]
    );

    // once the daemon ended, the earlier folder is free; the recorded one
    // never is
    daemon.kill().unwrap();
    daemon.wait().unwrap();
    let swept = run_root(&root);
    assert_eq!(removed(&swept), ["lfs-0123abcd-live"]);
    assert!(live.join("daemon.log").is_file());
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_worker_sweeps_when_it_starts_and_before_a_sandbox_create() {
    let scratch = worker_store::scratch("sweepw");
    let root = dir(&scratch.join("data"));
    // the guest's run root (#819 R). This test does not mark the root as
    // a worker's: the worker must do so before its start's sweep, or the
    // sweep looks under the host machine's folder and `before` stays
    let run = root.join("machine/linux-guest/run");
    let outside = dir(&scratch.join("outside"));
    let before = run_folder_at(&root, &run, "layerfs-0123abcd-before", "before", &outside);
    let fresh = run_folder_at(&root, &run, "layerfs-0123abcd-fresh", "fresh", &outside);
    aged(&fresh.join("owner"), Duration::ZERO);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint {
        address: listener.local_addr().unwrap(),
        secret: worker_store::SECRET.to_owned(),
    };
    let served = root.clone();
    std::thread::spawn(move || {
        layerfs_api::worker::serve::serve(listener, worker_store::SECRET.to_owned(), served)
    });
    // the first answer comes after the start's sweep
    worker::call(&endpoint, "worker.ping", SdkValue::Null, &mut |_| {}).unwrap();
    assert!(!before.exists(), "the worker's start swept it");
    assert_eq!(paths::run_root(&root), run);
    assert!(!root
        .join("machine")
        .join(layerfs_home::names::HOST_MACHINE)
        .exists());
    assert!(fresh.join("daemon.log").is_file(), "a fresh marker is kept");

    // one left after the start goes before the next create, which is
    // refused afterwards (no Store under this root); a Workspace status
    // does not sweep
    let after = run_folder(&root, "layerfs-0123abcd-after", "after", &outside);
    let mut args = worker_store::sandbox_args();
    let _ = worker::call(&endpoint, "sandbox.host", args.clone(), &mut |_| {});
    assert!(after.is_dir(), "only a create sweeps");
    args = home::with_field(args, "op", t("sandbox.create"));
    let created = worker::call(&endpoint, "sandbox.host", args, &mut |_| {});
    assert!(created.is_err(), "{created:?}");
    assert!(!after.exists(), "the create swept it");
    assert!(fresh.join("daemon.log").is_file());
    let lines = logged(&root);
    assert_eq!(
        lines,
        [before.display().to_string(), after.display().to_string()]
    );
    let _ = std::fs::remove_dir_all(&scratch);
}
