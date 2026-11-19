//! `log/*.jsonl` rotation (layerfs-project#180): a line is appended, and a
//! log then past 16 MiB rotates once to `<name>.1.jsonl` under a
//! non-blocking `flock` on `log/rotate.lock`, as the call log does under
//! `trace/calls.lock`. `log/uncertain.jsonl` never rotates (#179 D8). The
//! logs are made large as sparse files, so no test writes 16 MiB.
#![cfg(unix)]
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{flock, log_line, paths, records::UNCERTAIN};
use layerfs_home::logs::{self, ROTATE_AT};
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU32 = AtomicU32::new(0);

struct Home(PathBuf);

impl Home {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "layerfs-log-rotation-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn log(&self, name: &str) -> PathBuf {
        paths::log_dir(&self.0).join(name)
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn line(text: &str) -> SdkValue {
    SdkValue::map([("said", SdkValue::text(text))])
}

/// Makes `path` a log of `size` bytes without writing them.
fn sized(path: &Path, size: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .unwrap();
    file.set_len(size).unwrap();
}

fn size(path: &Path) -> u64 {
    fs::metadata(path).unwrap().len()
}

#[test]
fn every_jsonl_log_rotates_once_past_its_size_and_replaces_the_older_one() {
    let home = Home::new("each");
    // the home's own logs, and a worker root's exec.jsonl (the same writer)
    for name in ["leases", "sweep", "exec"] {
        let current = home.log(&format!("{name}.jsonl"));
        let rotated = home.log(&format!("{name}.1.jsonl"));
        // small: appended to, never rotated
        log_line(&home.0, &format!("{name}.jsonl"), &line("first")).unwrap();
        assert_eq!(
            fs::read_to_string(&current).unwrap(),
            "{\"said\":\"first\"}\n"
        );
        assert!(!rotated.exists(), "{name}");
        // exactly at the size after an append is not past it
        fs::write(&rotated, "the oldest\n").unwrap();
        let line_len = size(&current);
        sized(&current, ROTATE_AT - line_len);
        log_line(&home.0, &format!("{name}.jsonl"), &line("first")).unwrap();
        assert_eq!(size(&current), ROTATE_AT, "{name}");
        assert_eq!(fs::read_to_string(&rotated).unwrap(), "the oldest\n");
        // one more line takes it past: the whole log moves, the line with
        // it, and the older rotated file is replaced
        log_line(&home.0, &format!("{name}.jsonl"), &line("first")).unwrap();
        assert!(!current.exists(), "{name}");
        assert_eq!(size(&rotated), ROTATE_AT + line_len, "{name}");
        // once: the next line starts a new log and leaves the rotated one
        log_line(&home.0, &format!("{name}.jsonl"), &line("next")).unwrap();
        assert_eq!(
            fs::read_to_string(&current).unwrap(),
            "{\"said\":\"next\"}\n"
        );
        assert_eq!(size(&rotated), ROTATE_AT + line_len, "{name}");
    }
    let lock = paths::log_rotate_lock(&home.0);
    assert_eq!(lock, home.log("rotate.lock"));
    assert_eq!(
        fs::metadata(&lock).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let names: Vec<String> = fs::read_dir(paths::log_dir(&home.0))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(
        names.len(),
        7,
        "three logs, three .1 and the lock: {names:?}"
    );
}

#[test]
fn the_uncertain_log_never_rotates_however_large() {
    let home = Home::new("uncertain");
    assert_eq!(UNCERTAIN, "uncertain.jsonl");
    let current = home.log(UNCERTAIN);
    sized(&current, 3 * ROTATE_AT);
    for _ in 0..2 {
        log_line(&home.0, UNCERTAIN, &line("an uncertain outcome")).unwrap();
    }
    let lines = 2 * "{\"said\":\"an uncertain outcome\"}\n".len() as u64;
    assert_eq!(size(&current), 3 * ROTATE_AT + lines);
    assert!(!home.log("uncertain.1.jsonl").exists());
    assert!(
        !paths::log_rotate_lock(&home.0).exists(),
        "it never asks for the rotation lock"
    );
}

#[test]
fn a_held_rotation_lock_skips_the_rotation_without_waiting_or_failing() {
    let home = Home::new("held");
    let current = home.log("leases.jsonl");
    sized(&current, ROTATE_AT);
    let lock = paths::log_rotate_lock(&home.0);
    let holder = File::create(&lock).unwrap();
    assert!(flock::try_lock(&holder, true).unwrap());
    let started = Instant::now();
    log_line(&home.0, "leases.jsonl", &line("appended")).unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the append did not wait for the lock"
    );
    // the line is in the log, which stayed where it was
    assert!(size(&current) > ROTATE_AT);
    assert!(!home.log("leases.1.jsonl").exists());
    // the next writer that gets the lock rotates it
    flock::unlock(&holder);
    log_line(&home.0, "leases.jsonl", &line("appended")).unwrap();
    assert!(!current.exists());
    assert!(size(&home.log("leases.1.jsonl")) > ROTATE_AT);
}

#[test]
fn the_call_log_still_rotates_under_its_own_lock() {
    let home = Home::new("calls");
    // a home of this build's layout: an older one gets no call log
    layerfs_home::home::ensure(&home.0).unwrap();
    logs::append(&home.0, &line("a call"));
    let current = paths::calls_file(&home.0);
    let rotated = paths::calls_rotated(&home.0);
    assert!(current.is_file() && !rotated.exists());
    fs::write(&rotated, "the oldest\n").unwrap();
    sized(&current, ROTATE_AT);
    logs::append(&home.0, &line("a call"));
    assert!(!current.exists());
    assert!(size(&rotated) > ROTATE_AT);
    assert!(paths::calls_lock(&home.0).is_file());
    assert!(
        !paths::log_rotate_lock(&home.0).exists(),
        "the call log keeps its own lock"
    );
}

#[test]
fn the_worker_log_is_in_the_log_folder_of_its_root() {
    let root = Path::new("/layerfs/data");
    assert_eq!(
        paths::worker_log(root),
        Path::new("/layerfs/data/log/worker.log")
    );
    assert_eq!(
        layerfs_bridge::log_file::rotated(&paths::worker_log(root)),
        Path::new("/layerfs/data/log/worker.1.log")
    );
    assert_eq!(
        paths::worker_log(root).parent(),
        Some(&*paths::log_dir(root))
    );
}
