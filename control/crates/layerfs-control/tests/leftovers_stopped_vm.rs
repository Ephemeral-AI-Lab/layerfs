//! `home leftovers` on a VM that stopped under the worker record it left
//! (UI pass 1, minor): its worker does not answer, and its provider says
//! the VM is stopped, so it is `blocked_by {machine, reason: stopped}`,
//! not a worker that ran out of time. A VM whose provider says it runs
//! but whose worker is silent stays `timeout`. A stand-in `limactl` (its
//! state in a file) answers the provider's probe. One test, because it
//! sets the process's environment.
#![cfg(all(feature = "machine", any(target_os = "linux", target_os = "macos")))]
use layerfs_bridge::control::SdkValue;
use layerfs_control::doctor::leftover_list::list;
use layerfs_home::home::{paths, write_record};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const STAND_IN: &str = r#"#!/bin/sh
d="${0%/*}"
case "$1" in
  --version) echo "limactl version 0.0-stand-in" ;;
  list) read -r state < "$d/state"; printf '%s\t/nonexistent\n' "$state" ;;
  *) echo "the stand-in refuses: $*" >&2; exit 9 ;;
esac
"#;

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-leftovers-vm-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

#[test]
fn a_stopped_vm_with_a_stale_worker_record_is_blocked_by_stopped() {
    let tools = folder("tools");
    let limactl = tools.join("limactl");
    std::fs::write(&limactl, STAND_IN).unwrap();
    std::fs::set_permissions(&limactl, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("PATH", &tools);
    // only the stand-ins: no fallback folder after PATH
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    let home = folder("home").join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    layerfs_control::migrate::open(&home).unwrap();
    write_record(
        &paths::machine_record(&home, "dev"),
        &SdkValue::map([("name", t("dev")), ("platform", t("lima"))]),
    )
    .unwrap();
    // the worker record a VM that stopped left: nothing listens there
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = closed.local_addr().unwrap().to_string();
    drop(closed);
    write_record(
        &paths::worker_record(&home, "dev"),
        &SdkValue::map([("address", t(&address)), ("secret", t("k"))]),
    )
    .unwrap();
    let entry = |state: &str| {
        std::fs::write(tools.join("state"), format!("{state}\n")).unwrap();
        let listed = list(&home, Some("dev")).unwrap();
        let Some(SdkValue::List(rows)) = listed.get("not_listed") else {
            panic!("no not_listed: {listed:?}");
        };
        assert_eq!(rows.len(), 1, "{rows:?}");
        rows[0].clone()
    };

    let stopped = entry("Stopped");
    let by = stopped.get("blocked_by").unwrap();
    assert_eq!(by.get_text("machine"), Some("dev"), "{stopped:?}");
    assert_eq!(by.get_text("reason"), Some("stopped"), "{stopped:?}");
    assert_eq!(stopped.get_text("message"), Some("machine dev is stopped"));
    assert!(stopped.get("reason").is_none(), "{stopped:?}");

    let silent = entry("Running");
    assert_eq!(silent.get_text("reason"), Some("timeout"), "{silent:?}");
    assert!(silent.get("blocked_by").is_none(), "{silent:?}");

    let _ = std::fs::remove_dir_all(home.parent().unwrap());
    let _ = std::fs::remove_dir_all(&tools);
}
