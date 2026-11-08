#![cfg(unix)]
//! `lost` is reachable (layerfs-project#113, compute-plane 2.0 P-1, §8): a
//! Sandbox whose process tree is killed behind control's back reads `lost`
//! in `sandbox.get`, and `sandbox.close` clears it. The live half needs the
//! native daemon at LAYERFS_DAEMON; without it, it says NOT_RUN.
use layerfs_api::home::{self, paths};
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-lost-{tag}-{}", std::process::id()));
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

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

#[test]
fn a_sandbox_killed_behind_controls_back_is_lost_until_closed() {
    let Ok(daemon) = std::env::var("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: LAYERFS_DAEMON unset, no live Sandbox");
        return;
    };
    let home = tempdir("home");
    home::open(&home).unwrap();
    home::import_env(&home, &[("LAYERFS_DAEMON".to_owned(), daemon)]).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "lost\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    let folder_text = folder.display().to_string();
    ok(
        &home,
        "project.init",
        &[("project", t("shop/api")), ("folder", t(&folder_text))],
    );
    let made = ok(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/api")),
            ("name", t("s1")),
            ("machine", t("native")),
        ],
    );
    assert_eq!(made.get_text("state"), Some("running"));
    let got = ok(&home, "sandbox.get", &[("name", t("s1"))]);
    assert_eq!(got.get_text("state"), Some("running"), "{got:?}");

    // its daemon's process group, killed behind control's back
    let record = home::read_record(&paths::sandbox_record(&home, "s1"))
        .unwrap()
        .unwrap();
    let pid = record.get("daemon").and_then(|d| d.get_int("pid")).unwrap();
    let killed = std::process::Command::new("kill")
        .args(["-KILL", &format!("-{pid}"), &pid.to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    let deadline = Instant::now() + Duration::from_secs(10);
    let got = loop {
        let got = ok(&home, "sandbox.get", &[("name", t("s1"))]);
        if got.get_text("state") != Some("running") || Instant::now() >= deadline {
            break got;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(got.get_text("state"), Some("lost"), "{got:?}");
    // the list reads the record alone: it does not ask the machine
    let listed = layerfs_api::json::print(&ok(&home, "sandbox.list", &[]));
    assert!(listed.contains("\"s1\""), "{listed}");

    // close clears it: the record goes and the name is free
    let closed = ok(&home, "sandbox.close", &[("name", t("s1"))]);
    assert_eq!(
        closed.get("was_gone"),
        Some(&SdkValue::Bool(true)),
        "{closed:?}"
    );
    assert_eq!(
        call(&home, "sandbox.get", &[("name", t("s1"))])
            .unwrap_err()
            .0,
        "NoSuchSandbox"
    );
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
}
