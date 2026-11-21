//! `project prune` on the command line: it removes only a home entry with
//! no record, refuses an entry on record and a Store the home does not
//! record, and its help lists it.
use layerfs_api::home::{self, paths, private_dir};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LAYERFS: &str = env!("CARGO_BIN_EXE_layerfs");

fn home_path(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-cli-prune-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn layerfs(home: &Path, words: &[&str]) -> Output {
    Command::new(LAYERFS)
        .args(words)
        .env("LAYERFS_HOME", home)
        .output()
        .unwrap()
}

#[test]
fn project_prune_removes_only_a_folder_with_no_record() {
    let home = home_path("prune");
    home::open(&home).unwrap();
    let created = layerfs(&home, &["store", "create", "shop"]);
    assert_eq!(created.status.code(), Some(0), "{created:?}");
    // a real folder with no record is the leftover `prune` removes
    let orphan = paths::project_dir(&home, "shop", "orphan");
    private_dir(&orphan).unwrap();
    std::fs::write(orphan.join("sealed.sqlite"), b"leftover").unwrap();
    let out = layerfs(&home, &["project", "prune", "shop/orphan"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "{\"ok\":true}\n");
    assert!(!orphan.exists(), "the entry is gone");
    // an entry on record is not a leftover: `project delete` retires it
    let held = paths::project_dir(&home, "shop", "held");
    private_dir(&held).unwrap();
    std::fs::write(held.join("record.json"), "{}").unwrap();
    let out = layerfs(&home, &["project", "prune", "shop/held"]);
    assert_eq!(out.status.code(), Some(125), "{out:?}");
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("\"error\":\"Exists\""), "{err}");
    assert!(err.contains("project delete shop/held"), "{err}");
    assert!(held.exists(), "a recorded entry is left alone");
    // a Store the home does not record is not this call's to prune
    let out = layerfs(&home, &["project", "prune", "nope/x"]);
    assert_eq!(out.status.code(), Some(125), "{out:?}");
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("no Store named nope"), "{err}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn project_prune_is_listed_in_help() {
    let home = home_path("help");
    home::open(&home).unwrap();
    let family = String::from_utf8(layerfs(&home, &["help", "project"]).stdout).unwrap();
    assert!(
        family.lines().any(|line| line.contains("prune")),
        "{family}"
    );
    let call = layerfs(&home, &["help", "project", "prune"]);
    assert_eq!(call.status.code(), Some(0), "{call:?}");
    let text = String::from_utf8(call.stdout).unwrap();
    assert!(
        text.starts_with("layerfs project prune <project>\n"),
        "{text}"
    );
    std::fs::remove_dir_all(home).unwrap();
}
