//! #86: when the last call on an open SQLite Store returns, its write-ahead
//! log is checkpointed and truncated while the Store stays open, so the
//! quiesced file is the Store's closed bytes. The checkpoint runs after the
//! call is answered; the wait below is bounded.
#![cfg(unix)] // unix stat/lock semantics; the Windows face is its own port

use layerfs_api::home::{self};

use layerfs_bridge::control::SdkValue;
use layerfs_control::common::home::{lease, paths};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

#[test]
fn the_last_call_leaves_an_open_store_with_an_empty_log() {
    let home = tempdir("home");
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "hello\n").unwrap();
    home::open(&home).unwrap();
    ok(&home, "store.create", &[("name", "shop")]);
    let folder_text = folder.display().to_string();
    ok(
        &home,
        "project.init",
        &[("project", "shop/api"), ("folder", &folder_text)],
    );
    for name in ["dev", "qa"] {
        ok(
            &home,
            "branch.fork",
            &[
                ("project", "shop/api"),
                ("name", name),
                ("from_branch", "main"),
            ],
        );
    }
    let file = paths::store_file(&home, "shop");
    // answered first, then checkpointed: a bounded wait for it
    let deadline = Instant::now() + Duration::from_secs(10);
    while wal_bytes(&file) > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(wal_bytes(&file), 0, "the log is truncated");
    let quiesced = std::fs::read(&file).unwrap();
    // the Store was still open in this process: the idle close closes it
    // now, and finds nothing to copy in, so the file is unchanged
    assert!(layerfs_control::store::close_idle(Duration::ZERO) >= 1);
    assert_eq!(lease::holders(&home, "shop").unwrap(), Vec::<u32>::new());
    assert_eq!(wal_bytes(&file), 0);
    assert_eq!(std::fs::read(&file).unwrap(), quiesced);
    // and it reopens with the forks
    let branches = ok(&home, "branch.list", &[("project", "shop/api")]);
    let text = format!("{branches:?}");
    assert!(text.contains("dev") && text.contains("qa"), "{text}");
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&folder).unwrap();
}
