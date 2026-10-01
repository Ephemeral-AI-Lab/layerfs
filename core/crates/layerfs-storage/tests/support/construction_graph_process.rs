//! Owned process/kernel SHARED-lock coordination with acknowledged reaping.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Child, Output, Stdio};
use std::time::{Duration, Instant};

use super::support::TempDir;

pub struct HeldReader {
    child: Option<Child>,
    control: Option<UnixStream>,
    _directory: TempDir,
}

impl HeldReader {
    pub fn new(path: &Path, records: u64, stage: u8) -> Self {
        let directory = TempDir::new("lfcs4_ipc");
        let socket = directory.join("r");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "unknown::reader_child", "--nocapture"])
            .env("LAYERFS_GRAPH_READ_DB", path)
            .env("LAYERFS_GRAPH_READ_SOCKET", &socket)
            .env("LAYERFS_GRAPH_READ_RECORDS", records.to_string())
            .env("LAYERFS_GRAPH_READ_STAGE", stage.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut owner = Self {
            child: Some(child),
            control: None,
            _directory: directory,
        };
        let start = Instant::now();
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        owner.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                        "reader exited before IPC"
                    );
                    assert!(start.elapsed() < Duration::from_secs(5), "reader IPC bound");
                    std::thread::yield_now();
                }
                Err(error) => panic!("reader IPC: {error}"),
            }
        };
        owner.control = Some(stream);
        let stream = owner.control.as_mut().unwrap();
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut ack = [0];
        stream.read_exact(&mut ack).unwrap();
        assert_eq!(ack, [1], "actual BEGIN+SELECT holds SHARED before writer");
        owner
    }

    pub fn release_and_reap(&mut self) -> Output {
        let stream = self.control.as_mut().unwrap();
        stream.write_all(&[2]).unwrap();
        let mut ack = [0];
        stream.read_exact(&mut ack).unwrap();
        assert_eq!(ack, [3], "reader ROLLBACK+connection close acknowledged");
        let child = self.child.take().unwrap();
        bounded_output(child)
    }
}

impl Drop for HeldReader {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if let Some(stream) = self.control.take() {
            if let Err(error) = stream.shutdown(std::net::Shutdown::Both) {
                eprintln!("LFCS4 IPC shutdown: {error}");
            }
        }
        if child.try_wait().ok().flatten().is_none() {
            if let Err(error) = child.kill() {
                eprintln!("LFCS4 reader kill: {error}");
            }
            if let Err(error) = child.wait() {
                eprintln!("LFCS4 reader reap: {error}");
            }
        }
    }
}

pub fn writer(path: &Path, phase: &str) -> Output {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "unknown::writer_child", "--nocapture"])
        .env("LAYERFS_GRAPH_UNKNOWN_BASE", path)
        .env("LAYERFS_GRAPH_UNKNOWN_PHASE", phase)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    bounded_output(child)
}

fn bounded_output(mut child: Child) -> Output {
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() >= Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("owned LFCS4 helper exceeded correctness coordination bound");
        }
        std::thread::yield_now();
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_end(&mut stderr)
        .unwrap();
    Output {
        status,
        stdout,
        stderr,
    }
}
