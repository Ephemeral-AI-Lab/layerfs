//! Bounded owner-only Docker proof lifecycle. No product retry or readiness loop.
use std::{
    io::{BufRead, BufReader},
    net::SocketAddr,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const IMAGE: &str = "sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6";
fn docker(args: &[&str]) -> std::process::Output {
    Command::new("perl")
        .args(["-e", "alarm shift; exec @ARGV", "5", "docker"])
        .args(args)
        .output()
        .unwrap()
}
fn checked(args: &[&str]) -> String {
    let output = docker(args);
    assert!(
        output.status.success(),
        "docker {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
pub struct Daemon {
    name: String,
    volume: String,
    child: Child,
    lines: Receiver<String>,
    reader: Option<JoinHandle<()>>,
    output: String,
    finished: bool,
}
impl Daemon {
    pub fn start() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let name = format!("layerfs-pre-s8-install-{unique}");
        let volume = format!("{name}-store");
        let binary = std::env::var("LAYERFS_INSTALL_DAEMON_BINARY")
            .expect("explicit build-listed Linux binary");
        assert!(binary.starts_with("/work/core/target/cluster2-linux/debug/deps/host_handoff-"));
        let absent = docker(&["volume", "inspect", &volume]);
        assert!(
            !absent.status.success()
                && String::from_utf8_lossy(&absent.stderr).contains("no such volume"),
            "volume precondition: {absent:?}"
        );
        checked(&["volume", "create", &volume]);
        let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap();
        let repository = format!("{}:/work:ro", repository.display());
        let mount = format!("{volume}:/store");
        let mut child = Command::new("docker")
            .args([
                "run",
                "--rm",
                "--name",
                &name,
                "-e",
                "LAYERFS_CONSTRUCTION_WORKERS=1",
                "-e",
                "LAYERFS_INSTALL_CHILD=1",
                "-v",
                &repository,
                "-v",
                &mount,
                "-p",
                "127.0.0.1::43210",
                "-w",
                "/work",
                IMAGE,
                "timeout",
                "--kill-after=1s",
                "8s",
                &binary,
                "--ignored",
                "--exact",
                "linux_daemon_install_role",
                "--nocapture",
                "--test-threads=1",
            ])
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
                assert!(total <= 65536, "bounded child output");
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        let mut owned = Self {
            name,
            volume,
            child,
            lines,
            reader: Some(reader),
            output: String::new(),
            finished: false,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let line = owned
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("original child listen event");
            let ready = line.contains("LAYERFS_INSTALL_LISTEN");
            owned.output.push_str(&line);
            owned.output.push('\n');
            if ready {
                break;
            }
        }
        println!(
            "HOST_HANDOFF_DOCKER image={IMAGE} container={} volume={}",
            owned.name, owned.volume
        );
        owned
    }
    pub fn address(&self) -> SocketAddr {
        checked(&["port", &self.name, "43210/tcp"])
            .trim()
            .parse()
            .unwrap()
    }
    pub fn finish(&mut self) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) => {
                    self.output.push_str(&line);
                    self.output.push('\n');
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(error) => panic!("bounded child completion: {error}"),
            }
        }
        self.reader.take().unwrap().join().unwrap();
        let status = self.child.wait().unwrap();
        assert!(status.success(), "daemon {status}: {}", self.output);
        self.finished = true;
        checked(&["volume", "rm", &self.volume]);
        println!("{}", self.output);
        println!(
            "HOST_HANDOFF_CLEANUP child_exit=0 container_auto_removed=true volume_removed=true"
        );
        self.output.clone()
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        if !self.finished {
            let stopped = docker(&["stop", "--time", "1", &self.name]);
            eprintln!(
                "owned container stop: {:?} {}",
                stopped.status,
                String::from_utf8_lossy(&stopped.stderr)
            );
            let _ = self.child.wait();
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
            eprintln!("failed handoff retained volume {}", self.volume);
        }
    }
}
