//! A machine record naming a platform removed in 2.0 (`firecracker`, `ssh`,
//! `microsandbox`), left by an older home: `machine.list` and `machine.get`
//! still answer and say why it is gone, every other call on it is refused
//! `Unsupported` with that reason, and `machine.remove` forgets the record
//! only, saying what it left. No tool runs.
#![cfg(feature = "machine")]
use layerfs_api::home::{paths, write_record};
use layerfs_machine::table::removed::removed_reason;
use layerfs_protocol::sdk::SdkValue;
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
    let detail = ("detail".to_owned(), SdkValue::Bool(true));
    let args = SdkValue::Map(
        (args.iter().map(|(n, v)| ((*n).to_owned(), v.clone())))
            .chain([detail])
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn shows_removed(machine: &SdkValue, platform: &str, reason: &str) {
    let name = machine.get_text("machine").unwrap();
    assert_eq!(machine.get_text("platform"), Some(platform), "{machine:?}");
    assert_eq!(machine.get("backend"), Some(&SdkValue::Null), "{machine:?}");
    assert_eq!(machine.get_text("state"), Some("broken"), "{machine:?}");
    assert_eq!(machine.get_text("reason"), Some("unsupported"));
    let fix = format!("layerfs machine remove {name}");
    assert_eq!(machine.get_text("fix"), Some(fix.as_str()));
    assert_eq!(machine.get_text("message"), Some(reason), "{machine:?}");
    assert_eq!(machine.get("supports"), Some(&SdkValue::List(Vec::new())));
    assert_eq!(
        machine.get("loopback"),
        Some(&SdkValue::Null),
        "{machine:?}"
    );
}

#[test]
fn a_removed_platform_is_shown_refused_and_its_record_removed() {
    let _env = crate::env_lock();
    // the built-in docker machine is probed by `machine.list`: no tool is found
    let empty = tempdir("path");
    std::env::set_var("PATH", &empty);
    // only the stand-ins: no fallback folder after PATH
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
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
    let SdkValue::List(listed) = list.get("items").unwrap() else {
        panic!("{list:?}")
    };
    for (name, platform, _) in &machines {
        let reason = removed_reason(platform).unwrap();
        let shown = listed
            .iter()
            .find(|m| m.get_text("machine") == Some(name))
            .unwrap_or_else(|| panic!("{name} not listed: {list:?}"));
        shows_removed(shown, platform, reason);
        let got = call(&home, "machine.get", &[("machine", t(name))]).unwrap();
        shows_removed(&got, platform, reason);
    }

    for (name, platform, _) in &machines {
        let reason = removed_reason(platform).unwrap();
        let refused = [
            ("machine.start", vec![]),
            ("machine.stop", vec![]),
            ("machine.update", vec![("cpus", SdkValue::Int(4))]),
        ];
        for (call_name, mut args) in refused {
            args.push(("machine", t(name)));
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
        // v19: the home's image copy locks of the machine go with it
        let locks = home.join("run/image-lock").join(name);
        std::fs::create_dir_all(&locks).unwrap();
        std::fs::write(locks.join(format!("{}.lock", "a".repeat(64))), b"").unwrap();
        let removed = call(&home, "machine.remove", &[("machine", t(name))]).unwrap();
        assert!(!locks.exists(), "{name}'s image copy locks are removed");
        assert_eq!(removed.get_text("machine"), Some(*name));
        assert_eq!(removed.get("forgotten"), None);
        assert_eq!(removed.get_text("outcome"), Some("removed"));
        let note = removed.get_text("message").unwrap();
        assert!(note.starts_with("only its record was removed ("), "{note}");
        assert!(note.contains(reason), "{note}");
        assert!(note.ends_with("nothing on its host was touched"), "{note}");
        assert!(!paths::machine_dir(&home, name).exists());
        assert_eq!(
            call(&home, "machine.get", &[("machine", t(name))])
                .unwrap_err()
                .0,
            "NotFound"
        );
    }
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(empty).unwrap();
}
