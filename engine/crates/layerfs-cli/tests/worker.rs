//! `layerfs worker` (#533 step 15): it serves behind its secret file and
//! answers a ping; without its settings it refuses to start.
use layerfs_api::worker::{ping, Endpoint};
use std::io::Read;
use std::net::TcpListener;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const LAYERFS: &str = env!("CARGO_BIN_EXE_layerfs");

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-cli-worker-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .unwrap();
    dir
}

#[test]
fn the_worker_answers_its_secret() {
    let dir = tempdir();
    let secret = dir.join("secret");
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&secret)
            .unwrap();
        file.write_all(b"s3cret\n").unwrap();
    }
    // a free loopback port
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let address = format!("127.0.0.1:{port}");
    let mut worker = Command::new(LAYERFS)
        .arg("worker")
        .env("LAYERFS_WORKER_LISTEN", &address)
        .env("LAYERFS_WORKER_SECRET", &secret)
        .env("LAYERFS_WORKER_ROOT", dir.join("root"))
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let endpoint = Endpoint {
        address: address.parse().unwrap(),
        secret: "s3cret".to_owned(),
    };
    // the readiness wait: until it listens, at most 10 s
    let deadline = Instant::now() + Duration::from_secs(10);
    let answer = loop {
        match ping(&endpoint) {
            Ok(answer) => break answer,
            Err(refusal) if Instant::now() < deadline && refusal.code == "Unavailable" => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(refusal) => {
                let _ = worker.kill();
                panic!("ping: {refusal:?}");
            }
        }
    };
    assert_eq!(answer.get_text("kind"), Some("worker"));
    let wrong = ping(&Endpoint {
        address: endpoint.address,
        secret: "other".to_owned(),
    });
    worker.kill().unwrap();
    let _ = worker.wait();
    assert!(wrong.is_err(), "a wrong secret is refused");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn without_its_settings_the_worker_does_not_start() {
    let mut run = Command::new(LAYERFS)
        .arg("worker")
        .env_remove("LAYERFS_WORKER_LISTEN")
        .env_remove("LAYERFS_WORKER_SECRET")
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let status = run.wait().unwrap();
    let mut said = String::new();
    run.stderr
        .take()
        .unwrap()
        .read_to_string(&mut said)
        .unwrap();
    assert_eq!(status.code(), Some(125));
    assert!(said.contains("LAYERFS_WORKER_LISTEN is not set"), "{said}");
}
