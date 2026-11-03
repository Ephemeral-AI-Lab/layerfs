//! A `layerfs` call closes the Stores it opened before the process ends:
//! a sqlite Store is left with no write-ahead log, so a later `store push`
//! is not refused for an unfinished one (#535 terminal pass, lane K2).
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const LAYERFS: &str = env!("CARGO_BIN_EXE_layerfs");

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-cli-closed-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn layerfs(home: &Path, words: &[&str]) {
    let out = Command::new(LAYERFS)
        .args(words)
        .env("LAYERFS_HOME", home)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "layerfs {words:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_cli_call_leaves_no_write_ahead_log() {
    let home = tempdir("home");
    let source = tempdir("source");
    std::fs::write(source.join("a.txt"), b"hi\n").unwrap();
    layerfs(&home, &["store", "create", "s"]);
    layerfs(&home, &["project", "init", "s/p", source.to_str().unwrap()]);
    layerfs(&home, &["branch", "fork", "s/p", "lane", "main"]);
    let store = home.join("store/s");
    assert!(store.join("store.sqlite").is_file());
    let wal = store.join("store.sqlite-wal");
    let left = std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0);
    assert_eq!(left, 0, "{} holds {left} bytes", wal.display());
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&source).unwrap();
}
