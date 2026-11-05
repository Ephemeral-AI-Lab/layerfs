//! `layerfs sandbox scratch <machine> --dry-run` (#549): the operator pass
//! lists exactly the roots a real run deletes, each with its record, and
//! deletes nothing; the real run deletes the same set. A wrong option is a
//! command line error, exit 2, before any call.
use layerfs_api::home::{self, paths, write_record};
use layerfs_api::json;
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LAYERFS: &str = env!("CARGO_BIN_EXE_layerfs");

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-cli-scratch-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A settled Sandbox record owning its own scratch root: no id, no intent,
/// so an automatic pass may delete it.
fn scratch_record(home: &Path, name: &str) -> PathBuf {
    let root = home
        .join("machine/native/run")
        .join(format!("lfs-x-{name}"))
        .join("tmp");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("commands-left-this"), "scratch").unwrap();
    write_record(
        &paths::sandbox_record(home, name),
        &SdkValue::map([
            ("name", t(name)),
            ("project", t("shop/api")),
            ("machine", t("native")),
            ("created", SdkValue::Int(1)),
            (
                "scratch",
                SdkValue::map([
                    ("root", SdkValue::text(root.display().to_string())),
                    ("createdAt", SdkValue::Int(1)),
                    ("generation", SdkValue::Int(7)),
                ]),
            ),
        ]),
    )
    .unwrap();
    root
}

fn layerfs(home: &Path, words: &[&str]) -> Output {
    Command::new(LAYERFS)
        .args(words)
        .env("LAYERFS_HOME", home)
        .output()
        .unwrap()
}

/// `field` of the call's JSON answer as `(sandbox, root)` pairs.
fn pairs(stdout: &[u8], field: &str) -> Vec<(String, String)> {
    let value = json::parse(String::from_utf8_lossy(stdout).trim()).unwrap();
    let Some(SdkValue::List(items)) = value.get(field) else {
        panic!("{field} is not a list: {value:?}");
    };
    items
        .iter()
        .map(|item| {
            (
                item.get_text("sandbox").unwrap().to_owned(),
                item.get_text("root").unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn dry_run_lists_and_the_real_run_deletes_the_same_roots() {
    let home = tempdir("home");
    home::open(&home).unwrap();
    let root = scratch_record(&home, "b");
    let inside = root.join("commands-left-this");

    let dry = layerfs(&home, &["sandbox", "scratch", "native", "--dry-run"]);
    assert_eq!(dry.status.code(), Some(0), "{dry:?}");
    assert_eq!(
        pairs(&dry.stdout, "planned"),
        vec![("b".to_owned(), root.display().to_string())]
    );
    assert!(pairs(&dry.stdout, "deleted").is_empty());
    assert!(inside.is_file(), "a dry run deletes nothing");

    let real = layerfs(&home, &["sandbox", "scratch", "native"]);
    assert_eq!(real.status.code(), Some(0), "{real:?}");
    assert_eq!(
        pairs(&real.stdout, "deleted"),
        pairs(&dry.stdout, "planned"),
        "the real run deletes exactly the plan"
    );
    assert!(!root.exists(), "the real run deletes the planned root");

    // a wrong option is the command line's, not the library's: exit 2
    let wrong = layerfs(&home, &["sandbox", "scratch", "native", "--dryrun"]);
    assert_eq!(wrong.status.code(), Some(2), "{wrong:?}");
    let _ = std::fs::remove_dir_all(home);
}
