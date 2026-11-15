//! The run-folder names of Sandboxes (layerfs-project#180). A new Sandbox
//! is `layerfs-<home id8>-<name>`. A record made before that prefix keeps
//! `lfs-<home id8>-<name>`: it is reopened, force-closed and reclaimed by
//! the name it holds. A record whose own name is neither of those two,
//! exactly, or whose directory is not that name under the run root, is
//! neither used nor deleted: no prefix is stripped from a recorded name.
//!
//! Nothing runs a daemon: a record names this test's own process, or a
//! `sleep` this test started, as its daemon.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::process::LocalProcess;
use layerfs_home::home::{self, paths, write_record};
use layerfs_sandbox::force::native_host;
use layerfs_sandbox::host::{Deploy, Host};
use layerfs_sandbox::lifecycle::{recorded_names, sandbox_name};
use layerfs_sandbox::record::{DaemonRecord, Local};
use layerfs_sandbox::reopen::{recorded, Recorded};
use std::path::{Path, PathBuf};

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A new home and the first 8 digits of its id.
fn new_home(tag: &str) -> (PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("lfs-names-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    home::private_dir(&dir).unwrap();
    let id = home::ensure(&dir).unwrap().id;
    (dir, id[..8].to_owned())
}

/// Sandbox `id` as its home records it once started: a native daemon, the
/// process `pid`, under the run-folder name `name` in `folder` (made here).
fn record(home: &Path, id: &str, name: &str, folder: &Path, pid: u32) -> DaemonRecord {
    let process = LocalProcess::observe(pid).unwrap();
    let daemon = DaemonRecord::local(Local {
        platform: "mac".to_owned(),
        name: name.to_owned(),
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
    });
    std::fs::create_dir_all(folder).unwrap();
    write_record(
        &paths::sandbox_record(home, id),
        &SdkValue::map([
            ("name", t(id)),
            ("project", t("shop/api")),
            ("machine", t("native")),
            ("id", t(id)),
            ("runtime", t("native")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
    daemon
}

/// Why `daemon` is not reopened as Sandbox `id`: always `InvalidInput`,
/// before anything it names is used or deleted.
fn refused(host: &Host, id: &str, daemon: &DaemonRecord) -> String {
    let refusal = match recorded(host, id, daemon) {
        Ok(_) => panic!("a record named {:?} was used", daemon.name()),
        Err(refusal) => refusal,
    };
    assert_eq!(refusal.code, "InvalidInput", "{}", refusal.message);
    assert!(
        refusal.message.contains("neither used nor deleted"),
        "{}",
        refusal.message
    );
    refusal.message
}

#[test]
fn a_record_holds_the_new_name_or_the_earlier_one_exactly() {
    let (home, id8) = new_home("exact");
    let run = paths::run_root(&home);
    let me = std::process::id();
    let host = native_host(&home, "s1").unwrap();
    let (new, old) = (format!("layerfs-{id8}-s1"), format!("lfs-{id8}-s1"));

    // a new Sandbox's name, and the two a record may hold
    assert_eq!(sandbox_name(host.deploy(), "s1").unwrap(), new);
    assert_eq!(
        recorded_names(host.deploy(), "s1").unwrap(),
        [new.clone(), old.clone()]
    );

    // either name, in the folder of that name under the run root
    for name in [&new, &old] {
        let folder = run.join(name);
        let daemon = record(&home, "s1", name, &folder, me);
        match recorded(&host, "s1", &daemon) {
            Ok(Recorded::Local { directory, .. }) => assert_eq!(directory, folder),
            // Linux has a second variant, a namespaces record
            #[allow(unreachable_patterns)]
            Ok(_) => panic!("{name}: not a local record"),
            Err(refusal) => panic!("{name}: {}", refusal.message),
        }
    }

    // a name and a folder that disagree, each one a valid name by itself
    let daemon = record(&home, "s1", &old, &run.join(&new), me);
    refused(&host, "s1", &daemon);
    let daemon = record(&home, "s1", &new, &run.join(&old), me);
    refused(&host, "s1", &daemon);

    // neither candidate: another prefix, another home's id, another
    // Sandbox's name, and a name that only ends in a candidate (so that
    // stripping a prefix would have found one)
    let other_home = format!("lfs-{}-s1", "f".repeat(8));
    let nested = format!("layerfs-{id8}-{old}");
    for name in [
        &format!("other-{id8}-s1"),
        &other_home,
        &format!("lfs-{id8}-s2"),
        &format!("layerfs-{id8}-s2"),
        &nested,
        &"layerfs-s1".to_owned(),
        &"s1".to_owned(),
    ] {
        let daemon = record(&home, "s1", name, &run.join(name), me);
        let message = refused(&host, "s1", &daemon);
        assert!(message.contains(&format!("(as {name})")), "{message}");
        assert!(run.join(name).is_dir(), "{name}: its folder was removed");
    }

    // a candidate name in a folder outside the run root
    let elsewhere = home.join("elsewhere").join(&old);
    let daemon = record(&home, "s1", &old, &elsewhere, me);
    refused(&host, "s1", &daemon);
    assert!(elsewhere.is_dir());

    // a deployment that names no earlier prefix has one candidate only
    let only_new = Host {
        deploy: Some(Deploy {
            legacy_prefix: None,
            ..host.deploy().unwrap().clone()
        }),
        ..host.clone()
    };
    assert_eq!(recorded_names(only_new.deploy(), "s1").unwrap(), [new]);
    let daemon = record(&home, "s1", &old, &run.join(&old), me);
    refused(&only_new, "s1", &daemon);
    std::fs::remove_dir_all(home).unwrap();
}

/// A `sleep` standing in for a Sandbox's daemon, ended when dropped.
#[cfg(target_os = "macos")]
struct StandIn(std::process::Child);

#[cfg(target_os = "macos")]
impl StandIn {
    fn start() -> Self {
        let child = std::process::Command::new("/bin/sleep")
            .arg("60")
            .spawn()
            .unwrap();
        Self(child)
    }

    /// Whether it still runs (an ended one is reaped here).
    fn runs(&mut self) -> bool {
        self.0.try_wait().unwrap().is_none()
    }

    /// Whether it ended within 5 s.
    fn ended(&mut self) -> bool {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while self.runs() {
            if std::time::Instant::now() >= until {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        true
    }
}

#[cfg(target_os = "macos")]
impl Drop for StandIn {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A forced close finds a Sandbox by the name its record holds: one made
/// under the earlier prefix is stopped and its `lfs-` run folder deleted,
/// like a new one. A record under neither name is refused before any
/// signal, and nothing of it is deleted.
#[cfg(target_os = "macos")]
#[test]
fn a_forced_close_finds_a_sandbox_under_either_name() {
    let (home, id8) = new_home("force");
    let run = paths::run_root(&home);
    for (id, prefix) in [("old", "lfs"), ("new", "layerfs")] {
        let folder = run.join(format!("{prefix}-{id8}-{id}"));
        let mut daemon = StandIn::start();
        let name = folder.file_name().unwrap().to_str().unwrap().to_owned();
        let value = record(&home, id, &name, &folder, daemon.0.id());
        let host = native_host(&home, id).unwrap();
        // found running first, as every other call on it reopens it
        let reopened = recorded(&host, id, &value).unwrap().reopen();
        assert!(reopened.is_ok(), "{id}: not reopened");
        drop(reopened);
        let closed = layerfs_sandbox::force::close(&host, &[]).unwrap();
        assert_eq!(closed.get_text("kind"), Some("Closed"), "{closed:?}");
        assert!(daemon.ended(), "{id}: its daemon still runs");
        assert!(!folder.exists(), "{id}: its run folder is left");
        assert!(!paths::sandbox_dir(&home, id).exists(), "{id}: its record");
    }

    let folder = run.join(format!("other-{id8}-odd"));
    let mut daemon = StandIn::start();
    let name = folder.file_name().unwrap().to_str().unwrap().to_owned();
    record(&home, "odd", &name, &folder, daemon.0.id());
    let host = native_host(&home, "odd").unwrap();
    let refusal = layerfs_sandbox::force::close(&host, &[]).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{}", refusal.message);
    assert!(daemon.runs(), "a refused close signalled the process");
    assert!(folder.is_dir(), "a refused close deleted the run folder");
    assert!(paths::sandbox_record(&home, "odd").is_file());
    drop(daemon);
    std::fs::remove_dir_all(home).unwrap();
}

/// After a restart (its daemon is gone), a Sandbox made under the earlier
/// prefix is found stopped by the name its record holds and reclaimed: its
/// `lfs-` run folder and its record go, so its name can be created again.
#[cfg(target_os = "macos")]
#[test]
fn an_earlier_sandbox_whose_daemon_ended_is_reclaimed() {
    let (home, id8) = new_home("reclaim");
    let folder = paths::run_root(&home).join(format!("lfs-{id8}-gone"));
    let mut daemon = StandIn::start();
    let name = folder.file_name().unwrap().to_str().unwrap().to_owned();
    let value = record(&home, "gone", &name, &folder, daemon.0.id());
    daemon.0.kill().unwrap();
    assert!(daemon.ended());
    let host = native_host(&home, "gone").unwrap();
    let failure = match recorded(&host, "gone", &value).unwrap().reopen() {
        Ok(_) => panic!("a Sandbox whose daemon ended was reopened"),
        Err(failure) => failure,
    };
    assert!(failure.container_stopped(), "{failure:?}");
    let container = value.container().unwrap().to_owned();
    let reclaimed = layerfs_sandbox::lifecycle::reclaim(&host, &container, true).unwrap();
    assert_eq!(reclaimed.get_text("kind"), Some("Reclaimed"));
    assert!(!folder.exists(), "its run folder is left");
    assert!(!paths::sandbox_dir(&home, "gone").exists());
    std::fs::remove_dir_all(home).unwrap();
}
