//! #352: Exec create and inspect share one kept-alive Engine connection; Start
//! keeps its own Upgrade connection; a kept connection the Engine closed or
//! refused to keep is never used, and nothing is resent.
#![cfg(unix)]
use layerfs_sandbox::{
    backend::docker::{Docker, ExecRequest},
    CommandIdentity, ContainerId, ExecId,
};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn container() -> ContainerId {
    ContainerId::parse(&"b".repeat(64)).unwrap()
}
fn exec() -> ExecId {
    ExecId::parse(&"a".repeat(64)).unwrap()
}
fn request() -> ExecRequest {
    ExecRequest {
        identity: CommandIdentity { uid: 501, gid: 20 },
        arguments: vec!["true".into()],
        environment: vec![],
        directory: "/tmp".into(),
        stdin: false,
    }
}
fn response(status: u16, body: &str, close: bool) -> Vec<u8> {
    let connection = if close { "Connection: close\r\n" } else { "" };
    format!(
        "HTTP/1.1 {status} test\r\n{connection}Content-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}
fn created(close: bool) -> Vec<u8> {
    response(201, &format!("{{\"Id\":\"{}\"}}", exec()), close)
}
fn inspected() -> Vec<u8> {
    let body = format!(
        "{{\"ID\":\"{}\",\"ContainerID\":\"{}\",\"Running\":false,\"ExitCode\":0,\"Pid\":0}}\n",
        exec(),
        container()
    );
    response(200, &body, false)
}
fn upgraded() -> Vec<u8> {
    b"HTTP/1.1 101 UPGRADED\r\nContent-Type: application/vnd.docker.multiplexed-stream\r\nConnection: Upgrade\r\nUpgrade: tcp\r\n\r\n".to_vec()
}

/// One scripted connection: its responses in order, and the signal that closes it.
type Connection = (Vec<Vec<u8>>, Option<mpsc::Receiver<()>>);

/// A scripted Engine: each entry is one accepted connection and the responses
/// it serves in order, one per request read; after its last response the
/// connection closes once `close` receives (or at once when `None`).
struct Engine {
    directory: PathBuf,
    worker: Option<JoinHandle<Vec<Vec<String>>>>,
}
impl Engine {
    /// The client's first connection asks the version, so the script follows
    /// an Engine that answers it; `finish` leaves that connection out.
    fn new(mut script: Vec<Connection>) -> (Docker, Self) {
        script.insert(
            0,
            (vec![response(200, r#"{"ApiVersion":"1.54"}"#, true)], None),
        );
        let directory = std::env::temp_dir().join(format!(
            "layerfs-keepalive-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("sock");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let worker = thread::spawn(move || {
            // each accepted connection is served on its own thread, in
            // accept order, so a kept connection waiting for its next request
            // does not hold back a new one
            let mut servers = Vec::new();
            for (responses, close) in script {
                let deadline = Instant::now() + Duration::from_secs(3);
                let stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "bounded admission");
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                servers.push(thread::spawn(move || serve(stream, responses, close)));
            }
            let connections = servers
                .into_iter()
                .map(|server| server.join().expect("a connection's server"))
                .collect();
            // no further connection may arrive
            thread::sleep(Duration::from_millis(50));
            assert!(listener.accept().is_err(), "an unexpected connection");
            connections
        });
        (
            Docker::new(path, Duration::from_secs(2)).unwrap(),
            Self {
                directory,
                worker: Some(worker),
            },
        )
    }
    /// The request heads each connection received.
    fn finish(mut self) -> Vec<Vec<String>> {
        let mut connections = self.worker.take().unwrap().join().unwrap();
        connections.remove(0);
        connections
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
fn serve(
    stream: UnixStream,
    responses: Vec<Vec<u8>>,
    close: Option<mpsc::Receiver<()>>,
) -> Vec<String> {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);
    let mut heads = Vec::new();
    for bytes in responses {
        let mut head = String::new();
        let mut length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(!line.is_empty(), "the client closed early");
            if let Some(value) = line.strip_prefix("Content-Length: ") {
                length = value.trim().parse().unwrap();
            }
            head.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        heads.push(head);
        writer.write_all(&bytes).unwrap();
    }
    if let Some(close) = close {
        close.recv_timeout(Duration::from_secs(3)).unwrap();
    }
    heads
}

#[test]
fn create_and_inspect_share_one_kept_connection() {
    let (docker, engine) = Engine::new(vec![(
        vec![created(false), inspected(), created(false), inspected()],
        None,
    )]);
    for _ in 0..2 {
        let created = docker.create_exec(container(), request()).unwrap();
        assert_eq!(created.identity(), (container(), exec()));
        assert_eq!(
            docker.inspect_exec(container(), exec()).unwrap().exit_code,
            Some(0)
        );
    }
    drop(docker);
    let connections = engine.finish();
    assert_eq!(
        connections.len(),
        1,
        "one connection for all four exchanges"
    );
    assert_eq!(connections[0].len(), 4);
    assert!(connections[0].iter().all(|h| !h.contains("Connection:")));
}

#[test]
fn start_keeps_its_own_upgrade_connection() {
    let (docker, engine) = Engine::new(vec![
        (vec![created(false), inspected()], None),
        (vec![upgraded()], None),
    ]);
    let created = docker.create_exec(container(), request()).unwrap();
    let attached = created.start().unwrap();
    drop(attached);
    assert!(docker.inspect_exec(container(), exec()).is_ok());
    drop(docker);
    let connections = engine.finish();
    assert_eq!(connections.len(), 2);
    assert!(connections[0][0].starts_with("POST /v1.54/containers/"));
    assert!(connections[0][1].starts_with("GET /v1.54/exec/"));
    assert!(connections[1][0].starts_with("POST /v1.54/exec/"));
    assert!(connections[1][0].contains("Connection: Upgrade\r\n"));
}

#[test]
fn a_kept_connection_the_engine_closed_is_not_used() {
    let (closed, close) = mpsc::channel();
    let (docker, engine) = Engine::new(vec![
        (vec![created(false)], Some(close)),
        (vec![inspected()], None),
    ]);
    docker.create_exec(container(), request()).unwrap();
    closed.send(()).unwrap();
    // the Engine closes the idle connection; give the hangup a bounded moment
    thread::sleep(Duration::from_millis(50));
    assert!(docker.inspect_exec(container(), exec()).is_ok());
    drop(docker);
    let connections = engine.finish();
    assert_eq!(connections.iter().map(Vec::len).collect::<Vec<_>>(), [1, 1]);
}

#[test]
fn a_response_that_says_close_is_not_kept() {
    let (docker, engine) = Engine::new(vec![
        (vec![created(true)], None),
        (vec![created(false)], None),
    ]);
    docker.create_exec(container(), request()).unwrap();
    docker.create_exec(container(), request()).unwrap();
    drop(docker);
    let connections = engine.finish();
    assert_eq!(connections.iter().map(Vec::len).collect::<Vec<_>>(), [1, 1]);
}
