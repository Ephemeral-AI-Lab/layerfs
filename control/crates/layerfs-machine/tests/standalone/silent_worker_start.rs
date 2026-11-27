//! A recorded worker that does not answer is never started over while its
//! machine runs: a start would end that worker and every Sandbox on it.
//! The machine is recorded `broken` `not_answering` and the start refused
//! `Unavailable`, naming a stop and a start and how many Sandboxes are on
//! it; the provider is asked of the VM alone and no `limactl start` runs.
//! A machine its provider reports stopped is still started. A stand-in
//! `limactl` logs every command; no VM is used. One test, because it sets
//! the process's environment.
#![cfg(all(feature = "machine", target_os = "macos"))]
use layerfs_home::disk::{self, paths, write_record};
use layerfs_home::reply::state::Reason;
use layerfs_machine::{
    ensure_built_ins, record::machine::MachineRecord, running, vm_name, Related,
};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn none(_: &Path, _: &str) -> Result<Vec<String>, SdkRefusal> {
    Ok(Vec::new())
}

fn one_sandbox(_: &Path, _: &str) -> Result<Vec<String>, SdkRefusal> {
    Ok(vec!["sb".to_owned()])
}

fn free(_: &Path, _: &str) -> Result<(), SdkRefusal> {
    Ok(())
}

const RELATED: Related = Related {
    stores_on: none,
    sandboxes_on: one_sandbox,
    hosted_on: none,
    can_remove: free,
    can_stop: free,
    can_change: free,
};

#[test]
fn a_silent_worker_on_a_running_machine_is_refused_not_replaced() {
    let root = std::env::temp_dir().join(format!("lfs-silent-start-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bin")).unwrap();
    // a stand-in limactl: `list` reports the state in `<root>/state`;
    // `start` fails; every command is logged
    let script = format!(
        r#"#!/bin/sh
echo "$*" >> '{root}/calls'
case "$1" in
  --version) echo 'limactl version 0.0-stand-in' ;;
  list) printf '%s\t{root}/vm\n' "$(cat '{root}/state')" ;;
  start) echo 'stand-in start' >&2; exit 1 ;;
esac
"#,
        root = root.display()
    );
    let limactl = root.join("bin/limactl");
    fs::write(&limactl, script).unwrap();
    fs::set_permissions(&limactl, fs::Permissions::from_mode(0o755)).unwrap();
    let bundle = root.join("bundle");
    fs::create_dir(&bundle).unwrap();
    for name in ["layerfs", "layerfs-daemon", "layerfs-sandbox-helper"] {
        fs::write(bundle.join(name), b"").unwrap();
    }
    fs::write(
        bundle.join("bundle.json"),
        r#"{"hash":"abcd","protocol":3}"#,
    )
    .unwrap();
    std::env::set_var("LAYERFS_BUNDLE", &bundle);
    let path = std::env::var_os("PATH").unwrap();
    std::env::set_var(
        "PATH",
        format!("{}:{}", root.join("bin").display(), path.to_string_lossy()),
    );

    let h = root.join("home");
    let id = disk::ensure(&h).unwrap().id;
    ensure_built_ins(&h, &id).unwrap();
    MachineRecord::from_record(
        "dev",
        SdkValue::map([
            ("name", SdkValue::text("dev")),
            ("platform", SdkValue::text("lima")),
            ("vm_name", SdkValue::text(vm_name(&id, "dev"))),
            ("state", SdkValue::text("running")),
            ("lasting", SdkValue::map([("port", SdkValue::Int(41999))])),
        ]),
    )
    .write(&h)
    .unwrap();
    // a worker recorded where nothing listens: its hello fails at once
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = closed.local_addr().unwrap();
    drop(closed);
    write_record(
        &paths::worker_record(&h, "dev"),
        &SdkValue::map([
            ("address", SdkValue::text(address.to_string())),
            ("secret", SdkValue::text("00".repeat(32))),
            ("started", SdkValue::Int(0)),
        ]),
    )
    .unwrap();
    let calls = || fs::read_to_string(root.join("calls")).unwrap_or_default();

    // running: refused, recorded not_answering, nothing started
    fs::write(root.join("state"), "Running").unwrap();
    let refused = running(&h, "dev", RELATED).unwrap_err();
    assert_eq!(refused.code, "Unavailable", "{refused:?}");
    assert_eq!(
        refused.detail.get_text("fix"),
        Some("layerfs machine stop dev --force && layerfs machine start dev"),
        "{refused:?}"
    );
    assert!(
        refused.message.contains("1 Sandbox is on it")
            && refused.message.contains("layerfs machine doctor dev"),
        "{}",
        refused.message
    );
    let record = MachineRecord::of(&h, "dev").unwrap();
    assert_eq!(record.reason(), Some(Reason::NotAnswering), "{record:?}");
    assert!(record.op().is_none(), "no start's op was recorded");
    let seen = calls();
    assert!(seen.contains("list"), "the provider was asked: {seen}");
    assert!(
        !seen.lines().any(|line| line.starts_with("start")),
        "a running machine was started over its worker: {seen}"
    );
    assert!(
        paths::worker_record(&h, "dev").is_file(),
        "the worker's endpoint stays for an explicit stop"
    );

    // stopped: started as before (the stand-in start then fails)
    fs::write(root.join("state"), "Stopped").unwrap();
    let failed = running(&h, "dev", RELATED).unwrap_err();
    assert_eq!(failed.code, "Failed", "{failed:?}");
    let seen = calls();
    assert!(
        seen.lines().any(|line| line.starts_with("start")),
        "a stopped machine was not started: {seen}"
    );

    std::env::set_var("PATH", path);
    std::env::remove_var("LAYERFS_BUNDLE");
    fs::remove_dir_all(root).unwrap();
}
