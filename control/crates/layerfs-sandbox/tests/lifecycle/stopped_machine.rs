//! A Sandbox on a machine recorded `stopped`: its daemon ended with the
//! machine and no worker answers, so `sandbox.remove` goes by the home's
//! records alone when a forced `machine.stop` ended its runtime
//! (`machine_stopped`) or the remove is forced, answering `already_gone`
//! with `left` saying what stays on the machine. Without either it is
//! refused `Unavailable`, naming `machine start` and `--force`. When the
//! machine's worker next starts, it removes its own record of each Sandbox
//! whose daemon ended, and then that Sandbox's run folder.
//!
//! Nothing runs a machine or a daemon: records are written by hand, and a
//! `sleep` this test starts and ends stands for a daemon that ended.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_home::home::{self, paths, write_record};
use layerfs_os::process::LocalProcess;
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::family::{remove, RemoveRequest, Removed};
use layerfs_sandbox::record::{DaemonRecord, Local};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn new_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-stopped-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    home::private_dir(&dir).unwrap();
    home::ensure(&dir).unwrap();
    dir
}

fn removed(
    home: &Path,
    name: &str,
    force: bool,
) -> Result<Removed, layerfs_protocol::sdk::SdkRefusal> {
    let mut sink = |_| {};
    let mut events = layerfs_home::events::Events::new(&mut sink);
    let request = RemoveRequest {
        name: name.to_owned(),
        force,
    };
    remove(home, &mut events, &request, local)
}

/// A started Sandbox `name` on machine `dev`, as the home records it.
fn started_on_dev(home: &Path, name: &str, machine_stopped: bool) {
    let mut fields = vec![
        ("name", t(name)),
        ("project", t("shop/api")),
        ("machine", t("dev")),
        ("id", t(name)),
        ("platform", t("lima")),
        ("runtime", t("namespaces")),
        ("backend", t("namespaces")),
    ];
    if machine_stopped {
        fields.push(("machine_stopped", SdkValue::Bool(true)));
        fields.push(("machine_stop_unknown", SdkValue::Bool(false)));
    }
    let record = SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect());
    write_record(&paths::sandbox_record(home, name), &record).unwrap();
}

#[test]
fn a_sandbox_on_a_stopped_machine_goes_by_its_records() {
    let home = new_root("remove");
    write_record(
        &paths::machine_record(&home, "dev"),
        &SdkValue::map([
            ("name", t("dev")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-00000000-dev")),
            ("state", t("stopped")),
        ]),
    )
    .unwrap();

    // a forced machine stop ended its runtime: no force needed
    started_on_dev(&home, "ended", true);
    let left = match removed(&home, "ended", false) {
        Ok(Removed::AlreadyGone { left }) => left.join(" | "),
        Ok(Removed::Stopped { .. }) => panic!("a stopped machine's Sandbox was stopped"),
        Err(refusal) => panic!("{refusal:?}"),
    };
    assert!(left.contains("machine dev is stopped"), "{left}");
    assert!(left.contains("run folder"), "{left}");
    assert!(left.contains("next starts"), "{left}");
    assert!(!paths::sandbox_dir(&home, "ended").exists());

    // only the machine is recorded stopped: refused without force, naming
    // both ways on, and the record is as it was
    started_on_dev(&home, "plain", false);
    let refusal = match removed(&home, "plain", false) {
        Err(refusal) => refusal,
        Ok(_) => panic!("removed without force"),
    };
    assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs machine start dev")
    );
    assert!(
        refusal
            .message
            .contains("layerfs sandbox remove plain --force"),
        "{}",
        refusal.message
    );
    let kept = home::read_record(&paths::sandbox_record(&home, "plain"))
        .unwrap()
        .unwrap();
    assert!(kept.get("op").is_none(), "{kept:?}");

    // forced, it goes by its records; so does an unstarted one
    assert!(matches!(
        removed(&home, "plain", true),
        Ok(Removed::AlreadyGone { .. })
    ));
    assert!(!paths::sandbox_dir(&home, "plain").exists());
    write_record(
        &paths::sandbox_record(&home, "half"),
        &SdkValue::map([
            ("name", t("half")),
            ("project", t("shop/api")),
            ("machine", t("dev")),
        ]),
    )
    .unwrap();
    assert!(matches!(
        removed(&home, "half", true),
        Ok(Removed::AlreadyGone { .. })
    ));
    assert!(!paths::sandbox_dir(&home, "half").exists());
    std::fs::remove_dir_all(home).unwrap();
}

/// A worker's record of Sandbox `name` under `root`, its daemon process
/// `process`, in a run folder (made here) whose `owner` marker names it
/// and is older than a start's window.
fn worker_record(root: &Path, name: &str, process: LocalProcess) -> PathBuf {
    let folder = paths::run_root(root).join(format!("layerfs-0123abcd-{name}"));
    std::fs::create_dir_all(folder.join("overlay")).unwrap();
    std::fs::write(folder.join("daemon.log"), b"x").unwrap();
    let daemon = DaemonRecord::local(Local {
        platform: "linux".to_owned(),
        name: format!("layerfs-0123abcd-{name}"),
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
        &paths::sandbox_record(root, name),
        &SdkValue::map([
            ("name", t(name)),
            ("project", t("shop/api")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
    let records = std::fs::canonicalize(root.join("sandbox")).unwrap();
    let owner = folder.join("owner");
    std::fs::write(&owner, records.join(name).as_os_str().as_encoded_bytes()).unwrap();
    (std::fs::File::open(&owner).unwrap())
        .set_modified(SystemTime::now() - Duration::from_secs(600))
        .unwrap();
    folder
}

#[test]
fn a_starting_worker_removes_the_records_of_daemons_that_ended() {
    let root = new_root("worker");
    // a daemon that ended with its machine
    let mut ended = std::process::Command::new("/bin/sleep")
        .arg("60")
        .spawn()
        .unwrap();
    let process = LocalProcess::observe(ended.id()).unwrap();
    ended.kill().unwrap();
    ended.wait().unwrap();
    assert!(!process.alive());
    let gone = worker_record(&root, "gone", process);
    // a daemon that still runs (this test's own process)
    let me = LocalProcess::observe(std::process::id()).unwrap();
    let live = worker_record(&root, "live", me);

    let swept = layerfs_sandbox::record::sweep::worker_start(&root);
    assert!(swept.failed.is_empty(), "{swept:?}");
    assert!(!paths::sandbox_dir(&root, "gone").exists());
    assert!(!gone.exists(), "its run folder stays");
    assert!(paths::sandbox_record(&root, "live").exists());
    assert!(live.join("daemon.log").is_file());
    std::fs::remove_dir_all(root).unwrap();
}

/// The Workspace drain these tests pass: they mount no Workspace.
struct NoDrain;

impl layerfs_sandbox::lifecycle::serve::WorkspaceDrain for NoDrain {
    fn drain(&self, _host: &layerfs_sandbox::route::machine::Host, _path: &str) {}
}

/// The in-process runner `remove` takes (the workspace crate's dispatch in
/// the product): the Sandbox's own dispatch, with no Workspace to drain.
fn local(
    host: layerfs_sandbox::route::machine::Host,
    call: layerfs_sandbox::route::machine::Call,
) -> layerfs_home::outcome::Outcome {
    layerfs_sandbox::route::machine::dispatch::call(host, call, &NoDrain)
}
