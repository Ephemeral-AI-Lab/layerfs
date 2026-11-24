//! A Sandbox whose create never recorded its id, though it may have started
//! a daemon: its caller ended between the start and the record, or the
//! start's outcome was unknown. `sandbox.remove` closes such a Sandbox as it
//! closes a started one, or refuses and keeps its record with its create's
//! intent; it never drops the record and leaves the daemon running.
//!
//! - On `native` the Sandbox's own side wrote `daemon` into the home's
//!   record before the create recorded the id: a forced remove stops that
//!   daemon through the forced close and deletes its run folder; a remove
//!   that cannot close it (here: no Store to open) refuses, and the daemon
//!   and the record stay.
//! - On a machine with a worker, the worker keeps its own record: the
//!   remove asks the worker to close it, and a worker that is not reached
//!   refuses the remove, naming `layerfs machine start`.
//!
//! Nothing runs a daemon: a `sleep` stands for one.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, paths, read_record, write_record};
use layerfs_home::op::Op;
use layerfs_sandbox::family::{remove, RemoveRequest, Removed};
use std::path::{Path, PathBuf};

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A new home.
fn new_home(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-half-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    home::private_dir(&dir).unwrap();
    home::ensure(&dir).unwrap();
    dir
}

/// The intent of a create whose process ended at `phase`: its owner is no
/// process, and its outcome was not known.
fn ended_create(phase: &str) -> SdkValue {
    let op = Op::begin("sandbox.create", phase, SdkValue::Null)
        .unwrap()
        .as_uncertain();
    home::with_field(
        op.to_record().clone(),
        "owner",
        SdkValue::map([("pid", SdkValue::Int(0))]),
    )
}

/// `sandbox.remove(name, { force })`.
fn removed(
    home: &Path,
    name: &str,
    force: bool,
) -> Result<Removed, layerfs_bridge::control::SdkRefusal> {
    let mut sink = |_| {};
    let mut events = layerfs_home::events::Events::new(&mut sink);
    let request = RemoveRequest {
        name: name.to_owned(),
        force,
    };
    remove(home, &mut events, &request)
}

/// The create's intent Sandbox `name`'s record still holds.
fn kept_op(home: &Path, name: &str) -> Option<String> {
    let record = read_record(&paths::sandbox_record(home, name)).unwrap()?;
    let op = record.get("op")?;
    Some(format!(
        "{}@{}",
        op.get_text("name").unwrap_or(""),
        op.get_text("phase").unwrap_or("")
    ))
}

#[test]
fn an_unstarted_sandbox_on_a_machine_is_closed_by_its_worker_or_kept() {
    let home = new_home("worker");
    write_record(
        &paths::machine_record(&home, "dev"),
        &SdkValue::map([
            ("name", t("dev")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-00000000-dev")),
            ("state", t("running")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("machine", t("dev")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    // its create sent the start to the worker, which keeps its own record
    // and daemon, and the caller ended before the answer
    write_record(
        &paths::sandbox_record(&home, "s1"),
        &SdkValue::map([
            ("name", t("s1")),
            ("project", t("shop/api")),
            ("machine", t("dev")),
            ("op", ended_create("start")),
        ]),
    )
    .unwrap();
    // the worker is not reached (nothing records it): the remove is refused
    // and the record keeps its create's intent, so a later remove can
    // still reach what the worker keeps
    let refusal = match removed(&home, "s1", false) {
        Err(refusal) => refusal,
        Ok(_) => panic!("an unstarted Sandbox was removed without its worker"),
    };
    assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs machine start dev"),
        "{refusal:?}"
    );
    assert_eq!(
        kept_op(&home, "s1").as_deref(),
        Some("sandbox.create@start")
    );

    // a create that ended while it claimed the name started nothing: its
    // records go, as before
    write_record(
        &paths::sandbox_record(&home, "s2"),
        &SdkValue::map([
            ("name", t("s2")),
            ("project", t("shop/api")),
            ("machine", t("dev")),
            ("op", ended_create("claim")),
        ]),
    )
    .unwrap();
    assert!(matches!(
        removed(&home, "s2", false),
        Ok(Removed::AlreadyGone { .. })
    ));
    assert!(!paths::sandbox_dir(&home, "s2").exists());
    std::fs::remove_dir_all(home).unwrap();
}

/// A `native` Sandbox's record as its own side writes it before the create
/// records the id: `daemon` names process `pid`, in its run folder (made
/// here), and the create's intent stays.
#[cfg(target_os = "macos")]
fn half_created(home: &Path, name: &str, pid: u32) -> PathBuf {
    use layerfs_bridge::process::LocalProcess;
    use layerfs_sandbox::record::{DaemonRecord, Local};
    let host = layerfs_sandbox::force::local_host(home, name).unwrap();
    let given = layerfs_sandbox::lifecycle::sandbox_name(host.deploy(), name).unwrap();
    let folder = paths::run_root(home).join(&given);
    std::fs::create_dir_all(folder.join("workspaces")).unwrap();
    std::fs::write(folder.join("daemon.log"), b"").unwrap();
    let process = LocalProcess::observe(pid).unwrap();
    let daemon = DaemonRecord::local(Local {
        platform: "mac".to_owned(),
        name: given,
        container: "ab".repeat(32),
        runtime: "native",
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
        &paths::sandbox_record(home, name),
        &SdkValue::map([
            ("name", t(name)),
            ("project", t("shop/api")),
            ("machine", t("native")),
            ("op", ended_create("start")),
            ("backend", t("native")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
    folder
}

/// A stand-in daemon in its own process group, so a stop of its group
/// signals nothing else.
#[cfg(target_os = "macos")]
fn stand_in() -> std::process::Child {
    use std::os::unix::process::CommandExt;
    std::process::Command::new("/bin/sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .unwrap()
}

#[cfg(target_os = "macos")]
#[test]
fn a_half_created_native_sandbox_has_its_daemon_stopped_or_kept() {
    use layerfs_bridge::process::LocalProcess;
    let home = new_home("native");

    // no Store to open: the ordinary close cannot run, so the remove is
    // refused and nothing is dropped
    let mut kept = stand_in();
    let folder = half_created(&home, "s1", kept.id());
    let daemon = LocalProcess::observe(kept.id()).unwrap();
    let refusal = match removed(&home, "s1", false) {
        Err(refusal) => refusal,
        Ok(_) => panic!("a half-created Sandbox was removed with its daemon running"),
    };
    assert_eq!(refusal.code, "NoSuchStore", "{refusal:?}");
    assert!(daemon.alive(), "its daemon was signalled");
    assert!(folder.is_dir());
    assert_eq!(
        kept_op(&home, "s1").as_deref(),
        Some("sandbox.create@start")
    );

    // forced, the remove stops that daemon and deletes its run folder,
    // through the forced close a started Sandbox takes
    match removed(&home, "s1", true) {
        Ok(Removed::Stopped { unmounted, left }) => {
            assert!(unmounted.is_empty(), "{unmounted:?}");
            assert!(left.is_empty(), "{left:?}");
        }
        Ok(Removed::AlreadyGone { left }) => panic!("taken as gone: {left:?}"),
        Err(refusal) => panic!("{refusal:?}"),
    }
    let _ = kept.wait();
    assert!(!daemon.alive(), "its daemon still runs");
    assert!(!folder.exists(), "its run folder stays");
    assert!(!paths::sandbox_dir(&home, "s1").exists());
    std::fs::remove_dir_all(home).unwrap();
}
