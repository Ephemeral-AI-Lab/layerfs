//! Live fixtures for the host entry. Use short canonical paths on both kernels.
#![allow(dead_code)]
use layerfs_api::worker::{
    read_host_frame, write_host_frame, HostPaths, HOST_MAX_REPLY, MAX_HELLO,
};
use layerfs_protocol::sdk::SdkValue;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const BINARY: &str = env!("CARGO_BIN_EXE_layerfs");
const IO: Duration = Duration::from_secs(7);

pub struct Fixture {
    pub paths: HostPaths,
    aliases: Vec<PathBuf>,
}

impl Fixture {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self::at(PathBuf::from(format!(
            "/tmp/lh819-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }

    /// A new home at `path` (a short path whose parent exists).
    pub fn at(path: PathBuf) -> Self {
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .unwrap();
        layerfs_api::call(Some(&path), "home.init", SdkValue::map([]), &mut |_| {}).unwrap();
        Self {
            paths: HostPaths::new(&path).unwrap(),
            aliases: Vec::new(),
        }
    }

    pub fn alias(&mut self) -> PathBuf {
        let alias = self.paths.home.with_extension("alias");
        std::os::unix::fs::symlink(&self.paths.home, &alias).unwrap();
        self.aliases.push(alias.clone());
        alias
    }

    pub fn start(&self) -> Worker {
        Self::spawn(&self.paths.home)
    }

    pub fn spawn(home: &Path) -> Worker {
        Worker {
            child: Command::new(BINARY)
                .args(["worker", "--host"])
                .env("LAYERFS_HOME", home)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        }
    }

    pub fn dial(&self, control: bool) -> Exchange {
        self.dial_at(if control {
            &self.paths.control
        } else {
            &self.paths.socket
        })
    }

    /// Dials the streamed write endpoint.
    pub fn dial_writes(&self) -> Exchange {
        self.dial_at(&self.paths.writes)
    }

    /// Reads the hello on `socket`, connected to an endpoint, by `wait`.
    pub fn greeted(&self, socket: UnixStream, wait: Duration) -> Exchange {
        let hello = read_host_frame(&socket, MAX_HELLO, Some(Instant::now() + wait))
            .unwrap()
            .expect("host hello");
        assert!(hello.get_bool("ready"));
        assert_eq!(hello.get_text("kind"), Some("host"));
        assert_eq!(hello.get_text("home_id"), Some(self.paths.home_id.as_str()));
        Exchange { socket, hello }
    }

    fn dial_at(&self, endpoint: &Path) -> Exchange {
        let deadline = Instant::now() + IO;
        let socket = loop {
            match UnixStream::connect(endpoint) {
                Ok(socket) => break socket,
                Err(error) if Instant::now() < deadline => {
                    if !matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    ) {
                        panic!("cannot dial {}: {error}", endpoint.display());
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{} was not ready: {error}", endpoint.display()),
            }
        };
        self.greeted(socket, deadline.saturating_duration_since(Instant::now()))
    }

    pub fn ping(&self) -> SdkValue {
        self.dial(true).request("ping").get("ok").unwrap().clone()
    }

    pub fn drain(&self) {
        let ack = self.dial(true).request("drain");
        assert_eq!(ack.get("ok").unwrap().get_text("state"), Some("draining"));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for alias in &self.aliases {
            let _ = std::fs::remove_file(alias);
        }
        let _ = std::fs::remove_dir_all(&self.paths.home);
    }
}

pub struct Worker {
    child: Child,
}

impl Worker {
    pub fn pid(&self) -> i64 {
        i64::from(self.child.id())
    }

    pub fn wait(&mut self, wait: Duration) -> ExitStatus {
        let deadline = Instant::now() + wait;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "host worker did not exit");
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    pub fn kill(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

pub struct Exchange {
    socket: UnixStream,
    pub hello: SdkValue,
}

impl Exchange {
    pub fn socket(&self) -> &UnixStream {
        &self.socket
    }

    pub fn request(self, op: &str) -> SdkValue {
        let request = SdkValue::map([
            ("op", SdkValue::text(op)),
            ("args", SdkValue::map([])),
            ("generation", self.hello.get("generation").unwrap().clone()),
            ("request_id", SdkValue::text("fixture-1")),
        ]);
        let deadline = Instant::now() + IO;
        write_host_frame(&self.socket, &request, HOST_MAX_REPLY, deadline).unwrap();
        let answer = read_host_frame(&self.socket, HOST_MAX_REPLY, Some(deadline))
            .unwrap()
            .expect("one answer");
        assert_eq!(answer.get_text("request_id"), Some("fixture-1"));
        answer
    }
}
