//! Removing a machine, and changing one with Sandboxes on it (#533 review
//! P1-9, P1-23, P2-32), through records only: no VM is made. A remove waits
//! for the machine's start lock, records its `op` so nothing starts or uses
//! the machine meanwhile (`Busy`; `Unresolved` once a gone owner left its
//! outcome unknown), and puts the record back when the delete is refused;
//! `machine update` is refused while Sandboxes run.
#![cfg(unix)] // unix stat/lock semantics; the Windows face is its own port

#![cfg(feature = "machine")]
#![allow(deprecated)] // nix's flock, as the product uses it
use layerfs_api::home::{self, paths, read_record, write_record};

use layerfs_bridge::control::SdkValue;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-mtd-{tag}-{}", std::process::id()));
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

fn lima(home: &Path, name: &str, more: &[(&str, SdkValue)]) {
    let mut fields = vec![
        ("name".to_owned(), t(name)),
        ("platform".to_owned(), t("lima")),
    ];
    fields.extend(more.iter().map(|(n, v)| ((*n).to_owned(), v.clone())));
    write_record(&paths::machine_record(home, name), &SdkValue::Map(fields)).unwrap();
}

fn dead_pid() -> i64 {
    let mut ended = std::process::Command::new("true").spawn().unwrap();
    let pid = i64::from(ended.id());
    ended.wait().unwrap();
    pid
}

