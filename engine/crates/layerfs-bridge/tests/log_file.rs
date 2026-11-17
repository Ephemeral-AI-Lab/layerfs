//! Log rotation (layerfs-project#180): a log past its size moves once to
//! `<name>.1.<extension>`, replacing an older one. `rotate` renames a log
//! no process holds, before its writer opens it; `rotate_in_place` copies
//! and empties a log other writers hold open, so none of them is left
//! writing into the rotated file. The size is the caller's argument, so
//! these use a small one; production passes `ROTATE_AT`.
#![cfg(unix)]
use layerfs_bridge::log_file::{rotate, rotate_in_place, rotated, ROTATE_AT};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "layerfs-log-file-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn appender(path: &Path) -> fs::File {
    OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .unwrap()
}

fn text(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn the_size_is_16_mib_and_the_rotated_name_keeps_the_extension() {
    assert_eq!(ROTATE_AT, 16 * 1024 * 1024);
    for (name, to) in [
        (
            "/layerfs/data/log/worker.log",
            "/layerfs/data/log/worker.1.log",
        ),
        ("log/leases.jsonl", "log/leases.1.jsonl"),
        ("machine/vm/forward.log", "machine/vm/forward.1.log"),
        ("run/lfs-x/daemon.log", "run/lfs-x/daemon.1.log"),
        ("trace/calls.jsonl", "trace/calls.1.jsonl"),
        ("plain", "plain.1"),
    ] {
        assert_eq!(rotated(Path::new(name)), Path::new(to));
    }
}

#[test]
fn a_log_rotates_once_past_its_size_and_replaces_the_older_rotated_file() {
    let scratch = Scratch::new("rename");
    let log = scratch.0.join("worker.log");
    let old = scratch.0.join("worker.1.log");
    // no log yet: nothing to rotate, nothing made
    assert!(!rotate(&log, 8).unwrap());
    assert!(!old.exists());
    // at the size, not past it
    fs::write(&log, "12345678").unwrap();
    fs::write(&old, "the oldest\n").unwrap();
    assert!(!rotate(&log, 8).unwrap());
    assert_eq!(text(&log), "12345678");
    assert_eq!(text(&old), "the oldest\n");
    // past it: one rename, the older rotated file replaced
    fs::write(&log, "123456789").unwrap();
    assert!(rotate(&log, 8).unwrap());
    assert!(!log.exists());
    assert_eq!(text(&old), "123456789");
    // once: the next start finds no log past the size
    assert!(!rotate(&log, 8).unwrap());
    assert_eq!(text(&old), "123456789");
    let names: Vec<_> = fs::read_dir(&scratch.0).unwrap().flatten().collect();
    assert_eq!(names.len(), 1, "a log and one rotated file, never a .2");
}

#[test]
fn a_log_is_renamed_before_its_writer_opens_it_never_under_it() {
    let scratch = Scratch::new("at-open");
    let log = scratch.0.join("daemon.log");
    fs::write(&log, "the run before, past the size\n").unwrap();
    let before = fs::metadata(&log).unwrap().ino();
    // the launcher's order: rotate, then open the writer's descriptor
    assert!(rotate(&log, 8).unwrap());
    let mut writer = appender(&log);
    let opened = writer.metadata().unwrap().ino();
    assert_ne!(opened, before, "the writer holds a new file");
    // however much it writes, nothing renames the file it holds: a later
    // look at the size by anyone but a starting launcher is not a rotation
    writer.write_all(b"this run, well past the size\n").unwrap();
    assert_eq!(fs::metadata(&log).unwrap().ino(), opened);
    assert_eq!(text(&log), "this run, well past the size\n");
    assert_eq!(
        text(&rotated(&log)),
        "the run before, past the size\n",
        "the run before is whole in the rotated file"
    );
}

#[test]
fn a_shared_log_rotates_in_place_under_the_writers_that_hold_it() {
    let scratch = Scratch::new("in-place");
    let log = scratch.0.join("forward.log");
    let old = scratch.0.join("forward.1.log");
    // no log, then one at the size: nothing happens
    assert!(!rotate_in_place(&log, 8).unwrap());
    let mut first = appender(&log);
    first.write_all(b"12345678").unwrap();
    assert!(!rotate_in_place(&log, 8).unwrap());
    assert!(!old.exists());
    // a forward that runs holds the log; a second one starts past the size
    first.write_all(b"\nfirst forward: a warning\n").unwrap();
    fs::write(&old, "the oldest\n").unwrap();
    let held = fs::metadata(&log).unwrap().ino();
    assert!(rotate_in_place(&log, 8).unwrap());
    assert_eq!(text(&old), "12345678\nfirst forward: a warning\n");
    assert_eq!(fs::metadata(&log).unwrap().len(), 0);
    assert_eq!(
        fs::metadata(&log).unwrap().ino(),
        held,
        "the log is emptied, never renamed"
    );
    // both writers, the one that ran and the one that starts, append to the
    // log itself, from its new end; neither writes into the rotated file
    let mut second = appender(&log);
    second.write_all(b"second forward: up\n").unwrap();
    first.write_all(b"first forward: still here\n").unwrap();
    assert_eq!(
        text(&log),
        "second forward: up\nfirst forward: still here\n"
    );
    assert_eq!(text(&old), "12345678\nfirst forward: a warning\n");
    assert_eq!(first.metadata().unwrap().ino(), held);
}
