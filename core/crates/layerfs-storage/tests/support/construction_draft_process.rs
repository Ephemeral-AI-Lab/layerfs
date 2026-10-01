//! One actual selected executable/provider reader in a separate process.
use super::support::TempDir;
use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
    process::{Child, Output, Stdio},
    time::{Duration, Instant},
};
pub struct HeldDraftReader {
    child: Option<Child>,
    control: Option<UnixStream>,
    _directory: TempDir,
}
impl HeldDraftReader {
    pub fn new(path: &Path, phase: &str) -> Self {
        let directory = TempDir::new("lfd6_ipc");
        let socket = directory.join("r");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "unix::draft_reader_child", "--nocapture"])
            .env(
                "LAYERFS_DRAFT_READER_DB",
                std::fs::canonicalize(path).unwrap(),
            )
            .env("LAYERFS_DRAFT_READER_SOCKET", socket)
            .env("LAYERFS_DRAFT_READER_PHASE", phase)
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
                        "reader exited before actual barrier"
                    );
                    assert!(
                        start.elapsed() < Duration::from_secs(5),
                        "reader acknowledgement bound"
                    );
                    std::thread::yield_now();
                }
                Err(error) => panic!("reader IPC: {error}"),
            }
        };
        owner.control = Some(stream);
        let control = owner.control.as_mut().unwrap();
        control.set_nonblocking(false).unwrap();
        control
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        control
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut ack = [0];
        control.read_exact(&mut ack).unwrap();
        assert_eq!(
            ack,
            [1],
            "actual BEGIN+SELECT snapshot holds SHARED before mutation"
        );
        owner
    }
    pub fn release_and_reap(&mut self) -> Output {
        let control = self.control.as_mut().unwrap();
        control.write_all(&[2]).unwrap();
        let mut ack = [0];
        control.read_exact(&mut ack).unwrap();
        assert_eq!(
            ack,
            [3],
            "actual ROLLBACK and selected connection close acknowledged"
        );
        let mut child = self.child.take().unwrap();
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if start.elapsed() >= Duration::from_secs(5) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("reader reap bound");
            }
            std::thread::yield_now();
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        child
            .stdout
            .take()
            .unwrap()
            .take(16385)
            .read_to_end(&mut stdout)
            .unwrap();
        child
            .stderr
            .take()
            .unwrap()
            .take(16385)
            .read_to_end(&mut stderr)
            .unwrap();
        assert!(
            stdout.len() <= 16384 && stderr.len() <= 16384,
            "bounded child output"
        );
        Output {
            status,
            stdout,
            stderr,
        }
    }
}
impl Drop for HeldDraftReader {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if let Some(control) = self.control.take() {
                let _ = control.shutdown(std::net::Shutdown::Both);
            }
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}
