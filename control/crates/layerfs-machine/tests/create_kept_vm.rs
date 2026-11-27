//! A create that fails once its VM exists keeps the VM as broken `failed`
//! and records the worker port the VM was made with, before the VM is
//! made: the port Lima's template forwards is the recorded one, and a
//! later `machine start` reaches the provider on it instead of being
//! refused `InvalidInput` for a port it never recorded. A stand-in
//! `limactl` whose `create` makes the instance and then fails logs every
//! command; no VM is used. One test, because it sets the process's
//! environment.
#![cfg(all(feature = "machine", target_os = "macos"))]
use layerfs_home::events::Events;
use layerfs_home::home;
use layerfs_home::state::Reason;
use layerfs_machine::{create, ensure_built_ins, record::MachineRecord, running};
use layerfs_machine::{MachineCreate, Related};
use layerfs_protocol::sdk::SdkRefusal;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn none(_: &Path, _: &str) -> Result<Vec<String>, SdkRefusal> {
    Ok(Vec::new())
}

fn free(_: &Path, _: &str) -> Result<(), SdkRefusal> {
    Ok(())
}

const RELATED: Related = Related {
    stores_on: none,
    sandboxes_on: none,
    hosted_on: none,
    can_remove: free,
    can_stop: free,
    can_change: free,
};

#[test]
fn a_vm_a_failed_create_kept_starts_on_its_recorded_port() {
    let root = std::env::temp_dir().join(format!("lfs-kept-vm-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bin")).unwrap();
    // a stand-in limactl: no instance until `create`, which keeps its
    // template, makes the instance (stopped) and fails; `start` fails
    let script = format!(
        r#"#!/bin/sh
echo "$*" >> '{root}/calls'
case "$1" in
  --version) echo 'limactl version 0.0-stand-in' ;;
  list) [ -f '{root}/made' ] && printf 'Stopped\t{root}/vm\n' ;;
  disk) ;;
  create) cat > '{root}/template'; touch '{root}/made'; echo 'stand-in create' >&2; exit 1 ;;
  start) echo 'stand-in start' >&2; exit 1 ;;
esac
exit 0
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
    let id = home::ensure(&h).unwrap().id;
    ensure_built_ins(&h, &id).unwrap();

    let request = MachineCreate {
        name: "dev".to_owned(),
        platform: "lima".to_owned(),
        cpus: None,
        memory: None,
        disk: None,
        image: None,
        os_image: Some("stock".to_owned()),
    };
    let mut sink = |_| {};
    let failed = create(&h, &mut Events::new(&mut sink), &request, RELATED).unwrap_err();
    assert_eq!(failed.code, "Failed", "{failed:?}");
    assert_eq!(
        failed.detail.get("vm_exists"),
        Some(&layerfs_protocol::sdk::SdkValue::Bool(true)),
        "{failed:?}"
    );
    let record = MachineRecord::of(&h, "dev").unwrap();
    assert_eq!(record.reason(), Some(Reason::Failed), "{record:?}");
    assert!(record.op().is_none(), "{record:?}");
    let port = record
        .field("lasting")
        .and_then(|lasting| lasting.get_int("port"))
        .expect("the kept VM's worker port is recorded");
    let template = fs::read_to_string(root.join("template")).unwrap();
    assert!(
        template.contains(&format!("\"hostPort\": {port}")),
        "the VM forwards the recorded port {port}: {template}"
    );

    // the kept VM is started on that port: the provider is asked to boot it
    let start = running(&h, "dev", RELATED).unwrap_err();
    assert_ne!(start.code, "InvalidInput", "{start:?}");
    assert_eq!(start.code, "Failed", "{start:?}");
    let calls = fs::read_to_string(root.join("calls")).unwrap();
    assert!(
        calls.lines().any(|line| line.starts_with("start")),
        "the kept VM was not started: {calls}"
    );

    std::env::set_var("PATH", path);
    std::env::remove_var("LAYERFS_BUNDLE");
    fs::remove_dir_all(root).unwrap();
}
