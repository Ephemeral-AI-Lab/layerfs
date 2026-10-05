#![cfg(feature = "native")]
use layerfs_bridge::adapters::native::pipe::Pipe;
use std::{
    io::{ErrorKind, Read, Write},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn cancellation(test: &str, reading: bool) {
    if std::env::var("LAYERFS_PIPE_CANCEL_TEST").as_deref() == Ok(test) {
        let (reader, writer) = nix::unistd::pipe().unwrap();
        let cancelled = AtomicBool::new(true);
        let mut pipe = Pipe {
            fd: if reading { &reader } else { &writer },
            deadline: Instant::now() + Duration::from_millis(50),
            cancel: &cancelled,
        };
        let result = if reading {
            pipe.read_exact(&mut [0])
        } else {
            pipe.write_all(&[0])
        };
        assert_eq!(result.unwrap_err().kind(), ErrorKind::ConnectionAborted);
        return;
    }
    // A regression returns Interrupted forever: the standard helpers retry it
    // even after the Pipe deadline. Keep that failure inside a reaped child.
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("LAYERFS_PIPE_CANCEL_TEST", test)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut expired = false;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            expired = true;
            child.kill().unwrap();
            break;
        }
        std::thread::park_timeout(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        !expired && output.status.success(),
        "{test}: watchdog_expired={expired}; status={}; stdout={}; stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn cancelled_read_exact_terminates() {
    cancellation("cancelled_read_exact_terminates", true);
}

#[test]
fn cancelled_write_all_terminates() {
    cancellation("cancelled_write_all_terminates", false);
}
