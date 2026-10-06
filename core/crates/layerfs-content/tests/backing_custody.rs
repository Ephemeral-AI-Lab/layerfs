//! File-backing custody: partial appends stay charged and a failed release is final.
//!
//! Exercised through the public backing API only. The account must describe what
//! is on disk after a failure, keep the deciding host cause, and never hand a
//! retained path's bytes back because another path was dropped.
#![cfg(unix)]

mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};
use layerfs_content::ContentError;
use support::filesystem::TempDir;

/// Marks the re-executed process that runs under the file-size limit.
const LIMITED: &str = "LAYERFS_BACKING_CUSTODY_LIMITED";
const APPEND_BYTES: usize = 65_536;

fn run_files(directory: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(directory)
        .expect("listing")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "run"))
        .collect();
    files.sort();
    files
}

/// Body of the limited process: the host accepts only the first part of one append.
fn partial_append_under_the_file_size_limit() {
    let dir = TempDir::new("backing-partial");
    let mut backing = FileBacking::with_capacity(dir.path(), u64::MAX);
    let mut run = backing.create_run().expect("run");
    let error = run
        .append(&vec![42; APPEND_BYTES])
        .expect_err("the file-size limit stops the append");
    assert_eq!(error, ContentError::Io);
    let files = run_files(dir.path());
    assert_eq!(files.len(), 1);
    let on_disk = std::fs::metadata(&files[0]).expect("run file").len();
    assert!(
        on_disk > 0 && on_disk < APPEND_BYTES as u64,
        "the limit must admit a strict prefix, found {on_disk}"
    );
    assert_eq!(
        backing.held_bytes(),
        on_disk,
        "bytes that reached the file stay charged after the failed append"
    );
    assert_eq!(backing.peak_bytes(), APPEND_BYTES as u64);
    assert_eq!(run.len(), 0, "a failed append adds no logical rows");
    assert!(backing.owns_storage());

    // The run ends in a partial record, so it takes nothing further and the
    // refused append neither reserves nor writes.
    assert_eq!(run.append(b"later"), Err(ContentError::Io));
    assert_eq!(backing.held_bytes(), on_disk);
    assert_eq!(
        std::fs::metadata(&files[0]).expect("run file").len(),
        on_disk
    );

    drop(run);
    assert_eq!(
        backing.held_bytes(),
        0,
        "removing the file returns its bytes"
    );
    assert!(!backing.owns_storage());
    backing.release().expect("nothing is left to release");
    assert!(!backing.cleanup_failed());
    println!("BACKING_PARTIAL_APPEND on_disk={on_disk} requested={APPEND_BYTES}");
}

/// A host file-size limit is process state, so the body runs in a re-executed
/// copy of this test binary with the limit applied by the shell that starts it.
/// The limit signal is ignored there so the write reports its error instead.
#[test]
fn a_failed_partial_append_keeps_its_physical_bytes_charged() {
    if std::env::var_os(LIMITED).is_some() {
        partial_append_under_the_file_size_limit();
        return;
    }
    let binary = std::env::current_exe().expect("test binary");
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("trap '' XFSZ; ulimit -f 8 || exit 97; exec \"$0\" \"$@\"")
        .arg(&binary)
        .args([
            "--exact",
            "a_failed_partial_append_keeps_its_physical_bytes_charged",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(LIMITED, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("limited process");
    // Bounded wait: a limited process that has not finished is stopped and failed.
    let deadline = Instant::now() + Duration::from_secs(30);
    let finished = loop {
        if child.try_wait().expect("child status").is_some() {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if !finished {
        let _ = child.kill();
    }
    let output = child.wait_with_output().expect("limited output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        finished,
        "limited process exceeded its wait\n{stdout}\n{stderr}"
    );
    assert!(
        output.status.success(),
        "limited process failed: {:?}\n{stdout}\n{stderr}",
        output.status
    );
    assert!(
        stdout.contains("BACKING_PARTIAL_APPEND on_disk="),
        "the limited body did not run\n{stdout}\n{stderr}"
    );
    print!("{stdout}");
}

#[test]
fn a_failed_release_is_final_and_keeps_its_cause_and_bytes() {
    let dir = TempDir::new("backing-final-release");
    let mut backing = FileBacking::new(dir.path());
    let mut kept = backing.create_run().expect("first run");
    kept.append(&[0x5a; 96]).expect("append");
    kept.flush().expect("flush");
    let mut removed = backing.create_run().expect("second run");
    removed.append(&[0x3c; 32]).expect("append");
    removed.flush().expect("flush");
    assert_eq!(backing.held_bytes(), 128);
    assert_eq!(backing.owned_runs(), 2);
    assert!(backing.cleanup_failure().is_none());

    // The first run's path becomes a directory, so the host refuses to unlink it.
    let files = run_files(dir.path());
    let (kept_path, removed_path) = (files[0].clone(), files[1].clone());
    std::fs::remove_file(&kept_path).expect("the run file is there");
    std::fs::create_dir(&kept_path).expect("a directory takes its place");
    let host = std::fs::remove_file(&kept_path).expect_err("the host refuses");

    let error = backing.release().expect_err("one removal fails");
    assert_eq!(
        error,
        ContentError::ResourceUnavailable {
            what: "ordering run cleanup"
        }
    );
    let failure = backing.cleanup_failure().expect("the deciding cause");
    assert_eq!(failure.path, kept_path);
    assert_eq!(failure.bytes, 96);
    assert_eq!(failure.kind, host.kind());
    assert_eq!(failure.os_error, host.raw_os_error());
    assert_eq!(failure.to_io_error().raw_os_error(), host.raw_os_error());
    assert!(!removed_path.exists(), "the removable run is gone");
    assert_eq!(backing.owned_runs(), 1);
    assert_eq!(
        backing.held_bytes(),
        96,
        "only the retained path is charged"
    );

    // Dropping the run whose file is already gone must not return the retained
    // path's bytes, and dropping the retained run changes nothing.
    drop(removed);
    assert_eq!(backing.held_bytes(), 96);
    drop(kept);
    assert_eq!(backing.held_bytes(), 96);
    assert_eq!(backing.owned_runs(), 1);
    assert!(backing.cleanup_failed());

    // The path becomes removable again. Neither the run's destructor (above) nor
    // the backing's replays the failed release: the path is the caller's now.
    std::fs::remove_dir(&kept_path).expect("the directory is empty");
    std::fs::write(&kept_path, b"retained").expect("a plain file again");
    let first = backing.cleanup_failure();
    drop(backing);
    assert!(
        kept_path.exists(),
        "a destructor must not attempt the failed release again"
    );
    assert_eq!(first.expect("cause").path, kept_path);
}

#[test]
fn a_released_run_takes_no_further_rows() {
    let dir = TempDir::new("backing-released-run");
    let mut backing = FileBacking::new(dir.path());
    let mut run = backing.create_run().expect("run");
    run.append(&[1; 64]).expect("append");
    backing.release().expect("release");
    assert_eq!(backing.held_bytes(), 0);
    assert_eq!(
        run.append(&[2; 64]),
        Err(ContentError::ResourceUnavailable {
            what: "released ordering run"
        })
    );
    assert_eq!(backing.held_bytes(), 0, "a refused append reserves nothing");
    drop(run);
    assert_eq!(backing.held_bytes(), 0);
    assert!(!backing.owns_storage());
    assert!(!backing.cleanup_failed());
}
