//! Bounded ownership of an actual daemon subprocess in this proof's kernel.
use super::support;
use layerfs_bridge::native;
use layerfs_sdk::control::Control;
use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub struct Daemon {
    child: Child,
    lines: Receiver<String>,
    reader: Option<JoinHandle<()>>,
    output: String,
    finished: bool,
}
impl Daemon {
    pub fn start(role: &str, manifest: &Path, overlay: &Path) -> (Self, Control) {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "daemon_process_role",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("LAYERFS_SHARING_ROLE", role)
            .env("LAYERFS_SHARING_MANIFEST", manifest)
            .env("LAYERFS_SHARING_OVERLAY", overlay)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, lines) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut total = 0;
            for line in BufReader::new(stdout).lines() {
                let line = line.unwrap();
                total += line.len();
                assert!(total <= 131072, "bounded child output");
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        let mut owned = Self {
            child,
            lines,
            reader: Some(reader),
            output: String::new(),
            finished: false,
        };
        let ready = owned.event("SHARING_READY");
        let address: SocketAddr = ready
            .split("address=")
            .nth(1)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let socket = TcpStream::connect_timeout(&address, Duration::from_secs(3)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let connection = native::initiate(
            socket,
            &support::CLIENT_PRIVATE,
            native::public_key(&support::SERVER_PRIVATE).unwrap(),
        )
        .unwrap();
        (owned, Control::new(connection))
    }
    pub fn event(&mut self, wanted: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let line = self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|e| panic!("child event {wanted}: {e}; {}", self.output));
            self.output.push_str(&line);
            self.output.push('\n');
            if line.contains(wanted) {
                return line;
            }
        }
    }
    pub fn release(&mut self) {
        self.child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"continue\n")
            .unwrap();
    }
    pub fn kill(&mut self) {
        self.child.kill().unwrap();
        let status = self.child.wait().unwrap();
        assert!(!status.success());
        self.finished = true;
        println!("SHARING_KILLED pid={} status={status}", self.child.id());
        self.drain();
    }
    pub fn finish(&mut self) {
        let status = self.child.wait().unwrap();
        self.finished = true;
        self.drain();
        assert!(status.success(), "daemon {status}: {}", self.output);
        println!("SHARING_EXIT pid={} status={status}", self.child.id());
    }
    fn drain(&mut self) {
        if let Some(reader) = self.reader.take() {
            reader.join().unwrap();
        }
        for line in self.lines.try_iter() {
            self.output.push_str(&line);
            self.output.push('\n');
        }
        println!("{}", self.output);
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        if !self.finished {
            if matches!(self.child.try_wait(), Ok(None)) {
                let _ = self.child.kill();
            }
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
