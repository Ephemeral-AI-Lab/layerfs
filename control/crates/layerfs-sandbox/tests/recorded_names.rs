//! The run-folder names of Sandboxes (layerfs-project#180). A Sandbox is
//! `layerfs-<home id8>-<name>`. A record whose own name is not that one,
//! exactly (the `lfs-` prefix before #180 included), or whose directory is
//! not that name under the run root, is neither used nor deleted: no
//! prefix is stripped from a recorded name. A record whose name this home
//! does not give names `sandbox remove` as its fix, and that remove takes
//! its records alone: its run folder and daemon process are left, and
//! `left` names both with the line that removes them: the process only
//! while the one its record identifies still runs, the folder as one shell
//! word (RV3 FAIL 4).
//!
//! A record this build does not read (a field missing, here `boot`) is
//! refused with `sandbox remove` as its fix, and that remove, forced or
//! not, takes its records alone with no signal sent (RV3 FAIL 2). The
//! native runtime is macOS only (RV3 FAIL 3), and so are these tests. A
//! forwarded record (a worker's Sandbox: forward, name, runtime, platform,
//! image) names no local daemon and is never read as an unread local one
//! (B10, the CUT3 regression).
//!
//! Nothing runs a daemon: a record names this test's own process.
#![cfg(target_os = "macos")]
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::process::LocalProcess;
use layerfs_home::home::{self, paths, write_record};
use layerfs_sandbox::family::{remove, RemoveRequest, Removed};
use layerfs_sandbox::force::native_host;
use layerfs_sandbox::host::Host;
use layerfs_sandbox::lifecycle::sandbox_name;
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

/// Sandbox `id`'s record written again with its daemon record changed by
/// `change`.
fn rewrite(
    home: &Path,
    id: &str,
    daemon: &DaemonRecord,
    change: impl Fn(&str, SdkValue) -> Option<SdkValue>,
) {
    let SdkValue::Map(fields) = daemon.to_record() else {
        panic!("a daemon record is a map")
    };
    let fields = (fields.into_iter())
        .filter_map(|(name, value)| change(&name, value).map(|value| (name, value)))
        .collect();
    write_record(
        &paths::sandbox_record(home, id),
        &SdkValue::map([
            ("name", t(id)),
            ("project", t("shop/api")),
            ("machine", t("native")),
            ("id", t(id)),
            ("runtime", t("native")),
            ("daemon", SdkValue::Map(fields)),
        ]),
    )
    .unwrap();
}

