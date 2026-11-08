//! A machine record naming a platform removed in 2.0 (`firecracker`, `ssh`,
//! `microsandbox`), left by an older home: `machine.list` and `machine.get`
//! still answer and say why it is gone, every other call on it is refused
//! `Unsupported` with that reason, and `machine.remove` forgets the record
//! only, saying what it left. No tool runs.
#![cfg(feature = "machine")]
use layerfs_api::home::{paths, write_record};
use layerfs_bridge::control::SdkValue;
use layerfs_sandbox_backend::platform_policy::removed_reason;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-removed-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
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

fn shows_removed(machine: &SdkValue, platform: &str, reason: &str) {
    assert_eq!(machine.get_text("platform"), Some(platform), "{machine:?}");
    assert_eq!(machine.get_text("backend"), Some("-"), "{machine:?}");
    assert_eq!(
        machine.get_text("state"),
        Some("unsupported"),
        "{machine:?}"
    );
    assert_eq!(machine.get_text("error"), Some(reason), "{machine:?}");
    let supports = machine.get("supports").unwrap();
    assert_eq!(supports.get("sqlite"), Some(&SdkValue::Bool(false)));
    assert_eq!(
        supports.get("postgresql_sandbox"),
        Some(&SdkValue::Bool(false))
    );
    assert_eq!(supports.get_text("reason"), Some(reason));
}

#[test]
fn a_removed_platform_is_shown_refused_and_forgotten() {
    // the built-in docker machine is probed by `machine.list`: no tool is found
    let empty = tempdir("path");
    std::env::set_var("PATH", &empty);
    std::env::set_var("HOME", &empty);
    let home = tempdir("home");
    layerfs_api::home::open(&home).unwrap();
    let machines = [
        (
            "old-ssh",
            "ssh",
            ("ssh", SdkValue::map([("host", t("box.invalid"))])),
        ),
        (
            "old-msb",
            "microsandbox",
            ("vm_name", t("lfs-0123abcd-old-msb")),
        ),
        ("old-fc", "firecracker", ("host", t("dev"))),
    ];
    for (name, platform, extra) in &machines {
        let mut record = SdkValue::map([
            ("name", t(name)),
            ("platform", t(platform)),
            ("state", t("running")),
            extra.clone(),
        ]);
        if *platform == "firecracker" {
            record = layerfs_api::home::with_field(record, "vm_name", t("lfs-0123abcd-old-fc"));
        }
        write_record(&paths::machine_record(&home, name), &record).unwrap();
    }

    let list = call(&home, "machine.list", &[]).unwrap();
    let SdkValue::List(listed) = list.get("machines").unwrap() else {
        panic!("{list:?}")
    };
    for (name, platform, (field, value)) in &machines {
        let reason = removed_reason(platform).unwrap();
        let shown = listed
            .iter()
            .find(|m| m.get_text("name") == Some(name))
            .unwrap_or_else(|| panic!("{name} not listed: {list:?}"));
        shows_removed(shown, platform, reason);
        assert_eq!(shown.get(field), Some(value), "{shown:?}");
        let got = call(&home, "machine.get", &[("name", t(name))]).unwrap();
        shows_removed(&got, platform, reason);
    }

    for (name, platform, _) in &machines {
        let reason = removed_reason(platform).unwrap();
        let refused = [
            ("machine.status", vec![]),
            ("machine.start", vec![]),
            ("machine.stop", vec![]),
            ("machine.update", vec![("cpus", SdkValue::Int(4))]),
        ];
        for (call_name, mut args) in refused {
            args.push(("name", t(name)));
            assert_eq!(
                call(&home, call_name, &args),
                Err(("Unsupported".to_owned(), reason.to_owned())),
                "{call_name} {name}"
            );
        }
        let sandbox = call(
            &home,
            "sandbox.create",
            &[
                ("project", t("shop/api")),
                ("name", t("s")),
                ("machine", t(name)),
            ],
        );
        assert_eq!(
            sandbox,
            Err(("Unsupported".to_owned(), reason.to_owned())),
            "{name}"
        );
        assert!(paths::machine_record(&home, name).exists());
    }

    for (name, platform, _) in &machines {
        let reason = removed_reason(platform).unwrap();
        let removed = call(&home, "machine.remove", &[("name", t(name))]).unwrap();
        assert_eq!(removed.get_text("name"), Some(*name));
        assert_eq!(removed.get("removed"), Some(&SdkValue::Bool(true)));
        let note = removed.get_text("note").unwrap();
        assert!(note.starts_with("forgot the record only: "), "{note}");
        assert!(note.contains(reason), "{note}");
        let left = match *platform {
            "ssh" => "nothing on its host was touched",
            "firecracker" => "its microVM lfs-0123abcd-old-fc was not touched: its folder /layerfs/data/firecracker/vms/lfs-0123abcd-old-fc and data disk /layerfs/data/firecracker/disks/lfs-0123abcd-old-fc.ext4 stay in Lima machine dev",
            _ => "`msb remove --force lfs-0123abcd-old-msb` and `msb volume remove lfs-0123abcd-old-msb-data`",
        };
        assert!(note.contains(left), "{note}");
        assert!(!paths::machine_dir(&home, name).exists());
        assert_eq!(
            call(&home, "machine.get", &[("name", t(name))])
                .unwrap_err()
                .0,
            "NotFound"
        );
    }
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(empty).unwrap();
}
