//! The `image.artifact` step `sweep` (spec v20): a machine's image cache
//! loses the unpack temporaries killed copies left of every image whose
//! copy lock in the home is free; one whose image a live copy holds (sent
//! in `locked`) and one whose unpack lock `<hex>.2.lock` is held stay. The
//! removal is a line of `log/sweep.jsonl` under the root. A copy sends
//! this step under the machine's prune gate (`image_load`, covered with a
//! real Docker by `lp176_image_export`).
use layerfs_protocol::provision::install::SdkValue;
use layerfs_sandbox::deploy::Side;
use layerfs_sandbox::image_artifact::{self as cache};
use std::fs;
use std::path::{Path, PathBuf};

fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-image-sweep-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// An unpack temporary of image `hex` in `cache`, holding one file.
fn temporary(cache: &Path, hex: &str) -> PathBuf {
    let at = cache.join(format!(".tmp-{hex}.2-ab12"));
    fs::create_dir_all(at.join("usr/bin")).unwrap();
    fs::write(at.join("usr/bin/tool"), b"tool").unwrap();
    at
}

#[test]
fn a_sweep_removes_free_unpacks_and_keeps_held_ones() {
    let root = root("step");
    let dir = cache::cache(&root);
    fs::create_dir_all(&dir).unwrap();
    let (free, copied, unpacking) = ("aa".repeat(32), "bb".repeat(32), "cc".repeat(32));
    let free_tmp = temporary(&dir, &free);
    let copied_tmp = temporary(&dir, &copied);
    let unpacking_tmp = temporary(&dir, &unpacking);
    // an unpack of that image runs: its lock is held
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(format!("{unpacking}.2.lock")))
        .unwrap();
    assert!(layerfs_home::home::flock::try_lock(&lock, true).unwrap());

    let args = SdkValue::map([
        ("step", SdkValue::text("sweep")),
        (
            "locked",
            SdkValue::List(vec![SdkValue::text(format!("sha256:{copied}"))]),
        ),
        ("within_ms", SdkValue::Int(2000)),
    ]);
    let swept = cache::step(&Side::Home(&root), &args).unwrap();
    assert_eq!(swept.get_int("removed"), Some(1), "{swept:?}");
    assert!(!free_tmp.exists());
    assert!(
        copied_tmp.join("usr/bin/tool").is_file(),
        "a live copy's image"
    );
    assert!(
        unpacking_tmp.join("usr/bin/tool").is_file(),
        "a running unpack"
    );
    let log = fs::read_to_string(root.join("log").join("sweep.jsonl")).unwrap();
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(log.contains(&free_tmp.display().to_string()) && log.contains("\"image_tmp\""));

    // once the unpack ended, the next sweep takes it
    layerfs_home::home::flock::unlock(&lock);
    let swept = cache::step(&Side::Worker(&root), &args).unwrap();
    assert_eq!(swept.get_int("removed"), Some(1), "{swept:?}");
    assert!(!unpacking_tmp.exists() && copied_tmp.is_dir());
    let _ = fs::remove_dir_all(root);
}
