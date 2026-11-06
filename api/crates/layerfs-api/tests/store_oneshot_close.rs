//! #86 regression: a one-shot process (the `layerfs` CLI) closes the Stores
//! its call opened before it ends, folding the write-ahead log in, even
//! while that call's idle checkpoint is still running. Its own test binary:
//! `close_idle` acts on every Store this process holds.
use layerfs_api::home::{self};
use layerfs_bridge::control::SdkValue;
use layerfs_control::common::lease;
use layerfs_home::home::paths;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-ckpt-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn ok(home: &Path, name: &str, args: &[(&str, &str)]) -> SdkValue {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), SdkValue::text(*v)))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {})
        .unwrap_or_else(|r| panic!("{name}: {r:?}"))
}

fn wal_bytes(file: &Path) -> u64 {
    let mut wal = file.as_os_str().to_os_string();
    wal.push("-wal");
    std::fs::metadata(wal).map_or(0, |m| m.len())
}

/// A one-shot process (the `layerfs` CLI) ends each call with
/// `close_idle(ZERO)`: the Store its call wrote must close then, with its
/// write-ahead log folded in, even while that call's idle checkpoint is
/// still running. Init, fork, then the process-level close, as before a
/// `store push`, which refuses a Store with a log left.
#[test]
fn a_one_shot_process_close_folds_the_log_after_a_fork() {
    let home = tempdir("oneshot");
    let folder = tempdir("oneshot-empty");
    home::open(&home).unwrap();
    ok(&home, "store.create", &[("name", "lane")]);
    let folder_text = folder.display().to_string();
    ok(
        &home,
        "project.init",
        &[("project", "lane/p"), ("folder", &folder_text)],
    );
    assert_eq!(layerfs_control::store::close_idle(Duration::ZERO), 0);
    ok(
        &home,
        "branch.fork",
        &[
            ("project", "lane/p"),
            ("name", "lane"),
            ("from_branch", "main"),
        ],
    );
    // the CLI's close right after the call: no wait, nothing else in between
    assert_eq!(layerfs_control::store::close_idle(Duration::ZERO), 1);
    let file = paths::store_file(&home, "lane");
    let mut wal = file.as_os_str().to_os_string();
    wal.push("-wal");
    assert!(
        !Path::new(&wal).exists(),
        "a write-ahead log is left: {} B",
        wal_bytes(&file)
    );
    assert_eq!(lease::holders(&home, "lane").unwrap(), Vec::<u32>::new());
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
}
