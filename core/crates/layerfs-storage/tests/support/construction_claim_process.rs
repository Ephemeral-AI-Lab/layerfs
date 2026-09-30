//! Owned process/kernel coordination for actual private SQLite lock failures.

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
    pub fn new(path: &Path, expected_records: u64) -> Self {
        let directory = TempDir::new("lfcs2_ipc");
        let socket = directory.join("r");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "unix::shared_reader_child", "--nocapture"])
            .env("LAYERFS_CLAIMS_READ_DB", path)
            .env("LAYERFS_CLAIMS_READ_SOCKET", &socket)
            .env(
                "LAYERFS_CLAIMS_EXPECTED_RECORDS",
                expected_records.to_string(),
            )
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
        let mut acknowledgement = [0];
        stream.read_exact(&mut acknowledgement).unwrap();
        assert_eq!(
            acknowledgement,
            [1],
            "acknowledged BEGIN+SELECT holds SHARED"
        );
        owner
    }

    pub fn release_and_reap(&mut self) -> Output {
        let stream = self.control.as_mut().unwrap();
        stream.write_all(&[2]).unwrap();
        let mut acknowledgement = [0];
        stream.read_exact(&mut acknowledgement).unwrap();
        assert_eq!(
            acknowledgement,
            [3],
            "ROLLBACK and connection close acknowledged"
        );
        let child = self.child.as_mut().unwrap();
        let status = child.wait().unwrap();
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
        self.child.take();
        Output {
            status,
            stdout,
            stderr,
        }
    }
}

impl Drop for HeldReader {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if let Some(stream) = self.control.take() {
            if let Err(error) = stream.shutdown(std::net::Shutdown::Both) {
                eprintln!("LFCS2 IPC shutdown: {error}");
            }
        }
        match child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => {
                if let Err(error) = child.kill() {
                    eprintln!("LFCS2 reader kill: {error}");
                }
                if let Err(error) = child.wait() {
                    eprintln!("LFCS2 reader reap: {error}");
                }
            }
        }
    }
}
