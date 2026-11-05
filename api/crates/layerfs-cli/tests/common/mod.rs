//! What the CLI tests share: a home, a run with a deadline, and standard
//! streams that are closed, full or whose reader is gone.
#![allow(dead_code)]
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const LAYERFS: &str = env!("CARGO_BIN_EXE_layerfs");

/// `O_NONBLOCK`, whose value differs between the supported platforms.
#[cfg(target_os = "linux")]
const O_NONBLOCK: i32 = 0o4000;
#[cfg(target_os = "macos")]
const O_NONBLOCK: i32 = 0x4;

/// A run that outlasts this is a hang, not a slow machine.
const DEADLINE: Duration = Duration::from_secs(60);

/// A fresh private folder, to be used as a home or a scratch directory.
pub fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

/// `layerfs <words>` on `home`, with stdin closed unless the test sets one.
pub fn layerfs<S: AsRef<std::ffi::OsStr>>(home: &Path, words: &[S]) -> Command {
    let mut command = Command::new(LAYERFS);
    command
        .args(words)
        .env("LAYERFS_HOME", home)
        .stdin(Stdio::null());
    command
}

/// Waits for `child` to end; kills it and fails once the deadline passes.
pub fn finish(mut child: Child) -> Output {
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let until = Instant::now() + DEADLINE;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("layerfs did not end within {DEADLINE:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    }
}

/// Reads `pipe` to its end on its own thread, so a large output never
/// fills the pipe and stops the child.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// Runs `command` with stdout and stderr captured.
pub fn run(command: &mut Command) -> Output {
    finish(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
}

/// Runs `command` with `stdin` written to it, then closed.
pub fn run_with_stdin(command: &mut Command, stdin: &[u8]) -> Output {
    use std::io::Write;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    finish(child)
}

/// A pipe whose reader has already gone: any write to it fails with
/// `EPIPE`, whenever the child writes. A fifo is opened for reading, then
/// for writing, and the reader is dropped before the child starts.
pub fn broken_pipe(tag: &str) -> Stdio {
    let dir = tempdir(&format!("fifo-{tag}"));
    let path = dir.join("pipe");
    assert!(Command::new("mkfifo")
        .arg(&path)
        .status()
        .unwrap()
        .success());
    // O_NONBLOCK, so the open for reading does not wait for a writer
    let reader = OpenOptions::new()
        .read(true)
        .custom_flags(O_NONBLOCK)
        .open(&path)
        .unwrap();
    let writer = OpenOptions::new().write(true).open(&path).unwrap();
    drop(reader);
    std::fs::remove_dir_all(dir).unwrap();
    Stdio::from(writer)
}

/// A device every write to which fails with `ENOSPC` (Linux has `/dev/full`).
#[cfg(target_os = "linux")]
fn full() -> Stdio {
    Stdio::from(OpenOptions::new().write(true).open("/dev/full").unwrap())
}

/// The standard-stream sinks every write to which fails, by name: a pipe
/// whose reader has gone, and on Linux a full device.
pub fn failing_sinks(tag: &str) -> Vec<(&'static str, Stdio)> {
    let mut sinks = vec![("broken pipe", broken_pipe(tag))];
    #[cfg(target_os = "linux")]
    sinks.push(("/dev/full", full()));
    sinks
}

pub fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
