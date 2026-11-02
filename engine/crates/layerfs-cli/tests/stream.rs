//! `layerfs workspace exec --stream` (#533 review, group calls): the
//! command's output passes through and the command exits with the
//! command's own code; 125 stays for refusals. The live half needs the
//! native daemon at LAYERFS_DAEMON; without it, it says NOT_RUN.
use layerfs_api::calls;
use layerfs_api::home;
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const LAYERFS: &str = env!("CARGO_BIN_EXE_layerfs");

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-cli-stream-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    calls::call(Some(home), name, args, &mut |_| {})
        .unwrap_or_else(|r| panic!("{name}: {} {}", r.code, r.message))
}

fn layerfs(home: &Path, words: &[&str]) -> std::process::Output {
    Command::new(LAYERFS)
        .args(words)
        .env("LAYERFS_HOME", home)
        .output()
        .unwrap()
}

#[test]
fn exec_stream_passes_output_and_the_commands_exit_code() {
    let home = tempdir("home");
    // a refusal is 125, before any Sandbox
    let refused = layerfs(&home, &["workspace", "exec", "t1:/w", "true", "--stream"]);
    assert_eq!(refused.status.code(), Some(125));

    let Ok(daemon) = std::env::var("LAYERFS_DAEMON") else {
        eprintln!("NOT_RUN: LAYERFS_DAEMON unset, no live Sandbox");
        let _ = std::fs::remove_dir_all(home);
        return;
    };
    home::open(&home).unwrap();
    home::import_env(&home, &[("LAYERFS_DAEMON".to_owned(), daemon)]).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "streamed\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    let folder_text = folder.display().to_string();
    ok(
        &home,
        "project.init",
        &[("project", t("shop/api")), ("folder", t(&folder_text))],
    );
    ok(
        &home,
        "branch.fork",
        &[
            ("project", t("shop/api")),
            ("name", t("dev")),
            ("from_branch", t("main")),
        ],
    );
    ok(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/api")),
            ("name", t("s1")),
            ("machine", t("native")),
        ],
    );
    let mounted = ok(
        &home,
        "workspace.mount",
        &[("sandbox", t("s1")), ("branch", t("dev"))],
    );
    let ws = mounted.get_text("ws").unwrap().to_owned();

    let failing = layerfs(
        &home,
        &[
            "workspace",
            "exec",
            &ws,
            "cat README.md; echo oops >&2; exit 3",
            "--stream",
        ],
    );
    let passing = layerfs(&home, &["workspace", "exec", &ws, "true", "--stream"]);
    let plain = layerfs(&home, &["workspace", "exec", &ws, "exit 4"]);

    ok(&home, "workspace.unmount", &[("ws", t(&ws))]);
    ok(&home, "sandbox.close", &[("name", t("s1"))]);
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);

    assert_eq!(failing.status.code(), Some(3), "{failing:?}");
    assert_eq!(String::from_utf8_lossy(&failing.stdout), "streamed\n");
    assert_eq!(String::from_utf8_lossy(&failing.stderr), "oops\n");
    assert_eq!(passing.status.code(), Some(0), "{passing:?}");
    // without --stream the command's code is in the JSON, and exit is 0
    assert_eq!(plain.status.code(), Some(0), "{plain:?}");
    assert!(String::from_utf8_lossy(&plain.stdout).contains("\"exit_code\":4"));
}