#[test]
fn a_running_sandbox_keeps_its_machine_s_settings() {
    let home = tempdir("update");
    home::open(&home).unwrap();
    call(&home, "machine.list", &[]).unwrap();
    write_record(
        &paths::sandbox_record(&home, "s1"),
        &SdkValue::map([
            ("name", t("s1")),
            ("project", t("shop/app")),
            ("machine", t("docker")),
            ("state", t("running")),
        ]),
    )
    .unwrap();
    let (code, message) = call(
        &home,
        "machine.update",
        &[("name", t("docker")), ("image", t("sha256:abc"))],
    )
    .unwrap_err();
    assert_eq!(code, "InUse", "{message}");
    assert!(message.contains("s1"), "{message}");
    std::fs::remove_dir_all(paths::sandbox_dir(&home, "s1")).unwrap();
    call(
        &home,
        "machine.update",
        &[("name", t("docker")), ("image", t("sha256:abc"))],
    )
    .unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

/// An `op` intent of `name` owned by process `pid` (no start or boot
/// recorded: its liveness is the process id's).
fn op(name: &str, pid: i64, uncertain: bool) -> SdkValue {
    SdkValue::map([
        ("id", t("op-00000000000000ab")),
        ("name", t(name)),
        ("phase", t("delete")),
        ("args", SdkValue::Null),
        ("owner", SdkValue::map([("pid", SdkValue::Int(pid))])),
        ("started", SdkValue::Int(1)),
        ("updated", SdkValue::Int(2)),
        ("uncertain", SdkValue::Bool(uncertain)),
    ])
}

#[test]
fn a_machine_being_removed_is_used_by_no_call() {
    let home = tempdir("removing");
    home::open(&home).unwrap();
    let me = i64::from(std::process::id());
    lima(&home, "dev", &[("op", op("machine.remove", me, false))]);
    for (name, args) in [
        ("machine.remove", vec![("name", t("dev"))]),
        (
            "machine.update",
            vec![("name", t("dev")), ("image", t("/x.tar"))],
        ),
        ("machine.start", vec![("name", t("dev"))]),
    ] {
        let (code, message) = call(&home, name, &args).unwrap_err();
        assert_eq!(code, "Busy", "{name}: {message}");
        assert!(message.contains("machine.remove"), "{message}");
    }
    let refused = layerfs_api::running(&home, "dev").unwrap_err();
    assert_eq!(refused.code, "Busy", "{}", refused.message);
    // a read answers, probing nothing
    let got = call(&home, "machine.get", &[("name", t("dev"))]).unwrap();
    assert_eq!(got.get_text("state"), Some("removing"), "{got:?}");
    let status = call(&home, "machine.status", &[("name", t("dev"))]).unwrap();
    assert_eq!(status.get_text("state"), Some("removing"), "{status:?}");
    let shown = status.get("op").expect("status shows the op");
    assert_eq!(shown.get_text("name"), Some("machine.remove"));
    assert!(shown.get("owner").is_none(), "{shown:?}");
    // a remover that is gone after a certain outcome holds nothing
    lima(
        &home,
        "dev",
        &[("op", op("machine.remove", dead_pid(), false))],
    );
    call(
        &home,
        "machine.update",
        &[("name", t("dev")), ("image", t("/x.tar"))],
    )
    .unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_gone_owner_s_unknown_outcome_is_unresolved_until_removed() {
    let home = tempdir("unresolved");
    home::open(&home).unwrap();
    let lost = op("machine.create", dead_pid(), true);
    lima(
        &home,
        "dev",
        &[("op", lost), ("error", t("lost the reply"))],
    );
    let (code, message) = call(
        &home,
        "machine.update",
        &[("name", t("dev")), ("image", t("/x.tar"))],
    )
    .unwrap_err();
    assert_eq!(code, "Unresolved", "{message}");
    assert!(
        message.contains("`layerfs machine status dev`"),
        "{message}"
    );
    assert!(
        message.contains("`layerfs machine remove dev`"),
        "{message}"
    );
    let status = call(&home, "machine.status", &[("name", t("dev"))]).unwrap();
    assert_eq!(status.get_text("state"), Some("unresolved"), "{status:?}");
    let shown = status.get("op").expect("status shows the op");
    assert_eq!(shown.get_text("name"), Some("machine.create"));
    assert_eq!(shown.get("uncertain"), Some(&SdkValue::Bool(true)));
    let got = call(&home, "machine.get", &[("name", t("dev"))]).unwrap();
    assert_eq!(got.get_text("state"), Some("unresolved"), "{got:?}");
    assert_eq!(got.get_text("error"), Some("lost the reply"), "{got:?}");
    // the remove that clears it is not refused for it: it deletes, or its
    // delete is refused and the record stays as it was
    match call(&home, "machine.remove", &[("name", t("dev"))]) {
        Err((code, message)) => {
            assert!(
                !["Unresolved", "Busy"].contains(&code.as_str()),
                "{code}: {message}"
            );
            let record = read_record(&paths::machine_record(&home, "dev"))
                .unwrap()
                .expect("the record stays");
            assert_eq!(
                record.get("op").and_then(|o| o.get_text("name")),
                Some("machine.create")
            );
        }
        Ok(_) => assert!(!paths::machine_dir(&home, "dev").exists()),
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_remove_waits_for_the_start_lock_and_restores_a_refused_record() {
    let home = tempdir("remove");
    home::open(&home).unwrap();
    lima(&home, "dev", &[]);
    // another process's start holds the lock for a moment
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(paths::machine_dir(&home, "dev").join("start.lock"))
        .unwrap();
    nix::fcntl::flock(lock.as_raw_fd(), nix::fcntl::FlockArg::LockExclusive).unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(700));
        nix::fcntl::flock(lock.as_raw_fd(), nix::fcntl::FlockArg::Unlock).unwrap();
    });
    let started = Instant::now();
    let removed = call(&home, "machine.remove", &[("name", t("dev"))]);
    assert!(
        started.elapsed() >= Duration::from_millis(600),
        "the remove waited for the start lock"
    );
    release.join().unwrap();
    match removed {
        // the VM tool or bundle was missing: the record is as it was
        Err((code, message)) => {
            let record = read_record(&paths::machine_record(&home, "dev"))
                .unwrap()
                .expect("the record stays");
            assert!(record.get("op").is_none(), "{code}: {message}");
        }
        // a host where the delete found no VM to delete
        Ok(_) => assert!(!paths::machine_dir(&home, "dev").exists()),
    }
    std::fs::remove_dir_all(home).unwrap();
}