/// `sandbox.remove(id, { force })`'s outcome, which must be `already_gone`:
/// its `left`, joined.
fn removed_alone(home: &Path, id: &str, force: bool) -> String {
    let mut sink = |_| {};
    let mut events = layerfs_home::events::Events::new(&mut sink);
    let request = RemoveRequest {
        name: id.to_owned(),
        force,
    };
    match remove(home, &mut events, &request) {
        Ok(Removed::AlreadyGone { left }) => left.join(" | "),
        Ok(Removed::Stopped { .. }) => panic!("Sandbox {id} was stopped"),
        Err(refusal) => panic!("Sandbox {id}: {refusal:?}"),
    }
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
fn a_record_holds_the_one_name_exactly() {
    let (home, id8) = new_home("exact");
    let run = paths::run_root(&home);
    let me = std::process::id();
    let host = native_host(&home, "s1").unwrap();
    let (new, old) = (format!("layerfs-{id8}-s1"), format!("lfs-{id8}-s1"));
    assert_eq!(sandbox_name(host.deploy(), "s1").unwrap(), new);

    // the name, in the folder of that name under the run root
    let folder = run.join(&new);
    let daemon = record(&home, "s1", &new, &folder, me);
    match recorded(&host, "s1", &daemon) {
        Ok(Recorded::Local { directory, .. }) => assert_eq!(directory, folder),
        // Linux has a second variant, a namespaces record
        #[allow(unreachable_patterns)]
        Ok(_) => panic!("{new}: not a local record"),
        Err(refusal) => panic!("{new}: {}", refusal.message),
    }

    // the name in another folder
    let daemon = record(&home, "s1", &new, &run.join(&old), me);
    refused(&host, "s1", &daemon);

    // any other name: the prefix before layerfs-project#180, another
    // prefix, another home's id, another Sandbox's name, and a name that
    // only ends in the name (so that stripping a prefix would have found it)
    let nested = format!("layerfs-{id8}-{new}");
    for name in [
        &old,
        &format!("other-{id8}-s1"),
        &format!("layerfs-{}-s1", "f".repeat(8)),
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

    // the name in a folder outside the run root
    let elsewhere = home.join("elsewhere").join(&new);
    let daemon = record(&home, "s1", &new, &elsewhere, me);
    refused(&host, "s1", &daemon);
    assert!(elsewhere.is_dir());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_record_with_a_name_this_home_does_not_give_is_removed_by_its_records() {
    let (home, id8) = new_home("foreign");
    let run = paths::run_root(&home);
    let me = std::process::id();
    let host = native_host(&home, "s1").unwrap();
    let old = format!("lfs-{id8}-s1");
    let folder = run.join(&old);
    let daemon = record(&home, "s1", &old, &folder, me);
    let refusal = recorded(&host, "s1", &daemon).err().unwrap();
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs sandbox remove s1"),
        "{refusal:?}"
    );
    let mut sink = |_| {};
    let mut events = layerfs_home::events::Events::new(&mut sink);
    let request = RemoveRequest {
        name: "s1".to_owned(),
        force: false,
    };
    let left = match remove(&home, &mut events, &request).unwrap() {
        Removed::AlreadyGone { left } => left,
        Removed::Stopped { .. } => panic!("a record named {old} was stopped"),
    };
    let said = left.join(" | ");
    assert!(said.contains(&old), "{said}");
    assert!(said.contains(&folder.display().to_string()), "{said}");
    assert!(said.contains(&format!("kill {me};")), "{said}");
    assert!(!paths::sandbox_record(&home, "s1").exists());
    assert!(folder.is_dir(), "its run folder was removed");
    assert!(LocalProcess::observe(me).is_ok());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_foreign_records_left_names_its_process_only_while_it_is_the_same_one() {
    let (home, id8) = new_home("foreign-gone");
    let me = std::process::id();
    let old = format!("lfs-{id8}-s1");
    // a folder whose path is not one shell word
    let folder = home.join("my runs").join(&old);
    let daemon = record(&home, "s1", &old, &folder, me);
    // the same id with another start time: a reused id, never offered to kill
    rewrite(&home, "s1", &daemon, |name, value| match (name, &value) {
        ("start", SdkValue::Text(start)) => {
            Some(t(&(start.parse::<u64>().unwrap() + 1).to_string()))
        }
        _ => Some(value),
    });
    let said = removed_alone(&home, "s1", false);
    assert!(!said.contains("kill"), "{said}");
    assert!(said.contains("is gone"), "{said}");
    let quoted = format!("rm -rf '{}'", folder.display());
    assert!(said.contains(&quoted), "{said}");
    assert!(folder.is_dir(), "its run folder was removed");
    assert!(LocalProcess::observe(me).is_ok());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_record_this_build_does_not_read_is_removed_by_its_records_without_a_signal() {
    for force in [false, true] {
        let (home, id8) = new_home(&format!("unread-{force}"));
        let me = std::process::id();
        let host = native_host(&home, "s1").unwrap();
        let new = format!("layerfs-{id8}-s1");
        let folder = paths::run_root(&home).join(&new);
        let daemon = record(&home, "s1", &new, &folder, me);
        // a record an earlier build wrote, without `boot`
        rewrite(&home, "s1", &daemon, |name, value| {
            (name != "boot").then_some(value)
        });
        let unread = layerfs_sandbox::record::SandboxRecord::read(&home, "s1")
            .unwrap()
            .and_then(|record| record.daemon().cloned())
            .unwrap();
        let refusal = recorded(&host, "s1", &unread).err().unwrap();
        assert_eq!(refusal.code, "InvalidInput", "{refusal:?}");
        assert_eq!(
            refusal.detail.get_text("fix"),
            Some("layerfs sandbox remove s1"),
            "{refusal:?}"
        );
        let said = removed_alone(&home, "s1", force);
        assert!(said.contains("not one this build reads"), "{said}");
        assert!(said.contains("no signal was sent"), "{said}");
        assert!(said.contains(&folder.display().to_string()), "{said}");
        assert!(!paths::sandbox_record(&home, "s1").exists());
        // the process its record names was not signalled: it is this one
        assert!(LocalProcess::observe(me).is_ok());
        std::fs::remove_dir_all(home).unwrap();
    }
}

/// A forwarded record holds no directory, container, pid, port or labels:
/// it is not a local record, so `unread` never sends its remove down the
/// records-only path (B10). The same fields with no forward are unread.
#[test]
fn a_forwarded_record_is_not_an_unread_local_one() {
    use layerfs_sandbox::host::Started;
    use layerfs_sandbox::record::Forward;
    use layerfs_sandbox::reopen::unread;
    let started = Started {
        name: Some("layerfs-00000000-s1".to_owned()),
        runtime: Some("namespaces".to_owned()),
        platform: Some("docker".to_owned()),
        image: Some(format!("sha256:{}", "a".repeat(64))),
        ..Started::default()
    };
    let address = "127.0.0.1:4100".parse().unwrap();
    let forwarded = DaemonRecord::forwarded(Forward::new(address, "box", None), &started);
    assert_eq!(unread(&forwarded), None);
    let encoded = forwarded.to_record();
    let SdkValue::Map(fields) = encoded else {
        panic!("{encoded:?}")
    };
    let local = SdkValue::Map(fields.into_iter().filter(|(n, _)| n != "forward").collect());
    let local = DaemonRecord::decode(&local).unwrap();
    assert!(
        unread(&local).is_some(),
        "a local record missing its fields"
    );
}
