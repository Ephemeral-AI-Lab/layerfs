//! A worker out of descriptors keeps serving (layerfs-project#177): `accept`
//! fails with `EMFILE`, the failure is logged once, and the next request is
//! answered once descriptors are free again.
//!
//! The limit is a process's, so the worker here is a process of its own:
//! this test binary run again under `sh`'s `ulimit -n` (`setrlimit`), at
//! the entry [`the_worker_process`]. The same can be done to a machine's
//! real worker: `prlimit --nofile=64:64 --pid <its pid>`, then more idle
//! connections than that to its port.
use layerfs_machine::worker::{self, Endpoint};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Names the folder a worker process serves; set only by the test below.
const SERVES: &str = "LAYERFS_TEST_WORKER_PROCESS";
/// The descriptors the worker process may hold.
const DESCRIPTORS: usize = 48;

/// Not a test of its own: the worker process of the test below. It does
/// nothing unless [`SERVES`] names a folder; then it writes its address
/// there and serves until it is killed.
#[test]
fn the_worker_process() {
    let Some(dir) = std::env::var_os(SERVES).map(PathBuf::from) else {
        return;
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    std::fs::write(dir.join("address.new"), address).unwrap();
    std::fs::rename(dir.join("address.new"), dir.join("address")).unwrap();
    let stopped = layerfs_control::worker_server::serve(listener, "k".to_owned(), dir.join("data"));
    panic!("the worker stopped: {stopped:?}");
}

fn wait_for<T>(within: Duration, what: &str, mut ready: impl FnMut() -> Option<T>) -> T {
    let started = Instant::now();
    loop {
        if let Some(value) = ready() {
            return value;
        }
        assert!(started.elapsed() < within, "{what}: not within {within:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Ends the worker process on every way out of the test.
struct Ended(std::process::Child);

impl Drop for Ended {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn accept_lines(log: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    (text.lines())
        .filter(|line| line.starts_with("layerfs worker: accept failed"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_worker_out_of_descriptors_keeps_serving() {
    let dir = std::env::temp_dir().join(format!("layerfs-fd-limit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("worker.log");
    let child = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "ulimit -n {DESCRIPTORS} && exec \"$0\" --exact the_worker_process --nocapture --test-threads 1"
        ))
        .arg(std::env::current_exe().unwrap())
        .env(SERVES, &dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&log).unwrap())
        .spawn()
        .unwrap();
    let mut child = Ended(child);
    let address: SocketAddr = wait_for(Duration::from_secs(20), "the worker's address", || {
        std::fs::read_to_string(dir.join("address")).ok()
    })
    .parse()
    .unwrap();
    let endpoint = Endpoint {
        address,
        secret: "k".to_owned(),
    };
    worker::ping(&endpoint).unwrap();
    assert!(accept_lines(&log).is_empty());

    // more connections than the worker has descriptors: each one it accepts
    // holds a descriptor while its hello is awaited, the rest wait unaccepted
    let idle: Vec<TcpStream> = (0..DESCRIPTORS * 2)
        .map(|_| TcpStream::connect(address).unwrap())
        .collect();
    let failed = wait_for(
        Duration::from_secs(8),
        "an accept failure in the log",
        || Some(accept_lines(&log)).filter(|lines| !lines.is_empty()),
    );
    assert!(failed[0].contains("EMFILE"), "{failed:?}");
    assert!(failed[0].contains("still serving"), "{failed:?}");
    assert!(child.0.try_wait().unwrap().is_none(), "the worker stopped");

    // a request made now waits unaccepted; it is answered once the idle
    // connections close and their descriptors are free
    let waiting = std::thread::spawn({
        let endpoint = endpoint.clone();
        move || worker::ping(&endpoint)
    });
    std::thread::sleep(Duration::from_millis(300));
    assert!(!waiting.is_finished(), "answered with no descriptor free");
    drop(idle);
    let answer = waiting.join().unwrap().unwrap();
    assert_eq!(answer.get_text("kind"), Some("worker"));
    assert_eq!(answer.get_int("pid"), Some(i64::from(child.0.id())));
    worker::ping(&endpoint).unwrap();
    assert!(child.0.try_wait().unwrap().is_none(), "the worker stopped");
    // the failures of these few seconds are one line of their kind
    let lines = accept_lines(&log);
    let emfile = lines.iter().filter(|line| line.contains("EMFILE")).count();
    assert_eq!(emfile, 1, "{lines:?}");
    assert!(
        lines.iter().all(|line| line.contains("still serving")),
        "{lines:?}"
    );

    drop(child);
    std::fs::remove_dir_all(&dir).unwrap();
}
